// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Podman container tester
//!
//! This binary tests running a podman container within the sandbox environment
//! to verify that container workloads function correctly.

use std::env;
use std::process::Command;
use tracing::{error, info};

fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    info!("Testing podman container execution in sandbox");

    // Get the directory where this executable is located to find sbx-helper
    let exe_path = env::current_exe()?;
    let exe_dir = exe_path.parent().unwrap_or(exe_path.as_path());
    let project_root = exe_dir
        .parent() // target/debug
        .and_then(|p| p.parent()) // target
        .unwrap_or(exe_dir); // fallback to exe dir

    let sbx_helper_path = project_root.join("target/debug/sbx-helper");

    // Check if sbx-helper exists
    if !sbx_helper_path.exists() {
        error!("❌ sbx-helper binary not found at: {:?}", sbx_helper_path);
        println!("FAIL: sbx-helper not found - build it first with 'cargo build --bin sbx-helper'");
        std::process::exit(1);
    }

    info!("Found sbx-helper at: {:?}", sbx_helper_path);

    // Test running a simple busybox container INSIDE the sandbox
    // This requires podman to be available and the sandbox to allow container devices
    let output = Command::new(&sbx_helper_path)
        .args(&[
            "--allow-containers", // Enable container device access
            "--debug",            // Enable debug logging
            "podman",
            "run",
            "--rm",
            "docker.io/library/busybox:latest",
            "echo",
            "Hello from container in sandbox!",
        ])
        .output();

    match output {
        Ok(result) => {
            if result.status.success() {
                let stdout = String::from_utf8_lossy(&result.stdout);
                let stderr = String::from_utf8_lossy(&result.stderr);

                // Check if the expected output is in stdout
                if stdout.contains("Hello from container in sandbox!") {
                    info!("✓ Podman container executed successfully within sandbox");
                    println!("SUCCESS: Container ran inside sandbox");
                    info!("Sandbox stderr: {}", stderr);
                    std::process::exit(0);
                } else {
                    error!(
                        "✗ Container output not found. stdout: '{}', stderr: '{}'",
                        stdout, stderr
                    );
                    println!("FAIL: Expected container output not found");
                    println!("stdout: {}", stdout);
                    println!("stderr: {}", stderr);
                    std::process::exit(1);
                }
            } else {
                let stderr = String::from_utf8_lossy(&result.stderr);
                let stdout = String::from_utf8_lossy(&result.stdout);
                error!(
                    "✗ Sandbox execution failed: exit code {:?}",
                    result.status.code()
                );
                println!("FAIL: Sandbox execution failed");
                println!("stdout: {}", stdout);
                println!("stderr: {}", stderr);
                std::process::exit(1);
            }
        }
        Err(e) => {
            error!("✗ Failed to execute sbx-helper: {}", e);
            println!("FAIL: Could not execute sbx-helper: {}", e);
            std::process::exit(1);
        }
    }
}
