// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Test program for auto-propagation functionality.
//!
//! This program can spawn child processes to test whether library injection
//! is automatically propagated.

use std::env;
use std::process::{Command, Stdio};

fn log_stderr(msg: &str) {
    let bytes = msg.as_bytes();
    unsafe {
        libc::write(2, bytes.as_ptr() as *const libc::c_void, bytes.len());
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() > 1 && args[1] == "--child" {
        // Child process: just call getpid to trigger the hook
        log_stderr("[CHILD] Child process started\n");
        let _pid = unsafe { libc::getpid() };
        log_stderr("[CHILD] Child process completed\n");
        return;
    }

    // Parent process: spawn a child
    log_stderr("[PARENT] Parent process started\n");

    // Call getpid to trigger the hook in the parent
    let _pid = unsafe { libc::getpid() };

    log_stderr("[PARENT] Spawning child process...\n");

    // Spawn a child process using posix_spawn
    let child_result = Command::new(&args[0])
        .arg("--child")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status();

    match child_result {
        Ok(status) => {
            if status.success() {
                log_stderr("[PARENT] Child process exited successfully\n");
            } else {
                log_stderr("[PARENT] Child process failed\n");
                std::process::exit(1);
            }
        }
        Err(_e) => {
            log_stderr("[PARENT] Failed to spawn child\n");
            std::process::exit(1);
        }
    }

    log_stderr("[PARENT] Parent process completed\n");
}
