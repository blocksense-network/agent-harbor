/*
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

// AgentFSKitFFI.h
// Bridging header for AgentFS Rust FFI functions

#ifndef AGENTFSKITFFI_H
#define AGENTFSKITFFI_H

#include <stdint.h>
#include <stddef.h>

// Core lifecycle functions
void* agentfs_bridge_core_create(void);
void agentfs_bridge_core_destroy(void* core);

// Error handling
size_t agentfs_bridge_get_error_message(char* buffer, size_t buffer_size);

// File system operations
int agentfs_bridge_statfs(void* core, char* buffer, size_t buffer_size);
int agentfs_bridge_getattr(void* core, const char* path, char* buffer, size_t buffer_size);
int agentfs_bridge_mkdir(void* core, uint32_t pid, const char* path, uint32_t mode);
int agentfs_bridge_readdir(void* core, const char* path, char* buffer, size_t buffer_size, size_t* out_len);
int agentfs_bridge_open(void* core, uint32_t pid, const char* path, const char* options, uint64_t* handle);
int agentfs_bridge_open_by_id(void* core, uint32_t pid, uint64_t node_id, const char* options, uint64_t* handle);
int agentfs_bridge_read(void* core, uint32_t pid, uint64_t handle, uint64_t offset, void* buffer, uint32_t length, uint32_t* bytes_read);
int agentfs_bridge_write(void* core, uint32_t pid, uint64_t handle, uint64_t offset, const void* buffer, uint32_t length, uint32_t* bytes_written);
int agentfs_bridge_close(void* core, uint32_t pid, uint64_t handle);
int agentfs_bridge_rename(void* core, const char* old_path, const char* new_path);
int agentfs_bridge_set_times(void* core, const char* path, int64_t atime, int64_t mtime, int64_t ctime, int64_t birthtime);
int agentfs_bridge_set_mode(void* core, const char* path, uint32_t mode);
int agentfs_bridge_set_owner(void* core, const char* path, uint32_t uid, uint32_t gid);

// Control plane operations
int agentfs_bridge_snapshot_create(void* core, const char* name, char* snapshot_id, size_t snapshot_id_size);
int agentfs_bridge_branch_create(void* core, const char* snapshot_id, const char* branch_name, char* branch_id, size_t branch_id_size);
int agentfs_bridge_bind_process(void* core, const char* branch_id);

// Resolve IDs
int af_resolve_id(uint64_t fs, const char* path, uint64_t* out_node_id, uint64_t* out_parent_id);
int agentfs_bridge_create_child_by_id(void* core, uint64_t parent_id, const uint8_t* name_ptr, size_t name_len, uint32_t item_type, uint32_t mode, uint64_t* out_node_id);

// Xattr
int agentfs_bridge_xattr_get(void* core, const char* path, const char* name, void* buffer, size_t buffer_size, size_t* out_len);
int agentfs_bridge_xattr_set(void* core, const char* path, const char* name, const void* value, size_t value_len);
int agentfs_bridge_xattr_list(void* core, const char* path, void* buffer, size_t buffer_size, size_t* out_len);
int af_control_request(uint64_t fs, const uint8_t* request_data, size_t request_len, uint8_t* response_data, size_t response_max_len, size_t* response_actual_len);

#endif // AGENTFSKITFFI_H
