// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! macOS sandbox launcher library
//!
//! Provides functionality to launch processes in a macOS sandbox using
//! Seatbelt profiles, chroot, and exec.

use ah_sandbox_macos::{SbplBuilder, apply_builder};
use anyhow::{Context, Result, bail};
use libc::{chdir, chroot, execv};
use std::ffi::CString;
use std::fs;
use std::io;
use std::path::PathBuf;

/// Configuration for launching a process in a macOS sandbox
#[derive(Debug, Clone)]
pub struct LauncherConfig {
    /// Path to use as the new root (already bound to AgentFS mount)
    pub root: Option<String>,
    /// Working directory inside the new root
    pub workdir: Option<String>,
    /// Paths to allow reading under
    pub allow_read: Vec<String>,
    /// Paths to allow writing under
    pub allow_write: Vec<String>,
    /// Paths to allow executing under
    pub allow_exec: Vec<String>,
    /// Allow network egress (default: off per strategy)
    pub allow_network: bool,
    /// Harden process-info and limit signals to same-group
    pub harden_process: bool,
    /// Command to exec (first is program)
    pub command: Vec<String>,
}

impl LauncherConfig {
    /// Create a new launcher configuration
    pub fn new(command: Vec<String>) -> Self {
        Self {
            root: None,
            workdir: None,
            allow_read: Vec::new(),
            allow_write: Vec::new(),
            allow_exec: Vec::new(),
            allow_network: false,
            harden_process: false,
            command,
        }
    }

    /// Set the root directory
    pub fn root(mut self, root: impl Into<String>) -> Self {
        self.root = Some(root.into());
        self
    }

    /// Set the working directory
    pub fn workdir(mut self, workdir: impl Into<String>) -> Self {
        self.workdir = Some(workdir.into());
        self
    }

    /// Add a path to allow reading
    pub fn allow_read(mut self, path: impl Into<String>) -> Self {
        self.allow_read.push(path.into());
        self
    }

    /// Add a path to allow writing
    pub fn allow_write(mut self, path: impl Into<String>) -> Self {
        self.allow_write.push(path.into());
        self
    }

    /// Add a path to allow executing
    pub fn allow_exec(mut self, path: impl Into<String>) -> Self {
        self.allow_exec.push(path.into());
        self
    }

    /// Allow network access
    pub fn allow_network(mut self, allow: bool) -> Self {
        self.allow_network = allow;
        self
    }

    /// Enable process hardening
    pub fn harden_process(mut self, harden: bool) -> Self {
        self.harden_process = harden;
        self
    }
}

/// Launch a process in a macOS sandbox
///
/// This function applies the sandbox configuration and executes the specified command.
/// It does not return on success (the process is replaced via exec).
pub fn launch_in_sandbox(config: LauncherConfig) -> Result<()> {
    // Optional chroot into AgentFS view
    if let Some(root) = config.root.as_deref() {
        let c = CString::new(root)?;
        let rc = unsafe { chroot(c.as_ptr()) };
        if rc != 0 {
            bail!("chroot to {} failed", root);
        }
    }
    if let Some(wd) = config.workdir.as_deref() {
        let c = CString::new(wd)?;
        let rc = unsafe { chdir(c.as_ptr()) };
        if rc != 0 {
            bail!("chdir to {} failed", wd);
        }
    }

    // Build and apply SBPL
    let mut builder = SbplBuilder::new();
    for p in &config.allow_read {
        builder = builder.allow_read_subpath(p.clone());
    }
    for p in &config.allow_write {
        builder = builder.allow_write_subpath(p.clone());
    }
    for p in &config.allow_exec {
        builder = builder.allow_exec_subpath(p.clone());
    }
    if config.allow_network {
        builder = builder.allow_network();
    }
    if config.harden_process {
        builder = builder.harden_process_info();
        // Note: allow_signal_same_group() generates invalid SBPL, skipping for now
        // builder = builder.allow_signal_same_group();
    }
    builder = builder.allow_process_fork();
    apply_builder(builder).context("applying seatbelt profile failed")?;

    // Exec: resolve using PATH if needed, otherwise use provided path
    let prog_str = &config.command[0];
    let resolved = if prog_str.contains('/') {
        Some(PathBuf::from(prog_str))
    } else {
        resolve_in_path(prog_str)
    };
    let path = resolved
        .ok_or_else(|| anyhow::anyhow!(format!("program not found in PATH: {}", prog_str)))?;
    let prog_c: CString = CString::new(path.to_string_lossy().into_owned())?;

    let c_args: Vec<CString> =
        config.command.iter().map(|s| CString::new(s.as_str()).unwrap()).collect();
    // Build argv pointer array
    let mut ptrs: Vec<*const i8> = c_args.iter().map(|c| c.as_ptr()).collect();
    ptrs.push(std::ptr::null());
    let rc = unsafe { execv(prog_c.as_ptr(), ptrs.as_ptr()) };
    if rc != 0 {
        let err = io::Error::last_os_error();
        bail!("execv returned unexpectedly with rc={} ({})", rc, err);
    }
    bail!("execv returned unexpectedly")
}

fn resolve_in_path(cmd: &str) -> Option<PathBuf> {
    if let Ok(path) = std::env::var("PATH") {
        for dir in path.split(':') {
            let candidate = PathBuf::from(dir).join(cmd);
            if let Ok(meta) = fs::metadata(&candidate) {
                if meta.is_file() && is_executable(&candidate) {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

#[cfg(unix)]
fn is_executable(path: &PathBuf) -> bool {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = fs::metadata(path) {
        let mode = meta.permissions().mode();
        mode & 0o111 != 0
    } else {
        false
    }
}
