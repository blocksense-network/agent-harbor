#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

extern crate ethereum_ssz as ssz;

use once_cell::sync::{Lazy, OnceCell};
use std::ffi::OsStr;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

// SSZ imports
use ssz::{Decode, Encode};
use ssz_derive::{Decode as SSZDecode, Encode as SSZEncode};

#[cfg(target_os = "macos")]
use std::os::unix::net::UnixStream;

const LOG_PREFIX: &str = "[agentfs-interpose]";
const ENV_ENABLED: &str = "AGENTFS_INTERPOSE_ENABLED";
const ENV_SOCKET: &str = "AGENTFS_INTERPOSE_SOCKET";
const ENV_ALLOWLIST: &str = "AGENTFS_INTERPOSE_ALLOWLIST";
const ENV_LOG_LEVEL: &str = "AGENTFS_INTERPOSE_LOG";
const DEFAULT_BANNER: &str = "AgentFS interpose shim loaded";

static INIT_GUARD: OnceCell<()> = OnceCell::new();
#[cfg(target_os = "macos")]
static STREAM: OnceCell<UnixStream> = OnceCell::new();

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
        log_message(&format!(
            "process '{}' not present in allowlist; skipping handshake",
            exe.display()
        ));
        return;
    }

    let Some(socket_path) = std::env::var_os(ENV_SOCKET).map(PathBuf::from) else {
        log_message(&format!("{ENV_SOCKET} not set; skipping handshake"));
        return;
    };

    match attempt_handshake(&socket_path, &exe, &allow) {
        Ok(stream) => {
            let _ = STREAM.set(stream);
        }
        Err(err) => {
            log_message(&format!("handshake failed: {err}"));
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
    let stream = StdUnixStream::connect(socket_path).map_err(|err| {
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

    let mut writer = stream.try_clone().map_err(|err| format!("failed to clone stream: {err}"))?;

    let ssz_bytes = encode_ssz(&message);
    let ssz_len = ssz_bytes.len() as u32;

    writer
        .write_all(&ssz_len.to_le_bytes())
        .and_then(|_| writer.write_all(&ssz_bytes))
        .map_err(|err| format!("failed to send handshake: {err}"))?;

    // Attempt to read acknowledgement, but tolerate timeout
    let mut reader = std::io::BufReader::new(
        stream
            .try_clone()
            .map_err(|err| format!("failed to clone stream for reading: {err}"))?,
    );
    let mut response = String::new();
    match reader.read_line(&mut response) {
        Ok(0) => {
            log_message(&format!(
                "control socket closed without acknowledgement from {}",
                socket_display
            ));
        }
        Ok(_) => {
            log_message(&format!(
                "handshake acknowledged by {socket_display}: {response}"
            ));
        }
        Err(err) => {
            log_message(&format!("failed to read handshake acknowledgement: {err}"));
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

#[cfg(all(test, target_os = "macos"))]
mod integration_tests {
    use super::*;
    use std::fs;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;
    use std::process::Command;
    use std::sync::mpsc;
    use std::thread;
    use tempfile::tempdir;

    fn build_dylib_path() -> PathBuf {
        let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".into());
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("target")
            .join(&profile);
        let direct = root.join("libagentfs_interpose_shim.dylib");
        if direct.exists() {
            return direct;
        }

        // Ensure the cdylib is built
        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
        let status = Command::new("cargo")
            .current_dir(&workspace_root)
            .args(["build", "-p", "agentfs-interpose-shim", "--lib"])
            .status()
            .expect("failed to invoke cargo build for interpose shim");
        assert!(status.success(), "cargo build for interpose shim failed");

        if direct.exists() {
            return direct;
        }

        let deps_dir = root.join("deps");
        if let Ok(entries) = std::fs::read_dir(&deps_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(OsStr::to_str) {
                    if name.starts_with("libagentfs_interpose_shim") && name.ends_with(".dylib") {
                        return path;
                    }
                }
            }
        }

        panic!(
            "unable to locate built interpose shim dylib in {:?} or {:?}",
            direct, deps_dir
        );
    }

    fn compile_helper_binary(dir: &Path) -> PathBuf {
        let source = dir.join("helper.rs");
        fs::write(&source, "fn main() { println!(\"interpose-helper\"); }").unwrap();
        let output = dir.join("helper-bin");
        let status = Command::new("rustc")
            .arg(&source)
            .arg("-C")
            .arg("opt-level=0")
            .arg("-o")
            .arg(&output)
            .status()
            .expect("failed to invoke rustc");
        assert!(status.success(), "failed to compile helper binary");
        output
    }

    #[test]
    fn shim_performs_handshake_when_allowed() {
        let _lock = ENV_GUARD.lock().unwrap();
        let dir = tempdir().unwrap();
        let socket_path = dir.path().join("agentfs.sock");
        let listener = UnixListener::bind(&socket_path).unwrap();

        let (tx, rx) = mpsc::channel();

        thread::spawn({
            let tx = tx.clone();
            move || {
                if let Ok((stream, _addr)) = listener.accept() {
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    // Read length-prefixed SSZ data
                    let mut len_buf = [0u8; 4];
                    reader.read_exact(&mut len_buf).unwrap();
                    let msg_len = u32::from_le_bytes(len_buf) as usize;

                    let mut msg_bytes = vec![0u8; msg_len];
                    reader.read_exact(&mut msg_bytes).unwrap();

                    tx.send(msg_bytes).unwrap();
                    let mut stream = stream;
                    // Send back length-prefixed SSZ success response
                    let response = HandshakeMessage::Handshake(HandshakeData {
                        version: b"1".to_vec(),
                        shim: ShimInfo {
                            name: b"agentfs-server".to_vec(),
                            crate_version: b"1.0.0".to_vec(),
                            features: vec![b"ack".to_vec()],
                        },
                        process: ProcessInfo {
                            pid: 1,
                            ppid: 0,
                            uid: 0,
                            gid: 0,
                            exe_path: b"/server".to_vec(),
                            exe_name: b"server".to_vec(),
                        },
                        allowlist: AllowlistInfo {
                            matched_entry: None,
                            configured_entries: None,
                        },
                        timestamp: b"1234567890".to_vec(),
                    });
                    let response_bytes = encode_ssz(&response);
                    let response_len = response_bytes.len() as u32;
                    let _ = stream.write_all(&response_len.to_le_bytes());
                    let _ = stream.write_all(&response_bytes);
                } else {
                    tx.send(Vec::new()).ok();
                }
            }
        });

        let helper = compile_helper_binary(dir.path());
        set_env_var(ENV_ALLOWLIST, "helper-bin");
        set_env_var(ENV_SOCKET, socket_path.to_str().unwrap());
        set_env_var(ENV_LOG_LEVEL, "1");

        let status = Command::new(&helper)
            .env("DYLD_INSERT_LIBRARIES", build_dylib_path())
            .env(ENV_SOCKET, &socket_path)
            .env(ENV_ALLOWLIST, "helper-bin")
            .env(ENV_LOG_LEVEL, "1")
            .status()
            .expect("failed to launch helper");
        assert!(status.success());

        let handshake_bytes = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(
            !handshake_bytes.is_empty(),
            "expected handshake payload (raw SSZ bytes)"
        );

        // Verify the received data can be decoded as SSZ
        let decoded: HandshakeMessage =
            decode_ssz(&handshake_bytes).expect("handshake should be valid SSZ");
        match decoded {
            HandshakeMessage::Handshake(data) => {
                assert_eq!(data.version, b"1");
                assert_eq!(data.shim.name, b"agentfs-interpose-shim");
                assert!(data.process.pid > 0);
            }
        }

        remove_env_var(ENV_ALLOWLIST);
        remove_env_var(ENV_SOCKET);
        remove_env_var(ENV_LOG_LEVEL);
    }

    #[test]
    fn shim_skips_handshake_when_not_allowed() {
        let _lock = ENV_GUARD.lock().unwrap();
        let dir = tempdir().unwrap();
        let socket_path = dir.path().join("agentfs.sock");
        let listener = UnixListener::bind(&socket_path).unwrap();
        listener.set_nonblocking(true).unwrap();

        let helper = compile_helper_binary(dir.path());

        set_env_var(ENV_ALLOWLIST, "some-other-binary");
        set_env_var(ENV_SOCKET, socket_path.to_str().unwrap());
        set_env_var(ENV_LOG_LEVEL, "1");

        let status = Command::new(&helper)
            .env("DYLD_INSERT_LIBRARIES", build_dylib_path())
            .env(ENV_SOCKET, &socket_path)
            .env(ENV_ALLOWLIST, "some-other-binary")
            .env(ENV_LOG_LEVEL, "1")
            .status()
            .expect("failed to launch helper");
        assert!(status.success());

        let mut accepted = false;
        for _ in 0..20 {
            match listener.accept() {
                Ok((_stream, _addr)) => {
                    accepted = true;
                    break;
                }
                Err(ref err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(50));
                }
                Err(err) => panic!("listener error: {err}"),
            }
        }

        assert!(
            !accepted,
            "shim should not have connected to socket when disallowed"
        );

        remove_env_var(ENV_ALLOWLIST);
        remove_env_var(ENV_SOCKET);
        remove_env_var(ENV_LOG_LEVEL);
    }
}
