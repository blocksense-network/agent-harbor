// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Linux-specific test utilities for shim injection

use std::path::Path;
use std::process::Output;

/// Inject the shim using LD_PRELOAD and run a command
pub async fn inject_shim_and_run(
    shim_path: &Path,
    socket_path: &str,
    command: &str,
    args: &[&str],
) -> Result<Output, Box<dyn std::error::Error>> {
    let shim_path_str = shim_path.to_string_lossy();

    // Set all environment variables directly on the command
    let output = tokio::process::Command::new(command)
        .args(args)
        .env("LD_PRELOAD", shim_path_str.as_ref())
        .env("AH_CMDTRACE_ENABLED", "1")
        .env("AH_CMDTRACE_SOCKET", socket_path)
        .env("AH_CMDTRACE_LOG", "1")
        .output()
        .await?;

    Ok(output)
}
