//! Swift-callable bridge for AgentFS FSKit adapter.
//!
//! This crate provides a higher-level interface for the Swift FSKit extension
//! to communicate with the Rust AgentFS core, handling memory management and
//! type conversions.

use libc::{c_char, size_t, ssize_t};
use std::ffi::{CStr, CString};
use std::ptr;
use thiserror::Error;

// Declare extern C functions from agentfs-ffi
extern "C" {
    fn af_fs_create(config_json: *const libc::c_char, out_fs: *mut u64) -> i32;
    fn af_fs_destroy(fs: u64) -> i32;
    fn af_mkdir(fs: u64, path: *const libc::c_char, mode: u32) -> i32;
    fn af_snapshot_create(fs: u64, name: *const libc::c_char, out_id: *mut u8) -> i32;
    fn af_branch_create_from_snapshot(
        fs: u64,
        snap: *const u8,
        name: *const libc::c_char,
        out_id: *mut u8,
    ) -> i32;
    fn af_bind_process_to_branch(fs: u64, branch: *const u8) -> i32;
    fn af_open(
        fs: u64,
        path: *const libc::c_char,
        options_json: *const libc::c_char,
        out_h: *mut u64,
    ) -> i32;
    fn af_read(fs: u64, h: u64, off: u64, buf: *mut u8, len: u32, out_read: *mut u32) -> i32;
    fn af_write(fs: u64, h: u64, off: u64, buf: *const u8, len: u32, out_written: *mut u32) -> i32;
    fn af_close(fs: u64, h: u64) -> i32;
    fn af_control_request(
        fs: u64,
        request_data: *const u8,
        request_len: usize,
        response_data: *mut u8,
        response_max_len: usize,
        response_actual_len: *mut usize,
    ) -> i32;
    fn af_getattr(fs: u64, path: *const libc::c_char, out_attrs: *mut u8, attrs_size: usize)
    -> i32;
    fn af_rmdir(fs: u64, path: *const libc::c_char) -> i32;
    fn af_unlink(fs: u64, path: *const libc::c_char) -> i32;
    fn af_symlink(fs: u64, target: *const libc::c_char, linkpath: *const libc::c_char) -> i32;
    fn af_readlink(
        fs: u64,
        path: *const libc::c_char,
        out_target: *mut libc::c_char,
        target_size: usize,
    ) -> i32;
}

// ============================================================================
// Type definitions (mirrored from agentfs-fskit-sys for FFI compatibility)
// ============================================================================

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

/// Errors that can occur in the bridge layer
#[derive(Error, Debug)]
pub enum BridgeError {
    #[error("Invalid UTF-8 string")]
    InvalidUtf8,
    #[error("Null pointer")]
    NullPointer,
    #[error("Operation failed: {0}")]
    OperationFailed(String),
    #[error("Out of memory")]
    OutOfMemory,
}

pub type Result<T> = std::result::Result<T, BridgeError>;

/// Safe wrapper around FsCore handle
pub struct FsCore {
    handle: u64, // AfFs from agentfs-ffi
}

impl FsCore {
    /// Create a new FsCore instance with default configuration
    pub fn new() -> Result<Self> {
        // Create default config JSON
        let config_json = r#"{"max_memory_bytes": 67108864, "max_open_handles": 1024, "max_branches": 10, "max_snapshots": 10}"#;
        let config_cstr = CString::new(config_json).map_err(|_| BridgeError::InvalidUtf8)?;

        let mut handle: u64 = 0;

        let result = unsafe {
            af_fs_create(
                config_cstr.as_ptr() as *const c_char,
                &mut handle as *mut u64,
            )
        };

        if result != 0 {
            // AfResult::AfOk = 0
            return Err(BridgeError::OperationFailed(
                "Failed to create FsCore".to_string(),
            ));
        }

        Ok(Self { handle })
    }

    /// Create a directory
    pub fn mkdir(&self, path: &str, mode: u32) -> Result<()> {
        let c_path = CString::new(path).map_err(|_| BridgeError::InvalidUtf8)?;

        let result = unsafe { af_mkdir(self.handle, c_path.as_ptr(), mode) };

        if result == 0 {
            // AfResult::AfOk = 0
            Ok(())
        } else {
            Err(BridgeError::OperationFailed(format!(
                "mkdir failed for path: {}",
                path
            )))
        }
    }

    /// Create a snapshot
    pub fn create_snapshot(&self, name: Option<&str>) -> Result<String> {
        let c_name = if let Some(name) = name {
            Some(CString::new(name).map_err(|_| BridgeError::InvalidUtf8)?)
        } else {
            None
        };

        let mut snapshot_id = [0u8; 32]; // hex encoded is 32 chars

        let result = unsafe {
            af_snapshot_create(
                self.handle,
                c_name.as_ref().map_or(ptr::null(), |s| s.as_ptr()),
                snapshot_id.as_mut_ptr(),
            )
        };

        if result == 0 {
            // AfResult::AfOk = 0
            let c_str = unsafe { CStr::from_ptr(snapshot_id.as_ptr() as *const c_char) };
            let id_str = c_str.to_str().map_err(|_| BridgeError::InvalidUtf8)?.to_string();
            Ok(id_str)
        } else {
            Err(BridgeError::OperationFailed(
                "snapshot creation failed".to_string(),
            ))
        }
    }

    /// Create a branch from a snapshot
    pub fn create_branch(&self, snapshot_id: &str, branch_name: &str) -> Result<String> {
        let c_snapshot_id = CString::new(snapshot_id).map_err(|_| BridgeError::InvalidUtf8)?;
        let c_branch_name = CString::new(branch_name).map_err(|_| BridgeError::InvalidUtf8)?;

        let mut branch_id = [0u8; 32]; // hex encoded is 32 chars

        let result = unsafe {
            af_branch_create_from_snapshot(
                self.handle,
                c_snapshot_id.as_ptr() as *const u8,
                c_branch_name.as_ptr(),
                branch_id.as_mut_ptr(),
            )
        };

        if result == 0 {
            // AfResult::AfOk = 0
            let c_str = unsafe { CStr::from_ptr(branch_id.as_ptr() as *const c_char) };
            let id_str = c_str.to_str().map_err(|_| BridgeError::InvalidUtf8)?.to_string();
            Ok(id_str)
        } else {
            Err(BridgeError::OperationFailed(format!(
                "branch creation failed: {}",
                branch_name
            )))
        }
    }

    /// Bind process to a branch
    pub fn bind_process(&self, branch_id: &str) -> Result<()> {
        let c_branch_id = CString::new(branch_id).map_err(|_| BridgeError::InvalidUtf8)?;

        let result =
            unsafe { af_bind_process_to_branch(self.handle, c_branch_id.as_ptr() as *const u8) };

        if result == 0 {
            // AfResult::AfOk = 0
            Ok(())
        } else {
            Err(BridgeError::OperationFailed(format!(
                "process binding failed: {}",
                branch_id
            )))
        }
    }
}

impl Drop for FsCore {
    fn drop(&mut self) {
        unsafe {
            af_fs_destroy(self.handle);
        }
    }
}

/// File type enumeration for Swift
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsFileType {
    Regular,
    Directory,
    Symlink,
}

/// File attributes structure for Swift
#[derive(Debug, Clone)]
pub struct FsAttributes {
    pub file_id: u64,
    pub parent_id: u64,
    pub size: u64,
    pub alloc_size: u64,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub link_count: u32,
    pub flags: u32,
    pub file_type: FsFileType,
    pub access_time_sec: i64,
    pub access_time_nsec: i64,
    pub modify_time_sec: i64,
    pub modify_time_nsec: i64,
    pub change_time_sec: i64,
    pub change_time_nsec: i64,
    pub birth_time_sec: i64,
    pub birth_time_nsec: i64,
}

impl FsAttributes {
    fn from_c(attrs: AgentFsAttributes) -> Self {
        Self {
            file_id: attrs.file_id,
            parent_id: attrs.parent_id,
            size: attrs.size,
            alloc_size: attrs.alloc_size,
            mode: attrs.mode,
            uid: attrs.uid,
            gid: attrs.gid,
            link_count: attrs.link_count,
            flags: attrs.flags,
            file_type: match attrs.file_type {
                AgentFsFileType::Regular => FsFileType::Regular,
                AgentFsFileType::Directory => FsFileType::Directory,
                AgentFsFileType::Symlink => FsFileType::Symlink,
            },
            access_time_sec: attrs.access_time_sec,
            access_time_nsec: attrs.access_time_nsec,
            modify_time_sec: attrs.modify_time_sec,
            modify_time_nsec: attrs.modify_time_nsec,
            change_time_sec: attrs.change_time_sec,
            change_time_nsec: attrs.change_time_nsec,
            birth_time_sec: attrs.birth_time_sec,
            birth_time_nsec: attrs.birth_time_nsec,
        }
    }
}

// ============================================================================
// Swift-callable C functions
// ============================================================================

/// Create a new FsCore instance (Swift-callable)
#[no_mangle]
pub unsafe extern "C" fn agentfs_bridge_core_create() -> *mut FsCore {
    match FsCore::new() {
        Ok(core) => {
            // Leak the box to return a raw pointer (Swift will manage cleanup)
            Box::into_raw(Box::new(core))
        }
        Err(_) => ptr::null_mut(),
    }
}

/// Destroy an FsCore instance (Swift-callable)
#[no_mangle]
pub unsafe extern "C" fn agentfs_bridge_core_destroy(core: *mut FsCore) {
    if !core.is_null() {
        unsafe {
            // First destroy the Rust FsCore
            let _ = Box::from_raw(core);
            // Note: The underlying FFI handle is automatically dropped by FsCore::drop
        }
    }
}

/// Create directory (Swift-callable)
#[no_mangle]
pub unsafe extern "C" fn agentfs_bridge_mkdir(
    core: *mut FsCore,
    path: *const c_char,
    mode: u32,
) -> i32 {
    if core.is_null() || path.is_null() {
        return -1;
    }

    let core_ref = unsafe { &*core };
    let path_str = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };

    match core_ref.mkdir(path_str, mode) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// Create snapshot (Swift-callable)
#[no_mangle]
pub unsafe extern "C" fn agentfs_bridge_snapshot_create(
    core: *mut FsCore,
    name: *const c_char,
    out_id: *mut u8,
    id_size: usize,
) -> i32 {
    if core.is_null() || out_id.is_null() || id_size < 32 {
        return -1; // 16 bytes hex encoded = 32 chars
    }

    let core_ref = unsafe { &*core };
    let name_str = if name.is_null() {
        None
    } else {
        match unsafe { CStr::from_ptr(name) }.to_str() {
            Ok(s) => Some(s),
            Err(_) => return -1,
        }
    };

    match core_ref.create_snapshot(name_str) {
        Ok(snapshot_id) => {
            let id_bytes = snapshot_id.as_bytes();
            if id_bytes.len() <= id_size {
                unsafe {
                    ptr::copy_nonoverlapping(id_bytes.as_ptr(), out_id, id_bytes.len());
                    *out_id.add(id_bytes.len()) = 0; // null terminator
                }
                0
            } else {
                -1
            }
        }
        Err(_) => -1,
    }
}

/// Create branch (Swift-callable)
#[no_mangle]
pub unsafe extern "C" fn agentfs_bridge_branch_create(
    core: *mut FsCore,
    snapshot_id: *const c_char,
    branch_name: *const c_char,
    out_id: *mut u8,
    id_size: usize,
) -> i32 {
    if core.is_null()
        || snapshot_id.is_null()
        || branch_name.is_null()
        || out_id.is_null()
        || id_size < 32
    {
        return -1;
    }

    let core_ref = unsafe { &*core };
    let snapshot_str = match unsafe { CStr::from_ptr(snapshot_id) }.to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let branch_str = match unsafe { CStr::from_ptr(branch_name) }.to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };

    match core_ref.create_branch(snapshot_str, branch_str) {
        Ok(branch_id) => {
            let id_bytes = branch_id.as_bytes();
            if id_bytes.len() <= id_size {
                unsafe {
                    ptr::copy_nonoverlapping(id_bytes.as_ptr(), out_id, id_bytes.len());
                    *out_id.add(id_bytes.len()) = 0; // null terminator
                }
                0
            } else {
                -1
            }
        }
        Err(_) => -1,
    }
}

/// Bind process to branch (Swift-callable)
#[no_mangle]
pub unsafe extern "C" fn agentfs_bridge_bind_process(
    core: *mut FsCore,
    branch_id: *const c_char,
) -> i32 {
    if core.is_null() || branch_id.is_null() {
        return -1;
    }

    let core_ref = unsafe { &*core };
    let branch_str = match unsafe { CStr::from_ptr(branch_id) }.to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };

    match core_ref.bind_process(branch_str) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// Open file (Swift-callable)
#[no_mangle]
pub unsafe extern "C" fn agentfs_bridge_open(
    core: *mut FsCore,
    path: *const c_char,
    options_json: *const c_char,
    out_handle: *mut u64,
) -> i32 {
    if core.is_null() || path.is_null() || options_json.is_null() || out_handle.is_null() {
        return -1;
    }

    let core_handle = unsafe { (*core).handle };

    let result = unsafe { af_open(core_handle, path, options_json, out_handle) };

    result
}

/// Read from file (Swift-callable)
#[no_mangle]
pub unsafe extern "C" fn agentfs_bridge_read(
    core: *mut FsCore,
    handle: u64,
    offset: u64,
    buffer: *mut u8,
    length: u32,
    out_read: *mut u32,
) -> i32 {
    if core.is_null() || buffer.is_null() || out_read.is_null() {
        return -1;
    }

    let core_handle = unsafe { (*core).handle };

    let result = unsafe { af_read(core_handle, handle, offset, buffer, length, out_read) };

    result
}

/// Write to file (Swift-callable)
#[no_mangle]
pub unsafe extern "C" fn agentfs_bridge_write(
    core: *mut FsCore,
    handle: u64,
    offset: u64,
    buffer: *const u8,
    length: u32,
    out_written: *mut u32,
) -> i32 {
    if core.is_null() || buffer.is_null() || out_written.is_null() {
        return -1;
    }

    let core_handle = unsafe { (*core).handle };

    let result = unsafe { af_write(core_handle, handle, offset, buffer, length, out_written) };

    result
}

/// Close file (Swift-callable)
#[no_mangle]
pub unsafe extern "C" fn agentfs_bridge_close(core: *mut FsCore, handle: u64) -> i32 {
    if core.is_null() {
        return -1;
    }

    let core_handle = unsafe { (*core).handle };

    let result = unsafe { af_close(core_handle, handle) };

    result
}

/// Control request (Swift-callable)
#[no_mangle]
pub unsafe extern "C" fn agentfs_bridge_control_request(
    fs: u64,
    request_data: *const u8,
    request_len: usize,
    response_data: *mut u8,
    response_max_len: usize,
    response_actual_len: *mut usize,
) -> i32 {
    if request_data.is_null() || response_data.is_null() || response_actual_len.is_null() {
        return -1;
    }

    let result = unsafe {
        af_control_request(
            fs,
            request_data,
            request_len,
            response_data,
            response_max_len,
            response_actual_len,
        )
    };

    result
}

/// Get file statistics (Swift-callable)
/// Returns JSON string with file attributes
#[no_mangle]
pub unsafe extern "C" fn agentfs_bridge_stat(
    core: *mut FsCore,
    path: *const c_char,
    buffer: *mut c_char,
    buffer_size: usize,
) -> i32 {
    if core.is_null() || path.is_null() || buffer.is_null() || buffer_size == 0 {
        return -1;
    }

    // For now, return a basic stat structure
    // In a full implementation, this would call af_stat or similar
    let stat_json = r#"{
        "file_id": 1,
        "parent_id": 0,
        "size": 0,
        "alloc_size": 4096,
        "mode": 33188,
        "uid": 501,
        "gid": 20,
        "link_count": 1,
        "flags": 0,
        "file_type": 0,
        "access_time_sec": 1640995200,
        "access_time_nsec": 0,
        "modify_time_sec": 1640995200,
        "modify_time_nsec": 0,
        "change_time_sec": 1640995200,
        "change_time_nsec": 0,
        "birth_time_sec": 1640995200,
        "birth_time_nsec": 0
    }"#;

    let len = stat_json.len().min(buffer_size - 1);

    unsafe {
        ptr::copy_nonoverlapping(stat_json.as_ptr(), buffer as *mut u8, len);
        *buffer.add(len) = 0; // null terminator
    }

    0
}

/// Get filesystem statistics (Swift-callable)
/// Returns JSON string with filesystem stats
#[no_mangle]
pub unsafe extern "C" fn agentfs_bridge_statfs(
    core: *mut FsCore,
    buffer: *mut c_char,
    buffer_size: usize,
) -> i32 {
    if core.is_null() || buffer.is_null() || buffer_size == 0 {
        return -1;
    }

    let statfs_json = r#"{
        "block_size": 4096,
        "io_size": 4096,
        "total_blocks": 1048576,
        "available_blocks": 1048576,
        "free_blocks": 1048576,
        "total_files": 1048576,
        "free_files": 1048576
    }"#;

    let len = statfs_json.len().min(buffer_size - 1);

    unsafe {
        ptr::copy_nonoverlapping(statfs_json.as_ptr(), buffer as *mut u8, len);
        *buffer.add(len) = 0; // null terminator
    }

    0
}

/// Read directory contents (Swift-callable)
/// Returns JSON array of directory entries
#[no_mangle]
pub unsafe extern "C" fn agentfs_bridge_readdir(
    core: *mut FsCore,
    path: *const c_char,
    buffer: *mut c_char,
    buffer_size: usize,
) -> i32 {
    if core.is_null() || path.is_null() || buffer.is_null() || buffer_size == 0 {
        return -1;
    }

    // For now, return a basic directory listing
    // In a full implementation, this would enumerate the actual directory contents
    let readdir_json = r#"[
        {"name": ".", "type": "directory"},
        {"name": "..", "type": "directory"},
        {"name": ".agentfs", "type": "directory"}
    ]"#;

    let len = readdir_json.len().min(buffer_size - 1);

    unsafe {
        ptr::copy_nonoverlapping(readdir_json.as_ptr(), buffer as *mut u8, len);
        *buffer.add(len) = 0; // null terminator
    }

    0
}

/// Get file attributes (Swift-callable)
#[no_mangle]
pub unsafe extern "C" fn agentfs_bridge_getattr(
    core: *mut FsCore,
    path: *const c_char,
    buffer: *mut c_char,
    buffer_size: usize,
) -> i32 {
    if core.is_null() || path.is_null() || buffer.is_null() || buffer_size == 0 {
        return -1;
    }

    let core_handle = unsafe { (*core).handle };
    let result = unsafe { af_getattr(core_handle, path, buffer as *mut u8, buffer_size) };

    result
}

/// Create symlink (Swift-callable)
#[no_mangle]
pub unsafe extern "C" fn agentfs_bridge_symlink(
    core: *mut FsCore,
    target: *const c_char,
    linkpath: *const c_char,
) -> i32 {
    if core.is_null() || target.is_null() || linkpath.is_null() {
        return -1;
    }

    let core_handle = unsafe { (*core).handle };
    let result = unsafe { af_symlink(core_handle, target, linkpath) };

    result
}

/// Read symlink (Swift-callable)
#[no_mangle]
pub unsafe extern "C" fn agentfs_bridge_readlink(
    core: *mut FsCore,
    path: *const c_char,
    buffer: *mut c_char,
    buffer_size: usize,
) -> i32 {
    if core.is_null() || path.is_null() || buffer.is_null() || buffer_size == 0 {
        return -1;
    }

    let core_handle = unsafe { (*core).handle };
    let result = unsafe { af_readlink(core_handle, path, buffer, buffer_size) };

    result
}

/// Rename/move file (Swift-callable)
#[no_mangle]
pub unsafe extern "C" fn agentfs_bridge_rename(
    _core: *mut FsCore,
    _oldpath: *const c_char,
    _newpath: *const c_char,
) -> i32 {
    // TODO: Implement rename in Rust core
    -1
}

/// Remove directory (Swift-callable)
#[no_mangle]
pub unsafe extern "C" fn agentfs_bridge_rmdir(core: *mut FsCore, path: *const c_char) -> i32 {
    if core.is_null() || path.is_null() {
        return -1;
    }

    let core_handle = unsafe { (*core).handle };
    let result = unsafe { af_rmdir(core_handle, path) };

    result
}

/// Unlink file (Swift-callable)
#[no_mangle]
pub unsafe extern "C" fn agentfs_bridge_unlink(core: *mut FsCore, path: *const c_char) -> i32 {
    if core.is_null() || path.is_null() {
        return -1;
    }

    let core_handle = unsafe { (*core).handle };
    let result = unsafe { af_unlink(core_handle, path) };

    result
}

/// Get error message from last operation (Swift-callable)
#[no_mangle]
pub unsafe extern "C" fn agentfs_bridge_get_error_message(
    buffer: *mut c_char,
    buffer_size: usize,
) -> usize {
    // For now, return empty string. In a real implementation,
    // this would return the last error message.
    if buffer.is_null() || buffer_size == 0 {
        return 0;
    }

    let error_msg = "Unknown error";
    let len = error_msg.len().min(buffer_size - 1);

    unsafe {
        ptr::copy_nonoverlapping(error_msg.as_ptr(), buffer as *mut u8, len);
        *buffer.add(len) = 0; // null terminator
    }

    len
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_sizes() {
        // Test that our C-compatible types have expected sizes
        assert_eq!(std::mem::size_of::<AgentFsResult>(), 4); // C enum size
        assert_eq!(std::mem::size_of::<AgentFsFileType>(), 4); // C enum size
        assert_eq!(std::mem::size_of::<AgentFsAttributes>(), 120); // Struct size: 4*u64 + 5*u32 + 1*enum + 8*i64
    }
}
