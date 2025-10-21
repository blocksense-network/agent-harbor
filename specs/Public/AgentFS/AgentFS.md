# AgentFS — Cross-Platform Filesystem Snapshots and Per-Process Mounting

## Purpose

AgentFS implements the necessary filesystem snapshot and per-process mounting capabilities on macOS and Windows platforms, where native operating system support for these features is limited or absent. Linux provides these capabilities natively through mount namespaces and filesystem-level snapshots (ZFS/Btrfs/OverlayFS), but macOS and Windows require user-space implementations.

## Key Capabilities

### Filesystem Snapshots

- **Copy-on-Write (CoW) Snapshots**: Create point-in-time snapshots of the entire filesystem state without duplicating data
- **Writable Branches**: Create independent, diverging versions (branches) from any snapshot
- **Memory-Efficient Storage**: Primarily in-memory with transparent disk spillover for large files
- **Cross-Platform Implementation**: Core Rust library with platform-specific glue layers
- **Overlay Mode (LowerFS + Copy-Up)**: When mounted over an existing workspace, AgentFS behaves as an overlay:
  - **Pass-through reads** from the underlying ("lower") filesystem until a path is modified in a branch.
  - **Copy-up on first write or metadata-mutation**, creating an upper entry in the branch's overlay with metadata initialized from the lower object.
  - **Whiteouts** for deletes: upper entries that mask lower files/dirs without altering the lower layer.
  - **LowerFS Provider**: a pluggable platform component used by adapters for stat/readdir/readlink/open-for-read on unmodified paths.
  - **Consistency**: The branch/snapshot model remains authoritative; lower content is read-only input for branches that have not diverged at a path.
  - **Performance**: Reads of unmodified paths are pass-through (potentially kernel-fast on Linux via FUSE passthrough fd); only changed paths incur upper-layer I/O.
- **Kernel-Backstore Proxy (KBP)**: Adapters maintain hidden **host-FS handles** to files living under a configured **backstore root** (e.g., RAM disk).
  - **Read path**: If a path has **no upper entry**, the adapter opens a **read-only lower handle** and services reads directly from the underlying filesystem (no branch state is created by reads).
  - **Write path**: On first data mutation or metadata change that must persist, the core performs **copy-up**; the adapter opens an **upper backstore handle** and all subsequent I/O goes through the host kernel's fast paths.
  - **Integrity**: Permissions/ownership initialize from lower on copy-up; deletes are **whiteouts**; branch and snapshot rules are authoritative. The backstore path is **never visible** to applications.
- **Interpose Mode (optional, zero-overhead data path)**: For selected processes, AgentFS may run without a visible mount by using **API interposition** (e.g., `DYLD_INSERT_LIBRARIES` / Windows hooks).
  - The injected shim requests file opens from the AgentFS server and receives **real OS handles** (SCM_RIGHTS / DuplicateHandle). Data I/O bypasses the adapter entirely; path-level ops still route to AgentFS for overlay semantics.
  - **Eager Upperization**: When FD-forwarding is enabled, the system **always returns an upper handle**. If the path has not yet been copied-up, the core **creates the upper via reflink/clone** when available or by bounded copy subject to `interpose-max-copy-bytes`. If not feasible, the server declines FD-forwarding for that open and the shim **falls back to the mounted volume** (KBP).
  - Intended for controlled developer workflows where maximum throughput is required; not universal due to platform hardening policies.
  - **Implementation details:** See [macOS-FS-Hooks.md](macOS-FS-Hooks.md) and [Windows-FS-Hooks.md](Windows-FS-Hooks.md) for complete hook specifications.
- **Windows open-redirect (experimental)**: An optional **open-redirect** path may return STATUS_REPARSE to a backstore path for certain opens, handing the handle to NTFS. This bypasses adapter I/O but has caveats (handle-based rename/delete, share modes) and is disabled by default.

### Per-Process Mounting

- **Process-Scoped Views**: Each process can have its own isolated filesystem view (branch)
- **Isolation**: Changes in one branch are invisible to processes in other branches
- **Platform-Specific Integration**:
  - **macOS**: Uses chroot with overlay mounting
  - **Windows**: Implements drive letter or mount point isolation
  - **Linux**: Leverages native mount namespaces (complementary to existing capabilities)

## Implementation Architecture

- **Core Library**: Rust-based filesystem logic with comprehensive operations support and per-process branch binding
- **Backstore Manager (NEW)**: a core subsystem that provisions and owns a backing filesystem (ramdisk or host FS directory) used by the overlay's **upper** layer. It provides opaque handles to adapters and supports snapshot/export primitives when the native FS offers them.
- **LowerFS Provider Trait (Core/Adapter boundary)**:
  ```rust
  trait LowerFs {
      fn stat(&self, abs_path: &Path) -> Result<Metadata>;
      fn open_ro(&self, abs_path: &Path) -> Result<Box<dyn Read + Send>>;
      fn readdir(&self, abs_dir: &Path) -> Result<Vec<DirEntry>>;
      fn readlink(&self, abs_path: &Path) -> Result<PathBuf>;
      fn getxattr(&self, abs_path: &Path, name: &str) -> Result<Vec<u8>>;
      fn listxattr(&self, abs_path: &Path) -> Result<Vec<String>>;
  }
  ```
  The adapter implements `LowerFs` to talk to the host filesystem at a configured **lower root**. The core asks for lower metadata/content only when a path has no upper entry in the active branch. LowerFS handles are adapter-owned and must support concurrent access.
- **Platform-specific LowerFs Implementations**: Platform-specific modules implemetning the LowerFs trait are developed for each targetter platform.
- **Platform Glue Layers**:
  - Linux: FUSE integration with ioctl-based control plane
  - Windows: WinFsp integration with DeviceIoControl control plane
  - macOS: FSKit Unary File System extension with XPC control service
- **Snapshot Model**: CoW mechanism ensuring efficient storage and fast snapshot creation
- **Branch Isolation**: Per-process branch binding allows different processes to see different filesystem branches concurrently

### Overlay & Pass-through (clarified)

- AgentFS operates as an **overlay**: unmodified paths are served from the **lower** (real) filesystem via **pass-through reads**; on first mutation AgentFS performs **copy-up** into the **upper** backstore.
- **Pass-through reads are platform-optimized** but semantically identical across OS's. The adapter must not materialize an upper object for pure reads.
- On the first upper object materialization, **permissions/ownership/ACLs are initialized from the lower** (with policy knobs for umask/override).

### Copy-up Triggers & Whiteouts

- **Data writes** (`write`, `truncate>0`, `fallocate(punch)`) trigger **copy-up**:
  1. materialize an upper node with **metadata initialized from lower** (owner, group, mode, times, xattrs);
  2. allocate backing storage per `FsConfig.backstore`; and
  3. perform the write against the new upper data stream.
- **Metadata-only changes** (chmod/chown/utimens/xattr set) create a **metadata overlay** entry without copying file data. Subsequent reads of file data continue to pass through to lower until the first data write occurs.
- **Deletes** create a **whiteout** upper marker that hides the lower entry. Unlink/rename-over follow POSIX rules; open-unlinked handles remain valid until close.

### Branch Isolation Guarantees

- The only exception is the base underlying filesystem: if no branch has overridden a file, they all see the same underlying content.
- **Overlay rule:** When a path has no upper entry in the active branch, lookups/stat/readdir/readlink read **directly from the lower layer** via the LowerFS provider. No branch state is created by reads alone.

### Kernel-Backstore Proxy (KBP) vs Interpose modes

- **KBP (default, portable):** adapters service read/write/mmap callbacks but fulfill them using hidden **host-FS handles** into the backstore, leveraging kernel cache and readahead. Heavy I/O still crosses the adapter on cache miss/write-back.
- **Interpose / FD-Forwarding (optional, zero-overhead data path):** a per-process shim asks AgentFS to **open** and then receives a **real OS handle/FD** (SCM_RIGHTS / DuplicateHandle). Data I/O bypasses the adapter entirely; path-level ops still route to AgentFS for overlay semantics.
- **Windows open-redirect (experimental):** for eligible opens, the WinFsp adapter may return **STATUS_REPARSE** to a backstore path so the I/O manager completes the open directly on NTFS. Guarded by policy; semantics caveats documented below.

### Capability negotiation

At mount time AgentFS detects and records:
`cap.backstore={none|hostfs|ramdisk(native-snapshots|none)}`
`cap.fd_forwarding={off|shim|shim+reparse(win)}`
Adapters consult these to choose optimal paths; correctness is invariant to capability.

### Backstore

`FsConfig.backstore`: `{ mode: "InMemory" | "HostFs", root?: "<absolute host path>", prefer_native_snapshots: bool }`

- When `HostFs`, adapters use **KBP** against `root`. The core still governs overlay/branch/snapshot semantics.

### Interpose

`InterposeConfig` (optional component): handshake sockets, policy (allowlist of pids/bundles), default branch binding. Disabled by default.

## Current Implementation Approach

Unlike traditional overlay filesystems or mount namespace simulation, AgentFS implements **per-process branch binding directly in the core filesystem logic**, and can operate **as an overlay** atop a host workspace using the LowerFS provider. This approach provides:

- **Native Branch Isolation**: Each process can be bound to a specific filesystem branch at runtime
- **Cross-Platform Consistency**: Same branch binding mechanism works across all supported platforms
- **Efficient Resource Usage**: No need for multiple filesystem mounts or overlay layers
- **Direct Control**: Branch binding is managed through platform-specific control interfaces (XPC/ioctl/DeviceIoControl)

## Functional Requirements

- **Pass-through read-only lower access:** Reads **must** be serviced from the lower layer until copy-up is required. Copy-up triggers upper creation, initializing metadata (mode, uid/gid, ACL/xattrs) from lower.
- **Backstore**:
  - **Creation:** core can create a ramdisk+FS (platform-specific helper) or attach to a host directory. Failure to provision falls back to in-memory upper.
  - **Snapshot/export:** if the backstore FS supports **native snapshots** (e.g., APFS/Btrfs/ZFS), the core can invoke them; otherwise, AgentFS performs **selective file copy** for active upper entries at snapshot time.
- **Interpose/FD forwarding:** if enabled for a process, bulk I/O to handles obtained via AgentFS **must not** traverse AgentFS data callbacks; only control-plane ops (rename/unlink/metadata/snapshot) do.
- **Windows reparse fast-path (experimental):** may be used for open() when policy allows; handle-based rename/delete semantics risks are documented; disabled by default.

## Security and Access Control

On copy-up, initial **mode/owner/ACLs** are cloned from the lower entry. Policy can alter:

- `copyup.mode_strategy={clone|clone&umask|fixed}`
- `copyup.acl_strategy={clone|drop|map-basic}`
  Interpose mode inherits kernel enforcement for the returned OS handle; AgentFS still validates path-level ops in the control plane.

### Metadata Initialization in Overlay Mode

- When an upper entry is created by **copy-up**, AgentFS initializes:
  - `owner_uid`, `owner_gid`, `mode`, all timestamps, and selected xattrs **from the lower object** (subject to platform mapping), then applies the operation's requested changes (e.g., chmod target, utimens values).
- For **newly created** paths (no lower entry), initialize `owner_uid/gid` from the caller identity and `mode` from `(create_mode & ~umask)`.
- **Access checks** on unmodified paths use **lower metadata**. Once a metadata overlay (chmod/chown/utimens/xattr set) exists, checks consult the **upper metadata** even if data still passes through from lower.

### Platform Notes

- **Windows**: On copy-up, attempt to synthesize a Security Descriptor from lower (owner SID + DACL) or map to POSIX bits when `enable_windows_acl_compat=false`. Later changes via SetSecurity are stored in the upper entry and enforced by the adapter.

## Performance

- **KBP (cache-warm sequential)**: ≥ 90% of direct host-FS throughput for 1–4 GiB reads/writes.
- **Interpose**: ≈ native for direct FD I/O; metadata ops within 10–20% of baseline.
- **Snapshot (active set ≤ 5k files)**: ≤ 300 ms when native snapshotting is available; ≤ 2 s when selective copy is required.

## Limitations

- FSKit/WinFsp do **not** support general cross-FS handle splicing; thus KBP still receives read/write callbacks.
- Windows reparse open-redirect may expose **handle-based** rename/delete that bypasses overlay policy unless interposed; keep off by default or combine with shim.

## Use Cases

- **Isolated Agent Execution**: Each AI agent runs in its own filesystem branch
- **Multi-Version Testing**: Test applications against different filesystem states
- **Development Sandboxes**: Create isolated development environments
- **Cross-Platform Consistency**: Uniform filesystem behavior across all supported platforms
- **Overlay over user workspaces**: Fast, zero-copy reads from the real workspace until a branch mutates a path.

## Files in This Directory

- [AgentFS: Per-process FS mounts](AgentFS-Per-process-FS-mounts.md): Detailed specification for per-process mount namespace simulation
- [AgentFS: Snapshots and Branching](AgentFS-Snapshots-and-Branching.md): Comprehensive specification for snapshot and branching functionality

Implementation Status: See [AgentFS.status.md](AgentFS.status.md) for current milestones, tasks, and success criteria.
