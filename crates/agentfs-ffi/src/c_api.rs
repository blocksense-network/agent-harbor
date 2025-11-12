// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! C API definitions for AgentFS FFI
//!
//! # Safety
//! All FFI functions in this module require that:
//! - All pointer arguments are valid, properly aligned, and point to initialized memory (or null where documented).
//! - String pointers (`*const c_char`) point to valid null-terminated C strings.
//! - Output pointers (`*mut`) point to writable memory of the expected size.
//! - The `fs` handle passed to functions must have been obtained from `af_fs_create` and not yet destroyed.
//!
//! Violating these preconditions results in undefined behavior.

#![allow(clippy::missing_safety_doc)] // Safety documented at module level above

use agentfs_core::{
    CachePolicy, CaseSensitivity, FsConfig, FsCore, FsLimits, MemoryPolicy, OpenOptions,
};
use std::collections::HashMap;
use std::ffi::CStr;
use std::os::raw::c_char;
use std::path::Path;
use std::ptr;
use std::sync::Mutex;

// Global registry for filesystem instances
lazy_static::lazy_static! {
    static ref FS_INSTANCES: Mutex<HashMap<u64, FsCore>> = Mutex::new(HashMap::new());
    static ref NEXT_ID: Mutex<u64> = Mutex::new(1);
}

/// Opaque filesystem handle (just an ID)
pub type AfFs = u64;

/// Snapshot ID (16 bytes)
#[repr(C)]
pub struct AfSnapshotId {
    pub bytes: [u8; 16],
}

/// Branch ID (16 bytes)
#[repr(C)]
pub struct AfBranchId {
    pub bytes: [u8; 16],
}

/// Handle ID
pub type AfHandleId = u64;

/// Result codes matching POSIX errno where applicable
#[repr(C)]
pub enum AfResult {
    AfOk = 0,
    AfErrNotFound = 2,     // ENOENT
    AfErrExists = 17,      // EEXIST
    AfErrAcces = 13,       // EACCES
    AfErrNospc = 28,       // ENOSPC
    AfErrInval = 22,       // EINVAL
    AfErrBusy = 16,        // EBUSY
    AfErrUnsupported = 95, // ENOTSUP
}

/// Process registration functions
#[no_mangle]
pub unsafe extern "C" fn af_register_process(
    fs: AfFs,
    pid: u32,
    parent_pid: u32,
    uid: u32,
    gid: u32,
    out_pid: *mut u32,
) -> AfResult {
    if out_pid.is_null() {
        return AfResult::AfErrInval;
    }

    let instances = FS_INSTANCES.lock().unwrap();
    let core = match instances.get(&fs) {
        Some(c) => c,
        None => return AfResult::AfErrInval,
    };

    let registered_pid = core.register_process(pid, parent_pid, uid, gid);
    unsafe { *out_pid = registered_pid.as_u32() };
    AfResult::AfOk
}

/// Lifecycle functions
#[no_mangle]
pub unsafe extern "C" fn af_fs_create(config_json: *const c_char, out_fs: *mut AfFs) -> AfResult {
    if config_json.is_null() || out_fs.is_null() {
        return AfResult::AfErrInval;
    }

    // Parse config JSON
    let c_str = unsafe { CStr::from_ptr(config_json) };
    let config_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return AfResult::AfErrInval,
    };

    let config: serde_json::Value = match serde_json::from_str(config_str) {
        Ok(c) => c,
        Err(_) => return AfResult::AfErrInval,
    };

    // Create FsConfig from JSON
    let fs_config = FsConfig {
        case_sensitivity: CaseSensitivity::InsensitivePreserving, // Default for macOS
        memory: MemoryPolicy {
            max_bytes_in_memory: config.get("max_memory_bytes").and_then(|v| v.as_u64()),
            spill_directory: config
                .get("spill_directory")
                .and_then(|v| v.as_str())
                .map(|s| s.into()),
        },
        limits: FsLimits {
            max_open_handles: config
                .get("max_open_handles")
                .and_then(|v| v.as_u64())
                .unwrap_or(65536) as u32,
            max_branches: config.get("max_branches").and_then(|v| v.as_u64()).unwrap_or(256) as u32,
            max_snapshots: config.get("max_snapshots").and_then(|v| v.as_u64()).unwrap_or(4096)
                as u32,
        },
        cache: CachePolicy {
            attr_ttl_ms: 1000,
            entry_ttl_ms: 1000,
            negative_ttl_ms: 1000,
            enable_readdir_plus: true,
            auto_cache: true,
            writeback_cache: false,
        },
        enable_xattrs: true,
        enable_ads: false, // macOS uses xattrs instead of ADS
        track_events: true,
        security: agentfs_core::config::SecurityPolicy::default(),
        backstore: agentfs_core::config::BackstoreMode::InMemory,
        overlay: agentfs_core::config::OverlayConfig::default(),
        interpose: agentfs_core::config::InterposeConfig::default(),
    };

    // Create FsCore instance
    let fs_core = match FsCore::new(fs_config) {
        Ok(core) => core,
        Err(_) => return AfResult::AfErrInval,
    };

    // Store in registry and return ID
    let mut instances = FS_INSTANCES.lock().unwrap();
    let mut next_id = NEXT_ID.lock().unwrap();
    let id = *next_id;
    *next_id += 1;

    instances.insert(id, fs_core);
    unsafe { *out_fs = id };

    AfResult::AfOk
}

#[no_mangle]
pub unsafe extern "C" fn af_fs_destroy(fs: AfFs) -> AfResult {
    let mut instances = FS_INSTANCES.lock().unwrap();
    instances.remove(&fs);
    AfResult::AfOk
}

/// Convert FsError to AfResult
fn fs_error_to_af_result(err: &agentfs_core::FsError) -> AfResult {
    match err {
        agentfs_core::FsError::NotFound => AfResult::AfErrNotFound,
        agentfs_core::FsError::AlreadyExists => AfResult::AfErrExists,
        agentfs_core::FsError::AccessDenied => AfResult::AfErrAcces,
        agentfs_core::FsError::InvalidArgument => AfResult::AfErrInval,
        agentfs_core::FsError::Busy => AfResult::AfErrBusy,
        agentfs_core::FsError::NoSpace => AfResult::AfErrNospc,
        agentfs_core::FsError::Unsupported => AfResult::AfErrUnsupported,
        _ => AfResult::AfErrInval,
    }
}

/// Resolve a path to internal node ID and parent ID for stable identity mapping on adapters
#[no_mangle]
pub unsafe extern "C" fn af_resolve_id(
    fs: AfFs,
    pid: u32,
    path: *const c_char,
    out_node_id: *mut u64,
    out_parent_id: *mut u64,
) -> AfResult {
    if path.is_null() || out_node_id.is_null() {
        return AfResult::AfErrInval;
    }
    let path_str = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(_) => return AfResult::AfErrInval,
    };
    let instances = FS_INSTANCES.lock().unwrap();
    let core = match instances.get(&fs) {
        Some(c) => c,
        None => return AfResult::AfErrInval,
    };
    let pid_ref = &agentfs_core::PID::new(pid);
    match core.resolve_path_public(pid_ref, Path::new(path_str)) {
        Ok((nid, pid_opt)) => {
            unsafe {
                *out_node_id = nid;
                if !out_parent_id.is_null() {
                    *out_parent_id = pid_opt.unwrap_or(0);
                }
            }
            AfResult::AfOk
        }
        Err(e) => fs_error_to_af_result(&e),
    }
}

/// Snapshot operations
#[no_mangle]
pub unsafe extern "C" fn af_snapshot_create(
    fs: AfFs,
    name: *const c_char,
    out_id: *mut AfSnapshotId,
) -> AfResult {
    if out_id.is_null() {
        return AfResult::AfErrInval;
    }

    let name_str = if name.is_null() {
        None
    } else {
        match unsafe { CStr::from_ptr(name) }.to_str() {
            Ok(s) => Some(s),
            Err(_) => return AfResult::AfErrInval,
        }
    };

    let instances = FS_INSTANCES.lock().unwrap();
    let core = match instances.get(&fs) {
        Some(c) => c,
        None => return AfResult::AfErrInval,
    };

    match core.snapshot_create(name_str) {
        Ok(snapshot_id) => {
            unsafe { (*out_id).bytes.copy_from_slice(&snapshot_id.0) };
            AfResult::AfOk
        }
        Err(e) => fs_error_to_af_result(&e),
    }
}

/// Control plane operations - accepts raw SSZ request bytes, returns raw SSZ response bytes
#[no_mangle]
pub unsafe extern "C" fn af_control_request(
    fs: AfFs,
    request_data: *const u8,
    _request_len: usize,
    response_data: *mut u8,
    response_max_len: usize,
    response_actual_len: *mut usize,
) -> AfResult {
    if request_data.is_null() || response_data.is_null() || response_actual_len.is_null() {
        return AfResult::AfErrInval;
    }

    let instances = FS_INSTANCES.lock().unwrap();
    match instances.get(&fs) {
        Some(_) => {} // Core exists, continue
        None => return AfResult::AfErrInval,
    };

    // TODO: Implement proper SSZ decoding and processing
    // For now, create a simple success response to demonstrate the forwarding works
    // The Swift code forwards raw bytes without parsing, and Rust handles the actual processing
    let response_bytes = b"{\"status\":\"success\"}";

    // Check if response fits in output buffer
    if response_bytes.len() > response_max_len {
        return AfResult::AfErrInval; // Buffer too small
    }

    // Copy response bytes to output buffer
    unsafe {
        std::ptr::copy_nonoverlapping(response_bytes.as_ptr(), response_data, response_bytes.len());
        *response_actual_len = response_bytes.len();
    }

    AfResult::AfOk
}

/// Branch operations (legacy - kept for compatibility)
#[no_mangle]
pub unsafe extern "C" fn af_branch_create_from_snapshot(
    fs: AfFs,
    snap: AfSnapshotId,
    name: *const c_char,
    out_id: *mut AfBranchId,
) -> AfResult {
    if out_id.is_null() {
        return AfResult::AfErrInval;
    }

    let name_str = if name.is_null() {
        None
    } else {
        match unsafe { CStr::from_ptr(name) }.to_str() {
            Ok(s) => Some(s),
            Err(_) => return AfResult::AfErrInval,
        }
    };

    let instances = FS_INSTANCES.lock().unwrap();
    let core = match instances.get(&fs) {
        Some(c) => c,
        None => return AfResult::AfErrInval,
    };

    let snapshot_id = agentfs_core::SnapshotId(snap.bytes);

    match core.branch_create_from_snapshot(snapshot_id, name_str) {
        Ok(branch_id) => {
            unsafe { (*out_id).bytes.copy_from_slice(&branch_id.0) };
            AfResult::AfOk
        }
        Err(e) => fs_error_to_af_result(&e),
    }
}

#[no_mangle]
pub unsafe extern "C" fn af_bind_process_to_branch(fs: AfFs, branch: AfBranchId) -> AfResult {
    let instances = FS_INSTANCES.lock().unwrap();
    let core = match instances.get(&fs) {
        Some(c) => c,
        None => return AfResult::AfErrInval,
    };

    let branch_id = agentfs_core::BranchId(branch.bytes);

    match core.bind_process_to_branch(branch_id) {
        Ok(()) => AfResult::AfOk,
        Err(e) => fs_error_to_af_result(&e),
    }
}

/// File operations (minimal set)
#[no_mangle]
pub unsafe extern "C" fn af_mkdir(fs: AfFs, pid: u32, path: *const c_char, mode: u32) -> AfResult {
    if path.is_null() {
        return AfResult::AfErrInval;
    }

    let path_str = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(_) => return AfResult::AfErrInval,
    };

    let instances = FS_INSTANCES.lock().unwrap();
    let core = match instances.get(&fs) {
        Some(c) => c,
        None => return AfResult::AfErrInval,
    };

    let pid_ref = &agentfs_core::PID::new(pid);
    match core.mkdir(pid_ref, Path::new(path_str), mode) {
        Ok(()) => AfResult::AfOk,
        Err(e) => fs_error_to_af_result(&e),
    }
}

#[no_mangle]
pub unsafe extern "C" fn af_open(
    fs: AfFs,
    pid: u32,
    path: *const c_char,
    options_json: *const c_char,
    out_h: *mut AfHandleId,
) -> AfResult {
    if path.is_null() || options_json.is_null() || out_h.is_null() {
        return AfResult::AfErrInval;
    }

    let path_str = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(_) => return AfResult::AfErrInval,
    };

    let options_str = match unsafe { CStr::from_ptr(options_json) }.to_str() {
        Ok(s) => s,
        Err(_) => return AfResult::AfErrInval,
    };

    // Parse options JSON - simplified for now
    let options: serde_json::Value = match serde_json::from_str(options_str) {
        Ok(o) => o,
        Err(_) => return AfResult::AfErrInval,
    };

    let open_options = OpenOptions {
        read: options.get("read").and_then(|v| v.as_bool()).unwrap_or(false),
        write: options.get("write").and_then(|v| v.as_bool()).unwrap_or(false),
        create: options.get("create").and_then(|v| v.as_bool()).unwrap_or(false),
        truncate: options.get("truncate").and_then(|v| v.as_bool()).unwrap_or(false),
        append: options.get("append").and_then(|v| v.as_bool()).unwrap_or(false),
        share: vec![], // Simplified
        stream: None,  // macOS uses xattrs instead
    };

    let instances = FS_INSTANCES.lock().unwrap();
    let core = match instances.get(&fs) {
        Some(c) => c,
        None => return AfResult::AfErrInval,
    };

    let pid_ref = &agentfs_core::PID::new(pid);
    let result = if open_options.create {
        core.create(pid_ref, Path::new(path_str), &open_options)
    } else {
        core.open(pid_ref, Path::new(path_str), &open_options)
    };

    match result {
        Ok(handle_id) => {
            unsafe { *out_h = handle_id.0 };
            AfResult::AfOk
        }
        Err(e) => fs_error_to_af_result(&e),
    }
}

/// Create child by parent id and raw name bytes (0=file,1=dir)
#[no_mangle]
pub unsafe extern "C" fn af_create_child_by_id(
    fs: AfFs,
    parent_id: u64,
    name_ptr: *const u8,
    name_len: usize,
    item_type: u32,
    mode: u32,
    out_node_id: *mut u64,
) -> AfResult {
    if name_ptr.is_null() || out_node_id.is_null() {
        return AfResult::AfErrInval;
    }
    let instances = FS_INSTANCES.lock().unwrap();
    let core = match instances.get(&fs) {
        Some(c) => c,
        None => return AfResult::AfErrInval,
    };
    let name = unsafe { std::slice::from_raw_parts(name_ptr, name_len) };
    match core.create_child_by_id(parent_id, name, item_type, mode) {
        Ok(nid) => {
            unsafe {
                *out_node_id = nid;
            }
            AfResult::AfOk
        }
        Err(e) => fs_error_to_af_result(&e),
    }
}

#[no_mangle]
pub unsafe extern "C" fn af_read(
    fs: AfFs,
    pid: u32,
    h: AfHandleId,
    off: u64,
    buf: *mut u8,
    len: u32,
    out_read: *mut u32,
) -> AfResult {
    if buf.is_null() || out_read.is_null() {
        return AfResult::AfErrInval;
    }

    let instances = FS_INSTANCES.lock().unwrap();
    let core = match instances.get(&fs) {
        Some(c) => c,
        None => return AfResult::AfErrInval,
    };

    let buffer = unsafe { std::slice::from_raw_parts_mut(buf, len as usize) };

    let pid_ref = &agentfs_core::PID::new(pid);
    match core.read(pid_ref, agentfs_core::HandleId(h), off, buffer) {
        Ok(bytes_read) => {
            unsafe { *out_read = bytes_read as u32 };
            AfResult::AfOk
        }
        Err(e) => fs_error_to_af_result(&e),
    }
}

#[no_mangle]
pub unsafe extern "C" fn af_write(
    fs: AfFs,
    pid: u32,
    h: AfHandleId,
    off: u64,
    buf: *const u8,
    len: u32,
    out_written: *mut u32,
) -> AfResult {
    if buf.is_null() || out_written.is_null() {
        return AfResult::AfErrInval;
    }

    let instances = FS_INSTANCES.lock().unwrap();
    let core = match instances.get(&fs) {
        Some(c) => c,
        None => return AfResult::AfErrInval,
    };

    let buffer = unsafe { std::slice::from_raw_parts(buf, len as usize) };

    let pid_ref = &agentfs_core::PID::new(pid);
    match core.write(pid_ref, agentfs_core::HandleId(h), off, buffer) {
        Ok(bytes_written) => {
            unsafe { *out_written = bytes_written as u32 };
            AfResult::AfOk
        }
        Err(e) => fs_error_to_af_result(&e),
    }
}

#[no_mangle]
pub unsafe extern "C" fn af_close(fs: AfFs, pid: u32, h: AfHandleId) -> AfResult {
    let instances = FS_INSTANCES.lock().unwrap();
    let core = match instances.get(&fs) {
        Some(c) => c,
        None => return AfResult::AfErrInval,
    };

    let pid_ref = &agentfs_core::PID::new(pid);
    match core.close(pid_ref, agentfs_core::HandleId(h)) {
        Ok(()) => AfResult::AfOk,
        Err(e) => fs_error_to_af_result(&e),
    }
}

/// Open by internal node id
#[no_mangle]
pub unsafe extern "C" fn af_open_by_id(
    fs: AfFs,
    pid: u32,
    node_id: u64,
    options_json: *const c_char,
    out_h: *mut AfHandleId,
) -> AfResult {
    if options_json.is_null() || out_h.is_null() {
        return AfResult::AfErrInval;
    }

    let options_str = match unsafe { CStr::from_ptr(options_json) }.to_str() {
        Ok(s) => s,
        Err(_) => return AfResult::AfErrInval,
    };
    let options: serde_json::Value = match serde_json::from_str(options_str) {
        Ok(o) => o,
        Err(_) => return AfResult::AfErrInval,
    };
    let open_options = OpenOptions {
        read: options.get("read").and_then(|v| v.as_bool()).unwrap_or(false),
        write: options.get("write").and_then(|v| v.as_bool()).unwrap_or(false),
        create: options.get("create").and_then(|v| v.as_bool()).unwrap_or(false),
        truncate: options.get("truncate").and_then(|v| v.as_bool()).unwrap_or(false),
        append: options.get("append").and_then(|v| v.as_bool()).unwrap_or(false),
        share: vec![],
        stream: None,
    };

    let instances = FS_INSTANCES.lock().unwrap();
    let core = match instances.get(&fs) {
        Some(c) => c,
        None => return AfResult::AfErrInval,
    };
    let pid_ref = &agentfs_core::PID::new(pid);

    match core.open_by_id(pid_ref, node_id, &open_options) {
        Ok(handle_id) => {
            unsafe {
                *out_h = handle_id.0;
            }
            AfResult::AfOk
        }
        Err(e) => fs_error_to_af_result(&e),
    }
}

/// Get filesystem statistics
#[no_mangle]
pub unsafe extern "C" fn af_stats(fs: AfFs, out_stats: *mut u8, stats_size: usize) -> AfResult {
    if out_stats.is_null() || stats_size == 0 {
        return AfResult::AfErrInval;
    }

    let instances = FS_INSTANCES.lock().unwrap();
    let core = match instances.get(&fs) {
        Some(c) => c,
        None => return AfResult::AfErrInval,
    };

    let stats = core.stats();

    // Binary format: branches(u32) + snapshots(u32) + open_handles(u32) + bytes_in_memory(u64) + bytes_spilled(u64)
    // Total: 4 + 4 + 4 + 8 + 8 = 28 bytes
    if stats_size < 28 {
        return AfResult::AfErrInval;
    }

    unsafe {
        // Helper to write unaligned integers in little-endian order
        let base = out_stats;
        std::ptr::copy_nonoverlapping(stats.branches.to_le_bytes().as_ptr(), base.add(0), 4);
        std::ptr::copy_nonoverlapping(stats.snapshots.to_le_bytes().as_ptr(), base.add(4), 4);
        std::ptr::copy_nonoverlapping(stats.open_handles.to_le_bytes().as_ptr(), base.add(8), 4);
        std::ptr::copy_nonoverlapping(
            stats.bytes_in_memory.to_le_bytes().as_ptr(),
            base.add(12),
            8,
        );
        std::ptr::copy_nonoverlapping(stats.bytes_spilled.to_le_bytes().as_ptr(), base.add(20), 8);
    }
    AfResult::AfOk
}

/// Get file attributes
#[no_mangle]
pub unsafe extern "C" fn af_getattr(
    fs: AfFs,
    pid: u32,
    path: *const c_char,
    out_attrs: *mut u8,
    attrs_size: usize,
) -> AfResult {
    if path.is_null() || out_attrs.is_null() || attrs_size == 0 {
        return AfResult::AfErrInval;
    }

    let path_str = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(_) => return AfResult::AfErrInval,
    };

    let instances = FS_INSTANCES.lock().unwrap();
    let core = match instances.get(&fs) {
        Some(c) => c,
        None => return AfResult::AfErrInval,
    };

    let pid_ref = &agentfs_core::PID::new(pid);
    match core.getattr(pid_ref, std::path::Path::new(path_str)) {
        Ok(attrs) => {
            // Rich binary format for FSKit bridge (little endian):
            // off 0:  len              u64
            // off 8:  file_type        u8  (0=file,1=dir,2=symlink)
            // off 9:  mode             u32 (POSIX mode)
            // off 13: uid              u32
            // off 17: gid              u32
            // off 21: atime            i64 (seconds)
            // off 29: mtime            i64
            // off 37: ctime            i64
            // off 45: birthtime        i64
            // total 53 bytes; align to 56 for safety
            let min = 56usize;
            if attrs_size < min {
                return AfResult::AfErrInval;
            }

            let file_type = if attrs.is_symlink {
                2u8
            } else if attrs.is_dir {
                1u8
            } else {
                0u8
            };

            // Synthesize a POSIX-like mode from per-class FileMode and dir bit
            let mut mode: u32 = 0;
            let (ur, uw, ux) = (
                attrs.mode_user.read,
                attrs.mode_user.write,
                attrs.mode_user.exec,
            );
            let (gr, gw, gx) = (
                attrs.mode_group.read,
                attrs.mode_group.write,
                attrs.mode_group.exec,
            );
            let (or, ow, ox) = (
                attrs.mode_other.read,
                attrs.mode_other.write,
                attrs.mode_other.exec,
            );
            if ur {
                mode |= 0o400
            }
            if uw {
                mode |= 0o200
            }
            if ux {
                mode |= 0o100
            }
            if gr {
                mode |= 0o040
            }
            if gw {
                mode |= 0o020
            }
            if gx {
                mode |= 0o010
            }
            if or {
                mode |= 0o004
            }
            if ow {
                mode |= 0o002
            }
            if ox {
                mode |= 0o001
            }
            if attrs.is_dir {
                mode |= 0o040000
            } else if attrs.is_symlink {
                mode |= 0o120000
            } else {
                mode |= 0o100000
            }

            unsafe {
                let base = out_attrs;
                std::ptr::copy_nonoverlapping(attrs.len.to_le_bytes().as_ptr(), base.add(0), 8);
                *base.add(8) = file_type;
                std::ptr::copy_nonoverlapping(mode.to_le_bytes().as_ptr(), base.add(9), 4);
                std::ptr::copy_nonoverlapping(attrs.uid.to_le_bytes().as_ptr(), base.add(13), 4);
                std::ptr::copy_nonoverlapping(attrs.gid.to_le_bytes().as_ptr(), base.add(17), 4);
                std::ptr::copy_nonoverlapping(
                    attrs.times.atime.to_le_bytes().as_ptr(),
                    base.add(21),
                    8,
                );
                std::ptr::copy_nonoverlapping(
                    attrs.times.mtime.to_le_bytes().as_ptr(),
                    base.add(29),
                    8,
                );
                std::ptr::copy_nonoverlapping(
                    attrs.times.ctime.to_le_bytes().as_ptr(),
                    base.add(37),
                    8,
                );
                std::ptr::copy_nonoverlapping(
                    attrs.times.birthtime.to_le_bytes().as_ptr(),
                    base.add(45),
                    8,
                );
            }
            AfResult::AfOk
        }
        Err(e) => fs_error_to_af_result(&e),
    }
}

/// Set times (utimens-like) on a path
#[no_mangle]
pub unsafe extern "C" fn af_set_times(
    fs: AfFs,
    pid: u32,
    path: *const c_char,
    atime: i64,
    mtime: i64,
    ctime: i64,
    birthtime: i64,
) -> AfResult {
    if path.is_null() {
        return AfResult::AfErrInval;
    }
    let path_str = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(_) => return AfResult::AfErrInval,
    };
    let instances = FS_INSTANCES.lock().unwrap();
    let core = match instances.get(&fs) {
        Some(c) => c,
        None => return AfResult::AfErrInval,
    };
    let pid_ref = &agentfs_core::PID::new(pid);
    let times = agentfs_core::FileTimes {
        atime,
        mtime,
        ctime,
        birthtime,
    };
    match core.set_times(pid_ref, std::path::Path::new(path_str), times) {
        Ok(()) => AfResult::AfOk,
        Err(e) => fs_error_to_af_result(&e),
    }
}

/// Set mode (chmod-like)
#[no_mangle]
pub unsafe extern "C" fn af_set_mode(
    fs: AfFs,
    pid: u32,
    path: *const c_char,
    mode: u32,
) -> AfResult {
    if path.is_null() {
        return AfResult::AfErrInval;
    }
    let path_str = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(_) => return AfResult::AfErrInval,
    };
    let instances = FS_INSTANCES.lock().unwrap();
    let core = match instances.get(&fs) {
        Some(c) => c,
        None => return AfResult::AfErrInval,
    };
    let pid_ref = &agentfs_core::PID::new(pid);
    match core.set_mode(pid_ref, std::path::Path::new(path_str), mode) {
        Ok(()) => AfResult::AfOk,
        Err(e) => fs_error_to_af_result(&e),
    }
}

/// Set owner (chown-like)
#[no_mangle]
pub unsafe extern "C" fn af_set_owner(
    fs: AfFs,
    pid: u32,
    path: *const c_char,
    uid: u32,
    gid: u32,
) -> AfResult {
    if path.is_null() {
        return AfResult::AfErrInval;
    }
    let path_str = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(_) => return AfResult::AfErrInval,
    };
    let instances = FS_INSTANCES.lock().unwrap();
    let core = match instances.get(&fs) {
        Some(c) => c,
        None => return AfResult::AfErrInval,
    };
    let pid_ref = &agentfs_core::PID::new(pid);
    match core.set_owner(pid_ref, std::path::Path::new(path_str), uid, gid) {
        Ok(()) => AfResult::AfOk,
        Err(e) => fs_error_to_af_result(&e),
    }
}

/// Rename path (no overwrite)
#[no_mangle]
pub unsafe extern "C" fn af_rename(
    fs: AfFs,
    pid: u32,
    old_path: *const c_char,
    new_path: *const c_char,
) -> AfResult {
    if old_path.is_null() || new_path.is_null() {
        return AfResult::AfErrInval;
    }
    let old_str = match unsafe { CStr::from_ptr(old_path) }.to_str() {
        Ok(s) => s,
        Err(_) => return AfResult::AfErrInval,
    };
    let new_str = match unsafe { CStr::from_ptr(new_path) }.to_str() {
        Ok(s) => s,
        Err(_) => return AfResult::AfErrInval,
    };
    let instances = FS_INSTANCES.lock().unwrap();
    let core = match instances.get(&fs) {
        Some(c) => c,
        None => return AfResult::AfErrInval,
    };
    let pid_ref = &agentfs_core::PID::new(pid);
    match core.rename(
        pid_ref,
        std::path::Path::new(old_str),
        std::path::Path::new(new_str),
    ) {
        Ok(()) => AfResult::AfOk,
        Err(e) => fs_error_to_af_result(&e),
    }
}

/// Enumerate directory names (NUL-delimited into buffer)
#[no_mangle]
pub unsafe extern "C" fn af_readdir(
    fs: AfFs,
    pid: u32,
    path: *const c_char,
    out_buf: *mut u8,
    buf_size: usize,
    out_len: *mut usize,
) -> AfResult {
    if path.is_null() || out_buf.is_null() || out_len.is_null() {
        return AfResult::AfErrInval;
    }
    let path_str = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(_) => return AfResult::AfErrInval,
    };
    let instances = FS_INSTANCES.lock().unwrap();
    let core_ref: &agentfs_core::vfs::FsCore = match instances.get(&fs) {
        Some(c) => c,
        None => return AfResult::AfErrInval,
    };
    let pid_ref = &agentfs_core::PID::new(pid);
    match core_ref.readdir_plus_raw(pid_ref, std::path::Path::new(path_str)) {
        Ok(entries) => {
            let mut offset = 0usize;
            for (name_bytes, _attrs) in entries {
                let need = name_bytes.len() + 1; // plus NUL
                if offset + need > buf_size {
                    break;
                }
                unsafe {
                    ptr::copy_nonoverlapping(
                        name_bytes.as_ptr(),
                        out_buf.add(offset),
                        name_bytes.len(),
                    );
                }
                offset += name_bytes.len();
                unsafe {
                    *out_buf.add(offset) = 0;
                }
                offset += 1;
            }
            unsafe {
                *out_len = offset;
            }
            AfResult::AfOk
        }
        Err(e) => fs_error_to_af_result(&e),
    }
}

/// Xattr get
#[no_mangle]
pub unsafe extern "C" fn af_xattr_get(
    fs: AfFs,
    pid: u32,
    path: *const c_char,
    name: *const c_char,
    out_buf: *mut u8,
    buf_size: usize,
    out_len: *mut usize,
) -> AfResult {
    if path.is_null() || name.is_null() || out_buf.is_null() || out_len.is_null() {
        return AfResult::AfErrInval;
    }
    let path_str = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(_) => return AfResult::AfErrInval,
    };
    let name_str = match unsafe { CStr::from_ptr(name) }.to_str() {
        Ok(s) => s,
        Err(_) => return AfResult::AfErrInval,
    };
    let instances = FS_INSTANCES.lock().unwrap();
    let core = match instances.get(&fs) {
        Some(c) => c,
        None => return AfResult::AfErrInval,
    };
    let pid_ref = &agentfs_core::PID::new(pid);
    match core.xattr_get(pid_ref, std::path::Path::new(path_str), name_str) {
        Ok(val) => {
            let n = val.len().min(buf_size);
            unsafe {
                ptr::copy_nonoverlapping(val.as_ptr(), out_buf, n);
                *out_len = n;
            }
            AfResult::AfOk
        }
        Err(e) => fs_error_to_af_result(&e),
    }
}

/// Xattr set
#[no_mangle]
pub unsafe extern "C" fn af_xattr_set(
    fs: AfFs,
    pid: u32,
    path: *const c_char,
    name: *const c_char,
    value: *const u8,
    value_len: usize,
) -> AfResult {
    if path.is_null() || name.is_null() {
        return AfResult::AfErrInval;
    }
    let path_str = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(_) => return AfResult::AfErrInval,
    };
    let name_str = match unsafe { CStr::from_ptr(name) }.to_str() {
        Ok(s) => s,
        Err(_) => return AfResult::AfErrInval,
    };
    let instances = FS_INSTANCES.lock().unwrap();
    let core = match instances.get(&fs) {
        Some(c) => c,
        None => return AfResult::AfErrInval,
    };
    let pid_ref = &agentfs_core::PID::new(pid);
    let val = if value.is_null() {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(value, value_len).to_vec() }
    };
    match core.xattr_set(pid_ref, std::path::Path::new(path_str), name_str, &val) {
        Ok(()) => AfResult::AfOk,
        Err(e) => fs_error_to_af_result(&e),
    }
}

/// Xattr list - returns NUL-delimited names
#[no_mangle]
pub unsafe extern "C" fn af_xattr_list(
    fs: AfFs,
    pid: u32,
    path: *const c_char,
    out_buf: *mut u8,
    buf_size: usize,
    out_len: *mut usize,
) -> AfResult {
    if path.is_null() || out_buf.is_null() || out_len.is_null() {
        return AfResult::AfErrInval;
    }
    let path_str = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(_) => return AfResult::AfErrInval,
    };
    let instances = FS_INSTANCES.lock().unwrap();
    let core = match instances.get(&fs) {
        Some(c) => c,
        None => return AfResult::AfErrInval,
    };
    let pid_ref = &agentfs_core::PID::new(pid);
    match core.xattr_list(pid_ref, std::path::Path::new(path_str)) {
        Ok(names) => {
            let mut offset = 0usize;
            for n in names {
                let bytes = n.as_bytes();
                let need = bytes.len() + 1;
                if offset + need > buf_size {
                    break;
                }
                unsafe {
                    ptr::copy_nonoverlapping(bytes.as_ptr(), out_buf.add(offset), bytes.len());
                }
                offset += bytes.len();
                unsafe {
                    *out_buf.add(offset) = 0;
                }
                offset += 1;
            }
            unsafe {
                *out_len = offset;
            }
            AfResult::AfOk
        }
        Err(e) => fs_error_to_af_result(&e),
    }
}

/// Remove directory
#[no_mangle]
pub unsafe extern "C" fn af_rmdir(fs: AfFs, pid: u32, path: *const c_char) -> AfResult {
    if path.is_null() {
        return AfResult::AfErrInval;
    }

    let path_str = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(_) => return AfResult::AfErrInval,
    };

    let instances = FS_INSTANCES.lock().unwrap();
    let core = match instances.get(&fs) {
        Some(c) => c,
        None => return AfResult::AfErrInval,
    };

    let pid_ref = &agentfs_core::PID::new(pid);
    match core.rmdir(pid_ref, std::path::Path::new(path_str)) {
        Ok(()) => AfResult::AfOk,
        Err(e) => fs_error_to_af_result(&e),
    }
}

/// Unlink file
#[no_mangle]
pub unsafe extern "C" fn af_unlink(fs: AfFs, pid: u32, path: *const c_char) -> AfResult {
    if path.is_null() {
        return AfResult::AfErrInval;
    }

    let path_str = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(_) => return AfResult::AfErrInval,
    };

    let instances = FS_INSTANCES.lock().unwrap();
    let core = match instances.get(&fs) {
        Some(c) => c,
        None => return AfResult::AfErrInval,
    };

    let pid_ref = &agentfs_core::PID::new(pid);
    match core.unlink(pid_ref, std::path::Path::new(path_str)) {
        Ok(()) => AfResult::AfOk,
        Err(e) => fs_error_to_af_result(&e),
    }
}

/// Create symbolic link
#[no_mangle]
pub unsafe extern "C" fn af_symlink(
    fs: AfFs,
    pid: u32,
    target: *const c_char,
    linkpath: *const c_char,
) -> AfResult {
    if target.is_null() || linkpath.is_null() {
        return AfResult::AfErrInval;
    }

    let target_str = match unsafe { CStr::from_ptr(target) }.to_str() {
        Ok(s) => s,
        Err(_) => return AfResult::AfErrInval,
    };

    let linkpath_str = match unsafe { CStr::from_ptr(linkpath) }.to_str() {
        Ok(s) => s,
        Err(_) => return AfResult::AfErrInval,
    };

    let instances = FS_INSTANCES.lock().unwrap();
    let core = match instances.get(&fs) {
        Some(c) => c,
        None => return AfResult::AfErrInval,
    };

    let pid_ref = &agentfs_core::PID::new(pid);
    match core.symlink(pid_ref, target_str, std::path::Path::new(linkpath_str)) {
        Ok(()) => AfResult::AfOk,
        Err(e) => fs_error_to_af_result(&e),
    }
}

/// Read symbolic link
#[no_mangle]
pub unsafe extern "C" fn af_readlink(
    fs: AfFs,
    pid: u32,
    path: *const c_char,
    out_target: *mut c_char,
    target_size: usize,
) -> AfResult {
    if path.is_null() || out_target.is_null() || target_size == 0 {
        return AfResult::AfErrInval;
    }

    let path_str = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(_) => return AfResult::AfErrInval,
    };

    let instances = FS_INSTANCES.lock().unwrap();
    let core = match instances.get(&fs) {
        Some(c) => c,
        None => return AfResult::AfErrInval,
    };

    let pid_ref = &agentfs_core::PID::new(pid);
    match core.readlink(pid_ref, std::path::Path::new(path_str)) {
        Ok(target) => {
            let target_bytes = target.as_bytes();
            let len = target_bytes.len().min(target_size - 1);

            unsafe {
                ptr::copy_nonoverlapping(target_bytes.as_ptr(), out_target as *mut u8, len);
                *out_target.add(len) = 0; // null terminator
            }

            AfResult::AfOk
        }
        Err(e) => fs_error_to_af_result(&e),
    }
}
