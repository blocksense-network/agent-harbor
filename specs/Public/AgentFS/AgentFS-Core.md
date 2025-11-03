## AgentFS Core — Rust API Specification

### Purpose and Scope

- Provide a cross‑platform, in‑process filesystem core implementing snapshots, writable branches, and process‑scoped views (AgentFS) for macOS, Windows, and Linux.
- Expose a clean Rust API to embed in glue layers (FUSE/libfuse on Linux, WinFsp on Windows, FSKit on macOS) and a C ABI for Swift/C++ integrations.
- Ensure POSIX/NT semantics (configurable case sensitivity, xattrs/ADS, locking, share modes) with high concurrency and memory‑efficient CoW.

## Backstore Manager

The core exposes a `Backstore` abstraction with implementations:
`InMemoryUpper`, `HostFsUpper{root: Path}` (mostly platform-agnostic logic, but reliant on platform-specific setup), `RamdiskUpper{mount: Path, fs: FsKind, supports_native_snapshots: bool}` (platform-specific).
Responsibilities:

1. **Provisioning**: `create_ramdisk(cfg)`, `attach_hostfs(root)`, `fallback_inmem()`.
2. **Handles**: open RO **lower** and RW **upper** host handles used by adapters.
3. **Metadata seeding**: `seed_metadata_from_lower(lower_meta) -> UpperMetaInit`.
4. **Snapshot/export**: `snapshot_native(label)` if supported; else `snapshot_copy(active_set)`.

### Lower (read-through) path

`vfs.lookup(path)` returns a **lower view** unless an upper entry exists. `read(path, off, len)`:

1. If `upper.exists`: read via upper handle.
2. Else: read via **lower handle** (no upper materialization).

### Copy-up & metadata initialization

On first mutation (`create`, `write`, `setattr`, etc.), core:

1. Creates an upper node and copies data lazily/greedily per file policy.
2. Initializes mode/uid/gid/acl/xattrs from lower via `Backstore.seed_metadata_from_lower`.
3. Records whiteouts for deletes.

### Interpose / FD forwarding control-plane

Core provides `fd_open(req)` which **always materializes an upper entry** for interposed processes:

- On first interposed open of a path—even if O_RDONLY—AgentFS creates an upper entry via:
  - **reflink/clone** (O(metadata) cost) when supported by the backstore filesystem
  - **bounded copy** (up to `interpose-max-copy-bytes`) when reflink unavailable but file size ≤ threshold
  - **decline forwarding** with `FORWARDING_UNAVAILABLE` when size exceeds threshold and no reflink available
- Returns a **transferable upper handle** descriptor:
  - Unix: sent via `SCM_RIGHTS` to shim.
  - Windows: duplicated via `DuplicateHandle` to shim process.
- **Fallback**: When forwarding unavailable, shim falls back to KBP (mounted volume) for that process/file.
  Core maintains path ↔ handle bookkeeping for snapshots/branch isolation and validates path ops (`rename/unlink/chmod/...`) via control messages.

### Snapshot semantics with backstore

If `supports_native_snapshots==true`, `snapshot_native(label)` maps branch roots to native snapshot IDs and records cross-mapping. Otherwise core computes the **active set** (upper objects/metadata) and materializes a copy in a snapshot store (efficient for typical small active sets).

#### Clonefile-based snapshot materialization

For interpose mode on filesystems supporting native block cloning (APFS `clonefile`, ReFS block cloning), snapshots can be **materialized on disk** using filesystem-native copy-on-write operations:

- **macOS APFS**: When `snapshot_create()` is called in interpose mode on APFS-backed storage, the core iterates through all upper layer files and calls `clonefile()` to create persistent clones in a snapshot directory. This leverages APFS's block-level copy-on-write: unmodified blocks are shared between the original and cloned files, while writes to either file allocate new blocks only for modified regions.

- **Windows ReFS**: Similar approach using `FSCTL_DUPLICATE_EXTENTS_TO_FILE` to create block-level clones. Constraints include same-volume requirements, cluster alignment, and ≤4GB per clone operation, but automatic block cloning is supported in copy operations as of Windows 11 24H2.

**Benefits:**
- **Persistence**: When backed by a directory (not a RamDisk), cloned files remain accessible even if AgentFS process terminates.
- **Memory efficiency**: Large files don't consume RAM for snapshots.
- **Performance**: Filesystem handles CoW natively through the fast paths in the kernel.

**Implementation**: Snapshot directories contain `clonefile`/`block-clone` copies of upper layer files. Branch creation from such snapshots uses these cloned files as the baseline, with further modifications creating additional clones as needed.

### High‑Level Architecture

- Core modules (Rust crates/modules):
  - core: `FsCore`, primary entrypoint; orchestrates namespace, VFS, storage, locking, snapshots, events.
  - namespace: branch and snapshot management (`BranchId`, `SnapshotId`, `BranchManager`, process‑scoped bindings).
  - vfs: virtual filesystem tree (directories, files, symlinks), filehandles, path resolution, metadata.
  - storage: content store with CoW, in‑memory primary storage plus disk spillover; pluggable `StorageBackend`.
  - metadata: attributes, times, permissions, optional ACL model.
  - streams: extended attributes and alternate data streams (ADS).
  - locking: byte‑range locks, Windows share modes, open handle tables.
  - snapshots: immutable tree roots with reference counting; writable clones as branches.
  - config: case sensitivity, limits, memory policy, feature flags.
  - events: change notifications and lifecycle hooks.
  - ffi: C ABI surface for glue layers.

### Core Types and IDs

- `SnapshotId`: opaque, stable identifier (ULID‑like) for read‑only points.
- `BranchId`: opaque identifier for writable branches (derived from a snapshot or current state).
- `HandleId`: opaque identifier for open file descriptors.
- `NodeId`: internal inode identifier (not exposed across ABI boundaries).

### Configuration

```rust
pub enum CaseSensitivity { Sensitive, InsensitivePreserving }

pub struct MemoryPolicy {
    pub max_bytes_in_memory: Option<u64>,
    pub spill_directory: Option<std::path::PathBuf>,
}

pub struct FsLimits {
    pub max_open_handles: u32,
    pub max_branches: u32,
    pub max_snapshots: u32,
}

pub struct CachePolicy {
    // Mirrors libfuse defaults and winfsp config patterns
    pub attr_ttl_ms: u32,          // attributes cache TTL
    pub entry_ttl_ms: u32,         // dentry cache TTL
    pub negative_ttl_ms: u32,      // negative dentry TTL
    pub enable_readdir_plus: bool, // return attrs alongside dir entries where supported
    pub auto_cache: bool,          // lower FS changes reflected immediately (passthrough patterns)
    pub writeback_cache: bool,     // allow kernel to buffer writes (requires robust fsync)
}

pub struct FsConfig {
    pub case_sensitivity: CaseSensitivity,
    pub memory: MemoryPolicy,
    pub limits: FsLimits,
    pub cache: CachePolicy,
    pub enable_xattrs: bool,
    pub enable_ads: bool,          // Windows
    pub track_events: bool,
    /// Overlay lower layer (optional)
    pub lower: Option<LowerConfig>,
    /// Backing store mode for upper data
    pub backstore: BackstoreConfig,
    /// Interpose/FD-forwarding configuration
    pub interpose: InterposeConfig,
}

pub struct InterposeConfig {
    /// Whether FD-forwarding is enabled
    pub forwarding: ForwardingMode,
    /// Maximum bytes to copy when creating upper entries for FD-forwarding
    pub max_copy_bytes: u64,
    /// Whether to require reflink/clone support for forwarding (fail if unavailable)
    pub require_reflink: bool,
}

pub enum ForwardingMode {
    /// FD-forwarding disabled
    Disabled,
    /// Eager upperization for all interposed opens
    EagerUpperize,
}

pub struct LowerConfig {
    /// Absolute host path used as the lower root (read-only view)
    pub root: std::path::PathBuf,
}

pub enum BackstoreMode { InMemory, HostFs }

pub struct BackstoreConfig {
    pub mode: BackstoreMode,
    /// Host path for upper files when `HostFs` (e.g., a RAM disk mount)
    pub root: Option<std::path::PathBuf>,
    /// Try to use native snapshots when supported by the host filesystem
    pub prefer_native_snapshots: bool,
}
```

### Error Model

```rust
#[derive(thiserror::Error, Debug)]
pub enum FsError {
    #[error("not found")] NotFound,
    #[error("already exists")] AlreadyExists,
    #[error("access denied")] AccessDenied,
    #[error("invalid argument")] InvalidArgument,
    #[error("name not allowed")] InvalidName,
    #[error("not a directory")] NotADirectory,
    #[error("is a directory")] IsADirectory,
    #[error("busy")] Busy,
    #[error("too many open files")] TooManyOpenFiles,
    #[error("no space left")] NoSpace,
    #[error("io error: {0}")] Io(std::io::Error),
    #[error("unsupported")] Unsupported,
}

pub type FsResult<T> = Result<T, FsError>;
```

### Storage Backend (Copy‑on‑Write)

```rust
pub trait StorageBackend: Send + Sync {
    fn read(&self, id: ContentId, offset: u64, buf: &mut [u8]) -> FsResult<usize>;
    fn write(&self, id: ContentId, offset: u64, data: &[u8]) -> FsResult<usize>;
    fn truncate(&self, id: ContentId, new_len: u64) -> FsResult<()>;
    fn allocate(&self, initial: &[u8]) -> FsResult<ContentId>;
    fn clone_cow(&self, base: ContentId) -> FsResult<ContentId>;
    fn seal(&self, id: ContentId) -> FsResult<()>; // for snapshot immutability
}

pub struct InMemoryBackend { /* ... */ }
pub struct TempFileBackend { /* ... */ }
```

### Metadata, Streams, and Xattrs

```rust
#[derive(Clone, Copy)]
pub struct FileTimes { pub atime: i64, pub mtime: i64, pub ctime: i64, pub birthtime: i64 }

#[derive(Clone)]
pub struct FileMode { pub read: bool, pub write: bool, pub exec: bool }

#[derive(Clone)]
pub struct Attributes {
    pub len: u64,
    pub times: FileTimes,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub mode_user: FileMode,
    pub mode_group: FileMode,
    pub mode_other: FileMode,
}

pub struct XattrEntry { pub name: String, pub value: Vec<u8> }
pub struct StreamSpec { pub name: String }
```

### Locking and Share Modes

```rust
#[derive(Clone, Copy)]
pub enum ShareMode { Read, Write, Delete }

#[derive(Clone, Copy)]
pub enum LockKind { Shared, Exclusive }

pub struct LockRange { pub offset: u64, pub len: u64, pub kind: LockKind }
```

### Branching and Process‑Scoped Views

```rust
pub struct BranchInfo { pub id: BranchId, pub parent: Option<SnapshotId>, pub name: Option<String> }

pub trait BranchManager: Send + Sync {
    fn current_branch(&self) -> BranchId;
    fn create_branch(&self, from: impl Into<SnapshotRef>, name: Option<&str>) -> FsResult<BranchId>;
    fn list_branches(&self) -> Vec<BranchInfo>;
    fn delete_branch(&self, id: BranchId) -> FsResult<()>;
}

pub trait ProcessBinding: Send + Sync {
    fn bind_current_process(&self, target: BranchId) -> FsResult<()>;  // used by glue before exec/attach
    fn unbind_current_process(&self) -> FsResult<()>;
}
```

### Core VFS API (Path‑based façade)

```rust
pub struct OpenOptions {
    pub read: bool,
    pub write: bool,
    pub create: bool,
    pub truncate: bool,
    pub append: bool,
    pub share: Vec<ShareMode>,
    pub stream: Option<String>, // Windows ADS; None = unnamed data
}

pub struct DirEntry { pub name: String, pub is_dir: bool, pub is_symlink: bool, pub len: u64 }

pub struct FsCore { /* fields hidden */ }

impl FsCore {
    pub fn new(cfg: FsConfig) -> FsResult<Self>;

    // Snapshots & branches
    pub fn snapshot_create(&self, name: Option<&str>) -> FsResult<SnapshotId>;
    pub fn branch_create_from_snapshot(&self, snap: SnapshotId, name: Option<&str>) -> FsResult<BranchId>;
    pub fn branch_create_from_current(&self, name: Option<&str>) -> FsResult<BranchId>;
    pub fn branch_list(&self) -> Vec<BranchInfo>;
    pub fn snapshot_list(&self) -> Vec<(SnapshotId, Option<String>)>;
    pub fn snapshot_delete(&self, snap: SnapshotId) -> FsResult<()>;

    // Process‑scoped view control (library‑level)
    pub fn bind_process_to_branch(&self, branch: BranchId) -> FsResult<()>;
    pub fn unbind_process(&self) -> FsResult<()>;

    // Files & directories (path‑based façade expected by glue layers)
    pub fn mkdir(&self, path: &std::path::Path, mode: u32) -> FsResult<()>;
    pub fn rmdir(&self, path: &std::path::Path) -> FsResult<()>;
    pub fn create(&self, path: &std::path::Path, opts: &OpenOptions) -> FsResult<HandleId>;
    pub fn open(&self, path: &std::path::Path, opts: &OpenOptions) -> FsResult<HandleId>;
    pub fn getattr(&self, path: &std::path::Path) -> FsResult<Attributes>;
    pub fn set_times(&self, path: &std::path::Path, times: FileTimes) -> FsResult<()>;
    pub fn symlink(&self, target: &std::path::Path, link_path: &std::path::Path) -> FsResult<()>;
    pub fn readlink(&self, link_path: &std::path::Path) -> FsResult<std::path::PathBuf>;
    pub fn link(&self, existing: &std::path::Path, new_link: &std::path::Path) -> FsResult<()>;
    pub fn rename(&self, from: &std::path::Path, to: &std::path::Path, replace: bool) -> FsResult<()>;
    pub fn unlink(&self, path: &std::path::Path) -> FsResult<()>;
    pub fn readdir(&self, path: &std::path::Path) -> FsResult<Vec<DirEntry>>;
    // Optional readdir+ that includes attributes without extra getattr calls (libfuse pattern)
    pub fn readdir_plus(&self, path: &std::path::Path) -> FsResult<Vec<(DirEntry, Attributes)>>;

    // File handle I/O
    pub fn read(&self, h: HandleId, offset: u64, buf: &mut [u8]) -> FsResult<usize>;
    pub fn write(&self, h: HandleId, offset: u64, data: &[u8]) -> FsResult<usize>;
    pub fn truncate(&self, h: HandleId, new_len: u64) -> FsResult<()>;
    pub fn flush(&self, h: HandleId) -> FsResult<()>;
    pub fn fsync(&self, h: HandleId, data_only: bool) -> FsResult<()>;
    pub fn close(&self, h: HandleId) -> FsResult<()>;

    // Locks & share modes
    pub fn lock(&self, h: HandleId, range: LockRange) -> FsResult<()>;
    pub fn unlock(&self, h: HandleId, range: LockRange) -> FsResult<()>;

    // Advanced I/O helpers (optional)
    pub fn copy_file_range(&self, src: HandleId, src_off: u64, dst: HandleId, dst_off: u64, len: u64) -> FsResult<u64>;
    pub fn fallocate(&self, h: HandleId, mode: FallocateMode, offset: u64, len: u64) -> FsResult<()>;

    // Extended attributes & streams
    pub fn xattr_get(&self, path: &std::path::Path, name: &str) -> FsResult<Vec<u8>>;
    pub fn xattr_set(&self, path: &std::path::Path, name: &str, value: &[u8]) -> FsResult<()>;
    pub fn xattr_list(&self, path: &std::path::Path) -> FsResult<Vec<String>>;

    pub fn streams_list(&self, path: &std::path::Path) -> FsResult<Vec<StreamSpec>>;

    // Events and stats
    pub fn subscribe_events(&self, cb: Arc<dyn EventSink>) -> FsResult<SubscriptionId>;
    pub fn unsubscribe_events(&self, sub: SubscriptionId) -> FsResult<()>;
    pub fn stats(&self) -> FsStats;
}
```

### Events API

```rust
pub enum EventKind {
    Created { path: String },
    Removed { path: String },
    Modified { path: String },
    Renamed { from: String, to: String },
    BranchCreated { id: BranchId, name: Option<String> },
    SnapshotCreated { id: SnapshotId, name: Option<String> },
}

pub trait EventSink: Send + Sync {
    fn on_event(&self, evt: &EventKind);
}

pub struct FsStats {
    pub branches: u32,
    pub snapshots: u32,
    pub open_handles: u32,
    pub bytes_in_memory: u64,
    pub bytes_spilled: u64,
}
```

### Internal Design Notes

- Immutable directory trees (persistent data structures) per snapshot; branches point to a root; updates path‑copy nodes (copy‑on‑write) to preserve sharing.
- File content stored as chunks with reference counts; `clone_cow` when a branch first writes.
- Lookup layer implements case sensitivity per config; path normalization rules per platform are applied in glue, core uses UTF‑8 paths.
- Lock manager separates Windows share mode admission (at open) and POSIX record locks (runtime), tracked by `HandleId`.
- Provide optional inode‑oriented low‑level façade to align with libfuse low‑level (future): `InodeId`, `lookup()`, `forget()`, `readdir_ll()` with cookies; keeps core portable while enabling higher performance glue when needed.
- Cache policy mirrors libfuse configuration (`attr_ttl`, `entry_ttl`, `negative_ttl`, `auto_cache`) and WinFsp change notification expectations; readdir+ minimizes getattr round‑trips.
- Delete‑on‑close behavior is modeled: unlink hides entries immediately; actual storage reclamation occurs on last handle close (Windows semantics via share‑modes/deferred delete).

### C ABI (FFI) Surface (subset)

```c
// Error codes map to POSIX errno or NTSTATUS translated to canonical errors.
typedef struct AfFs AfFs;
typedef struct { uint8_t bytes[16]; } AfSnapshotId;
typedef struct { uint8_t bytes[16]; } AfBranchId;
typedef uint64_t AfHandleId;

typedef enum {
  AF_OK = 0,
  AF_ERR_NOT_FOUND = 2,
  AF_ERR_EXISTS = 17,
  AF_ERR_ACCES = 13,
  AF_ERR_NOSPC = 28,
  AF_ERR_INVAL = 22,
  AF_ERR_BUSY = 16,
  AF_ERR_UNSUPPORTED = 95
} AfResult;

// Lifecycle
AfResult af_fs_create(const char *config_json, AfFs **out_fs);
AfResult af_fs_destroy(AfFs *fs);

// Snapshots / branches
AfResult af_snapshot_create(AfFs *fs, const char *name, AfSnapshotId *out_id);
AfResult af_branch_create_from_snapshot(AfFs *fs, AfSnapshotId snap, const char *name, AfBranchId *out_id);
AfResult af_bind_process_to_branch(AfFs *fs, AfBranchId branch);

// File ops (UTF‑8 paths)
AfResult af_mkdir(AfFs *fs, const char *path, unsigned mode);
AfResult af_open(AfFs *fs, const char *path, const char *options_json, AfHandleId *out_h);
AfResult af_read(AfFs *fs, AfHandleId h, uint64_t off, void *buf, uint32_t len, uint32_t *out_read);
AfResult af_write(AfFs *fs, AfHandleId h, uint64_t off, const void *buf, uint32_t len, uint32_t *out_written);
AfResult af_close(AfFs *fs, AfHandleId h);
```

Notes:

- JSON for configuration/options keeps ABI surface small and forwards‑compatible (glue can parse/validate).
- Glue layers translate platform specifics (e.g., security descriptors, Finder resource forks) to core concepts.

### Usage Sketch (Rust)

```rust
let core = FsCore::new(FsConfig {
    case_sensitivity: CaseSensitivity::InsensitivePreserving,
    memory: MemoryPolicy { max_bytes_in_memory: Some(512<<20), spill_directory: Some("/tmp/agentfs".into()) },
    limits: FsLimits { max_open_handles: 65536, max_branches: 256, max_snapshots: 4096 },
    cache: CachePolicy { attr_ttl_ms: 0, entry_ttl_ms: 0, negative_ttl_ms: 0, enable_readdir_plus: true, auto_cache: true, writeback_cache: true },
    enable_xattrs: true,
    enable_ads: true,
    track_events: true,
})?;

let snap = core.snapshot_create(Some("clean"))?;
let b1 = core.branch_create_from_snapshot(snap, Some("task‑123"))?;
core.bind_process_to_branch(b1)?; // glue calls this before launching agent

core.mkdir("/project".as_ref(), 0o755)?;
let h = core.create("/project/README.md".as_ref(), &OpenOptions{ read:true, write:true, create:true, truncate:true, append:false, share:vec![], stream:None })?;
core.write(h, 0, b"Hello AgentFS")?;
core.close(h)?;
```

### Invariants and Guarantees

- Snapshot immutability: once created, a snapshot’s tree and file contents are never modified.
- Branch isolation: changes in one branch are not visible in any other branch unless explicitly copied.
- Handle stability: a `HandleId` references a node version within the branch context that opened it; switching process bindings does not retarget existing handles.
- Crash safety (in‑memory semantics): operations are linearizable at API boundaries; on process crash, no partial in‑core corruption is observable after restart.

### Extensibility Hooks

- Storage plug‑ins implementing `StorageBackend` (e.g., memory‑only, temp‑file, chunked dedupe).
- Event sinks for glue to forward change notifications to platform facilities (WinFsp change notify, FSKit events, inotify bridges).
- Optional policy module for egress/egress‑free sandboxes (outside core filesystem semantics).

### Design Influences from Reference Projects

- WinFsp: API shape separates admission (share modes) from runtime I/O; explicit delete‑on‑close; change notifications bridged via event sinks; FSCTL‑style control channel is mirrored by our event/FFI control functions.
- libfuse: cache knobs (`attr_timeout`, `entry_timeout`, `negative_timeout`), readdir+ optimization, direct I/O vs writeback cache; our cache policy maps cleanly to these.
- sandboxfs: path mapping and reconfiguration patterns inform our process/branch binding and overlay whiteout handling; concurrent threadpool patterns guide internal locking layout; explicit error model and mapping.
- FSKit: case‑insensitive‑preserving default on macOS; extended attributes are frequently used (quarantine, FinderInfo); our xattr support is first‑class and orthogonal.
