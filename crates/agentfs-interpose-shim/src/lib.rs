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
use agentfs_proto::messages::{
    AclDeleteDefFileRequest, AclGetFdRequest, AclGetFileRequest, AclSetFdRequest,
    AclSetFileRequest, ChflagsRequest, ClonefileRequest, CopyfileRequest, DirEntry,
    FchflagsRequest, FclonefileatRequest, FcopyfileRequest, FgetxattrRequest, FlistxattrRequest,
    FremovexattrRequest, FsetxattrRequest, GetattrlistRequest, GetattrlistbulkRequest,
    GetxattrRequest, LchflagsRequest, LgetxattrRequest, ListxattrRequest, LlistxattrRequest,
    LremovexattrRequest, LsetxattrRequest, RemovexattrRequest, SetattrlistRequest, SetxattrRequest,
    StatData, StatfsData, TimespecData,
};
use agentfs_proto::*;

// Error codes for interpose forwarding failures
const FORWARDING_UNAVAILABLE: u32 = 1;

#[cfg(target_os = "macos")]
use std::os::unix::net::UnixStream;

const LOG_PREFIX: &str = "[agentfs-interpose]";
const ENV_ENABLED: &str = "AGENTFS_INTERPOSE_ENABLED";
const ENV_SOCKET: &str = "AGENTFS_INTERPOSE_SOCKET";
const ENV_ALLOWLIST: &str = "AGENTFS_INTERPOSE_ALLOWLIST";
const ENV_LOG_LEVEL: &str = "AGENTFS_INTERPOSE_LOG";
const ENV_FAIL_FAST: &str = "AGENTFS_INTERPOSE_FAIL_FAST";
const DEFAULT_BANNER: &str = "AgentFS interpose shim loaded";

/// Per-process directory file descriptor mapping
#[derive(Clone, Debug)]
struct DirfdMapping {
    /// Current working directory for AT_FDCWD resolution
    cwd: PathBuf,
    /// File descriptor to path mappings
    fd_paths: HashMap<RawFd, PathBuf>,
}

impl DirfdMapping {
    fn new() -> Self {
        Self {
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")),
            fd_paths: HashMap::new(),
        }
    }

    /// Get the path for a directory file descriptor
    fn get_path(&self, dirfd: RawFd) -> Option<&PathBuf> {
        self.fd_paths.get(&dirfd)
    }

    /// Set the path for a directory file descriptor
    fn set_path(&mut self, dirfd: RawFd, path: PathBuf) {
        self.fd_paths.insert(dirfd, path);
    }

    /// Remove a directory file descriptor mapping
    fn remove_path(&mut self, dirfd: RawFd) {
        self.fd_paths.remove(&dirfd);
    }

    /// Update current working directory
    fn set_cwd(&mut self, cwd: PathBuf) {
        self.cwd = cwd;
    }

    /// Duplicate file descriptor mapping
    fn dup_fd(&mut self, old_fd: RawFd, new_fd: RawFd) {
        if let Some(path) = self.fd_paths.get(&old_fd).cloned() {
            self.fd_paths.insert(new_fd, path);
        }
    }
}

/// Execute a closure with access to the current process's dirfd mapping
fn with_dirfd_mapping<F, R>(f: F) -> R
where
    F: FnOnce(&mut DirfdMapping) -> R,
{
    let pid = std::process::id();
    let mut global_map = DIRFD_MAPPING.lock().unwrap();

    // Initialize mapping for this process if it doesn't exist
    if !global_map.contains_key(&pid) {
        global_map.insert(pid, DirfdMapping::new());
    }

    // Get mutable reference to the mapping and execute the closure
    let mapping = global_map.get_mut(&pid).unwrap();
    f(mapping)
}

/// Resolve a directory file descriptor to its path
fn resolve_dirfd(dirfd: RawFd) -> Option<PathBuf> {
    with_dirfd_mapping(|mapping| match dirfd {
        libc::AT_FDCWD => Some(mapping.cwd.clone()),
        fd if fd >= 0 => mapping.get_path(fd).cloned(),
        _ => None,
    })
}

/// Resolve a path by combining dirfd + relative path with symlink and .. handling
fn resolve_path_with_dirfd(dirfd: RawFd, path: &CStr) -> Option<PathBuf> {
    let base_path = resolve_dirfd(dirfd)?;
    let relative_path = Path::new(path.to_str().ok()?);

    // Combine base path with relative path
    let mut resolved_path = base_path.clone();
    resolved_path.push(relative_path);

    // Canonicalize the path to resolve . and .. components
    // Note: This follows symlinks, which is the expected behavior for most *at functions
    match resolved_path.canonicalize() {
        Ok(canonical) => Some(canonical),
        Err(_) => {
            // If canonicalization fails (e.g., path doesn't exist), return the non-canonicalized path
            // This allows operations on non-existent files to work correctly
            Some(resolved_path)
        }
    }
}

/// Helper function to resolve path for *at operations
fn resolve_at_path(dirfd: RawFd, path: &CStr) -> PathBuf {
    resolve_path_with_dirfd(dirfd, path).unwrap_or_else(|| {
        // Fallback: construct path manually if resolution fails
        resolve_dirfd(dirfd)
            .unwrap_or_else(|| PathBuf::from("."))
            .join(path.to_str().unwrap_or(""))
    })
}

static INIT_GUARD: OnceCell<()> = OnceCell::new();
#[cfg(target_os = "macos")]
static STREAM: Mutex<Option<Arc<Mutex<UnixStream>>>> = Mutex::new(None);

// Global directory file descriptor mapping keyed by process ID
static DIRFD_MAPPING: std::sync::LazyLock<Mutex<HashMap<u32, DirfdMapping>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

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
        CMSG_DATA, CMSG_FIRSTHDR, CMSG_LEN, CMSG_SPACE, c_char, c_int, c_void, cmsghdr, gid_t,
        iovec, mode_t, msghdr, off_t, size_t, ssize_t, timespec, uid_t,
    };

    // ACL types - these may need to be defined manually for macOS
    type acl_type_t = u32;
    type acl_t = *mut c_void;

    // attrlist types for getattrlist operations
    #[repr(C)]
    struct attrlist {
        bitmapcount: u16,
        reserved: u16,
        commonattr: u32,
        volattr: u32,
        dirattr: u32,
        fileattr: u32,
        forkattr: u32,
    }

    // copyfile types for copyfile operations
    type copyfile_state_t = *mut c_void;
    type copyfile_flags_t = u32;

    // Additional type aliases for macOS
    type u_long = usize;
    type u_int64_t = u64;
    use std::io::Read;
    use std::mem;
    use std::os::unix::io::FromRawFd;

    /// Generic function to send a request and receive a response
    fn send_request<F, T>(request: Request, extract_response: F) -> Result<T, String>
    where
        F: FnOnce(Response) -> Option<T>,
    {
        let stream_arc = {
            let stream_guard = STREAM.lock().unwrap();
            match stream_guard.as_ref() {
                Some(arc) => Arc::clone(arc),
                None => {
                    let fail_fast =
                        std::env::var_os(ENV_FAIL_FAST).map(|s| s == "1").unwrap_or(false);
                    if fail_fast {
                        log_message(&format!(
                            "STREAM not set but AGENTFS_INTERPOSE_FAIL_FAST=1; terminating program"
                        ));
                        std::process::exit(1);
                    } else {
                        log_message("STREAM not set, falling back to original function");
                        return Err("not connected to AgentFS control socket".to_string());
                    }
                }
            }
        };

        let ssz_bytes = encode_ssz(&request);
        let ssz_len = ssz_bytes.len() as u32;

        {
            let mut stream_guard = stream_arc.lock().unwrap();
            stream_guard
                .write_all(&ssz_len.to_le_bytes())
                .and_then(|_| stream_guard.write_all(&ssz_bytes))
                .map_err(|e| format!("send request: {e}"))?;
        }

        // Read response
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
        match decode_ssz::<Response>(&msg_buf) {
            Ok(response) => match extract_response(response) {
                Some(result) => Ok(result),
                None => Err("unexpected response type".to_string()),
            },
            Err(e) => Err(format!("decode response failed: {:?}", e)),
        }
    }

    /// Send dir_open request and receive directory handle
    fn send_dir_open_request(path: &CStr) -> Result<u64, String> {
        let stream_arc = {
            let stream_guard = STREAM.lock().unwrap();
            log_message(&format!(
                "STREAM.lock() returned: {:?}",
                stream_guard.as_ref().map(|_| "Some(arc)")
            ));
            match stream_guard.as_ref() {
                Some(arc) => {
                    log_message("STREAM found, sending dir_open request");
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
            stream_guard
                .write_all(&ssz_len.to_le_bytes())
                .and_then(|_| stream_guard.write_all(&ssz_bytes))
                .map_err(|e| format!("send dir_open request: {e}"))?;
        }

        // Read response
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
        let dir_handle = match decode_ssz::<Response>(&msg_buf) {
            Ok(response) => match response {
                Response::DirOpen(dir_response) => {
                    let handle = dir_response.handle;
                    log_message(&format!("received dir handle {}", handle));
                    handle
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

        Ok(dir_handle)
    }

    /// Send dir_read request and receive directory entries
    fn send_dir_read_request(handle: u64) -> Result<Vec<DirEntry>, String> {
        let stream_arc = {
            let stream_guard = STREAM.lock().unwrap();
            match stream_guard.as_ref() {
                Some(arc) => Arc::clone(arc),
                None => {
                    let fail_fast =
                        std::env::var_os(ENV_FAIL_FAST).map(|s| s == "1").unwrap_or(false);
                    if fail_fast {
                        log_message(&format!(
                            "STREAM not set but AGENTFS_INTERPOSE_FAIL_FAST=1; terminating program"
                        ));
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
            stream_guard
                .write_all(&ssz_len.to_le_bytes())
                .and_then(|_| stream_guard.write_all(&ssz_bytes))
                .map_err(|e| format!("send dir_read request: {e}"))?;
        }

        // Read response
        let mut len_buf = [0u8; 4];
        let mut msg_buf: Vec<u8>;
        {
            let mut stream_guard = stream_arc.lock().unwrap();
            stream_guard
                .read_exact(&mut len_buf)
                .map_err(|e| format!("read dir_read response length: {e}"))?;
            let msg_len = u32::from_le_bytes(len_buf) as usize;
            msg_buf = vec![0u8; msg_len];
            stream_guard
                .read_exact(&mut msg_buf)
                .map_err(|e| format!("read dir_read response: {e}"))?;
        }

        // Decode the response
        let entries = match decode_ssz::<Response>(&msg_buf) {
            Ok(response) => match response {
                Response::DirRead(dir_read_response) => {
                    log_message(&format!(
                        "received {} directory entries",
                        dir_read_response.entries.len()
                    ));
                    dir_read_response.entries
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

        Ok(entries)
    }

    /// Send dir_close request
    fn send_dir_close_request(handle: u64) -> Result<(), String> {
        let stream_arc = {
            let stream_guard = STREAM.lock().unwrap();
            match stream_guard.as_ref() {
                Some(arc) => Arc::clone(arc),
                None => {
                    let fail_fast =
                        std::env::var_os(ENV_FAIL_FAST).map(|s| s == "1").unwrap_or(false);
                    if fail_fast {
                        log_message(&format!(
                            "STREAM not set but AGENTFS_INTERPOSE_FAIL_FAST=1; terminating program"
                        ));
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
            stream_guard
                .write_all(&ssz_len.to_le_bytes())
                .and_then(|_| stream_guard.write_all(&ssz_bytes))
                .map_err(|e| format!("send dir_close request: {e}"))?;
        }

        // Read response
        let mut len_buf = [0u8; 4];
        let mut msg_buf: Vec<u8>;
        {
            let mut stream_guard = stream_arc.lock().unwrap();
            stream_guard
                .read_exact(&mut len_buf)
                .map_err(|e| format!("read dir_close response length: {e}"))?;
            let msg_len = u32::from_le_bytes(len_buf) as usize;
            msg_buf = vec![0u8; msg_len];
            stream_guard
                .read_exact(&mut msg_buf)
                .map_err(|e| format!("read dir_close response: {e}"))?;
        }

        // Decode the response
        match decode_ssz::<Response>(&msg_buf) {
            Ok(response) => match response {
                Response::DirClose(_) => {
                    log_message("received dir_close response");
                    Ok(())
                }
                Response::Error(err) => Err(format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                )),
                _ => Err("unexpected response type".to_string()),
            },
            Err(e) => Err(format!("decode response failed: {:?}", e)),
        }
    }

    /// Send readlink request and receive link target
    fn send_readlink_request(path: &CStr) -> Result<String, String> {
        let stream_arc = {
            let stream_guard = STREAM.lock().unwrap();
            log_message(&format!(
                "STREAM.lock() returned: {:?}",
                stream_guard.as_ref().map(|_| "Some(arc)")
            ));
            match stream_guard.as_ref() {
                Some(arc) => {
                    log_message("STREAM found, sending readlink request");
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
            stream_guard
                .write_all(&ssz_len.to_le_bytes())
                .and_then(|_| stream_guard.write_all(&ssz_bytes))
                .map_err(|e| format!("send readlink request: {e}"))?;
        }

        // Read response
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
        let link_target = match decode_ssz::<Response>(&msg_buf) {
            Ok(response) => match response {
                Response::Readlink(readlink_response) => {
                    let target = String::from_utf8_lossy(&readlink_response.target).into_owned();
                    log_message(&format!("received link target '{}'", target));
                    target
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

    /// Interposed open function (fd_open + fd tracking)
    redhook::hook! {
        unsafe fn open(path: *const c_char, flags: c_int, mode: mode_t) -> c_int => my_open {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };

            log_message(&format!("interposing open({}, {:#x}, {:#o})", c_path.to_string_lossy(), flags, mode));

            // First try fd_open request
            match send_fd_open_request(c_path, flags, mode) {
                Ok(fd) => {
                    // Track directory fd if needed
                    if (flags & libc::O_DIRECTORY) != 0 {
                        if let Ok(path_str) = c_path.to_str() {
                            with_dirfd_mapping(|mapping| {
                                let path_buf = PathBuf::from(path_str);
                                let canonical_path = path_buf.canonicalize().unwrap_or(path_buf);
                                mapping.set_path(fd, canonical_path);
                                log_message(&format!("tracked directory fd {} -> {}", fd, path_str));
                            });
                        }
                    }
                    log_message(&format!("fd_open succeeded, returning fd {}", fd));
                    fd as c_int
                }
                Err(err) => {
                    log_message(&format!("fd_open failed: {}, falling back to original", err));
                    // Fall back to original function and track if it's a directory
                    let result = redhook::real!(open)(path, flags, mode);
                    if result >= 0 && (flags & libc::O_DIRECTORY) != 0 {
                        if let Ok(path_str) = c_path.to_str() {
                            with_dirfd_mapping(|mapping| {
                                let path_buf = PathBuf::from(path_str);
                                let canonical_path = path_buf.canonicalize().unwrap_or(path_buf);
                                mapping.set_path(result, canonical_path);
                                log_message(&format!("tracked directory fd {} -> {}", result, path_str));
                            });
                        }
                    }
                    result
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

    /// Send stat request and receive file status
    fn send_stat_request(path: &CStr) -> Result<StatData, String> {
        let path_str = path.to_string_lossy().into_owned();
        let request = Request::stat(path_str);

        send_request(request, |response| match response {
            Response::Stat(stat_response) => {
                log_message("received stat response");
                Some(stat_response.stat)
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send lstat request and receive file status (without following symlinks)
    fn send_lstat_request(path: &CStr) -> Result<StatData, String> {
        let path_str = path.to_string_lossy().into_owned();
        let request = Request::lstat(path_str);

        send_request(request, |response| match response {
            Response::Lstat(lstat_response) => {
                log_message("received lstat response");
                Some(lstat_response.stat)
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send fstat request and receive file status for open file descriptor
    fn send_fstat_request(fd: c_int) -> Result<StatData, String> {
        let request = Request::fstat(fd as u32);

        send_request(request, |response| match response {
            Response::Fstat(fstat_response) => {
                log_message(&format!("received fstat response for fd {}", fd));
                Some(fstat_response.stat)
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send fstatat request and receive file status relative to directory fd
    fn send_fstatat_request(dirfd: c_int, path: &CStr, flags: c_int) -> Result<StatData, String> {
        let resolved_path = resolve_at_path(dirfd, path);
        let path_str = resolved_path.to_string_lossy().into_owned();
        let request = Request::fstatat(path_str.clone(), flags as u32);

        send_request(request, |response| match response {
            Response::Fstatat(fstatat_response) => {
                log_message(&format!(
                    "received fstatat response for resolved path {}",
                    path_str
                ));
                Some(fstatat_response.stat)
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send chown request to change file owner and group
    fn send_chown_request(path: &CStr, uid: uid_t, gid: gid_t) -> Result<(), String> {
        let path_str = path.to_string_lossy().into_owned();
        let request = Request::chown(path_str, uid, gid);

        send_request(request, |response| match response {
            Response::Chown(_) => {
                log_message("received chown response");
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send lchown request to change file owner and group (without following symlinks)
    fn send_lchown_request(path: &CStr, uid: uid_t, gid: gid_t) -> Result<(), String> {
        let path_str = path.to_string_lossy().into_owned();
        let request = Request::lchown(path_str, uid, gid);

        send_request(request, |response| match response {
            Response::Lchown(_) => {
                log_message("received lchown response");
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send fchown request to change file owner and group by file descriptor
    fn send_fchown_request(fd: c_int, uid: uid_t, gid: gid_t) -> Result<(), String> {
        let request = Request::fchown(fd as u32, uid, gid);

        send_request(request, |response| match response {
            Response::Fchown(_) => {
                log_message(&format!("received fchown response for fd {}", fd));
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send fchownat request to change file owner and group relative to directory fd
    fn send_fchownat_request(
        dirfd: c_int,
        path: &CStr,
        uid: uid_t,
        gid: gid_t,
        flags: c_int,
    ) -> Result<(), String> {
        let resolved_path = resolve_at_path(dirfd, path);
        let path_str = resolved_path.to_string_lossy().into_owned();
        let request = Request::fchownat(path_str.clone(), uid, gid, flags as u32);

        send_request(request, |response| match response {
            Response::Fchownat(_) => {
                log_message(&format!(
                    "received fchownat response for resolved path {}",
                    path_str
                ));
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send chmod request to change file mode
    fn send_chmod_request(path: &CStr, mode: mode_t) -> Result<(), String> {
        let path_str = path.to_string_lossy().into_owned();
        let request = Request::chmod(path_str, mode as u32);

        send_request(request, |response| match response {
            Response::Chmod(_) => {
                log_message("received chmod response");
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send fchmod request to change file mode by file descriptor
    fn send_fchmod_request(fd: c_int, mode: mode_t) -> Result<(), String> {
        let request = Request::fchmod(fd as u32, mode as u32);

        send_request(request, |response| match response {
            Response::Fchmod(_) => {
                log_message(&format!("received fchmod response for fd {}", fd));
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send fchmodat request to change file mode relative to directory fd
    fn send_fchmodat_request(
        dirfd: c_int,
        path: &CStr,
        mode: mode_t,
        flags: c_int,
    ) -> Result<(), String> {
        let resolved_path = resolve_at_path(dirfd, path);
        let path_str = resolved_path.to_string_lossy().into_owned();
        let request = Request::fchmodat(path_str.clone(), mode as u32, flags as u32);

        send_request(request, |response| match response {
            Response::Fchmodat(_) => {
                log_message(&format!(
                    "received fchmodat response for resolved path {}",
                    path_str
                ));
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send statfs request to get filesystem statistics
    fn send_statfs_request(path: &CStr) -> Result<StatfsData, String> {
        let path_str = path.to_string_lossy().into_owned();
        let request = Request::statfs(path_str);

        send_request(request, |response| match response {
            Response::Statfs(statfs_response) => {
                log_message("received statfs response");
                Some(statfs_response.statfs)
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send fstatfs request to get filesystem statistics by file descriptor
    fn send_fstatfs_request(fd: c_int) -> Result<StatfsData, String> {
        let request = Request::fstatfs(fd as u32);

        send_request(request, |response| match response {
            Response::Fstatfs(fstatfs_response) => {
                log_message(&format!("received fstatfs response for fd {}", fd));
                Some(fstatfs_response.statfs)
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send truncate request to change file size
    fn send_truncate_request(path: &CStr, length: off_t) -> Result<(), String> {
        let path_str = path.to_string_lossy().into_owned();
        let request = Request::truncate(path_str, length as u64);

        send_request(request, |response| match response {
            Response::Truncate(_) => {
                log_message("received truncate response");
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send ftruncate request to change file size by file descriptor
    fn send_ftruncate_request(fd: c_int, length: off_t) -> Result<(), String> {
        let request = Request::ftruncate(fd as u32, length as u64);

        send_request(request, |response| match response {
            Response::Ftruncate(_) => {
                log_message(&format!("received ftruncate response for fd {}", fd));
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send utimes request to change file access and modification times
    fn send_utimes_request(path: &CStr, times: Option<&[timespec; 2]>) -> Result<(), String> {
        let path_str = path.to_string_lossy().into_owned();
        let times_data = times.map(|t| {
            (
                TimespecData {
                    tv_sec: t[0].tv_sec as u64,
                    tv_nsec: t[0].tv_nsec as u32,
                },
                TimespecData {
                    tv_sec: t[1].tv_sec as u64,
                    tv_nsec: t[1].tv_nsec as u32,
                },
            )
        });
        let request = Request::utimes(path_str, times_data);

        send_request(request, |response| match response {
            Response::Utimes(_) => {
                log_message("received utimes response");
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send futimes request to change file access and modification times by file descriptor
    fn send_futimes_request(fd: c_int, times: Option<&[timespec; 2]>) -> Result<(), String> {
        let times_data = times.map(|t| {
            (
                TimespecData {
                    tv_sec: t[0].tv_sec as u64,
                    tv_nsec: t[0].tv_nsec as u32,
                },
                TimespecData {
                    tv_sec: t[1].tv_sec as u64,
                    tv_nsec: t[1].tv_nsec as u32,
                },
            )
        });
        let request = Request::futimes(fd as u32, times_data);

        send_request(request, |response| match response {
            Response::Futimes(_) => {
                log_message(&format!("received futimes response for fd {}", fd));
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send utimensat request to change file access and modification times relative to directory fd
    fn send_utimensat_request(
        dirfd: c_int,
        path: &CStr,
        times: Option<&[timespec; 2]>,
        flags: c_int,
    ) -> Result<(), String> {
        let resolved_path = resolve_at_path(dirfd, path);
        let path_str = resolved_path.to_string_lossy().into_owned();
        let times_data = times.map(|t| {
            (
                TimespecData {
                    tv_sec: t[0].tv_sec as u64,
                    tv_nsec: t[0].tv_nsec as u32,
                },
                TimespecData {
                    tv_sec: t[1].tv_sec as u64,
                    tv_nsec: t[1].tv_nsec as u32,
                },
            )
        });
        let request = Request::utimensat(path_str.clone(), times_data, flags as u32);

        send_request(request, |response| match response {
            Response::Utimensat(_) => {
                log_message(&format!(
                    "received utimensat response for resolved path {}",
                    path_str
                ));
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send futimens request to change file access and modification times by file descriptor (nanosecond precision)
    fn send_futimens_request(fd: c_int, times: Option<&[timespec; 2]>) -> Result<(), String> {
        let times_data = times.map(|t| {
            (
                TimespecData {
                    tv_sec: t[0].tv_sec as u64,
                    tv_nsec: t[0].tv_nsec as u32,
                },
                TimespecData {
                    tv_sec: t[1].tv_sec as u64,
                    tv_nsec: t[1].tv_nsec as u32,
                },
            )
        });
        let request = Request::futimens(fd as u32, times_data);

        send_request(request, |response| match response {
            Response::Futimens(_) => {
                log_message(&format!("received futimens response for fd {}", fd));
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Interposed stat function
    redhook::hook! {
        unsafe fn stat(path: *const c_char, buf: *mut libc::stat) -> c_int => my_stat {
            if path.is_null() || buf.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            log_message(&format!("interposing stat({})", c_path.to_string_lossy()));

            match send_stat_request(c_path) {
                Ok(stat_data) => {
                    // Convert StatData to libc::stat
                    let libc_stat = libc::stat {
                        st_dev: stat_data.st_dev as i32,
                        st_ino: stat_data.st_ino,
                        st_mode: stat_data.st_mode as u16,
                        st_nlink: stat_data.st_nlink as u16,
                        st_uid: stat_data.st_uid,
                        st_gid: stat_data.st_gid,
                        st_rdev: stat_data.st_rdev as i32,
                        st_size: stat_data.st_size as i64,
                        st_blksize: stat_data.st_blksize as i32,
                        st_blocks: stat_data.st_blocks as i64,
                        st_atime: stat_data.st_atime as i64,
                        st_atime_nsec: stat_data.st_atime_nsec as i64,
                        st_mtime: stat_data.st_mtime as i64,
                        st_mtime_nsec: stat_data.st_mtime_nsec as i64,
                        st_ctime: stat_data.st_ctime as i64,
                        st_ctime_nsec: stat_data.st_ctime_nsec as i64,
                        #[cfg(target_os = "macos")]
                        st_birthtime: 0, // Not provided by AgentFS yet
                        #[cfg(target_os = "macos")]
                        st_birthtime_nsec: 0,
                        #[cfg(target_os = "macos")]
                        st_flags: 0,
                        #[cfg(target_os = "macos")]
                        st_gen: 0,
                        #[cfg(target_os = "macos")]
                        st_lspare: 0,
                        #[cfg(target_os = "macos")]
                        st_qspare: [0; 2],
                    };

                    unsafe { *buf = libc_stat };
                    log_message("stat succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("stat failed: {}, falling back to original", err));
                    // Fall back to original function
                    redhook::real!(stat)(path, buf)
                }
            }
        }
    }

    /// Interposed lstat function
    redhook::hook! {
        unsafe fn lstat(path: *const c_char, buf: *mut libc::stat) -> c_int => my_lstat {
            if path.is_null() || buf.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            log_message(&format!("interposing lstat({})", c_path.to_string_lossy()));

            match send_lstat_request(c_path) {
                Ok(stat_data) => {
                    // Convert StatData to libc::stat
                    let libc_stat = libc::stat {
                        st_dev: stat_data.st_dev as i32,
                        st_ino: stat_data.st_ino,
                        st_mode: stat_data.st_mode as u16,
                        st_nlink: stat_data.st_nlink as u16,
                        st_uid: stat_data.st_uid,
                        st_gid: stat_data.st_gid,
                        st_rdev: stat_data.st_rdev as i32,
                        st_size: stat_data.st_size as i64,
                        st_blksize: stat_data.st_blksize as i32,
                        st_blocks: stat_data.st_blocks as i64,
                        st_atime: stat_data.st_atime as i64,
                        st_atime_nsec: stat_data.st_atime_nsec as i64,
                        st_mtime: stat_data.st_mtime as i64,
                        st_mtime_nsec: stat_data.st_mtime_nsec as i64,
                        st_ctime: stat_data.st_ctime as i64,
                        st_ctime_nsec: stat_data.st_ctime_nsec as i64,
                        #[cfg(target_os = "macos")]
                        st_birthtime: 0, // Not provided by AgentFS yet
                        #[cfg(target_os = "macos")]
                        st_birthtime_nsec: 0,
                        #[cfg(target_os = "macos")]
                        st_flags: 0,
                        #[cfg(target_os = "macos")]
                        st_gen: 0,
                        #[cfg(target_os = "macos")]
                        st_lspare: 0,
                        #[cfg(target_os = "macos")]
                        st_qspare: [0; 2],
                    };

                    unsafe { *buf = libc_stat };
                    log_message("lstat succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("lstat failed: {}, falling back to original", err));
                    // Fall back to original function
                    redhook::real!(lstat)(path, buf)
                }
            }
        }
    }

    /// Interposed fstat function
    redhook::hook! {
        unsafe fn fstat(fd: c_int, buf: *mut libc::stat) -> c_int => my_fstat {
            if buf.is_null() {
                return -1;
            }

            log_message(&format!("interposing fstat({})", fd));

            match send_fstat_request(fd) {
                Ok(stat_data) => {
                    // Convert StatData to libc::stat
                    let libc_stat = libc::stat {
                        st_dev: stat_data.st_dev as i32,
                        st_ino: stat_data.st_ino,
                        st_mode: stat_data.st_mode as u16,
                        st_nlink: stat_data.st_nlink as u16,
                        st_uid: stat_data.st_uid,
                        st_gid: stat_data.st_gid,
                        st_rdev: stat_data.st_rdev as i32,
                        st_size: stat_data.st_size as i64,
                        st_blksize: stat_data.st_blksize as i32,
                        st_blocks: stat_data.st_blocks as i64,
                        st_atime: stat_data.st_atime as i64,
                        st_atime_nsec: stat_data.st_atime_nsec as i64,
                        st_mtime: stat_data.st_mtime as i64,
                        st_mtime_nsec: stat_data.st_mtime_nsec as i64,
                        st_ctime: stat_data.st_ctime as i64,
                        st_ctime_nsec: stat_data.st_ctime_nsec as i64,
                        #[cfg(target_os = "macos")]
                        st_birthtime: 0, // Not provided by AgentFS yet
                        #[cfg(target_os = "macos")]
                        st_birthtime_nsec: 0,
                        #[cfg(target_os = "macos")]
                        st_flags: 0,
                        #[cfg(target_os = "macos")]
                        st_gen: 0,
                        #[cfg(target_os = "macos")]
                        st_lspare: 0,
                        #[cfg(target_os = "macos")]
                        st_qspare: [0; 2],
                    };

                    unsafe { *buf = libc_stat };
                    log_message(&format!("fstat succeeded for fd {}", fd));
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("fstat failed: {}, falling back to original", err));
                    // Fall back to original function
                    redhook::real!(fstat)(fd, buf)
                }
            }
        }
    }

    /// Interposed fstatat function
    redhook::hook! {
        unsafe fn fstatat(dirfd: c_int, path: *const c_char, buf: *mut libc::stat, flags: c_int) -> c_int => my_fstatat {
            if path.is_null() || buf.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            log_message(&format!("interposing fstatat({}, {}, {:#x})", dirfd, c_path.to_string_lossy(), flags));

            match send_fstatat_request(dirfd, c_path, flags) {
                Ok(stat_data) => {
                    // Convert StatData to libc::stat
                    let libc_stat = libc::stat {
                        st_dev: stat_data.st_dev as i32,
                        st_ino: stat_data.st_ino,
                        st_mode: stat_data.st_mode as u16,
                        st_nlink: stat_data.st_nlink as u16,
                        st_uid: stat_data.st_uid,
                        st_gid: stat_data.st_gid,
                        st_rdev: stat_data.st_rdev as i32,
                        st_size: stat_data.st_size as i64,
                        st_blksize: stat_data.st_blksize as i32,
                        st_blocks: stat_data.st_blocks as i64,
                        st_atime: stat_data.st_atime as i64,
                        st_atime_nsec: stat_data.st_atime_nsec as i64,
                        st_mtime: stat_data.st_mtime as i64,
                        st_mtime_nsec: stat_data.st_mtime_nsec as i64,
                        st_ctime: stat_data.st_ctime as i64,
                        st_ctime_nsec: stat_data.st_ctime_nsec as i64,
                        #[cfg(target_os = "macos")]
                        st_birthtime: 0, // Not provided by AgentFS yet
                        #[cfg(target_os = "macos")]
                        st_birthtime_nsec: 0,
                        #[cfg(target_os = "macos")]
                        st_flags: 0,
                        #[cfg(target_os = "macos")]
                        st_gen: 0,
                        #[cfg(target_os = "macos")]
                        st_lspare: 0,
                        #[cfg(target_os = "macos")]
                        st_qspare: [0; 2],
                    };

                    unsafe { *buf = libc_stat };
                    log_message("fstatat succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("fstatat failed: {}, falling back to original", err));
                    // Fall back to original function
                    redhook::real!(fstatat)(dirfd, path, buf, flags)
                }
            }
        }
    }

    /// Interposed statfs function
    redhook::hook! {
        unsafe fn statfs(path: *const c_char, buf: *mut libc::statfs) -> c_int => my_statfs {
            if path.is_null() || buf.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            log_message(&format!("interposing statfs({})", c_path.to_string_lossy()));

            match send_statfs_request(c_path) {
                Ok(statfs_data) => {
                    // Convert StatfsData to libc::statfs
                    let libc_statfs = libc::statfs {
                        f_bsize: statfs_data.f_bsize,
                        f_iosize: 0, // Not provided by AgentFS
                        f_blocks: statfs_data.f_blocks,
                        f_bfree: statfs_data.f_bfree,
                        f_bavail: statfs_data.f_bavail,
                        f_files: statfs_data.f_files,
                        f_ffree: statfs_data.f_ffree,
                        f_fsid: unsafe { std::mem::zeroed() }, // Not provided by AgentFS
                        f_owner: 0, // Not provided by AgentFS
                        f_type: 0, // Not provided by AgentFS
                        f_flags: statfs_data.f_flag as u32,
                        f_fssubtype: 0, // Not provided by AgentFS
                        f_fstypename: [0; 16], // Not provided by AgentFS
                        f_mntonname: [0; 1024], // Not provided by AgentFS
                        f_mntfromname: [0; 1024], // Not provided by AgentFS
                        f_flags_ext: 0, // Not provided by AgentFS
                        f_reserved: [0; 7], // Not provided by AgentFS
                    };

                    unsafe { *buf = libc_statfs };
                    log_message("statfs succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("statfs failed: {}, falling back to original", err));
                    // Fall back to original function
                    redhook::real!(statfs)(path, buf)
                }
            }
        }
    }

    /// Interposed fstatfs function
    redhook::hook! {
        unsafe fn fstatfs(fd: c_int, buf: *mut libc::statfs) -> c_int => my_fstatfs {
            if buf.is_null() {
                return -1;
            }

            log_message(&format!("interposing fstatfs({})", fd));

            match send_fstatfs_request(fd) {
                Ok(statfs_data) => {
                    // Convert StatfsData to libc::statfs
                    let libc_statfs = libc::statfs {
                        f_bsize: statfs_data.f_bsize,
                        f_iosize: 0, // Not provided by AgentFS
                        f_blocks: statfs_data.f_blocks,
                        f_bfree: statfs_data.f_bfree,
                        f_bavail: statfs_data.f_bavail,
                        f_files: statfs_data.f_files,
                        f_ffree: statfs_data.f_ffree,
                        f_fsid: unsafe { std::mem::zeroed() }, // Not provided by AgentFS
                        f_owner: 0, // Not provided by AgentFS
                        f_type: 0, // Not provided by AgentFS
                        f_flags: statfs_data.f_flag as u32,
                        f_fssubtype: 0, // Not provided by AgentFS
                        f_fstypename: [0; 16], // Not provided by AgentFS
                        f_mntonname: [0; 1024], // Not provided by AgentFS
                        f_mntfromname: [0; 1024], // Not provided by AgentFS
                        f_flags_ext: 0, // Not provided by AgentFS
                        f_reserved: [0; 7], // Not provided by AgentFS
                    };

                    unsafe { *buf = libc_statfs };
                    log_message(&format!("fstatfs succeeded for fd {}", fd));
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("fstatfs failed: {}, falling back to original", err));
                    // Fall back to original function
                    redhook::real!(fstatfs)(fd, buf)
                }
            }
        }
    }

    /// Interposed truncate function
    redhook::hook! {
        unsafe fn truncate(path: *const c_char, length: off_t) -> c_int => my_truncate {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            log_message(&format!("interposing truncate({}, {})", c_path.to_string_lossy(), length));

            match send_truncate_request(c_path, length) {
                Ok(()) => {
                    log_message("truncate succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("truncate failed: {}, falling back to original", err));
                    // Fall back to original function
                    redhook::real!(truncate)(path, length)
                }
            }
        }
    }

    /// Interposed ftruncate function
    redhook::hook! {
        unsafe fn ftruncate(fd: c_int, length: off_t) -> c_int => my_ftruncate {
            log_message(&format!("interposing ftruncate({}, {})", fd, length));

            match send_ftruncate_request(fd, length) {
                Ok(()) => {
                    log_message(&format!("ftruncate succeeded for fd {}", fd));
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("ftruncate failed: {}, falling back to original", err));
                    // Fall back to original function
                    redhook::real!(ftruncate)(fd, length)
                }
            }
        }
    }

    /// Interposed utimes function
    redhook::hook! {
        unsafe fn utimes(path: *const c_char, times: *const timespec) -> c_int => my_utimes {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            let times_opt = if times.is_null() {
                // Use current time if times is NULL
                None
            } else {
                Some(unsafe { &std::ptr::read(times as *const [timespec; 2]) })
            };
            log_message(&format!("interposing utimes({}, times={:?})", c_path.to_string_lossy(), times_opt.as_ref().map(|t| (t[0].tv_sec, t[1].tv_sec))));

            match send_utimes_request(c_path, times_opt) {
                Ok(()) => {
                    log_message("utimes succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("utimes failed: {}, falling back to original", err));
                    // Fall back to original function
                    redhook::real!(utimes)(path, times)
                }
            }
        }
    }

    /// Interposed futimes function
    redhook::hook! {
        unsafe fn futimes(fd: c_int, times: *const timespec) -> c_int => my_futimes {
            let times_opt = if times.is_null() {
                // Use current time if times is NULL
                None
            } else {
                Some(unsafe { &std::ptr::read(times as *const [timespec; 2]) })
            };
            log_message(&format!("interposing futimes({}, times={:?})", fd, times_opt.as_ref().map(|t| (t[0].tv_sec, t[1].tv_sec))));

            match send_futimes_request(fd, times_opt) {
                Ok(()) => {
                    log_message(&format!("futimes succeeded for fd {}", fd));
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("futimes failed: {}, falling back to original", err));
                    // Fall back to original function
                    redhook::real!(futimes)(fd, times)
                }
            }
        }
    }

    /// Interposed utimensat function
    redhook::hook! {
        unsafe fn utimensat(dirfd: c_int, path: *const c_char, times: *const timespec, flags: c_int) -> c_int => my_utimensat {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            let times_opt = if times.is_null() {
                // Use current time if times is NULL
                None
            } else {
                Some(unsafe { &std::ptr::read(times as *const [timespec; 2]) })
            };
            log_message(&format!("interposing utimensat({}, {}, times={:?}, flags={:#x})", dirfd, c_path.to_string_lossy(), times_opt.as_ref().map(|t| (t[0].tv_sec, t[1].tv_sec)), flags));

            match send_utimensat_request(dirfd, c_path, times_opt, flags) {
                Ok(()) => {
                    log_message("utimensat succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("utimensat failed: {}, falling back to original", err));
                    // Fall back to original function
                    redhook::real!(utimensat)(dirfd, path, times, flags)
                }
            }
        }
    }

    /// Interposed futimens function
    redhook::hook! {
        unsafe fn futimens(fd: c_int, times: *const timespec) -> c_int => my_futimens {
            let times_opt = if times.is_null() {
                // Use current time if times is NULL
                None
            } else {
                Some(unsafe { &std::ptr::read(times as *const [timespec; 2]) })
            };
            log_message(&format!("interposing futimens({}, times={:?})", fd, times_opt.as_ref().map(|t| (t[0].tv_sec, t[1].tv_sec))));

            match send_futimens_request(fd, times_opt) {
                Ok(()) => {
                    log_message(&format!("futimens succeeded for fd {}", fd));
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("futimens failed: {}, falling back to original", err));
                    // Fall back to original function
                    redhook::real!(futimens)(fd, times)
                }
            }
        }
    }

    /// Interposed chown function
    redhook::hook! {
        unsafe fn chown(path: *const c_char, uid: uid_t, gid: gid_t) -> c_int => my_chown {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            log_message(&format!("interposing chown({}, {}, {})", c_path.to_string_lossy(), uid, gid));

            match send_chown_request(c_path, uid, gid) {
                Ok(()) => {
                    log_message("chown succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("chown failed: {}, falling back to original", err));
                    // Fall back to original function
                    redhook::real!(chown)(path, uid, gid)
                }
            }
        }
    }

    /// Interposed lchown function
    redhook::hook! {
        unsafe fn lchown(path: *const c_char, uid: uid_t, gid: gid_t) -> c_int => my_lchown {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            log_message(&format!("interposing lchown({}, {}, {})", c_path.to_string_lossy(), uid, gid));

            match send_lchown_request(c_path, uid, gid) {
                Ok(()) => {
                    log_message("lchown succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("lchown failed: {}, falling back to original", err));
                    // Fall back to original function
                    redhook::real!(lchown)(path, uid, gid)
                }
            }
        }
    }

    /// Interposed fchown function
    redhook::hook! {
        unsafe fn fchown(fd: c_int, uid: uid_t, gid: gid_t) -> c_int => my_fchown {
            log_message(&format!("interposing fchown({}, {}, {})", fd, uid, gid));

            match send_fchown_request(fd, uid, gid) {
                Ok(()) => {
                    log_message(&format!("fchown succeeded for fd {}", fd));
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("fchown failed: {}, falling back to original", err));
                    // Fall back to original function
                    redhook::real!(fchown)(fd, uid, gid)
                }
            }
        }
    }

    /// Interposed fchownat function
    redhook::hook! {
        unsafe fn fchownat(dirfd: c_int, path: *const c_char, uid: uid_t, gid: gid_t, flags: c_int) -> c_int => my_fchownat {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            log_message(&format!("interposing fchownat({}, {}, {}, {}, {:#x})", dirfd, c_path.to_string_lossy(), uid, gid, flags));

            match send_fchownat_request(dirfd, c_path, uid, gid, flags) {
                Ok(()) => {
                    log_message("fchownat succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("fchownat failed: {}, falling back to original", err));
                    // Fall back to original function
                    redhook::real!(fchownat)(dirfd, path, uid, gid, flags)
                }
            }
        }
    }

    /// Interposed chmod function
    redhook::hook! {
        unsafe fn chmod(path: *const c_char, mode: mode_t) -> c_int => my_chmod {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            log_message(&format!("interposing chmod({}, {:#o})", c_path.to_string_lossy(), mode));

            match send_chmod_request(c_path, mode) {
                Ok(()) => {
                    log_message("chmod succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("chmod failed: {}, falling back to original", err));
                    // Fall back to original function
                    redhook::real!(chmod)(path, mode)
                }
            }
        }
    }

    /// Interposed fchmod function
    redhook::hook! {
        unsafe fn fchmod(fd: c_int, mode: mode_t) -> c_int => my_fchmod {
            log_message(&format!("interposing fchmod({}, {:#o})", fd, mode));

            match send_fchmod_request(fd, mode) {
                Ok(()) => {
                    log_message(&format!("fchmod succeeded for fd {}", fd));
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("fchmod failed: {}, falling back to original", err));
                    // Fall back to original function
                    redhook::real!(fchmod)(fd, mode)
                }
            }
        }
    }

    /// Interposed fchmodat function
    redhook::hook! {
        unsafe fn fchmodat(dirfd: c_int, path: *const c_char, mode: mode_t, flags: c_int) -> c_int => my_fchmodat {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            log_message(&format!("interposing fchmodat({}, {}, {:#o}, {:#x})", dirfd, c_path.to_string_lossy(), mode, flags));

            match send_fchmodat_request(dirfd, c_path, mode, flags) {
                Ok(()) => {
                    log_message("fchmodat succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("fchmodat failed: {}, falling back to original", err));
                    // Fall back to original function
                    redhook::real!(fchmodat)(dirfd, path, mode, flags)
                }
            }
        }
    }

    /// Send rename request
    fn send_rename_request(old_path: &CStr, new_path: &CStr) -> Result<(), String> {
        let old_path_str = old_path.to_string_lossy().into_owned();
        let new_path_str = new_path.to_string_lossy().into_owned();
        let request = Request::rename(old_path_str, new_path_str);

        send_request(request, |response| match response {
            Response::Rename(_) => {
                log_message("received rename response");
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send renameat request
    fn send_renameat_request(
        old_dirfd: c_int,
        old_path: &CStr,
        new_dirfd: c_int,
        new_path: &CStr,
    ) -> Result<(), String> {
        let old_resolved_path = resolve_at_path(old_dirfd, old_path);
        let new_resolved_path = resolve_at_path(new_dirfd, new_path);
        let old_path_str = old_resolved_path.to_string_lossy().into_owned();
        let new_path_str = new_resolved_path.to_string_lossy().into_owned();
        let request = Request::renameat(old_path_str, new_path_str);

        send_request(request, |response| match response {
            Response::Renameat(_) => {
                log_message("received renameat response");
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send renameatx_np request (macOS-specific)
    fn send_renameatx_np_request(
        old_dirfd: c_int,
        old_path: &CStr,
        new_dirfd: c_int,
        new_path: &CStr,
        flags: c_int,
    ) -> Result<(), String> {
        let old_path_str = old_path.to_string_lossy().into_owned();
        let new_path_str = new_path.to_string_lossy().into_owned();
        let request = Request::renameatx_np(
            old_dirfd as u32,
            old_path_str,
            new_dirfd as u32,
            new_path_str,
            flags as u32,
        );

        send_request(request, |response| match response {
            Response::RenameatxNp(_) => {
                log_message("received renameatx_np response");
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send link request
    fn send_link_request(old_path: &CStr, new_path: &CStr) -> Result<(), String> {
        let old_path_str = old_path.to_string_lossy().into_owned();
        let new_path_str = new_path.to_string_lossy().into_owned();
        let request = Request::link(old_path_str, new_path_str);

        send_request(request, |response| match response {
            Response::Link(_) => {
                log_message("received link response");
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send linkat request
    fn send_linkat_request(
        old_dirfd: c_int,
        old_path: &CStr,
        new_dirfd: c_int,
        new_path: &CStr,
        flags: c_int,
    ) -> Result<(), String> {
        let old_resolved_path = resolve_at_path(old_dirfd, old_path);
        let new_resolved_path = resolve_at_path(new_dirfd, new_path);
        let old_path_str = old_resolved_path.to_string_lossy().into_owned();
        let new_path_str = new_resolved_path.to_string_lossy().into_owned();
        let request = Request::linkat(old_path_str, new_path_str, flags as u32);

        send_request(request, |response| match response {
            Response::Linkat(_) => {
                log_message("received linkat response");
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send symlink request
    fn send_symlink_request(target: &CStr, linkpath: &CStr) -> Result<(), String> {
        let target_str = target.to_string_lossy().into_owned();
        let linkpath_str = linkpath.to_string_lossy().into_owned();
        let request = Request::symlink(target_str, linkpath_str);

        send_request(request, |response| match response {
            Response::Symlink(_) => {
                log_message("received symlink response");
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send symlinkat request
    fn send_symlinkat_request(
        target: &CStr,
        new_dirfd: c_int,
        linkpath: &CStr,
    ) -> Result<(), String> {
        let target_str = target.to_string_lossy().into_owned();
        let resolved_linkpath = resolve_at_path(new_dirfd, linkpath);
        let linkpath_str = resolved_linkpath.to_string_lossy().into_owned();
        let request = Request::symlinkat(target_str, linkpath_str);

        send_request(request, |response| match response {
            Response::Symlinkat(_) => {
                log_message("received symlinkat response");
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send unlink request
    fn send_unlink_request(path: &CStr) -> Result<(), String> {
        let path_str = path.to_string_lossy().into_owned();
        let request = Request::unlink(path_str);

        send_request(request, |response| match response {
            Response::Unlink(_) => {
                log_message("received unlink response");
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send unlinkat request
    fn send_unlinkat_request(dirfd: c_int, path: &CStr, flags: c_int) -> Result<(), String> {
        let resolved_path = resolve_at_path(dirfd, path);
        let path_str = resolved_path.to_string_lossy().into_owned();
        let request = Request::unlinkat(path_str, flags as u32);

        send_request(request, |response| match response {
            Response::Unlinkat(_) => {
                log_message("received unlinkat response");
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send remove request (alias for unlink)
    fn send_remove_request(path: &CStr) -> Result<(), String> {
        let path_str = path.to_string_lossy().into_owned();
        let request = Request::remove(path_str);

        send_request(request, |response| match response {
            Response::Remove(_) => {
                log_message("received remove response");
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send mkdir request
    fn send_mkdir_request(path: &CStr, mode: mode_t) -> Result<(), String> {
        let path_str = path.to_string_lossy().into_owned();
        let request = Request::mkdir(path_str, mode as u32);

        send_request(request, |response| match response {
            Response::Mkdir(_) => {
                log_message("received mkdir response");
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send mkdirat request
    fn send_mkdirat_request(dirfd: c_int, path: &CStr, mode: mode_t) -> Result<(), String> {
        let resolved_path = resolve_at_path(dirfd, path);
        let path_str = resolved_path.to_string_lossy().into_owned();
        let request = Request::mkdirat(path_str, mode as u32);

        send_request(request, |response| match response {
            Response::Mkdirat(_) => {
                log_message("received mkdirat response");
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Interposed rename function
    redhook::hook! {
        unsafe fn rename(old_path: *const c_char, new_path: *const c_char) -> c_int => my_rename {
            if old_path.is_null() || new_path.is_null() {
                return -1;
            }

            let c_old_path = unsafe { CStr::from_ptr(old_path) };
            let c_new_path = unsafe { CStr::from_ptr(new_path) };
            log_message(&format!("interposing rename({}, {})", c_old_path.to_string_lossy(), c_new_path.to_string_lossy()));

            match send_rename_request(c_old_path, c_new_path) {
                Ok(()) => {
                    log_message("rename succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("rename failed: {}, falling back to original", err));
                    // Fall back to original function
                    redhook::real!(rename)(old_path, new_path)
                }
            }
        }
    }

    /// Interposed renameat function
    redhook::hook! {
        unsafe fn renameat(old_dirfd: c_int, old_path: *const c_char, new_dirfd: c_int, new_path: *const c_char) -> c_int => my_renameat {
            if old_path.is_null() || new_path.is_null() {
                return -1;
            }

            let c_old_path = unsafe { CStr::from_ptr(old_path) };
            let c_new_path = unsafe { CStr::from_ptr(new_path) };
            log_message(&format!("interposing renameat({}, {}, {}, {})", old_dirfd, c_old_path.to_string_lossy(), new_dirfd, c_new_path.to_string_lossy()));

            match send_renameat_request(old_dirfd, c_old_path, new_dirfd, c_new_path) {
                Ok(()) => {
                    log_message("renameat succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("renameat failed: {}, falling back to original", err));
                    // Fall back to original function
                    redhook::real!(renameat)(old_dirfd, old_path, new_dirfd, new_path)
                }
            }
        }
    }

    /// Interposed renameatx_np function (macOS-specific)
    redhook::hook! {
        unsafe fn renameatx_np(old_dirfd: c_int, old_path: *const c_char, new_dirfd: c_int, new_path: *const c_char, flags: libc::c_uint) -> c_int => my_renameatx_np {
            if old_path.is_null() || new_path.is_null() {
                return -1;
            }

            let c_old_path = unsafe { CStr::from_ptr(old_path) };
            let c_new_path = unsafe { CStr::from_ptr(new_path) };
            log_message(&format!("interposing renameatx_np({}, {}, {}, {}, {:#x})", old_dirfd, c_old_path.to_string_lossy(), new_dirfd, c_new_path.to_string_lossy(), flags));

            match send_renameatx_np_request(old_dirfd, c_old_path, new_dirfd, c_new_path, flags as c_int) {
                Ok(()) => {
                    log_message("renameatx_np succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("renameatx_np failed: {}, falling back to original", err));
                    // Fall back to original function
                    redhook::real!(renameatx_np)(old_dirfd, old_path, new_dirfd, new_path, flags)
                }
            }
        }
    }

    /// Interposed link function
    redhook::hook! {
        unsafe fn link(old_path: *const c_char, new_path: *const c_char) -> c_int => my_link {
            if old_path.is_null() || new_path.is_null() {
                return -1;
            }

            let c_old_path = unsafe { CStr::from_ptr(old_path) };
            let c_new_path = unsafe { CStr::from_ptr(new_path) };
            log_message(&format!("interposing link({}, {})", c_old_path.to_string_lossy(), c_new_path.to_string_lossy()));

            match send_link_request(c_old_path, c_new_path) {
                Ok(()) => {
                    log_message("link succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("link failed: {}, falling back to original", err));
                    // Fall back to original function
                    redhook::real!(link)(old_path, new_path)
                }
            }
        }
    }

    /// Interposed linkat function
    redhook::hook! {
        unsafe fn linkat(old_dirfd: c_int, old_path: *const c_char, new_dirfd: c_int, new_path: *const c_char, flags: c_int) -> c_int => my_linkat {
            if old_path.is_null() || new_path.is_null() {
                return -1;
            }

            let c_old_path = unsafe { CStr::from_ptr(old_path) };
            let c_new_path = unsafe { CStr::from_ptr(new_path) };
            log_message(&format!("interposing linkat({}, {}, {}, {}, {:#x})", old_dirfd, c_old_path.to_string_lossy(), new_dirfd, c_new_path.to_string_lossy(), flags));

            match send_linkat_request(old_dirfd, c_old_path, new_dirfd, c_new_path, flags) {
                Ok(()) => {
                    log_message("linkat succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("linkat failed: {}, falling back to original", err));
                    // Fall back to original function
                    redhook::real!(linkat)(old_dirfd, old_path, new_dirfd, new_path, flags)
                }
            }
        }
    }

    /// Interposed symlink function
    redhook::hook! {
        unsafe fn symlink(target: *const c_char, linkpath: *const c_char) -> c_int => my_symlink {
            if target.is_null() || linkpath.is_null() {
                return -1;
            }

            let c_target = unsafe { CStr::from_ptr(target) };
            let c_linkpath = unsafe { CStr::from_ptr(linkpath) };
            log_message(&format!("interposing symlink({}, {})", c_target.to_string_lossy(), c_linkpath.to_string_lossy()));

            match send_symlink_request(c_target, c_linkpath) {
                Ok(()) => {
                    log_message("symlink succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("symlink failed: {}, falling back to original", err));
                    // Fall back to original function
                    redhook::real!(symlink)(target, linkpath)
                }
            }
        }
    }

    /// Interposed symlinkat function
    redhook::hook! {
        unsafe fn symlinkat(target: *const c_char, new_dirfd: c_int, linkpath: *const c_char) -> c_int => my_symlinkat {
            if target.is_null() || linkpath.is_null() {
                return -1;
            }

            let c_target = unsafe { CStr::from_ptr(target) };
            let c_linkpath = unsafe { CStr::from_ptr(linkpath) };
            log_message(&format!("interposing symlinkat({}, {}, {})", c_target.to_string_lossy(), new_dirfd, c_linkpath.to_string_lossy()));

            match send_symlinkat_request(c_target, new_dirfd, c_linkpath) {
                Ok(()) => {
                    log_message("symlinkat succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("symlinkat failed: {}, falling back to original", err));
                    // Fall back to original function
                    redhook::real!(symlinkat)(target, new_dirfd, linkpath)
                }
            }
        }
    }

    /// Interposed unlink function
    redhook::hook! {
        unsafe fn unlink(path: *const c_char) -> c_int => my_unlink {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            log_message(&format!("interposing unlink({})", c_path.to_string_lossy()));

            match send_unlink_request(c_path) {
                Ok(()) => {
                    log_message("unlink succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("unlink failed: {}, falling back to original", err));
                    // Fall back to original function
                    redhook::real!(unlink)(path)
                }
            }
        }
    }

    /// Interposed unlinkat function
    redhook::hook! {
        unsafe fn unlinkat(dirfd: c_int, path: *const c_char, flags: c_int) -> c_int => my_unlinkat {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            log_message(&format!("interposing unlinkat({}, {}, {:#x})", dirfd, c_path.to_string_lossy(), flags));

            match send_unlinkat_request(dirfd, c_path, flags) {
                Ok(()) => {
                    log_message("unlinkat succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("unlinkat failed: {}, falling back to original", err));
                    // Fall back to original function
                    redhook::real!(unlinkat)(dirfd, path, flags)
                }
            }
        }
    }

    /// Interposed remove function (alias for unlink)
    redhook::hook! {
        unsafe fn remove(path: *const c_char) -> c_int => my_remove {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            log_message(&format!("interposing remove({})", c_path.to_string_lossy()));

            match send_remove_request(c_path) {
                Ok(()) => {
                    log_message("remove succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("remove failed: {}, falling back to original", err));
                    // Fall back to original function
                    redhook::real!(remove)(path)
                }
            }
        }
    }

    /// Interposed mkdir function
    redhook::hook! {
        unsafe fn mkdir(path: *const c_char, mode: mode_t) -> c_int => my_mkdir {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            log_message(&format!("interposing mkdir({}, {:#o})", c_path.to_string_lossy(), mode));

            match send_mkdir_request(c_path, mode) {
                Ok(()) => {
                    log_message("mkdir succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("mkdir failed: {}, falling back to original", err));
                    // Fall back to original function
                    redhook::real!(mkdir)(path, mode)
                }
            }
        }
    }

    /// Interposed mkdirat function
    redhook::hook! {
        unsafe fn mkdirat(dirfd: c_int, path: *const c_char, mode: mode_t) -> c_int => my_mkdirat {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            log_message(&format!("interposing mkdirat({}, {}, {:#o})", dirfd, c_path.to_string_lossy(), mode));

            match send_mkdirat_request(dirfd, c_path, mode) {
                Ok(()) => {
                    log_message("mkdirat succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("mkdirat failed: {}, falling back to original", err));
                    // Fall back to original function
                    redhook::real!(mkdirat)(dirfd, path, mode)
                }
            }
        }
    }

    /// Interposed close function (for fd tracking)
    redhook::hook! {
        unsafe fn close(fd: c_int) -> c_int => my_close_fd_tracking {
            // Remove fd mapping before closing
            with_dirfd_mapping(|mapping| {
                mapping.remove_path(fd);
                log_message(&format!("removed fd {} from tracking", fd));
            });

            redhook::real!(close)(fd)
        }
    }

    /// Interposed dup function (for fd tracking)
    redhook::hook! {
        unsafe fn dup(oldfd: c_int) -> c_int => my_dup_fd_tracking {
            let result = redhook::real!(dup)(oldfd);
            if result >= 0 {
                // Duplicate the fd mapping
                with_dirfd_mapping(|mapping| {
                    mapping.dup_fd(oldfd, result);
                    log_message(&format!("duplicated fd {} -> {}", oldfd, result));
                });
            }
            result
        }
    }

    /// Interposed dup2 function (for fd tracking)
    redhook::hook! {
        unsafe fn dup2(oldfd: c_int, newfd: c_int) -> c_int => my_dup2_fd_tracking {
            let result = redhook::real!(dup2)(oldfd, newfd);
            if result >= 0 {
                // Duplicate the fd mapping
                with_dirfd_mapping(|mapping| {
                    mapping.dup_fd(oldfd, newfd);
                    log_message(&format!("duplicated fd {} -> {} with dup2", oldfd, newfd));
                });
            }
            result
        }
    }

    /// Interposed dup3 function (for fd tracking)
    redhook::hook! {
        unsafe fn dup3(oldfd: c_int, newfd: c_int, flags: c_int) -> c_int => my_dup3_fd_tracking {
            let result = redhook::real!(dup3)(oldfd, newfd, flags);
            if result >= 0 {
                // Duplicate the fd mapping
                with_dirfd_mapping(|mapping| {
                    mapping.dup_fd(oldfd, newfd);
                    log_message(&format!("duplicated fd {} -> {} with dup3", oldfd, newfd));
                });
            }
            result
        }
    }

    /// Interposed chdir function (for cwd tracking)
    redhook::hook! {
        unsafe fn chdir(path: *const c_char) -> c_int => my_chdir_fd_tracking {
            if path.is_null() {
                return redhook::real!(chdir)(path);
            }

            let result = redhook::real!(chdir)(path);
            if result == 0 {
                // Update current working directory
                let c_path = unsafe { CStr::from_ptr(path) };
                if let Ok(path_str) = c_path.to_str() {
                    with_dirfd_mapping(|mapping| {
                        let path_buf = PathBuf::from(path_str);
                        let canonical_path = path_buf.canonicalize().unwrap_or(path_buf);
                        mapping.set_cwd(canonical_path.clone());
                        log_message(&format!("updated cwd to {}", canonical_path.display()));
                    });
                }
            }
            result
        }
    }

    /// Interposed fchdir function (for cwd tracking)
    redhook::hook! {
        unsafe fn fchdir(fd: c_int) -> c_int => my_fchdir_fd_tracking {
            let result = redhook::real!(fchdir)(fd);
            if result == 0 {
                // Update current working directory from fd
                with_dirfd_mapping(|mapping| {
                    if let Some(path) = mapping.get_path(fd).cloned() {
                        mapping.set_cwd(path.clone());
                        log_message(&format!("updated cwd to {} via fd {}", path.display(), fd));
                    }
                });
            }
            result
        }
    }

    /// Interposed getxattr function
    redhook::hook! {
        unsafe fn getxattr(path: *const c_char, name: *const c_char, value: *mut c_void, size: size_t) -> ssize_t => my_getxattr {
            if path.is_null() || name.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            let c_name = unsafe { CStr::from_ptr(name) };

            log_message(&format!("interposing getxattr({}, {}, {}, {})",
                c_path.to_string_lossy(), c_name.to_string_lossy(), value as usize, size));

            let request = GetxattrRequest {
                path: c_path.to_bytes().to_vec(),
                name: c_name.to_bytes().to_vec(),
            };

            match send_request(Request::Getxattr((b"1".to_vec(), request)), |response| match response {
                Response::Getxattr(resp) => Some(resp),
                _ => None,
            }) {
                Ok(response) => {
                    let value_len = response.value.len();
                    if size == 0 {
                        // Return required buffer size
                        return value_len as ssize_t;
                    } else if value.is_null() {
                        return -1;
                    } else if value_len > size {
                        // Buffer too small
                        unsafe { *libc::__error() = libc::ERANGE };
                        return -1;
                    } else {
                        // Copy value to buffer
                        unsafe {
                            std::ptr::copy_nonoverlapping(
                                response.value.as_ptr(),
                                value as *mut u8,
                                value_len
                            );
                        }
                        return value_len as ssize_t;
                    }
                }
                Err(_) => {
                    // Fallback to original function
                    return redhook::real!(getxattr)(path, name, value, size);
                }
            }
        }
    }

    /// Interposed lgetxattr function
    redhook::hook! {
        unsafe fn lgetxattr(path: *const c_char, name: *const c_char, value: *mut c_void, size: size_t, position: u32, options: c_int) -> ssize_t => my_lgetxattr {
            if path.is_null() || name.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            let c_name = unsafe { CStr::from_ptr(name) };

            log_message(&format!("interposing lgetxattr({}, {}, {}, {}, {}, {})",
                c_path.to_string_lossy(), c_name.to_string_lossy(), value as usize, size, position, options));

            let request = LgetxattrRequest {
                path: c_path.to_bytes().to_vec(),
                name: c_name.to_bytes().to_vec(),
            };

            match send_request(Request::Lgetxattr((b"1".to_vec(), request)), |response| match response {
                Response::Lgetxattr(resp) => Some(resp),
                _ => None,
            }) {
                Ok(response) => {
                    let value_len = response.value.len();
                    if size == 0 {
                        return value_len as ssize_t;
                    } else if value.is_null() {
                        return -1;
                    } else if value_len > size {
                        unsafe { *libc::__error() = libc::ERANGE };
                        return -1;
                    } else {
                        unsafe {
                            std::ptr::copy_nonoverlapping(
                                response.value.as_ptr(),
                                value as *mut u8,
                                value_len
                            );
                        }
                        return value_len as ssize_t;
                    }
                }
                _ => {
                    return redhook::real!(lgetxattr)(path, name, value, size, position, options);
                }
            }
        }
    }

    /// Interposed fgetxattr function
    redhook::hook! {
        unsafe fn fgetxattr(fd: c_int, name: *const c_char, value: *mut c_void, size: size_t, position: u32, options: c_int) -> ssize_t => my_fgetxattr {
            if name.is_null() {
                return -1;
            }

            let c_name = unsafe { CStr::from_ptr(name) };

            log_message(&format!("interposing fgetxattr({}, {}, {}, {}, {}, {})",
                fd, c_name.to_string_lossy(), value as usize, size, position, options));

            let request = FgetxattrRequest {
                handle_id: fd as u64, // For now, assume fd maps directly to handle_id
                name: c_name.to_bytes().to_vec(),
            };

            match send_request(Request::Fgetxattr((b"1".to_vec(), request)), |response| match response {
                Response::Fgetxattr(resp) => Some(resp),
                _ => None,
            }) {
                Ok(response) => {
                    let value_len = response.value.len();
                    if size == 0 {
                        return value_len as ssize_t;
                    } else if value.is_null() {
                        return -1;
                    } else if value_len > size {
                        unsafe { *libc::__error() = libc::ERANGE };
                        return -1;
                    } else {
                        unsafe {
                            std::ptr::copy_nonoverlapping(
                                response.value.as_ptr(),
                                value as *mut u8,
                                value_len
                            );
                        }
                        return value_len as ssize_t;
                    }
                }
                _ => {
                    return redhook::real!(fgetxattr)(fd, name, value, size, position, options);
                }
            }
        }
    }

    /// Interposed setxattr function
    redhook::hook! {
        unsafe fn setxattr(path: *const c_char, name: *const c_char, value: *const c_void, size: size_t, position: u32, options: c_int) -> c_int => my_setxattr {
            if path.is_null() || name.is_null() || value.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            let c_name = unsafe { CStr::from_ptr(name) };
            let value_slice = unsafe { std::slice::from_raw_parts(value as *const u8, size) };

            log_message(&format!("interposing setxattr({}, {}, value[{}])",
                c_path.to_string_lossy(), c_name.to_string_lossy(), size));

            let request = SetxattrRequest {
                path: c_path.to_bytes().to_vec(),
                name: c_name.to_bytes().to_vec(),
                value: value_slice.to_vec(),
                flags: options as u32,
            };

            match send_request(Request::Setxattr((b"1".to_vec(), request)), |response| match response {
                Response::Setxattr(_) => Some(()),
                _ => None,
            }) {
                Ok(_) => 0,
                _ => {
                    return redhook::real!(setxattr)(path, name, value, size, position, options);
                }
            }
        }
    }

    /// Interposed lsetxattr function
    redhook::hook! {
        unsafe fn lsetxattr(path: *const c_char, name: *const c_char, value: *const c_void, size: size_t, position: u32, options: c_int) -> c_int => my_lsetxattr {
            if path.is_null() || name.is_null() || value.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            let c_name = unsafe { CStr::from_ptr(name) };
            let value_slice = unsafe { std::slice::from_raw_parts(value as *const u8, size) };

            log_message(&format!("interposing lsetxattr({}, {}, value[{}])",
                c_path.to_string_lossy(), c_name.to_string_lossy(), size));

            let request = LsetxattrRequest {
                path: c_path.to_bytes().to_vec(),
                name: c_name.to_bytes().to_vec(),
                value: value_slice.to_vec(),
                flags: options as u32,
            };

            match send_request(Request::Lsetxattr((b"1".to_vec(), request)), |response| match response {
                Response::Lsetxattr(_) => Some(()),
                _ => None,
            }) {
                Ok(_) => 0,
                _ => {
                    return redhook::real!(lsetxattr)(path, name, value, size, position, options);
                }
            }
        }
    }

    /// Interposed fsetxattr function
    redhook::hook! {
        unsafe fn fsetxattr(fd: c_int, name: *const c_char, value: *const c_void, size: size_t, position: u32, options: c_int) -> c_int => my_fsetxattr {
            if name.is_null() || value.is_null() {
                return -1;
            }

            let c_name = unsafe { CStr::from_ptr(name) };
            let value_slice = unsafe { std::slice::from_raw_parts(value as *const u8, size) };

            log_message(&format!("interposing fsetxattr({}, {}, value[{}])",
                fd, c_name.to_string_lossy(), size));

            let request = FsetxattrRequest {
                handle_id: fd as u64, // For now, assume fd maps directly to handle_id
                name: c_name.to_bytes().to_vec(),
                value: value_slice.to_vec(),
                flags: options as u32,
            };

            match send_request(Request::Fsetxattr((b"1".to_vec(), request)), |response| match response {
                Response::Fsetxattr(_) => Some(()),
                _ => None,
            }) {
                Ok(_) => 0,
                _ => {
                    return redhook::real!(fsetxattr)(fd, name, value, size, position, options);
                }
            }
        }
    }

    /// Interposed listxattr function
    redhook::hook! {
        unsafe fn listxattr(path: *const c_char, namebuf: *mut c_char, size: size_t, options: c_int) -> ssize_t => my_listxattr {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };

            log_message(&format!("interposing listxattr({}, {}, {}, {})",
                c_path.to_string_lossy(), namebuf as usize, size, options));

            let request = ListxattrRequest {
                path: c_path.to_bytes().to_vec(),
            };

            match send_request(Request::Listxattr((b"1".to_vec(), request)), |response| match response {
                Response::Listxattr(resp) => Some(resp),
                _ => None,
            }) {
                Ok(response) => {
                    let names_data: Vec<u8> = response.names.into_iter().flatten().collect();
                    let names_len = names_data.len();
                    if size == 0 {
                        return names_len as ssize_t;
                    } else if namebuf.is_null() {
                        return -1;
                    } else if names_len > size {
                        unsafe { *libc::__error() = libc::ERANGE };
                        return -1;
                    } else {
                        unsafe {
                            std::ptr::copy_nonoverlapping(
                                names_data.as_ptr(),
                                namebuf as *mut u8,
                                names_len
                            );
                        }
                        return names_len as ssize_t;
                    }
                }
                _ => {
                    return redhook::real!(listxattr)(path, namebuf, size, options);
                }
            }
        }
    }

    /// Interposed llistxattr function
    redhook::hook! {
        unsafe fn llistxattr(path: *const c_char, namebuf: *mut c_char, size: size_t, options: c_int) -> ssize_t => my_llistxattr {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };

            log_message(&format!("interposing llistxattr({}, {}, {}, {})",
                c_path.to_string_lossy(), namebuf as usize, size, options));

            let request = LlistxattrRequest {
                path: c_path.to_bytes().to_vec(),
            };

            match send_request(Request::Llistxattr((b"1".to_vec(), request)), |response| match response {
                Response::Llistxattr(resp) => Some(resp),
                _ => None,
            }) {
                Ok(response) => {
                    let names_data: Vec<u8> = response.names.into_iter().flatten().collect();
                    let names_len = names_data.len();
                    if size == 0 {
                        return names_len as ssize_t;
                    } else if namebuf.is_null() {
                        return -1;
                    } else if names_len > size {
                        unsafe { *libc::__error() = libc::ERANGE };
                        return -1;
                    } else {
                        unsafe {
                            std::ptr::copy_nonoverlapping(
                                names_data.as_ptr(),
                                namebuf as *mut u8,
                                names_len
                            );
                        }
                        return names_len as ssize_t;
                    }
                }
                _ => {
                    return redhook::real!(llistxattr)(path, namebuf, size, options);
                }
            }
        }
    }

    /// Interposed flistxattr function
    redhook::hook! {
        unsafe fn flistxattr(fd: c_int, namebuf: *mut c_char, size: size_t, options: c_int) -> ssize_t => my_flistxattr {
            log_message(&format!("interposing flistxattr({}, {}, {}, {})",
                fd, namebuf as usize, size, options));

            let request = FlistxattrRequest {
                handle_id: fd as u64, // For now, assume fd maps directly to handle_id
            };

            match send_request(Request::Flistxattr((b"1".to_vec(), request)), |response| match response {
                Response::Flistxattr(resp) => Some(resp),
                _ => None,
            }) {
                Ok(response) => {
                    let names_data: Vec<u8> = response.names.into_iter().flatten().collect();
                    let names_len = names_data.len();
                    if size == 0 {
                        return names_len as ssize_t;
                    } else if namebuf.is_null() {
                        return -1;
                    } else if names_len > size {
                        unsafe { *libc::__error() = libc::ERANGE };
                        return -1;
                    } else {
                        unsafe {
                            std::ptr::copy_nonoverlapping(
                                names_data.as_ptr(),
                                namebuf as *mut u8,
                                names_len
                            );
                        }
                        return names_len as ssize_t;
                    }
                }
                _ => {
                    return redhook::real!(flistxattr)(fd, namebuf, size, options);
                }
            }
        }
    }

    /// Interposed removexattr function
    redhook::hook! {
        unsafe fn removexattr(path: *const c_char, name: *const c_char, options: c_int) -> c_int => my_removexattr {
            if path.is_null() || name.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            let c_name = unsafe { CStr::from_ptr(name) };

            log_message(&format!("interposing removexattr({}, {}, {})",
                c_path.to_string_lossy(), c_name.to_string_lossy(), options));

            let request = RemovexattrRequest {
                path: c_path.to_bytes().to_vec(),
                name: c_name.to_bytes().to_vec(),
            };

            match send_request(Request::Removexattr((b"1".to_vec(), request)), |response| match response {
                Response::Removexattr(_) => Some(()),
                _ => None,
            }) {
                Ok(_) => 0,
                _ => {
                    return redhook::real!(removexattr)(path, name, options);
                }
            }
        }
    }

    /// Interposed lremovexattr function
    redhook::hook! {
        unsafe fn lremovexattr(path: *const c_char, name: *const c_char, options: c_int) -> c_int => my_lremovexattr {
            if path.is_null() || name.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            let c_name = unsafe { CStr::from_ptr(name) };

            log_message(&format!("interposing lremovexattr({}, {}, {})",
                c_path.to_string_lossy(), c_name.to_string_lossy(), options));

            let request = LremovexattrRequest {
                path: c_path.to_bytes().to_vec(),
                name: c_name.to_bytes().to_vec(),
            };

            match send_request(Request::Lremovexattr((b"1".to_vec(), request)), |response| match response {
                Response::Lremovexattr(_) => Some(()),
                _ => None,
            }) {
                Ok(_) => 0,
                _ => {
                    return redhook::real!(lremovexattr)(path, name, options);
                }
            }
        }
    }

    /// Interposed fremovexattr function
    redhook::hook! {
        unsafe fn fremovexattr(fd: c_int, name: *const c_char, options: c_int) -> c_int => my_fremovexattr {
            if name.is_null() {
                return -1;
            }

            let c_name = unsafe { CStr::from_ptr(name) };

            log_message(&format!("interposing fremovexattr({}, {}, {})",
                fd, c_name.to_string_lossy(), options));

            let request = FremovexattrRequest {
                handle_id: fd as u64, // For now, assume fd maps directly to handle_id
                name: c_name.to_bytes().to_vec(),
            };

            match send_request(Request::Fremovexattr((b"1".to_vec(), request)), |response| match response {
                Response::Fremovexattr(_) => Some(()),
                _ => None,
            }) {
                Ok(_) => 0,
                Err(_) => {
                    return redhook::real!(fremovexattr)(fd, name, options);
                }
            }
        }
    }

    /// Interposed acl_get_file function
    redhook::hook! {
        unsafe fn acl_get_file(path: *const c_char, acl_type: acl_type_t) -> acl_t => my_acl_get_file {
            if path.is_null() {
                return std::ptr::null_mut();
            }

            let c_path = unsafe { CStr::from_ptr(path) };

            log_message(&format!("interposing acl_get_file({}, {})",
                c_path.to_string_lossy(), acl_type));

            let request = AclGetFileRequest {
                path: c_path.to_bytes().to_vec(),
                acl_type: acl_type as u32,
            };

            match send_request(Request::AclGetFile((b"1".to_vec(), request)), |response| match response {
                Response::AclGetFile(resp) => Some(resp.acl_data),
                _ => None,
            }) {
                Ok(acl_data) => {
                    // Convert binary ACL data back to acl_t
                    if !acl_data.is_empty() {
                        1 as acl_t
                    } else {
                        std::ptr::null_mut()
                    }
                }
                Err(_) => {
                    return redhook::real!(acl_get_file)(path, acl_type);
                }
            }
        }
    }

    /// Interposed acl_set_file function
    redhook::hook! {
        unsafe fn acl_set_file(path: *const c_char, acl_type: acl_type_t, acl: acl_t) -> c_int => my_acl_set_file {
            if path.is_null() || acl.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };

            log_message(&format!("interposing acl_set_file({}, {}, {:p})",
                c_path.to_string_lossy(), acl_type, acl));

            // Convert acl_t to binary data
            let acl_data = if acl == 1 as acl_t {
                vec![1] // Dummy data
            } else {
                Vec::new()
            };

            let request = AclSetFileRequest {
                path: c_path.to_bytes().to_vec(),
                acl_type: acl_type as u32,
                acl_data,
            };

            match send_request(Request::AclSetFile((b"1".to_vec(), request)), |response| match response {
                Response::AclSetFile(_) => Some(()),
                _ => None,
            }) {
                Ok(_) => 0,
                Err(_) => {
                    return redhook::real!(acl_set_file)(path, acl_type, acl);
                }
            }
        }
    }

    /// Interposed acl_get_fd function
    redhook::hook! {
        unsafe fn acl_get_fd(fd: c_int, acl_type: acl_type_t) -> acl_t => my_acl_get_fd {
            log_message(&format!("interposing acl_get_fd({}, {})", fd, acl_type));

            let request = AclGetFdRequest {
                handle_id: fd as u64,
                acl_type: acl_type as u32,
            };

            match send_request(Request::AclGetFd((b"1".to_vec(), request)), |response| match response {
                Response::AclGetFd(resp) => Some(resp.acl_data),
                _ => None,
            }) {
                Ok(acl_data) => {
                    // Convert binary ACL data back to acl_t
                    if !acl_data.is_empty() {
                        1 as acl_t
                    } else {
                        std::ptr::null_mut()
                    }
                }
                Err(_) => {
                    return redhook::real!(acl_get_fd)(fd, acl_type);
                }
            }
        }
    }

    /// Interposed acl_set_fd function
    redhook::hook! {
        unsafe fn acl_set_fd(fd: c_int, acl_type: acl_type_t, acl: acl_t) -> c_int => my_acl_set_fd {
            if acl.is_null() {
                return -1;
            }

            log_message(&format!("interposing acl_set_fd({}, {}, {:p})", fd, acl_type, acl));

            // Convert acl_t to binary data
            let acl_data = if acl == 1 as acl_t {
                vec![1] // Dummy data
            } else {
                Vec::new()
            };

            let request = AclSetFdRequest {
                handle_id: fd as u64,
                acl_type: acl_type as u32,
                acl_data,
            };

            match send_request(Request::AclSetFd((b"1".to_vec(), request)), |response| match response {
                Response::AclSetFd(_) => Some(()),
                _ => None,
            }) {
                Ok(_) => 0,
                Err(_) => {
                    return redhook::real!(acl_set_fd)(fd, acl_type, acl);
                }
            }
        }
    }

    /// Interposed acl_delete_def_file function
    redhook::hook! {
        unsafe fn acl_delete_def_file(path: *const c_char) -> c_int => my_acl_delete_def_file {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };

            log_message(&format!("interposing acl_delete_def_file({})",
                c_path.to_string_lossy()));

            let request = AclDeleteDefFileRequest {
                path: c_path.to_bytes().to_vec(),
            };

            match send_request(Request::AclDeleteDefFile((b"1".to_vec(), request)), |response| match response {
                Response::AclDeleteDefFile(_) => Some(()),
                _ => None,
            }) {
                Ok(_) => 0,
                Err(_) => {
                    return redhook::real!(acl_delete_def_file)(path);
                }
            }
        }
    }

    /// Interposed chflags function
    redhook::hook! {
        unsafe fn chflags(path: *const c_char, flags: libc::c_uint) -> c_int => my_chflags {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };

            log_message(&format!("interposing chflags({}, {:#x})",
                c_path.to_string_lossy(), flags));

            let request = ChflagsRequest {
                path: c_path.to_bytes().to_vec(),
                flags,
            };

            match send_request(Request::Chflags((b"1".to_vec(), request)), |response| match response {
                Response::Chflags(_) => Some(()),
                _ => None,
            }) {
                Ok(_) => 0,
                Err(_) => {
                    return redhook::real!(chflags)(path, flags);
                }
            }
        }
    }

    /// Interposed lchflags function
    redhook::hook! {
        unsafe fn lchflags(path: *const c_char, flags: libc::c_uint) -> c_int => my_lchflags {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };

            log_message(&format!("interposing lchflags({}, {:#x})",
                c_path.to_string_lossy(), flags));

            let request = LchflagsRequest {
                path: c_path.to_bytes().to_vec(),
                flags,
            };

            match send_request(Request::Lchflags((b"1".to_vec(), request)), |response| match response {
                Response::Lchflags(_) => Some(()),
                _ => None,
            }) {
                Ok(_) => 0,
                Err(_) => {
                    return redhook::real!(lchflags)(path, flags);
                }
            }
        }
    }

    /// Interposed fchflags function
    redhook::hook! {
        unsafe fn fchflags(fd: c_int, flags: libc::c_uint) -> c_int => my_fchflags {
            log_message(&format!("interposing fchflags({}, {:#x})", fd, flags));

            let request = FchflagsRequest {
                handle_id: fd as u64,
                flags,
            };

            match send_request(Request::Fchflags((b"1".to_vec(), request)), |response| match response {
                Response::Fchflags(_) => Some(()),
                _ => None,
            }) {
                Ok(_) => 0,
                Err(_) => {
                    return redhook::real!(fchflags)(fd, flags);
                }
            }
        }
    }

    /// Interposed getattrlist function
    redhook::hook! {
        unsafe fn getattrlist(path: *const c_char, attr_list: *mut attrlist, attr_buf: *mut c_void, attr_buf_size: size_t, options: u_long) -> c_int => my_getattrlist {
            if path.is_null() || attr_list.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };

            log_message(&format!("interposing getattrlist({}, attr_list={:p}, attr_buf={:p}, size={}, options={:#x})",
                c_path.to_string_lossy(), attr_list, attr_buf, attr_buf_size, options));

            // For now, serialize the attrlist structure as binary data
            let attr_list_data = unsafe {
                std::slice::from_raw_parts(attr_list as *const u8, std::mem::size_of::<attrlist>())
            }.to_vec();

            let request = GetattrlistRequest {
                path: c_path.to_bytes().to_vec(),
                attr_list: attr_list_data,
                options: options as u32,
            };

            match send_request(Request::Getattrlist((b"1".to_vec(), request)), |response| match response {
                Response::Getattrlist(resp) => Some(resp.attr_data),
                _ => None,
            }) {
                Ok(attr_data) => {
                    // Copy the result data back to the caller's buffer
                    if !attr_buf.is_null() && attr_buf_size > 0 && attr_data.len() <= attr_buf_size {
                        unsafe {
                            std::ptr::copy_nonoverlapping(
                                attr_data.as_ptr(),
                                attr_buf as *mut u8,
                                attr_data.len()
                            );
                        }
                        attr_data.len() as c_int
                    } else if attr_buf_size == 0 {
                        // Size query - return the required size
                        attr_data.len() as c_int
                    } else {
                        -1 // Buffer too small or invalid
                    }
                }
                Err(_) => {
                    return redhook::real!(getattrlist)(path, attr_list, attr_buf, attr_buf_size, options);
                }
            }
        }
    }

    /// Interposed setattrlist function
    redhook::hook! {
        unsafe fn setattrlist(path: *const c_char, attr_list: *mut attrlist, attr_buf: *mut c_void, attr_buf_size: size_t, options: u_long) -> c_int => my_setattrlist {
            if path.is_null() || attr_list.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };

            log_message(&format!("interposing setattrlist({}, attr_list={:p}, attr_buf={:p}, size={}, options={:#x})",
                c_path.to_string_lossy(), attr_list, attr_buf, attr_buf_size, options));

            // Serialize the attrlist structure
            let attr_list_data = unsafe {
                std::slice::from_raw_parts(attr_list as *const u8, std::mem::size_of::<attrlist>())
            }.to_vec();

            // Serialize the attribute buffer data
            let attr_data = if !attr_buf.is_null() && attr_buf_size > 0 {
                unsafe {
                    std::slice::from_raw_parts(attr_buf as *const u8, attr_buf_size)
                }.to_vec()
            } else {
                Vec::new()
            };

            let request = SetattrlistRequest {
                path: c_path.to_bytes().to_vec(),
                attr_list: attr_list_data,
                attr_data,
                options: options as u32,
            };

            match send_request(Request::Setattrlist((b"1".to_vec(), request)), |response| match response {
                Response::Setattrlist(_) => Some(()),
                _ => None,
            }) {
                Ok(_) => 0,
                Err(_) => {
                    return redhook::real!(setattrlist)(path, attr_list, attr_buf, attr_buf_size, options);
                }
            }
        }
    }

    /// Interposed getattrlistbulk function
    redhook::hook! {
        unsafe fn getattrlistbulk(dirfd: c_int, attr_list: *mut attrlist, attr_buf: *mut c_void, attr_buf_size: size_t, options: u_int64_t) -> c_int => my_getattrlistbulk {
            if attr_list.is_null() {
                return -1;
            }

            log_message(&format!("interposing getattrlistbulk({}, attr_list={:p}, attr_buf={:p}, size={}, options={:#x})",
                dirfd, attr_list, attr_buf, attr_buf_size, options));

            // Serialize the attrlist structure
            let attr_list_data = unsafe {
                std::slice::from_raw_parts(attr_list as *const u8, std::mem::size_of::<attrlist>())
            }.to_vec();

            let request = GetattrlistbulkRequest {
                fd: dirfd as u32,
                attr_list: attr_list_data,
                options: options as u32,
            };

            match send_request(Request::Getattrlistbulk((b"1".to_vec(), request)), |response| match response {
                Response::Getattrlistbulk(resp) => Some(resp.entries),
                _ => None,
            }) {
                Ok(entries) => {
                    // For bulk operations, we need to pack multiple entries into the buffer
                    // This is a simplified implementation
                    if entries.is_empty() {
                        0 // No more entries
                    } else {
                        // Copy first entry as an example
                        if let Some(first_entry) = entries.first() {
                            if !attr_buf.is_null() && attr_buf_size >= first_entry.len() {
                                unsafe {
                                    std::ptr::copy_nonoverlapping(
                                        first_entry.as_ptr(),
                                        attr_buf as *mut u8,
                                        first_entry.len()
                                    );
                                }
                                1 // One entry returned
                            } else {
                                -1 // Buffer too small
                            }
                        } else {
                            0
                        }
                    }
                }
                Err(_) => {
                    return redhook::real!(getattrlistbulk)(dirfd, attr_list, attr_buf, attr_buf_size, options);
                }
            }
        }
    }

    /// Interposed copyfile function
    redhook::hook! {
        unsafe fn copyfile(from: *const c_char, to: *const c_char, state: copyfile_state_t, flags: copyfile_flags_t) -> c_int => my_copyfile {
            if from.is_null() || to.is_null() {
                return -1;
            }

            let c_from = unsafe { CStr::from_ptr(from) };
            let c_to = unsafe { CStr::from_ptr(to) };

            log_message(&format!("interposing copyfile({}, {}, state={:p}, flags={:#x})",
                c_from.to_string_lossy(), c_to.to_string_lossy(), state, flags));

            // Serialize copyfile state (simplified - real implementation would need to handle copyfile_state_t)
            let state_data = if !state.is_null() {
                vec![1] // Placeholder for state data
            } else {
                Vec::new()
            };

            let request = CopyfileRequest {
                src_path: c_from.to_bytes().to_vec(),
                dst_path: c_to.to_bytes().to_vec(),
                state: state_data,
                flags,
            };

            match send_request(Request::Copyfile((b"1".to_vec(), request)), |response| match response {
                Response::Copyfile(_) => Some(()),
                _ => None,
            }) {
                Ok(_) => 0,
                Err(_) => {
                    return redhook::real!(copyfile)(from, to, state, flags);
                }
            }
        }
    }

    /// Interposed fcopyfile function
    redhook::hook! {
        unsafe fn fcopyfile(from_fd: c_int, to_fd: c_int, state: copyfile_state_t, flags: copyfile_flags_t) -> c_int => my_fcopyfile {
            log_message(&format!("interposing fcopyfile({}, {}, state={:p}, flags={:#x})",
                from_fd, to_fd, state, flags));

            // Serialize copyfile state (simplified)
            let state_data = if !state.is_null() {
                vec![1] // Placeholder for state data
            } else {
                Vec::new()
            };

            let request = FcopyfileRequest {
                src_fd: from_fd as u32,
                dst_fd: to_fd as u32,
                state: state_data,
                flags,
            };

            match send_request(Request::Fcopyfile((b"1".to_vec(), request)), |response| match response {
                Response::Fcopyfile(_) => Some(()),
                _ => None,
            }) {
                Ok(_) => 0,
                Err(_) => {
                    return redhook::real!(fcopyfile)(from_fd, to_fd, state, flags);
                }
            }
        }
    }

    /// Interposed clonefile function
    redhook::hook! {
        unsafe fn clonefile(from: *const c_char, to: *const c_char, flags: c_int) -> c_int => my_clonefile {
            if from.is_null() || to.is_null() {
                return -1;
            }

            let c_from = unsafe { CStr::from_ptr(from) };
            let c_to = unsafe { CStr::from_ptr(to) };

            log_message(&format!("interposing clonefile({}, {}, flags={})",
                c_from.to_string_lossy(), c_to.to_string_lossy(), flags));

            let request = ClonefileRequest {
                src_path: c_from.to_bytes().to_vec(),
                dst_path: c_to.to_bytes().to_vec(),
                flags: flags as u32,
            };

            match send_request(Request::Clonefile((b"1".to_vec(), request)), |response| match response {
                Response::Clonefile(_) => Some(()),
                _ => None,
            }) {
                Ok(_) => 0,
                Err(_) => {
                    return redhook::real!(clonefile)(from, to, flags);
                }
            }
        }
    }

    /// Interposed fclonefileat function
    redhook::hook! {
        unsafe fn fclonefileat(from_fd: c_int, to_fd: c_int, to: *const c_char, flags: c_int) -> c_int => my_fclonefileat {
            if to.is_null() {
                return -1;
            }

            let c_to = unsafe { CStr::from_ptr(to) };

            log_message(&format!("interposing fclonefileat({}, {}, {}, flags={})",
                from_fd, to_fd, c_to.to_string_lossy(), flags));

            let request = FclonefileatRequest {
                src_dirfd: from_fd as u32,
                src_path: Vec::new(), // Empty path for fd-based operation
                dst_dirfd: to_fd as u32,
                dst_path: c_to.to_bytes().to_vec(),
                flags: flags as u32,
            };

            match send_request(Request::Fclonefileat((b"1".to_vec(), request)), |response| match response {
                Response::Fclonefileat(_) => Some(()),
                _ => None,
            }) {
                Ok(_) => 0,
                Err(_) => {
                    return redhook::real!(fclonefileat)(from_fd, to_fd, to, flags);
                }
            }
        }
    }
}
