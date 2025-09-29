use anyhow::{bail, Context, Result};
use aw_sandbox_macos::{apply_builder, SbplBuilder};
use clap::Parser;
use libc::{chdir, chroot, execv};
use std::ffi::CString;
use std::path::PathBuf;
use std::fs;

#[derive(Parser, Debug)]
#[command(name = "aw-macos-launcher", about = "macOS sandbox launcher")] 
struct Args {
    /// Path to use as the new root (already bound to AgentFS mount)
    #[arg(long)]
    root: Option<String>,

    /// Working directory inside the new root
    #[arg(long)]
    workdir: Option<String>,

    /// Allow read under path (repeatable)
    #[arg(long = "allow-read", action = clap::ArgAction::Append)]
    allow_read: Vec<String>,

    /// Allow write under path (repeatable)
    #[arg(long = "allow-write", action = clap::ArgAction::Append)]
    allow_write: Vec<String>,

    /// Allow exec under path (repeatable)
    #[arg(long = "allow-exec", action = clap::ArgAction::Append)]
    allow_exec: Vec<String>,

    /// Allow network egress (default: off per strategy)
    #[arg(long, default_value_t = false)]
    allow_network: bool,

    /// Harden process-info and limit signals to same-group
    #[arg(long, default_value_t = false)]
    harden_process: bool,

    /// Command to exec (first is program)
    #[arg(last = true, required = true)]
    command: Vec<String>,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    // Optional chroot into AgentFS view
    if let Some(root) = args.root.as_deref() {
        let c = CString::new(root)?;
        let rc = unsafe { chroot(c.as_ptr()) };
        if rc != 0 { bail!("chroot to {} failed", root); }
    }
    if let Some(wd) = args.workdir.as_deref() {
        let c = CString::new(wd)?;
        let rc = unsafe { chdir(c.as_ptr()) };
        if rc != 0 { bail!("chdir to {} failed", wd); }
    }

    // Build and apply SBPL
    let mut builder = SbplBuilder::new();
    for p in &args.allow_read { builder = builder.allow_read_subpath(p.clone()); }
    for p in &args.allow_write { builder = builder.allow_write_subpath(p.clone()); }
    for p in &args.allow_exec { builder = builder.allow_exec_subpath(p.clone()); }
    if args.allow_network { builder = builder.allow_network(); }
    if args.harden_process { 
        builder = builder.harden_process_info().allow_signal_same_group();
    }
    apply_builder(builder).context("applying seatbelt profile failed")?;

    // Exec: resolve using PATH if needed, otherwise use provided path
    let prog_str = &args.command[0];
    let prog_c: CString;
    let resolved = if prog_str.contains('/') {
        Some(PathBuf::from(prog_str))
    } else {
        resolve_in_path(prog_str)
    };
    let path = resolved.ok_or_else(|| anyhow::anyhow!(format!("program not found in PATH: {}", prog_str)))?;
    prog_c = CString::new(path.to_string_lossy().into_owned())?;

    let c_args: Vec<CString> = args
        .command
        .iter()
        .map(|s| CString::new(s.as_str()).unwrap())
        .collect();
    // Build argv pointer array
    let mut ptrs: Vec<*const i8> = c_args.iter().map(|c| c.as_ptr()).collect();
    ptrs.push(std::ptr::null());
    let rc = unsafe { execv(prog_c.as_ptr(), ptrs.as_ptr()) };
    if rc != 0 {
        bail!("execv returned unexpectedly with rc={}", rc);
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


