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
use ah_logging::{CliLogLevel, CliLoggingArgs};

// CLI argument parsing
use clap::Parser;

// AgentFS proto imports
use agentfs_proto::*;

#[cfg(target_os = "macos")]
use agentfs_daemon::macos::interposition::create_remote_port;

// Import specific types that need explicit qualification
use agentfs_proto::messages::{
    DaemonStateFilesystemRequest, DaemonStateProcessesRequest, DaemonStateResponse,
    DaemonStateStatsRequest, FstatRequest, FstatatRequest, LstatRequest, StatRequest,
};

use agentfs_core::{BranchId, PID, SnapshotId};

// Use handshake types from the daemon crate
use agentfs_daemon::HandshakeMessage;

const COMPONENT: &str = "agentfs-daemon";

#[derive(Parser)]
#[command(name = "agentfs-daemon")]
#[command(about = "AgentFS Daemon - Production-ready filesystem daemon with interpose support")]
#[command(version, author, long_about = None)]
struct Cli {
    /// Unix socket path for client connections
    socket_path: String,

    /// Lower directory for overlay filesystem
    #[arg(long)]
    lower_dir: Option<String>,

    /// Upper directory for overlay filesystem
    #[arg(long)]
    upper_dir: Option<String>,

    /// Work directory for overlay filesystem
    #[arg(long)]
    work_dir: Option<String>,

    /// Backstore mode for data persistence
    #[arg(long, default_value = "InMemory")]
    backstore_mode: String,

    /// Root directory for HostFs backstore mode
    #[arg(long)]
    backstore_root: Option<String>,

    /// Size in MB for RamDisk backstore mode
    #[arg(long, default_value = "64")]
    backstore_size_mb: u64,

    /// Owner UID for filesystem operations
    #[arg(long)]
    owner_uid: Option<u32>,

    /// Owner GID for filesystem operations
    #[arg(long)]
    owner_gid: Option<u32>,

    #[command(flatten)]
    logging: CliLoggingArgs,
}

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
    let cli = Cli::parse();

    // Initialize logging
    if let Err(e) = cli.logging.init_with_default_level("agentfs-daemon", false, CliLogLevel::Info)
    {
        let _ = writeln!(std::io::stderr(), "Failed to initialize logging: {}", e);
        std::process::exit(1);
    }

    // Parse backstore mode and handle special options
    let backstore_mode = match cli.backstore_mode.as_str() {
        "InMemory" => agentfs_core::config::BackstoreMode::InMemory,
        "HostFs" => {
            let root = cli
                .backstore_root
                .as_ref()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/tmp/agentfs-backstore"));
            agentfs_core::config::BackstoreMode::HostFs {
                root,
                prefer_native_snapshots: true,
            }
        }
        "RamDisk" => {
            let size_mb = cli.backstore_size_mb as u32;
            agentfs_core::config::BackstoreMode::RamDisk { size_mb }
        }
        _ => {
            error!(
                component = COMPONENT,
                "Invalid backstore mode: {}", cli.backstore_mode
            );
            std::process::exit(1);
        }
    };

    let socket_path = cli.socket_path.clone();

    // Clean up any existing socket
    let _ = std::fs::remove_file(&socket_path);

    let listener = UnixListener::bind(&socket_path).expect("failed to bind socket");
    info!(
        component = COMPONENT,
        "AgentFS Daemon: listening on {}", socket_path
    );

    // Create the daemon instance (this will use the library implementation)
    let daemon = match AgentFsDaemon::new_with_backstore(
        cli.lower_dir.as_ref().map(PathBuf::from),
        cli.upper_dir.as_ref().map(PathBuf::from),
        cli.work_dir.as_ref().map(PathBuf::from),
        backstore_mode,
        cli.owner_uid.zip(cli.owner_gid),
    ) {
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
            Ok(mut stream) => {
                // Perform handshake to get session information
                let mut len_buf = [0u8; 4];
                if stream.read_exact(&mut len_buf).is_err() {
                    continue;
                }

                let msg_len = u32::from_le_bytes(len_buf) as usize;
                let mut msg_buf = vec![0u8; msg_len];

                if stream.read_exact(&mut msg_buf).is_err() {
                    continue;
                }

                let handshake = match decode_ssz_message::<HandshakeMessage>(&msg_buf) {
                    Ok(HandshakeMessage::Handshake(data)) => data,
                    _ => {
                        continue;
                    }
                };

                let client_pid = handshake.process.pid;
                let client_ppid = handshake.process.ppid;
                let client_uid = handshake.process.uid;
                let client_gid = handshake.process.gid;
                let session_id = String::from_utf8_lossy(&handshake.session_id).to_string();
                info!(
                    component = COMPONENT,
                    "SESSION_ID_LOGGING: agentfs-daemon received session_id={}", session_id
                );

                // TODO: Why are we returning a non-SSZ value here?
                // Send back a simple acknowledgment
                if stream.write_all(b"OK\n").is_err() {
                    continue;
                }
                if stream.flush().is_err() {
                    continue;
                }

                let daemon_clone = daemon.clone();
                thread::spawn(move || {
                    handle_client_after_handshake(
                        stream,
                        daemon_clone,
                        client_pid,
                        client_ppid,
                        client_uid,
                        client_gid,
                        session_id,
                    );
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
fn handle_client_after_handshake(
    mut stream: UnixStream,
    daemon: Arc<Mutex<AgentFsDaemon>>,
    client_pid: u32,
    client_ppid: u32,
    client_uid: u32,
    client_gid: u32,
    session_id: String,
) {
    // Register the process with the daemon now that we know its identity
    {
        let mut daemon = daemon.lock().unwrap();
        if let Err(err) = daemon.register_process(client_pid, client_ppid, client_uid, client_gid) {
            error!(
                component = COMPONENT,
                "AgentFS Daemon: register_process failed for pid {}: {}", client_pid, err
            );
            return;
        }
        daemon.register_connection(
            client_pid,
            stream.try_clone().expect("Failed to clone stream"),
        );
    }

    // Ensure cleanup happens when function exits
    let daemon_clone = daemon.clone();
    let session_id_clone = session_id.clone();
    let cleanup = move || {
        let mut daemon = daemon_clone.lock().unwrap();
        daemon.unregister_connection(client_pid);
        daemon.cleanup_process_watches(client_pid, &session_id_clone);
    };

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
                    session_id = %session_id,
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

                        match daemon
                            .lock()
                            .unwrap()
                            .handle_snapshot_export(snapshot_id, Some(client_pid))
                        {
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
                                    session_id = %session_id,
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
                                    session_id = %session_id,
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

                        match daemon.lock().unwrap().handle_snapshot_create(
                            pid.as_u32(),
                            name,
                            &session_id,
                        ) {
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

                        match daemon.lock().unwrap().handle_branch_create(
                            snapshot_id,
                            name,
                            &session_id,
                        ) {
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
                            &session_id,
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
                        match daemon.handle_dir_open(path, client_pid, &session_id) {
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
                            session_id = %session_id,
                            "AgentFS Daemon: readlink({}, pid={})", path, client_pid
                        );
                        let mut daemon = daemon.lock().unwrap();
                        match daemon.handle_readlink(path, client_pid, &session_id) {
                            Ok(target) => {
                                debug!(
                                    component = COMPONENT,
                                    session_id = %session_id,
                                    "AgentFS Daemon: readlink succeeded, target: {}", target
                                );
                                let response = Response::readlink(target);
                                send_response(&mut stream, &response);
                            }
                            Err(e) => {
                                debug!(
                                    component = COMPONENT,
                                    session_id = %session_id,
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
                    // Metadata operations
                    Request::Stat((_version, stat_req)) => {
                        let path = String::from_utf8_lossy(&stat_req.path).to_string();
                        let pid = get_client_pid_helper(&daemon, client_pid);
                        match daemon
                            .lock()
                            .unwrap()
                            .core()
                            .lock()
                            .unwrap()
                            .stat(&pid, path.as_ref())
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
                        match daemon
                            .lock()
                            .unwrap()
                            .core()
                            .lock()
                            .unwrap()
                            .lstat(&pid, path.as_ref())
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
                        let handle_id = agentfs_core::HandleId(fstat_req.fd as u64);
                        match daemon.lock().unwrap().core().lock().unwrap().fstat(&pid, handle_id) {
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
                        match daemon.lock().unwrap().core().lock().unwrap().fstatat(
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
