// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared utilities for integration tests

use crate::AgentBinary;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Start the Rust mock LLM API server for testing
/// Note: scenario_path is required - the mock server always uses scenario files for deterministic testing
pub fn start_mock_llm_api_server(
    port: u16,
    agent_binary: &AgentBinary,
    scenario_path: &str,
) -> Result<std::process::Child, std::io::Error> {
    // Launch the Rust LLM API proxy binary
    let proxy_binary =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/llm-api-proxy");

    Command::new(proxy_binary)
        .arg("--test-server")
        .arg(port.to_string())
        .arg(scenario_path)
        .arg(agent_binary.tools_profile())
        .arg(&agent_binary.version)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
}

/// Start the Rust mock LLM API server for testing (async version)
/// Note: scenario_path is required - the mock server always uses scenario files for deterministic testing
pub async fn start_mock_llm_api_server_async(
    port: u16,
    agent_binary: &AgentBinary,
    scenario_path: &str,
) -> Result<tokio::process::Child, std::io::Error> {
    // Launch the Rust LLM API proxy binary
    let proxy_binary =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/llm-api-proxy");

    tokio::process::Command::new(proxy_binary)
        .arg("--test-server")
        .arg(port.to_string())
        .arg(scenario_path)
        .arg(agent_binary.tools_profile())
        .arg(&agent_binary.version)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
}

/// Wait for the mock server to be ready by checking the health endpoint
pub fn wait_for_mock_server(port: u16, timeout_secs: u64) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed().as_secs() < timeout_secs {
        if let Ok(resp) = ureq::get(&format!("http://127.0.0.1:{}/health", port)).call() {
            if resp.status() == 200 {
                return true;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    false
}
