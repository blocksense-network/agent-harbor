// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! macOS-specific test utilities for shim injection

use std::path::Path;
use std::process::{Command, Output};

/// Inject the shim using DYLD_INSERT_LIBRARIES and run a command
pub fn inject_shim_and_run(
    shim_path: &Path,
    socket_path: &str,
    command: &str,
    args: &[&str],
) -> Result<Output, Box<dyn std::error::Error>> {
    let shim_path_str = shim_path.to_string_lossy();

    eprintln!("Injecting shim: {}", shim_path_str);
    eprintln!("Socket path: {}", socket_path);
    eprintln!("Command: {} {:?}", command, args);

    // Set all environment variables directly on the command
    let output = Command::new(command)
        .args(args)
        .env("DYLD_INSERT_LIBRARIES", shim_path_str.as_ref())
        .env("AH_CMDTRACE_ENABLED", "1")
        .env("AH_CMDTRACE_SOCKET", socket_path)
        .env("AH_CMDTRACE_LOG", "1")
        .output()?;

    eprintln!("Command output: {:?}", output);

    Ok(output)
}
