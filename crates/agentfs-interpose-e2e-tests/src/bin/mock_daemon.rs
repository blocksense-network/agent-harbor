// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::io::RawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use libc;

// AgentFS core imports
use agentfs_core::{
    FsCore, HandleId, OpenOptions, PID,
    config::{FsConfig, InterposeConfig},
    error::FsResult,
};

// AgentFS proto imports
use agentfs_proto::*;

// Import specific types that need explicit qualification
use agentfs_proto::messages::{
    DaemonStateFilesystemRequest, DaemonStateProcessesRequest, DaemonStateResponse,
    DaemonStateResponseWrapper, DaemonStateStatsRequest, DirCloseRequest, DirEntry, DirReadRequest,
    FdDupRequest, FilesystemQuery, FilesystemState, FsStats, PathOpRequest, ProcessInfo, StatData,
    StatfsData, TimespecData,
};

// Use handshake types and functions from the main crate
use agentfs_interpose_e2e_tests::handshake::*;
use agentfs_interpose_e2e_tests::{decode_ssz_message, encode_ssz_message};

/// Real AgentFS daemon using the core filesystem
struct AgentFsDaemon {
    core: FsCore,
    processes: HashMap<u32, PID>,       // pid -> registered PID
    opened_files: HashMap<String, u32>, // path -> open count (for testing)
    opened_dirs: HashMap<String, u32>,  // path -> open count (for testing)
}

impl AgentFsDaemon {
    fn new() -> FsResult<Self> {
        Self::new_with_overlay(None, None, None)
    }

    fn new_with_overlay(
        lower_dir: Option<PathBuf>,
        upper_dir: Option<PathBuf>,
        _work_dir: Option<PathBuf>,
    ) -> FsResult<Self> {
        // Configure FsCore based on overlay settings
        let config = if let Some(lower) = lower_dir {
            println!(
                "AgentFsDaemon: configuring overlay with lower={}",
                lower.display()
            );
            FsConfig {
                interpose: InterposeConfig {
                    enabled: true,
                    max_copy_bytes: 64 * 1024 * 1024, // 64MB
                    require_reflink: false,
                    allow_windows_reparse: false,
                },
                overlay: agentfs_core::config::OverlayConfig {
                    enabled: true,
                    lower_root: Some(lower),
                    copyup_mode: agentfs_core::config::CopyUpMode::Lazy,
                },
                ..Default::default()
            }
        } else {
            // Use default FsCore configuration (overlay disabled, in-memory operations)
            // Only linkat/symlinkat operations use direct filesystem calls for e2e test visibility
            FsConfig {
                interpose: InterposeConfig {
                    enabled: true,
                    max_copy_bytes: 64 * 1024 * 1024, // 64MB
                    require_reflink: false,
                    allow_windows_reparse: false,
                },
                ..Default::default()
            }
        };

        let core = FsCore::new(config)?;

        Ok(Self {
            core,
            processes: HashMap::new(),
            opened_files: HashMap::new(),
            opened_dirs: HashMap::new(),
        })
    }

    fn register_process(&mut self, pid: u32, ppid: u32, uid: u32, gid: u32) -> FsResult<PID> {
        let registered_pid = self.core.register_process(pid, ppid, uid, gid);
        self.processes.insert(pid, registered_pid.clone());
        Ok(registered_pid)
    }

    fn get_process_pid(&self, os_pid: u32) -> Option<&PID> {
        self.processes.get(&os_pid)
    }

    fn handle_fd_open(
        &mut self,
        path: String,
        flags: u32,
        mode: u32,
        os_pid: u32,
    ) -> Result<RawFd, String> {
        println!(
            "AgentFsDaemon: fd_open({}, flags={:#x}, mode={:#o}, pid={})",
            path, flags, mode, os_pid
        );

        // For interpose testing, we provide direct access to files in the test directory
        // This simulates what the real AgentFS interpose mode would do - provide direct
        // access to lower filesystem files without overlay semantics

        // Convert flags to libc flags for direct open
        let mut libc_flags = 0;

        if (flags & (libc::O_RDONLY as u32)) != 0 {
            libc_flags |= libc::O_RDONLY;
        }
        if (flags & (libc::O_WRONLY as u32)) != 0 {
            libc_flags |= libc::O_WRONLY;
        }
        if (flags & (libc::O_RDWR as u32)) != 0 {
            libc_flags |= libc::O_RDWR;
        }
        if (flags & (libc::O_CREAT as u32)) != 0 {
            libc_flags |= libc::O_CREAT;
        }
        if (flags & (libc::O_TRUNC as u32)) != 0 {
            libc_flags |= libc::O_TRUNC;
        }
        if (flags & (libc::O_APPEND as u32)) != 0 {
            libc_flags |= libc::O_APPEND;
        }

        // For testing, we expect paths to be relative to a test directory
        // The test will set up files in a known location
        let c_path = std::ffi::CString::new(path.clone())
            .map_err(|e| format!("invalid path '{}': {}", path, e))?;

        // Use libc::open directly to get a real file descriptor
        let fd = unsafe { libc::open(c_path.as_ptr(), libc_flags, mode as libc::c_uint) };

        if fd == -1 {
            let err = std::io::Error::last_os_error();
            Err(format!("libc::open failed for '{}': {}", path, err))
        } else {
            println!("AgentFsDaemon: opened '{}' -> fd {}", path, fd);
            // Record the file open for testing state tracking
            *self.opened_files.entry(path.clone()).or_insert(0) += 1;
            Ok(fd as RawFd)
        }
    }

    fn handle_dir_open(&mut self, path: String, client_pid: u32) -> Result<u64, String> {
        println!("AgentFsDaemon: dir_open({}, pid={})", path, client_pid);

        // Use FsCore to handle the directory open
        let pid = agentfs_core::PID::new(client_pid);
        let path_obj = std::path::Path::new(&path);

        match self.core.opendir(&pid, path_obj) {
            Ok(handle_id) => {
                println!(
                    "AgentFsDaemon: FsCore opendir succeeded for {}, handle_id={}",
                    path, handle_id.0
                );
                // Track the directory access for filesystem state verification
                *self.opened_dirs.entry(path.clone()).or_insert(0) += 1;
                Ok(handle_id.0 as u64) // Return FsCore handle ID
            }
            Err(e) => {
                println!("AgentFsDaemon: FsCore opendir failed for {}: {:?}", path, e);
                Err(format!("opendir failed: {:?}", e))
            }
        }
    }

    fn handle_readlink(&mut self, path: String, client_pid: u32) -> Result<String, String> {
        println!("AgentFsDaemon: readlink({}, pid={})", path, client_pid);

        // Use FsCore to handle the readlink
        let pid = agentfs_core::PID::new(client_pid);
        let path_obj = std::path::Path::new(&path);

        match self.core.readlink(&pid, path_obj) {
            Ok(target) => {
                println!(
                    "AgentFsDaemon: FsCore readlink succeeded for {}, target: {}",
                    path, target
                );
                Ok(target)
            }
            Err(e) => {
                println!(
                    "AgentFsDaemon: FsCore readlink failed for {}: {:?}",
                    path, e
                );
                Err(format!("readlink failed: {:?}", e))
            }
        }
    }

    fn handle_dir_read(&mut self, handle: u64, client_pid: u32) -> Result<Vec<DirEntry>, String> {
        println!(
            "AgentFsDaemon: dir_read(handle={}, pid={})",
            handle, client_pid
        );

        // Use FsCore to read from the directory handle
        let pid = agentfs_core::PID::new(client_pid);
        let handle_id = agentfs_core::HandleId(handle as u64);

        match self.core.readdir(&pid, handle_id) {
            Ok(Some(entry)) => {
                println!(
                    "AgentFsDaemon: dir_read succeeded, got entry: {}",
                    entry.name
                );
                // Convert from agentfs_core::DirEntry to agentfs_proto::messages::DirEntry
                let proto_entry = DirEntry {
                    name: entry.name.into_bytes(),
                    kind: match entry.is_dir {
                        true => 1, // directory
                        false => match entry.is_symlink {
                            true => 2,  // symlink
                            false => 0, // file
                        },
                    },
                };
                Ok(vec![proto_entry])
            }
            Ok(None) => {
                println!("AgentFsDaemon: dir_read reached end of directory");
                Ok(Vec::new()) // End of directory
            }
            Err(e) => {
                println!("AgentFsDaemon: FsCore readdir failed: {:?}", e);
                Err(format!("readdir failed: {:?}", e))
            }
        }
    }

    fn handle_dir_close(&mut self, handle: u64, client_pid: u32) -> Result<(), String> {
        println!(
            "AgentFsDaemon: dir_close(handle={}, pid={})",
            handle, client_pid
        );

        // Use FsCore to close the directory handle
        let pid = agentfs_core::PID::new(client_pid);
        let handle_id = agentfs_core::HandleId(handle as u64);

        match self.core.closedir(&pid, handle_id) {
            Ok(()) => {
                println!("AgentFsDaemon: FsCore closedir succeeded");
                Ok(())
            }
            Err(e) => {
                println!("AgentFsDaemon: FsCore closedir failed: {:?}", e);
                Err(format!("closedir failed: {:?}", e))
            }
        }
    }

    fn handle_fd_dup(&mut self, fd: u32, client_pid: u32) -> Result<u32, String> {
        println!("AgentFsDaemon: fd_dup(fd={}, pid={})", fd, client_pid);

        // For testing purposes, just return the same fd (simulated dup)
        // In a real implementation, we'd need to track file descriptors
        println!("AgentFsDaemon: fd_dup returning same fd (simulated)");
        Ok(fd)
    }

    fn handle_path_op(
        &mut self,
        path: String,
        operation: String,
        args: Option<Vec<u8>>,
        client_pid: u32,
    ) -> Result<Option<String>, String> {
        println!(
            "AgentFsDaemon: path_op(path={}, op={}, pid={})",
            path, operation, client_pid
        );

        // Use FsCore to handle path operations
        let pid = agentfs_core::PID::new(client_pid);
        let path_obj = std::path::Path::new(&path);

        match operation.as_str() {
            "stat" => {
                match self.core.getattr(&pid, path_obj) {
                    Ok(attrs) => {
                        // For testing, return a simple stat result
                        // In a real implementation, we'd serialize the full stat structure
                        println!("AgentFsDaemon: path_op stat succeeded");
                        Ok(Some("stat_result".to_string())) // dummy data
                    }
                    Err(e) => {
                        println!("AgentFsDaemon: path_op stat failed: {:?}", e);
                        Err(format!("stat failed: {:?}", e))
                    }
                }
            }
            _ => {
                println!("AgentFsDaemon: path_op {} not implemented", operation);
                Err(format!("operation {} not implemented", operation))
            }
        }
    }

    /// Get processes state
    fn get_daemon_state_processes(&self) -> Result<DaemonStateResponseWrapper, String> {
        let processes: Vec<agentfs_proto::ProcessInfo> = self
            .processes
            .iter()
            .map(|(os_pid, registered_pid)| agentfs_proto::ProcessInfo {
                os_pid: *os_pid,
                registered_pid: format!("{:?}", registered_pid).into_bytes(),
            })
            .collect();

        Ok(DaemonStateResponseWrapper {
            response: DaemonStateResponse::Processes(processes),
        })
    }

    /// Get stats state
    fn get_daemon_state_stats(&self) -> Result<DaemonStateResponseWrapper, String> {
        let stats = self.core.stats();
        let fs_stats = FsStats {
            branches: stats.branches,
            snapshots: stats.snapshots,
            open_handles: stats.open_handles,
            memory_usage: stats.bytes_in_memory,
        };

        Ok(DaemonStateResponseWrapper {
            response: DaemonStateResponse::Stats(fs_stats),
        })
    }

    /// Get filesystem state
    fn get_daemon_state_filesystem(
        &self,
        query: &FilesystemQuery,
    ) -> Result<DaemonStateResponseWrapper, String> {
        let filesystem_state = self.capture_filesystem_state(query)?;
        Ok(DaemonStateResponseWrapper {
            response: DaemonStateResponse::FilesystemState(filesystem_state),
        })
    }

    /// Capture filesystem state from FsCore instead of real filesystem
    fn capture_filesystem_state(&self, query: &FilesystemQuery) -> Result<FilesystemState, String> {
        let test_pid = agentfs_core::PID::new(12345);
        let mut entries = Vec::new();

        // If include_overlay is true, traverse FsCore's node structure
        if query.include_overlay != 0 {
            println!("AgentFsDaemon: capturing filesystem state from FsCore");

            // Start traversal from root directory "/"
            self.traverse_fscore_tree(&test_pid, std::path::Path::new("/"), query, &mut entries)?;
        }

        // Sort entries by path for binary search
        entries.sort_by(|a, b| a.path.cmp(&b.path));

        Ok(FilesystemState { entries })
    }

    /// Traverse FsCore's node tree and build filesystem entries
    fn traverse_fscore_tree(
        &self,
        pid: &agentfs_core::PID,
        current_path: &std::path::Path,
        query: &FilesystemQuery,
        entries: &mut Vec<FilesystemEntry>,
    ) -> Result<(), String> {
        // Get attributes for the current path
        match self.core.getattr(pid, current_path) {
            Ok(attrs) => {
                let path_str = current_path.to_string_lossy().to_string();

                // Determine file kind based on attributes
                let discriminant = if attrs.is_symlink {
                    2 // Symlink
                } else if attrs.is_dir {
                    1 // Directory
                } else {
                    0 // File
                };

                let mut entry = FilesystemEntry {
                    path: path_str.as_bytes().to_vec(),
                    kind: FileKind { discriminant },
                    size: attrs.len,
                    content: None,
                    target: None,
                };

                // For symlinks, get the target
                if discriminant == 2 {
                    if let Ok(target) = self.core.readlink(pid, current_path) {
                        entry.target = Some(target.as_bytes().to_vec());
                    }
                }

                // For files, get content if not too large
                if discriminant == 0 && attrs.len <= query.max_file_size as u64 {
                    // For now, we can't easily read file content from FsCore without opening handles
                    // This is a limitation of the current implementation
                    // We could potentially read from the storage backend directly
                }

                entries.push(entry);

                // If this is a directory, recurse into children
                if discriminant == 1 {
                    match self.core.readdir_plus(pid, current_path) {
                        Ok(dir_entries) => {
                            for (dir_entry, _) in dir_entries {
                                // Skip "." and ".." entries
                                if dir_entry.name == "." || dir_entry.name == ".." {
                                    continue;
                                }

                                let child_path = current_path.join(&dir_entry.name);
                                self.traverse_fscore_tree(pid, &child_path, query, entries)?;
                            }
                        }
                        Err(e) => {
                            println!(
                                "AgentFsDaemon: failed to read directory {}: {:?}",
                                current_path.display(),
                                e
                            );
                        }
                    }
                }
            }
            Err(e) => {
                println!(
                    "AgentFsDaemon: failed to get attributes for {}: {:?}",
                    current_path.display(),
                    e
                );
            }
        }

        Ok(())
    }

    /// Capture overlay entries recursively (legacy method, kept for compatibility)
    fn capture_overlay_entries(
        &self,
        pid: &agentfs_core::PID,
        current_path: &std::path::Path,
        query: &FilesystemQuery,
        current_depth: u32,
        entries: &mut Vec<FilesystemEntry>,
    ) -> Result<(), String> {
        use agentfs_core::OpenOptions;

        if current_depth >= query.max_depth {
            return Ok(());
        }

        // Try to read the directory
        match self.core.readdir_plus(pid, current_path) {
            Ok(dir_entries) => {
                for (dir_entry, attrs) in dir_entries {
                    let full_path = current_path.join(&dir_entry.name);

                    // Skip "." and ".." entries
                    if dir_entry.name == "." || dir_entry.name == ".." {
                        continue;
                    }

                    let entry = if dir_entry.is_symlink {
                        let target = self.core.readlink(pid, &full_path).map_err(|e| {
                            format!("Failed to read symlink {}: {:?}", full_path.display(), e)
                        })?;

                        FilesystemEntry {
                            path: full_path.to_string_lossy().as_bytes().to_vec(),
                            kind: FileKind { discriminant: 2 }, // Symlink
                            size: 0,
                            content: None,
                            target: Some(target.into_bytes()),
                        }
                    } else if dir_entry.is_dir {
                        // Recursively capture subdirectory
                        self.capture_overlay_entries(
                            pid,
                            &full_path,
                            query,
                            current_depth + 1,
                            entries,
                        )?;

                        FilesystemEntry {
                            path: full_path.to_string_lossy().as_bytes().to_vec(),
                            kind: FileKind { discriminant: 1 }, // Directory
                            size: 0,
                            content: None,
                            target: None,
                        }
                    } else {
                        let mut entry = FilesystemEntry {
                            path: full_path.to_string_lossy().as_bytes().to_vec(),
                            kind: FileKind { discriminant: 0 }, // File
                            size: attrs.len,
                            content: None,
                            target: None,
                        };

                        // Include content if file is small enough
                        if attrs.len <= query.max_file_size as u64 {
                            let open_opts = OpenOptions {
                                read: true,
                                write: false,
                                append: false,
                                truncate: false,
                                create: false,
                                share: vec![],
                                stream: None,
                            };

                            if let Ok(handle_id) = self.core.open(pid, &full_path, &open_opts) {
                                let mut buffer = vec![0u8; attrs.len as usize];
                                if let Ok(bytes_read) =
                                    self.core.read(pid, handle_id, 0, &mut buffer)
                                {
                                    if bytes_read > 0 {
                                        entry.content = Some(buffer[..bytes_read].to_vec());
                                    }
                                }
                            }
                        }

                        entry
                    };

                    entries.push(entry);
                }
                Ok(())
            }
            Err(_) => Ok(()), // Skip directories we can't read
        }
    }

    /// Capture directory structure from the real filesystem (for testing)
    fn capture_directory_from_filesystem(
        &self,
        dir_path: &str,
        query: &FilesystemQuery,
        entries: &mut Vec<FilesystemEntry>,
    ) -> Result<(), String> {
        let path = std::path::Path::new(dir_path);
        if !path.exists() || !path.is_dir() {
            return Ok(());
        }

        // Add the directory itself
        entries.push(FilesystemEntry {
            path: dir_path.as_bytes().to_vec(),
            kind: FileKind { discriminant: 1 }, // Directory
            size: 0,
            content: None,
            target: None,
        });

        // Walk the directory and add files/subdirs
        self.walk_directory_recursive(path, dir_path, query, 0, entries)
    }

    /// Recursively walk a directory and add entries
    fn walk_directory_recursive(
        &self,
        full_path: &std::path::Path,
        relative_path: &str,
        query: &FilesystemQuery,
        depth: u32,
        entries: &mut Vec<FilesystemEntry>,
    ) -> Result<(), String> {
        if depth >= query.max_depth {
            return Ok(());
        }

        match std::fs::read_dir(full_path) {
            Ok(dir_entries) => {
                for entry in dir_entries {
                    if let Ok(entry) = entry {
                        let entry_path = entry.path();
                        let entry_name_owned = entry.file_name().to_string_lossy().to_string();
                        let entry_relative_path = if relative_path == "/" {
                            format!("/{}", entry_name_owned)
                        } else {
                            format!(
                                "{}/{}",
                                relative_path.trim_end_matches('/'),
                                entry_name_owned
                            )
                        };

                        if entry_path.is_dir() {
                            // Add subdirectory
                            entries.push(FilesystemEntry {
                                path: entry_relative_path.as_bytes().to_vec(),
                                kind: FileKind { discriminant: 1 }, // Directory
                                size: 0,
                                content: None,
                                target: None,
                            });

                            // Recurse into subdirectory
                            self.walk_directory_recursive(
                                &entry_path,
                                &entry_relative_path,
                                query,
                                depth + 1,
                                entries,
                            )?;
                        } else if entry_path.is_file() {
                            // Add file with content if small enough
                            if let Ok(metadata) = entry_path.metadata() {
                                let size = metadata.len();
                                let content = if size <= query.max_file_size as u64 {
                                    std::fs::read(&entry_path).ok()
                                } else {
                                    None
                                };

                                entries.push(FilesystemEntry {
                                    path: entry_relative_path.as_bytes().to_vec(),
                                    kind: FileKind { discriminant: 0 }, // File
                                    size,
                                    content,
                                    target: None,
                                });
                            }
                        } else if entry_path.is_symlink() {
                            // Add symlink
                            if let Ok(target) = std::fs::read_link(&entry_path) {
                                entries.push(FilesystemEntry {
                                    path: entry_relative_path.as_bytes().to_vec(),
                                    kind: FileKind { discriminant: 2 }, // Symlink
                                    size: 0,
                                    content: None,
                                    target: Some(target.to_string_lossy().as_bytes().to_vec()),
                                });
                            }
                        }
                    }
                }
                Ok(())
            }
            Err(_) => Ok(()), // Skip directories we can't read
        }
    }

    /// Scan temp directory for test directories and files (for testing)
    fn scan_for_test_dirs_and_files(
        &self,
        temp_path: &std::path::Path,
        query: &FilesystemQuery,
        entries: &mut Vec<FilesystemEntry>,
    ) -> Result<(), String> {
        eprintln!("AgentFsDaemon: ENTERING scan_for_test_dirs_and_files");
        match std::fs::read_dir(temp_path) {
            Ok(dir_entries) => {
                let mut found_dirs = 0;
                for entry in dir_entries {
                    if let Ok(entry) = entry {
                        let path = entry.path();
                        if path.is_dir() {
                            found_dirs += 1;
                            // Check if this looks like a test directory
                            let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                            eprintln!(
                                "AgentFsDaemon: found dir: {} (len: {})",
                                dir_name,
                                dir_name.len()
                            );
                            let starts_with_tmp = dir_name.starts_with(".tmp");
                            let len_check = dir_name.len() >= 10;
                            let is_test_dir = dir_name == "agentfs_readlink_test";
                            eprintln!(
                                "AgentFsDaemon: starts_with .tmp: {}, len >= 10: {}, is_test_dir: {}",
                                starts_with_tmp, len_check, is_test_dir
                            );
                            if (starts_with_tmp && len_check) || is_test_dir {
                                // This looks like a temp directory created by the test
                                eprintln!(
                                    "AgentFsDaemon: CONDITION MET - scanning test temp dir: {}",
                                    dir_name
                                );
                                eprintln!(
                                    "AgentFsDaemon: about to call scan_test_directory for path: {:?}",
                                    path
                                );
                                match self.scan_test_directory(&path, query, entries) {
                                    Ok(_) => eprintln!(
                                        "AgentFsDaemon: scan_test_directory succeeded for {}",
                                        dir_name
                                    ),
                                    Err(e) => eprintln!(
                                        "AgentFsDaemon: scan_test_directory failed for {}: {}",
                                        dir_name, e
                                    ),
                                }
                            }
                        }
                    }
                }
                println!(
                    "AgentFsDaemon: found {} directories in temp dir",
                    found_dirs
                );
                Ok(())
            }
            Err(e) => {
                println!("AgentFsDaemon: failed to read temp dir: {:?}", e);
                Ok(())
            }
        }
    }

    /// Scan a test directory for files and subdirectories
    fn scan_test_directory(
        &self,
        dir_path: &std::path::Path,
        query: &FilesystemQuery,
        entries: &mut Vec<FilesystemEntry>,
    ) -> Result<(), String> {
        let dir_path_str = dir_path.to_string_lossy().to_string();
        eprintln!(
            "AgentFsDaemon: === SCANNING TEST DIRECTORY: {} ===",
            dir_path_str
        );

        // Add the directory itself
        entries.push(FilesystemEntry {
            path: dir_path_str.as_bytes().to_vec(),
            kind: FileKind { discriminant: 1 }, // Directory
            size: 0,
            content: None,
            target: None,
        });

        // Scan contents
        match std::fs::read_dir(dir_path) {
            Ok(contents) => {
                let mut found_files = 0;
                let mut found_symlinks = 0;
                eprintln!("AgentFsDaemon: reading directory: {}", dir_path.display());
                let all_entries: Vec<_> = contents.collect();
                eprintln!(
                    "AgentFsDaemon: directory has {} total entries",
                    all_entries.len()
                );

                for (i, entry) in all_entries.iter().enumerate() {
                    eprintln!("AgentFsDaemon: entry {}: {:?}", i, entry);
                }

                for entry in all_entries {
                    if let Ok(entry) = entry {
                        let path = entry.path();
                        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                        let entry_path_str = path.to_string_lossy().to_string();

                        if path.is_dir() {
                            if file_name == "test_dir" || file_name == "test_files" {
                                // This is a test directory we want to capture
                                println!(
                                    "AgentFsDaemon: found test dir {}, capturing it",
                                    file_name
                                );
                                self.capture_directory_from_filesystem(
                                    &entry_path_str,
                                    query,
                                    entries,
                                )?;
                            } else {
                                // Add the directory itself
                                entries.push(FilesystemEntry {
                                    path: entry_path_str.as_bytes().to_vec(),
                                    kind: FileKind { discriminant: 1 }, // Directory
                                    size: 0,
                                    content: None,
                                    target: None,
                                });
                            }
                        } else if path.is_symlink() {
                            // Add symlinks (check this before is_file since symlinks can return true for is_file)
                            found_symlinks += 1;
                            eprintln!("AgentFsDaemon: found symlink: {}", entry_path_str);
                            if let Ok(target) = std::fs::read_link(&path) {
                                eprintln!("AgentFsDaemon: symlink target: {}", target.display());
                                entries.push(FilesystemEntry {
                                    path: entry_path_str.as_bytes().to_vec(),
                                    kind: FileKind { discriminant: 2 }, // Symlink
                                    size: 0,
                                    content: None,
                                    target: Some(target.to_string_lossy().as_bytes().to_vec()),
                                });
                            }
                        } else if path.is_file() {
                            // Add regular files
                            found_files += 1;
                            eprintln!("AgentFsDaemon: found file: {}", entry_path_str);
                            if let Ok(metadata) = path.metadata() {
                                let size = metadata.len();
                                let content = if size <= query.max_file_size as u64 {
                                    std::fs::read(&path).ok()
                                } else {
                                    None
                                };
                                entries.push(FilesystemEntry {
                                    path: entry_path_str.as_bytes().to_vec(),
                                    kind: FileKind { discriminant: 0 }, // File
                                    size,
                                    content,
                                    target: None,
                                });
                            }
                        }
                    }
                }
                eprintln!(
                    "AgentFsDaemon: scan_test_directory found {} files, {} symlinks in {}",
                    found_files,
                    found_symlinks,
                    dir_path.display()
                );
                Ok(())
            }
            Err(e) => {
                eprintln!(
                    "AgentFsDaemon: failed to read directory {}: {:?}",
                    dir_path.display(),
                    e
                );
                Ok(())
            }
        }
    }

    fn handle_dirfd_open_dir(&mut self, pid: u32, path: String, fd: i32) -> Result<(), String> {
        println!(
            "AgentFsDaemon: dirfd_open_dir(pid={}, path={}, fd={})",
            pid, path, fd
        );

        // Register the dirfd mapping with FsCore
        self.core
            .register_process_dirfd_mapping(pid)
            .map_err(|e| format!("Failed to register process mapping: {}", e))?;

        self.core
            .open_dir_fd(pid, PathBuf::from(path), fd as std::os::fd::RawFd)
            .map_err(|e| format!("Failed to register dirfd: {}", e))
    }

    fn handle_dirfd_close_fd(&mut self, pid: u32, fd: i32) -> Result<(), String> {
        println!("AgentFsDaemon: dirfd_close_fd(pid={}, fd={})", pid, fd);

        self.core
            .close_fd(pid, fd as std::os::fd::RawFd)
            .map_err(|e| format!("Failed to close dirfd: {}", e))
    }

    fn handle_dirfd_dup_fd(&mut self, pid: u32, old_fd: i32, new_fd: i32) -> Result<(), String> {
        println!(
            "AgentFsDaemon: dirfd_dup_fd(pid={}, old_fd={}, new_fd={})",
            pid, old_fd, new_fd
        );

        self.core
            .dup_fd(
                pid,
                old_fd as std::os::fd::RawFd,
                new_fd as std::os::fd::RawFd,
            )
            .map_err(|e| format!("Failed to dup dirfd: {}", e))
    }

    fn handle_dirfd_set_cwd(&mut self, pid: u32, cwd: String) -> Result<(), String> {
        println!("AgentFsDaemon: dirfd_set_cwd(pid={}, cwd={})", pid, cwd);

        self.core
            .set_process_cwd(pid, PathBuf::from(cwd))
            .map_err(|e| format!("Failed to set cwd: {}", e))
    }

    fn handle_dirfd_resolve_path(
        &mut self,
        pid: u32,
        dirfd: i32,
        relative_path: String,
    ) -> Result<String, String> {
        println!(
            "AgentFsDaemon: dirfd_resolve_path(pid={}, dirfd={}, relative_path={})",
            pid, dirfd, relative_path
        );

        let resolved_path = self
            .core
            .resolve_path_with_dirfd(pid, dirfd, Path::new(&relative_path))
            .map_err(|e| format!("Failed to resolve path: {}", e))?;

        Ok(resolved_path.to_string_lossy().to_string())
    }
}

fn handle_client(mut stream: UnixStream, daemon: Arc<Mutex<AgentFsDaemon>>, client_pid: u32) {
    // Register the process with the daemon
    {
        let mut daemon = daemon.lock().unwrap();
        if let Err(e) = daemon.register_process(client_pid, 0, 0, 0) {
            return;
        }
    }

    // Handle handshake
    let mut len_buf = [0u8; 4];
    if stream.read_exact(&mut len_buf).is_err() {
        return;
    }

    let msg_len = u32::from_le_bytes(len_buf) as usize;
    let mut msg_buf = vec![0u8; msg_len];

    if stream.read_exact(&mut msg_buf).is_err() {
        return;
    }

    if let Ok(handshake) = decode_ssz_message::<HandshakeMessage>(&msg_buf) {
        // Send back a simple text acknowledgment
        let ack = b"OK\n";
        let _ = stream.write_all(ack);
        let _ = stream.flush();
    } else {
        return;
    }

    // Handle one request
    let mut len_buf = [0u8; 4];
    if stream.read_exact(&mut len_buf).is_err() {
        return;
    }

    let msg_len = u32::from_le_bytes(len_buf) as usize;

    let mut msg_buf = vec![0u8; msg_len];
    if stream.read_exact(&mut msg_buf).is_err() {
        return;
    }

    // Try to decode as regular request
    match decode_ssz_message::<Request>(&msg_buf) {
        Ok(request) => {
            match request {
                Request::FdOpen((version, fd_open_req)) => {
                    let path = String::from_utf8_lossy(&fd_open_req.path).to_string();
                    let mut daemon = daemon.lock().unwrap();
                    match daemon.handle_fd_open(
                        path,
                        fd_open_req.flags,
                        fd_open_req.mode,
                        client_pid,
                    ) {
                        Ok(fd) => {
                            // For now, send a simple success response with the fd number
                            // TODO: Implement proper SCM_RIGHTS
                            let response = Response::fd_open(fd as u32);
                            send_response(&mut stream, &response);
                            // Close our copy of the fd
                            unsafe {
                                libc::close(fd);
                            }
                        }
                        Err(e) => {
                            let response =
                                Response::error(format!("fd_open failed: {}", e), Some(2));
                            send_response(&mut stream, &response);
                        }
                    }
                }
                Request::DirOpen((version, dir_open_req)) => {
                    let path = String::from_utf8_lossy(&dir_open_req.path).to_string();
                    let mut daemon = daemon.lock().unwrap();
                    match daemon.handle_dir_open(path, client_pid) {
                        Ok(handle) => {
                            let response = Response::dir_open(handle);
                            send_response(&mut stream, &response);
                        }
                        Err(e) => {
                            let response =
                                Response::error(format!("dir_open failed: {}", e), Some(2));
                            send_response(&mut stream, &response);
                        }
                    }
                }
                Request::DaemonStateProcesses(DaemonStateProcessesRequest { data: version }) => {
                    let daemon = daemon.lock().unwrap();
                    match daemon.get_daemon_state_processes() {
                        Ok(response) => {
                            let response = Response::DaemonState(response);
                            send_response(&mut stream, &response);
                        }
                        Err(e) => {
                            let response = Response::error(
                                format!("daemon_state_processes failed: {}", e),
                                Some(4),
                            );
                            send_response(&mut stream, &response);
                        }
                    }
                }
                Request::DaemonStateStats(DaemonStateStatsRequest { data: version }) => {
                    let daemon = daemon.lock().unwrap();
                    match daemon.get_daemon_state_stats() {
                        Ok(response) => {
                            let response = Response::DaemonState(response);
                            send_response(&mut stream, &response);
                        }
                        Err(e) => {
                            let response = Response::error(
                                format!("daemon_state_stats failed: {}", e),
                                Some(4),
                            );
                            send_response(&mut stream, &response);
                        }
                    }
                }
                Request::Readlink((version, readlink_req)) => {
                    let path = String::from_utf8_lossy(&readlink_req.path).to_string();
                    println!("AgentFsDaemon: readlink({}, pid={})", path, client_pid);
                    let mut daemon = daemon.lock().unwrap();
                    match daemon.handle_readlink(path, client_pid) {
                        Ok(target) => {
                            println!("AgentFsDaemon: readlink succeeded, target: {}", target);
                            let response = Response::readlink(target);
                            send_response(&mut stream, &response);
                        }
                        Err(e) => {
                            println!("AgentFsDaemon: readlink failed: {}", e);
                            let response =
                                Response::error(format!("readlink failed: {}", e), Some(2));
                            send_response(&mut stream, &response);
                        }
                    }
                }
                Request::DirRead((version, dir_read_req)) => {
                    let handle = dir_read_req.handle;
                    println!("AgentFsDaemon: dir_read(handle={})", handle);
                    let mut daemon = daemon.lock().unwrap();
                    match daemon.handle_dir_read(handle, client_pid) {
                        Ok(entries) => {
                            println!(
                                "AgentFsDaemon: dir_read succeeded, {} entries",
                                entries.len()
                            );
                            let response = Response::dir_read(entries);
                            send_response(&mut stream, &response);
                        }
                        Err(e) => {
                            println!("AgentFsDaemon: dir_read failed: {}", e);
                            let response =
                                Response::error(format!("dir_read failed: {}", e), Some(3));
                            send_response(&mut stream, &response);
                        }
                    }
                }
                Request::DirClose((version, dir_close_req)) => {
                    let handle = dir_close_req.handle;
                    println!("AgentFsDaemon: dir_close(handle={})", handle);
                    let mut daemon = daemon.lock().unwrap();
                    match daemon.handle_dir_close(handle, client_pid) {
                        Ok(()) => {
                            println!("AgentFsDaemon: dir_close succeeded");
                            let response = Response::dir_close();
                            send_response(&mut stream, &response);
                        }
                        Err(e) => {
                            println!("AgentFsDaemon: dir_close failed: {}", e);
                            let response =
                                Response::error(format!("dir_close failed: {}", e), Some(3));
                            send_response(&mut stream, &response);
                        }
                    }
                }
                Request::FdDup((version, fd_dup_req)) => {
                    let fd = fd_dup_req.fd;
                    println!("AgentFsDaemon: fd_dup(fd={})", fd);
                    let mut daemon = daemon.lock().unwrap();
                    match daemon.handle_fd_dup(fd, client_pid) {
                        Ok(duped_fd) => {
                            println!("AgentFsDaemon: fd_dup succeeded, new fd: {}", duped_fd);
                            let response = Response::fd_dup(duped_fd);
                            send_response(&mut stream, &response);
                        }
                        Err(e) => {
                            println!("AgentFsDaemon: fd_dup failed: {}", e);
                            let response =
                                Response::error(format!("fd_dup failed: {}", e), Some(2));
                            send_response(&mut stream, &response);
                        }
                    }
                }
                Request::PathOp((version, path_op_req)) => {
                    let path = String::from_utf8_lossy(&path_op_req.path).to_string();
                    let operation = String::from_utf8_lossy(&path_op_req.operation).to_string();
                    println!("AgentFsDaemon: path_op(path={}, op={})", path, operation);
                    let mut daemon = daemon.lock().unwrap();
                    match daemon.handle_path_op(path, operation, path_op_req.args, client_pid) {
                        Ok(result) => {
                            println!("AgentFsDaemon: path_op succeeded");
                            let response = Response::path_op(result);
                            send_response(&mut stream, &response);
                        }
                        Err(e) => {
                            println!("AgentFsDaemon: path_op failed: {}", e);
                            let response =
                                Response::error(format!("path_op failed: {}", e), Some(4));
                            send_response(&mut stream, &response);
                        }
                    }
                }
                Request::DaemonStateFilesystem(DaemonStateFilesystemRequest { query }) => {
                    println!(
                        "AgentFsDaemon: processing filesystem state query with max_depth={}, include_overlay={}, max_file_size={}",
                        query.max_depth, query.include_overlay, query.max_file_size
                    );
                    let daemon = daemon.lock().unwrap();
                    match daemon.get_daemon_state_filesystem(&query) {
                        Ok(response) => {
                            let entry_count = match &response.response {
                                DaemonStateResponse::FilesystemState(filesystem_state) => {
                                    filesystem_state.entries.len()
                                }
                                _ => 0,
                            };
                            println!(
                                "AgentFsDaemon: filesystem state query successful, {} entries",
                                entry_count
                            );
                            let response = Response::DaemonState(response);
                            send_response(&mut stream, &response);
                        }
                        Err(e) => {
                            println!("AgentFsDaemon: filesystem state query failed: {}", e);
                            let response = Response::error(
                                format!("daemon_state_filesystem failed: {}", e),
                                Some(4),
                            );
                            send_response(&mut stream, &response);
                        }
                    }
                }
                // Metadata operations
                Request::Stat((version, stat_req)) => {
                    let path = String::from_utf8_lossy(&stat_req.path).to_string();
                    let daemon = daemon.lock().unwrap();
                    match daemon.core.stat(&daemon.processes[&client_pid], path.as_ref()) {
                        Ok(stat_data) => {
                            let response = Response::stat(stat_data);
                            send_response(&mut stream, &response);
                        }
                        Err(e) => {
                            let response = Response::error(format!("stat failed: {}", e), Some(2));
                            send_response(&mut stream, &response);
                        }
                    }
                }
                Request::Lstat((version, lstat_req)) => {
                    let path = String::from_utf8_lossy(&lstat_req.path).to_string();
                    let daemon = daemon.lock().unwrap();
                    match daemon.core.lstat(&daemon.processes[&client_pid], path.as_ref()) {
                        Ok(stat_data) => {
                            let response = Response::lstat(stat_data);
                            send_response(&mut stream, &response);
                        }
                        Err(e) => {
                            let response = Response::error(format!("lstat failed: {}", e), Some(2));
                            send_response(&mut stream, &response);
                        }
                    }
                }
                Request::Fstat((version, fstat_req)) => {
                    let daemon = daemon.lock().unwrap();
                    let handle_id = HandleId(fstat_req.fd as u64);
                    match daemon.core.fstat(&daemon.processes[&client_pid], handle_id) {
                        Ok(stat_data) => {
                            let response = Response::fstat(stat_data);
                            send_response(&mut stream, &response);
                        }
                        Err(e) => {
                            let response = Response::error(format!("fstat failed: {}", e), Some(2));
                            send_response(&mut stream, &response);
                        }
                    }
                }
                Request::Fstatat((version, fstatat_req)) => {
                    let daemon = daemon.lock().unwrap();
                    let path = String::from_utf8_lossy(&fstatat_req.path).to_string();
                    match daemon.core.fstatat(
                        &daemon.processes[&client_pid],
                        path.as_ref(),
                        fstatat_req.flags,
                    ) {
                        Ok(stat_data) => {
                            let response = Response::fstatat(stat_data);
                            send_response(&mut stream, &response);
                        }
                        Err(e) => {
                            let response =
                                Response::error(format!("fstatat failed: {}", e), Some(2));
                            send_response(&mut stream, &response);
                        }
                    }
                }
                Request::Chmod((version, chmod_req)) => {
                    let path = String::from_utf8_lossy(&chmod_req.path).to_string();
                    let daemon = daemon.lock().unwrap();
                    match daemon.core.set_mode(
                        &daemon.processes[&client_pid],
                        path.as_ref(),
                        chmod_req.mode,
                    ) {
                        Ok(()) => {
                            let response = Response::chmod();
                            send_response(&mut stream, &response);
                        }
                        Err(e) => {
                            let response = Response::error(format!("chmod failed: {}", e), Some(2));
                            send_response(&mut stream, &response);
                        }
                    }
                }
                Request::Fchmod((version, fchmod_req)) => {
                    let daemon = daemon.lock().unwrap();
                    let handle_id = HandleId(fchmod_req.fd as u64);
                    match daemon.core.fchmod(
                        &daemon.processes[&client_pid],
                        handle_id,
                        fchmod_req.mode,
                    ) {
                        Ok(()) => {
                            let response = Response::fchmod();
                            send_response(&mut stream, &response);
                        }
                        Err(e) => {
                            let response =
                                Response::error(format!("fchmod failed: {}", e), Some(2));
                            send_response(&mut stream, &response);
                        }
                    }
                }
                Request::Fchmodat((version, fchmodat_req)) => {
                    let daemon = daemon.lock().unwrap();
                    let path = String::from_utf8_lossy(&fchmodat_req.path).to_string();
                    match daemon.core.fchmodat(
                        &daemon.processes[&client_pid],
                        path.as_ref(),
                        fchmodat_req.mode,
                        fchmodat_req.flags,
                    ) {
                        Ok(()) => {
                            let response = Response::fchmodat();
                            send_response(&mut stream, &response);
                        }
                        Err(e) => {
                            let response =
                                Response::error(format!("fchmodat failed: {}", e), Some(2));
                            send_response(&mut stream, &response);
                        }
                    }
                }
                Request::Chown((version, chown_req)) => {
                    let path = String::from_utf8_lossy(&chown_req.path).to_string();
                    let daemon = daemon.lock().unwrap();
                    match daemon.core.set_owner(
                        &daemon.processes[&client_pid],
                        path.as_ref(),
                        chown_req.uid,
                        chown_req.gid,
                    ) {
                        Ok(()) => {
                            let response = Response::chown();
                            send_response(&mut stream, &response);
                        }
                        Err(e) => {
                            let response = Response::error(format!("chown failed: {}", e), Some(2));
                            send_response(&mut stream, &response);
                        }
                    }
                }
                Request::Lchown((version, lchown_req)) => {
                    let path = String::from_utf8_lossy(&lchown_req.path).to_string();
                    let daemon = daemon.lock().unwrap();
                    // For now, use regular chown (lchown would be different for symlinks)
                    match daemon.core.set_owner(
                        &daemon.processes[&client_pid],
                        path.as_ref(),
                        lchown_req.uid,
                        lchown_req.gid,
                    ) {
                        Ok(()) => {
                            let response = Response::lchown();
                            send_response(&mut stream, &response);
                        }
                        Err(e) => {
                            let response =
                                Response::error(format!("lchown failed: {}", e), Some(2));
                            send_response(&mut stream, &response);
                        }
                    }
                }
                Request::Fchown((version, fchown_req)) => {
                    let daemon = daemon.lock().unwrap();
                    let handle_id = HandleId(fchown_req.fd as u64);
                    match daemon.core.fchown(
                        &daemon.processes[&client_pid],
                        handle_id,
                        fchown_req.uid,
                        fchown_req.gid,
                    ) {
                        Ok(()) => {
                            let response = Response::fchown();
                            send_response(&mut stream, &response);
                        }
                        Err(e) => {
                            let response =
                                Response::error(format!("fchown failed: {}", e), Some(2));
                            send_response(&mut stream, &response);
                        }
                    }
                }
                Request::Fchownat((version, fchownat_req)) => {
                    let daemon = daemon.lock().unwrap();
                    let path = String::from_utf8_lossy(&fchownat_req.path).to_string();
                    match daemon.core.fchownat(
                        &daemon.processes[&client_pid],
                        path.as_ref(),
                        fchownat_req.uid,
                        fchownat_req.gid,
                        fchownat_req.flags,
                    ) {
                        Ok(()) => {
                            let response = Response::fchownat();
                            send_response(&mut stream, &response);
                        }
                        Err(e) => {
                            let response =
                                Response::error(format!("fchownat failed: {}", e), Some(2));
                            send_response(&mut stream, &response);
                        }
                    }
                }
                Request::Utimes((version, utimes_req)) => {
                    let path = String::from_utf8_lossy(&utimes_req.path).to_string();
                    let daemon = daemon.lock().unwrap();
                    // For path-based utimes, we need to open the file first
                    match daemon.core.create(
                        &daemon.processes[&client_pid],
                        path.as_ref(),
                        &OpenOptions {
                            read: true,
                            write: false,
                            create: false,
                            truncate: false,
                            append: false,
                            share: vec![],
                            stream: None,
                        },
                    ) {
                        Ok(handle) => {
                            let times = utimes_req.times.map(|t| (t.0, t.1));
                            match daemon.core.futimes(&daemon.processes[&client_pid], handle, times)
                            {
                                Ok(()) => {
                                    daemon.core.close(&daemon.processes[&client_pid], handle).ok();
                                    let response = Response::utimes();
                                    send_response(&mut stream, &response);
                                }
                                Err(e) => {
                                    daemon.core.close(&daemon.processes[&client_pid], handle).ok();
                                    let response =
                                        Response::error(format!("utimes failed: {}", e), Some(2));
                                    send_response(&mut stream, &response);
                                }
                            }
                        }
                        Err(e) => {
                            let response = Response::error(
                                format!("utimes failed to open file: {}", e),
                                Some(2),
                            );
                            send_response(&mut stream, &response);
                        }
                    }
                }
                Request::Futimes((version, futimes_req)) => {
                    let daemon = daemon.lock().unwrap();
                    let handle_id = HandleId(futimes_req.fd as u64);
                    let times = futimes_req.times.map(|t| (t.0, t.1));
                    match daemon.core.futimes(&daemon.processes[&client_pid], handle_id, times) {
                        Ok(()) => {
                            let response = Response::futimes();
                            send_response(&mut stream, &response);
                        }
                        Err(e) => {
                            let response =
                                Response::error(format!("futimes failed: {}", e), Some(2));
                            send_response(&mut stream, &response);
                        }
                    }
                }
                Request::Utimensat((version, utimensat_req)) => {
                    let daemon = daemon.lock().unwrap();
                    let path = String::from_utf8_lossy(&utimensat_req.path).to_string();
                    match daemon.core.utimensat(
                        &daemon.processes[&client_pid],
                        path.as_ref(),
                        utimensat_req.times,
                        utimensat_req.flags,
                    ) {
                        Ok(()) => {
                            let response = Response::utimensat();
                            send_response(&mut stream, &response);
                        }
                        Err(e) => {
                            let response =
                                Response::error(format!("utimensat failed: {}", e), Some(2));
                            send_response(&mut stream, &response);
                        }
                    }
                }
                Request::Linkat((version, linkat_req)) => {
                    let daemon = daemon.lock().unwrap();
                    let old_path = String::from_utf8_lossy(&linkat_req.old_path).to_string();
                    let new_path = String::from_utf8_lossy(&linkat_req.new_path).to_string();
                    match daemon.core.linkat(
                        &daemon.processes[&client_pid],
                        old_path.as_ref(),
                        new_path.as_ref(),
                        linkat_req.flags,
                    ) {
                        Ok(()) => {
                            let response = Response::linkat();
                            send_response(&mut stream, &response);
                        }
                        Err(e) => {
                            let response =
                                Response::error(format!("linkat failed: {}", e), Some(2));
                            send_response(&mut stream, &response);
                        }
                    }
                }
                Request::Symlinkat((version, symlinkat_req)) => {
                    let daemon = daemon.lock().unwrap();
                    let target = String::from_utf8_lossy(&symlinkat_req.target).to_string();
                    let linkpath = String::from_utf8_lossy(&symlinkat_req.linkpath).to_string();
                    match daemon.core.symlinkat(
                        &daemon.processes[&client_pid],
                        &target,
                        linkpath.as_ref(),
                    ) {
                        Ok(()) => {
                            let response = Response::symlinkat();
                            send_response(&mut stream, &response);
                        }
                        Err(e) => {
                            let response =
                                Response::error(format!("symlinkat failed: {}", e), Some(2));
                            send_response(&mut stream, &response);
                        }
                    }
                }
                Request::Futimens((version, futimens_req)) => {
                    let daemon = daemon.lock().unwrap();
                    let handle_id = HandleId(futimens_req.fd as u64);
                    let times = futimens_req.times.map(|t| (t.0, t.1));
                    match daemon.core.futimens(&daemon.processes[&client_pid], handle_id, times) {
                        Ok(()) => {
                            let response = Response::futimens();
                            send_response(&mut stream, &response);
                        }
                        Err(e) => {
                            let response =
                                Response::error(format!("futimens failed: {}", e), Some(2));
                            send_response(&mut stream, &response);
                        }
                    }
                }
                Request::Truncate((version, truncate_req)) => {
                    let path = String::from_utf8_lossy(&truncate_req.path).to_string();
                    let daemon = daemon.lock().unwrap();
                    // For path-based truncate, we need to open the file first
                    match daemon.core.create(
                        &daemon.processes[&client_pid],
                        path.as_ref(),
                        &OpenOptions {
                            read: true,
                            write: true,
                            create: false,
                            truncate: false,
                            append: false,
                            share: vec![],
                            stream: None,
                        },
                    ) {
                        Ok(handle) => {
                            match daemon.core.ftruncate(
                                &daemon.processes[&client_pid],
                                handle,
                                truncate_req.length,
                            ) {
                                Ok(()) => {
                                    daemon.core.close(&daemon.processes[&client_pid], handle).ok();
                                    let response = Response::truncate();
                                    send_response(&mut stream, &response);
                                }
                                Err(e) => {
                                    daemon.core.close(&daemon.processes[&client_pid], handle).ok();
                                    let response =
                                        Response::error(format!("truncate failed: {}", e), Some(2));
                                    send_response(&mut stream, &response);
                                }
                            }
                        }
                        Err(e) => {
                            let response = Response::error(
                                format!("truncate failed to open file: {}", e),
                                Some(2),
                            );
                            send_response(&mut stream, &response);
                        }
                    }
                }
                Request::Ftruncate((version, ftruncate_req)) => {
                    let daemon = daemon.lock().unwrap();
                    let handle_id = HandleId(ftruncate_req.fd as u64);
                    match daemon.core.ftruncate(
                        &daemon.processes[&client_pid],
                        handle_id,
                        ftruncate_req.length,
                    ) {
                        Ok(()) => {
                            let response = Response::ftruncate();
                            send_response(&mut stream, &response);
                        }
                        Err(e) => {
                            let response =
                                Response::error(format!("ftruncate failed: {}", e), Some(2));
                            send_response(&mut stream, &response);
                        }
                    }
                }
                Request::Statfs((version, statfs_req)) => {
                    let path = String::from_utf8_lossy(&statfs_req.path).to_string();
                    let daemon = daemon.lock().unwrap();
                    match daemon.core.statfs(&daemon.processes[&client_pid], path.as_ref()) {
                        Ok(statfs_data) => {
                            let response = Response::statfs(statfs_data);
                            send_response(&mut stream, &response);
                        }
                        Err(e) => {
                            let response =
                                Response::error(format!("statfs failed: {}", e), Some(2));
                            send_response(&mut stream, &response);
                        }
                    }
                }
                Request::Fstatfs((version, fstatfs_req)) => {
                    let daemon = daemon.lock().unwrap();
                    let handle_id = HandleId(fstatfs_req.fd as u64);
                    match daemon.core.fstatfs(&daemon.processes[&client_pid], handle_id) {
                        Ok(statfs_data) => {
                            let response = Response::fstatfs(statfs_data);
                            send_response(&mut stream, &response);
                        }
                        Err(e) => {
                            let response =
                                Response::error(format!("fstatfs failed: {}", e), Some(2));
                            send_response(&mut stream, &response);
                        }
                    }
                }
                Request::DirfdOpenDir((version, dirfd_open_dir_req)) => {
                    let path = String::from_utf8_lossy(&dirfd_open_dir_req.path).to_string();
                    let mut daemon = daemon.lock().unwrap();
                    match daemon.handle_dirfd_open_dir(
                        dirfd_open_dir_req.pid,
                        path,
                        dirfd_open_dir_req.fd as std::os::fd::RawFd,
                    ) {
                        Ok(()) => {
                            let response = Response::dirfd_open_dir();
                            send_response(&mut stream, &response);
                        }
                        Err(e) => {
                            let response =
                                Response::error(format!("dirfd_open_dir failed: {}", e), Some(2));
                            send_response(&mut stream, &response);
                        }
                    }
                }
                Request::DirfdCloseFd((version, dirfd_close_fd_req)) => {
                    let mut daemon = daemon.lock().unwrap();
                    match daemon.handle_dirfd_close_fd(
                        dirfd_close_fd_req.pid,
                        dirfd_close_fd_req.fd as std::os::fd::RawFd,
                    ) {
                        Ok(()) => {
                            let response = Response::dirfd_close_fd();
                            send_response(&mut stream, &response);
                        }
                        Err(e) => {
                            let response =
                                Response::error(format!("dirfd_close_fd failed: {}", e), Some(2));
                            send_response(&mut stream, &response);
                        }
                    }
                }
                Request::DirfdDupFd((version, dirfd_dup_fd_req)) => {
                    let mut daemon = daemon.lock().unwrap();
                    match daemon.handle_dirfd_dup_fd(
                        dirfd_dup_fd_req.pid,
                        dirfd_dup_fd_req.old_fd as std::os::fd::RawFd,
                        dirfd_dup_fd_req.new_fd as std::os::fd::RawFd,
                    ) {
                        Ok(()) => {
                            let response = Response::dirfd_dup_fd();
                            send_response(&mut stream, &response);
                        }
                        Err(e) => {
                            let response =
                                Response::error(format!("dirfd_dup_fd failed: {}", e), Some(2));
                            send_response(&mut stream, &response);
                        }
                    }
                }
                Request::DirfdSetCwd((version, dirfd_set_cwd_req)) => {
                    let cwd = String::from_utf8_lossy(&dirfd_set_cwd_req.cwd).to_string();
                    let mut daemon = daemon.lock().unwrap();
                    match daemon.handle_dirfd_set_cwd(dirfd_set_cwd_req.pid, cwd) {
                        Ok(()) => {
                            let response = Response::dirfd_set_cwd();
                            send_response(&mut stream, &response);
                        }
                        Err(e) => {
                            let response =
                                Response::error(format!("dirfd_set_cwd failed: {}", e), Some(2));
                            send_response(&mut stream, &response);
                        }
                    }
                }
                Request::DirfdResolvePath((version, dirfd_resolve_path_req)) => {
                    let relative_path =
                        String::from_utf8_lossy(&dirfd_resolve_path_req.relative_path).to_string();
                    let mut daemon = daemon.lock().unwrap();
                    match daemon.handle_dirfd_resolve_path(
                        dirfd_resolve_path_req.pid,
                        dirfd_resolve_path_req.dirfd as std::os::fd::RawFd,
                        relative_path,
                    ) {
                        Ok(resolved_path) => {
                            let response = Response::dirfd_resolve_path(resolved_path);
                            send_response(&mut stream, &response);
                        }
                        Err(e) => {
                            let response = Response::error(
                                format!("dirfd_resolve_path failed: {}", e),
                                Some(2),
                            );
                            send_response(&mut stream, &response);
                        }
                    }
                }
                _ => {
                    let response = Response::error("unsupported request".to_string(), Some(3));
                    send_response(&mut stream, &response);
                }
            }
        }
        Err(e) => {}
    }
}

fn send_fd_via_scmsg(stream: &UnixStream, fd: RawFd) -> Result<(), String> {
    use libc::{
        CMSG_DATA, CMSG_FIRSTHDR, CMSG_LEN, CMSG_SPACE, SCM_RIGHTS, SOL_SOCKET, c_int, cmsghdr,
        iovec, msghdr,
    };

    // Create a dummy message (we're only sending the fd)
    let dummy_data = [0u8; 1];
    let mut iov = iovec {
        iov_base: dummy_data.as_ptr() as *mut libc::c_void,
        iov_len: dummy_data.len(),
    };

    let mut msg: msghdr = unsafe { std::mem::zeroed() };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;

    let cmsg_space =
        unsafe { libc::CMSG_SPACE(std::mem::size_of::<RawFd>() as libc::c_uint) } as usize;
    let mut cmsg_buf = vec![0u8; cmsg_space];
    msg.msg_control = cmsg_buf.as_mut_ptr() as *mut libc::c_void;
    #[cfg(target_os = "linux")]
    {
        msg.msg_controllen = cmsg_buf.len() as usize;
    }
    #[cfg(target_os = "macos")]
    {
        msg.msg_controllen = cmsg_buf.len() as libc::c_uint;
    }

    let cmsg = unsafe { libc::CMSG_FIRSTHDR(&msg) };
    if cmsg.is_null() {
        return Err("failed to get control message header".to_string());
    }

    unsafe {
        #[cfg(target_os = "linux")]
        {
            (*cmsg).cmsg_len = std::mem::size_of::<libc::cmsghdr>() + std::mem::size_of::<RawFd>();
        }
        #[cfg(target_os = "macos")]
        {
            (*cmsg).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<RawFd>() as libc::c_uint);
        }
        (*cmsg).cmsg_level = libc::SOL_SOCKET;
        (*cmsg).cmsg_type = libc::SCM_RIGHTS;
        *(libc::CMSG_DATA(cmsg) as *mut RawFd) = fd;
    }

    let result = unsafe { libc::sendmsg(stream.as_raw_fd(), &msg, 0) };
    if result < 0 {
        return Err(format!(
            "sendmsg failed: {}",
            std::io::Error::last_os_error()
        ));
    }

    Ok(())
}

fn send_response(stream: &mut UnixStream, response: &Response) {
    let encoded = encode_ssz_message(&response);
    let len_bytes = (encoded.len() as u32).to_le_bytes();

    let _ = stream.write_all(&len_bytes);
    let _ = stream.write_all(&encoded);
    let _ = stream.flush();
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!(
            "Usage: {} <socket_path> [--lower-dir <path>] [--upper-dir <path>] [--work-dir <path>]",
            args[0]
        );
        std::process::exit(1);
    }

    let socket_path = &args[1];

    // Parse overlay arguments
    let mut lower_dir = None;
    let mut upper_dir = None;
    let mut work_dir = None;

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--lower-dir" => {
                if i + 1 < args.len() {
                    lower_dir = Some(PathBuf::from(&args[i + 1]));
                    i += 2;
                } else {
                    eprintln!("--lower-dir requires an argument");
                    std::process::exit(1);
                }
            }
            "--upper-dir" => {
                if i + 1 < args.len() {
                    upper_dir = Some(PathBuf::from(&args[i + 1]));
                    i += 2;
                } else {
                    eprintln!("--upper-dir requires an argument");
                    std::process::exit(1);
                }
            }
            "--work-dir" => {
                if i + 1 < args.len() {
                    work_dir = Some(PathBuf::from(&args[i + 1]));
                    i += 2;
                } else {
                    eprintln!("--work-dir requires an argument");
                    std::process::exit(1);
                }
            }
            _ => {
                eprintln!("Unknown argument: {}", args[i]);
                std::process::exit(1);
            }
        }
    }

    // Clean up any existing socket
    let _ = std::fs::remove_file(socket_path);

    let listener = UnixListener::bind(socket_path).expect("failed to bind socket");
    println!("AgentFsDaemon: listening on {}", socket_path);

    let daemon = match AgentFsDaemon::new_with_overlay(lower_dir, upper_dir, work_dir) {
        Ok(daemon) => Arc::new(Mutex::new(daemon)),
        Err(e) => {
            eprintln!("Failed to create AgentFS daemon: {:?}", e);
            std::process::exit(1);
        }
    };

    println!("AgentFsDaemon: initialized successfully");

    // Handle incoming connections
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                // For testing, we'll use a dummy client PID since we don't have a way to get it from the Unix socket
                // In production, this would need to be passed through the handshake or connection
                let client_pid = 12345; // Dummy PID for testing
                let daemon_clone = daemon.clone();
                thread::spawn(move || {
                    handle_client(stream, daemon_clone, client_pid);
                });
            }
            Err(e) => {
                eprintln!("AgentFsDaemon: accept error: {}", e);
                break;
            }
        }
    }

    println!("AgentFsDaemon: shutting down");
}
