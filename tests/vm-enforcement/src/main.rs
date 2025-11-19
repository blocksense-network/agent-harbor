// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! VM enforcement test orchestrator
//!
//! This binary runs VM-related tests within the sandbox environment
//! to verify that virtual machines can run properly with appropriate device access.

use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::sleep;
use tracing::{error, info};

#[derive(Debug)]
struct TestResult {
    name: String,
    passed: bool,
    output: String,
}

async fn run_test(test_name: &str, command: &mut Command) -> TestResult {
    info!("Running test: {}", test_name);

    let output = command.stdout(Stdio::piped()).stderr(Stdio::piped()).output().await;

    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let full_output = format!("stdout: {}\nstderr: {}", stdout, stderr);

            let passed = output.status.success();
            if passed {
                info!("✓ Test '{}' passed", test_name);
            } else {
                error!("✗ Test '{}' failed", test_name);
            }

            TestResult {
                name: test_name.to_string(),
                passed,
                output: full_output,
            }
        }
        Err(e) => {
            error!("✗ Test '{}' failed to execute: {}", test_name, e);
            TestResult {
                name: test_name.to_string(),
                passed: false,
                output: format!("Execution error: {}", e),
            }
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    info!("Starting VM enforcement tests");

    let mut results = Vec::new();

    // Get the directory where this executable is located
    let exe_path = std::env::current_exe()?;
    let exe_dir = exe_path.parent().unwrap_or(exe_path.as_path());
    let project_root = exe_dir
        .parent() // target/debug
        .and_then(|p| p.parent()) // target
        .unwrap_or(exe_dir); // fallback to exe dir

    let qemu_tester_path = project_root.join("target/debug/qemu_vm_tester");
    let kvm_tester_path = project_root.join("target/debug/kvm_device_tester");

    // Test 1: Verify QEMU VM runs successfully with KVM device access
    let mut qemu_kvm_test = Command::new(&qemu_tester_path);
    results.push(run_test("qemu_kvm_echo", &mut qemu_kvm_test).await);

    // Wait a bit between tests
    sleep(Duration::from_secs(2)).await;

    // Test 2: Verify KVM device access is properly managed
    let mut kvm_device_test = Command::new(&kvm_tester_path);
    results.push(run_test("kvm_device_access", &mut kvm_device_test).await);

    // Summary
    let passed_count = results.iter().filter(|r| r.passed).count();
    let total_count = results.len();

    info!("vm test results summary follows");
    for result in &results {
        if result.passed {
            info!(test = %result.name, "pass");
        } else {
            error!(test = %result.name, output = %result.output, "fail");
        }
    }
    info!(
        passed = passed_count,
        total = total_count,
        "vm tests pass count"
    );

    if passed_count == total_count {
        info!("All VM tests passed!");
        std::process::exit(0);
    } else {
        error!("Some VM tests failed!");
        std::process::exit(1);
    }
}
