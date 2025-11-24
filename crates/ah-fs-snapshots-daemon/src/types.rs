// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use ssz_derive::{Decode, Encode};

// SSZ Union-based request/response types for type-safe daemon communication
// Using Vec<u8> for strings as SSZ supports variable-length byte vectors

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
#[ssz(enum_behaviour = "union")]
pub enum Request {
    Ping(Vec<u8>),                     // empty vec for ping
    ListZfsSnapshots(Vec<u8>),         // dataset
    CloneZfs((Vec<u8>, Vec<u8>)),      // (snapshot, clone)
    SnapshotZfs((Vec<u8>, Vec<u8>)),   // (source, snapshot)
    DeleteZfs(Vec<u8>),                // target
    CloneBtrfs((Vec<u8>, Vec<u8>)),    // (source, destination)
    SnapshotBtrfs((Vec<u8>, Vec<u8>)), // (source, destination)
    DeleteBtrfs(Vec<u8>),              // target
    MountAgentfsFuse(AgentfsFuseMountRequest),
    UnmountAgentfsFuse(Vec<u8>),
    StatusAgentfsFuse(Vec<u8>),
    MountAgentfsInterpose(AgentfsInterposeMountRequest),
    UnmountAgentfsInterpose(Vec<u8>),
    StatusAgentfsInterpose(Vec<u8>),
    MountAgentfsInterposeWithHints((AgentfsInterposeMountRequest, AgentfsInterposeMountHints)),
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
#[ssz(enum_behaviour = "union")]
pub enum Response {
    Success(Vec<u8>),               // empty vec for success
    SuccessWithMountpoint(Vec<u8>), // mountpoint
    SuccessWithPath(Vec<u8>),       // path
    SuccessWithList(Vec<u8>),       // JSON-encoded list
    Error(Vec<u8>),                 // message
    AgentfsFuseStatus(AgentfsFuseStatusData),
    AgentfsInterposeStatus(AgentfsInterposeStatusData),
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct AgentfsFuseMountRequest {
    pub mount_point: Vec<u8>,
    pub uid: u32,
    pub gid: u32,
    pub allow_other: bool,
    pub allow_root: bool,
    pub auto_unmount: bool,
    pub writeback_cache: bool,
    pub mount_timeout_ms: u32,
    pub backstore: AgentfsFuseBackstore,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
#[ssz(enum_behaviour = "union")]
pub enum AgentfsFuseBackstore {
    InMemory(Vec<u8>),
    HostFs(AgentfsHostFsBackstore),
    RamDisk(AgentfsRamDiskBackstore),
}

impl Default for AgentfsFuseBackstore {
    fn default() -> Self {
        Self::InMemory(vec![])
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct AgentfsHostFsBackstore {
    pub root: Vec<u8>,
    pub prefer_native_snapshots: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct AgentfsRamDiskBackstore {
    pub size_mb: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct AgentfsFuseStatusData {
    pub state: u8,
    pub mount_point: Vec<u8>,
    pub pid: u64,
    pub restart_count: u32,
    pub log_path: Vec<u8>,
    pub runtime_dir: Vec<u8>,
    pub last_error: Vec<u8>,
    pub backstore: AgentfsFuseBackstore,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct AgentfsInterposeMountRequest {
    pub repo_root: Vec<u8>,
    pub uid: u32,
    pub gid: u32,
    pub mount_timeout_ms: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, Default)]
pub struct AgentfsInterposeMountHints {
    pub socket_path: Vec<u8>,
    pub runtime_dir: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct AgentfsInterposeStatusData {
    pub state: u8,
    pub socket_path: Vec<u8>,
    pub pid: u64,
    pub restart_count: u32,
    pub log_path: Vec<u8>,
    pub runtime_dir: Vec<u8>,
    pub last_error: Vec<u8>,
    pub repo_root: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentfsFuseState {
    Unknown = 0,
    Starting = 1,
    Running = 2,
    BackingOff = 3,
    Unmounted = 4,
    Failed = 5,
}

impl AgentfsFuseState {
    pub fn as_code(self) -> u8 {
        self as u8
    }

    pub fn from_code(code: u8) -> Self {
        match code {
            1 => AgentfsFuseState::Starting,
            2 => AgentfsFuseState::Running,
            3 => AgentfsFuseState::BackingOff,
            4 => AgentfsFuseState::Unmounted,
            5 => AgentfsFuseState::Failed,
            _ => AgentfsFuseState::Unknown,
        }
    }
}

// Constructors for SSZ union variants (convert String to Vec<u8>)
#[allow(dead_code)]
impl Request {
    pub fn ping() -> Self {
        Self::Ping(vec![])
    }

    pub fn list_zfs_snapshots(dataset: String) -> Self {
        Self::ListZfsSnapshots(dataset.into_bytes())
    }

    pub fn clone_zfs(snapshot: String, clone: String) -> Self {
        Self::CloneZfs((snapshot.into_bytes(), clone.into_bytes()))
    }

    pub fn snapshot_zfs(source: String, snapshot: String) -> Self {
        Self::SnapshotZfs((source.into_bytes(), snapshot.into_bytes()))
    }

    pub fn delete_zfs(target: String) -> Self {
        Self::DeleteZfs(target.into_bytes())
    }

    pub fn clone_btrfs(source: String, destination: String) -> Self {
        Self::CloneBtrfs((source.into_bytes(), destination.into_bytes()))
    }

    pub fn snapshot_btrfs(source: String, destination: String) -> Self {
        Self::SnapshotBtrfs((source.into_bytes(), destination.into_bytes()))
    }

    pub fn delete_btrfs(target: String) -> Self {
        Self::DeleteBtrfs(target.into_bytes())
    }

    pub fn mount_agentfs_fuse(request: AgentfsFuseMountRequest) -> Self {
        Self::MountAgentfsFuse(request)
    }

    pub fn unmount_agentfs_fuse() -> Self {
        Self::UnmountAgentfsFuse(vec![])
    }

    pub fn status_agentfs_fuse() -> Self {
        Self::StatusAgentfsFuse(vec![])
    }

    pub fn mount_agentfs_interpose(request: AgentfsInterposeMountRequest) -> Self {
        Self::MountAgentfsInterpose(request)
    }

    pub fn mount_agentfs_interpose_with_hints(
        request: AgentfsInterposeMountRequest,
        hints: AgentfsInterposeMountHints,
    ) -> Self {
        Self::MountAgentfsInterposeWithHints((request, hints))
    }

    pub fn unmount_agentfs_interpose() -> Self {
        Self::UnmountAgentfsInterpose(vec![])
    }

    pub fn status_agentfs_interpose() -> Self {
        Self::StatusAgentfsInterpose(vec![])
    }
}

impl Response {
    pub fn success() -> Self {
        Self::Success(vec![])
    }

    pub fn success_with_mountpoint(mountpoint: String) -> Self {
        Self::SuccessWithMountpoint(mountpoint.into_bytes())
    }

    pub fn success_with_path(path: String) -> Self {
        Self::SuccessWithPath(path.into_bytes())
    }

    pub fn success_with_list(list: String) -> Self {
        Self::SuccessWithList(list.into_bytes())
    }

    pub fn error(message: String) -> Self {
        Self::Error(message.into_bytes())
    }

    pub fn agentfs_fuse_status(status: AgentfsFuseStatusData) -> Self {
        Self::AgentfsFuseStatus(status)
    }

    pub fn agentfs_interpose_status(status: AgentfsInterposeStatusData) -> Self {
        Self::AgentfsInterposeStatus(status)
    }
}
