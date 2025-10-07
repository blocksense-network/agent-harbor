# macOS Filesystem Hooks for Interpose Mode

This document specifies the filesystem API hooks required for AgentFS Interpose/FD-forwarding mode on macOS. These hooks enable zero-overhead data I/O while maintaining overlay and branch semantics.

## Strategy

Interpose at the POSIX/libSystem boundary (`libsystem_c` and `libsystem_kernel` in libSystem.B.dylib). Hook both path-based and fd-based variants where metadata might diverge between overlay and backstore. Prefer to forward **heavy data I/O** to the real kernel FD (no hook on read/write/mmap), but **always** mediate *path* operations and *metadata-changing* calls.

**Symbol variants:** On macOS many functions have `$INODE64` / `$DARWIN_EXTSN` aliases. Interpose **both the base symbol and its 64-bit alias** where present (e.g., `stat` and `stat$INODE64`).

## A. Opens & Creation (**Must-hook**)

* `open`, `openat`, `creat`
* `fopen`, `freopen` (C stdio wrappers that some apps call directly)
* Temporary/unique creators: `mkstemp`, `mkstemps`, `mkdtemp`, `tmpfile`

## B. Directory Enumeration & Path Traversal (**Must-hook**)

* `opendir`, `fdopendir`, `readdir`, `readdir_r` (deprecated but still seen), `closedir`, `scandir`
* Symlink read: `readlink`, `readlinkat`

## C. Rename / Link / Delete / Create-dir (**Must-hook**)

* `rename`, `renameat`, **`renameatx_np`** (handles `RENAME_EXCL`, `RENAME_SWAP`)
* `link`, `linkat`, `symlink`, `symlinkat`
* `unlink`, `unlinkat`, `remove`
* `mkdir`, `mkdirat`, (optionally `mkfifo`, `mkfifoat` if you want FIFOs covered)

## D. Metadata & Times (**Must-hook** path + fd variants)

* Stat family: `stat`, `lstat`, `fstat`, `fstatat`
* Mode/owner/time: `chmod`, `fchmod`, `fchmodat`; `chown`, `lchown`, `fchown`, `fchownat`
* Times: `utimes`, `futimes`, `utimensat`, `futimens`
* Size/allocation: `truncate`, `ftruncate`
* File system info (often used by toolchains): `statfs`, `fstatfs`

> Why hook some **fd-based** calls (e.g., `fstat`, `fchmod`)?
> With FD-forwarding, the kernel handle's metadata may not match overlay policy (e.g., whiteouts, cloned mode/uid). We adjust results / route mutations through AgentFS so overlay stays authoritative.

## E. Extended Attributes & ACLs (**Must-hook** if you need Finder/IDE fidelity)

* xattrs: `getxattr`, `lgetxattr`, `fgetxattr`; `setxattr`, `lsetxattr`, `fsetxattr`; `listxattr`, `llistxattr`, `flistxattr`; `removexattr`, `lremovexattr`, `fremovexattr`
* ACLs: `acl_get_file`, `acl_set_file`, `acl_get_fd`, `acl_set_fd`, `acl_delete_def_file`
* Finder/BSD flags & rich attrs: `getattrlist`, `setattrlist`, `getattrlistbulk`; BSD flags `chflags`, `lchflags`, `fchflags`

## F. Copy/Clone & Rich Copies (**Recommended**)

* `copyfile`, `fcopyfile` (NSFileManager, Finder, many installers)
* `clonefile`, `fclonefileat` (APFS block-clone aware copies)

> Hooking these lets you **promote copy to clone** inside the backstore and preserve overlay metadata atomically.

## G. Reporting / Path Exposure (**Must-hook** to avoid leaking backstore paths)

* `realpath` (map to overlay view)
* `fcntl` with **`F_GETPATH`** (return overlay path for forwarded FDs)

## H. File Locking & Allocation (**Recommended**)

* `fcntl` record locks (POSIX locks): no semantic hook needed if you forward the FD, but you may wish to observe for diagnostics.
* Preallocation: `fcntl(F_PREALLOCATE)`; `posix_fallocate` (if present) (route to backstore when supported)

## I. File Watching (to keep notifications coherent) (**Recommended**)

* **FSEvents:** `FSEventStreamCreate` / `FSEventStreamCreateRelativeToDevice`
  (Translate watched overlay paths → backstore paths so the process still receives events when its I/O bypasses the mount.)
* (Usually **not** needed) `kqueue`/`kevent`: watchers are fd-based; since `open(O_EVTONLY)` is forwarded, vnode events arrive naturally.

## Cross-cutting Guidance

### 1) Priority & Fallback

* **Absolute minimum for correctness:** A-D + G on both platforms.
* For **developer-tool fidelity** (IDEs, Finder integration, installers), add E + F + macOS rich metadata (E/F).
* Leave heavy I/O and mmap **unhooked**; that's the whole point of FD-forwarding.

### 2) Reentrancy & Bypass Guards

* Maintain an **allowlist/guard** so hooks **don't re-enter** on backstore paths (and lower-only reads, where you choose to bypass).
* Keep a fast per-thread "in shim" flag to avoid recursion on functions like `copyfile` that call back into opens/stats.

### 3) Namespace Translation

* Centralize **overlay⇄backstore path mapping** and **fd→overlay metadata shims** (for `fstat`, `F_GETPATH`, `GetFinalPathNameByHandleW`).
* When a path op implies copy-up (e.g., `chmod` on a lower-only file), the hook should request the AgentFS server to **materialize upper** first, then apply the mutation.

### 4) Locking & Sharing

* Don't hook POSIX `fcntl` locks— the kernel enforces them on the forwarded handle.

### 5) Events/Watchers

* If the app watches **by fd** (kqueue/dispatch-vnode), forwarding the open is enough.
* If it watches **by path** (FSEvents), **translate** to backstore paths at registration.

## Testing Matrix (Sanity)

1. **Zero-copy guarantee:** Large read/write/mmap on forwarded FDs → no adapter data callbacks observed.
2. **Overlay semantics preserved:** `rename`, `unlink`, `chmod`, `xattr` on lower-only files trigger copy-up + metadata seeding, whiteouts applied, directory listings reflect overlay.
3. **No path leaks:** `realpath` / `F_GETPATH` return overlay paths.
4. **Watchers:** FSEvents on overlay path receive events when the app writes via forwarded FDs.
5. **ACLs/xattrs/flags:** Round-trip set/get behaves as if working on the overlay.
6. **Share/locking semantics:** POSIX locks behave identically to native.
