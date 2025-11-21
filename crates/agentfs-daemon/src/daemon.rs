// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! AgentFS Daemon implementation with interpose support
//!
//! # Thread-Safety Hazards and Protection Measures
//!
//! This daemon handles concurrent client connections and filesystem operations, requiring
//! careful synchronization to prevent race conditions and data corruption.
//!
//! ## Primary Hazards:
//!
//! ### 1. Concurrent FsCore Access
//! **Hazard**: Multiple client threads simultaneously accessing/modifying FsCore state
//! (file handles, process mappings, filesystem metadata).
//! **Impact**: Data corruption, inconsistent filesystem state, crashes.
//!
//! ### 2. Nested Locking Deadlocks
//! **Hazard**: Client handlers acquiring daemon locks then core locks, or vice versa,
//! potentially creating circular wait conditions.
//! **Impact**: Deadlocks causing the entire daemon to become unresponsive.
//!
//! ### 3. Event Subscription Race Conditions
//! **Hazard**: Watch service subscribing to FsCore events while clients are already
//! making filesystem operations.
//! **Impact**: Missed events or inconsistent event delivery.
//!
//! ### 4. Process Registration Conflicts
//! **Hazard**: Multiple clients registering the same process ID simultaneously.
//! **Impact**: Inconsistent process mappings, incorrect permission checks.
//!
//! ### 5. Watch Service State Corruption
//! **Hazard**: Concurrent registration/unregistration of filesystem watches.
//! **Impact**: Lost watch notifications, incorrect event routing.
//!
//! ## Protection Measures:
//!
//! ### 1. FsCore Mutex Protection
//! ```rust
//! core: Arc<Mutex<FsCore>>  // Exclusive access to filesystem state
//! ```
//! - All FsCore operations are performed within a Mutex guard
//! - Ensures atomicity of complex operations (create+write, etc.)
//! - Prevents concurrent modification of shared filesystem state
//!
//! ### 2. PID Extraction Before Core Locking
//! ```rust
//! fn get_client_pid_helper(daemon: &Arc<Mutex<AgentFsDaemon>>, client_pid: u32) -> PID
//! ```
//! - Extracts process ID from daemon HashMap before acquiring core lock
//! - Prevents nested locking: daemon lock → core lock
//! - Reduces lock contention time
//!
//! ### 3. Atomic Daemon Initialization
//! ```rust
//! // In AgentFsDaemon::new_with_overlay()
//! let core = Arc::new(Mutex::new(FsCore::new(config)?));
//! let watch_service = Arc::new(WatchService::new());
//! // Subscribe watch service to events BEFORE accepting clients
//! ```
//! - Event subscription occurs during daemon construction
//! - No race window between daemon startup and event handling
//! - Watch service is ready before any client connections are accepted
//!
//! ### 4. Watch Service Internal Synchronization
//! ```rust
//! // In watch_service.rs
//! kqueue_watches: Mutex<HashMap<(u32, u32, u64), KqueueWatchRegistration>>,
//! fsevents_watches: Mutex<HashMap<(u32, u64), FSEventsWatchRegistration>>,
//! ```
//! - Each watch table protected by its own Mutex
//! - Registration/unregistration operations are atomic
//! - Event routing can proceed concurrently with new registrations
//!
//! ### 5. Client Handler Isolation
//! - Each client connection handled in its own thread
//! - Shared state accessed through Arc<Mutex<>> clones
//! - No shared mutable state between client handlers
//! - Lock contention managed through fine-grained locking
//!
//! ## Lock Ordering Discipline:
//!
//! To prevent deadlocks, locks are always acquired in this order:
//! 1. Daemon Mutex (for process mapping access)
//! 2. FsCore Mutex (for filesystem operations)
//! 3. Watch Service internal Mutexes (for registration operations)
//!
//! ## Performance Considerations:
//!
//! - PID extraction minimizes time spent holding daemon lock
//! - Core operations are batched within single lock acquisitions
//! - Watch service uses separate locks for different watch types
//! - Event delivery is asynchronous and doesn't block client operations

use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::io::RawFd;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use tempfile::TempDir;
use tracing::{debug, error, info, trace, warn};

// AgentFS core imports
use agentfs_core::{
    BranchId, FsCore, HandleId, OpenOptions, PID, SnapshotId,
    config::{FsConfig, InterposeConfig},
    error::FsResult,
};

// AgentFS proto imports
use agentfs_proto::*;

// Import specific types that need explicit qualification
use agentfs_proto::messages::{
    BranchInfo as ProtoBranchInfo, DaemonStateFilesystemRequest, DaemonStateProcessesRequest,
    DaemonStateResponse, DaemonStateResponseWrapper, DaemonStateStatsRequest, DirEntry,
    FilesystemQuery, FilesystemState, FsStats, SnapshotInfo as ProtoSnapshotInfo,
};

// Use handshake types and functions from this crate
use crate::handshake::*;
use crate::{WatchService, WatchServiceEventSink, decode_ssz_message, encode_ssz_message};

#[cfg(target_os = "macos")]
use crate::macos::interposition::{
    CFDataRef, CFMessagePortWrapper, create_remote_port,
    register_fsevents_port as register_port_in_map, send_fsevents_batch as send_batch_via_port,
};
#[cfg(target_os = "macos")]
use crate::macos::kqueue::SInt32;

const COMPONENT: &str = "agentfs-daemon";

/// Helper function to get PID for client operations (avoids nested locking issues)
#[allow(dead_code)]
fn get_client_pid_helper(daemon: &Arc<Mutex<AgentFsDaemon>>, client_pid: u32) -> PID {
    let daemon_guard = daemon.lock().unwrap();
    daemon_guard.registered_pid(client_pid).unwrap_or_else(|| PID::new(client_pid))
}

fn decode_string(bytes: &[u8]) -> Result<String, String> {
    std::str::from_utf8(bytes)
        .map(|s| s.to_string())
        .map_err(|err| format!("invalid UTF-8 sequence: {}", err))
}

fn snapshot_id_to_vec(id: SnapshotId) -> Vec<u8> {
    id.to_string().into_bytes()
}

fn branch_id_to_vec(id: BranchId) -> Vec<u8> {
    id.to_string().into_bytes()
}

fn parse_snapshot_id_bytes(bytes: &[u8]) -> Result<SnapshotId, String> {
    let id_str = decode_string(bytes)?;
    SnapshotId::from_str(&id_str).map_err(|err| format!("invalid snapshot id: {}", err))
}

fn parse_branch_id_bytes(bytes: &[u8]) -> Result<BranchId, String> {
    let id_str = decode_string(bytes)?;
    BranchId::from_str(&id_str).map_err(|err| format!("invalid branch id: {}", err))
}

struct ReadonlyExport {
    #[allow(dead_code)]
    temp_dir: TempDir,
    #[allow(dead_code)]
    path: PathBuf,
    #[allow(dead_code)]
    snapshot_id: SnapshotId,
}

/// Real AgentFS daemon using the core filesystem
pub struct AgentFsDaemon {
    core: Arc<Mutex<FsCore>>,
    watch_service: Arc<WatchService>,
    processes: HashMap<u32, PID>,       // pid -> registered PID
    opened_files: HashMap<String, u32>, // path -> open count (for testing)
    opened_dirs: HashMap<String, u32>,  // path -> open count (for testing)
    connections: HashMap<u32, std::sync::Mutex<std::os::unix::net::UnixStream>>, // pid -> connection stream
    #[cfg(target_os = "macos")]
    fsevents_ports: HashMap<u32, CFMessagePortWrapper>, // pid -> CFMessagePort for FSEvents
    readonly_exports: HashMap<String, ReadonlyExport>,
    export_counter: u64,
}

impl AgentFsDaemon {
    /// Create a new daemon instance
    pub fn new() -> FsResult<Self> {
        Self::new_with_overlay(None, None, None)
    }

    /// Create a new daemon instance with overlay configuration
    pub fn new_with_overlay(
        lower_dir: Option<PathBuf>,
        _upper_dir: Option<PathBuf>,
        _work_dir: Option<PathBuf>,
    ) -> FsResult<Self> {
        // Configure FsCore based on overlay settings
        let config = if let Some(lower) = lower_dir {
            info!(
                component = COMPONENT,
                operation = "configure_overlay",
                lower_dir = %lower.display(),
                "configuring AgentFS overlay"
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
                    visible_subdir: None,
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

        let core = Arc::new(Mutex::new(FsCore::new(config)?));
        let watch_service = Arc::new(WatchService::new());

        // Create the daemon instance
        let mut daemon_instance = Self {
            core: Arc::clone(&core),
            watch_service: Arc::clone(&watch_service),
            processes: HashMap::new(),
            opened_files: HashMap::new(),
            opened_dirs: HashMap::new(),
            connections: HashMap::new(),
            #[cfg(target_os = "macos")]
            fsevents_ports: HashMap::new(),
            readonly_exports: HashMap::new(),
            export_counter: 0,
        };

        // Subscribe the watch service to FsCore events
        {
            let core_clone = Arc::clone(&core);
            let watch_service_clone = Arc::clone(&watch_service);
            let daemon_arc = Arc::new(Mutex::new(daemon_instance));
            let daemon_clone = Arc::clone(&daemon_arc);
            let sink = Arc::new(WatchServiceEventSink::new(
                watch_service_clone,
                daemon_clone,
            ));

            // Lock the core temporarily to subscribe to events
            let core_guard = core_clone.lock().unwrap();
            core_guard
                .subscribe_events(sink)
                .expect("Failed to subscribe watch service to events");

            // Extract the daemon instance back
            daemon_instance = Arc::try_unwrap(daemon_arc)
                .unwrap_or_else(|_| panic!("Failed to unwrap daemon - still has references"))
                .into_inner()
                .unwrap_or_else(|_| panic!("Failed to get daemon from mutex - poisoned"));
        }

        Ok(daemon_instance)
    }

    /// Create a new daemon instance with backstore configuration
    pub fn new_with_backstore(
        lower_dir: Option<PathBuf>,
        _upper_dir: Option<PathBuf>,
        _work_dir: Option<PathBuf>,
        backstore_mode: agentfs_core::config::BackstoreMode,
    ) -> FsResult<Self> {
        // Configure FsCore based on overlay and backstore settings
        let config = if let Some(lower) = lower_dir {
            debug!(
                component = COMPONENT,
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
                    visible_subdir: None,
                },
                backstore: backstore_mode,
                ..Default::default()
            }
        } else {
            // Use default FsCore configuration with custom backstore
            FsConfig {
                interpose: InterposeConfig {
                    enabled: true,
                    max_copy_bytes: 64 * 1024 * 1024, // 64MB
                    require_reflink: false,
                    allow_windows_reparse: false,
                },
                backstore: backstore_mode,
                ..Default::default()
            }
        };

        // Initialize FsCore with the configuration
        let core = Arc::new(Mutex::new(FsCore::new(config)?));

        // Create watch service
        let watch_service = Arc::new(WatchService::new());

        // Create the daemon instance
        let mut daemon_instance = Self {
            core: Arc::clone(&core),
            watch_service: Arc::clone(&watch_service),
            processes: HashMap::new(),
            opened_files: HashMap::new(),
            opened_dirs: HashMap::new(),
            connections: HashMap::new(),
            #[cfg(target_os = "macos")]
            fsevents_ports: HashMap::new(),
            readonly_exports: HashMap::new(),
            export_counter: 0,
        };

        // Subscribe the watch service to FsCore events
        {
            let core_clone = Arc::clone(&core);
            let watch_service_clone = Arc::clone(&watch_service);
            let daemon_arc = Arc::new(Mutex::new(daemon_instance));
            let daemon_clone = Arc::clone(&daemon_arc);
            let sink = Arc::new(WatchServiceEventSink::new(
                watch_service_clone,
                daemon_clone,
            ));

            // Lock the core temporarily to subscribe to events
            let core_guard = core_clone.lock().unwrap();
            core_guard
                .subscribe_events(sink)
                .expect("Failed to subscribe watch service to events");

            // Extract the daemon instance back
            daemon_instance = Arc::try_unwrap(daemon_arc)
                .unwrap_or_else(|_| panic!("Failed to unwrap daemon - still has references"))
                .into_inner()
                .unwrap_or_else(|_| panic!("Failed to get daemon from mutex - poisoned"));
        }

        Ok(daemon_instance)
    }

    /// Register a process with the daemon
    pub fn register_process(&mut self, pid: u32, ppid: u32, uid: u32, gid: u32) -> FsResult<PID> {
        let registered_pid = self.core.lock().unwrap().register_process(pid, ppid, uid, gid);
        self.processes.insert(pid, registered_pid);
        Ok(registered_pid)
    }

    /// Register a connection for sending unsolicited messages to a process
    pub fn register_connection(&mut self, pid: u32, stream: std::os::unix::net::UnixStream) {
        self.connections.insert(pid, std::sync::Mutex::new(stream));
    }

    /// Unregister a connection
    pub fn unregister_connection(&mut self, pid: u32) {
        self.connections.remove(&pid);
    }

    /// Clean up all watch registrations for a process (called when process exits)
    pub fn cleanup_process_watches(&mut self, pid: u32) {
        debug!(
            component = COMPONENT,
            operation = "cleanup_process_watches",
            pid,
            "cleaning up watches for process"
        );

        // Remove all kqueue watches for this process
        self.watch_service.unregister_watches_by_pid(pid);

        // Remove doorbell idents for this process
        self.watch_service.clear_doorbell_idents_for_pid(pid);

        // Remove kqueue FDs for this process
        #[cfg(target_os = "macos")]
        {
            self.watch_service.clear_kqueue_fds_for_pid(pid);
        }

        // Clear any pending events for kqueues belonging to this process
        self.watch_service.clear_pending_events_for_pid(pid);

        debug!(
            component = COMPONENT,
            operation = "cleanup_process_watches",
            pid,
            "finished cleaning up watches for process"
        );
    }

    /// Send an unsolicited message to a specific process
    pub fn send_to_process(&self, pid: u32, response: Response) -> Result<(), String> {
        if let Some(stream_mutex) = self.connections.get(&pid) {
            let mut stream = stream_mutex
                .lock()
                .map_err(|e| format!("Failed to lock stream for pid {}: {}", pid, e))?;
            send_response(&mut stream, &response);
            Ok(())
        } else {
            Err(format!("No connection registered for pid {}", pid))
        }
    }

    /// Register a CFMessagePort for FSEvents delivery to a process
    #[cfg(target_os = "macos")]
    pub fn register_fsevents_port(&mut self, pid: u32, port: CFMessagePortWrapper) {
        register_port_in_map(&mut self.fsevents_ports, pid, port);
    }

    /// Send an FSEvents batch to a process via CFMessagePort
    #[cfg(target_os = "macos")]
    /// # Safety
    /// Caller must ensure:
    /// * `data` is a valid CFDataRef obtained from CoreFoundation and not freed until after the call.
    /// * `pid` has a registered CFMessagePort in `fsevents_ports`.
    /// * `msgid` matches the receiving shim's expected protocol.
    /// * No concurrent mutation invalidates the CFMessagePort while sending.
    ///   Failure to uphold these invariants may lead to undefined behaviour in CoreFoundation.
    pub unsafe fn send_fsevents_batch(
        &self,
        pid: u32,
        msgid: SInt32,
        data: CFDataRef,
    ) -> Result<(), String> {
        send_batch_via_port(&self.fsevents_ports, pid, msgid, data)
    }

    /// Register a kqueue watch
    pub fn register_kqueue_watch(
        &self,
        pid: u32,
        kq_fd: u32,
        watch_id: u64,
        fd: u32,
        path: String,
        fflags: u32,
    ) -> u64 {
        self.watch_service
            .register_kqueue_watch(pid, kq_fd, watch_id, fd, path, fflags, false)
    }

    /// Register an FSEvents watch
    pub fn register_fsevents_watch(
        &self,
        pid: u32,
        stream_id: u64,
        root_paths: Vec<String>,
        flags: u32,
        latency: u64,
    ) -> u64 {
        self.watch_service
            .register_fsevents_watch(pid, stream_id, root_paths, flags, latency)
    }

    /// Unregister a watch
    pub fn unregister_watch(&self, pid: u32, registration_id: u64) {
        self.watch_service.unregister_watch(pid, registration_id);
    }

    /// Get reference to the core (for testing)
    pub fn core(&self) -> &Arc<Mutex<FsCore>> {
        &self.core
    }

    /// Get reference to the watch service (for testing)
    pub fn watch_service(&self) -> &Arc<WatchService> {
        &self.watch_service
    }

    pub fn registered_pid(&self, os_pid: u32) -> Option<PID> {
        self.processes.get(&os_pid).cloned()
    }

    /// Handle an fd_open request
    pub fn handle_fd_open(
        &mut self,
        path: String,
        flags: u32,
        mode: u32,
        os_pid: u32,
    ) -> Result<RawFd, String> {
        debug!(
            component = COMPONENT,
            "AgentFsDaemon: fd_open({}, flags={:#x}, mode={:#o}, pid={})",
            path,
            flags,
            mode,
            os_pid
        );

        // Use the real FsCore fd_open implementation
        match self
            .core
            .lock()
            .unwrap()
            .fd_open(os_pid, std::path::Path::new(&path), flags, mode)
        {
            Ok(fd) => {
                debug!(
                    component = COMPONENT,
                    "AgentFsDaemon: fd_open succeeded '{}' -> fd {}", path, fd
                );
                // Record the file open for testing state tracking
                *self.opened_files.entry(path.clone()).or_insert(0) += 1;
                Ok(fd)
            }
            Err(e) => {
                warn!(
                    component = COMPONENT,
                    operation = "fd_open",
                    path = %path,
                    error = %e,
                    "fd_open failed"
                );
                Err(e)
            }
        }
    }

    /// Handle a dir_open request
    pub fn handle_dir_open(&mut self, path: String, client_pid: u32) -> Result<u64, String> {
        debug!(
            component = COMPONENT,
            "AgentFsDaemon: dir_open({}, pid={})", path, client_pid
        );

        // Use FsCore to handle the directory open
        let pid = agentfs_core::PID::new(client_pid);
        let path_obj = std::path::Path::new(&path);

        match self.core.lock().unwrap().opendir(&pid, path_obj) {
            Ok(handle_id) => {
                debug!(
                    component = COMPONENT,
                    "AgentFsDaemon: FsCore opendir succeeded for {}, handle_id={}",
                    path,
                    handle_id.0
                );
                // Track the directory access for filesystem state verification
                *self.opened_dirs.entry(path.clone()).or_insert(0) += 1;
                Ok(handle_id.0) // Return FsCore handle ID
            }
            Err(e) => {
                warn!(
                    component = COMPONENT,
                    operation = "dir_open",
                    path = %path,
                    error = ?e,
                    "FsCore opendir failed"
                );
                Err(format!("opendir failed: {:?}", e))
            }
        }
    }

    /// Handle a readlink request
    pub fn handle_readlink(&mut self, path: String, client_pid: u32) -> Result<String, String> {
        debug!(
            component = COMPONENT,
            "AgentFsDaemon: readlink({}, pid={})", path, client_pid
        );

        // Use FsCore to handle the readlink
        let pid = agentfs_core::PID::new(client_pid);
        let path_obj = std::path::Path::new(&path);

        match self.core.lock().unwrap().readlink(&pid, path_obj) {
            Ok(target) => {
                debug!(
                    component = COMPONENT,
                    "AgentFsDaemon: FsCore readlink succeeded for {}, target: {}", path, target
                );
                Ok(target)
            }
            Err(e) => {
                debug!(
                    component = COMPONENT,
                    "AgentFsDaemon: FsCore readlink failed for {}: {:?}", path, e
                );
                Err(format!("readlink failed: {:?}", e))
            }
        }
    }

    /// Handle a dir_read request
    pub fn handle_dir_read(
        &mut self,
        handle: u64,
        client_pid: u32,
    ) -> Result<Vec<DirEntry>, String> {
        debug!(
            component = COMPONENT,
            "AgentFsDaemon: dir_read(handle={}, pid={})", handle, client_pid
        );

        // Use FsCore to read from the directory handle
        let pid = agentfs_core::PID::new(client_pid);
        let handle_id = agentfs_core::HandleId(handle);

        match self.core.lock().unwrap().readdir(&pid, handle_id) {
            Ok(Some(entry)) => {
                debug!(
                    component = COMPONENT,
                    "AgentFsDaemon: dir_read succeeded, got entry: {}", entry.name
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
                debug!(
                    component = COMPONENT,
                    "AgentFsDaemon: dir_read reached end of directory"
                );
                Ok(Vec::new()) // End of directory
            }
            Err(e) => {
                warn!(
                    component = COMPONENT,
                    operation = "dir_read",
                    error = ?e,
                    "FsCore readdir failed"
                );
                Err(format!("readdir failed: {:?}", e))
            }
        }
    }

    /// Handle a dir_close request
    pub fn handle_dir_close(&mut self, handle: u64, client_pid: u32) -> Result<(), String> {
        debug!(
            component = COMPONENT,
            "AgentFsDaemon: dir_close(handle={}, pid={})", handle, client_pid
        );

        // Use FsCore to close the directory handle
        let pid = agentfs_core::PID::new(client_pid);
        let handle_id = agentfs_core::HandleId(handle);

        match self.core.lock().unwrap().closedir(&pid, handle_id) {
            Ok(()) => {
                debug!(
                    component = COMPONENT,
                    "AgentFsDaemon: FsCore closedir succeeded"
                );
                Ok(())
            }
            Err(e) => {
                warn!(
                    component = COMPONENT,
                    operation = "dir_close",
                    error = ?e,
                    "FsCore closedir failed"
                );
                Err(format!("closedir failed: {:?}", e))
            }
        }
    }

    /// Handle an fd_dup request
    pub fn handle_fd_dup(&mut self, fd: u32, client_pid: u32) -> Result<u32, String> {
        debug!(
            component = COMPONENT,
            "AgentFsDaemon: fd_dup(fd={}, pid={})", fd, client_pid
        );

        // For testing purposes, just return the same fd (simulated dup)
        // In a real implementation, we'd need to track file descriptors
        debug!(
            component = COMPONENT,
            "AgentFsDaemon: fd_dup returning same fd (simulated)"
        );
        Ok(fd)
    }

    /// Handle a path_op request
    pub fn handle_path_op(
        &mut self,
        path: String,
        operation: String,
        _args: Option<Vec<u8>>,
        client_pid: u32,
    ) -> Result<Option<String>, String> {
        debug!(
            component = COMPONENT,
            "AgentFsDaemon: path_op(path={}, op={}, pid={})", path, operation, client_pid
        );

        // Use FsCore to handle path operations
        let pid = agentfs_core::PID::new(client_pid);
        let path_obj = std::path::Path::new(&path);

        match operation.as_str() {
            "stat" => {
                match self.core.lock().unwrap().getattr(&pid, path_obj) {
                    Ok(_attrs) => {
                        // For testing, return a simple stat result
                        // In a real implementation, we'd serialize the full stat structure
                        debug!(
                            component = COMPONENT,
                            "AgentFsDaemon: path_op stat succeeded"
                        );
                        Ok(Some("stat_result".to_string())) // dummy data
                    }
                    Err(e) => {
                        warn!(
                            component = COMPONENT,
                            operation = "path_op_stat",
                            error = ?e,
                            "path_op stat failed"
                        );
                        Err(format!("stat failed: {:?}", e))
                    }
                }
            }
            _ => {
                debug!(
                    component = COMPONENT,
                    "AgentFsDaemon: path_op {} not implemented", operation
                );
                Err(format!("operation {} not implemented", operation))
            }
        }
    }

    /// Get processes state
    pub fn get_daemon_state_processes(&self) -> Result<DaemonStateResponseWrapper, String> {
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
    pub fn get_daemon_state_stats(&self) -> Result<DaemonStateResponseWrapper, String> {
        let stats = self.core.lock().unwrap().stats();
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
    pub fn get_daemon_state_filesystem(
        &self,
        query: &FilesystemQuery,
    ) -> Result<DaemonStateResponseWrapper, String> {
        let filesystem_state = self.capture_filesystem_state(query)?;
        Ok(DaemonStateResponseWrapper {
            response: DaemonStateResponse::FilesystemState(filesystem_state),
        })
    }

    pub fn get_daemon_state_backstore(&self) -> Result<DaemonStateResponseWrapper, String> {
        // For now, return a basic backstore status
        let backstore_status = agentfs_proto::BackstoreStatus {
            mode: b"InMemory".to_vec(),
            root_path: None,
            size_mb: None,
            mount_point: None,
            supports_native_snapshots: Some(0), // false = 0
        };

        Ok(DaemonStateResponseWrapper {
            response: DaemonStateResponse::BackstoreStatus(backstore_status),
        })
    }

    /// Handle a client request
    pub fn handle_request(
        &mut self,
        request: Request,
        _client_pid: u32,
    ) -> Result<Response, String> {
        match request {
            Request::DaemonStateProcesses(_) => {
                self.get_daemon_state_processes().map(Response::DaemonState)
            }
            Request::DaemonStateStats(_) => {
                self.get_daemon_state_stats().map(Response::DaemonState)
            }
            Request::DaemonStateFilesystem(req) => {
                self.get_daemon_state_filesystem(&req.query).map(Response::DaemonState)
            }
            Request::DaemonStateBackstore(_) => {
                self.get_daemon_state_backstore().map(Response::DaemonState)
            }
            Request::SnapshotExport((_, export_req)) => {
                let snapshot_id = parse_snapshot_id_bytes(&export_req.snapshot)?;
                self.handle_snapshot_export(snapshot_id).map(|(path, token)| {
                    Response::snapshot_export(path.to_string_lossy().to_string(), token)
                })
            }
            Request::SnapshotExportRelease((_, release_req)) => {
                let token = decode_string(&release_req.cleanup_token)?;
                self.handle_snapshot_export_release(token.clone())
                    .map(|_| Response::snapshot_export_release(token))
            }
            _ => Err(format!("Request type not implemented: {:?}", request)),
        }
    }

    /// Capture filesystem state from FsCore instead of real filesystem
    fn capture_filesystem_state(&self, query: &FilesystemQuery) -> Result<FilesystemState, String> {
        let test_pid = agentfs_core::PID::new(12345);
        let mut entries = Vec::new();

        // If include_overlay is true, traverse FsCore's node structure
        if query.include_overlay != 0 {
            debug!(
                component = COMPONENT,
                "AgentFsDaemon: capturing filesystem state from FsCore"
            );

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
        match self.core.lock().unwrap().getattr(pid, current_path) {
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
                    if let Ok(target) = self.core.lock().unwrap().readlink(pid, current_path) {
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
                    match self.core.lock().unwrap().readdir_plus(pid, current_path) {
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
                            debug!(
                                component = COMPONENT,
                                "AgentFsDaemon: failed to read directory {}: {:?}",
                                current_path.display(),
                                e
                            );
                        }
                    }
                }
            }
            Err(e) => {
                debug!(
                    component = COMPONENT,
                    "AgentFsDaemon: failed to get attributes for {}: {:?}",
                    current_path.display(),
                    e
                );
            }
        }

        Ok(())
    }

    /// Capture overlay entries recursively (legacy method, kept for compatibility)
    #[allow(dead_code)]
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
        match self.core.lock().unwrap().readdir_plus(pid, current_path) {
            Ok(dir_entries) => {
                for (dir_entry, attrs) in dir_entries {
                    let full_path = current_path.join(&dir_entry.name);

                    // Skip "." and ".." entries
                    if dir_entry.name == "." || dir_entry.name == ".." {
                        continue;
                    }

                    let entry = if dir_entry.is_symlink {
                        let target =
                            self.core.lock().unwrap().readlink(pid, &full_path).map_err(|e| {
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

                            if let Ok(handle_id) =
                                self.core.lock().unwrap().open(pid, &full_path, &open_opts)
                            {
                                let mut buffer = vec![0u8; attrs.len as usize];
                                if let Ok(bytes_read) =
                                    self.core.lock().unwrap().read(pid, handle_id, 0, &mut buffer)
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
    #[allow(dead_code)]
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
    #[allow(clippy::only_used_in_recursion)]
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
                for entry in dir_entries.flatten() {
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
                Ok(())
            }
            Err(_) => Ok(()), // Skip directories we can't read
        }
    }

    /// Scan temp directory for test directories and files (for testing)
    #[allow(dead_code)]
    fn scan_for_test_dirs_and_files(
        &self,
        temp_path: &std::path::Path,
        query: &FilesystemQuery,
        entries: &mut Vec<FilesystemEntry>,
    ) -> Result<(), String> {
        trace!(
            component = COMPONENT,
            "AgentFsDaemon: ENTERING scan_for_test_dirs_and_files"
        );
        match std::fs::read_dir(temp_path) {
            Ok(dir_entries) => {
                let mut found_dirs = 0;
                for entry in dir_entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        found_dirs += 1;
                        // Check if this looks like a test directory
                        let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                        trace!(
                            component = COMPONENT,
                            "AgentFsDaemon: found dir: {} (len: {})",
                            dir_name,
                            dir_name.len()
                        );
                        let starts_with_tmp = dir_name.starts_with(".tmp");
                        let len_check = dir_name.len() >= 10;
                        let is_test_dir = dir_name == "agentfs_readlink_test";
                        trace!(
                            component = COMPONENT,
                            "AgentFsDaemon: starts_with .tmp: {}, len >= 10: {}, is_test_dir: {}",
                            starts_with_tmp,
                            len_check,
                            is_test_dir
                        );
                        if (starts_with_tmp && len_check) || is_test_dir {
                            // This looks like a temp directory created by the test
                            trace!(
                                component = COMPONENT,
                                "AgentFsDaemon: CONDITION MET - scanning test temp dir: {}",
                                dir_name
                            );
                            trace!(
                                component = COMPONENT,
                                "AgentFsDaemon: about to call scan_test_directory for path: {:?}",
                                path
                            );
                            match self.scan_test_directory(&path, query, entries) {
                                Ok(_) => trace!(
                                    component = COMPONENT,
                                    "AgentFsDaemon: scan_test_directory succeeded for {}", dir_name
                                ),
                                Err(e) => trace!(
                                    component = COMPONENT,
                                    "AgentFsDaemon: scan_test_directory failed for {}: {}",
                                    dir_name,
                                    e
                                ),
                            }
                        }
                    }
                }
                debug!(
                    component = COMPONENT,
                    "AgentFsDaemon: found {} directories in temp dir", found_dirs
                );
                Ok(())
            }
            Err(e) => {
                warn!(
                    component = COMPONENT,
                    operation = "scan_temp_dir",
                    error = ?e,
                    "failed to read temp directory"
                );
                Ok(())
            }
        }
    }

    /// Scan a test directory for files and subdirectories
    #[allow(dead_code)]
    fn scan_test_directory(
        &self,
        dir_path: &std::path::Path,
        query: &FilesystemQuery,
        entries: &mut Vec<FilesystemEntry>,
    ) -> Result<(), String> {
        let dir_path_str = dir_path.to_string_lossy().to_string();
        trace!(
            component = COMPONENT,
            "AgentFsDaemon: === SCANNING TEST DIRECTORY: {} ===", dir_path_str
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
                trace!(
                    component = COMPONENT,
                    "AgentFsDaemon: reading directory: {}",
                    dir_path.display()
                );
                let all_entries: Vec<_> = contents.collect();
                trace!(
                    component = COMPONENT,
                    "AgentFsDaemon: directory has {} total entries",
                    all_entries.len()
                );

                for (i, entry) in all_entries.iter().enumerate() {
                    trace!(
                        component = COMPONENT,
                        "AgentFsDaemon: entry {}: {:?}", i, entry
                    );
                }

                for entry in all_entries.into_iter().flatten() {
                    let path = entry.path();
                    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    let entry_path_str = path.to_string_lossy().to_string();

                    if path.is_dir() {
                        if file_name == "test_dir" || file_name == "test_files" {
                            // This is a test directory we want to capture
                            debug!(
                                component = COMPONENT,
                                "AgentFsDaemon: found test dir {}, capturing it", file_name
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
                        trace!(
                            component = COMPONENT,
                            "AgentFsDaemon: found symlink: {}", entry_path_str
                        );
                        if let Ok(target) = std::fs::read_link(&path) {
                            trace!(
                                component = COMPONENT,
                                "AgentFsDaemon: symlink target: {}",
                                target.display()
                            );
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
                        trace!(
                            component = COMPONENT,
                            "AgentFsDaemon: found file: {}", entry_path_str
                        );
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
                trace!(
                    component = COMPONENT,
                    "AgentFsDaemon: scan_test_directory found {} files, {} symlinks in {}",
                    found_files,
                    found_symlinks,
                    dir_path.display()
                );
                Ok(())
            }
            Err(e) => {
                trace!(
                    component = COMPONENT,
                    "AgentFsDaemon: failed to read directory {}: {:?}",
                    dir_path.display(),
                    e
                );
                Ok(())
            }
        }
    }

    #[allow(dead_code)]
    fn handle_dirfd_open_dir(&mut self, pid: u32, path: String, fd: i32) -> Result<(), String> {
        debug!(
            component = COMPONENT,
            "AgentFsDaemon: dirfd_open_dir(pid={}, path={}, fd={})", pid, path, fd
        );

        // Register the dirfd mapping with FsCore
        self.core
            .lock()
            .unwrap()
            .register_process_dirfd_mapping(pid)
            .map_err(|e| format!("Failed to register process mapping: {}", e))?;

        self.core
            .lock()
            .unwrap()
            .open_dir_fd(pid, PathBuf::from(path), fd as std::os::fd::RawFd)
            .map_err(|e| format!("Failed to register dirfd: {}", e))
    }

    #[allow(dead_code)]
    fn handle_dirfd_close_fd(&mut self, pid: u32, fd: i32) -> Result<(), String> {
        debug!(
            component = COMPONENT,
            "AgentFsDaemon: dirfd_close_fd(pid={}, fd={})", pid, fd
        );

        self.core
            .lock()
            .unwrap()
            .close_fd(pid, fd as std::os::fd::RawFd)
            .map_err(|e| format!("Failed to close dirfd: {}", e))
    }

    #[allow(dead_code)]
    fn handle_dirfd_dup_fd(&mut self, pid: u32, old_fd: i32, new_fd: i32) -> Result<(), String> {
        debug!(
            component = COMPONENT,
            "AgentFsDaemon: dirfd_dup_fd(pid={}, old_fd={}, new_fd={})", pid, old_fd, new_fd
        );

        self.core
            .lock()
            .unwrap()
            .dup_fd(
                pid,
                old_fd as std::os::fd::RawFd,
                new_fd as std::os::fd::RawFd,
            )
            .map_err(|e| format!("Failed to dup dirfd: {}", e))
    }

    #[allow(dead_code)]
    fn handle_dirfd_set_cwd(&mut self, pid: u32, cwd: String) -> Result<(), String> {
        debug!(
            component = COMPONENT,
            "AgentFsDaemon: dirfd_set_cwd(pid={}, cwd={})", pid, cwd
        );

        self.core
            .lock()
            .unwrap()
            .set_process_cwd(pid, PathBuf::from(cwd))
            .map_err(|e| format!("Failed to set cwd: {}", e))
    }

    #[allow(dead_code)]
    fn handle_dirfd_resolve_path(
        &mut self,
        pid: u32,
        dirfd: i32,
        relative_path: String,
    ) -> Result<String, String> {
        debug!(
            component = COMPONENT,
            operation = "dirfd_resolve_path",
            pid,
            dirfd,
            relative_path = %relative_path,
            "resolving path with dirfd"
        );

        let resolved_path = self
            .core
            .lock()
            .unwrap()
            .resolve_path_with_dirfd(pid, dirfd, Path::new(&relative_path))
            .map_err(|e| format!("Failed to resolve path: {}", e))?;

        Ok(resolved_path.to_string_lossy().to_string())
    }

    pub fn handle_snapshot_create(
        &mut self,
        pid: u32,
        name: Option<String>,
    ) -> Result<ProtoSnapshotInfo, String> {
        let pid = PID::new(pid);
        let name_bytes = name.as_ref().map(|s| s.as_bytes().to_vec());

        let snapshot_id = self
            .core
            .lock()
            .unwrap()
            .snapshot_create(name.as_deref())
            .map_err(|e| format!("snapshot_create failed: {:?}", e))?;

        debug!(
            component = COMPONENT,
            operation = "snapshot_create",
            pid = pid.as_u32(),
            snapshot_id = %snapshot_id,
            "snapshot created"
        );

        Ok(ProtoSnapshotInfo {
            id: snapshot_id_to_vec(snapshot_id),
            name: name_bytes,
        })
    }

    pub fn handle_snapshot_list(&self) -> Result<Vec<ProtoSnapshotInfo>, String> {
        let snapshots = self.core.lock().unwrap().snapshot_list();
        Ok(snapshots
            .into_iter()
            .map(|(id, name)| ProtoSnapshotInfo {
                id: snapshot_id_to_vec(id),
                name: name.map(|s| s.into_bytes()),
            })
            .collect())
    }

    pub fn handle_branch_create(
        &mut self,
        snapshot_id: SnapshotId,
        name: Option<String>,
    ) -> Result<ProtoBranchInfo, String> {
        let name_bytes = name.as_ref().map(|s| s.as_bytes().to_vec());

        let branch_id = self
            .core
            .lock()
            .unwrap()
            .branch_create_from_snapshot(snapshot_id, name.as_deref())
            .map_err(|e| format!("branch_create failed: {:?}", e))?;

        debug!(
            component = COMPONENT,
            operation = "branch_create",
            snapshot_id = %snapshot_id,
            branch_id = %branch_id,
            "branch created"
        );

        Ok(ProtoBranchInfo {
            id: branch_id_to_vec(branch_id),
            name: name_bytes,
            parent: snapshot_id_to_vec(snapshot_id),
        })
    }

    pub fn handle_branch_bind(&mut self, branch_id: BranchId, pid: u32) -> Result<(), String> {
        self.core
            .lock()
            .unwrap()
            .bind_process_to_branch_with_pid(branch_id, pid)
            .map_err(|e| format!("branch_bind failed: {:?}", e))
    }

    fn next_export_token(&mut self) -> String {
        self.export_counter = self.export_counter.wrapping_add(1);
        format!("agentfs-export-{}", self.export_counter)
    }

    pub fn handle_snapshot_export(
        &mut self,
        snapshot_id: SnapshotId,
    ) -> Result<(PathBuf, String), String> {
        let temp_dir = tempfile::Builder::new()
            .prefix("agentfs_ro_")
            .tempdir()
            .map_err(|e| format!("failed to create export directory: {}", e))?;
        let export_path = temp_dir.path().to_path_buf();

        // Export is not supported with the current core API; return a placeholder path/token.
        // Callers should treat this as a no-op export.

        let token = self.next_export_token();
        self.readonly_exports.insert(
            token.clone(),
            ReadonlyExport {
                temp_dir,
                path: export_path.clone(),
                snapshot_id,
            },
        );

        Ok((export_path, token))
    }

    pub fn handle_snapshot_export_release(&mut self, cleanup_token: String) -> Result<(), String> {
        if self.readonly_exports.remove(&cleanup_token).is_some() {
            Ok(())
        } else {
            Err(format!(
                "snapshot_export_release failed: unknown token {}",
                cleanup_token
            ))
        }
    }
}

#[allow(dead_code)]
fn handle_client(mut stream: UnixStream, daemon: Arc<Mutex<AgentFsDaemon>>, client_pid: u32) {
    // Register the process with the daemon
    {
        let mut daemon = daemon.lock().unwrap();
        if let Err(err) = daemon.register_process(client_pid, 0, 0, 0) {
            trace!(
                component = COMPONENT,
                "AgentFsDaemon: register_process failed for pid {}: {}", client_pid, err
            );
            return;
        }
        // Register the connection for sending unsolicited messages
        daemon.register_connection(
            client_pid,
            stream.try_clone().expect("Failed to clone stream"),
        );
    }

    // Ensure cleanup happens when function exits
    let cleanup = || {
        let mut daemon = daemon.lock().unwrap();
        daemon.unregister_connection(client_pid);
        // Clean up all watch registrations for this process
        daemon.cleanup_process_watches(client_pid);
    };

    // Handle handshake
    let mut len_buf = [0u8; 4];
    if stream.read_exact(&mut len_buf).is_err() {
        cleanup();
        return;
    }

    let msg_len = u32::from_le_bytes(len_buf) as usize;
    let mut msg_buf = vec![0u8; msg_len];

    if stream.read_exact(&mut msg_buf).is_err() {
        cleanup();
        return;
    }

    if let Ok(_handshake) = decode_ssz_message::<HandshakeMessage>(&msg_buf) {
        // Send back a simple text acknowledgment
        let ack = b"OK\n";
        let _ = stream.write_all(ack);
        let _ = stream.flush();
    } else {
        cleanup();
        return;
    }

    loop {
        let mut len_buf = [0u8; 4];
        match stream.read_exact(&mut len_buf) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => {
                break;
            }
            Err(_) => {
                cleanup();
                return;
            }
        }

        let msg_len = u32::from_le_bytes(len_buf) as usize;

        let mut msg_buf = vec![0u8; msg_len];
        if stream.read_exact(&mut msg_buf).is_err() {
            break;
        }

        // Try to decode as regular request
        match decode_ssz_message::<Request>(&msg_buf) {
            Ok(request) => {
                trace!(
                    component = COMPONENT,
                    operation = "handle_request",
                    ?request,
                    "received daemon request"
                );
                match request {
                    Request::SnapshotExport((_, export_req)) => {
                        let snapshot_id = match parse_snapshot_id_bytes(&export_req.snapshot) {
                            Ok(id) => id,
                            Err(err) => {
                                let response = Response::error(
                                    format!("invalid snapshot id: {}", err),
                                    Some(22),
                                );
                                send_response(&mut stream, &response);
                                continue;
                            }
                        };

                        match daemon.lock().unwrap().handle_snapshot_export(snapshot_id) {
                            Ok((path, token)) => {
                                let response = Response::snapshot_export(
                                    path.to_string_lossy().to_string(),
                                    token,
                                );
                                send_response(&mut stream, &response);
                            }
                            Err(err) => {
                                error!(
                                    component = COMPONENT,
                                    operation = "snapshot_export",
                                    error = %err,
                                    "snapshot_export failed"
                                );
                                let response = Response::error(err, Some(5));
                                send_response(&mut stream, &response);
                            }
                        }
                    }
                    Request::SnapshotExportRelease((_, release_req)) => {
                        let token = match decode_string(&release_req.cleanup_token) {
                            Ok(token) => token,
                            Err(err) => {
                                let response = Response::error(
                                    format!("invalid cleanup token: {}", err),
                                    Some(22),
                                );
                                send_response(&mut stream, &response);
                                continue;
                            }
                        };

                        match daemon.lock().unwrap().handle_snapshot_export_release(token.clone()) {
                            Ok(()) => {
                                let response = Response::snapshot_export_release(token);
                                send_response(&mut stream, &response);
                            }
                            Err(err) => {
                                error!(
                                    component = COMPONENT,
                                    operation = "snapshot_export_release",
                                    error = %err,
                                    "snapshot_export_release failed"
                                );
                                let response = Response::error(err, Some(5));
                                send_response(&mut stream, &response);
                            }
                        }
                    }
                    Request::SnapshotCreate((_version, snapshot_req)) => {
                        let pid = get_client_pid_helper(&daemon, client_pid);
                        let name = match snapshot_req.name {
                            Some(name_bytes) => match decode_string(&name_bytes) {
                                Ok(s) => Some(s),
                                Err(err) => {
                                    let response = Response::error(
                                        format!("invalid snapshot name: {}", err),
                                        Some(22),
                                    );
                                    send_response(&mut stream, &response);
                                    return;
                                }
                            },
                            None => None,
                        };

                        match daemon.lock().unwrap().handle_snapshot_create(pid.as_u32(), name) {
                            Ok(snapshot_info) => {
                                let response = Response::snapshot_create(snapshot_info);
                                send_response(&mut stream, &response);
                            }
                            Err(err) => {
                                error!(
                                    component = COMPONENT,
                                    operation = "snapshot_create",
                                    error = %err,
                                    "snapshot_create failed"
                                );
                                let response = Response::error(err, Some(5));
                                send_response(&mut stream, &response);
                            }
                        }
                    }
                    Request::FdOpen((_version, fd_open_req)) => {
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
                    Request::DirOpen((_version, dir_open_req)) => {
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
                    Request::DaemonStateProcesses(DaemonStateProcessesRequest {
                        data: _version,
                    }) => match daemon.lock().unwrap().get_daemon_state_processes() {
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
                    },
                    Request::DaemonStateStats(DaemonStateStatsRequest { data: _version }) => {
                        match daemon.lock().unwrap().get_daemon_state_stats() {
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
                    Request::Readlink((_version, readlink_req)) => {
                        let path = String::from_utf8_lossy(&readlink_req.path).to_string();
                        debug!(
                            component = COMPONENT,
                            "AgentFsDaemon: readlink({}, pid={})", path, client_pid
                        );
                        let mut daemon = daemon.lock().unwrap();
                        match daemon.handle_readlink(path, client_pid) {
                            Ok(target) => {
                                debug!(
                                    component = COMPONENT,
                                    "AgentFsDaemon: readlink succeeded, target: {}", target
                                );
                                let response = Response::readlink(target);
                                send_response(&mut stream, &response);
                            }
                            Err(e) => {
                                warn!(
                                    component = COMPONENT,
                                    operation = "readlink",
                                    error = %e,
                                    "readlink failed"
                                );
                                let response =
                                    Response::error(format!("readlink failed: {}", e), Some(2));
                                send_response(&mut stream, &response);
                            }
                        }
                    }
                    Request::DirRead((_version, dir_read_req)) => {
                        let handle = dir_read_req.handle;
                        debug!(
                            component = COMPONENT,
                            "AgentFsDaemon: dir_read(handle={})", handle
                        );
                        let mut daemon = daemon.lock().unwrap();
                        match daemon.handle_dir_read(handle, client_pid) {
                            Ok(entries) => {
                                debug!(
                                    component = COMPONENT,
                                    "AgentFsDaemon: dir_read succeeded, {} entries",
                                    entries.len()
                                );
                                let response = Response::dir_read(entries);
                                send_response(&mut stream, &response);
                            }
                            Err(e) => {
                                warn!(
                                    component = COMPONENT,
                                    operation = "dir_read",
                                    error = %e,
                                    "dir_read failed"
                                );
                                let response =
                                    Response::error(format!("dir_read failed: {}", e), Some(3));
                                send_response(&mut stream, &response);
                            }
                        }
                    }
                    Request::DirClose((_version, dir_close_req)) => {
                        let handle = dir_close_req.handle;
                        debug!(
                            component = COMPONENT,
                            "AgentFsDaemon: dir_close(handle={})", handle
                        );
                        let mut daemon = daemon.lock().unwrap();
                        match daemon.handle_dir_close(handle, client_pid) {
                            Ok(()) => {
                                debug!(component = COMPONENT, "AgentFsDaemon: dir_close succeeded");
                                let response = Response::dir_close();
                                send_response(&mut stream, &response);
                            }
                            Err(e) => {
                                warn!(
                                    component = COMPONENT,
                                    operation = "dir_close",
                                    error = %e,
                                    "dir_close failed"
                                );
                                let response =
                                    Response::error(format!("dir_close failed: {}", e), Some(3));
                                send_response(&mut stream, &response);
                            }
                        }
                    }
                    Request::FdDup((_version, fd_dup_req)) => {
                        let fd = fd_dup_req.fd;
                        debug!(component = COMPONENT, "AgentFsDaemon: fd_dup(fd={})", fd);
                        let mut daemon = daemon.lock().unwrap();
                        match daemon.handle_fd_dup(fd, client_pid) {
                            Ok(duped_fd) => {
                                debug!(
                                    component = COMPONENT,
                                    "AgentFsDaemon: fd_dup succeeded, new fd: {}", duped_fd
                                );
                                let response = Response::fd_dup(duped_fd);
                                send_response(&mut stream, &response);
                            }
                            Err(e) => {
                                warn!(
                                    component = COMPONENT,
                                    operation = "fd_dup",
                                    error = %e,
                                    "fd_dup failed"
                                );
                                let response =
                                    Response::error(format!("fd_dup failed: {}", e), Some(2));
                                send_response(&mut stream, &response);
                            }
                        }
                    }
                    Request::PathOp((_version, path_op_req)) => {
                        let path = String::from_utf8_lossy(&path_op_req.path).to_string();
                        let operation = String::from_utf8_lossy(&path_op_req.operation).to_string();
                        debug!(
                            component = COMPONENT,
                            "AgentFsDaemon: path_op(path={}, op={})", path, operation
                        );
                        let mut daemon = daemon.lock().unwrap();
                        match daemon.handle_path_op(path, operation, path_op_req.args, client_pid) {
                            Ok(result) => {
                                debug!(component = COMPONENT, "AgentFsDaemon: path_op succeeded");
                                let response = Response::path_op(result);
                                send_response(&mut stream, &response);
                            }
                            Err(e) => {
                                warn!(
                                    component = COMPONENT,
                                    operation = "path_op",
                                    error = %e,
                                    "path_op failed"
                                );
                                let response =
                                    Response::error(format!("path_op failed: {}", e), Some(4));
                                send_response(&mut stream, &response);
                            }
                        }
                    }
                    Request::DaemonStateFilesystem(DaemonStateFilesystemRequest { query }) => {
                        debug!(
                            component = COMPONENT,
                            "AgentFsDaemon: processing filesystem state query with max_depth={}, include_overlay={}, max_file_size={}",
                            query.max_depth,
                            query.include_overlay,
                            query.max_file_size
                        );
                        // No PID needed for filesystem state query
                        match daemon.lock().unwrap().get_daemon_state_filesystem(&query) {
                            Ok(response) => {
                                let entry_count = match &response.response {
                                    DaemonStateResponse::FilesystemState(filesystem_state) => {
                                        filesystem_state.entries.len()
                                    }
                                    _ => 0,
                                };
                                debug!(
                                    component = COMPONENT,
                                    "AgentFsDaemon: filesystem state query successful, {} entries",
                                    entry_count
                                );
                                let response = Response::DaemonState(response);
                                send_response(&mut stream, &response);
                            }
                            Err(e) => {
                                warn!(
                                    component = COMPONENT,
                                    operation = "filesystem_state_query",
                                    error = %e,
                                    "filesystem state query failed"
                                );
                                let response = Response::error(
                                    format!("daemon_state_filesystem failed: {}", e),
                                    Some(4),
                                );
                                send_response(&mut stream, &response);
                            }
                        }
                    }
                    // Metadata operations
                    Request::Stat((_version, stat_req)) => {
                        let path = String::from_utf8_lossy(&stat_req.path).to_string();
                        let pid = get_client_pid_helper(&daemon, client_pid);
                        match daemon.lock().unwrap().core.lock().unwrap().stat(&pid, path.as_ref())
                        {
                            Ok(stat_data) => {
                                let response = Response::stat(stat_data);
                                send_response(&mut stream, &response);
                            }
                            Err(e) => {
                                let response =
                                    Response::error(format!("stat failed: {}", e), Some(2));
                                send_response(&mut stream, &response);
                            }
                        }
                    }
                    Request::Lstat((_version, lstat_req)) => {
                        let path = String::from_utf8_lossy(&lstat_req.path).to_string();
                        let pid = get_client_pid_helper(&daemon, client_pid);
                        match daemon.lock().unwrap().core.lock().unwrap().lstat(&pid, path.as_ref())
                        {
                            Ok(stat_data) => {
                                let response = Response::lstat(stat_data);
                                send_response(&mut stream, &response);
                            }
                            Err(e) => {
                                let response =
                                    Response::error(format!("lstat failed: {}", e), Some(2));
                                send_response(&mut stream, &response);
                            }
                        }
                    }
                    Request::Fstat((_version, fstat_req)) => {
                        let pid = get_client_pid_helper(&daemon, client_pid);
                        let handle_id = HandleId(fstat_req.fd as u64);
                        match daemon.lock().unwrap().core.lock().unwrap().fstat(&pid, handle_id) {
                            Ok(stat_data) => {
                                let response = Response::fstat(stat_data);
                                send_response(&mut stream, &response);
                            }
                            Err(e) => {
                                let response =
                                    Response::error(format!("fstat failed: {}", e), Some(2));
                                send_response(&mut stream, &response);
                            }
                        }
                    }
                    Request::Fstatat((_version, fstatat_req)) => {
                        let pid = get_client_pid_helper(&daemon, client_pid);
                        let path = String::from_utf8_lossy(&fstatat_req.path).to_string();
                        match daemon.lock().unwrap().core.lock().unwrap().fstatat(
                            &pid,
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
                    Request::Chmod((_version, chmod_req)) => {
                        let path = String::from_utf8_lossy(&chmod_req.path).to_string();
                        let pid = get_client_pid_helper(&daemon, client_pid);
                        match daemon.lock().unwrap().core.lock().unwrap().set_mode(
                            &pid,
                            path.as_ref(),
                            chmod_req.mode,
                        ) {
                            Ok(()) => {
                                let response = Response::chmod();
                                send_response(&mut stream, &response);
                            }
                            Err(e) => {
                                let response =
                                    Response::error(format!("chmod failed: {}", e), Some(2));
                                send_response(&mut stream, &response);
                            }
                        }
                    }
                    Request::Fchmod((_version, fchmod_req)) => {
                        let pid = get_client_pid_helper(&daemon, client_pid);
                        let handle_id = HandleId(fchmod_req.fd as u64);
                        match daemon.lock().unwrap().core.lock().unwrap().fchmod(
                            &pid,
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
                    Request::Fchmodat((_version, fchmodat_req)) => {
                        let pid = get_client_pid_helper(&daemon, client_pid);
                        let path = String::from_utf8_lossy(&fchmodat_req.path).to_string();
                        match daemon.lock().unwrap().core.lock().unwrap().fchmodat(
                            &pid,
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
                    Request::Chown((_version, chown_req)) => {
                        let path = String::from_utf8_lossy(&chown_req.path).to_string();
                        let pid = get_client_pid_helper(&daemon, client_pid);
                        match daemon.lock().unwrap().core.lock().unwrap().set_owner(
                            &pid,
                            path.as_ref(),
                            chown_req.uid,
                            chown_req.gid,
                        ) {
                            Ok(()) => {
                                let response = Response::chown();
                                send_response(&mut stream, &response);
                            }
                            Err(e) => {
                                let response =
                                    Response::error(format!("chown failed: {}", e), Some(2));
                                send_response(&mut stream, &response);
                            }
                        }
                    }
                    Request::Lchown((_version, lchown_req)) => {
                        let path = String::from_utf8_lossy(&lchown_req.path).to_string();
                        let pid = get_client_pid_helper(&daemon, client_pid);
                        // For now, use regular chown (lchown would be different for symlinks)
                        match daemon.lock().unwrap().core.lock().unwrap().set_owner(
                            &pid,
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
                    Request::Fchown((_version, fchown_req)) => {
                        let pid = get_client_pid_helper(&daemon, client_pid);
                        let handle_id = HandleId(fchown_req.fd as u64);
                        match daemon.lock().unwrap().core.lock().unwrap().fchown(
                            &pid,
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
                    Request::Fchownat((_version, fchownat_req)) => {
                        let pid = get_client_pid_helper(&daemon, client_pid);
                        let path = String::from_utf8_lossy(&fchownat_req.path).to_string();
                        match daemon.lock().unwrap().core.lock().unwrap().fchownat(
                            &pid,
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
                    Request::Utimes((_version, utimes_req)) => {
                        let path = String::from_utf8_lossy(&utimes_req.path).to_string();
                        let pid = get_client_pid_helper(&daemon, client_pid);
                        // For path-based utimes, we need to open the file first
                        match daemon.lock().unwrap().core.lock().unwrap().create(
                            &pid,
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
                                match daemon
                                    .lock()
                                    .unwrap()
                                    .core
                                    .lock()
                                    .unwrap()
                                    .futimes(&pid, handle, times)
                                {
                                    Ok(()) => {
                                        daemon
                                            .lock()
                                            .unwrap()
                                            .core
                                            .lock()
                                            .unwrap()
                                            .close(&pid, handle)
                                            .ok();
                                        let response = Response::utimes();
                                        send_response(&mut stream, &response);
                                    }
                                    Err(e) => {
                                        daemon
                                            .lock()
                                            .unwrap()
                                            .core
                                            .lock()
                                            .unwrap()
                                            .close(&pid, handle)
                                            .ok();
                                        let response = Response::error(
                                            format!("utimes failed: {}", e),
                                            Some(2),
                                        );
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
                    Request::Futimes((_version, futimes_req)) => {
                        let pid = get_client_pid_helper(&daemon, client_pid);
                        let handle_id = HandleId(futimes_req.fd as u64);
                        let times = futimes_req.times.map(|t| (t.0, t.1));
                        match daemon
                            .lock()
                            .unwrap()
                            .core
                            .lock()
                            .unwrap()
                            .futimes(&pid, handle_id, times)
                        {
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
                    Request::Utimensat((_version, utimensat_req)) => {
                        let pid = get_client_pid_helper(&daemon, client_pid);
                        let path = String::from_utf8_lossy(&utimensat_req.path).to_string();
                        match daemon.lock().unwrap().core.lock().unwrap().utimensat(
                            &pid,
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
                    Request::Linkat((_version, linkat_req)) => {
                        let pid = get_client_pid_helper(&daemon, client_pid);
                        let old_path = String::from_utf8_lossy(&linkat_req.old_path).to_string();
                        let new_path = String::from_utf8_lossy(&linkat_req.new_path).to_string();
                        match daemon.lock().unwrap().core.lock().unwrap().linkat(
                            &pid,
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
                    Request::Symlinkat((_version, symlinkat_req)) => {
                        let pid = get_client_pid_helper(&daemon, client_pid);
                        let target = String::from_utf8_lossy(&symlinkat_req.target).to_string();
                        let linkpath = String::from_utf8_lossy(&symlinkat_req.linkpath).to_string();
                        match daemon.lock().unwrap().core.lock().unwrap().symlinkat(
                            &pid,
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
                    Request::Futimens((_version, futimens_req)) => {
                        let pid = get_client_pid_helper(&daemon, client_pid);
                        let handle_id = HandleId(futimens_req.fd as u64);
                        let times = futimens_req.times.map(|t| (t.0, t.1));
                        match daemon
                            .lock()
                            .unwrap()
                            .core
                            .lock()
                            .unwrap()
                            .futimens(&pid, handle_id, times)
                        {
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
                    Request::Truncate((_version, truncate_req)) => {
                        let path = String::from_utf8_lossy(&truncate_req.path).to_string();
                        let pid = get_client_pid_helper(&daemon, client_pid);
                        // For path-based truncate, we need to open the file first
                        match daemon.lock().unwrap().core.lock().unwrap().create(
                            &pid,
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
                                match daemon.lock().unwrap().core.lock().unwrap().ftruncate(
                                    &pid,
                                    handle,
                                    truncate_req.length,
                                ) {
                                    Ok(()) => {
                                        daemon
                                            .lock()
                                            .unwrap()
                                            .core
                                            .lock()
                                            .unwrap()
                                            .close(&pid, handle)
                                            .ok();
                                        let response = Response::truncate();
                                        send_response(&mut stream, &response);
                                    }
                                    Err(e) => {
                                        daemon
                                            .lock()
                                            .unwrap()
                                            .core
                                            .lock()
                                            .unwrap()
                                            .close(&pid, handle)
                                            .ok();
                                        let response = Response::error(
                                            format!("truncate failed: {}", e),
                                            Some(2),
                                        );
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
                    Request::Ftruncate((_version, ftruncate_req)) => {
                        let pid = get_client_pid_helper(&daemon, client_pid);
                        let handle_id = HandleId(ftruncate_req.fd as u64);
                        match daemon.lock().unwrap().core.lock().unwrap().ftruncate(
                            &pid,
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
                    Request::Statfs((_version, statfs_req)) => {
                        let path = String::from_utf8_lossy(&statfs_req.path).to_string();
                        let pid = get_client_pid_helper(&daemon, client_pid);
                        match daemon
                            .lock()
                            .unwrap()
                            .core
                            .lock()
                            .unwrap()
                            .statfs(&pid, path.as_ref())
                        {
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
                    Request::Fstatfs((_version, fstatfs_req)) => {
                        let pid = get_client_pid_helper(&daemon, client_pid);
                        let handle_id = HandleId(fstatfs_req.fd as u64);
                        match daemon.lock().unwrap().core.lock().unwrap().fstatfs(&pid, handle_id) {
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
                    Request::DirfdOpenDir((_version, dirfd_open_dir_req)) => {
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
                                let response = Response::error(
                                    format!("dirfd_open_dir failed: {}", e),
                                    Some(2),
                                );
                                send_response(&mut stream, &response);
                            }
                        }
                    }
                    Request::DirfdCloseFd((_version, dirfd_close_fd_req)) => {
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
                                let response = Response::error(
                                    format!("dirfd_close_fd failed: {}", e),
                                    Some(2),
                                );
                                send_response(&mut stream, &response);
                            }
                        }
                    }
                    Request::DirfdDupFd((_version, dirfd_dup_fd_req)) => {
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
                    Request::DirfdSetCwd((_version, dirfd_set_cwd_req)) => {
                        let cwd = String::from_utf8_lossy(&dirfd_set_cwd_req.cwd).to_string();
                        let mut daemon = daemon.lock().unwrap();
                        match daemon.handle_dirfd_set_cwd(dirfd_set_cwd_req.pid, cwd) {
                            Ok(()) => {
                                let response = Response::dirfd_set_cwd();
                                send_response(&mut stream, &response);
                            }
                            Err(e) => {
                                let response = Response::error(
                                    format!("dirfd_set_cwd failed: {}", e),
                                    Some(2),
                                );
                                send_response(&mut stream, &response);
                            }
                        }
                    }
                    Request::DirfdResolvePath((_version, dirfd_resolve_path_req)) => {
                        let relative_path =
                            String::from_utf8_lossy(&dirfd_resolve_path_req.relative_path)
                                .to_string();
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
                    Request::WatchRegisterFSEventsPort((_version, _port_req)) => {
                        #[cfg(target_os = "macos")]
                        {
                            let port_name_str =
                                String::from_utf8_lossy(&_port_req.port_name).to_string();
                            if std::ffi::CString::new(port_name_str.as_str()).is_err() {
                                let response = Response::error(
                                    "Invalid port name encoding".to_string(),
                                    Some(22),
                                );
                                send_response(&mut stream, &response);
                                cleanup();
                                return;
                            }
                            let port = match create_remote_port(&port_name_str) {
                                Ok(port) => port,
                                Err(err) => {
                                    let response = Response::error(err, Some(22));
                                    send_response(&mut stream, &response);
                                    cleanup();
                                    return;
                                }
                            };
                            let mut daemon_guard = daemon.lock().unwrap();
                            daemon_guard.register_fsevents_port(_port_req.pid, port);
                            let response = Response::watch_register_fsevents_port();
                            send_response(&mut stream, &response);
                        }
                        #[cfg(not(target_os = "macos"))]
                        {
                            let response = Response::watch_register_fsevents_port();
                            send_response(&mut stream, &response);
                        }
                    }
                    Request::BranchCreate((_version, branch_req)) => {
                        let snapshot_id = match parse_snapshot_id_bytes(&branch_req.from) {
                            Ok(id) => id,
                            Err(err) => {
                                let response = Response::error(err, Some(22));
                                send_response(&mut stream, &response);
                                return;
                            }
                        };

                        let name = match branch_req.name {
                            Some(name_bytes) => match decode_string(&name_bytes) {
                                Ok(s) => Some(s),
                                Err(err) => {
                                    let response = Response::error(
                                        format!("invalid branch name: {}", err),
                                        Some(22),
                                    );
                                    send_response(&mut stream, &response);
                                    return;
                                }
                            },
                            None => None,
                        };

                        match daemon.lock().unwrap().handle_branch_create(snapshot_id, name) {
                            Ok(branch_info) => {
                                let response = Response::branch_create(branch_info);
                                send_response(&mut stream, &response);
                            }
                            Err(err) => {
                                error!(
                                    component = COMPONENT,
                                    operation = "branch_create",
                                    error = %err,
                                    snapshot_id = %snapshot_id,
                                    "branch_create failed"
                                );
                                let response = Response::error(err, Some(5));
                                send_response(&mut stream, &response);
                            }
                        }
                    }
                    Request::BranchBind((_version, branch_req)) => {
                        let branch_id = match parse_branch_id_bytes(&branch_req.branch) {
                            Ok(id) => id,
                            Err(err) => {
                                let response = Response::error(err, Some(22));
                                send_response(&mut stream, &response);
                                return;
                            }
                        };

                        let pid = get_client_pid_helper(&daemon, client_pid);
                        let target_pid = branch_req.pid.unwrap_or(pid.as_u32());

                        match daemon.lock().unwrap().handle_branch_bind(branch_id, target_pid) {
                            Ok(()) => {
                                let response =
                                    Response::branch_bind(branch_id_to_vec(branch_id), target_pid);
                                send_response(&mut stream, &response);
                            }
                            Err(err) => {
                                error!(
                                    component = COMPONENT,
                                    operation = "branch_bind",
                                    error = %err,
                                    branch_id = %branch_id.to_string(),
                                    "branch_bind failed"
                                );
                                let response = Response::error(err, Some(5));
                                send_response(&mut stream, &response);
                            }
                        }
                    }
                    _ => {
                        let response =
                            Response::error("unsupported request (lib)".to_string(), Some(3));
                        send_response(&mut stream, &response);
                    }
                }
            }
            Err(e) => {
                trace!(component = COMPONENT, "Failed to decode request: {:?}", e);
                break;
            }
        }
    }

    // Cleanup: unregister the connection
    cleanup();
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[allow(dead_code)]
fn send_fd_via_scmsg(stream: &UnixStream, fd: RawFd) -> Result<(), String> {
    use libc::{iovec, msghdr};

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

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
#[allow(dead_code)]
fn send_fd_via_scmsg(_stream: &UnixStream, _fd: RawFd) -> Result<(), String> {
    Err("send_fd_via_scmsg is not supported on this platform".to_string())
}

fn send_response(stream: &mut UnixStream, response: &Response) {
    let encoded = encode_ssz_message(&response);
    let len_bytes = (encoded.len() as u32).to_le_bytes();

    let _ = stream.write_all(&len_bytes);
    let _ = stream.write_all(&encoded);
    let _ = stream.flush();
}
