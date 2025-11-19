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

use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Once;

/// Ensure the shim cdylib has been built before we try to load it.
fn ensure_shim_built() {
    static BUILD_ONCE: Once = Once::new();
    BUILD_ONCE.call_once(|| {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
        let status = Command::new(cargo)
            .args(["build", "-p", "ah-command-trace-shim"])
            .status()
            .expect("failed to invoke cargo build for ah-command-trace-shim");
        if !status.success() {
            panic!("failed to build ah-command-trace-shim");
        }
    });
}

fn locate_shim_artifact(profile: &str) -> PathBuf {
    ensure_shim_built();

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join(profile)
        .join("deps");

    #[cfg(target_os = "macos")]
    const DYLIB_EXT: &str = "dylib";
    #[cfg(target_os = "linux")]
    const DYLIB_EXT: &str = "so";

    #[cfg(target_os = "macos")]
    const SHIM_PREFIX: &str = "libah_command_trace_shim";
    #[cfg(target_os = "linux")]
    const SHIM_PREFIX: &str = "libah_command_trace_shim";

    let read_dir = std::fs::read_dir(&root)
        .unwrap_or_else(|e| panic!("failed to read shim directory {:?}: {e}", root));

    let mut candidates: Vec<PathBuf> = read_dir
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(OsStr::to_str)
                .map(|name| name.starts_with(SHIM_PREFIX) && name.ends_with(DYLIB_EXT))
                .unwrap_or(false)
        })
        .collect();

    candidates.sort();

    if let Some(path) = candidates.into_iter().find(|p| p.is_file()) {
        return path;
    }

    panic!(
        "unable to locate ah-command-trace shim artifact in {:?}",
        root
    );
}

/// Find the path to the built shim library
pub fn find_shim_path() -> PathBuf {
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".into());
    locate_shim_artifact(&profile)
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
    let output = inject_shim_and_run(&shim_path, socket_path, command, args).await?;

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
    let output = tokio::process::Command::new(command)
        .args(args)
        .env("AH_CMDTRACE_ENABLED", "0")
        .output()
        .await?;

    Ok(output)
}
