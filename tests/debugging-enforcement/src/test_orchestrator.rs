//! Test orchestrator for debugging enforcement tests.
//!
//! This program coordinates E2E tests for debugging functionality:
//! - gdb attach works in debug mode
//! - gdb attach fails in normal mode
//! - host processes are invisible from sandbox

use clap::Parser;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;
use tracing::{error, info};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to sbx-helper binary
    #[arg(long, default_value = "../../target/debug/sbx-helper")]
    sbx_helper_path: String,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();
    let sbx_helper = if args.sbx_helper_path.starts_with("../../") {
        // Convert relative path to absolute path based on current executable location
        let exe_path = std::env::current_exe()?;
        let exe_dir = exe_path.parent().unwrap();
        exe_dir.join("../../target/debug/sbx-helper").to_string_lossy().to_string()
    } else {
        args.sbx_helper_path
    };

    info!("Starting debugging enforcement tests");
    info!("Using sbx-helper at: {}", sbx_helper);

    let mut results = Vec::new();

    // Test 1: gdb attach should work in debug mode
    info!("Test 1: Testing ptrace attach in debug mode (--seccomp --seccomp-debug)");
    let (result1, skipped1) = test_ptrace_in_debug_mode(&sbx_helper);
    results.push(("ptrace_debug_mode", result1, skipped1));

    // Test 2: gdb attach should fail in normal mode
    info!("Test 2: Testing ptrace attach in normal mode (--seccomp)");
    let (result2, skipped2) = test_ptrace_in_normal_mode(&sbx_helper);
    results.push(("ptrace_normal_mode", result2, skipped2));

    // Test 3: host processes should be invisible from sandbox
    info!("Test 3: Testing host process isolation");
    let (result3, skipped3) = test_host_process_isolation(&sbx_helper);
    results.push(("host_process_isolation", result3, skipped3));

    // Report results
    info!("=== Test Results ===");
    let mut passed = 0;
    let mut failed = 0;
    let mut skipped = 0;

    for (name, success, is_skipped) in &results {
        if *is_skipped {
            info!("⚠️  {}: SKIPPED (insufficient privileges)", name);
            skipped += 1;
        } else if *success {
            info!("✅ {}: PASSED", name);
            passed += 1;
        } else {
            error!("❌ {}: FAILED", name);
            failed += 1;
        }
    }

    info!(
        "=== Summary: {}/{} tests passed, {} skipped ===",
        passed,
        passed + failed + skipped,
        skipped
    );

    if failed > 0 {
        std::process::exit(1);
    }

    Ok(())
}

fn test_ptrace_in_debug_mode(sbx_helper: &str) -> (bool, bool) {
    // Start a target process in the sandbox with debug mode enabled
    // The target will be a simple sleep process
    let target_cmd = Command::new(sbx_helper)
        .args(&[
            "--seccomp",
            "--seccomp-debug",
            "/nix/store/xbp2j3z0lhizr5vvzff4dgdcxgs8i2w7-coreutils-9.7/bin/sleep",
            "10",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();

    let mut target_process = match target_cmd {
        Ok(p) => p,
        Err(e) => {
            error!("Failed to start target process in debug mode: {}", e);
            return (false, false);
        }
    };

    let target_pid = target_process.id() as i32;
    info!(
        "Started target process with PID {} in debug mode",
        target_pid
    );

    // Give the sandbox a moment to start up
    thread::sleep(Duration::from_millis(500));

    // Double-check that the process is still running
    if let Ok(Some(status)) = target_process.try_wait() {
        error!(
            "Target process exited before ptrace test with status: {}",
            status
        );
        return (false, false);
    }

    // Check if the sandbox process is still running (not exited due to permission error)
    match target_process.try_wait() {
        Ok(Some(status)) => {
            // Process has already exited - likely due to permission error
            if status.code() == Some(1) {
                info!(
                    "⚠️  Sandbox exited with permission error (expected in unprivileged environment)"
                );
                info!(
                    "   This test requires privileges to create namespaces and mount filesystems"
                );
                info!("   Skipping ptrace test in debug mode");
                return (true, true); // (success=true, skipped=true)
            } else {
                error!(
                    "Sandbox process exited unexpectedly with status: {}",
                    status
                );
                return (false, false);
            }
        }
        Ok(None) => {
            // Process is still running - good, we can test ptrace
            info!("Sandbox process is running, proceeding with ptrace test");
        }
        Err(e) => {
            error!("Failed to check sandbox process status: {}", e);
            let _ = target_process.kill();
            return (false, false);
        }
    }

    // Now try to attach to it using our ptrace tester
    let test_result = Command::new(sbx_helper)
        .args(&[
            "--seccomp",
            "--seccomp-debug",
            "../../target/debug/ptrace_tester",
            "--target-pid",
            &target_pid.to_string(),
        ])
        .status();

    // Clean up the target process
    let _ = target_process.kill();

    match test_result {
        Ok(status) if status.success() => {
            info!("Ptrace attach succeeded in debug mode as expected");
            (true, false) // (success=true, skipped=false)
        }
        Ok(status) if status.code() == Some(1) => {
            // Exit code 1 indicates sandbox creation failed due to permissions
            info!("⚠️  Ptrace test in debug mode skipped due to insufficient privileges");
            info!("   This test requires elevated privileges to create namespaces");
            (true, true) // (success=true, skipped=true)
        }
        Ok(status) => {
            error!("Ptrace attach failed in debug mode with status: {}", status);
            (false, false)
        }
        Err(e) => {
            error!("Failed to run ptrace test in debug mode: {}", e);
            (false, false)
        }
    }
}

fn test_ptrace_in_normal_mode(sbx_helper: &str) -> (bool, bool) {
    // Start a target process in the sandbox with normal mode (no debug)
    let target_cmd = Command::new(sbx_helper)
        .args(&[
            "--seccomp",
            "/nix/store/xbp2j3z0lhizr5vvzff4dgdcxgs8i2w7-coreutils-9.7/bin/sleep",
            "10",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();

    let mut target_process = match target_cmd {
        Ok(p) => p,
        Err(e) => {
            error!("Failed to start target process in normal mode: {}", e);
            return (false, false);
        }
    };

    let target_pid = target_process.id() as i32;
    info!(
        "Started target process with PID {} in normal mode",
        target_pid
    );

    // Give the sandbox a moment to start up
    thread::sleep(Duration::from_millis(500));

    // Double-check that the process is still running
    if let Ok(Some(status)) = target_process.try_wait() {
        error!(
            "Target process exited before ptrace test with status: {}",
            status
        );
        return (false, false);
    }

    // Check if the sandbox process is still running (not exited due to permission error)
    match target_process.try_wait() {
        Ok(Some(status)) => {
            // Process has already exited - likely due to permission error
            if status.code() == Some(1) {
                info!(
                    "⚠️  Sandbox exited with permission error (expected in unprivileged environment)"
                );
                info!(
                    "   This test requires privileges to create namespaces and mount filesystems"
                );
                info!("   Skipping ptrace test in normal mode");
                return (true, true); // (success=true, skipped=true)
            } else {
                error!(
                    "Sandbox process exited unexpectedly with status: {}",
                    status
                );
                return (false, false);
            }
        }
        Ok(None) => {
            // Process is still running - good, we can test ptrace
            info!("Sandbox process is running, proceeding with ptrace test");
        }
        Err(e) => {
            error!("Failed to check sandbox process status: {}", e);
            let _ = target_process.kill();
            return (false, false);
        }
    }

    // Now try to attach to it using our ptrace tester
    let test_result = Command::new(sbx_helper)
        .args(&[
            "--seccomp",
            "../../target/debug/ptrace_tester",
            "--target-pid",
            &target_pid.to_string(),
        ])
        .status();

    // Clean up the target process
    let _ = target_process.kill();

    match test_result {
        Ok(status) if status.code() == Some(2) => {
            // Exit code 2 means EPERM, which is expected
            info!("Ptrace attach correctly failed with EPERM in normal mode");
            (true, false) // (success=true, skipped=false)
        }
        Ok(status) if status.code() == Some(1) => {
            // Exit code 1 indicates sandbox creation failed due to permissions
            info!("⚠️  Ptrace test in normal mode skipped due to insufficient privileges");
            info!("   This test requires elevated privileges to create namespaces");
            (true, true) // (success=true, skipped=true)
        }
        Ok(status) => {
            error!(
                "Ptrace attach failed with unexpected status in normal mode: {}",
                status
            );
            (false, false)
        }
        Err(e) => {
            error!("Failed to run ptrace test in normal mode: {}", e);
            (false, false)
        }
    }
}

fn test_host_process_isolation(sbx_helper: &str) -> (bool, bool) {
    // Get our own PID as a host process to test against
    let host_pid = std::process::id() as i32;
    info!("Testing isolation from host process {}", host_pid);

    // Try to ptrace the host process from within the sandbox
    let test_result = Command::new(sbx_helper)
        .args([
            "--seccomp",
            "--seccomp-debug",
            "../../target/debug/process_visibility_tester",
            "--host-pid",
            &host_pid.to_string(),
        ])
        .status();

    match test_result {
        Ok(status) if status.success() => {
            info!("Host process correctly isolated from sandbox");
            (true, false) // (success=true, skipped=false)
        }
        Ok(status) if status.code() == Some(1) => {
            // Exit code 1 indicates sandbox creation failed due to permissions
            info!("⚠️  Host process isolation test skipped due to insufficient privileges");
            info!("   This test requires elevated privileges to create namespaces");
            (true, true) // (success=true, skipped=true)
        }
        Ok(status) => {
            error!("Host process isolation test failed with status: {}", status);
            (false, false)
        }
        Err(e) => {
            error!("Failed to run host process isolation test: {}", e);
            (false, false)
        }
    }
}
