// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! AgentFS Daemon executable - Production-ready filesystem daemon with interpose support

use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::io::RawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use libc;

// AgentFS daemon library
use agentfs_daemon::{AgentFsDaemon, decode_ssz_message, encode_ssz_message};

// AgentFS proto imports
use agentfs_proto::*;

#[cfg(target_os = "macos")]
use agentfs_daemon::macos::interposition::create_remote_port;

// Import specific types that need explicit qualification
use agentfs_proto::messages::{
    DaemonStateFilesystemRequest, DaemonStateProcessesRequest, DaemonStateResponse,
    DaemonStateResponseWrapper, DaemonStateStatsRequest, DirCloseRequest, DirEntry, DirReadRequest,
    FdDupRequest, FilesystemQuery, FilesystemState, FsStats, PathOpRequest, ProcessInfo, StatData,
    StatfsData, TimespecData,
};

// Use handshake types from the daemon crate
use agentfs_daemon::HandshakeMessage;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!(
            "Usage: {} <socket_path> [--backstore-mode <mode> [--backstore-root <path>] [--backstore-size-mb <mb>]] [--lower-dir <path>] [--upper-dir <path>] [--work-dir <path>]",
            args[0]
        );
        eprintln!("Backstore modes: InMemory, HostFs, RamDisk");
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
                            eprintln!("Invalid backstore mode: {}", args[i + 1]);
                            std::process::exit(1);
                        }
                    };
                    i += 2;
                } else {
                    eprintln!("--backstore-mode requires an argument");
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
                        eprintln!("--backstore-root can only be used with --backstore-mode HostFs");
                        std::process::exit(1);
                    }
                    i += 2;
                } else {
                    eprintln!("--backstore-root requires an argument");
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
                        eprintln!(
                            "--backstore-size-mb can only be used with --backstore-mode RamDisk"
                        );
                        std::process::exit(1);
                    }
                    i += 2;
                } else {
                    eprintln!("--backstore-size-mb requires an argument");
                    std::process::exit(1);
                }
            }
            arg => {
                // Assume this is the socket path if we haven't found it yet
                if socket_path.is_none() {
                    socket_path = Some(arg.to_string());
                    i += 1;
                } else {
                    eprintln!("Unknown argument: {}", arg);
                    std::process::exit(1);
                }
            }
        }
    }

    let socket_path = socket_path.expect("Socket path is required");

    // Clean up any existing socket
    let _ = std::fs::remove_file(&socket_path);

    let listener = UnixListener::bind(&socket_path).expect("failed to bind socket");
    println!("AgentFS Daemon: listening on {}", socket_path);

    // Create the daemon instance (this will use the library implementation)
    let daemon =
        match AgentFsDaemon::new_with_backstore(lower_dir, upper_dir, work_dir, backstore_mode) {
            Ok(daemon) => Arc::new(Mutex::new(daemon)),
            Err(e) => {
                eprintln!("Failed to create AgentFS daemon: {:?}", e);
                std::process::exit(1);
            }
        };

    println!("AgentFS Daemon: initialized successfully");

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
                eprintln!("AgentFS Daemon: accept error: {}", e);
                break;
            }
        }
    }

    println!("AgentFS Daemon: shutting down");
}

/// Handle a client connection with the daemon
/// This function is moved from the daemon.rs module to here as it's specific to the executable
fn handle_client(mut stream: UnixStream, daemon: Arc<Mutex<AgentFsDaemon>>, client_pid: u32) {
    // Register the process with the daemon
    {
        let mut daemon = daemon.lock().unwrap();
        if let Err(e) = daemon.register_process(client_pid, 0, 0, 0) {
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

    if let Ok(handshake) = decode_ssz_message::<HandshakeMessage>(&msg_buf) {
        // Send back a simple text acknowledgment
        let ack = b"OK\n";
        let _ = stream.write_all(ack);
        let _ = stream.flush();
    } else {
        cleanup();
        return;
    }

    // Handle one request
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
                    println!("AgentFS Daemon: readlink({}, pid={})", path, client_pid);
                    let mut daemon = daemon.lock().unwrap();
                    match daemon.handle_readlink(path, client_pid) {
                        Ok(target) => {
                            println!("AgentFS Daemon: readlink succeeded, target: {}", target);
                            let response = Response::readlink(target);
                            send_response(&mut stream, &response);
                        }
                        Err(e) => {
                            println!("AgentFS Daemon: readlink failed: {}", e);
                            let response =
                                Response::error(format!("readlink failed: {}", e), Some(2));
                            send_response(&mut stream, &response);
                        }
                    }
                }
                Request::DirRead((version, dir_read_req)) => {
                    let handle = dir_read_req.handle;
                    println!("AgentFS Daemon: dir_read(handle={})", handle);
                    let mut daemon = daemon.lock().unwrap();
                    match daemon.handle_dir_read(handle, client_pid) {
                        Ok(entries) => {
                            println!(
                                "AgentFS Daemon: dir_read succeeded, {} entries",
                                entries.len()
                            );
                            let response = Response::dir_read(entries);
                            send_response(&mut stream, &response);
                        }
                        Err(e) => {
                            println!("AgentFS Daemon: dir_read failed: {}", e);
                            let response =
                                Response::error(format!("dir_read failed: {}", e), Some(3));
                            send_response(&mut stream, &response);
                        }
                    }
                }
                Request::DirClose((version, dir_close_req)) => {
                    let handle = dir_close_req.handle;
                    println!("AgentFS Daemon: dir_close(handle={})", handle);
                    let mut daemon = daemon.lock().unwrap();
                    match daemon.handle_dir_close(handle, client_pid) {
                        Ok(()) => {
                            println!("AgentFS Daemon: dir_close succeeded");
                            let response = Response::dir_close();
                            send_response(&mut stream, &response);
                        }
                        Err(e) => {
                            println!("AgentFS Daemon: dir_close failed: {}", e);
                            let response =
                                Response::error(format!("dir_close failed: {}", e), Some(3));
                            send_response(&mut stream, &response);
                        }
                    }
                }
                Request::FdDup((version, fd_dup_req)) => {
                    let fd = fd_dup_req.fd;
                    println!("AgentFS Daemon: fd_dup(fd={})", fd);
                    let mut daemon = daemon.lock().unwrap();
                    match daemon.handle_fd_dup(fd, client_pid) {
                        Ok(duped_fd) => {
                            println!("AgentFS Daemon: fd_dup succeeded, new fd: {}", duped_fd);
                            let response = Response::fd_dup(duped_fd);
                            send_response(&mut stream, &response);
                        }
                        Err(e) => {
                            println!("AgentFS Daemon: fd_dup failed: {}", e);
                            let response =
                                Response::error(format!("fd_dup failed: {}", e), Some(2));
                            send_response(&mut stream, &response);
                        }
                    }
                }
                Request::PathOp((version, path_op_req)) => {
                    let path = String::from_utf8_lossy(&path_op_req.path).to_string();
                    let operation = String::from_utf8_lossy(&path_op_req.operation).to_string();
                    println!("AgentFS Daemon: path_op(path={}, op={})", path, operation);
                    let mut daemon = daemon.lock().unwrap();
                    match daemon.handle_path_op(path, operation, path_op_req.args, client_pid) {
                        Ok(result) => {
                            println!("AgentFS Daemon: path_op succeeded");
                            let response = Response::path_op(result);
                            send_response(&mut stream, &response);
                        }
                        Err(e) => {
                            println!("AgentFS Daemon: path_op failed: {}", e);
                            let response =
                                Response::error(format!("path_op failed: {}", e), Some(4));
                            send_response(&mut stream, &response);
                        }
                    }
                }
                Request::DaemonStateFilesystem(DaemonStateFilesystemRequest { query }) => {
                    println!(
                        "AgentFS Daemon: processing filesystem state query with max_depth={}, include_overlay={}, max_file_size={}",
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
                                "AgentFS Daemon: filesystem state query successful, {} entries",
                                entry_count
                            );
                            let response = Response::DaemonState(response);
                            send_response(&mut stream, &response);
                        }
                        Err(e) => {
                            println!("AgentFS Daemon: filesystem state query failed: {}", e);
                            let response = Response::error(
                                format!("daemon_state_filesystem failed: {}", e),
                                Some(4),
                            );
                            send_response(&mut stream, &response);
                        }
                    }
                }
                Request::WatchRegisterKqueue((version, watch_reg_req)) => {
                    let mut daemon = daemon.lock().unwrap();
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
                Request::WatchRegisterFSEvents((version, watch_reg_req)) => {
                    let root_paths: Vec<String> = watch_reg_req
                        .root_paths
                        .iter()
                        .map(|p| String::from_utf8_lossy(p).to_string())
                        .collect();
                    let mut daemon = daemon.lock().unwrap();
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
                Request::WatchRegisterFSEventsPort((version, port_reg_req)) => {
                    let port_name = String::from_utf8_lossy(&port_reg_req.port_name).to_string();
                    let mut daemon = daemon.lock().unwrap();

                    // Create CFMessagePort remote connection
                    #[cfg(target_os = "macos")]
                    {
                        if std::ffi::CString::new(port_name.as_str()).is_err() {
                            log::error!("Invalid port name encoding: {}", port_name);
                        } else {
                            match create_remote_port(&port_name) {
                                Ok(port) => {
                                    daemon.register_fsevents_port(port_reg_req.pid, port);
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
                Request::WatchUnregister((version, watch_unreg_req)) => {
                    let mut daemon = daemon.lock().unwrap();
                    daemon.unregister_watch(watch_unreg_req.pid, watch_unreg_req.registration_id);
                    let response = Response::watch_unregister();
                    send_response(&mut stream, &response);
                }
                Request::WatchDoorbell((version, doorbell_req)) => {
                    // Handle WatchDoorbell - the kqueue FD should be received via SCM_RIGHTS
                    // TODO: Implement proper SCM_RIGHTS reception to get the actual kqueue FD
                    // For now, just acknowledge and set the doorbell ident in watch service
                    let mut daemon = daemon.lock().unwrap();
                    daemon.watch_service().set_doorbell(
                        doorbell_req.pid,
                        doorbell_req.kq_fd,
                        doorbell_req.doorbell_ident,
                    );

                    // TODO: Receive the actual kqueue FD via SCM_RIGHTS and store it
                    // daemon.watch_service().store_kqueue_fd(doorbell_req.pid, doorbell_req.kq_fd, received_fd);

                    println!(
                        "AgentFS Daemon: registered doorbell ident {:#x} for kqueue fd {} from pid {}",
                        doorbell_req.doorbell_ident, doorbell_req.kq_fd, doorbell_req.pid
                    );
                    let response = Response::watch_doorbell();
                    send_response(&mut stream, &response);
                }
                Request::UpdateDoorbellIdent((version, update_req)) => {
                    let mut daemon = daemon.lock().unwrap();
                    // Find the kqueue fd for this pid
                    if let Some(kq_fd) =
                        daemon.watch_service().find_kqueue_fd_for_pid(update_req.pid)
                    {
                        daemon.watch_service().set_doorbell(
                            update_req.pid,
                            kq_fd,
                            update_req.new_ident,
                        );
                        println!(
                            "AgentFS Daemon: updated doorbell ident from {:#x} to {:#x} for pid {} kq_fd {}",
                            update_req.old_ident, update_req.new_ident, update_req.pid, kq_fd
                        );
                    } else {
                        println!(
                            "AgentFS Daemon: warning - no kqueue found for pid {} when updating doorbell ident",
                            update_req.pid
                        );
                    }
                    let response = Response::update_doorbell_ident();
                    send_response(&mut stream, &response);
                }
                Request::QueryDoorbellIdent((version, query_req)) => {
                    let daemon = daemon.lock().unwrap();
                    // Look up the current doorbell ident for this pid (legacy method for compatibility)
                    let current_ident =
                        daemon.watch_service().get_doorbell_ident_legacy(query_req.pid);
                    println!(
                        "AgentFS Daemon: queried doorbell ident for pid {}: {:#x}",
                        query_req.pid, current_ident
                    );
                    let response = Response::query_doorbell_ident(current_ident);
                    send_response(&mut stream, &response);
                }
                Request::FsEventBroadcast((version, event_broadcast_req)) => {
                    // Handle FsCore event broadcast to shim
                    // This would trigger the watch service to route events
                    // For now, just acknowledge
                    let response = Response::fs_event_broadcast();
                    send_response(&mut stream, &response);
                }
                Request::WatchDrainEvents((version, drain_req)) => {
                    let daemon = daemon.lock().unwrap();
                    println!(
                        "AgentFS Daemon: watch_drain_events(pid={}, kq_fd={}, max_events={})",
                        drain_req.pid, drain_req.kq_fd, drain_req.max_events
                    );

                    // Drain pending events for this kqueue
                    let events = daemon.watch_service().drain_events(
                        drain_req.pid,
                        drain_req.kq_fd,
                        drain_req.max_events as usize,
                    );

                    println!(
                        "AgentFS Daemon: drained {} events for kqueue (pid={}, fd={})",
                        events.len(),
                        drain_req.pid,
                        drain_req.kq_fd
                    );

                    let response = Response::watch_drain_events_response(events);
                    send_response(&mut stream, &response);
                }
                Request::WatchUnregisterFd((version, unregister_fd_req)) => {
                    let mut daemon = daemon.lock().unwrap();
                    println!(
                        "AgentFS Daemon: watch_unregister_fd(pid={}, fd={})",
                        unregister_fd_req.pid, unregister_fd_req.fd
                    );

                    // Remove all watches for this fd from all kqueues for this pid
                    daemon
                        .watch_service()
                        .unregister_watches_by_fd(unregister_fd_req.pid, unregister_fd_req.fd);

                    let response = Response::watch_unregister_fd();
                    send_response(&mut stream, &response);
                }
                Request::WatchUnregisterKqueue((version, unregister_kq_req)) => {
                    let mut daemon = daemon.lock().unwrap();
                    println!(
                        "AgentFS Daemon: watch_unregister_kqueue(pid={}, kq_fd={})",
                        unregister_kq_req.pid, unregister_kq_req.kq_fd
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
                    let response = Response::error("unsupported request".to_string(), Some(3));
                    send_response(&mut stream, &response);
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to decode request: {:?}", e);
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
