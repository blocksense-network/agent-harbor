// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Control plane message types for AgentFS

// Note: Using u32 for serialization instead of c_int to work with SSZ
use ssz::{Decode, Encode};
use ssz_derive::{Decode, Encode};

// SSZ Union-based request/response types for type-safe communication
// Using Vec<u8> for strings as SSZ supports variable-length byte vectors

/// Request union - each variant contains version and operation-specific data
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
#[ssz(enum_behaviour = "union")]
pub enum Request {
    SnapshotCreate((Vec<u8>, SnapshotCreateRequest)), // (version, request)
    SnapshotList(Vec<u8>),                            // version
    BranchCreate((Vec<u8>, BranchCreateRequest)),     // (version, request)
    BranchBind((Vec<u8>, BranchBindRequest)),         // (version, request)
    FdOpen((Vec<u8>, FdOpenRequest)),                 // (version, request)
    FdDup((Vec<u8>, FdDupRequest)),                   // (version, request)
    DirOpen((Vec<u8>, DirOpenRequest)),               // (version, request)
    DirRead((Vec<u8>, DirReadRequest)),               // (version, request)
    DirClose((Vec<u8>, DirCloseRequest)),             // (version, request)
    Readlink((Vec<u8>, ReadlinkRequest)),             // (version, request)
    Stat((Vec<u8>, StatRequest)),                     // (version, request)
    Lstat((Vec<u8>, LstatRequest)),                   // (version, request)
    Fstat((Vec<u8>, FstatRequest)),                   // (version, request)
    Fstatat((Vec<u8>, FstatatRequest)),               // (version, request)
    Chmod((Vec<u8>, ChmodRequest)),                   // (version, request)
    Fchmod((Vec<u8>, FchmodRequest)),                 // (version, request)
    Fchmodat((Vec<u8>, FchmodatRequest)),             // (version, request)
    Chown((Vec<u8>, ChownRequest)),                   // (version, request)
    Lchown((Vec<u8>, LchownRequest)),                 // (version, request)
    Fchown((Vec<u8>, FchownRequest)),                 // (version, request)
    Fchownat((Vec<u8>, FchownatRequest)),             // (version, request)
    Utimes((Vec<u8>, UtimesRequest)),                 // (version, request)
    Futimes((Vec<u8>, FutimesRequest)),               // (version, request)
    Utimensat((Vec<u8>, UtimensatRequest)),           // (version, request)
    Futimens((Vec<u8>, FutimensRequest)),             // (version, request)
    Truncate((Vec<u8>, TruncateRequest)),             // (version, request)
    Ftruncate((Vec<u8>, FtruncateRequest)),           // (version, request)
    Statfs((Vec<u8>, StatfsRequest)),                 // (version, request)
    Fstatfs((Vec<u8>, FstatfsRequest)),               // (version, request)
    Rename((Vec<u8>, RenameRequest)),                 // (version, request)
    Renameat((Vec<u8>, RenameatRequest)),             // (version, request)
    RenameatxNp((Vec<u8>, RenameatxNpRequest)),       // (version, request)
    Link((Vec<u8>, LinkRequest)),                     // (version, request)
    Linkat((Vec<u8>, LinkatRequest)),                 // (version, request)
    Symlink((Vec<u8>, SymlinkRequest)),               // (version, request)
    Symlinkat((Vec<u8>, SymlinkatRequest)),           // (version, request)
    Unlink((Vec<u8>, UnlinkRequest)),                 // (version, request)
    Unlinkat((Vec<u8>, UnlinkatRequest)),             // (version, request)
    Remove((Vec<u8>, RemoveRequest)),                 // (version, request)
    Mkdir((Vec<u8>, MkdirRequest)),                   // (version, request)
    Mkdirat((Vec<u8>, MkdiratRequest)),               // (version, request)
    PathOp((Vec<u8>, PathOpRequest)),                 // (version, request)
    InterposeSetGet((Vec<u8>, InterposeSetGetRequest)), // (version, request)
    DaemonStateProcesses(DaemonStateProcessesRequest), // version - for testing
    DaemonStateStats(DaemonStateStatsRequest),        // version - for testing
    DaemonStateFilesystem(DaemonStateFilesystemRequest), // dummy data - for testing
}

/// Response union - operation-specific success responses or errors
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
#[ssz(enum_behaviour = "union")]
pub enum Response {
    SnapshotCreate(SnapshotCreateResponse),
    SnapshotList(SnapshotListResponse),
    BranchCreate(BranchCreateResponse),
    BranchBind(BranchBindResponse),
    FdOpen(FdOpenResponse),
    FdDup(FdDupResponse),
    DirOpen(DirOpenResponse),
    DirRead(DirReadResponse),
    DirClose(DirCloseResponse),
    Readlink(ReadlinkResponse),
    Stat(StatResponse),
    Lstat(LstatResponse),
    Fstat(FstatResponse),
    Fstatat(FstatatResponse),
    Chmod(ChmodResponse),
    Fchmod(FchmodResponse),
    Fchmodat(FchmodatResponse),
    Chown(ChownResponse),
    Lchown(LchownResponse),
    Fchown(FchownResponse),
    Fchownat(FchownatResponse),
    Utimes(UtimesResponse),
    Futimes(FutimesResponse),
    Utimensat(UtimensatResponse),
    Futimens(FutimensResponse),
    Truncate(TruncateResponse),
    Ftruncate(FtruncateResponse),
    Statfs(StatfsResponse),
    Fstatfs(FstatfsResponse),
    Rename(RenameResponse),
    Renameat(RenameatResponse),
    RenameatxNp(RenameatxNpResponse),
    Link(LinkResponse),
    Linkat(LinkatResponse),
    Symlink(SymlinkResponse),
    Symlinkat(SymlinkatResponse),
    Unlink(UnlinkResponse),
    Unlinkat(UnlinkatResponse),
    Remove(RemoveResponse),
    Mkdir(MkdirResponse),
    Mkdirat(MkdiratResponse),
    PathOp(PathOpResponse),
    InterposeSetGet(InterposeSetGetResponse),
    DaemonState(DaemonStateResponseWrapper),
    Error(ErrorResponse),
}

/// Error response
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct ErrorResponse {
    pub error: Vec<u8>,
    pub code: Option<u32>,
}

/// Snapshot creation request payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct SnapshotCreateRequest {
    pub name: Option<Vec<u8>>,
}

/// Snapshot creation response payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct SnapshotCreateResponse {
    pub snapshot: SnapshotInfo,
}

/// Snapshot list request payload (empty)
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct SnapshotListRequest {}

/// Snapshot list response payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct SnapshotListResponse {
    pub snapshots: Vec<SnapshotInfo>,
}

/// Snapshot information
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct SnapshotInfo {
    pub id: Vec<u8>,
    pub name: Option<Vec<u8>>,
}

/// Branch creation request payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct BranchCreateRequest {
    pub from: Vec<u8>,
    pub name: Option<Vec<u8>>,
}

/// Branch creation response payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct BranchCreateResponse {
    pub branch: BranchInfo,
}

/// Branch information
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct BranchInfo {
    pub id: Vec<u8>,
    pub name: Option<Vec<u8>>,
    pub parent: Vec<u8>,
}

/// Branch bind request payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct BranchBindRequest {
    pub branch: Vec<u8>,
    pub pid: Option<u32>,
}

/// Branch bind response payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct BranchBindResponse {
    pub branch: Vec<u8>,
    pub pid: u32,
}

/// FdOpen request payload for interpose forwarding
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FdOpenRequest {
    pub path: Vec<u8>,
    pub flags: u32,
    pub mode: u32,
}

/// FdOpen response payload with file descriptor
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FdOpenResponse {
    pub fd: u32,
}

/// FdDup request payload for duplicating file descriptors
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FdDupRequest {
    pub fd: u32,
}

/// FdDup response payload with duplicated file descriptor
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FdDupResponse {
    pub fd: u32,
}

/// DirOpen request payload for directory operations
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct DirOpenRequest {
    pub path: Vec<u8>,
}

/// DirOpen response payload with directory handle
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct DirOpenResponse {
    pub handle: u64,
}

/// DirRead request payload for reading directory entries
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct DirReadRequest {
    pub handle: u64,
}

/// DirRead response payload with directory entries
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct DirReadResponse {
    pub entries: Vec<DirEntry>,
}

/// DirClose request payload for closing directory handles
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct DirCloseRequest {
    pub handle: u64,
}

/// DirClose response payload (empty on success)
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct DirCloseResponse {}

/// Readlink request payload for reading symbolic links
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct ReadlinkRequest {
    pub path: Vec<u8>,
}

/// Readlink response payload with link target
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct ReadlinkResponse {
    pub target: Vec<u8>,
}

/// Stat structure representing file attributes (similar to libc::stat)
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct StatData {
    pub st_dev: u64,        // Device ID
    pub st_ino: u64,        // Inode number
    pub st_mode: u32,       // File mode
    pub st_nlink: u32,      // Number of hard links
    pub st_uid: u32,        // User ID
    pub st_gid: u32,        // Group ID
    pub st_rdev: u64,       // Device ID (special files)
    pub st_size: u64,       // File size (using u64, handle negative as needed)
    pub st_blksize: u32,    // Block size
    pub st_blocks: u64,     // Number of blocks
    pub st_atime: u64,      // Access time (seconds)
    pub st_atime_nsec: u32, // Access time (nanoseconds)
    pub st_mtime: u64,      // Modification time (seconds)
    pub st_mtime_nsec: u32, // Modification time (nanoseconds)
    pub st_ctime: u64,      // Change time (seconds)
    pub st_ctime_nsec: u32, // Change time (nanoseconds)
}

/// Timespec structure for nanosecond-precision timestamps
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct TimespecData {
    pub tv_sec: u64,  // Seconds (using u64, handle negative as needed)
    pub tv_nsec: u32, // Nanoseconds
}

/// Statfs structure representing filesystem statistics (similar to libc::statfs)
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct StatfsData {
    pub f_bsize: u32,   // Block size
    pub f_frsize: u32,  // Fragment size
    pub f_blocks: u64,  // Total blocks
    pub f_bfree: u64,   // Free blocks
    pub f_bavail: u64,  // Available blocks
    pub f_files: u64,   // Total inodes
    pub f_ffree: u64,   // Free inodes
    pub f_favail: u64,  // Available inodes
    pub f_fsid: u32,    // Filesystem ID
    pub f_flag: u64,    // Mount flags
    pub f_namemax: u32, // Maximum filename length
}

/// Stat request payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct StatRequest {
    pub path: Vec<u8>,
}

/// Stat response payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct StatResponse {
    pub stat: StatData,
}

/// Lstat request payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct LstatRequest {
    pub path: Vec<u8>,
}

/// Lstat response payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct LstatResponse {
    pub stat: StatData,
}

/// Fstat request payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FstatRequest {
    pub fd: u32,
}

/// Fstat response payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FstatResponse {
    pub stat: StatData,
}

/// Fstatat request payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FstatatRequest {
    pub dirfd: u32,
    pub path: Vec<u8>,
    pub flags: u32,
}

/// Fstatat response payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FstatatResponse {
    pub stat: StatData,
}

/// Chmod request payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct ChmodRequest {
    pub path: Vec<u8>,
    pub mode: u32,
}

/// Chmod response payload (empty on success)
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct ChmodResponse {}

/// Fchmod request payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FchmodRequest {
    pub fd: u32,
    pub mode: u32,
}

/// Fchmod response payload (empty on success)
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FchmodResponse {}

/// Fchmodat request payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FchmodatRequest {
    pub dirfd: u32,
    pub path: Vec<u8>,
    pub mode: u32,
    pub flags: u32,
}

/// Fchmodat response payload (empty on success)
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FchmodatResponse {}

/// Chown request payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct ChownRequest {
    pub path: Vec<u8>,
    pub uid: u32,
    pub gid: u32,
}

/// Chown response payload (empty on success)
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct ChownResponse {}

/// Lchown request payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct LchownRequest {
    pub path: Vec<u8>,
    pub uid: u32,
    pub gid: u32,
}

/// Lchown response payload (empty on success)
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct LchownResponse {}

/// Fchown request payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FchownRequest {
    pub fd: u32,
    pub uid: u32,
    pub gid: u32,
}

/// Fchown response payload (empty on success)
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FchownResponse {}

/// Fchownat request payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FchownatRequest {
    pub dirfd: u32,
    pub path: Vec<u8>,
    pub uid: u32,
    pub gid: u32,
    pub flags: u32,
}

/// Fchownat response payload (empty on success)
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FchownatResponse {}

/// Utimes request payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct UtimesRequest {
    pub path: Vec<u8>,
    pub times: Option<(TimespecData, TimespecData)>, // (atime, mtime), None for current time
}

/// Utimes response payload (empty on success)
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct UtimesResponse {}

/// Futimes request payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FutimesRequest {
    pub fd: u32,
    pub times: Option<(TimespecData, TimespecData)>, // (atime, mtime), None for current time
}

/// Futimes response payload (empty on success)
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FutimesResponse {}

/// Utimensat request payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct UtimensatRequest {
    pub dirfd: u32,
    pub path: Vec<u8>,
    pub times: Option<(TimespecData, TimespecData)>, // (atime, mtime), None for current time
    pub flags: u32,
}

/// Utimensat response payload (empty on success)
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct UtimensatResponse {}

/// Futimens request payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FutimensRequest {
    pub fd: u32,
    pub times: Option<(TimespecData, TimespecData)>, // (atime, mtime), None for current time
}

/// Futimens response payload (empty on success)
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FutimensResponse {}

/// Truncate request payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct TruncateRequest {
    pub path: Vec<u8>,
    pub length: u64,
}

/// Truncate response payload (empty on success)
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct TruncateResponse {}

/// Ftruncate request payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FtruncateRequest {
    pub fd: u32,
    pub length: u64,
}

/// Ftruncate response payload (empty on success)
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FtruncateResponse {}

/// Statfs request payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct StatfsRequest {
    pub path: Vec<u8>,
}

/// Statfs response payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct StatfsResponse {
    pub statfs: StatfsData,
}

/// Fstatfs request payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FstatfsRequest {
    pub fd: u32,
}

/// Fstatfs response payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FstatfsResponse {
    pub statfs: StatfsData,
}

/// Rename request payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct RenameRequest {
    pub old_path: Vec<u8>,
    pub new_path: Vec<u8>,
}

/// Rename response payload (empty on success)
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct RenameResponse {}

/// Renameat request payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct RenameatRequest {
    pub old_dirfd: u32,
    pub old_path: Vec<u8>,
    pub new_dirfd: u32,
    pub new_path: Vec<u8>,
}

/// Renameat response payload (empty on success)
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct RenameatResponse {}

/// RenameatxNp request payload (macOS-specific)
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct RenameatxNpRequest {
    pub old_dirfd: u32,
    pub old_path: Vec<u8>,
    pub new_dirfd: u32,
    pub new_path: Vec<u8>,
    pub flags: u32,
}

/// RenameatxNp response payload (empty on success)
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct RenameatxNpResponse {}

/// Link request payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct LinkRequest {
    pub old_path: Vec<u8>,
    pub new_path: Vec<u8>,
}

/// Link response payload (empty on success)
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct LinkResponse {}

/// Linkat request payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct LinkatRequest {
    pub old_dirfd: u32,
    pub old_path: Vec<u8>,
    pub new_dirfd: u32,
    pub new_path: Vec<u8>,
    pub flags: u32,
}

/// Linkat response payload (empty on success)
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct LinkatResponse {}

/// Symlink request payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct SymlinkRequest {
    pub target: Vec<u8>,
    pub linkpath: Vec<u8>,
}

/// Symlink response payload (empty on success)
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct SymlinkResponse {}

/// Symlinkat request payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct SymlinkatRequest {
    pub target: Vec<u8>,
    pub new_dirfd: u32,
    pub linkpath: Vec<u8>,
}

/// Symlinkat response payload (empty on success)
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct SymlinkatResponse {}

/// Unlink request payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct UnlinkRequest {
    pub path: Vec<u8>,
}

/// Unlink response payload (empty on success)
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct UnlinkResponse {}

/// Unlinkat request payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct UnlinkatRequest {
    pub dirfd: u32,
    pub path: Vec<u8>,
    pub flags: u32,
}

/// Unlinkat response payload (empty on success)
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct UnlinkatResponse {}

/// Remove request payload (alias for unlink)
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct RemoveRequest {
    pub path: Vec<u8>,
}

/// Remove response payload (empty on success)
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct RemoveResponse {}

/// Mkdir request payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct MkdirRequest {
    pub path: Vec<u8>,
    pub mode: u32,
}

/// Mkdir response payload (empty on success)
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct MkdirResponse {}

/// Mkdirat request payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct MkdiratRequest {
    pub dirfd: u32,
    pub path: Vec<u8>,
    pub mode: u32,
}

/// Mkdirat response payload (empty on success)
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct MkdiratResponse {}

/// Directory entry information
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct DirEntry {
    pub name: Vec<u8>,
    pub kind: u8, // 0=file, 1=directory, 2=symlink
}

/// Empty struct for queries without parameters
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct EmptyQuery {
    pub dummy: u8,
}

/// Wrapper for daemon state processes request
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct DaemonStateProcessesRequest {
    pub data: Vec<u8>,
}

/// Wrapper for daemon state stats request
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct DaemonStateStatsRequest {
    pub data: Vec<u8>,
}

/// Wrapper for daemon state filesystem request
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct DaemonStateFilesystemRequest {
    pub query: FilesystemQuery,
}

/// Daemon state query types - using struct with discriminant
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct DaemonStateQuery {
    pub discriminant: u32, // 0=Processes, 1=Stats, 2=FilesystemState
    pub filesystem_params: Option<FilesystemQuery>,
}

/// Filesystem state query parameters
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FilesystemQuery {
    pub max_depth: u32,
    pub include_overlay: u32, // 0 = false, 1 = true
    pub max_file_size: u32,   // max bytes to include in content
}

/// Daemon state response types - using enum for SSZ union
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
#[ssz(enum_behaviour = "union")]
pub enum DaemonStateResponse {
    Processes(Vec<ProcessInfo>),
    Stats(FsStats),
    FilesystemState(FilesystemState),
}

/// Process information for daemon state
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct ProcessInfo {
    pub os_pid: u32,
    pub registered_pid: Vec<u8>, // String representation
}

/// Filesystem statistics
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FsStats {
    pub branches: u32,
    pub snapshots: u32,
    pub open_handles: u32,
    pub memory_usage: u64, // bytes_in_memory
}

/// Complete filesystem state as sorted flattened list
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FilesystemState {
    pub entries: Vec<FilesystemEntry>, // sorted by path for binary search
}

/// Individual filesystem entry in flattened list
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FilesystemEntry {
    pub path: Vec<u8>, // full path as UTF-8 bytes
    pub kind: FileKind,
    pub size: u64,                // file size, 0 for directories/symlinks
    pub content: Option<Vec<u8>>, // file content if small enough
    pub target: Option<Vec<u8>>,  // symlink target as UTF-8 bytes
}

/// File type enumeration - using discriminant
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FileKind {
    pub discriminant: u32, // 0=File, 1=Directory, 2=Symlink
}

/// Daemon state request for testing (query daemon internal state)
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct DaemonStateRequest {
    pub query: DaemonStateQuery,
}

/// Daemon state response with internal daemon information
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct DaemonStateResponseWrapper {
    pub response: DaemonStateResponse,
}

/// PathOp request payload for path-based operations
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct PathOpRequest {
    pub path: Vec<u8>,
    pub operation: Vec<u8>,    // "stat", "lstat", "chmod", etc.
    pub args: Option<Vec<u8>>, // operation-specific arguments
}

/// PathOp response payload
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct PathOpResponse {
    pub result: Option<Vec<u8>>, // operation-specific result
}

/// InterposeSetGet request payload for configuration management
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct InterposeSetGetRequest {
    pub key: Vec<u8>,           // "max_copy_bytes", "require_reflink", etc.
    pub value: Option<Vec<u8>>, // None for get, Some(value) for set
}

/// InterposeSetGet response payload with configuration value
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct InterposeSetGetResponse {
    pub value: Vec<u8>, // current/updated configuration value
}

/// Filesystem operation request union
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
#[ssz(enum_behaviour = "union")]
pub enum FsRequest {
    Open(FsOpenRequest),
    Create(FsCreateRequest),
    Close(FsCloseRequest),
    Read(FsReadRequest),
    Write(FsWriteRequest),
    GetAttr(FsGetAttrRequest),
    Mkdir(FsMkdirRequest),
    Unlink(FsUnlinkRequest),
    ReadDir(FsReadDirRequest),
}

/// Filesystem operation response union
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
#[ssz(enum_behaviour = "union")]
pub enum FsResponse {
    Handle(FsHandleResponse),
    Data(FsDataResponse),
    Written(FsWrittenResponse),
    Attrs(FsAttrsResponse),
    Entries(FsEntriesResponse),
    Ok(FsOkResponse),
    Error(FsErrorResponse),
}

/// Open file request
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FsOpenRequest {
    pub path: Vec<u8>,
    pub read: bool,
    pub write: bool,
}

/// Create file request
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FsCreateRequest {
    pub path: Vec<u8>,
    pub read: bool,
    pub write: bool,
}

/// Close file request
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FsCloseRequest {
    pub handle: u64,
}

/// Read file request
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FsReadRequest {
    pub handle: u64,
    pub offset: u64,
    pub len: usize,
}

/// Write file request
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FsWriteRequest {
    pub handle: u64,
    pub offset: u64,
    pub data: Vec<u8>,
}

/// Get attributes request
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FsGetAttrRequest {
    pub path: Vec<u8>,
}

/// Make directory request
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FsMkdirRequest {
    pub path: Vec<u8>,
}

/// Unlink file request
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FsUnlinkRequest {
    pub path: Vec<u8>,
}

/// Read directory request
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FsReadDirRequest {
    pub path: Vec<u8>,
}

/// Handle response
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FsHandleResponse {
    pub handle: u64,
}

/// Data response
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FsDataResponse {
    pub data: Vec<u8>,
}

/// Written response
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FsWrittenResponse {
    pub len: usize,
}

/// Attributes response
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FsAttrsResponse {
    pub len: u64,
    pub is_dir: bool,
    pub is_symlink: bool,
}

/// Directory entry
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FsDirEntry {
    pub name: Vec<u8>,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub len: u64,
}

/// Directory entries response
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FsEntriesResponse {
    pub entries: Vec<FsDirEntry>,
}

/// OK response
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FsOkResponse {}

/// Error response
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct FsErrorResponse {
    pub error: Vec<u8>,
    pub code: Option<u32>,
}

// Constructors for SSZ union variants (convert String to Vec<u8>)
impl Request {
    pub fn snapshot_create(name: Option<String>) -> Self {
        Self::SnapshotCreate((
            b"1".to_vec(),
            SnapshotCreateRequest {
                name: name.map(|s| s.into_bytes()),
            },
        ))
    }

    pub fn snapshot_list() -> Self {
        Self::SnapshotList(b"1".to_vec())
    }

    pub fn branch_create(from: String, name: Option<String>) -> Self {
        Self::BranchCreate((
            b"1".to_vec(),
            BranchCreateRequest {
                from: from.into_bytes(),
                name: name.map(|s| s.into_bytes()),
            },
        ))
    }

    pub fn branch_bind(branch: String, pid: Option<u32>) -> Self {
        Self::BranchBind((
            b"1".to_vec(),
            BranchBindRequest {
                branch: branch.into_bytes(),
                pid,
            },
        ))
    }

    pub fn fd_open(path: String, flags: u32, mode: u32) -> Self {
        Self::FdOpen((
            b"1".to_vec(),
            FdOpenRequest {
                path: path.into_bytes(),
                flags,
                mode,
            },
        ))
    }

    pub fn fd_dup(fd: u32) -> Self {
        Self::FdDup((b"1".to_vec(), FdDupRequest { fd }))
    }

    pub fn dir_open(path: String) -> Self {
        Self::DirOpen((
            b"1".to_vec(),
            DirOpenRequest {
                path: path.into_bytes(),
            },
        ))
    }

    pub fn dir_read(handle: u64) -> Self {
        Self::DirRead((b"1".to_vec(), DirReadRequest { handle }))
    }

    pub fn dir_close(handle: u64) -> Self {
        Self::DirClose((b"1".to_vec(), DirCloseRequest { handle }))
    }

    pub fn readlink(path: String) -> Self {
        Self::Readlink((
            b"1".to_vec(),
            ReadlinkRequest {
                path: path.into_bytes(),
            },
        ))
    }

    pub fn stat(path: String) -> Self {
        Self::Stat((
            b"1".to_vec(),
            StatRequest {
                path: path.into_bytes(),
            },
        ))
    }

    pub fn lstat(path: String) -> Self {
        Self::Lstat((
            b"1".to_vec(),
            LstatRequest {
                path: path.into_bytes(),
            },
        ))
    }

    pub fn fstat(fd: u32) -> Self {
        Self::Fstat((b"1".to_vec(), FstatRequest { fd }))
    }

    pub fn fstatat(dirfd: u32, path: String, flags: u32) -> Self {
        Self::Fstatat((
            b"1".to_vec(),
            FstatatRequest {
                dirfd,
                path: path.into_bytes(),
                flags,
            },
        ))
    }

    pub fn chmod(path: String, mode: u32) -> Self {
        Self::Chmod((
            b"1".to_vec(),
            ChmodRequest {
                path: path.into_bytes(),
                mode,
            },
        ))
    }

    pub fn fchmod(fd: u32, mode: u32) -> Self {
        Self::Fchmod((b"1".to_vec(), FchmodRequest { fd, mode }))
    }

    pub fn fchmodat(dirfd: u32, path: String, mode: u32, flags: u32) -> Self {
        Self::Fchmodat((
            b"1".to_vec(),
            FchmodatRequest {
                dirfd,
                path: path.into_bytes(),
                mode,
                flags,
            },
        ))
    }

    pub fn chown(path: String, uid: u32, gid: u32) -> Self {
        Self::Chown((
            b"1".to_vec(),
            ChownRequest {
                path: path.into_bytes(),
                uid,
                gid,
            },
        ))
    }

    pub fn lchown(path: String, uid: u32, gid: u32) -> Self {
        Self::Lchown((
            b"1".to_vec(),
            LchownRequest {
                path: path.into_bytes(),
                uid,
                gid,
            },
        ))
    }

    pub fn fchown(fd: u32, uid: u32, gid: u32) -> Self {
        Self::Fchown((b"1".to_vec(), FchownRequest { fd, uid, gid }))
    }

    pub fn fchownat(dirfd: u32, path: String, uid: u32, gid: u32, flags: u32) -> Self {
        Self::Fchownat((
            b"1".to_vec(),
            FchownatRequest {
                dirfd,
                path: path.into_bytes(),
                uid,
                gid,
                flags,
            },
        ))
    }

    pub fn utimes(path: String, times: Option<(TimespecData, TimespecData)>) -> Self {
        Self::Utimes((
            b"1".to_vec(),
            UtimesRequest {
                path: path.into_bytes(),
                times,
            },
        ))
    }

    pub fn futimes(fd: u32, times: Option<(TimespecData, TimespecData)>) -> Self {
        Self::Futimes((b"1".to_vec(), FutimesRequest { fd, times }))
    }

    pub fn utimensat(
        dirfd: u32,
        path: String,
        times: Option<(TimespecData, TimespecData)>,
        flags: u32,
    ) -> Self {
        Self::Utimensat((
            b"1".to_vec(),
            UtimensatRequest {
                dirfd,
                path: path.into_bytes(),
                times,
                flags,
            },
        ))
    }

    pub fn futimens(fd: u32, times: Option<(TimespecData, TimespecData)>) -> Self {
        Self::Futimens((b"1".to_vec(), FutimensRequest { fd, times }))
    }

    pub fn truncate(path: String, length: u64) -> Self {
        Self::Truncate((
            b"1".to_vec(),
            TruncateRequest {
                path: path.into_bytes(),
                length,
            },
        ))
    }

    pub fn ftruncate(fd: u32, length: u64) -> Self {
        Self::Ftruncate((b"1".to_vec(), FtruncateRequest { fd, length }))
    }

    pub fn statfs(path: String) -> Self {
        Self::Statfs((
            b"1".to_vec(),
            StatfsRequest {
                path: path.into_bytes(),
            },
        ))
    }

    pub fn fstatfs(fd: u32) -> Self {
        Self::Fstatfs((b"1".to_vec(), FstatfsRequest { fd }))
    }

    pub fn daemon_state_processes() -> Self {
        Self::DaemonStateProcesses(DaemonStateProcessesRequest {
            data: b"1".to_vec(),
        })
    }

    pub fn daemon_state_stats() -> Self {
        Self::DaemonStateStats(DaemonStateStatsRequest {
            data: b"1".to_vec(),
        })
    }

    pub fn daemon_state_filesystem(
        max_depth: u32,
        include_overlay: bool,
        max_file_size: u32,
    ) -> Self {
        Self::DaemonStateFilesystem(DaemonStateFilesystemRequest {
            query: FilesystemQuery {
                max_depth,
                include_overlay: if include_overlay { 1 } else { 0 },
                max_file_size,
            },
        })
    }

    pub fn path_op(path: String, operation: String, args: Option<String>) -> Self {
        Self::PathOp((
            b"1".to_vec(),
            PathOpRequest {
                path: path.into_bytes(),
                operation: operation.into_bytes(),
                args: args.map(|s| s.into_bytes()),
            },
        ))
    }

    pub fn interpose_setget(key: String, value: Option<String>) -> Self {
        Self::InterposeSetGet((
            b"1".to_vec(),
            InterposeSetGetRequest {
                key: key.into_bytes(),
                value: value.map(|s| s.into_bytes()),
            },
        ))
    }

    pub fn rename(old_path: String, new_path: String) -> Self {
        Self::Rename((
            b"1".to_vec(),
            RenameRequest {
                old_path: old_path.into_bytes(),
                new_path: new_path.into_bytes(),
            },
        ))
    }

    pub fn renameat(old_dirfd: u32, old_path: String, new_dirfd: u32, new_path: String) -> Self {
        Self::Renameat((
            b"1".to_vec(),
            RenameatRequest {
                old_dirfd,
                old_path: old_path.into_bytes(),
                new_dirfd,
                new_path: new_path.into_bytes(),
            },
        ))
    }

    pub fn renameatx_np(
        old_dirfd: u32,
        old_path: String,
        new_dirfd: u32,
        new_path: String,
        flags: u32,
    ) -> Self {
        Self::RenameatxNp((
            b"1".to_vec(),
            RenameatxNpRequest {
                old_dirfd,
                old_path: old_path.into_bytes(),
                new_dirfd,
                new_path: new_path.into_bytes(),
                flags,
            },
        ))
    }

    pub fn link(old_path: String, new_path: String) -> Self {
        Self::Link((
            b"1".to_vec(),
            LinkRequest {
                old_path: old_path.into_bytes(),
                new_path: new_path.into_bytes(),
            },
        ))
    }

    pub fn linkat(
        old_dirfd: u32,
        old_path: String,
        new_dirfd: u32,
        new_path: String,
        flags: u32,
    ) -> Self {
        Self::Linkat((
            b"1".to_vec(),
            LinkatRequest {
                old_dirfd,
                old_path: old_path.into_bytes(),
                new_dirfd,
                new_path: new_path.into_bytes(),
                flags,
            },
        ))
    }

    pub fn symlink(target: String, linkpath: String) -> Self {
        Self::Symlink((
            b"1".to_vec(),
            SymlinkRequest {
                target: target.into_bytes(),
                linkpath: linkpath.into_bytes(),
            },
        ))
    }

    pub fn symlinkat(target: String, new_dirfd: u32, linkpath: String) -> Self {
        Self::Symlinkat((
            b"1".to_vec(),
            SymlinkatRequest {
                target: target.into_bytes(),
                new_dirfd,
                linkpath: linkpath.into_bytes(),
            },
        ))
    }

    pub fn unlink(path: String) -> Self {
        Self::Unlink((
            b"1".to_vec(),
            UnlinkRequest {
                path: path.into_bytes(),
            },
        ))
    }

    pub fn unlinkat(dirfd: u32, path: String, flags: u32) -> Self {
        Self::Unlinkat((
            b"1".to_vec(),
            UnlinkatRequest {
                dirfd,
                path: path.into_bytes(),
                flags,
            },
        ))
    }

    pub fn remove(path: String) -> Self {
        Self::Remove((
            b"1".to_vec(),
            RemoveRequest {
                path: path.into_bytes(),
            },
        ))
    }

    pub fn mkdir(path: String, mode: u32) -> Self {
        Self::Mkdir((
            b"1".to_vec(),
            MkdirRequest {
                path: path.into_bytes(),
                mode,
            },
        ))
    }

    pub fn mkdirat(dirfd: u32, path: String, mode: u32) -> Self {
        Self::Mkdirat((
            b"1".to_vec(),
            MkdiratRequest {
                dirfd,
                path: path.into_bytes(),
                mode,
            },
        ))
    }
}

impl Response {
    pub fn snapshot_create(snapshot: SnapshotInfo) -> Self {
        Self::SnapshotCreate(SnapshotCreateResponse { snapshot })
    }

    pub fn snapshot_list(snapshots: Vec<SnapshotInfo>) -> Self {
        Self::SnapshotList(SnapshotListResponse { snapshots })
    }

    pub fn branch_create(branch: BranchInfo) -> Self {
        Self::BranchCreate(BranchCreateResponse { branch })
    }

    pub fn branch_bind(branch: Vec<u8>, pid: u32) -> Self {
        Self::BranchBind(BranchBindResponse { branch, pid })
    }

    pub fn fd_open(fd: u32) -> Self {
        Self::FdOpen(FdOpenResponse { fd })
    }

    pub fn fd_dup(fd: u32) -> Self {
        Self::FdDup(FdDupResponse { fd })
    }

    pub fn dir_open(handle: u64) -> Self {
        Self::DirOpen(DirOpenResponse { handle })
    }

    pub fn dir_read(entries: Vec<DirEntry>) -> Self {
        Self::DirRead(DirReadResponse { entries })
    }

    pub fn dir_close() -> Self {
        Self::DirClose(DirCloseResponse {})
    }

    pub fn readlink(target: String) -> Self {
        Self::Readlink(ReadlinkResponse {
            target: target.into_bytes(),
        })
    }

    pub fn stat(stat: StatData) -> Self {
        Self::Stat(StatResponse { stat })
    }

    pub fn lstat(stat: StatData) -> Self {
        Self::Lstat(LstatResponse { stat })
    }

    pub fn fstat(stat: StatData) -> Self {
        Self::Fstat(FstatResponse { stat })
    }

    pub fn fstatat(stat: StatData) -> Self {
        Self::Fstatat(FstatatResponse { stat })
    }

    pub fn chmod() -> Self {
        Self::Chmod(ChmodResponse {})
    }

    pub fn fchmod() -> Self {
        Self::Fchmod(FchmodResponse {})
    }

    pub fn fchmodat() -> Self {
        Self::Fchmodat(FchmodatResponse {})
    }

    pub fn chown() -> Self {
        Self::Chown(ChownResponse {})
    }

    pub fn lchown() -> Self {
        Self::Lchown(LchownResponse {})
    }

    pub fn fchown() -> Self {
        Self::Fchown(FchownResponse {})
    }

    pub fn fchownat() -> Self {
        Self::Fchownat(FchownatResponse {})
    }

    pub fn utimes() -> Self {
        Self::Utimes(UtimesResponse {})
    }

    pub fn futimes() -> Self {
        Self::Futimes(FutimesResponse {})
    }

    pub fn utimensat() -> Self {
        Self::Utimensat(UtimensatResponse {})
    }

    pub fn futimens() -> Self {
        Self::Futimens(FutimensResponse {})
    }

    pub fn truncate() -> Self {
        Self::Truncate(TruncateResponse {})
    }

    pub fn ftruncate() -> Self {
        Self::Ftruncate(FtruncateResponse {})
    }

    pub fn statfs(statfs: StatfsData) -> Self {
        Self::Statfs(StatfsResponse { statfs })
    }

    pub fn fstatfs(statfs: StatfsData) -> Self {
        Self::Fstatfs(FstatfsResponse { statfs })
    }

    pub fn daemon_state_processes(processes: Vec<ProcessInfo>) -> Self {
        Self::DaemonState(DaemonStateResponseWrapper {
            response: DaemonStateResponse::Processes(processes),
        })
    }

    pub fn daemon_state_stats(stats: FsStats) -> Self {
        Self::DaemonState(DaemonStateResponseWrapper {
            response: DaemonStateResponse::Stats(stats),
        })
    }

    pub fn daemon_state_filesystem(state: FilesystemState) -> Self {
        Self::DaemonState(DaemonStateResponseWrapper {
            response: DaemonStateResponse::FilesystemState(state),
        })
    }

    pub fn path_op(result: Option<String>) -> Self {
        Self::PathOp(PathOpResponse {
            result: result.map(|s| s.into_bytes()),
        })
    }

    pub fn interpose_setget(value: String) -> Self {
        Self::InterposeSetGet(InterposeSetGetResponse {
            value: value.into_bytes(),
        })
    }

    pub fn rename() -> Self {
        Self::Rename(RenameResponse {})
    }

    pub fn renameat() -> Self {
        Self::Renameat(RenameatResponse {})
    }

    pub fn renameatx_np() -> Self {
        Self::RenameatxNp(RenameatxNpResponse {})
    }

    pub fn link() -> Self {
        Self::Link(LinkResponse {})
    }

    pub fn linkat() -> Self {
        Self::Linkat(LinkatResponse {})
    }

    pub fn symlink() -> Self {
        Self::Symlink(SymlinkResponse {})
    }

    pub fn symlinkat() -> Self {
        Self::Symlinkat(SymlinkatResponse {})
    }

    pub fn unlink() -> Self {
        Self::Unlink(UnlinkResponse {})
    }

    pub fn unlinkat() -> Self {
        Self::Unlinkat(UnlinkatResponse {})
    }

    pub fn remove() -> Self {
        Self::Remove(RemoveResponse {})
    }

    pub fn mkdir() -> Self {
        Self::Mkdir(MkdirResponse {})
    }

    pub fn mkdirat() -> Self {
        Self::Mkdirat(MkdiratResponse {})
    }

    pub fn error(message: String, code: Option<u32>) -> Self {
        Self::Error(ErrorResponse {
            error: message.into_bytes(),
            code,
        })
    }
}

// Constructors for filesystem operation SSZ union variants
impl FsRequest {
    pub fn open(path: String, read: bool, write: bool) -> Self {
        Self::Open(FsOpenRequest {
            path: path.into_bytes(),
            read,
            write,
        })
    }

    pub fn create(path: String, read: bool, write: bool) -> Self {
        Self::Create(FsCreateRequest {
            path: path.into_bytes(),
            read,
            write,
        })
    }

    pub fn close(handle: u64) -> Self {
        Self::Close(FsCloseRequest { handle })
    }

    pub fn read(handle: u64, offset: u64, len: usize) -> Self {
        Self::Read(FsReadRequest {
            handle,
            offset,
            len,
        })
    }

    pub fn write(handle: u64, offset: u64, data: Vec<u8>) -> Self {
        Self::Write(FsWriteRequest {
            handle,
            offset,
            data,
        })
    }

    pub fn getattr(path: String) -> Self {
        Self::GetAttr(FsGetAttrRequest {
            path: path.into_bytes(),
        })
    }

    pub fn mkdir(path: String) -> Self {
        Self::Mkdir(FsMkdirRequest {
            path: path.into_bytes(),
        })
    }

    pub fn unlink(path: String) -> Self {
        Self::Unlink(FsUnlinkRequest {
            path: path.into_bytes(),
        })
    }

    pub fn readdir(path: String) -> Self {
        Self::ReadDir(FsReadDirRequest {
            path: path.into_bytes(),
        })
    }
}

impl FsResponse {
    pub fn handle(handle: u64) -> Self {
        Self::Handle(FsHandleResponse { handle })
    }

    pub fn data(data: Vec<u8>) -> Self {
        Self::Data(FsDataResponse { data })
    }

    pub fn written(len: usize) -> Self {
        Self::Written(FsWrittenResponse { len })
    }

    pub fn attrs(len: u64, is_dir: bool, is_symlink: bool) -> Self {
        Self::Attrs(FsAttrsResponse {
            len,
            is_dir,
            is_symlink,
        })
    }

    pub fn entries(entries: Vec<FsDirEntry>) -> Self {
        Self::Entries(FsEntriesResponse { entries })
    }

    pub fn ok() -> Self {
        Self::Ok(FsOkResponse {})
    }

    pub fn error(message: String, code: Option<u32>) -> Self {
        Self::Error(FsErrorResponse {
            error: message.into_bytes(),
            code,
        })
    }
}

// Helper constructors for directory entries
impl FsDirEntry {
    pub fn new(name: String, is_dir: bool, is_symlink: bool, len: u64) -> Self {
        Self {
            name: name.into_bytes(),
            is_dir,
            is_symlink,
            len,
        }
    }
}

// Helper constructors for directory entries (protocol level)
impl DirEntry {
    pub fn new(name: String, kind: u8) -> Self {
        Self {
            name: name.into_bytes(),
            kind,
        }
    }

    pub fn file(name: String) -> Self {
        Self::new(name, 0)
    }

    pub fn directory(name: String) -> Self {
        Self::new(name, 1)
    }

    pub fn symlink(name: String) -> Self {
        Self::new(name, 2)
    }
}
