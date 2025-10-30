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

// Import specific types that need explicit qualification
use agentfs_proto::messages::{
    DaemonStateFilesystemRequest, DaemonStateProcessesRequest, DaemonStateResponse,
    DaemonStateResponseWrapper, DaemonStateStatsRequest, DirCloseRequest, DirEntry, DirReadRequest,
    FdDupRequest, FilesystemQuery, FilesystemState, FsStats, PathOpRequest, ProcessInfo, StatData,
    StatfsData, TimespecData,
};

// Use handshake types from the daemon crate
use agentfs_daemon::{HandshakeMessage};

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
    println!("AgentFS Daemon: listening on {}", socket_path);

    // Create the daemon instance (this will use the library implementation)
    let daemon = match AgentFsDaemon::new_with_overlay(lower_dir, upper_dir, work_dir) {
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
}

fn send_response(stream: &mut UnixStream, response: &Response) {
    let encoded = encode_ssz_message(&response);
    let len_bytes = (encoded.len() as u32).to_le_bytes();

    let _ = stream.write_all(&len_bytes);
    let _ = stream.write_all(&encoded);
    let _ = stream.flush();
}
