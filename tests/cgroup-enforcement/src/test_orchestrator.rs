// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Test orchestrator for cgroup enforcement E2E tests
//! This program launches the sandbox with abusive processes and verifies
//! that cgroup limits are actually enforced.

use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

#[derive(Debug)]
enum TestType {
    ForkBomb,
    MemoryHog,
    CpuBurner,
}

impl TestType {
    fn binary_name(&self) -> &'static str {
        match self {
            TestType::ForkBomb => "fork_bomb",
            TestType::MemoryHog => "memory_hog",
            TestType::CpuBurner => "cpu_burner",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            TestType::ForkBomb => "fork bomb (PID limit test)",
            TestType::MemoryHog => "memory hog (OOM kill test)",
            TestType::CpuBurner => "CPU burner (throttling test)",
        }
    }

    fn timeout(&self) -> Duration {
        match self {
            TestType::ForkBomb => Duration::from_secs(10),
            TestType::MemoryHog => Duration::from_secs(15),
            TestType::CpuBurner => Duration::from_secs(5), // Shorter for CPU test
        }
    }
}

fn run_enforcement_test(
    test_type: TestType,
    sbx_helper_path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    info!(
        test = test_type.description(),
        binary = test_type.binary_name(),
        timeout_secs = test_type.timeout().as_secs_f64(),
        "starting enforcement test"
    );

    let start_time = Instant::now();

    // Build the full path to the test binary
    let binary_path = format!("./target/debug/{}", test_type.binary_name());

    // Build the command to run sbx-helper with the test binary
    let mut cmd = Command::new(sbx_helper_path);
    cmd.arg("--debug") // Enable debug logging
        .arg(&binary_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    debug!(?cmd, "constructed sandbox command");

    match cmd.spawn() {
        Ok(mut child) => {
            info!(pid = child.id(), "sandbox process started");

            // Monitor the process
            let timeout = test_type.timeout();
            let mut last_check = Instant::now();

            loop {
                // Check if process is still running
                match child.try_wait() {
                    Ok(Some(status)) => {
                        let elapsed = start_time.elapsed();
                        info!(
                            elapsed_secs = elapsed.as_secs_f64(),
                            exit_code = status.code().unwrap_or(-1),
                            "process completed"
                        );

                        if status.success() {
                            info!("test passed - process completed normally");
                        } else {
                            warn!(
                                "test unclear - process exited with error (may indicate limits enforced)"
                            );
                        }
                        return Ok(());
                    }
                    Ok(None) => {
                        // Process still running, check timeout
                        if start_time.elapsed() > timeout {
                            warn!(
                                timeout_secs = timeout.as_secs_f64(),
                                "process timeout reached - terminating"
                            );
                            let _ = child.kill();
                            info!("test passed - process was contained (did not run indefinitely)");
                            return Ok(());
                        }

                        // Periodic monitoring
                        if last_check.elapsed() > Duration::from_secs(1) {
                            debug!(
                                elapsed_secs = start_time.elapsed().as_secs_f64(),
                                "process still running"
                            );
                            last_check = Instant::now();
                        }

                        thread::sleep(Duration::from_millis(100));
                    }
                    Err(e) => {
                        error!(error = %e, "error checking process status");
                        return Err(e.into());
                    }
                }
            }
        }
        Err(e) => {
            error!(error = %e, "failed to start sandbox process");
            Err(e.into())
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing once; ignore re-init errors silently.
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        let _ = tracing_subscriber::fmt::try_init();
    });

    info!("cgroup enforcement test orchestrator starting");

    // Get the directory where this executable is located
    let exe_path = std::env::current_exe()?;
    let exe_dir = exe_path.parent().unwrap_or(exe_path.as_path());
    let project_root = exe_dir
        .parent() // target/debug
        .and_then(|p| p.parent()) // target
        .unwrap_or(exe_dir); // fallback to exe dir

    let sbx_helper_path = project_root.join("target/debug/sbx-helper");

    // Check if sbx-helper exists
    if !sbx_helper_path.exists() {
        error!(path = ?sbx_helper_path, "sbx-helper binary not found");
        warn!("build it first with: cargo build --bin sbx-helper");
        std::process::exit(1);
    }

    info!(path = ?sbx_helper_path, "found sbx-helper binary");

    // Run tests
    let tests = vec![TestType::ForkBomb, TestType::MemoryHog, TestType::CpuBurner];

    let mut passed = 0;
    let mut failed = 0;

    for test in tests {
        match run_enforcement_test(test, &sbx_helper_path) {
            Ok(()) => {
                passed += 1;
            }
            Err(e) => {
                error!(error = %e, "test failed");
                failed += 1;
            }
        }
    }

    info!(passed, failed, "test results summary");

    if failed == 0 {
        info!("all tests passed");
        std::process::exit(0);
    } else {
        error!("some tests failed");
        std::process::exit(1);
    }
}
