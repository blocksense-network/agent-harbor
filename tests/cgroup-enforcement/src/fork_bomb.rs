// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Fork bomb program to test PID limit enforcement
//! This program tries to create as many child processes as possible
//! to trigger cgroup PID limits.
//!
//! SAFETY: This program only performs the fork bomb attack when run inside
//! the sandbox with the SANDBOX_TEST_MODE environment variable set.

use std::process::{Command, Stdio};
use tracing::{debug, info, warn};

const SANDBOX_TEST_ENV: &str = "SANDBOX_TEST_MODE";

fn main() {
    // Initialize tracing
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        let _ = tracing_subscriber::fmt::try_init();
    });

    // Safety check: only run the attack if we're in a sandboxed test environment
    if std::env::var(SANDBOX_TEST_ENV).is_err() {
        warn!("safety: fork_bomb should only be run inside the sandbox for testing");
        warn!(
            env = SANDBOX_TEST_ENV,
            "set environment variable to enable the attack"
        );
        warn!("this prevents accidental system crashes during development");
        std::process::exit(1);
    }

    info!("running in sandbox test mode - proceeding with fork bomb attack");
    info!("starting fork bomb - attempting to create unlimited child processes");

    let mut child_count = 0;
    let mut failures = 0;

    loop {
        match Command::new(std::env::current_exe().unwrap())
            .arg("--child")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(_child) => {
                child_count += 1;
                if child_count % 10 == 0 {
                    debug!(child_count, "created child processes so far");
                }

                // Don't wait for children to avoid zombie processes
                // In a real fork bomb, we'd use fork() directly, but this simulates it
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(e) => {
                failures += 1;
                if failures % 5 == 0 {
                    warn!(failures, error = %e, "failed to create child process");
                }

                // If we get EAGAIN (resource temporarily unavailable) or other errors,
                // it might indicate we're hitting limits
                if failures > 10 {
                    info!("too many failures - likely hitting PID limits");
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
    }

    info!(child_count, failures, "fork bomb completed");
    std::process::exit(0);
}
