// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! AgentFS Protocol — Control plane types and validation
//!
//! This crate defines the SSZ schemas and request/response types
//! for the AgentFS control plane, used by CLI tools and adapters.

pub mod messages;
pub mod validation;

// Re-export key types
pub use messages::{
    BranchBindRequest,
    BranchBindResponse,
    BranchCreateRequest,
    BranchCreateResponse,
    BranchInfo,
    // Daemon state types
    DaemonStateQuery,
    DaemonStateRequest,
    DaemonStateResponse,
    DaemonStateResponseWrapper,
    EmptyQuery,
    ErrorResponse,
    FileKind,
    FilesystemEntry,
    FilesystemQuery,
    FilesystemState,
    FsAttrsResponse,
    FsCloseRequest,
    FsCreateRequest,
    FsDataResponse,
    FsDirEntry,
    FsEntriesResponse,
    FsErrorResponse,
    FsGetAttrRequest,
    FsHandleResponse,
    FsMkdirRequest,
    FsOkResponse,
    FsOpenRequest,
    FsReadDirRequest,
    FsReadRequest,
    // Filesystem operation types
    FsRequest,
    FsResponse,
    FsStats,
    FsUnlinkRequest,
    FsWriteRequest,
    FsWrittenResponse,
    ProcessInfo,
    Request,
    Response,
    SnapshotCreateRequest,
    SnapshotCreateResponse,
    SnapshotInfo,
    SnapshotListRequest,
    SnapshotListResponse,
};
pub use validation::*;
