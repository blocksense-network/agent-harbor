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
    PathOp((Vec<u8>, PathOpRequest)),                 // (version, request)
    InterposeSetGet((Vec<u8>, InterposeSetGetRequest)), // (version, request)
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
    PathOp(PathOpResponse),
    InterposeSetGet(InterposeSetGetResponse),
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
