// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! C ABI bindings for AgentFS FSKit adapter.
//!
//! This crate provides the C-compatible interface that the Swift FSKit extension
//! uses to communicate with the Rust AgentFS core.

use libc::{c_char, size_t, ssize_t};

/// Opaque handle to an FsCore instance
#[repr(C)]
pub struct AgentFsCoreHandle {
    _private: [u8; 0],
}

/// Result codes for FFI operations
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentFsResult {
    Ok = 0,
    Error = 1,
    NotFound = 2,
    PermissionDenied = 3,
    InvalidArgument = 4,
    OutOfMemory = 5,
    IoError = 6,
}

/// File type enumeration
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentFsFileType {
    Regular = 0,
    Directory = 1,
    Symlink = 2,
    // Add more as needed
}

/// File attributes structure (C-compatible)
#[repr(C)]
#[derive(Debug, Clone)]
pub struct AgentFsAttributes {
    pub file_id: u64,
    pub parent_id: u64,
    pub size: u64,
    pub alloc_size: u64,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub link_count: u32,
    pub flags: u32,
    pub file_type: AgentFsFileType,
    pub access_time_sec: i64,
    pub access_time_nsec: i64,
    pub modify_time_sec: i64,
    pub modify_time_nsec: i64,
    pub change_time_sec: i64,
    pub change_time_nsec: i64,
    pub birth_time_sec: i64,
    pub birth_time_nsec: i64,
}

/// Directory entry structure
#[repr(C)]
#[derive(Debug, Clone)]
pub struct AgentFsDirEntry {
    pub name: *const c_char, // null-terminated UTF-8 string
    pub file_type: AgentFsFileType,
    pub file_id: u64,
}

// ============================================================================
// Core lifecycle functions
// ============================================================================

/// Create a new FsCore instance
///
/// Returns a handle to the core instance, or null on failure
#[no_mangle]
pub extern "C" fn agentfs_core_create() -> *mut AgentFsCoreHandle {
    // TODO: Implement when agentfs-core is available
    std::ptr::null_mut()
}

/// Destroy an FsCore instance
#[no_mangle]
pub extern "C" fn agentfs_core_destroy(_handle: *mut AgentFsCoreHandle) {
    // TODO: Implement when agentfs-core is available
}

// ============================================================================
// Filesystem operations
// ============================================================================

/// Get attributes for a path
#[no_mangle]
pub extern "C" fn agentfs_getattr(
    _handle: *const AgentFsCoreHandle,
    _path: *const c_char,
    _attrs: *mut AgentFsAttributes,
) -> AgentFsResult {
    // TODO: Implement when agentfs-core is available
    AgentFsResult::Error
}

/// Read directory entries
///
/// Returns number of entries read, or negative value on error
#[no_mangle]
pub extern "C" fn agentfs_readdir(
    _handle: *const AgentFsCoreHandle,
    _path: *const c_char,
    _entries: *mut *mut AgentFsDirEntry,
    _max_entries: size_t,
) -> ssize_t {
    // TODO: Implement when agentfs-core is available
    -1
}

/// Create a file or directory
#[no_mangle]
pub extern "C" fn agentfs_create(
    _handle: *const AgentFsCoreHandle,
    _path: *const c_char,
    _file_type: AgentFsFileType,
    _mode: u32,
) -> AgentFsResult {
    // TODO: Implement when agentfs-core is available
    AgentFsResult::Error
}

/// Remove a file or directory
#[no_mangle]
pub extern "C" fn agentfs_remove(
    _handle: *const AgentFsCoreHandle,
    _path: *const c_char,
) -> AgentFsResult {
    // TODO: Implement when agentfs-core is available
    AgentFsResult::Error
}

/// Read data from a file
#[no_mangle]
pub extern "C" fn agentfs_read(
    _handle: *const AgentFsCoreHandle,
    _path: *const c_char,
    _offset: u64,
    _size: size_t,
    _buffer: *mut u8,
) -> ssize_t {
    // TODO: Implement when agentfs-core is available
    -1
}

/// Write data to a file
#[no_mangle]
pub extern "C" fn agentfs_write(
    _handle: *const AgentFsCoreHandle,
    _path: *const c_char,
    _offset: u64,
    _size: size_t,
    _buffer: *const u8,
) -> ssize_t {
    // TODO: Implement when agentfs-core is available
    -1
}

/// Rename/move a file or directory
#[no_mangle]
pub extern "C" fn agentfs_rename(
    _handle: *const AgentFsCoreHandle,
    _old_path: *const c_char,
    _new_path: *const c_char,
) -> AgentFsResult {
    // TODO: Implement when agentfs-core is available
    AgentFsResult::Error
}

/// Set attributes on a file
#[no_mangle]
pub extern "C" fn agentfs_setattr(
    _handle: *const AgentFsCoreHandle,
    _path: *const c_char,
    _attrs: *const AgentFsAttributes,
) -> AgentFsResult {
    // TODO: Implement when agentfs-core is available
    AgentFsResult::Error
}

// ============================================================================
// Snapshot and branch operations
// ============================================================================

/// Create a snapshot
#[no_mangle]
pub extern "C" fn agentfs_snapshot_create(
    _handle: *const AgentFsCoreHandle,
    _label: *const c_char,
    _snapshot_id: *mut c_char,
    _id_size: size_t,
) -> AgentFsResult {
    // TODO: Implement when agentfs-core is available
    AgentFsResult::Error
}

/// Create a branch from a snapshot
#[no_mangle]
pub extern "C" fn agentfs_branch_create(
    _handle: *const AgentFsCoreHandle,
    _snapshot_id: *const c_char,
    _branch_name: *const c_char,
) -> AgentFsResult {
    // TODO: Implement when agentfs-core is available
    AgentFsResult::Error
}

/// Bind process to a branch
#[no_mangle]
pub extern "C" fn agentfs_bind_process(
    _handle: *const AgentFsCoreHandle,
    _branch_name: *const c_char,
    _pid: u32,
) -> AgentFsResult {
    // TODO: Implement when agentfs-core is available
    AgentFsResult::Error
}

// ============================================================================
// Memory management helpers
// ============================================================================

/// Free a directory entry array
#[no_mangle]
pub extern "C" fn agentfs_free_dir_entries(_entries: *mut AgentFsDirEntry, _count: size_t) {
    // TODO: Implement proper memory management
}

/// Get last error message
#[no_mangle]
pub extern "C" fn agentfs_get_error_message(_buffer: *mut c_char, _buffer_size: size_t) -> size_t {
    // TODO: Implement error message retrieval
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_abi_sizes() {
        // Verify that our C structures have the expected sizes
        assert_eq!(std::mem::size_of::<AgentFsResult>(), 4);
        assert_eq!(std::mem::size_of::<AgentFsFileType>(), 4);
        assert_eq!(std::mem::size_of::<AgentFsAttributes>(), 120); // 4*u64 + 5*u32 + 1*enum + 8*i64
    }
}
