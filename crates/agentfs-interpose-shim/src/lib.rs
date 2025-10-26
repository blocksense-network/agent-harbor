// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use once_cell::sync::{Lazy, OnceCell};
use std::collections::HashMap;
use std::ffi::{CStr, OsStr};
use std::io::{BufRead, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::io::RawFd;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};


// SSZ imports
use ssz::{Decode, Encode};
use ssz_derive::{Decode, Encode};

// AgentFS proto imports
use agentfs_proto::*;
use agentfs_proto::messages::DirEntry;

#[cfg(target_os = "macos")]
use std::os::unix::net::UnixStream;

const LOG_PREFIX: &str = "[agentfs-interpose]";
const ENV_ENABLED: &str = "AGENTFS_INTERPOSE_ENABLED";
const ENV_SOCKET: &str = "AGENTFS_INTERPOSE_SOCKET";
const ENV_ALLOWLIST: &str = "AGENTFS_INTERPOSE_ALLOWLIST";
const ENV_LOG_LEVEL: &str = "AGENTFS_INTERPOSE_LOG";
const ENV_FAIL_FAST: &str = "AGENTFS_INTERPOSE_FAIL_FAST";
const DEFAULT_BANNER: &str = "AgentFS interpose shim loaded";

static INIT_GUARD: OnceCell<()> = OnceCell::new();
#[cfg(target_os = "macos")]
static STREAM: Mutex<Option<Arc<Mutex<UnixStream>>>> = Mutex::new(None);

// Directory handles are now managed by FsCore directly

#[cfg(test)]
static ENV_GUARD: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[cfg(test)]
fn set_env_var(key: &str, value: &str) {
    unsafe { std::env::set_var(key, value) };
}

#[cfg(test)]
fn remove_env_var(key: &str) {
    unsafe { std::env::remove_var(key) };
}

#[cfg(target_os = "macos")]
#[ctor::ctor]
fn initialize() {
    if INIT_GUARD.set(()).is_err() {
        return;
    }

    if !is_enabled() {
        return;
    }

    log_message(DEFAULT_BANNER);

    let exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(err) => {
            log_message(&format!("failed to resolve current executable: {err}"));
            PathBuf::from("<unknown>")
        }
    };

    let allow = AllowDecision::from_env(&exe);
    if !allow.allowed {
        let fail_fast = std::env::var_os(ENV_FAIL_FAST).map(|s| s == "1").unwrap_or(false);
        if fail_fast {
            log_message(&format!(
                "process '{}' not present in allowlist but AGENTFS_INTERPOSE_FAIL_FAST=1; terminating program",
                exe.display()
            ));
            std::process::exit(1);
        } else {
            log_message(&format!(
                "process '{}' not present in allowlist; skipping handshake",
                exe.display()
            ));
            return;
        }
    }

    let socket_env = std::env::var_os(ENV_SOCKET);
    log_message(&format!("{ENV_SOCKET} = {:?}", socket_env));
    let Some(socket_path) = socket_env.map(PathBuf::from) else {
        let fail_fast = std::env::var_os(ENV_FAIL_FAST).map(|s| s == "1").unwrap_or(false);
        if fail_fast {
            log_message(&format!(
                "{ENV_SOCKET} not set but AGENTFS_INTERPOSE_FAIL_FAST=1; terminating program"
            ));
            std::process::exit(1);
        } else {
            log_message(&format!("{ENV_SOCKET} not set; skipping handshake"));
            return;
        }
    };

    match attempt_handshake(&socket_path, &exe, &allow) {
        Ok(stream) => {
            let mut stream_guard = STREAM.lock().unwrap();
            if stream_guard.is_none() {
                *stream_guard = Some(Arc::new(Mutex::new(stream)));
                log_message("STREAM set successfully in ctor");
            } else {
                log_message("STREAM already set in ctor");
            }
        }
        Err(err) => {
            log_message(&format!("handshake failed: {err}"));
            // Check if we should fail fast
            let fail_fast = std::env::var_os(ENV_FAIL_FAST).map(|s| s == "1").unwrap_or(false);
            if fail_fast {
                log_message(&format!(
                    "AGENTFS_INTERPOSE_FAIL_FAST=1 set, terminating program due to handshake failure"
                ));
                std::process::exit(1);
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
#[ctor::ctor]
fn initialize() {}

fn is_enabled() -> bool {
    match std::env::var(ENV_ENABLED) {
        Ok(value) => matches!(value.as_str(), "1" | "true" | "TRUE" | "True"),
        Err(std::env::VarError::NotPresent) => true,
        Err(err) => {
            log_message(&format!("unable to read {ENV_ENABLED}: {err}"));
            true
        }
    }
}

fn log_message(message: &str) {
    if std::env::var_os(ENV_LOG_LEVEL).as_deref().map_or(false, |v| {
        matches!(v.to_str(), Some("0" | "false" | "FALSE" | "False"))
    }) {
        return;
    }
    let pid = std::process::id();
    eprintln!("{LOG_PREFIX} [pid={pid}] {message}");
}

#[derive(Debug)]
struct AllowDecision {
    allowed: bool,
    matched_entry: Option<String>,
    raw_entries: Option<Vec<String>>,
}

impl AllowDecision {
    fn from_env(exe_path: &Path) -> Self {
        let Some(raw) = std::env::var(ENV_ALLOWLIST).ok().filter(|s| !s.trim().is_empty()) else {
            return Self {
                allowed: true,
                matched_entry: None,
                raw_entries: None,
            };
        };

        let entries: Vec<String> =
            raw.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();

        if entries.is_empty() {
            return Self {
                allowed: true,
                matched_entry: None,
                raw_entries: Some(entries),
            };
        }

        let exe_basename =
            exe_path.file_name().and_then(OsStr::to_str).unwrap_or("").to_ascii_lowercase();

        let exe_display = exe_path.display().to_string().to_ascii_lowercase();

        for entry in &entries {
            if entry == "*" {
                return Self {
                    allowed: true,
                    matched_entry: Some(entry.clone()),
                    raw_entries: Some(entries),
                };
            }
            let entry_lower = entry.to_ascii_lowercase();
            if exe_basename == entry_lower || exe_display.contains(&entry_lower) {
                return Self {
                    allowed: true,
                    matched_entry: Some(entry.clone()),
                    raw_entries: Some(entries),
                };
            }
        }

        Self {
            allowed: false,
            matched_entry: None,
            raw_entries: Some(entries),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
#[ssz(enum_behaviour = "union")]
enum HandshakeMessage {
    Handshake(HandshakeData),
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
struct HandshakeData {
    version: Vec<u8>,
    shim: ShimInfo,
    process: ProcessInfo,
    allowlist: AllowlistInfo,
    timestamp: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
struct ShimInfo {
    name: Vec<u8>,
    crate_version: Vec<u8>,
    features: Vec<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
struct ProcessInfo {
    pid: u32,
    ppid: u32,
    uid: u32,
    gid: u32,
    exe_path: Vec<u8>,
    exe_name: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
struct AllowlistInfo {
    matched_entry: Option<Vec<u8>>,
    configured_entries: Option<Vec<Vec<u8>>>,
}

// SSZ encoding/decoding functions for interpose communication
fn encode_ssz(data: &impl Encode) -> Vec<u8> {
    data.as_ssz_bytes()
}

fn decode_ssz<T: Decode>(data: &[u8]) -> Result<T, String> {
    T::from_ssz_bytes(data).map_err(|e| format!("SSZ decode error: {:?}", e))
}

#[cfg(target_os = "macos")]
fn attempt_handshake(
    socket_path: &Path,
    exe: &Path,
    allow: &AllowDecision,
) -> Result<UnixStream, String> {
    use std::os::unix::net::UnixStream as StdUnixStream;

    let socket_display = socket_path.display();
    let mut stream = StdUnixStream::connect(socket_path).map_err(|err| {
        format!(
            "failed to connect to AgentFS control socket '{}': {}",
            socket_display, err
        )
    })?;

    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|err| format!("failed to set read timeout: {err}"))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|err| format!("failed to set write timeout: {err}"))?;

    let pid = std::process::id();
    let ppid = unsafe { libc::getppid() as u32 };
    let uid = unsafe { libc::geteuid() as u32 };
    let gid = unsafe { libc::getegid() as u32 };
    let exe_name = exe.file_name().and_then(OsStr::to_str).unwrap_or("<unknown>");
    let exe_path_owned = exe.to_string_lossy().into_owned();
    let exe_name_owned = exe_name.to_string();

    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or_default();

    let message = HandshakeMessage::Handshake(HandshakeData {
        version: b"1".to_vec(),
        shim: ShimInfo {
            name: b"agentfs-interpose-shim".to_vec(),
            crate_version: env!("CARGO_PKG_VERSION").as_bytes().to_vec(),
            features: vec![b"handshake".to_vec(), b"allowlist".to_vec()],
        },
        process: ProcessInfo {
            pid,
            ppid,
            uid,
            gid,
            exe_path: exe_path_owned.into_bytes(),
            exe_name: exe_name_owned.into_bytes(),
        },
        allowlist: AllowlistInfo {
            matched_entry: allow.matched_entry.as_ref().map(|s| s.clone().into_bytes()),
            configured_entries: allow
                .raw_entries
                .as_ref()
                .map(|entries| entries.iter().map(|s| s.clone().into_bytes()).collect()),
        },
        timestamp: format!("{timestamp}").into_bytes(),
    });

    let ssz_bytes = encode_ssz(&message);
    let ssz_len = ssz_bytes.len() as u32;

    stream
        .write_all(&ssz_len.to_le_bytes())
        .and_then(|_| stream.write_all(&ssz_bytes))
        .map_err(|err| format!("failed to send handshake: {err}"))?;

    // Read acknowledgement
    let mut response_buf = [0u8; 1024];
    match stream.read(&mut response_buf) {
        Ok(0) => {
            return Err(format!(
                "control socket closed without acknowledgement from {}",
                socket_display
            ));
        }
        Ok(n) => {
            let response = String::from_utf8_lossy(&response_buf[..n]);
            log_message(&format!(
                "handshake acknowledged by {socket_display}: {response}"
            ));
        }
        Err(err) => {
            return Err(format!("failed to read handshake acknowledgement: {err}"));
        }
    }

    log_message(&format!(
        "handshake completed with AgentFS control socket: {}",
        socket_display
    ));

    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist_allows_when_not_set() {
        let _lock = ENV_GUARD.lock().unwrap();
        remove_env_var(ENV_ALLOWLIST);
        let exe = Path::new("/Applications/MyApp.app/Contents/MacOS/MyApp");
        let decision = AllowDecision::from_env(exe);
        assert!(decision.allowed);
        assert!(decision.matched_entry.is_none());
    }

    #[test]
    fn allowlist_matches_basename() {
        let _lock = ENV_GUARD.lock().unwrap();
        set_env_var(ENV_ALLOWLIST, "MyApp,OtherApp");
        let exe = Path::new("/Applications/MyApp.app/Contents/MacOS/MyApp");
        let decision = AllowDecision::from_env(exe);
        assert!(decision.allowed);
        assert_eq!(decision.matched_entry.as_deref(), Some("MyApp"));
    }

    #[test]
    fn allowlist_rejects_non_match() {
        let _lock = ENV_GUARD.lock().unwrap();
        set_env_var(ENV_ALLOWLIST, "OtherApp");
        let exe = Path::new("/Applications/MyApp.app/Contents/MacOS/MyApp");
        let decision = AllowDecision::from_env(exe);
        assert!(!decision.allowed);
        assert!(decision.matched_entry.is_none());
    }
}

// Interposition implementation for file operations
#[cfg(target_os = "macos")]
mod interpose {
    use super::*;
    use libc::{
        CMSG_DATA, CMSG_FIRSTHDR, CMSG_LEN, CMSG_SPACE, c_char, c_int, cmsghdr, iovec, mode_t,
        msghdr,
    };
    use std::io::Read;
    use std::mem;
    use std::os::unix::io::FromRawFd;

    /// Send dir_open request and receive directory handle
    fn send_dir_open_request(path: &CStr) -> Result<u64, String> {
        let stream_arc = {
            let stream_guard = STREAM.lock().unwrap();
            log_message(&format!("STREAM.lock() returned: {:?}", stream_guard.as_ref().map(|_| "Some(arc)")));
            match stream_guard.as_ref() {
                Some(arc) => {
                    log_message("STREAM found, sending dir_open request");
                    Arc::clone(arc)
                },
                None => {
                    let fail_fast = std::env::var_os(ENV_FAIL_FAST).map(|s| s == "1").unwrap_or(false);
                    if fail_fast {
                        log_message(&format!("STREAM not set but AGENTFS_INTERPOSE_FAIL_FAST=1; terminating program"));
                        std::process::exit(1);
                    } else {
                        log_message("STREAM not set, falling back to original opendir");
                        return Err("not connected to AgentFS control socket".to_string());
                    }
                }
            }
        };

        let path_str = path.to_string_lossy().into_owned();
        let request = Request::dir_open(path_str);

        let ssz_bytes = encode_ssz(&request);
        let ssz_len = ssz_bytes.len() as u32;

        {
            let mut stream_guard = stream_arc.lock().unwrap();
            stream_guard.write_all(&ssz_len.to_le_bytes()).and_then(|_| stream_guard.write_all(&ssz_bytes))
                .map_err(|e| format!("send dir_open request: {e}"))?;
        }

        // Read response
        let mut len_buf = [0u8; 4];
        let mut msg_buf: Vec<u8>;
        {
            let mut stream_guard = stream_arc.lock().unwrap();
            stream_guard.read_exact(&mut len_buf).map_err(|e| format!("read response length: {e}"))?;
            let msg_len = u32::from_le_bytes(len_buf) as usize;
            msg_buf = vec![0u8; msg_len];
            stream_guard.read_exact(&mut msg_buf).map_err(|e| format!("read response: {e}"))?;
        }

        // Decode the response
        let dir_handle = match decode_ssz::<Response>(&msg_buf) {
            Ok(response) => {
                match response {
                    Response::DirOpen(dir_response) => {
                        let handle = dir_response.handle;
                        log_message(&format!("received dir handle {}", handle));
                        handle
                    }
                    Response::Error(err) => {
                        return Err(format!("daemon error: {}", String::from_utf8_lossy(&err.error)));
                    }
                    _ => {
                        return Err("unexpected response type".to_string());
                    }
                }
            }
            Err(e) => {
                return Err(format!("decode response failed: {:?}", e));
            }
        };

        Ok(dir_handle)
    }

    /// Send dir_read request and receive directory entries
    fn send_dir_read_request(handle: u64) -> Result<Vec<DirEntry>, String> {
        let stream_arc = {
            let stream_guard = STREAM.lock().unwrap();
            match stream_guard.as_ref() {
                Some(arc) => Arc::clone(arc),
                None => {
                    let fail_fast = std::env::var_os(ENV_FAIL_FAST).map(|s| s == "1").unwrap_or(false);
                    if fail_fast {
                        log_message(&format!("STREAM not set but AGENTFS_INTERPOSE_FAIL_FAST=1; terminating program"));
                        std::process::exit(1);
                    } else {
                        log_message("STREAM not set, falling back to original readdir");
                        return Err("not connected to AgentFS control socket".to_string());
                    }
                }
            }
        };

        let request = Request::dir_read(handle);
        let ssz_bytes = encode_ssz(&request);
        let ssz_len = ssz_bytes.len() as u32;

        {
            let mut stream_guard = stream_arc.lock().unwrap();
            stream_guard.write_all(&ssz_len.to_le_bytes()).and_then(|_| stream_guard.write_all(&ssz_bytes))
                .map_err(|e| format!("send dir_read request: {e}"))?;
        }

        // Read response
        let mut len_buf = [0u8; 4];
        let mut msg_buf: Vec<u8>;
        {
            let mut stream_guard = stream_arc.lock().unwrap();
            stream_guard.read_exact(&mut len_buf).map_err(|e| format!("read dir_read response length: {e}"))?;
            let msg_len = u32::from_le_bytes(len_buf) as usize;
            msg_buf = vec![0u8; msg_len];
            stream_guard.read_exact(&mut msg_buf).map_err(|e| format!("read dir_read response: {e}"))?;
        }

        // Decode the response
        let entries = match decode_ssz::<Response>(&msg_buf) {
            Ok(response) => {
                match response {
                    Response::DirRead(dir_read_response) => {
                        log_message(&format!("received {} directory entries", dir_read_response.entries.len()));
                        dir_read_response.entries
                    }
                    Response::Error(err) => {
                        return Err(format!("daemon error: {}", String::from_utf8_lossy(&err.error)));
                    }
                    _ => {
                        return Err("unexpected response type".to_string());
                    }
                }
            }
            Err(e) => {
                return Err(format!("decode response failed: {:?}", e));
            }
        };

        Ok(entries)
    }

    /// Send dir_close request
    fn send_dir_close_request(handle: u64) -> Result<(), String> {
        let stream_arc = {
            let stream_guard = STREAM.lock().unwrap();
            match stream_guard.as_ref() {
                Some(arc) => Arc::clone(arc),
                None => {
                    let fail_fast = std::env::var_os(ENV_FAIL_FAST).map(|s| s == "1").unwrap_or(false);
                    if fail_fast {
                        log_message(&format!("STREAM not set but AGENTFS_INTERPOSE_FAIL_FAST=1; terminating program"));
                        std::process::exit(1);
                    } else {
                        log_message("STREAM not set, falling back to original closedir");
                        return Err("not connected to AgentFS control socket".to_string());
                    }
                }
            }
        };

        let request = Request::dir_close(handle);
        let ssz_bytes = encode_ssz(&request);
        let ssz_len = ssz_bytes.len() as u32;

        {
            let mut stream_guard = stream_arc.lock().unwrap();
            stream_guard.write_all(&ssz_len.to_le_bytes()).and_then(|_| stream_guard.write_all(&ssz_bytes))
                .map_err(|e| format!("send dir_close request: {e}"))?;
        }

        // Read response
        let mut len_buf = [0u8; 4];
        let mut msg_buf: Vec<u8>;
        {
            let mut stream_guard = stream_arc.lock().unwrap();
            stream_guard.read_exact(&mut len_buf).map_err(|e| format!("read dir_close response length: {e}"))?;
            let msg_len = u32::from_le_bytes(len_buf) as usize;
            msg_buf = vec![0u8; msg_len];
            stream_guard.read_exact(&mut msg_buf).map_err(|e| format!("read dir_close response: {e}"))?;
        }

        // Decode the response
        match decode_ssz::<Response>(&msg_buf) {
            Ok(response) => {
                match response {
                    Response::DirClose(_) => {
                        log_message("received dir_close response");
                        Ok(())
                    }
                    Response::Error(err) => {
                        Err(format!("daemon error: {}", String::from_utf8_lossy(&err.error)))
                    }
                    _ => {
                        Err("unexpected response type".to_string())
                    }
                }
            }
            Err(e) => {
                Err(format!("decode response failed: {:?}", e))
            }
        }
    }

    /// Send readlink request and receive link target
    fn send_readlink_request(path: &CStr) -> Result<String, String> {
        let stream_arc = {
            let stream_guard = STREAM.lock().unwrap();
            log_message(&format!("STREAM.lock() returned: {:?}", stream_guard.as_ref().map(|_| "Some(arc)")));
            match stream_guard.as_ref() {
                Some(arc) => {
                    log_message("STREAM found, sending readlink request");
                    Arc::clone(arc)
                },
                None => {
                    let fail_fast = std::env::var_os(ENV_FAIL_FAST).map(|s| s == "1").unwrap_or(false);
                    if fail_fast {
                        log_message(&format!("STREAM not set but AGENTFS_INTERPOSE_FAIL_FAST=1; terminating program"));
                        std::process::exit(1);
                    } else {
                        log_message("STREAM not set, falling back to original readlink");
                        return Err("not connected to AgentFS control socket".to_string());
                    }
                }
            }
        };

        let path_str = path.to_string_lossy().into_owned();
        let request = Request::readlink(path_str);

        let ssz_bytes = encode_ssz(&request);
        let ssz_len = ssz_bytes.len() as u32;

        {
            let mut stream_guard = stream_arc.lock().unwrap();
            stream_guard.write_all(&ssz_len.to_le_bytes()).and_then(|_| stream_guard.write_all(&ssz_bytes))
                .map_err(|e| format!("send readlink request: {e}"))?;
        }

        // Read response
        let mut len_buf = [0u8; 4];
        let mut msg_buf: Vec<u8>;
        {
            let mut stream_guard = stream_arc.lock().unwrap();
            stream_guard.read_exact(&mut len_buf).map_err(|e| format!("read response length: {e}"))?;
            let msg_len = u32::from_le_bytes(len_buf) as usize;
            msg_buf = vec![0u8; msg_len];
            stream_guard.read_exact(&mut msg_buf).map_err(|e| format!("read response: {e}"))?;
        }

        // Decode the response
        let link_target = match decode_ssz::<Response>(&msg_buf) {
            Ok(response) => {
                match response {
                    Response::Readlink(readlink_response) => {
                        let target = String::from_utf8_lossy(&readlink_response.target).into_owned();
                        log_message(&format!("received link target '{}'", target));
                        target
                    }
                    Response::Error(err) => {
                        return Err(format!("daemon error: {}", String::from_utf8_lossy(&err.error)));
                    }
                    _ => {
                        return Err("unexpected response type".to_string());
                    }
                }
            }
            Err(e) => {
                return Err(format!("decode response failed: {:?}", e));
            }
        };

        Ok(link_target)
    }

    /// Send fd_open request and receive file descriptor via SCM_RIGHTS
    fn send_fd_open_request(path: &CStr, flags: c_int, mode: mode_t) -> Result<RawFd, String> {
        let stream_arc = {
            let stream_guard = STREAM.lock().unwrap();
            log_message(&format!(
                "STREAM.lock() returned: {:?}",
                stream_guard.as_ref().map(|_| "Some(arc)")
            ));
            match stream_guard.as_ref() {
                Some(arc) => {
                    log_message("STREAM found, sending fd_open request");
                    Arc::clone(arc)
                }
                None => {
                    let fail_fast =
                        std::env::var_os(ENV_FAIL_FAST).map(|s| s == "1").unwrap_or(false);
                    if fail_fast {
                        log_message(&format!(
                            "STREAM not set but AGENTFS_INTERPOSE_FAIL_FAST=1; terminating program"
                        ));
                        std::process::exit(1);
                    } else {
                        log_message("STREAM not set, falling back to original open");
                        return Err("not connected to AgentFS control socket".to_string());
                    }
                }
            }
        };

        let path_str = path.to_string_lossy().into_owned();
        let request = Request::fd_open(path_str, flags as u32, mode as u32);

        let ssz_bytes = encode_ssz(&request);
        let ssz_len = ssz_bytes.len() as u32;

        {
            let mut stream_guard = stream_arc.lock().unwrap();
            stream_guard
                .write_all(&ssz_len.to_le_bytes())
                .and_then(|_| stream_guard.write_all(&ssz_bytes))
                .map_err(|e| format!("send fd_open request: {e}"))?;
        }

        // Read response (for now, simple response with fd number)
        // TODO: Implement proper SCM_RIGHTS
        let mut len_buf = [0u8; 4];
        let mut msg_buf: Vec<u8>;
        {
            let mut stream_guard = stream_arc.lock().unwrap();
            stream_guard
                .read_exact(&mut len_buf)
                .map_err(|e| format!("read response length: {e}"))?;
            let msg_len = u32::from_le_bytes(len_buf) as usize;
            msg_buf = vec![0u8; msg_len];
            stream_guard
                .read_exact(&mut msg_buf)
                .map_err(|e| format!("read response: {e}"))?;
        }

        // Decode the response
        let fd = match decode_ssz::<Response>(&msg_buf) {
            Ok(response) => match response {
                Response::FdOpen(fd_response) => {
                    let fd = fd_response.fd as RawFd;
                    log_message(&format!("received fd {} from daemon", fd));
                    fd
                }
                Response::Error(err) => {
                    return Err(format!(
                        "daemon error: {}",
                        String::from_utf8_lossy(&err.error)
                    ));
                }
                _ => {
                    return Err("unexpected response type".to_string());
                }
            },
            Err(e) => {
                return Err(format!("decode response failed: {:?}", e));
            }
        };

        if fd < 0 {
            return Err("invalid file descriptor received".to_string());
        }

        // Duplicate the file descriptor to avoid issues with the received fd
        let dup_fd = unsafe { libc::dup(fd) };
        if dup_fd < 0 {
            return Err(format!("dup failed: {}", std::io::Error::last_os_error()));
        }

        Ok(dup_fd as RawFd)
    }

    /// Send file descriptor via SCM_RIGHTS (for testing/debugging)
    fn send_fd_response(stream: &UnixStream, fd: RawFd) -> Result<(), String> {
        let response = Response::fd_open(fd as u32);

        let ssz_bytes = encode_ssz(&response);
        let ssz_len = ssz_bytes.len() as u32;

        // Send response with file descriptor via SCM_RIGHTS
        let mut iov = iovec {
            iov_base: ssz_len.to_le_bytes().as_ptr() as *mut libc::c_void,
            iov_len: 4,
        };

        let mut msg: msghdr = unsafe { mem::zeroed() };
        msg.msg_iov = &mut iov;
        msg.msg_iovlen = 1;

        let cmsg_space = unsafe { CMSG_SPACE(mem::size_of::<RawFd>() as libc::c_uint) } as usize;
        let mut cmsg_buf = vec![0u8; cmsg_space];
        msg.msg_control = cmsg_buf.as_mut_ptr() as *mut libc::c_void;
        msg.msg_controllen = cmsg_space as libc::c_uint;

        let cmsg = unsafe { CMSG_FIRSTHDR(&msg) };
        if cmsg.is_null() {
            return Err("failed to get control message header".to_string());
        }

        unsafe {
            (*cmsg).cmsg_len = CMSG_LEN(mem::size_of::<RawFd>() as libc::c_uint);
            (*cmsg).cmsg_level = libc::SOL_SOCKET;
            (*cmsg).cmsg_type = libc::SCM_RIGHTS;
            *(CMSG_DATA(cmsg) as *mut RawFd) = fd;
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

    /// Interposed open function
    redhook::hook! {
        unsafe fn open(path: *const c_char, flags: c_int, mode: mode_t) -> c_int => my_open {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };

            log_message(&format!("interposing open({}, {:#x}, {:#o})", c_path.to_string_lossy(), flags, mode));

            match send_fd_open_request(c_path, flags, mode) {
                Ok(fd) => {
                    log_message(&format!("fd_open succeeded, returning fd {}", fd));
                    fd as c_int
                }
                Err(err) => {
                    log_message(&format!("fd_open failed: {}, falling back to original", err));
                    // Fall back to original function
                    redhook::real!(open)(path, flags, mode)
                }
            }
        }
    }

    /// Interposed openat function
    redhook::hook! {
        unsafe fn openat(dirfd: c_int, path: *const c_char, flags: c_int, mode: mode_t) -> c_int => my_openat {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };

            log_message(&format!("interposing openat({}, {}, {:#x}, {:#o})", dirfd, c_path.to_string_lossy(), flags, mode));

            // For now, fall back to original - openat forwarding needs more complex path resolution
            redhook::real!(openat)(dirfd, path, flags, mode)
        }
    }

    /// Interposed creat function
    redhook::hook! {
        unsafe fn creat(path: *const c_char, mode: mode_t) -> c_int => my_creat {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            let flags = libc::O_CREAT | libc::O_TRUNC | libc::O_WRONLY;

            log_message(&format!("interposing creat({}, {:#o})", c_path.to_string_lossy(), mode));

            match send_fd_open_request(c_path, flags, mode) {
                Ok(fd) => {
                    log_message(&format!("fd_open succeeded, returning fd {}", fd));
                    fd as c_int
                }
                Err(err) => {
                    log_message(&format!("fd_open failed: {}, falling back to original", err));
                    redhook::real!(creat)(path, mode)
                }
            }
        }
    }

    /// Interposed fopen function
    redhook::hook! {
        unsafe fn fopen(filename: *const c_char, mode: *const c_char) -> *mut libc::FILE => my_fopen {
            log_message("interposing fopen() - not yet implemented, falling back to original");

            // For now, fall back to original
            redhook::real!(fopen)(filename, mode)
        }
    }

    /// Interposed freopen function
    redhook::hook! {
        unsafe fn freopen(filename: *const c_char, mode: *const c_char, stream: *mut libc::FILE) -> *mut libc::FILE => my_freopen {
            log_message("interposing freopen() - not yet implemented, falling back to original");

            // For now, fall back to original
            redhook::real!(freopen)(filename, mode, stream)
        }
    }

    /// Interposed opendir function
    redhook::hook! {
        unsafe fn opendir(dirname: *const c_char) -> *mut libc::DIR => my_opendir {
            if dirname.is_null() {
                return std::ptr::null_mut();
            }

            let c_path = unsafe { CStr::from_ptr(dirname) };
            log_message(&format!("interposing opendir({})", c_path.to_string_lossy()));

            // Try to create directory handle with AgentFS entries
            match send_dir_open_request(c_path) {
                Ok(fscore_handle) => {
                    log_message(&format!("FsCore returned directory handle {}", fscore_handle));
                    // Return the FsCore handle directly as a DIR pointer
                    // This is safe because we're intercepting all directory operations
                    let dir_ptr = fscore_handle as *mut libc::DIR;
                    log_message(&format!("returning FsCore DIR pointer: {:?}", dir_ptr));
                    dir_ptr
                }
                Err(err) => {
                    log_message(&format!("failed to create AgentFS directory handle: {}, falling back to original", err));
                    // Fall back to original function
                    redhook::real!(opendir)(dirname)
                }
            }
        }
    }

    /// Interposed fdopendir function
    redhook::hook! {
        unsafe fn fdopendir(fd: c_int) -> *mut libc::DIR => my_fdopendir {
            log_message(&format!("interposing fdopendir({}) - not yet implemented, falling back to original", fd));

            // For now, fall back to original
            redhook::real!(fdopendir)(fd)
        }
    }

    /// Interposed readdir function
    redhook::hook! {
        unsafe fn readdir(dirp: *mut libc::DIR) -> *mut libc::dirent => my_readdir {
            log_message(&format!("interposing readdir({:?})", dirp));

            // The DIR pointer contains the FsCore handle ID
            let fscore_handle = dirp as u64;

            // Send DirRead request to get the next entry
            match send_dir_read_request(fscore_handle) {
                Ok(entries) => {
                    if entries.is_empty() {
                        log_message("readdir: reached end of directory");
                        return std::ptr::null_mut(); // End of directory
                    }

                    let entry = &entries[0];
                    log_message(&format!("readdir: returning entry {}", String::from_utf8_lossy(&entry.name)));

                    // For now, return null since we don't have proper dirent allocation
                    // In a full implementation, we'd allocate a libc::dirent and fill it
                    std::ptr::null_mut()
                }
                Err(err) => {
                    log_message(&format!("dir_read failed: {}, falling back to original", err));
                    // Fall back to original function
                    redhook::real!(readdir)(dirp)
                }
            }
        }
    }

    /// Interposed closedir function
    redhook::hook! {
        unsafe fn closedir(dirp: *mut libc::DIR) -> c_int => my_closedir {
            log_message(&format!("interposing closedir({:?})", dirp));

            // The DIR pointer contains the FsCore handle ID
            let fscore_handle = dirp as u64;

            // Send DirClose request to FsCore
            match send_dir_close_request(fscore_handle) {
                Ok(()) => {
                    log_message(&format!("FsCore closedir succeeded for handle {}", fscore_handle));
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("dir_close failed: {}, falling back to original", err));
                    // Fall back to original function
                    redhook::real!(closedir)(dirp)
                }
            }
        }
    }

    /// Interposed readlink function
    redhook::hook! {
        unsafe fn readlink(pathname: *const c_char, buf: *mut c_char, bufsiz: libc::size_t) -> libc::ssize_t => my_readlink {
            if pathname.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(pathname) };
            log_message(&format!("interposing readlink({}, bufsiz={})", c_path.to_string_lossy(), bufsiz));

            match send_readlink_request(c_path) {
                Ok(link_target) => {
                    // Copy the link target to the provided buffer
                    let target_bytes = link_target.as_bytes();
                    let copy_len = std::cmp::min(target_bytes.len(), bufsiz);
                    if !buf.is_null() && copy_len > 0 {
                        unsafe {
                            std::ptr::copy_nonoverlapping(target_bytes.as_ptr() as *const c_char, buf, copy_len);
                        }
                    }
                    log_message(&format!("readlink succeeded, returning {} bytes", copy_len));
                    copy_len as libc::ssize_t
                }
                Err(err) => {
                    log_message(&format!("readlink failed: {}, falling back to original", err));
                    // Fall back to original function
                    redhook::real!(readlink)(pathname, buf, bufsiz)
                }
            }
        }
    }

    /// Interposed readlinkat function
    redhook::hook! {
        unsafe fn readlinkat(dirfd: c_int, pathname: *const c_char, buf: *mut c_char, bufsiz: libc::size_t) -> libc::ssize_t => my_readlinkat {
            if pathname.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(pathname) };
            log_message(&format!("interposing readlinkat({}, {}, bufsiz={}) - not yet implemented, falling back to original", dirfd, c_path.to_string_lossy(), bufsiz));

            // For now, fall back to original
            redhook::real!(readlinkat)(dirfd, pathname, buf, bufsiz)
        }
    }

    // Note: _INODE64 variants are not implemented as they require symbol names with '$'
    // which is not valid in Rust function names. The base functions handle the common case.
}

