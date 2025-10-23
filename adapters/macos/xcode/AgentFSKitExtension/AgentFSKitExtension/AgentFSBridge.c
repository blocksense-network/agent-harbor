/*
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

// AgentFSBridge.c
// C wrapper for AgentFS Rust FFI functions to ensure proper C linkage

#include "AgentFSKitFFI.h"
#include <stdint.h>

// Forward declarations of Rust FFI functions
extern int32_t af_fs_create(const char* config_json, uint64_t* out_fs);
extern int32_t af_fs_destroy(uint64_t fs);
extern int32_t af_mkdir(uint64_t fs, uint32_t pid, const char* path, uint32_t mode);
extern int32_t af_snapshot_create(uint64_t fs, const char* name, uint8_t* out_id);
extern int32_t af_branch_create_from_snapshot(uint64_t fs, const uint8_t* snap, const char* name, uint8_t* out_id);
extern int32_t af_bind_process_to_branch(uint64_t fs, const uint8_t* branch);
extern int32_t af_getattr(uint64_t fs, const char* path, uint8_t* out_attrs, size_t attrs_size);
extern int32_t af_set_times(uint64_t fs, const char* path, int64_t atime, int64_t mtime, int64_t ctime, int64_t birthtime);
extern int32_t af_set_mode(uint64_t fs, const char* path, uint32_t mode);
extern int32_t af_set_owner(uint64_t fs, const char* path, uint32_t uid, uint32_t gid);
extern int32_t af_rename(uint64_t fs, const char* old_path, const char* new_path);
extern int32_t af_readdir(uint64_t fs, const char* path, uint8_t* out_buf, size_t buf_size, size_t* out_len);
extern int32_t af_open(uint64_t fs, uint32_t pid, const char* path, const char* options_json, uint64_t* out_h);
extern int32_t af_open_by_id(uint64_t fs, uint32_t pid, uint64_t node_id, const char* options_json, uint64_t* out_h);
extern int32_t af_read(uint64_t fs, uint32_t pid, uint64_t h, uint64_t off, uint8_t* buf, uint32_t len, uint32_t* out_read);
extern int32_t af_write(uint64_t fs, uint32_t pid, uint64_t h, uint64_t off, const uint8_t* buf, uint32_t len, uint32_t* out_written);
extern int32_t af_close(uint64_t fs, uint32_t pid, uint64_t h);
extern int32_t af_unlink(uint64_t fs, const char* path);
extern int32_t af_rmdir(uint64_t fs, const char* path);
extern int32_t af_symlink(uint64_t fs, const char* target, const char* linkpath);
extern int32_t af_readlink(uint64_t fs, const char* path, char* out_target, size_t target_size);
extern int32_t af_xattr_get(uint64_t fs, const char* path, const char* name, uint8_t* out_buf, size_t buf_size, size_t* out_len);
extern int32_t af_xattr_set(uint64_t fs, const char* path, const char* name, const uint8_t* value, size_t value_len);
extern int32_t af_xattr_list(uint64_t fs, const char* path, uint8_t* out_buf, size_t buf_size, size_t* out_len);
extern int32_t af_resolve_id(uint64_t fs, const char* path, uint64_t* out_node_id, uint64_t* out_parent_id);
extern int32_t af_create_child_by_id(uint64_t fs, uint64_t parent_id, const uint8_t* name_ptr, size_t name_len, uint32_t item_type, uint32_t mode, uint64_t* out_node_id);

// C wrapper implementations
void* agentfs_bridge_core_create(void) {
    const char* config = "{\"max_memory_bytes\": 67108864, \"max_open_handles\": 1024, \"max_branches\": 10, \"max_snapshots\": 10}";
    uint64_t handle = 0;

    if (af_fs_create(config, &handle) == 0) {
        return (void*)handle;
    }
    return NULL;
}

void agentfs_bridge_core_destroy(void* core) {
    if (core) {
        uint64_t handle = (uint64_t)core;
        af_fs_destroy(handle);
    }
}

size_t agentfs_bridge_get_error_message(char* buffer, size_t buffer_size) {
    // Simple implementation - return empty for now
    if (buffer && buffer_size > 0) {
        buffer[0] = '\0';
    }
    return 0;
}

int agentfs_bridge_getattr(void* core, const char* path, char* buffer, size_t buffer_size) {
    if (!core || !path || !buffer) return -22; // EINVAL
    return af_getattr((uint64_t)core, path, (uint8_t*)buffer, buffer_size);
}

int agentfs_bridge_set_times(void* core, const char* path, int64_t atime, int64_t mtime, int64_t ctime, int64_t birthtime) {
    if (!core || !path) return -22;
    return af_set_times((uint64_t)core, path, atime, mtime, ctime, birthtime);
}

int agentfs_bridge_set_mode(void* core, const char* path, uint32_t mode) {
    if (!core || !path) return -22;
    return af_set_mode((uint64_t)core, path, mode);
}

int agentfs_bridge_set_owner(void* core, const char* path, uint32_t uid, uint32_t gid) {
    if (!core || !path) return -22;
    return af_set_owner((uint64_t)core, path, uid, gid);
}

int agentfs_bridge_rename(void* core, const char* old_path, const char* new_path) {
    if (!core || !old_path || !new_path) return -22;
    return af_rename((uint64_t)core, old_path, new_path);
}

int agentfs_bridge_readdir(void* core, const char* path, char* buffer, size_t buffer_size, size_t* out_len) {
    if (!core || !path || !buffer || !out_len) return -22;
    return af_readdir((uint64_t)core, path, (uint8_t*)buffer, buffer_size, out_len);
}

int agentfs_bridge_open(void* core, uint32_t pid, const char* path, const char* options, uint64_t* handle) {
    if (!core || !path || !options || !handle) return -22;
    return af_open((uint64_t)core, pid, path, options, handle);
}

int agentfs_bridge_open_by_id(void* core, uint32_t pid, uint64_t node_id, const char* options, uint64_t* handle) {
    if (!core || !options || !handle) return -22;
    return af_open_by_id((uint64_t)core, pid, node_id, options, handle);
}

int agentfs_bridge_read(void* core, uint32_t pid, uint64_t handle, uint64_t offset, void* buffer, uint32_t length, uint32_t* bytes_read) {
    if (!core || !buffer || !bytes_read) return -22;
    return af_read((uint64_t)core, pid, handle, offset, (uint8_t*)buffer, length, bytes_read);
}

int agentfs_bridge_write(void* core, uint32_t pid, uint64_t handle, uint64_t offset, const void* buffer, uint32_t length, uint32_t* bytes_written) {
    if (!core || !buffer || !bytes_written) return -22;
    return af_write((uint64_t)core, pid, handle, offset, (const uint8_t*)buffer, length, bytes_written);
}

int agentfs_bridge_close(void* core, uint32_t pid, uint64_t handle) {
    if (!core) return -22;
    return af_close((uint64_t)core, pid, handle);
}

int agentfs_bridge_statfs(void* core, char* buffer, size_t buffer_size) {
    (void)core; (void)buffer; (void)buffer_size; return 0; // TODO: implement if needed
}

int agentfs_bridge_resolve_id(void* core, const char* path, uint64_t* out_node_id, uint64_t* out_parent_id) {
    if (!core || !path || !out_node_id) return -22;
    return af_resolve_id((uint64_t)core, path, out_node_id, out_parent_id);
}

int agentfs_bridge_create_child_by_id(void* core, uint64_t parent_id, const uint8_t* name_ptr, size_t name_len, uint32_t item_type, uint32_t mode, uint64_t* out_node_id) {
    if (!core || !name_ptr || !out_node_id) return -22;
    return af_create_child_by_id((uint64_t)core, parent_id, name_ptr, name_len, item_type, mode, out_node_id);
}

int agentfs_bridge_mkdir(void* core, uint32_t pid, const char* path, uint32_t mode) {
    if (!core || !path) return -22;
    return af_mkdir((uint64_t)core, pid, path, mode);
}

int agentfs_bridge_unlink(void* core, const char* path) {
    if (!core || !path) return -22;
    return af_unlink((uint64_t)core, path);
}

int agentfs_bridge_rmdir(void* core, const char* path) {
    if (!core || !path) return -22;
    return af_rmdir((uint64_t)core, path);
}

int agentfs_bridge_symlink(void* core, const char* target, const char* linkpath) {
    if (!core || !target || !linkpath) return -22;
    return af_symlink((uint64_t)core, target, linkpath);
}

int agentfs_bridge_readlink(void* core, const char* path, char* buffer, size_t buffer_size) {
    if (!core || !path || !buffer) return -22;
    return af_readlink((uint64_t)core, path, buffer, buffer_size);
}

int agentfs_bridge_xattr_get(void* core, const char* path, const char* name, void* buffer, size_t buffer_size, size_t* out_len) {
    if (!core || !path || !name || !buffer || !out_len) return -22;
    return af_xattr_get((uint64_t)core, path, name, (uint8_t*)buffer, buffer_size, out_len);
}

int agentfs_bridge_xattr_set(void* core, const char* path, const char* name, const void* value, size_t value_len) {
    if (!core || !path || !name) return -22;
    return af_xattr_set((uint64_t)core, path, name, (const uint8_t*)value, value_len);
}

int agentfs_bridge_xattr_list(void* core, const char* path, void* buffer, size_t buffer_size, size_t* out_len) {
    if (!core || !path || !buffer || !out_len) return -22;
    return af_xattr_list((uint64_t)core, path, (uint8_t*)buffer, buffer_size, out_len);
}
