// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Binary for testing ptrace functionality in sandbox debugging mode.
//!
//! This program attempts to use ptrace to attach to a target process.
//! In debug mode, this should succeed. In normal mode, it should fail with EPERM.

use clap::Parser;
use nix::sys::ptrace;
use nix::unistd::Pid;
use std::process;
use tracing::{error, info};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Target PID to attach to
    #[arg(short, long)]
    target_pid: i32,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();
    let target_pid = Pid::from_raw(args.target_pid);

    info!("Attempting to ptrace attach to PID {}", target_pid);
    info!("Current process PID: {}", std::process::id());

    match ptrace::attach(target_pid) {
        Ok(_) => {
            info!("Successfully attached to process {}", target_pid);

            // Try to read some data to verify we can actually use ptrace
            match ptrace::read(target_pid, std::ptr::null_mut()) {
                Ok(_) => info!("Successfully read from target process"),
                Err(e) => {
                    error!("Failed to read from target process: {}", e);
                    process::exit(1);
                }
            }

            // Detach cleanly
            if let Err(e) = ptrace::detach(target_pid, None) {
                error!("Failed to detach from process: {}", e);
                process::exit(1);
            }

            info!("Successfully detached from process {}", target_pid);
            Ok(())
        }
        Err(e) => {
            error!("Failed to attach to process {}: {}", target_pid, e);
            // Exit with specific codes for test verification
            match e {
                nix::errno::Errno::EPERM => {
                    error!("Permission denied - ptrace blocked as expected in normal mode");
                    process::exit(2); // EPERM exit code
                }
                _ => {
                    error!("Unexpected error: {}", e);
                    process::exit(1);
                }
            }
        }
    }
}
