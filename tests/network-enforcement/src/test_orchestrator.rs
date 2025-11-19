// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Network enforcement test orchestrator
//!
//! This binary runs various network-related tests within the sandbox environment
//! to verify that networking isolation and internet access work correctly.

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

    info!("Starting network enforcement tests");

    let mut results = Vec::new();

    // Test 1: Verify curl fails by default (no network access)
    let mut curl_no_network = Command::new("./curl_tester");
    curl_no_network.arg("1.1.1.1");
    results.push(run_test("curl_without_network", &mut curl_no_network).await);

    // Wait a bit between tests
    sleep(Duration::from_secs(1)).await;

    // Test 2: Port collision test - try to bind to a port that should be available
    let mut port_test = Command::new("./port_collision_tester");
    results.push(run_test("port_collision_test", &mut port_test).await);

    // Test 3: Verify basic loopback connectivity
    let mut loopback_test = Command::new("./curl_tester");
    loopback_test.arg("127.0.0.1");
    results.push(run_test("loopback_connectivity", &mut loopback_test).await);

    // Summary
    let passed_count = results.iter().filter(|r| r.passed).count();
    let total_count = results.len();

    info!("Network test results summary");
    for result in &results {
        if result.passed {
            info!(test = %result.name, "PASS");
        } else {
            error!(test = %result.name, output = %result.output, "FAIL");
        }
    }
    info!(
        passed = passed_count,
        total = total_count,
        "Aggregate results"
    );

    if passed_count == total_count {
        info!("All network tests passed!");
        std::process::exit(0);
    } else {
        error!("Some network tests failed!");
        std::process::exit(1);
    }
}
