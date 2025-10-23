// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Core type definitions for AgentFS

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Opaque snapshot identifier (ULID-like)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SnapshotId(pub [u8; 16]);

impl std::fmt::Display for SnapshotId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Convert to base32hex string (ULID format)
        let mut s = String::with_capacity(26);
        for &byte in &self.0 {
            s.push_str(&format!("{:02x}", byte));
        }
        f.write_str(&s)
    }
}

impl std::str::FromStr for SnapshotId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != 32 {
            return Err("Invalid length".to_string());
        }

        let mut bytes = [0u8; 16];
        for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
            if chunk.len() != 2 {
                return Err("Invalid hex".to_string());
            }
            bytes[i] =
                u8::from_str_radix(std::str::from_utf8(chunk).map_err(|_| "Invalid UTF-8")?, 16)
                    .map_err(|_| "Invalid hex digit")?;
        }

        Ok(SnapshotId(bytes))
    }
}

impl SnapshotId {
    pub fn new() -> Self {
        Self(Self::generate_ulid())
    }

    fn generate_ulid() -> [u8; 16] {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;

        // Simple ULID-like generation: timestamp + random bytes
        let mut bytes = [0u8; 16];
        bytes[0..8].copy_from_slice(&now.to_be_bytes());
        // For simplicity, use a counter for the remaining bytes
        // In production, this should use proper randomness
        static mut COUNTER: u64 = 0;
        unsafe {
            COUNTER += 1;
            bytes[8..16].copy_from_slice(&COUNTER.to_be_bytes());
        }
        bytes
    }
}

impl Default for SnapshotId {
    fn default() -> Self {
        Self::new()
    }
}

/// Opaque branch identifier
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BranchId(pub [u8; 16]);

impl std::fmt::Display for BranchId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Convert to base32hex string (ULID format)
        let mut s = String::with_capacity(26);
        for &byte in &self.0 {
            s.push_str(&format!("{:02x}", byte));
        }
        f.write_str(&s)
    }
}

impl std::str::FromStr for BranchId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != 32 {
            return Err("Invalid length".to_string());
        }

        let mut bytes = [0u8; 16];
        for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
            if chunk.len() != 2 {
                return Err("Invalid hex".to_string());
            }
            bytes[i] =
                u8::from_str_radix(std::str::from_utf8(chunk).map_err(|_| "Invalid UTF-8")?, 16)
                    .map_err(|_| "Invalid hex digit")?;
        }

        Ok(BranchId(bytes))
    }
}

impl BranchId {
    pub fn new() -> Self {
        Self(Self::generate_ulid())
    }

    fn generate_ulid() -> [u8; 16] {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;

        // Simple ULID-like generation: timestamp + random bytes
        let mut bytes = [0u8; 16];
        bytes[0..8].copy_from_slice(&now.to_be_bytes());
        // For simplicity, use a counter for the remaining bytes
        // In production, this should use proper randomness
        static mut COUNTER: u64 = 0;
        unsafe {
            COUNTER += 1;
            bytes[8..16].copy_from_slice(&COUNTER.to_be_bytes());
        }
        bytes
    }

    /// Special default branch ID for the initial branch
    pub const DEFAULT: BranchId = BranchId([0u8; 16]);
}

impl Default for BranchId {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Branch information
#[derive(Clone, Debug)]
pub struct BranchInfo {
    pub id: BranchId,
    pub parent: Option<SnapshotId>,
    pub name: Option<String>,
}

/// Opaque handle identifier
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HandleId(pub u64);

impl HandleId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }
}

/// File timestamps
#[derive(Clone, Copy, Debug)]
pub struct FileTimes {
    pub atime: i64,
    pub mtime: i64,
    pub ctime: i64,
    pub birthtime: i64,
}

/// File permissions
#[derive(Clone, Debug, PartialEq)]
pub struct FileMode {
    pub read: bool,
    pub write: bool,
    pub exec: bool,
}

/// File attributes
#[derive(Clone, Debug)]
pub struct Attributes {
    pub len: u64,
    pub times: FileTimes,
    pub uid: u32,
    pub gid: u32,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub mode_user: FileMode,
    pub mode_group: FileMode,
    pub mode_other: FileMode,
}

/// Directory entry information
#[derive(Clone, Debug)]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub len: u64,
}

/// Extended attribute entry
#[derive(Clone, Debug)]
pub struct XattrEntry {
    pub name: String,
    pub value: Vec<u8>,
}

/// Stream specification (for ADS)
#[derive(Clone, Debug)]
pub struct StreamSpec {
    pub name: String,
}

/// File open options
#[derive(Clone, Debug)]
pub struct OpenOptions {
    pub read: bool,
    pub write: bool,
    pub create: bool,
    pub truncate: bool,
    pub append: bool,
    pub share: Vec<ShareMode>,
    pub stream: Option<String>,
}

/// Share mode for Windows compatibility
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShareMode {
    Read,
    Write,
    Delete,
}

/// Lock kind for byte-range locking
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockKind {
    Shared,
    Exclusive,
}

/// Byte range lock specification
#[derive(Clone, Copy, Debug)]
pub struct LockRange {
    pub offset: u64,
    pub len: u64,
    pub kind: LockKind,
}

/// Fallocate mode (optional operation)
#[derive(Clone, Copy, Debug)]
pub enum FallocateMode {
    Allocate,
    PunchHole,
}

/// Content identifier for storage backend
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ContentId(pub u64);

impl ContentId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }
}

/// Event kinds for filesystem change notifications
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventKind {
    Created {
        path: String,
    },
    Removed {
        path: String,
    },
    Modified {
        path: String,
    },
    Renamed {
        from: String,
        to: String,
    },
    BranchCreated {
        id: BranchId,
        name: Option<String>,
    },
    SnapshotCreated {
        id: SnapshotId,
        name: Option<String>,
    },
}

/// Event sink trait for receiving filesystem change notifications
pub trait EventSink: Send + Sync {
    fn on_event(&self, evt: &EventKind);
}

/// Opaque event subscription identifier
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SubscriptionId(pub u64);

impl SubscriptionId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }
}

/// Filesystem statistics
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FsStats {
    pub branches: u32,
    pub snapshots: u32,
    pub open_handles: u32,
    pub bytes_in_memory: u64,
    pub bytes_spilled: u64,
}

/// Lower filesystem provider trait for overlay mode
/// This trait defines the interface to read from the underlying ("lower") filesystem
/// when no upper entry exists in the overlay branch.
#[cfg_attr(test, mockall::automock)]
pub trait LowerFs: Send + Sync {
    /// Get attributes for a path in the lower filesystem
    fn stat(&self, abs_path: &std::path::Path) -> crate::error::FsResult<Attributes>;

    /// Open a file for read-only access in the lower filesystem
    fn open_ro(
        &self,
        abs_path: &std::path::Path,
    ) -> crate::error::FsResult<Box<dyn std::io::Read + Send>>;

    /// List directory contents in the lower filesystem
    fn readdir(&self, abs_dir: &std::path::Path) -> crate::error::FsResult<Vec<DirEntry>>;

    /// Read symbolic link target in the lower filesystem
    fn readlink(&self, abs_path: &std::path::Path) -> crate::error::FsResult<std::path::PathBuf>;

    /// Get extended attribute from the lower filesystem
    fn getxattr(&self, abs_path: &std::path::Path, name: &str) -> crate::error::FsResult<Vec<u8>>;

    /// List extended attributes from the lower filesystem
    fn listxattr(&self, abs_path: &std::path::Path) -> crate::error::FsResult<Vec<String>>;
}

/// Backstore trait for managing upper layer storage
/// The backstore manages the storage location for copied-up files in overlay mode.
#[cfg_attr(test, mockall::automock)]
pub trait Backstore: Send + Sync {
    /// Check if this backstore supports native snapshots (e.g., APFS, ZFS, Btrfs)
    fn supports_native_snapshots(&self) -> bool;

    /// Create a native snapshot of the backstore if supported
    fn snapshot_native(&self, snapshot_name: &str) -> crate::error::FsResult<()>;

    /// Create a reflink/clone of a file within the backstore
    fn reflink(
        &self,
        from_path: &std::path::Path,
        to_path: &std::path::Path,
    ) -> crate::error::FsResult<()>;

    /// Get the root path of this backstore
    fn root_path(&self) -> std::path::PathBuf;
}
