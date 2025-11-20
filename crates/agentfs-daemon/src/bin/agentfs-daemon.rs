// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! AgentFS Daemon executable - Production-ready filesystem daemon with interpose support

use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::thread;
use tracing::{debug, error, info};

// AgentFS daemon library
use agentfs_daemon::{AgentFsDaemon, decode_ssz_message, encode_ssz_message};

// Logging
use ah_logging::{Level, LogFormat, init};

// AgentFS proto imports
use agentfs_proto::*;

#[cfg(target_os = "macos")]
use agentfs_daemon::macos::interposition::create_remote_port;

// Import specific types that need explicit qualification
use agentfs_proto::messages::{
    DaemonStateFilesystemRequest, DaemonStateProcessesRequest, DaemonStateResponse,
    DaemonStateStatsRequest,
};

use agentfs_core::{BranchId, PID, SnapshotId};

// Use handshake types from the daemon crate
use agentfs_daemon::HandshakeMessage;

const COMPONENT: &str = "agentfs-daemon";

fn decode_string(bytes: &[u8]) -> Result<String, String> {
    std::str::from_utf8(bytes)
        .map(|s| s.to_string())
        .map_err(|err| format!("invalid UTF-8 sequence: {}", err))
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

fn get_client_pid_helper(daemon: &Arc<Mutex<AgentFsDaemon>>, client_pid: u32) -> PID {
    daemon
        .lock()
        .unwrap()
        .registered_pid(client_pid)
        .unwrap_or_else(|| PID::new(client_pid))
}

fn main() {
    // Initialize logging
    if let Err(e) = init("agentfs-daemon", Level::WARN, LogFormat::Plaintext) {
        error!(component = COMPONENT, "Failed to initialize logging: {}", e);
        std::process::exit(1);
    }

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        error!(
            component = COMPONENT,
            "Usage: {} <socket_path> [--backstore-mode <mode> [--backstore-root <path>] [--backstore-size-mb <mb>]] [--lower-dir <path>] [--upper-dir <path>] [--work-dir <path>]",
            args[0]
        );
        error!(
            component = COMPONENT,
            "Backstore modes: InMemory, HostFs, RamDisk"
        );
        std::process::exit(1);
    }

    // Parse overlay and backstore arguments
    let mut socket_path = None;
    let mut lower_dir = None;
    let mut upper_dir = None;
    let mut work_dir = None;
    let mut backstore_mode = agentfs_core::config::BackstoreMode::InMemory;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--lower-dir" => {
                if i + 1 < args.len() {
                    lower_dir = Some(PathBuf::from(&args[i + 1]));
                    i += 2;
                } else {
                    error!(component = COMPONENT, "--lower-dir requires an argument");
                    std::process::exit(1);
                }
            }
            "--upper-dir" => {
                if i + 1 < args.len() {
                    upper_dir = Some(PathBuf::from(&args[i + 1]));
                    i += 2;
                } else {
                    error!(component = COMPONENT, "--upper-dir requires an argument");
                    std::process::exit(1);
                }
            }
            "--work-dir" => {
                if i + 1 < args.len() {
                    work_dir = Some(PathBuf::from(&args[i + 1]));
                    i += 2;
                } else {
                    error!(component = COMPONENT, "--work-dir requires an argument");
                    std::process::exit(1);
                }
            }
            "--backstore-mode" => {
                if i + 1 < args.len() {
                    backstore_mode = match args[i + 1].as_str() {
                        "InMemory" => agentfs_core::config::BackstoreMode::InMemory,
                        "HostFs" => agentfs_core::config::BackstoreMode::HostFs {
                            root: PathBuf::from("/tmp/agentfs-backstore"),
                            prefer_native_snapshots: true,
                        },
                        "RamDisk" => agentfs_core::config::BackstoreMode::RamDisk { size_mb: 64 },
                        _ => {
                            error!(
                                component = COMPONENT,
                                "Invalid backstore mode: {}",
                                args[i + 1]
                            );
                            std::process::exit(1);
                        }
                    };
                    i += 2;
                } else {
                    error!(
                        component = COMPONENT,
                        "--backstore-mode requires an argument"
                    );
                    std::process::exit(1);
                }
            }
            "--backstore-root" => {
                if i + 1 < args.len() {
                    // Update the backstore mode to HostFs with the specified root
                    if let agentfs_core::config::BackstoreMode::HostFs { ref mut root, .. } =
                        backstore_mode
                    {
                        *root = PathBuf::from(&args[i + 1]);
                    } else {
                        error!(
                            component = COMPONENT,
                            "--backstore-root can only be used with --backstore-mode HostFs"
                        );
                        std::process::exit(1);
                    }
                    i += 2;
                } else {
                    error!(
                        component = COMPONENT,
                        "--backstore-root requires an argument"
                    );
                    std::process::exit(1);
                }
            }
            "--backstore-size-mb" => {
                if i + 1 < args.len() {
                    // Update the backstore mode to RamDisk with the specified size
                    if let agentfs_core::config::BackstoreMode::RamDisk { ref mut size_mb } =
                        backstore_mode
                    {
                        *size_mb = args[i + 1].parse().unwrap_or(64);
                    } else {
                        error!(
                            component = COMPONENT,
                            "--backstore-size-mb can only be used with --backstore-mode RamDisk"
                        );
                        std::process::exit(1);
                    }
                    i += 2;
                } else {
                    error!(
                        component = COMPONENT,
                        "--backstore-size-mb requires an argument"
                    );
                    std::process::exit(1);
                }
            }
            arg => {
                // Assume this is the socket path if we haven't found it yet
                if socket_path.is_none() {
                    socket_path = Some(arg.to_string());
                    i += 1;
                } else {
                    error!(component = COMPONENT, "Unknown argument: {}", arg);
                    std::process::exit(1);
                }
            }
        }
    }

    let socket_path = socket_path.expect("Socket path is required");

    // Clean up any existing socket
    let _ = std::fs::remove_file(&socket_path);

    let listener = UnixListener::bind(&socket_path).expect("failed to bind socket");
    info!(
        component = COMPONENT,
        "AgentFS Daemon: listening on {}", socket_path
    );

    // Create the daemon instance (this will use the library implementation)
    let daemon =
        match AgentFsDaemon::new_with_backstore(lower_dir, upper_dir, work_dir, backstore_mode) {
            Ok(daemon) => Arc::new(Mutex::new(daemon)),
            Err(e) => {
                error!(
                    component = COMPONENT,
                    "Failed to create AgentFS daemon: {:?}", e
                );
                std::process::exit(1);
            }
        };

    info!(
        component = COMPONENT,
        "AgentFS Daemon: initialized successfully"
    );

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
                error!(component = COMPONENT, "AgentFS Daemon: accept error: {}", e);
                break;
            }
        }
    }

    info!(component = COMPONENT, "AgentFS Daemon: shutting down");
}

/// Handle a client connection with the daemon
/// This function is moved from the daemon.rs module to here as it's specific to the executable
fn handle_client(mut stream: UnixStream, daemon: Arc<Mutex<AgentFsDaemon>>, client_pid: u32) {
    // Register the process with the daemon
    {
        let mut daemon = daemon.lock().unwrap();
        if let Err(err) = daemon.register_process(client_pid, 0, 0, 0) {
            error!(
                component = COMPONENT,
                "AgentFS Daemon: register_process failed for pid {}: {}", client_pid, err
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
                debug!(
                    component = COMPONENT,
                    "AgentFS Daemon (bin): received request: {:?}", request
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
                                debug!(
                                    component = COMPONENT,
                                    "AgentFS Daemon: snapshot_export error: {}", err
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
                                debug!(
                                    component = COMPONENT,
                                    "AgentFS Daemon: snapshot_export_release error: {}", err
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
                                let response = Response::error(err, Some(5));
                                send_response(&mut stream, &response);
                            }
                        }
                    }
                    Request::SnapshotList(_version) => {
                        match daemon.lock().unwrap().handle_snapshot_list() {
                            Ok(snapshots) => {
                                let response = Response::snapshot_list(snapshots);
                                send_response(&mut stream, &response);
                            }
                            Err(err) => {
                                let response = Response::error(err, Some(5));
                                send_response(&mut stream, &response);
                            }
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
                    }) => {
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
                    Request::DaemonStateStats(DaemonStateStatsRequest { data: _version }) => {
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
                    Request::Readlink((_version, readlink_req)) => {
                        let path = String::from_utf8_lossy(&readlink_req.path).to_string();
                        debug!(
                            component = COMPONENT,
                            "AgentFS Daemon: readlink({}, pid={})", path, client_pid
                        );
                        let mut daemon = daemon.lock().unwrap();
                        match daemon.handle_readlink(path, client_pid) {
                            Ok(target) => {
                                debug!(
                                    component = COMPONENT,
                                    "AgentFS Daemon: readlink succeeded, target: {}", target
                                );
                                let response = Response::readlink(target);
                                send_response(&mut stream, &response);
                            }
                            Err(e) => {
                                debug!(
                                    component = COMPONENT,
                                    "AgentFS Daemon: readlink failed: {}", e
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
                            "AgentFS Daemon: dir_read(handle={})", handle
                        );
                        let mut daemon = daemon.lock().unwrap();
                        match daemon.handle_dir_read(handle, client_pid) {
                            Ok(entries) => {
                                debug!(
                                    component = COMPONENT,
                                    "AgentFS Daemon: dir_read succeeded, {} entries",
                                    entries.len()
                                );
                                let response = Response::dir_read(entries);
                                send_response(&mut stream, &response);
                            }
                            Err(e) => {
                                debug!(
                                    component = COMPONENT,
                                    "AgentFS Daemon: dir_read failed: {}", e
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
                            "AgentFS Daemon: dir_close(handle={})", handle
                        );
                        let mut daemon = daemon.lock().unwrap();
                        match daemon.handle_dir_close(handle, client_pid) {
                            Ok(()) => {
                                debug!(
                                    component = COMPONENT,
                                    "AgentFS Daemon: dir_close succeeded"
                                );
                                let response = Response::dir_close();
                                send_response(&mut stream, &response);
                            }
                            Err(e) => {
                                debug!(
                                    component = COMPONENT,
                                    "AgentFS Daemon: dir_close failed: {}", e
                                );
                                let response =
                                    Response::error(format!("dir_close failed: {}", e), Some(3));
                                send_response(&mut stream, &response);
                            }
                        }
                    }
                    Request::FdDup((_version, fd_dup_req)) => {
                        let fd = fd_dup_req.fd;
                        debug!(component = COMPONENT, "AgentFS Daemon: fd_dup(fd={})", fd);
                        let mut daemon = daemon.lock().unwrap();
                        match daemon.handle_fd_dup(fd, client_pid) {
                            Ok(duped_fd) => {
                                debug!(
                                    component = COMPONENT,
                                    "AgentFS Daemon: fd_dup succeeded, new fd: {}", duped_fd
                                );
                                let response = Response::fd_dup(duped_fd);
                                send_response(&mut stream, &response);
                            }
                            Err(e) => {
                                debug!(
                                    component = COMPONENT,
                                    "AgentFS Daemon: fd_dup failed: {}", e
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
                            "AgentFS Daemon: path_op(path={}, op={})", path, operation
                        );
                        let mut daemon = daemon.lock().unwrap();
                        match daemon.handle_path_op(path, operation, path_op_req.args, client_pid) {
                            Ok(result) => {
                                debug!(component = COMPONENT, "AgentFS Daemon: path_op succeeded");
                                let response = Response::path_op(result);
                                send_response(&mut stream, &response);
                            }
                            Err(e) => {
                                debug!(
                                    component = COMPONENT,
                                    "AgentFS Daemon: path_op failed: {}", e
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
                            "AgentFS Daemon: processing filesystem state query with max_depth={}, include_overlay={}, max_file_size={}",
                            query.max_depth,
                            query.include_overlay,
                            query.max_file_size
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
                                debug!(
                                    component = COMPONENT,
                                    "AgentFS Daemon: filesystem state query successful, {} entries",
                                    entry_count
                                );
                                let response = Response::DaemonState(response);
                                send_response(&mut stream, &response);
                            }
                            Err(e) => {
                                debug!(
                                    component = COMPONENT,
                                    "AgentFS Daemon: filesystem state query failed: {}", e
                                );
                                let response = Response::error(
                                    format!("daemon_state_filesystem failed: {}", e),
                                    Some(4),
                                );
                                send_response(&mut stream, &response);
                            }
                        }
                    }
                    Request::WatchRegisterKqueue((_version, watch_reg_req)) => {
                        let daemon = daemon.lock().unwrap();
                        // TODO: Get path from FD mapping - for now use placeholder
                        let path = format!("/fd/{}", watch_reg_req.fd);
                        let registration_id = daemon.register_kqueue_watch(
                            watch_reg_req.pid,
                            watch_reg_req.kq_fd,
                            watch_reg_req.watch_id,
                            watch_reg_req.fd,
                            path,
                            watch_reg_req.fflags,
                        );
                        let response = Response::watch_register_kqueue(registration_id);
                        send_response(&mut stream, &response);
                    }
                    Request::WatchRegisterFSEvents((_version, watch_reg_req)) => {
                        let root_paths: Vec<String> = watch_reg_req
                            .root_paths
                            .iter()
                            .map(|p| String::from_utf8_lossy(p).to_string())
                            .collect();
                        let daemon = daemon.lock().unwrap();
                        let registration_id = daemon.register_fsevents_watch(
                            watch_reg_req.pid,
                            watch_reg_req.stream_id,
                            root_paths,
                            watch_reg_req.flags,
                            watch_reg_req.latency,
                        );
                        let response = Response::watch_register_fsevents(registration_id);
                        send_response(&mut stream, &response);
                    }
                    Request::WatchRegisterFSEventsPort((_version, port_reg_req)) => {
                        #[cfg_attr(not(target_os = "macos"), allow(unused_variables))]
                        let port_name =
                            String::from_utf8_lossy(&port_reg_req.port_name).to_string();
                        let _daemon = daemon.lock().unwrap();

                        // Create CFMessagePort remote connection
                        #[cfg(target_os = "macos")]
                        {
                            if std::ffi::CString::new(port_name.as_str()).is_err() {
                                log::error!("Invalid port name encoding: {}", port_name);
                            } else {
                                match create_remote_port(&port_name) {
                                    Ok(port) => {
                                        daemon
                                            .lock()
                                            .unwrap()
                                            .register_fsevents_port(port_reg_req.pid, port);
                                        log::info!(
                                            "Registered FSEvents CFMessagePort for pid {}: {}",
                                            port_reg_req.pid,
                                            port_name
                                        );
                                    }
                                    Err(err) => {
                                        log::error!(
                                            "Failed to create CFMessagePort remote for pid {}: {}",
                                            port_reg_req.pid,
                                            err
                                        );
                                    }
                                }
                            }
                        }

                        let response = Response::watch_register_fsevents_port();
                        send_response(&mut stream, &response);
                    }
                    Request::WatchUnregister((_version, watch_unreg_req)) => {
                        let daemon = daemon.lock().unwrap();
                        daemon
                            .unregister_watch(watch_unreg_req.pid, watch_unreg_req.registration_id);
                        let response = Response::watch_unregister();
                        send_response(&mut stream, &response);
                    }
                    Request::WatchDoorbell((_version, doorbell_req)) => {
                        // Handle WatchDoorbell - the kqueue FD should be received via SCM_RIGHTS
                        // TODO: Implement proper SCM_RIGHTS reception to get the actual kqueue FD
                        // For now, just acknowledge and set the doorbell ident in watch service
                        let daemon = daemon.lock().unwrap();
                        daemon.watch_service().set_doorbell(
                            doorbell_req.pid,
                            doorbell_req.kq_fd,
                            doorbell_req.doorbell_ident,
                        );

                        // TODO: Receive the actual kqueue FD via SCM_RIGHTS and store it
                        // daemon.watch_service().store_kqueue_fd(doorbell_req.pid, doorbell_req.kq_fd, received_fd);

                        debug!(
                            component = COMPONENT,
                            "AgentFS Daemon: registered doorbell ident {:#x} for kqueue fd {} from pid {}",
                            doorbell_req.doorbell_ident,
                            doorbell_req.kq_fd,
                            doorbell_req.pid
                        );
                        let response = Response::watch_doorbell();
                        send_response(&mut stream, &response);
                    }
                    Request::UpdateDoorbellIdent((_version, update_req)) => {
                        let daemon = daemon.lock().unwrap();
                        // Find the kqueue fd for this pid
                        if let Some(kq_fd) =
                            daemon.watch_service().find_kqueue_fd_for_pid(update_req.pid)
                        {
                            daemon.watch_service().set_doorbell(
                                update_req.pid,
                                kq_fd,
                                update_req.new_ident,
                            );
                            debug!(
                                component = COMPONENT,
                                "AgentFS Daemon: updated doorbell ident from {:#x} to {:#x} for pid {} kq_fd {}",
                                update_req.old_ident,
                                update_req.new_ident,
                                update_req.pid,
                                kq_fd
                            );
                        } else {
                            debug!(
                                component = COMPONENT,
                                "AgentFS Daemon: warning - no kqueue found for pid {} when updating doorbell ident",
                                update_req.pid
                            );
                        }
                        let response = Response::update_doorbell_ident();
                        send_response(&mut stream, &response);
                    }
                    Request::QueryDoorbellIdent((_version, query_req)) => {
                        let daemon = daemon.lock().unwrap();
                        // Look up the current doorbell ident for this pid (legacy method for compatibility)
                        let current_ident =
                            daemon.watch_service().get_doorbell_ident_legacy(query_req.pid);
                        debug!(
                            component = COMPONENT,
                            "AgentFS Daemon: queried doorbell ident for pid {}: {:#x}",
                            query_req.pid,
                            current_ident
                        );
                        let response = Response::query_doorbell_ident(current_ident);
                        send_response(&mut stream, &response);
                    }
                    Request::FsEventBroadcast((_version, _event_broadcast_req)) => {
                        // Handle FsCore event broadcast to shim
                        // This would trigger the watch service to route events
                        // For now, just acknowledge
                        let response = Response::fs_event_broadcast();
                        send_response(&mut stream, &response);
                    }
                    Request::WatchDrainEvents((_version, drain_req)) => {
                        let daemon = daemon.lock().unwrap();
                        debug!(
                            component = COMPONENT,
                            "AgentFS Daemon: watch_drain_events(pid={}, kq_fd={}, max_events={})",
                            drain_req.pid,
                            drain_req.kq_fd,
                            drain_req.max_events
                        );

                        // Drain pending events for this kqueue
                        let events = daemon.watch_service().drain_events(
                            drain_req.pid,
                            drain_req.kq_fd,
                            drain_req.max_events as usize,
                        );

                        debug!(
                            component = COMPONENT,
                            "AgentFS Daemon: drained {} events for kqueue (pid={}, fd={})",
                            events.len(),
                            drain_req.pid,
                            drain_req.kq_fd
                        );

                        let response = Response::watch_drain_events_response(events);
                        send_response(&mut stream, &response);
                    }
                    Request::WatchUnregisterFd((_version, unregister_fd_req)) => {
                        let daemon = daemon.lock().unwrap();
                        debug!(
                            component = COMPONENT,
                            "AgentFS Daemon: watch_unregister_fd(pid={}, fd={})",
                            unregister_fd_req.pid,
                            unregister_fd_req.fd
                        );

                        // Remove all watches for this fd from all kqueues for this pid
                        daemon
                            .watch_service()
                            .unregister_watches_by_fd(unregister_fd_req.pid, unregister_fd_req.fd);

                        let response = Response::watch_unregister_fd();
                        send_response(&mut stream, &response);
                    }
                    Request::WatchUnregisterKqueue((_version, unregister_kq_req)) => {
                        let daemon = daemon.lock().unwrap();
                        debug!(
                            component = COMPONENT,
                            "AgentFS Daemon: watch_unregister_kqueue(pid={}, kq_fd={})",
                            unregister_kq_req.pid,
                            unregister_kq_req.kq_fd
                        );

                        // Remove all watches for this kqueue and clean up kqueue state
                        daemon.watch_service().unregister_watches_for_kqueue(
                            unregister_kq_req.pid,
                            unregister_kq_req.kq_fd,
                        );

                        let response = Response::watch_unregister_kqueue();
                        send_response(&mut stream, &response);
                    }
                    // All other request types would be handled here...
                    _ => {
                        let response =
                            Response::error("unsupported request (bin)".to_string(), Some(3));
                        send_response(&mut stream, &response);
                    }
                }
            }
            Err(e) => {
                error!(
                    component = COMPONENT,
                    operation = "decode_request",
                    error = ?e,
                    "failed to decode request"
                );
                break;
            }
        }
    }

    // Cleanup: unregister the connection
    cleanup();
}

fn send_response(stream: &mut UnixStream, response: &Response) {
    let encoded = encode_ssz_message(&response);
    let len_bytes = (encoded.len() as u32).to_le_bytes();

    let _ = stream.write_all(&len_bytes);
    let _ = stream.write_all(&encoded);
    let _ = stream.flush();
}
