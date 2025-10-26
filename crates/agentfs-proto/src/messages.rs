// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Control plane message types for AgentFS

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
