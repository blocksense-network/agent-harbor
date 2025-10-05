# Project Specification: Cross-Platform Userspace File System with Snapshots and Branching

## Overview and Objectives

This project will develop a **cross-platform user-space file system** with advanced snapshot capabilities. The file system (FS) will be backed by memory (for performance) and can spill to a temporary disk storage under memory pressure. A core library written in Rust will implement the file system logic, while thin platform-specific glue layers will integrate it with **Linux FUSE**, **Windows WinFsp**, and **macOS FSKit**. Key goals include:

- **Full Filesystem Functionality:** The FS must support all common operations and semantics expected by Linux, macOS, and Windows, ensuring applications cannot distinguish it from a native filesystem. This includes correct behavior for file creation, deletion, renaming, reading/writing, directory management, metadata, permissions, locks, etc.[\[1\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=,close%20files%2Fstreams)[\[2\]](https://winfsp.dev/doc/WinFsp-Design/#:~:text=,security%20and%20access%20control).

- **Snapshot & Branching Support:** The FS will offer an explicit “take snapshot” operation. Snapshots represent point-in-time versions of the entire filesystem state. Snapshots are **writable** (clonable), allowing branching of the filesystem state – i.e. creating independent, diverging versions (branches) from any snapshot[\[3\]](https://lwn.net/Articles/331808/#:~:text=Netapp%20does%20writable%20snapshots%20of,I%20believe%20you%20get%20256). This enables features similar to version control or advanced backup systems, where multiple timelines of changes can co-exist.

- **Memory-Backed with Disk Spillover:** Initially, the FS will store data in-memory for fast I/O. Under high memory pressure or when data exceeds a threshold, it should transparently spill or migrate data to a temporary disk storage (such as files in a temp directory). This hybrid approach maximizes performance while preventing out-of-memory conditions.

- **High Concurrency & Multi-Process Access:** The FS will support concurrent access from multiple threads and processes, just as a kernel filesystem would. It must handle parallel operations, synchronization, and locking correctly under heavy load (multiple processes reading/writing the same files, etc.), maintaining data consistency and integrity.

- **Cross-Platform Compliance:** The behavior must comply with platform-specific filesystem standards and expectations:

- **POSIX semantics** on Linux/macOS for calls like open, read, write, unlink (including allowing deleting open files), permission bits, etc.

- **Windows NTFS semantics** on Windows for operations like file sharing modes, delete-on-close behavior, case insensitivity (by default), NT security, etc.[\[2\]](https://winfsp.dev/doc/WinFsp-Design/#:~:text=,security%20and%20access%20control).

- **macOS FS expectations** (via FSKit) such as Finder integration, extended attributes, and any Mac-specific conventions (e.g. resource forks as extended attributes).

- **Robustness and Testing:** The core logic will be extensively unit tested in Rust (without requiring a mounted FS) for correctness, including edge cases and concurrency scenarios. Stress tests (including heavy parallel I/O, rapid file creation/deletion, etc.) and platform-specific FS test suites will be used to validate behavior (e.g. Windows HLK file system tests, Linux FUSE tests). The system must be stable and not leak resources or crash under high load or when faced with invalid operations.

## Architecture Overview

**Core Library (Rust):** Implements all filesystem functionality in a platform-agnostic way. Exposes a C-compatible API (FFI) so it can be invoked from C/C++ and Swift. The core manages the in-memory data structures, handles I/O and snapshot logic, enforces consistency, and implements all required operations (file reads/writes, directory ops, metadata updates, locking, etc.). It will be oblivious to how it’s mounted – the glue layers will translate OS-specific requests into core library calls.

**Glue Layers:** Thin adapters on each platform that connect OS/file-system-driver APIs to the Rust core:

- **Linux:** A FUSE module (possibly implemented in Rust using libfuse or a Rust FUSE crate) will forward FUSE callbacks (e.g. read, mkdir, etc.) to the core library.

- **Windows:** A WinFsp-based DLL or service (in C/C++ or Rust) will interface with the WinFsp API, calling core library functions for file operations. WinFsp handles the kernel interaction, presenting our user FS to Windows.

- **macOS:** A File Provider using **FSKit** (likely a system extension written in Swift) will bridge FSKit’s delegate methods to the core library. The Swift code will use the Rust library via FFI, and implement the FSKit interfaces required by macOS 15+.

Each glue layer is responsible for translating between platform-specific concepts and the core’s abstractions (for example, mapping Windows CreateFile parameters to an open-file call in core, or mapping a FSKit operation to core calls). The glue will also handle any required **policy or semantic differences** (e.g., case sensitivity flags, Windows vs POSIX permission checking, etc.), as described later.

### Snapshot/Branching Model

The core library will implement a **Copy-on-Write (CoW) snapshot mechanism**. When a snapshot is taken, it represents the entire filesystem state at that moment. Key aspects of the snapshot feature:

- Snapshots have unique identifiers (e.g. incremental numeric IDs or GUIDs) and can be given human-readable names for convenience.

- Creating a snapshot is an atomic operation that freezes a point-in-time state. It should be efficient (O(1) or O(log n) metadata operation), not involving full data copying. The core will use copy-on-write for file data and metadata:

- After a snapshot, existing data blocks and metadata are shared between the snapshot and the “live” state. Any new writes or modifications in the live filesystem (or in a particular branch) will allocate new blocks or metadata copies, so the snapshot’s view remains unchanged.

- Snapshots are **writable**, meaning you can **branch** from any snapshot. Creating a writable snapshot either:

- **Option A:** Clones the snapshot as a new _branch_ and sets that as the active filesystem state (potentially unmounting or switching out the previous branch), or

- **Option B:** Makes the snapshot accessible as a separate branch concurrently (for example, mounting the snapshot’s branch in a separate mountpoint).

- **Branching:** Branches allow divergent modifications. For example, one could take snapshot S1 at time T1, continue modifying the main branch, and later create a new branch from S1 (snapshot S1 becomes a starting point for branch B2). Branch B2 will include all data from snapshot S1 and then can be modified independently of the main branch (which by time of branching has different changes). This is analogous to version control branches or writable clones in advanced file systems (NetApp and Btrfs support nested writable snapshots/clones)[\[3\]](https://lwn.net/Articles/331808/#:~:text=Netapp%20does%20writable%20snapshots%20of,I%20believe%20you%20get%20256).

- The system should support multiple snapshots and branches (e.g., NetApp allows up to 256, and ZFS/Btrfs essentially allow unlimited[\[3\]](https://lwn.net/Articles/331808/#:~:text=Netapp%20does%20writable%20snapshots%20of,I%20believe%20you%20get%20256)). We should define a reasonable limit if needed to avoid performance degradation.

- **Accessing Snapshots:** The core library will provide APIs to list existing snapshots and to switch the active branch or mount a specific snapshot branch. While mounted, the FS presents one branch’s view at a time (the active branch). To access an old snapshot, one could either instruct the running FS to switch to that snapshot (effectively a rollback or fast-forward to a branch) or spin up a new instance of the FS on that snapshot ID (parallel mount). The exact mechanism (in-place branch switching vs. separate mount) can be decided during design; both should be possible with the core’s support.

- **Deleting Snapshots:** Requirements for removing a snapshot (to free space) should be defined. Removing a snapshot would mean cleaning up data blocks/metadata only that snapshot exclusively holds (others are reference-counted). The core must track reference counts of shared data due to CoW.

- **Consistency:** Taking a snapshot must freeze a consistent state of the FS. The operation should quiesce ongoing modifications (e.g., use a global lock or transaction mechanism to ensure no partial modifications). From the moment of snapshot, that snapshot’s data is immutable (unless branched into a new writable branch). Active operations that complete after the snapshot creation belong to the post-snapshot state.

- **Branch Isolation:** Changes in one branch (including the main line) must not affect other branches/snapshots. The core will need to isolate file handles and open states per branch. For instance, an open file handle in the current branch that was opened before a snapshot is taken may continue writing to the current branch’s version of the file; the snapshot retains the old version. Implementation may involve versioned identifiers for files/inodes per branch.

This snapshot and branching capability will be a **flagship feature** of our FS, enabling scenarios like instant checkpoints, branching experimentation on file sets, and quick rollbacks. We will draw inspiration from filesystems like **ZFS/Btrfs** which have similar features (ZFS uses logical sequence number-based snapshots; Btrfs uses copy-on-write and allows nested writable snapshots at some performance cost[\[3\]](https://lwn.net/Articles/331808/#:~:text=Netapp%20does%20writable%20snapshots%20of,I%20believe%20you%20get%20256)). Our design will incorporate best practices from these systems adapted to a user-space library context.

### Memory and Storage Management

The FS core will manage data storage using a hybrid memory/disk approach:

- **In-Memory Data Structures:** Directory structures, metadata (inodes, file attributes), and small file contents will reside in memory by default for speed. We will implement efficient in-memory indexing (e.g., hash maps or trees for directory entries, b-tree or similar for quick lookup if needed for large directories) to ensure scalability.

- **Memory Pressure Monitoring:** The core should monitor its memory usage and/or system memory pressure. When usage exceeds a configurable threshold or the OS signals pressure (if available, e.g. Linux cgroups memory events or macOS memory pressure notifications), the FS will start moving some data to disk:

- Large file contents can be flushed to a temporary file on the underlying real filesystem (e.g., in /tmp or an OS-provided temp directory). The core would swap the in-memory data for a reference to the on-disk temp file. On subsequent reads, it streams from disk. Writes to that file can either be directly on the temp file or staged in memory then flushed.

- Least-recently-used (LRU) or least important data should be offloaded first (for example, files that have not been accessed recently). The core might keep a cache metadata structure to track usage frequency or recency.

- The threshold for in-memory vs disk could be a configuration (e.g., “keep at most X MB in RAM”). A possible policy: Keep all metadata and directory structure in memory (they’re typically small), and offload file _contents_ selectively if needed (since file data can be large).

- **Lazy Loading:** If data has been offloaded to disk and is later accessed, the FS should seamlessly load it back (or read directly from the temp file). This should be transparent to the user process. Caching strategies (read-ahead, keeping partially loaded files in memory for a while, etc.) can be used to optimize performance.

- **Temp Storage Management:** The temporary disk store (a designated folder) may hold multiple files representing spilled contents or swapped-out snapshots. The core must manage cleanup: when files are deleted or no longer needed (including if a snapshot holding them is deleted), the corresponding temp files should be removed. Possibly use unique identifiers for temp files to avoid conflicts.

- **Persistence Expectations:** This FS is primarily ephemeral (especially in-memory). Data in the temp storage is not meant for long-term persistence beyond the life of the FS instance – it’s a cache or swap space. However, for the duration of a mounted FS, the data on disk must be treated with the same care as in-memory data to avoid corruption. On clean unmount, temp files can be deleted. On crash, they are just temporary data (the FS doesn’t guarantee persistence across crashes unless we later extend it for that).

- **Statfs and Capacity Reporting:** The FS should report a total and available size via statfs (for Linux/macOS) or equivalent in Windows (e.g., GetVolumeInformation and free space). We can define an artificial large size if mostly memory (since memory-backed FS can be considered as having size \= configured memory limit or a fraction of real RAM). Alternatively, if using disk, report underlying disk available space. It’s important that copying large files to the FS either respects actual limits or at least doesn’t report infinite space. We will likely use the backing temp directory’s free space as the limiting factor (and possibly also cap by a max memory usage configuration).

## Core Filesystem Features and Operations

The core Rust library will implement a comprehensive set of filesystem operations, ensuring that any typical filesystem action the OS expects is handled. Below we enumerate the required operations and features:

### File and Directory Operations

- **Create/Open File:** Ability to create new files and open existing files. Support various open flags and modes:

- Creation with exclusive mode (fail if exists), truncate to zero length, open for append, read-only, write-only, read-write, etc.

- Windows specifics: must handle CreateFile parameters like desired access, share mode, creation disposition, and flags (e.g., open existing, create new, open or create, truncate existing) as part of open logic. The core can expose a single open(path, flags, mode, options) that the Windows glue translates into the right flags. Must also handle **FILE_SHARE\_\* modes and access rights**: track open handles and deny opens that conflict with share modes (e.g., if one process opened file exclusively, another open should fail) – WinFsp will rely on our implementation for this logic[\[4\]](https://winfsp.dev/doc/WinFsp-Design/#:~:text=).

- _Result:_ Returns a file handle (internally, perhaps an ID or pointer to an in-memory file structure) to be used for subsequent reads/writes.

- **Read File:** Read data from a file at a given offset. Must handle reads beyond EOF correctly (return 0 bytes). Should integrate with OS caching: on Linux/macOS, the kernel may cache file pages, but also might forward reads to us if cache is disabled or on cache miss. On Windows, memory-mapped and cached IO go through our code via cache manager integration (WinFsp manages some caching, but we must supply the data)[\[5\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=,through%2C%20overlapped%2C%20etc.%20modes). We should ensure efficient reading, e.g., support reading into provided buffer, possibly implement read_buf optimization for FUSE (splicing data without extra copy).

- **Write File:** Write data to a file at given offset. Handle expanding the file size if writing beyond current EOF. If writing sparse (skipping offsets), we may either allocate zeros or mark holes if implementing sparse files. Writes may be cached by the OS or go through directly. We must ensure data is stored (in memory or spilled to disk). For concurrency, if multiple writes occur, proper locking or atomicity on overlapping regions might be needed (POSIX allows concurrent writes but doesn’t guarantee ordering beyond what the program does, however our FS should not intermix bytes incorrectly if writes overlap – typically file systems lock per inode or serialize writes that overlap).

- On Windows, support for **overlapped (asynchronous) I/O**: WinFsp can deliver parallel read/write requests with offset and length, we must handle possibly out-of-order completions (the core can be oblivious and just handle each as a separate call, since the glue/WinFsp will manage OS async mechanics).

- **Close File (Release):** Clean up an open handle. If file was deleted (unlinked or delete-on-close) while open, this is the point to finally remove it. The core should track if a file is marked for deletion and remove the data and metadata when the last handle closes (to satisfy Windows semantics of DeleteFile and POSIX semantics of unlink on open file).

- **Delete File (Unlink):** Remove a file name from a directory. The core must ensure correct semantics:

- POSIX: unlink is allowed even if the file is open by processes. The directory entry is removed immediately; any subsequent opens will fail (ENOENT). However, existing open file handles can still read/write the file until they close, at which point the core frees the file data (no name references and no open handles \-\> file is truly gone).

- Windows: deletion is typically not allowed if the file is open (unless opened with FILE*SHARE_DELETE and requested via NtSetInformationFile to delete on close). WinFsp provides a flag in the IRP_MJ_CLEANUP indicating a file is to be deleted on close. Our core can unify this by implementing a \_deferred deletion* mechanism: mark the file as deleted (and invisible in directory listings) but keep it alive until handles close[\[4\]](https://winfsp.dev/doc/WinFsp-Design/#:~:text=). This essentially matches the POSIX approach under the hood. The glue on Windows will ensure that a direct DeleteFile call results in the file being hidden and marked for deletion, and on last handle closure the core deletion occurs.

- **Create Directory (mkdir):** Create a directory with given name and permissions. Ensure entries like “.” and “..” if needed (though typically FUSE and OS handle “.” and “..” logically, but we may store them or not). Return error if directory already exists or if parent path doesn’t exist.

- **Remove Directory (rmdir):** Remove a directory. Should only succeed if directory is empty (no child files/dirs). The core must enforce this (check that only “.” and “..” are present, or no entries at all depending on how we store).

- **Rename:** Rename or move a file or directory from one path to another. This must be **atomic** (appear instantaneous to observers). Handle various cases:

- Overwrite target if it exists (depending on flags). POSIX rename replaces the target if it’s a file, and fails if target is a non-empty directory. Windows MoveFileEx can be told to fail if target exists or replace it. We should support a “no-replace” flag for scenarios where the OS wants an error if target exists (WinFsp passes a flag for FILE_RENAME_INFORMATION’s ReplaceIfExists \= false).

- If target is a directory, likely the OS will only attempt rename if it’s empty (or on Windows, might allow merging? But typically not).

- Ensure that if source and target are on different directories, we update both accordingly under proper locks.

- If the operation is cross-directory, update parent pointers. If it’s a rename of a directory, all its children’s paths logically change (but since our fs likely uses an internal tree, we just move the subtree node).

- **Concurrency:** If other operations are happening (e.g., listing directory while a rename happens), proper locking needed to avoid transient inconsistency. Usually, file systems use fine-grained locks (like lock two directory inodes in a global ordering to swap entries safely).

- On Windows, must also handle the case of open handles: Windows will not allow renaming an open file (if it doesn’t have share delete permission), similar to deletion. Our core can allow it if not conflicting, but better to mimic OS: we might disallow renaming an open file on Windows or handle it as needed (since POSIX allows renaming an open file).

- **Hard Links:** Support creating a hard link to a file (multiple directory entries (names) pointing to the same file content/inode). Core needs to track link count. Ensure that deleting one name doesn’t remove the file if other links exist. On Windows, CreateHardLink is an API and WinFsp will call our link handler or use the NTFS semantics. On Linux/macOS, handle link() system call. (Note: linking directories is not allowed in typical filesystems except for “.” and “..” or special cases, so we will not allow creating hard link to directory).

- **Symbolic Links:** Support symlinks (soft links). The core can store a special file type that contains a target path (as a string). Operations:

- Reading the link (readlink) should return the target path.

- Opening a symlink normally should either follow it or not depending on flags; typically OS handles following by default (except when O_NOFOLLOW is used). For simplicity, we might let the OS do the path resolution, but in user FS we might need to implement symlink resolution in the path lookup if the OS passes it to us raw (FUSE can choose to do its own resolution or let kernel, usually kernel does unless configured otherwise).

- Creating a symlink: the glue on each platform will call core with the symlink target string and link name. On Windows, symlinks are represented as NTFS reparse points; WinFsp should allow us to just specify a symlink with a target. We must handle differences: Windows symlinks have type (file or dir) and possibly need special privilege or flag. The core can just treat it as a special file containing target; Windows glue might mark it as a reparse point with the proper tag.

- Listing directories should show symlinks as such (with an appropriate file type flag).

- No need to support NTFS junctions specifically because those can be just seen as symlinks to directories; our FS doesn’t need to integrate with actual NTFS reparse unless we want to allow those semantics. But likely out of scope beyond symlinks.

- **Device/Special Files:** (Optional) Support for special nodes like UNIX named pipes (FIFOs), character or block device nodes, etc. Since this is a user-space FS primarily for general use, we can choose to implement creation of such nodes (so that tools like mknod don’t error). However, actual device files wouldn’t function (no kernel driver behind them), but we can at least store the type and return appropriate errors if opened. It might be acceptable to **not** support device files (return error on mknod) if not needed in our use cases. We will document whether creation of special files is supported. (On Windows, no concept of device nodes in FS; on macOS, since we are a user FS, likely skip device files).

- **Readdir (Directory Listing):** Ability to list directory contents. Given a directory path, return list of entries (names) with their types and metadata. This is called frequently (e.g., ls or File Explorer). Must handle large directories by supporting an offset/marker to continue listing if required (FUSE provides an off_t offset and a callback to fill entries; WinFsp may provide a pattern for continuing after a certain entry). Our core can simply return all entries in an array or iterator, but glue might need to handle pagination. We must ensure that adding or removing entries during iteration doesn’t crash; typical approach is to lock the directory during listing or snapshot the list of names at start of operation.

- **Stat (Getattr):** Return file or directory attributes (metadata). This includes:

- File type (file, dir, symlink, etc.), size, number of hard links, timestamps (creation, last modification, last access, possibly birth time if supported), permissions, owner UID/GID (on Unix), and possibly device ID for special files.

- For Windows, attributes like hidden, read-only, archive flags, etc., and the security information (ACL or a simplified view). WinFsp will call into methods to get file size, file times, file attributes, and security descriptor. Our core should maintain sufficient metadata to respond. Likely we maintain at least: file length, a bitfield for file attributes (or derive from permissions), and a security/permission structure.

- Must handle statfs or volume stat: return filesystem-level info (block size, total blocks, free blocks, etc. as discussed).

- **Setattr (Setting Metadata):** Support changing file metadata:

- **Truncate** (ftruncate or setting file size): Adjust file length either cutting or extending. If extending, allocate zero-filled space (or mark holes). If truncating, and file had data beyond new size, discard that (free memory or release temp storage space).

- **Permission bits (chmod):** On Linux/macOS, set Unix permission bits. The core should store mode (read/write/exec bits for user/group/other). If a file is open, changing perms should still be reflected next time someone checks. If the platform (Linux) does an access() call or expects the mode to be enforced, we should enforce it (or rely on default_permissions from FUSE which uses kernel check based on mode[\[6\]](https://libfuse.github.io/doxygen/structfuse__operations.html#:~:text=In%20general%2C%20all%20methods%20are,kernel%27s%20permission%20check%20has%20succeeded)). We likely implement basic permission checking in core for consistency (see **Permissions & Security** below).

- **Ownership (chown):** Record new UID/GID for file. On Windows, this might correspond to changing the Security Descriptor owner SID. It may not be heavily used; we can allow or ignore depending on context, but for completeness the core should store owner (perhaps default to the user who mounted or a fixed value if not meaningful on Windows).

- **Timestamps (utimens):** Set file access and modification times (and creation time if provided). Our FS will maintain timestamps for last modification, last access, and creation (birth) time. Windows has all three; Unix traditionally has mtime, atime, ctime (change time). We can map creation time \<-\> birth time, ctime perhaps as last status change (we update ctime whenever any metadata changes).

- **Extended Attributes (xattr):** Support xattr get/set/list/remove:

- On Linux, extended attributes are name-value pairs associated with files (namespaces like user._, security._, etc.). MacOS also uses xattrs for things like Finder info, where e.g. com.apple.\* attributes might be set. Our core can maintain a dictionary of xattrs per file.

- Ensure that listing xattrs and retrieving specific ones works. If not implementing security.\* fully, we might at least store whatever is set (e.g. OS may set com.apple.quarantine xattr on downloaded files – our FS should store it).

- Windows doesn’t have POSIX xattrs, but NTFS has alternate data streams and extended metadata. We might treat NTFS Alternate Data Streams (ADS) as a bit similar to xattrs or as special files. In WinFsp tests, they enumerate _streams_[\[7\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=,and%20streams). We should support file _streams_ (i.e., named forks of a file) on Windows, since WinFsp and NTFS API allow “file:streamname”. Perhaps map these to xattr or a separate core concept:
  - We can treat an ADS as a separate data blob attached to the file (with its own size). The core could store each stream by name (other than the default unnamed data). The glue on Windows will trigger create/open on a stream (WinFsp passes stream name). On Linux/macOS, xattrs could simulate small streams, but typically not used the same way. However, since the core is general, we can incorporate named streams support: any file can have named substreams with full file semantics (read/write). This satisfies Windows ADS requirements[\[7\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=,and%20streams). If not needed outside Windows, we can internally implement ADS as xattrs (for small data) or as full file blobs.

- **Memory-Mapped I/O Support:** This is mostly about ensuring the above operations integrate well with memory mapping. On Windows, memory-mapped files (through MapViewOfFile) will involve the cache manager. WinFsp indicates it supports memory-mapped files[\[2\]](https://winfsp.dev/doc/WinFsp-Design/#:~:text=,security%20and%20access%20control). To support this, our core must properly implement reading and writing at arbitrary offsets and allow caching. Specifically:

- If the OS requests us to flush or write back pages (via an fsync or specific calls), handle that.

- The data consistency for memory mapping means if one process writes via WriteFile and another sees via mapping, underlying OS should handle coherence via its cache if our FS provides correct cache hints. We likely rely on OS page cache for coherence between buffered I/O and memory map (as long as we go through the same FS driver).

- We need to implement **flush** and **fsync** operations: when OS calls flush (which may happen multiple times for one open)[\[8\]](https://www.cs.hmc.edu/~geoff/classes/hmc.cs135.201001/homework/fuse/fuse_doc.html#:~:text=CS135%20FUSE%20Documentation%20Important%3A%20there,path) or fsync, our core should persist any buffered data to the backing store (which for us might mean ensuring the data is in memory structures – which it already is – or if on disk, ensure it’s physically written). Essentially, flush/fsync are hints to ensure data integrity (e.g., if our FS was writing lazily to disk).

- If memory mapping is heavily used, the OS might call our read/write functions for page-ins or use an alternate path (FUSE has an option to use writepage callback for cache write-back, but we can possibly stick to simple approach).

- We will test with tools like FSX (File System exerciser) which are known to find issues with read/write vs mmap consistency[\[9\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=,mapped%20I%2FO).

- **File Locks:** Implement file locking to coordinate concurrent access:

- **POSIX advisory locks (fcntl F_SETLK / F_GETLK)**: FUSE exposes a lock handler for POSIX record locks (byte-range locks) and a flock handler for BSD locks[\[10\]](https://libfuse.github.io/doxygen/structfuse__operations.html#:~:text=int%28,int%20cmd%2C%20struct%20flock)[\[11\]](https://libfuse.github.io/doxygen/structfuse__operations.html#:~:text=match%20at%20L247%20int%28,int%20op). Our core should maintain a lock table per file to track byte-range locks (start, length, type, owner pid). The Linux/macOS glue will call core on lock/unlock requests. We must ensure correct behavior: prevent conflicting locks (e.g., two exclusive locks overlapping or exclusive vs shared conflict) and release locks on process exit (the OS will notify via unlock calls or if the file is closed).

- **Windows locks:** Windows file locking (LockFile/UnlockFileEx) are _mandatory_ range locks. WinFsp will forward these as lock requests, likely via the same mechanism or separate call. Additionally, Windows has the concept of **file sharing modes** (which are set at open and remain for the handle). Share modes (deny read, deny write, etc.) are effectively another layer of locking for opens. The core must enforce share modes at open time using the list of existing open handles for that file. (This was partly covered under Create/Open above).

- We will implement locking such that a lock acquired by one process (via any API) prevents other processes from violating the lock constraints, ensuring cross-process coordination. For advisory locks, because our FS runs in one process, it can coordinate easily.

- Locks should obey semantics on each platform (e.g., Windows mandatory locks mean if a region is locked exclusively, other reads/writes should fail – the OS might enforce some of this, but since calls come to us, we have to choose to allow or deny as appropriate).

- **File Change Notifications:** On Windows, WinFsp supports file change notifications (for FindFirstChangeNotification API)[\[2\]](https://winfsp.dev/doc/WinFsp-Design/#:~:text=,security%20and%20access%20control). On macOS and Linux, inotify or FSEvents could be used on a real FS. For a user FS, the OS might not automatically handle it, so our FS may need to implement a mechanism to notify the system when files change (especially on Windows).

- WinFsp likely has an API for us to signal changes (so that it can trigger the notifications). We should include in the core an ability to track subscribers or simply always notify on modifications. The core library could provide callbacks to the glue when certain events happen (file created, removed, modified, etc.), and the glue will call the platform’s notification mechanism. For Windows, this might be done by appropriately responding to IRP_MJ_DIRECTORY_CONTROL (for change notifications).

- On macOS FSKit, and on Linux (if we consider inotify), similar integration may be needed, but possibly the kernel will poll our FS for changes or require us to push events. This is an advanced feature; we aim to support it at least on Windows (since WinFsp explicitly lists it).

- **Case Sensitivity and Encoding:**

- Linux and macOS (in default mode) treat filenames as case-sensitive by default (except macOS typically formats are case-insensitive but FSKit might allow either). Windows is case-insensitive (but preserves case). Our FS core should allow a mode to operate in case-insensitive fashion (for Windows compatibility). This means when in insensitive mode, lookups and other operations should ignore case differences (likely by normalizing names to e.g. all lower-case for storage or keeping a case-insensitive map).

- We must decide how to store and search directory entries: perhaps use a trie or hash that either is case-sensitive or not depending on a flag. In insensitive mode, must also prevent names that only differ by case from coexisting (to mimic Windows behavior).

- Character encoding: Windows expects Unicode (UTF-16) filenames, Linux/mac expects UTF-8 bytes. The core can store Unicode (Rust OsString). The glue will convert as needed (e.g., Windows wide char to UTF-8 for core, Linux just passes through assuming UTF-8). We must also forbid characters not allowed on each platform: e.g., Windows forbids \<\>:"|?\* and names like "CON". The glue can validate and reject such names on Windows mount (or core can have a validation function when running in Windows-compatibility mode).

- Namespace differences: Windows paths can have drive letters and volume GUIDs, but that is abstracted by WinFsp. We just see relative paths from root of our volume. We should handle special cases like requests for volume information paths or certain reserved paths (e.g. WinFsp might ask for "\\\\" or similar). Likely not an issue, WinFsp abstracts it.

### Permissions and Security Model

Handling permissions is crucial for cross-platform compliance:

- **POSIX Permissions (Linux/macOS):** Each file/dir has a mode (rwx bits for owner/group/others and special bits like setuid, etc.) and an owner UID and group GID. Our core will store these. Operations like open, read, write, etc., should enforce permission checks if the OS hasn’t already:

- FUSE can let the kernel do permission checks if mounted with default_permissions[\[6\]](https://libfuse.github.io/doxygen/structfuse__operations.html#:~:text=In%20general%2C%20all%20methods%20are,kernel%27s%20permission%20check%20has%20succeeded), given we provide correct mode bits in getattr. We likely will use default_permissions to leverage kernel checking on Linux/macOS (makes it easier to stay in sync with system policy like root override, etc.). However, we still need to implement access() for certain calls if not using default_permissions (and FSKit maybe expects our code to enforce? Unclear).

- For safety, our core can double-check permissions on operations as well, especially for internal operations or if kernel bypasses (e.g., chmod or chown obviously require correct privileges – typically only owner or root can change).

- Group and other bits matter on multi-user systems; since our FS is user-space, if it’s mounted by a user, by default all accesses come from that user context (unless OS allows others to access it). On macOS, a file provider might be accessible by all users? Possibly restricted. We should not assume single-user; implement correct checks.

- **Windows Security:** Windows uses Access Control Lists (ACLs) and user impersonation for file access:

- WinFsp supports NTFS-level security[\[2\]](https://winfsp.dev/doc/WinFsp-Design/#:~:text=,security%20and%20access%20control), meaning it can convey Security Descriptors. Our core should either maintain a Security Descriptor for each file, or map it from simpler permissions.

- Minimal approach: map the POSIX mode bits to a Windows security descriptor with an owner SID and a DACL that approximates the rwx permissions (e.g., grant access to “Everyone” or to a fixed “Users” group based on those bits). This would allow basic access checks. Alternatively, if more accuracy is needed, allow storing a full ACL per file (but that complicates cross-platform significantly).

- Since our FS is not primarily meant for persistent storage, a full ACL model might be overkill. We can choose a strategy such as: each file has an owner SID (the user who created it, or the account running the FS if single-user) and possibly a simplified DACL that either allows all access to the owner and maybe read-only to others if we interpret the “others” bits. We ensure that trying to read a file without permission fails with Access Denied on Windows, etc.

- The glue on Windows can assist by taking a SecurityDescriptor from the core (perhaps core produces one via some helper if given owner and mode).

- Also important: **Privilege differences**: WinFsp testing notes things like backup/restore privileges[\[12\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=,SE_CHANGE_NOTIFY_NAME). Typically, if a process opens with backup intent (SE_BACKUP_NAME privilege), it can bypass normal ACL checks. That is mostly handled by OS/WinFsp; our FS might be asked to open file regardless of perms due to that. We should allow it if indicated (probably WinFsp does “force allow” in those cases).

- **Permissions on Mac (FSKit):** Likely similar to POSIX. FSKit probably expects a UID/GID and mode for each file, and the kernel will enforce it. Mac also has ACLs but simpler to stick to Unix permissions which are usually enough.

- **Default Permissions:** We will define defaults for new files and dirs (e.g., respect the mode passed on creation in FUSE/FSKit and the security attributes in Windows CreateFile security descriptor if provided). Possibly implement umask logic for Unix.

- **SetSecurity (Windows):** If a Windows app tries to set a custom ACL (SetFileSecurity), the core should either handle it (if we implement full ACLs) or return success but ignore (or some minimal support). Since WinFsp can present a security descriptor, maybe it calls our setsecurity callback. We can decide to implement storing DACL if needed for completeness. Given “all features” target, it would be good to at least store any DACL set by user (even if not interpreting it fully for checks, though that defeats the purpose if not enforced). This is an advanced area; in the specification we note that the core should support storing and retrieving ACLs on Windows for full NTFS compatibility.

### Other Features and Considerations

- **Volume Info:** The FS should report a volume name/label and filesystem type. For example, Windows expects a volume label and serial number (which can be static or random). Mac and Linux might not require name (except as mount name), but FSKit might allow setting one. We will have a way to set the volume label at mount time (passed to core).

- **Symbolic Link Emulation on Windows:** On Windows, if a symlink is created, ensure that listing it shows appropriate attributes (FILE_ATTRIBUTE_REPARSE_POINT and symlink type). WinFsp likely handles marking a file as reparse point if we specify it. The core just needs to tag the node as symlink and store target.

- **Alternate Data Streams (ADS):** As noted, support named streams on Windows. Our core can treat them similarly to how NTFS does: each file node can have multiple data streams (default unnamed one plus any named). They can be created via a special API (on Windows, file name like "filename:streamname"). The WinFsp glue will detect the colon and call a core function to open a specific stream. We must enumerate streams in FindStreams calls (if any; WinFsp tests enumerating streams[\[7\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=,and%20streams)). The core should list stream names and sizes. In our design, streams could either reuse the same underlying structure as files (with their own length and data store) under a parent file object. We’ll implement this to pass NTFS compatibility tests, though this feature may not be heavily used by end-users.

- **Reparse Points:** Aside from symlinks, NTFS reparse points include mount point junctions, etc. We likely do not need to implement arbitrary reparse point types. We will implement symlinks as mentioned. Possibly we can treat any unknown reparse as not supported or EOPNOTSUPP. (WinFsp tests reparse points and symlinks[\[7\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=,and%20streams), probably mainly expecting symlinks to work.)

- **Junction Mounts:** Our FS might be mounted as a network share or at a directory via NTFS junction as per WinFsp scenarios[\[13\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=,file%20system%20%28FILE_DEVICE_DISK_FILE_SYSTEM). We need to ensure path handling works in those cases (which should be transparent since path given by WinFsp will just start at the mounted root).

- **Large File Support:** Use 64-bit offsets and sizes everywhere. Support files larger than 4GB (especially if temp storage on disk can accommodate, and assuming memory isn’t limiting). We should test copying large files. Ensure that our internal data structures (e.g., storing file length in u64) and logic (like read/write offset calculations) can handle up to the theoretical limits (maybe 2^63-1 bytes as a max file size, which is exabyte scale, far beyond practical memory/disk in our context).

- **Atomicity and Consistency:**

- Many file system operations need to appear atomic to external observers. E.g., rename is atomic, as mentioned. Writing to a file at an offset – the write of given buffer is atomic with respect to readers (they either see old or new data for that region after completion, not a partial mix if overlapping with another writer).

- We should carefully design lock granularity: likely lock per file for writes (so two writes on same file are serialized or at least protected per overlapping region). Lock directory structure for operations that modify it (create/delete/rename). Possibly a global lock for certain snapshot operations or cross-directory renames (to lock two directories).

- We want high concurrency, so avoid one big lock for everything. Instead, use fine-grained locking but prevent deadlocks by a consistent ordering (e.g., always lock parent directory before child, or lock two directory inodes in a fixed global order for rename).

- Ensure that after crashes (if the FS process crashes), WinFsp or FSKit can recover gracefully[\[14\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=Fault%20Tolerance%20Testing). WinFsp will clean up resources if our process dies[\[15\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=For%20this%20purpose%20WinFsp%20is,and%20without%20crashing%20the%20OS), but our core should aim not to corrupt state. If the FS restarts, snapshots and data in memory would be lost since this is not persistent (unless we consider persisting some state to disk; not required initially).

- **Performance Considerations:**

- Use efficient algorithms for directory and file data management to handle potentially large numbers of files (millions) and deep directory trees.

- The Rust core should make use of concurrency internally (e.g., multiple threads can be in the core handling different requests). We will avoid global locks that prevent parallel I/O to different files. Possibly use a multi-reader multi-writer lock strategy: e.g., a global lock only for snapshot operations (since snapshot freezes global state), but normal file ops primarily need per-file or per-dir locks.

- Memory overhead should be monitored; use appropriate data structures (for instance, an implicit B-tree for file data might help if implementing sparse files or lots of extents, but could start with simpler vector of bytes for file content and optimize later).

- Provide options to tune performance vs consistency, like enabling/disabling OS caches. For instance, in FUSE we might enable writeback caching if our FS can handle it (which means the kernel may cache writes and flush later – we must then implement fsync properly).

- Ensure that operations complete quickly to not block the OS. E.g., directory listings should not take locks that block file creation in that dir for an excessively long time. Consider using lock-free data structures or copy-on-write for directory contents for snapshots – interestingly, since we have snapshots, maybe use an immutable data structure approach which naturally handles concurrency (like each modification makes a new version, and readers can continue on old version).

- **Testing Hooks:** The core library will be designed for testability. For example, we might allow a “debug mode” where certain faults can be injected (as WinFsp does with DEBUGTEST for testing retry paths[\[16\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=Windows%20File%20System%20Drivers%20,wait%20a%20bit%20and%20retry)[\[17\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=%2F,Result%29%29)). We should simulate out-of-memory or forced delays to test concurrency correctness. All core operations will have unit tests (e.g., create a few files, rename them concurrently, verify final state).

- **Compliance Test Suites:** We will run platform-specific tests:

- On Windows, use WinFsp’s tests (winfsp-tests, winfstest, IfsTest) to ensure NTFS-like behavior[\[18\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=WinFsp%20allows%20the%20creation%20of,properly%20on%20WinFsp%20file%20systems). Our FS should pass these to be considered compliant (except any features we explicitly choose not to support will be documented).

- On Linux, run tests like libfuse test suite and possibly POSIX File System Test Suite (e.g., pjdfstests) to verify compliance with POSIX semantics.

- On macOS, ensure it works well with Finder, and possibly any Apple verification if available (there might be an FSKit test or using the same FSX and other tools).

- We will also use the **FSX tool** (on all platforms, or the Windows port on Windows) to stress test random operations (FSX is known to find race conditions in file read/write and mmap)[\[9\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=,mapped%20I%2FO).

- Additionally, test branching: e.g., create files, take snapshot, modify files, switch branch, verify snapshot branch still has old content, etc.

## Platform-Specific Integration Details

This section outlines how the core library will interface with each platform’s filesystem interface, and specific requirements or adaptations for each.

### Linux Integration (FUSE)

We will create a FUSE file system daemon (or use a Rust FUSE library) that mounts the FS. Key points:

- **LibFUSE interface:** Implement the fuse_operations callbacks by calling into the Rust core. For example, getattr calls core’s stat function, readdir calls core’s directory listing, etc. All FUSE operations listed (see libfuse docs) will be covered:

- Essential ops: getattr, readdir, open, read, write, release, mkdir, rmdir, unlink, rename, link, symlink, readlink, truncate, statfs, setxattr, getxattr, listxattr, removexattr, flush, fsync, etc.[\[19\]](https://libfuse.github.io/doxygen/structfuse__operations.html#:~:text=int%28,fuse_file_info)[\[20\]](https://libfuse.github.io/doxygen/structfuse__operations.html#:~:text=int%28). We will implement as many as apply.

- Some ops are optional or special-purpose: access (we may rely on kernel default perm checks), create (which is open+truncate in one call; we will implement it for efficiency), lock and flock (we will implement to support file locks), ioctl and poll (not strictly necessary unless we need custom ioctls; likely skip unless needed for snapshot command maybe), fallocate (to allocate space or punch hole – we can implement to optimize large file allocation), copy_file_range (we may implement as optimization for copying data without user round-trip, or simply allow fallback)[\[21\]](https://libfuse.github.io/doxygen/structfuse__operations.html#:~:text=int%28,off_t%2C%20struct%20%202)[\[22\]](https://libfuse.github.io/doxygen/structfuse__operations.html#:~:text=int%28,off_t%2C%20off_t%2C%20struct%20fuse_file_info).

- We will support utimens (to set timestamps).

- We’ll set the appropriate feature flags with FUSE (for example, indicate support for atomic rename with RENAME_EXCHANGE or NOREPLACE if we implement it, indicate support for copy_file_range if done, etc.).

- **Threading:** libfuse can run multithreaded. We will use the multi-threaded mode so multiple requests are processed in parallel. The Rust core must be thread-safe (use mutexes/RwLocks around shared structures where needed).

- **Mount Options:** Use default_permissions if we want kernel to do perm checking (then implement getattr returning proper mode bits)[\[6\]](https://libfuse.github.io/doxygen/structfuse__operations.html#:~:text=In%20general%2C%20all%20methods%20are,kernel%27s%20permission%20check%20has%20succeeded). Possibly enable allow_other (so multiple users can access if needed). Also, possibly enable write-back caching in FUSE if our FS can handle it (we have to ensure consistency on fsync).

- **Case Sensitivity:** On Linux, case sensitivity is expected. We will mount as case-sensitive by default. (If we ever mount this FS on Windows via some layer, we’d use case-insensitive mode, but that’s covered under WinFsp).

- **Linux Specific behaviors:**

- Device nodes: If we choose to allow creating device files (with mknod), we will handle mknod FUSE operation[\[23\]](https://libfuse.github.io/doxygen/structfuse__operations.html#:~:text=match%20at%20L19%20int%28,mode_t%2C%20dev_t). These won’t function as real devices but should appear in listings with correct mode. Alternatively, we can return an error for mknod if we decide not to support devices, and similarly for FIFOs.

- statfs: Fill with meaningful values (e.g., block size 4096, total blocks \= some large number or backing fs’s space, free blocks accordingly).

- Extended attributes: Ensure we handle security.capability xattr gracefully (so tools like cp that copy xattrs don’t fail), even if just storing them.

- Symlinks: FUSE will call our readlink to get the target[\[24\]](https://libfuse.github.io/doxygen/structfuse__operations.html#:~:text=int%28,fi). We must ensure to not resolve it ourselves but just return the stored path.

- **Snapshot Control:** We need a way to trigger snapshot creation from user context. Options:

- A special ioctl on the FUSE file descriptor. We could define an ioctl that the user (root or the process controlling FS) can call to instruct "snapshot now" or "switch branch". The FUSE ioctl callback would call core-\>snapshot_create.

- Alternatively, a special file in the FS (like a control file /.\_snapshot) where writing a name could create a snapshot, etc. But that approach can interfere with normal FS namespace.

- We lean towards an **out-of-band API** (not strictly part of FS ops) for snapshot operations, since this is an advanced control feature. For instance, the application using this FS can directly call the Rust library API to snapshot. But if the FS is running in its own process, an ioctl via FUSE could be the mechanism.

- We will design an ioctl command (e.g., FSCTL or custom number) to create snapshot with a provided name, and one to list snapshots, maybe one to switch branch. This will be documented and implemented in glue.

### Windows Integration (WinFsp)

On Windows, we will create a user-mode file system using the WinFsp framework. This might be a small C/C++ harness or Rust code using WinFsp’s C API via FFI. Key integration points:

- **File System Registration:** We will use FspFileSystemCreate (from WinFsp API) to create a file system instance and provide a dispatch table of callbacks (similar to FUSE ops, WinFsp calls them “operations”). Alternatively, use WinFsp’s FUSE compatibility layer by linking with the POSIX layer – but we likely prefer native to fully leverage Windows features.

- **WinFsp Callbacks:** According to WinFsp documentation and Windows IFS semantics, we need to implement at least:

- Create/Open callback (Create): handles opening/creating files and directories. It receives parameters like file path, create options (directory/file, overwrite, etc.), desired access, share mode, security descriptor for new file, and so on. Our implementation will call core-\>open or core-\>create with equivalent logic. We must honor share modes and requested dispositions (if “create new” and file exists, error; if “open existing” and not found, error; if “open always”, create if not exist, etc.). Also handle directory opens.

- Cleanup/Close (Cleanup and Close): WinFsp separates Cleanup (called when a handle is about to close or on process termination) and Close (when the handle count truly drops to 0). The Delete-on-close logic is handled between these. We will use Cleanup to mark deletion if needed, and Close to free core resources. Our core’s release will likely be called on Close.

- Read, Write: similar to FUSE, but WinFsp might pass different structures (IRP with buffers). We will copy data to/from core as needed. Must handle paging IO (flag indicating if this is paging read/write for memory map – but WinFsp usually handles cache).

- Flush buffers: handle flush (called on handle flush or periodic flush).

- GetFileInfo/SetFileInfo: Windows uses these to get or change file attributes (size, times, attributes, etc.) and also to handle certain operations like rename or delete:
  - For example, setting file size (FileEndOfFileInformation), setting file allocation size, setting rename information (FileRenameInformation), setting disposition (FileDispositionInformation for delete-on-close). WinFsp will translate these calls to something our FS can process (possibly direct calls or via SetFileInfo code with identifiers).

  - We will implement logic to handle a SetFileInfo for rename by calling core-\>rename, for disposition (delete) by marking the file deleted (don’t remove if still open), for file size by core-\>truncate, for file basic info (times/attributes) by core-\>setattr, etc.

- Directory enumeration: WinFsp will call FindFiles or similar to list a directory’s contents. We provide the list from core-\>readdir. It might provide a buffer we fill with FileInformation structures in Windows format. We need to convert core’s file metadata to the Windows view (e.g., file size, file attributes flags, timestamps).

- QueryInformation: For things like FileStandardInformation (file size, link count, etc.), FileAllInformation, etc. Many of these can be answered from core metadata.

- Security operations: QuerySecurity (to get file ACL) and SetSecurity (to set file ACL). As discussed, our core will need to produce a security descriptor. We might implement a translation from core’s UID/GID/mode to a simple security descriptor for Windows:
  - For example, map owner UID to a user SID (if we know the mapping of UID-\>SID; if running in a context, might use the mounting user’s SID as owner).

  - Construct DACL: if mode says owner can read/write, give that SID full control; if others have some access, maybe give “Everyone” or “Users” group those accesses. For now, we can grant broad access to avoid permission issues, unless the user sets something specific.

  - For SetSecurity, accept a security descriptor and store it (perhaps as an opaque blob or parsed to some internal ACL structure). This is complex; we may consider not supporting full ACL modifications in the first iteration, but being able to present a consistent security descriptor is needed for many apps and for test compliance.

- Lock control: Windows byte-range locks via IRP_MJ_LOCK_CONTROL. WinFsp will call our filesystem to lock/unlock ranges. We integrate this with core’s lock manager (common with POSIX locks, since they can be treated similarly). Ensure to enforce exclusive vs shared locks properly. Also implement unlocking on file close or process close if not explicitly unlocked.

- Volume info: Implement queries like getting volume information (volume name, serial, FS name, max filename length, etc). WinFsp might let us set these in the volume creation. We will choose a FS name (e.g., "MemFS" or custom) and a random serial number each mount. Max filename length typically 255 for most FS.

- **Execution context:** The WinFsp FS will run as a normal user-mode process (maybe as a service or launched by user). It will register the volume (either as a drive letter, mount point, or UNC share). We must decide how it will be used: possibly mount as a drive like "Z:" or as a directory symlink. The glue code will handle mounting via FspFileSystemStartDispatcher or similar call to begin processing requests.

- **Case insensitivity:** We will run the Windows instance in case-insensitive mode. WinFsp allows marking the file system case sensitive or not[\[25\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=,using%20junctions). By default, to mimic NTFS, we will use case-insensitive (so FspFileSystemSetCaseSensitive(false)). Our core will then do case-insensitive lookups. If core is internally case-sensitive by default, we might need to have a mode flag. Alternatively, store all names in one normalized form (like uppercase) for Windows branch of the FS.

- **Alternate Data Streams:** When Windows requests an open on "file:stream", WinFsp provides the stream name. We will handle this in Create: if stream name not empty and file exists (or is being created), open or create the named stream in core. Streams should be listed when an application calls FindFirstStreamW – if we want to pass WinFsp’s test, implement listing streams[\[7\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=,and%20streams). (Perhaps WinFsp’s test just tries opening a named stream, writing, reading, and deleting it. We’ll support that).

- **Reparse/symlink:** We mark symlinks appropriately. WinFsp might expect us to provide a reparse tag and substitute path for symlinks. Possibly easier: when our core says a file is a symlink, WinFsp can manage it if we provide the target in a certain way (I recall WinFsp’s MEMFS sample might show how to do symlinks). We’ll implement according to WinFsp docs: typically, one sets the file attribute to reparse point and uses the reparse buffer. Alternatively, we could intercept when someone opens a symlink and perform redirection in user mode – but better let OS do it. We’ll research WinFsp symlink support and do likewise (spec note: ensure symlinks created via CreateSymbolicLinkW appear as such and that reading them returns the target).

- **Mounting as Network or Disk:** WinFsp can present the FS as disk (removable) or network drive. For our purposes, disk-like is fine (unless we want the network semantics like certain caching differences or UNC path usage). We’ll likely use Disk mode (so it appears as a normal drive).

- **Integration with Branching:** On Windows, how to trigger a snapshot? Possibly via an **FSCTL/DeviceIoControl** on the volume handle. Since WinFsp gives us a volume handle we could implement custom control codes. We can define an IOCTL like FSCTL_TAKE_SNAPSHOT that our FS recognizes. The user-space controlling program can call DeviceIoControl on the volume (WinFsp provides the handle for the volume when mounted). We will handle that in our dispatch (WinFsp likely delivers unrecognized FSCTLs to us). This is analogous to how for example some file systems allow volume snapshot via FSCTLs.

- **Stability:** Must be rock-solid as a Windows FS. We aim to pass IfsTest and appear as close to NTFS semantics as possible[\[18\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=WinFsp%20allows%20the%20creation%20of,properly%20on%20WinFsp%20file%20systems). This includes handling weird Windows behaviors: e.g., certain file flags (FILE_ATTRIBUTE_TEMPORARY might hint that we could keep data in memory, FILE_FLAG_WRITE_THROUGH vs caching – WinFsp might handle caching hints, but we should consider flush accordingly). Also, allow operations like renaming open files with appropriate sharing (NTFS prohibits by default without special flags).

- **Testing:** Use the provided MEMFS example in WinFsp as a baseline, which “implements all file system features that WinFsp supports”[\[26\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=WinFsp%20includes%20a%20test%20user,only). We will ensure our FS implements at least what MEMFS does, plus our snapshot extras.

### macOS Integration (FSKit via Swift)

Apple’s FSKit (available from macOS 14/15) allows implementing a user-space file system as a system extension (likely in Swift or Objective-C). We will create a System Extension in Swift that uses our Rust core. Key points:

- **FSKit API:** Based on limited documentation, FSKit likely provides an object-oriented interface. Possibly one creates a subclass of FSFileProvider or registers an extension that handles file operations via delegate methods (similar to the older Kernel’s mount if it were user).

- We need to find what callbacks FSKit expects. Common ones (extrapolating) would be: open, read, write, create node, remove node, etc., similar to FUSE. We will implement those by calling into core.

- FSKit might integrate with the existing File Provider or might be separate. It’s an official API for user file systems, replacing macFUSE. We assume it covers at least POSIX operations and extended attributes.

- **Swift \<-\> Rust:** We will use Swift’s C FFI to call into the Rust library (which we can expose as a C header). Swift can call C functions easily. Alternatively, we create a small C shim around the Rust library for comfortable calling.

- **Threading:** The FSKit framework likely handles multithreading internally (e.g., multiple simultaneous calls to our callbacks). We must ensure any Swift code calling Rust is thread-safe (the Rust core handles locking).

- **macOS Semantics:** By default, macOS HFS+ and APFS are case-insensitive (unless explicitly formatted case-sensitive). FSKit might allow specifying. We likely should default to case-insensitive on macOS to match user expectations (most Mac apps assume case-insensitivity). However, we can allow an option for case sensitivity. We’ll coordinate with core (maybe treat macOS same as Windows mode for names).

- **Permissions:** Mac uses standard Unix permissions and also ACLs. We will stick to Unix permissions as stored in core. FSKit presumably will let the kernel do the permission enforcement (like a normal FS). We will ensure to provide correct UID/GID and mode in attributes. Possibly we need to handle Mac extended security like quarantine xattr or Gatekeeper attributes (just stored as xattr).

- **Extended Attributes:** Very important on macOS because things like Finder comments, file flags (like the “hidden” flag is often an extended attribute or part of stat (UF_HIDDEN)). APFS exposes “finderInfo” and other aspects. Since FSKit is new, not fully documented, we assume:

- We will implement getxattr, setxattr, etc., similar to Linux, to store any attribute like com.apple.FinderInfo, com.apple.ResourceFork (which Finder might set for custom icons or old-style resource forks), etc. Storing them ensures better integration.

- If FSKit has specific calls for resource fork (since ResourceFork is basically an alternate stream named "com.apple.ResourceFork"), we can map that to our core’s extended attribute or stream.

- **File Locks:** macOS supports POSIX locks and BSD flock (same as Linux basically). We will handle those in core as already planned.

- **Special files:** If FSKit needs to handle symlinks, devices, etc. We will implement symlinks (as core does). Device nodes likely not needed (APFS even doesn’t allow device files on some setups if not root).

- **Volume Presentation:** The FSKit extension will register the file system with the system. We can specify a volume name, and it should appear in Finder as an external volume (likely with an icon). We ensure the volume flags (read/write, case sens.) are set appropriately.

- **Integration of Snapshots:**

- APFS has native snapshots (for Time Machine), but our snapshots are separate. We will not integrate with APFS’s snapshot system (that’s different layer). Instead, we provide our own.

- To allow a Mac user to invoke snapshot, we could again use an ioctl-like mechanism. FSKit might have support for custom FS controls or we might expose snapshot operations via a command-line tool or an API call to our extension. Possibly we can have the Swift layer listen for a specific message (maybe using XPC between the extension and a controlling app).

- Alternatively, since FSKit runs in user space (though in a system extension context), we might just call a Rust function to snapshot when a certain condition or external trigger happens. For admin use, perhaps a command-line utility that talks to the extension (FSKit might provide a communication channel).

- We'll plan for an out-of-band mechanism, e.g., an IPC call to the extension (Apple's system extensions can communicate with a containing app via XPC) to command snapshot creation or branch switching.

- **Stability and Performance:** Must ensure the extension doesn’t hang (macOS will kill if unresponsive). Keep operations quick and possibly implement any required callbacks like init and stop.

- **Testing on Mac:** Use typical filesystem usage, create files via Finder, duplicate them, etc., to ensure it behaves normally. Ensure package installers or other tools that might create extended attributes or resource forks work. Possibly run small filesystem tests if any (though not sure Apple provides a suite). At least run our unit tests on macOS for core. Also consider using the FSX tool on macOS (since FSX was originally an Apple tool) to stress test[\[9\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=,mapped%20I%2FO).

## Comprehensive Requirements Summary

Below is a summary list of core requirements that the implementation team must satisfy, grouped by category:

- **General Functional Requirements:**

- **File/Directory Creation and Deletion:** Must support creating, opening, and deleting files and directories with correct semantics and error cases (existence checks, empty dir checks, etc.)[\[1\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=,close%20files%2Fstreams).

- **File I/O:** Support reading and writing files (sequential and random-access), including large files, with proper handling of offsets, EOF, and growth/truncation.

- **Concurrent Access:** Allow multiple processes/threads to access the FS concurrently. Ensure thread-safe internal operations so that concurrent reads/writes/creates do not corrupt data or metadata. Results should be as if operations happened in some serial order (meet consistency, but not necessarily serializing everything).

- **Atomic Operations:** Operations like rename and file deletion (especially with open file references) must be atomic and follow platform rules (e.g., atomic rename, deferred deletion)[\[4\]](https://winfsp.dev/doc/WinFsp-Design/#:~:text=).

- **Links:** Implement hard links (multiple names for one file content) and symbolic links (store and resolve link targets) for full POSIX compatibility.

- **Metadata Management:** Maintain file metadata (size, timestamps, mode/permissions, owner, etc.) and update them on relevant operations (e.g., update mtime on file write, ctime on metadata change, atime on read if enabled).

- **Extended Attributes and Streams:** Support storing arbitrary extended attributes on files (for Linux/Mac). Also support multiple named data streams on a file (for Windows alternate data streams). Ensure they can be created, read, written, and deleted. Directory listings and queries should reveal their existence (as appropriate: e.g., Windows needs a way to list streams[\[7\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=,and%20streams)).

- **Flush and Sync:** Provide flush (flush) and sync (fsync) operations so that the OS can force writing of data to backing store (especially relevant when using disk swap). Ensure that after fsync returns, data is safe in memory or disk such that even if FS process is killed, data is not “lost” (for memory FS, maybe this doesn’t apply unless we choose to persist to disk on fsync).

- **Memory-Mapped I/O Support:** Ensure compatibility with memory mapping by handling page-aligned reads/writes and possibly allowing direct memory access. Pass relevant tests for reading/writing via mmap (no data corruption or inconsistencies)[\[9\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=,mapped%20I%2FO).

- **Locking:** Implement file locking (both whole-file locks via flock and byte-range locks via fcntl on Unix, and LockFile on Windows). Must prevent conflicting locks between processes and release locks on process termination or file close[\[4\]](https://winfsp.dev/doc/WinFsp-Design/#:~:text=). The locking must be integrated into read/write operations (e.g., if a region is locked by another, writes should be blocked or fail according to the semantics).

- **Case Handling:** Provide mode for case-sensitive vs case-insensitive operation depending on platform. In insensitive mode, "ABC.txt" and "abc.txt" refer to the same file, and no two names differing only by case can exist.

- **Path Limits:** Support reasonably long paths (at least 255 bytes per component, and deep hierarchies – e.g., up to 32k length as NTFS supports).

- **Special File Flags:** If applicable, support file flags on Unix (immutable, append-only, etc.) – this is advanced and can be skipped initially unless needed.

- **Change Notifications:** On Windows, implement directory change notification events so that if an application subscribes to changes (FindFirstChangeNotification), they get signaled when files change[\[2\]](https://winfsp.dev/doc/WinFsp-Design/#:~:text=,security%20and%20access%20control). (Likewise, consider macOS/Linux inotify – though kernel might handle those if we mark ourselves properly, or perhaps user FS needs to emulate. This is a nice-to-have for macOS/Linux, required for Windows to pass some app expectations).

- **Volume Information:** Provide volume name, serial, filesystem name and capabilities (e.g., support flags: case sensitive/insensitive, support for ACLs, max file size, etc.). Ensure reported capabilities match what we implement (e.g., if we don’t implement something like “sparse files” explicitly, it’s okay; but if asked we should gracefully fail or handle minimally).

- **Snapshot and Branching Requirements:**

- **Snapshot Creation:** Ability to create a snapshot on-demand (via API/command). The snapshot represents a read-only view of the entire FS state at a point in time, automatically preserving all files and directories as they existed then.

- **Writable Clones:** Ability to create a new writable branch from any snapshot (including the latest state). This could be done by branching at the current state (which effectively is creating a snapshot of current and continuing on a new branch) or branching an older snapshot (creating a parallel timeline). Each branch evolves independently thereafter, sharing unmodified data with the original to save memory.

- **Branch Identification:** Each snapshot/branch must have an ID or name. The system should track parent-child relationships (to know which snapshots were branched from which).

- **Switch Active Branch:** Provide a way to switch the mounted filesystem’s view to a different branch (or equivalently, mount a different branch). This should happen ideally without a full unmount/mount cycle – but if not possible, at least allow unmounting one branch and mounting another quickly using the saved snapshot.

- **Isolation:** Changes in one branch do not affect others. For example, if file “A” is modified in branch B1, branch B2 (from an earlier snapshot) still sees the old content of “A” unless merged (note: merging is not in scope).

- **Snapshot Listing & Management:** Provide an interface to list all existing snapshots/branches, with metadata (creation time, possibly differences or tags). Also provide a way to delete a snapshot/branch (which frees associated data if not used by others).

- **Performance:** Snapshot creation should be quick (constant time, no copying). Accessing data in an older snapshot should be as fast as accessing current data (aside from perhaps an extra indirection for copy-on-write).

- **Memory Efficiency:** Use copy-on-write to avoid duplicating data between snapshots. Only changes after a snapshot consume new memory/disk. Potentially use reference counting for shared data blocks or an immutable data structure approach.

- **Consistency and Crash Safety:** If a snapshot is taken, it must be internally consistent (no half-written files). Ensure any ongoing writes either landed fully before snapshot or will appear only after (this likely means quiescing file operations momentarily during snapshot). If the FS process crashes, snapshots either exist or not – no partial state corruption. A crash might lose recent snapshots that weren’t persisted, but should not corrupt existing branches.

- **Limits:** Define a practical limit on number of snapshots or branches to avoid pathological performance or memory usage (e.g., hundreds are fine, but thousands might degrade performance due to long ancestry chains). Document this for users.

- **Non-Functional Requirements (Quality Attributes):**

- **Reliability:** The FS should operate without crashes or deadlocks. Rigorous testing (unit, integration, and platform certification tests) will be done. The FS must recover gracefully from errors (e.g., allocation failures, partial I/O failures) and report errors correctly to the OS (e.g., ENOSPC if temp disk fills, or other appropriate errno/NTSTATUS codes for different conditions).

- **Performance:** Aim for low overhead so that using the FS is comparable to native disk for typical operations, within the limits of user-space overhead. E.g., throughput for large sequential read/writes should be high (possibly memory-bound when in-memory). We will measure and optimize hot paths (maybe use profiling and optimize locks, use batching in readdir, etc.). Latency for metadata ops (open, create) should be minimized (use caching internally of path lookups, etc.).

- **Scalability:** The FS should handle a large number of files (millions) and deep directory trees. Data structures should handle many entries without extreme slowdown (e.g., use of hash maps or trees with log(n) behavior rather than linear scans).

- **Memory Usage:** Avoid memory leaks. Implement mechanisms to free memory when files are deleted or snapshots dropped. The memory pressure handler must ensure we don’t cause system OOM – instead we gracefully swap to disk. We should also avoid excessive fragmentation; perhaps reuse temp files or pre-allocate large ones for spilled data if possible.

- **Security:** The FS should not allow unauthorized access. For example, on multi-user systems, if allow_other is used in FUSE or if the FS is shared, respect file permissions so one user can’t read another’s files if not permitted. On Windows, ensure the ACLs reflect who can open files. The FS code itself should validate inputs (avoid buffer overflows, check path lengths, etc., especially since it handles raw requests from the OS).

- **Compliance:** As reiterated, pass relevant OS tests. On Windows, behavior should be virtually indistinguishable from NTFS for normal apps[\[18\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=WinFsp%20allows%20the%20creation%20of,properly%20on%20WinFsp%20file%20systems). For instance, if an app expects delete-on-close behavior, we must do that. If an installer expects certain file attribute propagation or locking, we handle it.

- **Maintainability & Testability:** The core logic in Rust will be modular and thoroughly unit-tested. Use of Rust ensures memory safety and reduces bugs. We will write unit tests for every operation (create, rename, etc.) including tricky scenarios (e.g., rename file while open, delete directory with files, concurrent writes, etc.). Also include stress tests that spawn threads doing random operations, verifying consistency at the end.

- **Documentation:** The implementation team will produce documentation for the code (especially the core library API so that those writing glue code know how to call it). Also, usage docs for how to mount and operate the FS, how to trigger snapshots, any limitations or special behaviors on each OS.

## Guidance for Implementation

To ensure success, we outline some guidance and best practices:

- **Leverage Existing Examples:** Examine WinFsp’s MEMFS sample and libfuse examples for reference implementations of full filesystems[\[26\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=WinFsp%20includes%20a%20test%20user,only). These illustrate the breadth of operations required and edge cases (e.g., MEMFS shows handling of Windows-specific flags).

- **Design Data Structures for Snapshots Early:** The snapshot feature is complex; design the metadata structures with versioning in mind from the start. One approach: represent the filesystem tree as an **immutable tree** (each node contains references to children, and any update clones the path to root). For example, updating a file’s content could create a new version of that file node, and new versions of ancestor directories up to root. Each snapshot points to a root tree node. This way, snapshots naturally share unchanged portions, and branches are just pointers to different root nodes. Libraries or patterns from persistent data structures can be used.

- **Memory vs Disk Storage Implementation:** Possibly abstract the storage of file content behind an interface, so it’s easy to swap between an in-memory buffer and a file on disk. For instance, have a FileStorage trait with methods read/write, and implement it for InMemoryStorage and TempFileStorage. Then each file object in core can contain a FileStorage (which might switch from InMemory to TempFile on the fly when needed). This will simplify memory pressure handling.

- **Concurrency Control:** Use Rust’s concurrency primitives wisely – likely RwLock for global structures (to allow multiple concurrent reads) and mutexes or locks on individual file nodes for operations that modify those. We might also use lock-free structures for certain things if needed (but only if profiling indicates locks are bottleneck). Also consider that snapshot creation might need a global quiescent point – perhaps a global write lock that temporarily blocks new ops while snapshot metadata is taken (should be quick).

- **Unit Testing of Core:** Since the core is independent of OS, we can write tests that simulate sequences of operations. E.g., create some files, fork a snapshot, verify both snapshot and current have expected data, etc. Also test error cases (like fill up memory and see if it spills to disk properly, try operations that should fail like removing non-empty dir, etc.).

- **Use Platform Test Suites During Development:** Continuously test on each platform as features are implemented, not leaving it all to the end. This will catch platform-specific nuance early. For instance, if IfsTest reveals an issue with how we handle file sharing modes, we can adjust accordingly.

- **Performance Testing:** Use tools like FSBench on Windows[\[27\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=This%20test%20is%20developed%20together,It%20is%20written%20in%20C%2FC) and simple benchmarks on Linux (e.g., fio) to gauge performance. Identify slow paths (maybe deep copy on write might slow heavy write workloads; perhaps mitigate by chunking large files).

- **Graceful Degradation:** If some features prove too complex to fully implement initially (e.g., full Windows ACLs), implement a basic or stub version that at least doesn’t break things. Document these as future enhancements. For example, we might initially use a simplified security model where all files are world-accessible on Windows, just to pass general usage, and improve it later. But ensure any such simplification is clearly indicated to avoid security holes (maybe restrict usage to local single-user in that case).

- **Platform Specific Notes:** Pay attention to:

- Windows: differences between being mounted as Network vs Disk (timing, caching differences). Maybe stick to Disk mode. Also consider the Windows “Backup/Restore” access (which allows reading all files regardless of ACL for backup programs)[\[12\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=,SE_CHANGE_NOTIFY_NAME) – ensure that works (perhaps WinFsp handles it, but test it).

- macOS: FSKit is new – expect limited documentation. We might need to examine Apple’s sample (like the open-source MSDOS FS implementation using FSKit)[\[28\]](https://eclecticlight.co/2024/06/26/how-file-systems-can-change-in-sequoia-with-fskit/#:~:text=Apple%20has%20been%20using%20FSKit,1)[\[29\]](https://eclecticlight.co/2024/06/26/how-file-systems-can-change-in-sequoia-with-fskit/#:~:text=This%20passing%20remark%20leads%20to,explain%20them%20or%20FSKit%20itself). Possibly reverse-engineer from headers. Be prepared for adjustments once Apple’s docs are clearer.

- Linux: Make sure to handle signals or interrupts (FUSE can send an interrupt to an operation if the process that made request gave up; we should respect those if possible to cancel long ops).

Finally, by adhering to these specifications and thorough testing, we aim to deliver a robust, feature-rich userspace filesystem that behaves correctly under all typical OS operations and provides the powerful snapshot/branching capability for advanced use cases. The development team should use this document as a blueprint and checklist to ensure all aspects are covered. With careful implementation and testing, our FS will seamlessly integrate on each platform and meet the highest standards of functionality and stability[\[30\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=A%20file%20system%20is%20a,system%20is%20of%20paramount%20importance)[\[2\]](https://winfsp.dev/doc/WinFsp-Design/#:~:text=,security%20and%20access%20control).

---

[\[1\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=,close%20files%2Fstreams) [\[5\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=,through%2C%20overlapped%2C%20etc.%20modes) [\[7\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=,and%20streams) [\[9\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=,mapped%20I%2FO) [\[12\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=,SE_CHANGE_NOTIFY_NAME) [\[13\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=,file%20system%20%28FILE_DEVICE_DISK_FILE_SYSTEM) [\[14\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=Fault%20Tolerance%20Testing) [\[15\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=For%20this%20purpose%20WinFsp%20is,and%20without%20crashing%20the%20OS) [\[16\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=Windows%20File%20System%20Drivers%20,wait%20a%20bit%20and%20retry) [\[17\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=%2F,Result%29%29) [\[18\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=WinFsp%20allows%20the%20creation%20of,properly%20on%20WinFsp%20file%20systems) [\[25\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=,using%20junctions) [\[26\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=WinFsp%20includes%20a%20test%20user,only) [\[27\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=This%20test%20is%20developed%20together,It%20is%20written%20in%20C%2FC) [\[30\]](https://winfsp.dev/doc/WinFsp-Testing/#:~:text=A%20file%20system%20is%20a,system%20is%20of%20paramount%20importance) WinFsp Testing · WinFsp

[https://winfsp.dev/doc/WinFsp-Testing/](https://winfsp.dev/doc/WinFsp-Testing/)

[\[2\]](https://winfsp.dev/doc/WinFsp-Design/#:~:text=,security%20and%20access%20control) [\[4\]](https://winfsp.dev/doc/WinFsp-Design/#:~:text=) WinFsp Design · WinFsp

[https://winfsp.dev/doc/WinFsp-Design/](https://winfsp.dev/doc/WinFsp-Design/)

[\[3\]](https://lwn.net/Articles/331808/#:~:text=Netapp%20does%20writable%20snapshots%20of,I%20believe%20you%20get%20256) The two sides of reflink() \[LWN.net\]

[https://lwn.net/Articles/331808/](https://lwn.net/Articles/331808/)

[\[6\]](https://libfuse.github.io/doxygen/structfuse__operations.html#:~:text=In%20general%2C%20all%20methods%20are,kernel%27s%20permission%20check%20has%20succeeded) [\[10\]](https://libfuse.github.io/doxygen/structfuse__operations.html#:~:text=int%28,int%20cmd%2C%20struct%20flock) [\[11\]](https://libfuse.github.io/doxygen/structfuse__operations.html#:~:text=match%20at%20L247%20int%28,int%20op) [\[19\]](https://libfuse.github.io/doxygen/structfuse__operations.html#:~:text=int%28,fuse_file_info) [\[20\]](https://libfuse.github.io/doxygen/structfuse__operations.html#:~:text=int%28) [\[21\]](https://libfuse.github.io/doxygen/structfuse__operations.html#:~:text=int%28,off_t%2C%20struct%20%202) [\[22\]](https://libfuse.github.io/doxygen/structfuse__operations.html#:~:text=int%28,off_t%2C%20off_t%2C%20struct%20fuse_file_info) [\[23\]](https://libfuse.github.io/doxygen/structfuse__operations.html#:~:text=match%20at%20L19%20int%28,mode_t%2C%20dev_t) [\[24\]](https://libfuse.github.io/doxygen/structfuse__operations.html#:~:text=int%28,fi) libfuse: fuse_operations Struct Reference

[https://libfuse.github.io/doxygen/structfuse\_\_operations.html](https://libfuse.github.io/doxygen/structfuse__operations.html)

[\[8\]](https://www.cs.hmc.edu/~geoff/classes/hmc.cs135.201001/homework/fuse/fuse_doc.html#:~:text=CS135%20FUSE%20Documentation%20Important%3A%20there,path) CS135 FUSE Documentation

[https://www.cs.hmc.edu/\~geoff/classes/hmc.cs135.201001/homework/fuse/fuse_doc.html](https://www.cs.hmc.edu/~geoff/classes/hmc.cs135.201001/homework/fuse/fuse_doc.html)

[\[28\]](https://eclecticlight.co/2024/06/26/how-file-systems-can-change-in-sequoia-with-fskit/#:~:text=Apple%20has%20been%20using%20FSKit,1) [\[29\]](https://eclecticlight.co/2024/06/26/how-file-systems-can-change-in-sequoia-with-fskit/#:~:text=This%20passing%20remark%20leads%20to,explain%20them%20or%20FSKit%20itself) How file systems can change in Sequoia with FSKit – The Eclectic Light Company

[https://eclecticlight.co/2024/06/26/how-file-systems-can-change-in-sequoia-with-fskit/](https://eclecticlight.co/2024/06/26/how-file-systems-can-change-in-sequoia-with-fskit/)
