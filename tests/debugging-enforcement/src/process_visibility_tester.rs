// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Binary for testing process visibility from within sandbox.
//!
//! This program attempts to ptrace host processes, which should fail
//! due to namespace isolation even in debug mode.

use clap::Parser;
use nix::sys::ptrace;
use nix::unistd::Pid;
use std::fs;
use std::process;
use tracing::{error, info};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Host PID to attempt to ptrace (should fail due to namespace isolation)
    #[arg(long)]
    host_pid: i32,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();
    let host_pid = Pid::from_raw(args.host_pid);

    info!(
        "Attempting to ptrace host process {} (should fail due to namespace isolation)",
        host_pid
    );

    // First, verify the process exists by checking /proc/<pid>
    let proc_path = format!("/proc/{}", host_pid);
    if !fs::metadata(&proc_path).is_ok() {
        error!("Host process {} does not exist or is not visible", host_pid);
        process::exit(3); // Process not found
    }

    info!("Host process {} exists and is visible in /proc", host_pid);

    // Now try to attach - this should fail due to namespace isolation
    match ptrace::attach(host_pid) {
        Ok(_) => {
            error!(
                "Unexpectedly succeeded in attaching to host process {} - this violates namespace isolation!",
                host_pid
            );
            process::exit(1);
        }
        Err(e) => {
            info!(
                "Failed to attach to host process {}: {} (expected due to namespace isolation)",
                host_pid, e
            );
            match e {
                nix::errno::Errno::EPERM => {
                    info!("Permission denied - host process correctly isolated");
                    Ok(())
                }
                nix::errno::Errno::ESRCH => {
                    info!("Process not found - host process correctly isolated");
                    Ok(())
                }
                _ => {
                    info!("Other error: {} - treating as successful isolation", e);
                    Ok(())
                }
            }
        }
    }
}
