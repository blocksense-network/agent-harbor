// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::io::RawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use libc;

// AgentFS core imports
use agentfs_core::{
    FsCore, PID,
    config::{FsConfig, InterposeConfig},
    error::FsResult,
};

// AgentFS proto imports
use agentfs_proto::*;

// SSZ imports
use ssz::{Decode, Encode};
use ssz_derive::{Decode as SSZDecode, Encode as SSZEncode};

// Import the helper functions from the main crate
// For now, define them locally since we can't import from the crate in the fixture
fn encode_ssz<T: ssz::Encode>(value: &T) -> Vec<u8> {
    value.as_ssz_bytes()
}

fn decode_ssz<T: ssz::Decode>(bytes: &[u8]) -> Result<T, ssz::DecodeError> {
    T::from_ssz_bytes(bytes)
}

// Define handshake message types locally (simplified)
#[derive(Clone, Debug, PartialEq, SSZEncode, SSZDecode)]
#[ssz(enum_behaviour = "union")]
enum HandshakeMessage {
    Handshake(HandshakeData),
}

#[derive(Clone, Debug, PartialEq, SSZEncode, SSZDecode)]
struct HandshakeData {
    version: Vec<u8>,
    shim: ShimInfo,
    process: ProcessInfo,
    allowlist: AllowlistInfo,
    timestamp: Vec<u8>,
    session_id: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, SSZEncode, SSZDecode)]
struct ShimInfo {
    name: Vec<u8>,
    crate_version: Vec<u8>,
    features: Vec<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, SSZEncode, SSZDecode)]
struct ProcessInfo {
    pid: u32,
    ppid: u32,
    uid: u32,
    gid: u32,
    exe_path: Vec<u8>,
    exe_name: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, SSZEncode, SSZDecode)]
struct AllowlistInfo {
    matched_entry: Option<Vec<u8>>,
    configured_entries: Option<Vec<Vec<u8>>>,
}

/// Real AgentFS daemon using the core filesystem
struct AgentFsDaemon {
    core: FsCore,
    processes: HashMap<u32, PID>, // pid -> registered PID
}

impl AgentFsDaemon {
    fn new() -> FsResult<Self> {
        // Create a basic configuration for the daemon
        // In a real implementation, this would be configurable
        let config = FsConfig {
            interpose: InterposeConfig {
                enabled: true,
                max_copy_bytes: 64 * 1024 * 1024, // 64MB
                require_reflink: false,
                allow_windows_reparse: false,
            },
            ..Default::default()
        };

        let core = FsCore::new(config)?;

        Ok(Self {
            core,
            processes: HashMap::new(),
        })
    }

    fn register_process(&mut self, pid: u32, ppid: u32, uid: u32, gid: u32) -> FsResult<PID> {
        let registered_pid = self.core.register_process(pid, ppid, uid, gid);
        self.processes.insert(pid, registered_pid.clone());
        Ok(registered_pid)
    }

    fn registered_pid(&self, os_pid: u32) -> Option<PID> {
        self.processes.get(&os_pid).cloned()
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
            Ok(fd as RawFd)
        }
    }
}

fn handle_client(mut stream: UnixStream, daemon: Arc<Mutex<AgentFsDaemon>>, client_pid: u32) {
    println!("AgentFsDaemon: new client connected (pid: {})", client_pid);

    // Register the process with the daemon
    {
        let mut daemon = daemon.lock().unwrap();
        if let Err(e) = daemon.register_process(client_pid, 0, 0, 0) {
            println!(
                "AgentFsDaemon: failed to register process {}: {:?}",
                client_pid, e
            );
            return;
        }
    }

    loop {
        // Read message length
        let mut len_buf = [0u8; 4];
        if stream.read_exact(&mut len_buf).is_err() {
            println!("AgentFsDaemon: client disconnected");
            break;
        }

        let msg_len = u32::from_le_bytes(len_buf) as usize;
        let mut msg_buf = vec![0u8; msg_len];

        if stream.read_exact(&mut msg_buf).is_err() {
            println!("AgentFsDaemon: failed to read message");
            break;
        }

        // Try to decode as handshake message first, then as regular request
        if let Ok(handshake) = decode_ssz::<HandshakeMessage>(&msg_buf) {
            println!("AgentFsDaemon: received handshake: {:?}", handshake);
            // Send back a simple text acknowledgment
            let ack = b"OK\n";
            let _ = stream.write_all(ack);
            continue;
        }

        // Try to decode as regular request
        match decode_ssz::<Request>(&msg_buf) {
            Ok(request) => {
                println!("AgentFsDaemon: received request: {:?}", request);
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
                    _ => {
                        let response = Response::error("unsupported request".to_string(), Some(3));
                        send_response(&mut stream, &response);
                    }
                }
            }
            Err(e) => {
                println!("AgentFsDaemon: failed to decode message: {:?}", e);
                break;
            }
        }
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
    msg.msg_controllen = cmsg_buf.len() as libc::c_uint;

    let cmsg = unsafe { libc::CMSG_FIRSTHDR(&msg) };
    if cmsg.is_null() {
        return Err("failed to get control message header".to_string());
    }

    unsafe {
        (*cmsg).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<RawFd>() as libc::c_uint);
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
    let encoded = encode_ssz(response);
    let len_bytes = (encoded.len() as u32).to_le_bytes();

    let _ = stream.write_all(&len_bytes);
    let _ = stream.write_all(&encoded);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <socket_path>", args[0]);
        std::process::exit(1);
    }

    let socket_path = &args[1];

    // Clean up any existing socket
    let _ = std::fs::remove_file(socket_path);

    let listener = UnixListener::bind(socket_path).expect("failed to bind socket");
    println!("AgentFsDaemon: listening on {}", socket_path);

    let daemon = match AgentFsDaemon::new() {
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
