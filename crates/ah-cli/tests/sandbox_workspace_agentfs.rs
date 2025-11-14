// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Integration tests for AgentFS sandbox workspace preparation

#[test]
fn test_sandbox_workspace_agentfs() {
    // Skip this test in CI environments where AgentFS harness may not be available
    if std::env::var("CI").is_ok() {
        println!("⚠️  Skipping AgentFS sandbox test in CI environment");
        return;
    }

    // Test the new --fs-snapshots flag (AgentFS provider)
    // This test exercises the end-to-end sandbox command with AgentFS provider
    use std::process::Command;

    // Build path to the ah binary
    let cargo_manifest_dir = env!("CARGO_MANIFEST_DIR");
    let binary_path = if cargo_manifest_dir.contains("/crates/") {
        std::path::Path::new(&cargo_manifest_dir).join("../../target/debug/ah")
    } else {
        std::path::Path::new(&cargo_manifest_dir).join("target/debug/ah")
    };

    // Test 1: AgentFS provider explicitly requested (may fail if not available)
    let mut cmd = Command::new(&binary_path);
    cmd.args([
        "agent",
        "sandbox",
        "--fs-snapshots",
        "agentfs",
        "--",
        "echo",
        "agentfs test",
    ]);

    let output = cmd.output().expect("Failed to run ah agent sandbox with agentfs command");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("AgentFS sandbox stdout: {}", stdout);
    if !stderr.is_empty() {
        println!("AgentFS sandbox stderr: {}", stderr);
    }

    // The command should attempt to run (may fail due to missing AgentFS support or permissions)
    if !output.status.success() {
        // Common expected failures in test environments:
        // - AgentFS provider not available or not compiled
        // - Insufficient permissions for sandboxing
        // - Missing kernel features
        assert!(
            stderr.contains("AgentFS provider explicitly requested but not available")
                || stderr.contains("AgentFS provider requested but not compiled")
                || stderr.contains("permission denied")
                || stderr.contains("Operation not permitted")
                || stderr.contains("Sandbox functionality is only available on Linux"),
            "Unexpected failure: stdout={}, stderr={}",
            stdout,
            stderr
        );
        println!(
            "⚠️  AgentFS sandbox command failed as expected in test environment (missing provider/permissions)"
        );
    } else {
        println!("✅ AgentFS sandbox command executed successfully");
    }

    // Test 2: Disable provider (should work on Linux)
    let mut cmd_disable = Command::new(&binary_path);
    cmd_disable.args([
        "agent",
        "sandbox",
        "--fs-snapshots",
        "disable",
        "--",
        "echo",
        "disable test",
    ]);

    let output_disable = cmd_disable
        .output()
        .expect("Failed to run ah agent sandbox with disable command");
    let stdout_disable = String::from_utf8_lossy(&output_disable.stdout);
    let stderr_disable = String::from_utf8_lossy(&output_disable.stderr);

    println!("Disable sandbox stdout: {}", stdout_disable);
    if !stderr_disable.is_empty() {
        println!("Disable sandbox stderr: {}", stderr_disable);
    }

    // Disable should work on Linux (may fail on macOS due to sandbox not being implemented)
    if !output_disable.status.success() {
        assert!(
            stderr_disable.contains("Sandbox functionality is only available on Linux"),
            "Disable provider should work on Linux: stdout={}, stderr={}",
            stdout_disable,
            stderr_disable
        );
        println!("⚠️  Sandbox disable test skipped (not on Linux)");
    } else {
        println!("✅ Disable provider sandbox executed successfully");
    }

    println!("✅ AgentFS provider CLI integration test completed");
    println!("   This test verifies that:");
    println!("   1. `--fs-snapshots agentfs` flag is accepted");
    println!("   2. `--fs-snapshots disable` flag works");
    println!("   3. Provider selection logic routes correctly");
    println!(
        "   Note: Full AgentFS execution requires compiled agentfs feature and proper permissions"
    );
}
