// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! End-to-end tests for the command trace shim
//!
//! This crate provides utilities for testing the shim in real interposition scenarios.
//! It includes platform-specific loaders and test helpers.

#[cfg(target_os = "macos")]
pub mod platform;

#[cfg(target_os = "linux")]
pub mod platform;

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use platform::*;

use std::path::PathBuf;
use std::process::Command;

/// Find the path to the built shim library
pub fn find_shim_path() -> PathBuf {
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".into());
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join(&profile);

    #[cfg(target_os = "macos")]
    let shim_name = "libah_command_trace_shim.dylib";

    #[cfg(target_os = "linux")]
    let shim_name = "libah_command_trace_shim.so";

    let shim_path = root.join(shim_name);
    assert!(
        shim_path.exists(),
        "Shim library not found at {}. Make sure to build the ah-command-trace-shim crate.",
        shim_path.display()
    );

    shim_path
}

/// Execute a test scenario using the helper binary with shim injection
///
/// This function handles the common setup and execution of test scenarios:
/// - Sets up environment variables for interposition
/// - Runs the helper binary with the specified command and arguments
/// - Returns the exit status (success/failure)
/// - Cleans up environment variables
pub async fn execute_test_scenario(
    socket_path: &str,
    command: &str,
    args: &[&str],
) -> Result<std::process::Output, Box<dyn std::error::Error>> {
    let shim_path = find_shim_path();

    // Make sure the test process doesn't try to handshake
    std::env::remove_var("AH_CMDTRACE_SOCKET");
    std::env::remove_var("AH_CMDTRACE_LOG");

    // Execute the helper binary with shim injection
    let output = inject_shim_and_run(&shim_path, socket_path, command, args)?;

    // Clean up environment variables from test process
    std::env::remove_var("AH_CMDTRACE_SOCKET");
    std::env::remove_var("AH_CMDTRACE_LOG");

    Ok(output)
}

/// Execute a test scenario with shim disabled
pub async fn execute_test_scenario_disabled(
    command: &str,
    args: &[&str],
) -> Result<std::process::Output, Box<dyn std::error::Error>> {
    // Run without shim injection, explicitly disabling it
    let output = Command::new(command).args(args).env("AH_CMDTRACE_ENABLED", "0").output()?;

    Ok(output)
}
