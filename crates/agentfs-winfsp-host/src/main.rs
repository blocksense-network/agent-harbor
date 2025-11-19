// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! AgentFS WinFsp Host — Windows filesystem adapter
//!
//! This binary implements a WinFsp host that mounts AgentFS volumes
//! on Windows using the WinFsp user-mode filesystem framework.

#[cfg(target_os = "windows")]
use agentfs_core::{BranchId, FsCore, OpenOptions, ShareMode};
#[cfg(target_os = "windows")]
use agentfs_proto::{BranchBindRequest, BranchCreateRequest};
use anyhow::Result;
use clap::Parser;
#[cfg(target_os = "windows")]
use std::collections::HashMap;
#[cfg(target_os = "windows")]
use std::ffi::CStr;
use std::path::PathBuf;
#[cfg(target_os = "windows")]
use std::sync::{Arc, Mutex};

#[cfg(target_os = "windows")]
use winfsp::filesystem::{DirBuffer, FileInfo, VolumeInfo};
#[cfg(target_os = "windows")]
use winfsp::winfsp::{CreateOptions, FileAttributes, SecurityDescriptor};
#[cfg(target_os = "windows")]
use winfsp::{FileSystem, FileSystemHost};

#[derive(Parser)]
struct Args {
    /// Drive letter to mount (e.g., X:)
    mount_point: String,

    /// Configuration file (JSON)
    #[arg(short, long)]
    config: Option<PathBuf>,
}

/// File context stored in WinFsp's FileContext pointer
#[derive(Clone)]
#[cfg(target_os = "windows")]
struct FileContext {
    handle_id: agentfs_core::HandleId,
    path: String, // Store path for operations that need it
    branch_id: BranchId,
    pending_delete: bool,
}

/// WinFsp adapter that implements the FSP_FILE_SYSTEM_INTERFACE
#[cfg(target_os = "windows")]
struct AgentFsWinFsp {
    core: Arc<FsCore>,
    volume_label: Mutex<String>,
    #[cfg(target_os = "windows")]
    open_handles: Mutex<HashMap<winfsp::winfsp::FileContext, FileContext>>,
}

#[cfg(target_os = "windows")]
impl AgentFsWinFsp {
    fn new(core: Arc<FsCore>) -> Self {
        Self {
            core,
            volume_label: Mutex::new("AgentFS".to_string()),
            #[cfg(target_os = "windows")]
            open_handles: Mutex::new(HashMap::new()),
        }
    }

    #[cfg(target_os = "windows")]
    fn get_file_context(&self, file_context: winfsp::winfsp::FileContext) -> Option<FileContext> {
        self.open_handles.lock().unwrap().get(&file_context).cloned()
    }

    #[cfg(target_os = "windows")]
    fn set_file_context(&self, file_context: winfsp::winfsp::FileContext, ctx: FileContext) {
        self.open_handles.lock().unwrap().insert(file_context, ctx);
    }

    #[cfg(target_os = "windows")]
    fn remove_file_context(&self, file_context: winfsp::winfsp::FileContext) {
        self.open_handles.lock().unwrap().remove(&file_context);
    }

    fn convert_path(&self, file_name: &CStr) -> Result<String> {
        let path_str = file_name.to_str()?;
        if path_str.is_empty() {
            Ok("/".to_string())
        } else {
            Ok(format!("/{}", path_str.replace('\\', "/")))
        }
    }

    #[cfg(target_os = "windows")]
    fn convert_open_options(
        &self,
        create_options: CreateOptions,
        granted_access: u32,
    ) -> OpenOptions {
        // Map WinFsp access flags to AgentFS options
        let read = (granted_access & winfsp::winfsp::FILE_READ_DATA) != 0;
        let write = (granted_access & winfsp::winfsp::FILE_WRITE_DATA) != 0
            || (granted_access & winfsp::winfsp::FILE_APPEND_DATA) != 0;

        // Map share modes (WinFsp uses inverse logic - 0 means exclusive)
        let share_read = (create_options.share_access & winfsp::winfsp::FILE_SHARE_READ) != 0;
        let share_write = (create_options.share_access & winfsp::winfsp::FILE_SHARE_WRITE) != 0;
        let share_delete = (create_options.share_access & winfsp::winfsp::FILE_SHARE_DELETE) != 0;

        let mut share = Vec::new();
        if share_read {
            share.push(ShareMode::Read);
        }
        if share_write {
            share.push(ShareMode::Write);
        }
        if share_delete {
            share.push(ShareMode::Delete);
        }

        OpenOptions {
            read,
            write,
            create: (create_options.create_options & winfsp::winfsp::FILE_CREATE) != 0
                || (create_options.create_options & winfsp::winfsp::FILE_OPEN_IF) != 0,
            truncate: (create_options.create_options & winfsp::winfsp::FILE_OVERWRITE) != 0
                || (create_options.create_options & winfsp::winfsp::FILE_OVERWRITE_IF) != 0,
            append: (granted_access & winfsp::winfsp::FILE_APPEND_DATA) != 0,
            share,
            stream: None, // Will be set for ADS operations
        }
    }

    #[cfg(target_os = "windows")]
    fn convert_file_info(&self, attrs: &agentfs_core::Attributes) -> FileInfo {
        let mut file_info = FileInfo::default();

        // Set basic attributes
        file_info.file_attributes = if attrs.is_dir {
            FileAttributes::DIRECTORY
        } else {
            FileAttributes::ARCHIVE
        };

        // Set size
        file_info.file_size = attrs.len;

        // Set timestamps (convert from i64 to Windows FILETIME)
        file_info.creation_time = (attrs.times.birthtime as u64 + 116444736000000000) * 10000;
        file_info.last_access_time = (attrs.times.atime as u64 + 116444736000000000) * 10000;
        file_info.last_write_time = (attrs.times.mtime as u64 + 116444736000000000) * 10000;
        file_info.change_time = (attrs.times.ctime as u64 + 116444736000000000) * 10000;

        // Set allocation size (same as file size for simplicity)
        file_info.allocation_size = attrs.len;

        file_info
    }
}

#[cfg(target_os = "windows")]
impl FileSystem for AgentFsWinFsp {
    fn get_volume_info(
        &self,
        _volume_info: &mut VolumeInfo,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let stats = self.core.stats();

        _volume_info.total_size = 1024 * 1024 * 1024; // 1GB for demo
        _volume_info.free_size =
            _volume_info.total_size - (stats.bytes_in_memory + stats.bytes_spilled) as u64;

        // Set volume label
        let label = self.volume_label.lock().unwrap();
        let label_bytes = label.as_bytes();
        let copy_len = std::cmp::min(label_bytes.len(), _volume_info.volume_label.len() - 1);
        _volume_info.volume_label[..copy_len].copy_from_slice(&label_bytes[..copy_len]);
        _volume_info.volume_label[copy_len] = 0;

        Ok(())
    }

    fn set_volume_label(
        &self,
        volume_label: &CStr,
        _volume_info: &mut VolumeInfo,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let label = volume_label.to_str()?.to_string();
        *self.volume_label.lock().unwrap() = label;
        Ok(())
    }

    fn get_security_by_name(
        &self,
        file_name: &CStr,
        _p_file_attributes: Option<&mut FileAttributes>,
        _security_descriptor: Option<&mut SecurityDescriptor>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let path = self.convert_path(file_name)?;
        let _attrs = self.core.getattr(path.as_ref())?;

        // For now, return success with minimal security descriptor
        // TODO: Implement proper security descriptor mapping
        Ok(())
    }

    fn create(
        &self,
        file_name: &CStr,
        create_options: CreateOptions,
        granted_access: u32,
        _file_attributes: FileAttributes,
        _security_descriptor: Option<&SecurityDescriptor>,
        _allocation_size: u64,
        file_context: &mut winfsp::winfsp::FileContext,
        file_info: &mut FileInfo,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let path = self.convert_path(file_name)?;
        let options = self.convert_open_options(create_options, granted_access);

        // Check if it's a directory creation
        let is_dir = (create_options.create_options & winfsp::winfsp::FILE_DIRECTORY_FILE) != 0;

        let handle_id = if is_dir {
            // Directory creation
            self.core.mkdir(path.as_ref(), 0o755)?;
            // For directories, we need to open a handle for readdir operations
            self.core.open(
                path.as_ref(),
                &OpenOptions {
                    read: true,
                    write: false,
                    create: false,
                    truncate: false,
                    append: false,
                    share: vec![ShareMode::Read],
                    stream: None,
                },
            )?
        } else {
            // File creation
            self.core.create(path.as_ref(), &options)?
        };

        // Get attributes for file info
        let attrs = self.core.getattr(path.as_ref())?;
        *file_info = self.convert_file_info(&attrs);

        // Store context
        let context = FileContext {
            handle_id,
            branch_id: self.core.current_branch(),
            pending_delete: false,
        };
        self.set_file_context(*file_context, context);

        Ok(())
    }

    fn open(
        &self,
        file_name: &CStr,
        create_options: CreateOptions,
        granted_access: u32,
        file_context: &mut winfsp::winfsp::FileContext,
        file_info: &mut FileInfo,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let path = self.convert_path(file_name)?;
        let options = self.convert_open_options(create_options, granted_access);

        let handle_id = self.core.open(path.as_ref(), &options)?;

        // Get attributes for file info
        let attrs = self.core.getattr(path.as_ref())?;
        *file_info = self.convert_file_info(&attrs);

        // Store context
        let context = FileContext {
            handle_id,
            branch_id: self.core.current_branch(),
            pending_delete: false,
        };
        self.set_file_context(*file_context, context);

        Ok(())
    }

    fn overwrite(
        &self,
        file_context: winfsp::winfsp::FileContext,
        _file_attributes: FileAttributes,
        _replace_file_attributes: bool,
        _allocation_size: u64,
        file_info: &mut FileInfo,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let ctx = self.get_file_context(file_context).ok_or("Invalid file context")?;

        // Truncate the file
        self.core.truncate(ctx.handle_id, 0)?;

        // Update file info
        let path = "/".to_string(); // TODO: Store path in context for proper getattr
        let attrs = self.core.getattr(path.as_ref())?;
        *file_info = self.convert_file_info(&attrs);

        Ok(())
    }

    fn cleanup(
        &self,
        file_context: winfsp::winfsp::FileContext,
        file_name: Option<&CStr>,
        flags: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut ctx = self.get_file_context(file_context).ok_or("Invalid file context")?;

        // Handle delete-on-close
        if (flags & winfsp::winfsp::FspCleanupDelete) != 0 {
            ctx.pending_delete = true;
            self.set_file_context(file_context, ctx);
        }

        // Handle attribute updates when flags are set
        if let Some(file_name) = file_name {
            let path = self.convert_path(file_name)?;

            // Update timestamps if requested
            if (flags & winfsp::winfsp::FspCleanupSetAllocationSize) != 0
                || (flags & winfsp::winfsp::FspCleanupSetArchive) != 0
                || (flags & winfsp::winfsp::FspCleanupSetLastAccessTime) != 0
                || (flags & winfsp::winfsp::FspCleanupSetLastWriteTime) != 0
                || (flags & winfsp::winfsp::FspCleanupSetChangeTime) != 0
            {
                // For now, just touch the file to update mtime
                let times = agentfs_core::FileTimes {
                    atime: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs() as i64,
                    mtime: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs() as i64,
                    ctime: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs() as i64,
                    birthtime: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs() as i64,
                };
                let _ = self.core.set_times(path.as_ref(), times);
            }
        }

        Ok(())
    }

    fn close(
        &self,
        file_context: winfsp::winfsp::FileContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(ctx) = self.get_file_context(file_context) {
            // Handle pending delete
            if ctx.pending_delete {
                // TODO: Implement proper delete-on-close path tracking
                // For now, we can't unlink without the path, so this is incomplete
            }

            self.core.close(ctx.handle_id)?;
        }

        self.remove_file_context(file_context);
        Ok(())
    }

    fn read(
        &self,
        file_context: winfsp::winfsp::FileContext,
        buffer: &mut [u8],
        offset: u64,
    ) -> Result<u32, Box<dyn std::error::Error>> {
        let ctx = self.get_file_context(file_context).ok_or("Invalid file context")?;

        let bytes_read = self.core.read(ctx.handle_id, offset, buffer)?;
        Ok(bytes_read as u32)
    }

    fn write(
        &self,
        file_context: winfsp::winfsp::FileContext,
        buffer: &[u8],
        offset: u64,
        _write_to_eof: bool,
        _constrained_io: bool,
        file_info: &mut FileInfo,
    ) -> Result<u32, Box<dyn std::error::Error>> {
        let ctx = self.get_file_context(file_context).ok_or("Invalid file context")?;

        let bytes_written = self.core.write(ctx.handle_id, offset, buffer)?;

        // Update file info
        // TODO: Get path from context for proper getattr
        let path = "/".to_string();
        let attrs = self.core.getattr(path.as_ref())?;
        *file_info = self.convert_file_info(&attrs);

        Ok(bytes_written as u32)
    }

    fn flush(
        &self,
        file_context: Option<winfsp::winfsp::FileContext>,
        file_info: Option<&mut FileInfo>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(file_context) = file_context {
            if let Some(ctx) = self.get_file_context(file_context) {
                self.core.flush(ctx.handle_id)?;

                if let Some(file_info) = file_info {
                    // TODO: Update file info
                }
            }
        }
        Ok(())
    }

    fn get_file_info(
        &self,
        file_context: winfsp::winfsp::FileContext,
        file_info: &mut FileInfo,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let ctx = self.get_file_context(file_context).ok_or("Invalid file context")?;

        // TODO: Store path in context to get proper attributes
        // For now, return basic file info
        Ok(())
    }

    fn set_basic_info(
        &self,
        file_context: winfsp::winfsp::FileContext,
        _file_attributes: FileAttributes,
        _creation_time: u64,
        _last_access_time: u64,
        _last_write_time: u64,
        _change_time: u64,
        file_info: &mut FileInfo,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let ctx = self.get_file_context(file_context).ok_or("Invalid file context")?;

        // Convert Windows FILETIME to Unix timestamps
        let times = agentfs_core::FileTimes {
            atime: ((_last_access_time / 10000) - 116444736000000000) as i64,
            mtime: ((_last_write_time / 10000) - 116444736000000000) as i64,
            ctime: ((_change_time / 10000) - 116444736000000000) as i64,
            birthtime: ((_creation_time / 10000) - 116444736000000000) as i64,
        };

        // TODO: Need path to set times
        let path = "/".to_string();
        self.core.set_times(path.as_ref(), times)?;

        // Update file info
        let attrs = self.core.getattr(path.as_ref())?;
        *file_info = self.convert_file_info(&attrs);

        Ok(())
    }

    fn set_file_size(
        &self,
        file_context: winfsp::winfsp::FileContext,
        new_size: u64,
        _set_allocation_size: bool,
        file_info: &mut FileInfo,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let ctx = self.get_file_context(file_context).ok_or("Invalid file context")?;

        self.core.truncate(ctx.handle_id, new_size)?;

        // Update file info
        // TODO: Get path for proper getattr
        Ok(())
    }

    fn can_delete(
        &self,
        file_context: winfsp::winfsp::FileContext,
        file_name: &CStr,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let path = self.convert_path(file_name)?;

        // Check if it's a directory and if it's empty
        let attrs = self.core.getattr(path.as_ref())?;
        if attrs.is_dir {
            let entries = self.core.readdir(path.as_ref())?;
            if !entries.is_empty() {
                return Err("Directory not empty".into());
            }
        }

        Ok(())
    }

    fn rename(
        &self,
        file_context: winfsp::winfsp::FileContext,
        file_name: &CStr,
        new_file_name: &CStr,
        _replace_if_exists: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let old_path = self.convert_path(file_name)?;
        let new_path = self.convert_path(new_file_name)?;

        self.core.rename(old_path.as_ref(), new_path.as_ref(), _replace_if_exists)?;
        Ok(())
    }

    fn get_security(
        &self,
        file_context: winfsp::winfsp::FileContext,
        _security_descriptor: &mut SecurityDescriptor,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // TODO: Implement security descriptor mapping
        Ok(())
    }

    fn set_security(
        &self,
        file_context: winfsp::winfsp::FileContext,
        _security_descriptor: &SecurityDescriptor,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // TODO: Implement security descriptor setting
        Ok(())
    }

    fn read_directory(
        &self,
        file_context: winfsp::winfsp::FileContext,
        pattern: Option<&CStr>,
        marker: Option<&CStr>,
        buffer: &mut DirBuffer,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let ctx = self.get_file_context(file_context).ok_or("Invalid file context")?;

        // TODO: Need to track path in context
        let path = "/".to_string();

        let entries = self.core.readdir(path.as_ref())?;

        for entry in entries {
            let mut file_info = FileInfo::default();
            file_info.file_attributes = if entry.is_dir {
                FileAttributes::DIRECTORY
            } else {
                FileAttributes::ARCHIVE
            };
            file_info.file_size = entry.len;

            buffer.add(&entry.name, &file_info)?;
        }

        Ok(())
    }

    fn get_dir_info_by_name(
        &self,
        file_context: winfsp::winfsp::FileContext,
        file_name: &CStr,
        dir_info: &mut FileInfo,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let path = self.convert_path(file_name)?;
        let attrs = self.core.getattr(path.as_ref())?;
        *dir_info = self.convert_file_info(&attrs);
        Ok(())
    }

    fn control(
        &self,
        _file_context: winfsp::winfsp::FileContext,
        control_code: u32,
        input_buffer: &[u8],
        output_buffer: &mut [u8],
    ) -> Result<u32, Box<dyn std::error::Error>> {
        // Handle AgentFS control messages via DeviceIoControl
        match control_code {
            // TODO: Define proper IOCTL codes for AgentFS operations
            0x80000001 => {
                // AGENTFS_IOCTL_SNAPSHOT_CREATE (not yet implemented on this platform build)
                // Return zero bytes to indicate unimplemented.
                Ok(0)
            }
            0x80000002 => {
                // AGENTFS_IOCTL_BRANCH_CREATE
                let request: BranchCreateRequest = serde_json::from_slice(input_buffer)?;
                let branch_id = self
                    .core
                    .branch_create_from_snapshot(request.from_snapshot, request.name.as_deref())?;
                let response = agentfs_proto::BranchCreateResponse { branch_id };
                let json = serde_json::to_string(&response)?;
                let bytes = json.as_bytes();
                let copy_len = std::cmp::min(bytes.len(), output_buffer.len());
                output_buffer[..copy_len].copy_from_slice(&bytes[..copy_len]);
                Ok(copy_len as u32)
            }
            0x80000003 => {
                // AGENTFS_IOCTL_BRANCH_BIND
                let request: BranchBindRequest = serde_json::from_slice(input_buffer)?;
                let pid = request.pid.unwrap_or_else(|| std::process::id());
                self.core.bind_process_to_branch_with_pid(request.branch_id, pid)?;
                Ok(0)
            }
            _ => Err(format!("Unknown control code: {:#x}", control_code).into()),
        }
    }
}

fn main() -> Result<()> {
    #[cfg(not(target_os = "windows"))]
    {
        tracing::info!("AgentFS WinFsp Host - Windows-only binary");
        tracing::info!("This binary requires Windows with WinFsp installed.");
        tracing::info!("On Windows, it would mount AgentFS volumes using the WinFsp framework.");
        Ok(())
    }

    #[cfg(target_os = "windows")]
    {
        tracing_subscriber::init();

        let args = Args::parse();

        // Create default config
        let config = agentfs_core::FsConfig {
            case_sensitivity: agentfs_core::CaseSensitivity::InsensitivePreserving,
            memory: agentfs_core::MemoryPolicy {
                max_bytes_in_memory: Some(512 * 1024 * 1024), // 512MB
                spill_directory: None,
            },
            limits: agentfs_core::FsLimits {
                max_open_handles: 65536,
                max_branches: 256,
                max_snapshots: 4096,
            },
            cache: agentfs_core::CachePolicy {
                attr_ttl_ms: 1000,
                entry_ttl_ms: 1000,
                negative_ttl_ms: 1000,
                enable_readdir_plus: true,
                auto_cache: true,
                writeback_cache: false,
            },
            enable_xattrs: true,
            enable_ads: true,
            track_events: true,
        };

        let core = Arc::new(FsCore::new(config)?);
        let fs = AgentFsWinFsp::new(core);

        let mut host = FileSystemHost::new()?;
        host.mount(&args.mount_point, fs)?;

        println!("AgentFS mounted on {}", args.mount_point);
        println!("Press Ctrl+C to unmount");

        // Wait for unmount
        host.wait()?;

        Ok(())
    }
}
