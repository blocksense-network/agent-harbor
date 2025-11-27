// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only
//! Passthrough recorder rewrite helpers (Milestone 5 scaffold).
//!
//! When enabled via `AH_CMDTRACE_PASSTHROUGH=1`, exec/posix_spawn calls are
//! rewritten so the requested command is launched through
//! `ah agent record --passthrough ...` with the recorder sockets supplied by
//! environment. The rewrite is fail-open: if configuration is incomplete or a
//! string cannot be represented as a C string, we simply return `None` and let
//! the original exec proceed unmodified. A `AH_CMDTRACE_SKIP_REWRITE=1` guard
//! prevents recursive rewriting once the wrapper is active.

use std::{
    ffi::{CStr, CString},
    os::raw::c_char,
};

/// Environment keys controlling passthrough rewrite.
const ENV_PASSTHROUGH: &str = "AH_CMDTRACE_PASSTHROUGH";
const ENV_SKIP: &str = "AH_CMDTRACE_SKIP_REWRITE";
const ENV_SESSION_SOCKET: &str = "AH_CMDTRACE_SESSION_SOCKET";
const ENV_PARENT_SOCKET: &str = "AH_CMDTRACE_PARENT_SOCKET";
const ENV_AH_PATH: &str = "AH_CMDTRACE_AH_PATH";

/// Parsed configuration for passthrough rewrite.
#[derive(Debug, Clone)]
pub struct PassthroughConfig {
    pub ah_path: CString,
    pub session_socket: CString,
    pub parent_socket: Option<CString>,
}

impl PassthroughConfig {
    /// Build configuration from the current process environment.
    /// Returns `None` when passthrough is disabled or misconfigured.
    pub fn from_env() -> Option<Self> {
        if is_truthy(std::env::var(ENV_SKIP).ok().as_deref()) {
            return None;
        }
        if !is_truthy(std::env::var(ENV_PASSTHROUGH).ok().as_deref().or(Some("0"))) {
            return None;
        }

        let session = std::env::var(ENV_SESSION_SOCKET).ok()?;
        let ah = std::env::var(ENV_AH_PATH).ok().unwrap_or_else(|| "ah".into());

        let ah_path = CString::new(ah).ok()?;
        let session_socket = CString::new(session).ok()?;
        let parent_socket =
            std::env::var(ENV_PARENT_SOCKET).ok().and_then(|p| CString::new(p).ok());

        Some(Self {
            ah_path,
            session_socket,
            parent_socket,
        })
    }

    /// Build argv/envp buffers for the rewritten exec. The original argv/envp
    /// pointers are walked to construct the `--cmd` payload while preserving
    /// the existing environment (plus a skip guard to avoid recursion).
    pub unsafe fn rewrite(
        &self,
        argv: *const *mut c_char,
        envp: *const *mut c_char,
    ) -> Option<RewriteBuffers> {
        let original_args = collect_args(argv);
        if is_already_wrapped(&original_args) {
            return None;
        }

        let cmd_string = render_cmd_string(&original_args);

        let mut argv_storage = Vec::<CString>::new();
        argv_storage.push(self.ah_path.clone());
        argv_storage.push(CString::new("agent").ok()?);
        argv_storage.push(CString::new("record").ok()?);
        argv_storage.push(CString::new("--passthrough").ok()?);
        argv_storage.push(CString::new("--cmd").ok()?);
        argv_storage.push(CString::new(cmd_string).ok()?);
        argv_storage.push(CString::new("--session-socket").ok()?);
        argv_storage.push(self.session_socket.clone());
        if let Some(parent) = &self.parent_socket {
            argv_storage.push(CString::new("--parent-recorder-socket").ok()?);
            argv_storage.push(parent.clone());
        }
        // Separator to preserve the original argv (useful for debugging/telemetry).
        argv_storage.push(CString::new("--").ok()?);
        if let Some(first) = original_args.first() {
            argv_storage.push(CString::new(first.as_slice()).ok()?);
        }
        for arg in original_args.iter().skip(1) {
            argv_storage.push(CString::new(arg.as_slice()).ok()?);
        }

        let mut argv_ptrs: Vec<*const c_char> =
            argv_storage.iter().map(|c| c.as_ptr() as *const c_char).collect();
        argv_ptrs.push(std::ptr::null());

        let mut env_storage = collect_env(envp)
            .into_iter()
            .filter_map(|e| CString::new(e).ok())
            .collect::<Vec<CString>>();
        env_storage.push(CString::new(format!("{ENV_SKIP}=1")).ok()?);

        let mut env_ptrs: Vec<*const c_char> =
            env_storage.iter().map(|c| c.as_ptr() as *const c_char).collect();
        env_ptrs.push(std::ptr::null());

        Some(RewriteBuffers {
            _argv_storage: argv_storage,
            argv_ptrs,
            _env_storage: env_storage,
            env_ptrs,
        })
    }
}

/// Buffers that must outlive the exec/posix_spawn call.
pub struct RewriteBuffers {
    _argv_storage: Vec<CString>,
    argv_ptrs: Vec<*const c_char>,
    _env_storage: Vec<CString>,
    env_ptrs: Vec<*const c_char>,
}

impl RewriteBuffers {
    pub fn argv_ptr(&self) -> *const *mut c_char {
        self.argv_ptrs.as_ptr() as *const *mut c_char
    }

    pub fn env_ptr(&self) -> *const *mut c_char {
        self.env_ptrs.as_ptr() as *const *mut c_char
    }
}

fn is_truthy(value: Option<&str>) -> bool {
    matches!(
        value.map(|s| s.to_ascii_lowercase()),
        Some(ref v) if v == "1" || v == "true" || v == "yes" || v == "on"
    )
}

fn is_already_wrapped(args: &[Vec<u8>]) -> bool {
    if args.is_empty() {
        return false;
    }
    let first = String::from_utf8_lossy(&args[0]).to_ascii_lowercase();
    if !first.contains("ah") {
        return false;
    }
    args.get(1)
        .map(|a| String::from_utf8_lossy(a).to_ascii_lowercase())
        .map(|s| s.contains("agent"))
        .unwrap_or(false)
}

fn render_cmd_string(args: &[Vec<u8>]) -> String {
    if args.is_empty() {
        return "<unknown>".to_string();
    }
    args.iter().map(|a| shell_escape_bytes(a)).collect::<Vec<String>>().join(" ")
}

fn shell_escape_bytes(bytes: &[u8]) -> String {
    let s = String::from_utf8_lossy(bytes);
    if s.is_empty() {
        "''".to_string()
    } else if s
        .bytes()
        .all(|b| matches!(b, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'-' | b'.' | b'/'))
    {
        s.to_string()
    } else {
        let mut escaped = String::with_capacity(s.len() + 2);
        escaped.push('\'');
        for ch in s.chars() {
            if ch == '\'' {
                escaped.push_str("'\\''");
            } else {
                escaped.push(ch);
            }
        }
        escaped.push('\'');
        escaped
    }
}

unsafe fn collect_args(argv: *const *mut c_char) -> Vec<Vec<u8>> {
    if argv.is_null() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut i = 0isize;
    loop {
        let ptr = *argv.offset(i);
        if ptr.is_null() {
            break;
        }
        out.push(CStr::from_ptr(ptr).to_bytes().to_vec());
        i += 1;
    }
    out
}

unsafe fn collect_env(envp: *const *mut c_char) -> Vec<Vec<u8>> {
    if envp.is_null() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut i = 0isize;
    loop {
        let ptr = *envp.offset(i);
        if ptr.is_null() {
            break;
        }
        out.push(CStr::from_ptr(ptr).to_bytes().to_vec());
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_escape_handles_spaces_and_quotes() {
        assert_eq!(shell_escape_bytes(b"simple"), "simple");
        assert_eq!(shell_escape_bytes(b"a b"), "'a b'");
        assert_eq!(shell_escape_bytes(b"weird'quote"), "'weird'\\''quote'");
    }

    #[test]
    fn render_cmd_string_joins_args() {
        let cmd = render_cmd_string(&vec![
            b"python3".to_vec(),
            b"main.py".to_vec(),
            b"--flag".to_vec(),
        ]);
        assert_eq!(cmd, "python3 main.py --flag");
    }

    #[test]
    fn config_parses_truthy() {
        std::env::set_var(ENV_PASSTHROUGH, "1");
        std::env::set_var(ENV_SESSION_SOCKET, "/tmp/sock");
        std::env::set_var(ENV_PARENT_SOCKET, "/tmp/parent");
        std::env::remove_var(ENV_SKIP);
        let cfg = PassthroughConfig::from_env().expect("config");
        assert_eq!(cfg.session_socket.to_str().unwrap(), "/tmp/sock");
        assert!(cfg.parent_socket.is_some());
    }
}
