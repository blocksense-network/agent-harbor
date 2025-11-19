// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

/// Integration tests for third-party AI coding agents with filesystem snapshots
///
/// We launch Claude Code and Codex - these are third-party AI coding agent software which connects
/// to a LLM API endpoint. The LLM API endpoint provides the thinking and decisions to execute certain tools,
/// while the agent software carries out all actions.
///
/// In the integration test, we drive the agent software with a custom LLM API server which executes a
/// pre-defined scenario, always instructing the agent with deterministic messages and tool use instructions.
///
/// The goal of the integration test is to verify that:
/// 1. Our integration of the third-party software is correct
/// 2. We are executing the software with the right configuration
/// 3. Our post-tool use hooks are enabled that create filesystem snapshots (see @Agent-Time-Travel.md and @FS-Snapshots-Overview.md)
/// 4. Our LLM proxy is implementing the LLM APIs correctly
/// 5. Our tools profiles are correct (mapping generic tool names to agent-specific implementations)
///
/// These tests launch real agent CLIs (claude, codex) with a mock API server
/// to verify full end-to-end functionality including credential setup and onboarding bypass.
///
/// NOTE: These tests demonstrate that agents can be launched with custom HOME directories
/// and mock API servers.
/// The tests verify that:
/// 1. Agents launch without onboarding screens
/// 2. Agents connect to the mock server successfully
/// 3. Agents process prompts and make API requests
///
/// For actual file operations, scenarios with agent-specific tool names are defined in @Scenario-Format.md.
// Unused imports removed - tests use direct Command execution for fine-grained control
use ah_agents::test_utils::start_mock_llm_api_server;
use ah_agents::{AgentLaunchConfig, agent_by_name};
use ah_core::agent_binary::AgentBinary;
use ah_domain_types::AgentSoftware;
use std::ffi::OsString;
use std::path::Path;
use std::{fs, thread, time};
use tempfile::TempDir;

use tokio::io::AsyncReadExt;

/// Wait for the server to be ready
fn wait_for_server(port: u16, timeout_secs: u64) -> bool {
    let start = time::Instant::now();
    while start.elapsed().as_secs() < timeout_secs {
        if let Ok(resp) = ureq::get(&format!("http://127.0.0.1:{}/health", port)).call() {
            if resp.status() == 200 {
                return true;
            }
        }
        thread::sleep(time::Duration::from_millis(100));
    }
    false
}

fn find_free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("Failed to bind to an ephemeral port")
        .local_addr()
        .expect("Failed to read bound address")
        .port()
}

/// Create minimal Codex config to bypass onboarding
/// Codex setup is handled here because CodexAgent doesn't implement automatic onboarding skip yet
fn setup_codex_config(home_dir: &Path, api_key: &str) {
    let codex_dir = home_dir.join(".config").join("codex");
    fs::create_dir_all(&codex_dir).expect("Failed to create .config/codex directory");

    // Create minimal config.toml
    let config_toml = r#"
[user]
id = "test-user-integration"

[api]
# API configuration will be overridden by environment variables
wire_api = "chat"

[model_providers.openai]
name = "OpenAI"
base_url = "https://api.openai.com/v1"
env_key = "OPENAI_API_KEY"
wire_api = "chat"
"#;

    fs::write(codex_dir.join("config.toml"), config_toml).expect("Failed to write config.toml");

    let codex_home_dir = home_dir.join(".codex");
    fs::create_dir_all(&codex_home_dir).expect("Failed to create .codex directory");
    let auth_json = format!("{{\n  \"OPENAI_API_KEY\": \"{}\"\n}}\n", api_key);
    fs::write(codex_home_dir.join("auth.json"), auth_json).expect("Failed to write auth.json");
}

/// Set up environment variables needed for testing
fn setup_test_env(cmd: &mut std::process::Command) {
    // Forward environment variables needed for testing
    if let Ok(tui_testing_uri) = std::env::var("TUI_TESTING_URI") {
        cmd.env("TUI_TESTING_URI", tui_testing_uri);
    }
    if let Ok(ah_home) = std::env::var("AH_HOME") {
        cmd.env("AH_HOME", ah_home);
    }
    // Set git environment variables for consistent behavior
    cmd.env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", "echo")
        .env("SSH_ASKPASS", "echo");
}

#[cfg(feature = "claude")]
#[tokio::test]
#[ignore]
async fn test_claude_with_mock_server() {
    // Check if claude is available
    let agent_binary = AgentBinary::from_agent_type(&AgentSoftware::Claude)
        .expect("Claude binary not found in PATH");

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let home_dir = temp_dir.path().join("agent_home");
    let workspace = temp_dir.path().join("workspace");
    fs::create_dir_all(&home_dir).expect("Failed to create home dir");
    fs::create_dir_all(&workspace).expect("Failed to create workspace");

    // Note: Claude onboarding skip configuration is automatically created by ClaudeAgent::launch
    // when using a custom HOME directory

    // Start mock server with claude tools profile and basic scenario
    let port = 18081;
    let scenario_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/tools/mock-agent/scenarios/basic_timeline_scenario.yaml"
    );
    #[allow(clippy::zombie_processes)]
    let mut server = start_mock_llm_api_server(port, &agent_binary, scenario_path)
        .expect("Failed to start mock server");

    // Wait for server to be ready
    assert!(
        wait_for_server(port, 10),
        "Mock server failed to start within 10 seconds"
    );

    // Give server a moment to fully initialize
    thread::sleep(time::Duration::from_secs(1));

    // Build command manually to use --dangerously-skip-permissions
    // The prompt doesn't matter since scenarios provide deterministic responses
    let mut cmd = std::process::Command::new("claude");
    cmd.arg("--dangerously-skip-permissions")
        .arg("Create a test file")
        .env("HOME", &home_dir)
        .env("ANTHROPIC_API_KEY", "mock-key")
        .env("ANTHROPIC_BASE_URL", format!("http://127.0.0.1:{}", port))
        .current_dir(&workspace)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    // Set up environment variables needed for testing
    setup_test_env(&mut cmd);

    let mut child = tokio::process::Command::from(cmd)
        .spawn()
        .expect("Failed to spawn claude process");

    // Set up a timeout using tokio::time
    let timeout_duration = time::Duration::from_secs(30);
    let start = time::Instant::now();

    // Read output with timeout
    let mut stdout_data = Vec::new();
    let mut stderr_data = Vec::new();

    // Use tokio::select to implement timeout
    let result = tokio::time::timeout(timeout_duration, async {
        if let Some(stdout) = child.stdout.as_mut() {
            let _ = stdout.read_to_end(&mut stdout_data).await;
        }
        if let Some(stderr) = child.stderr.as_mut() {
            let _ = stderr.read_to_end(&mut stderr_data).await;
        }
        child.wait().await
    })
    .await;

    // Kill the child process if it's still running
    let _ = child.kill().await;

    // Stop server
    let _ = server.kill();

    // Check result
    match result {
        Ok(Ok(status)) => {
            let stdout_str = String::from_utf8_lossy(&stdout_data);
            let stderr_str = String::from_utf8_lossy(&stderr_data);

            tracing::info!(status = %status, "agent exited");
            tracing::debug!(stdout = %stdout_str, "agent stdout");
            tracing::debug!(stderr = %stderr_str, "agent stderr");

            // Verify the agent launched successfully and connected to mock server
            // The scenario should create a test.txt file
            assert!(
                status.success(),
                "Agent should exit successfully. Status: {}, Stdout: {}, Stderr: {}",
                status,
                stdout_str,
                stderr_str
            );

            // Verify the agent received a response (mock server provides deterministic responses)
            // The exact output depends on how the agent formats the response, but it should not be empty
            assert!(
                !stdout_str.trim().is_empty() || !stderr_str.trim().is_empty(),
                "Agent should receive some response from mock server. Stdout: '{}', Stderr: '{}'",
                stdout_str,
                stderr_str
            );

            // Verify no onboarding screens were shown (which would cause different output/errors)
            assert!(
                !stdout_str.contains("Terms of Service")
                    && !stdout_str.contains("Enter your API key")
                    && !stderr_str.contains("authentication required"),
                "Agent should not show onboarding screens. Stdout: {}, Stderr: {}",
                stdout_str,
                stderr_str
            );
        }
        Ok(Err(e)) => {
            panic!("Agent process failed: {}", e);
        }
        Err(_) => {
            panic!(
                "Agent process timed out after {} seconds",
                start.elapsed().as_secs()
            );
        }
    }
}

#[cfg(feature = "codex")]
#[tokio::test]
#[ignore]
async fn test_codex_with_mock_server() {
    let overall_timeout = time::Duration::from_secs(60);

    tokio::time::timeout(overall_timeout, async {
        // Check if codex is available
        let agent_binary = AgentBinary::from_agent_type(&AgentSoftware::Codex)
            .expect("Codex binary not found in PATH");

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let home_dir = temp_dir.path().join("agent_home");
        let workspace = temp_dir.path().join("workspace");
        fs::create_dir_all(&home_dir).expect("Failed to create home dir");
        fs::create_dir_all(&workspace).expect("Failed to create workspace");

        // Set up Codex config to bypass onboarding
        let mock_api_key = "mock-key";
        setup_codex_config(&home_dir, mock_api_key);

        // Start mock server with codex tools profile and basic scenario
        let port = find_free_port();
        let scenario_path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/tools/mock-agent/scenarios/basic_timeline_scenario.yaml"
        );
        #[allow(clippy::zombie_processes)]
        let mut server = start_mock_llm_api_server(port, &agent_binary, scenario_path)
            .expect("Failed to start mock server");

        // Wait for server to be ready
        assert!(
            wait_for_server(port, 10),
            "Mock server failed to start within 10 seconds"
        );

        // Give server a moment to fully initialize and print any startup errors
        thread::sleep(time::Duration::from_secs(1));

        // Stream server stderr asynchronously to avoid blocking the test while
        // still surfacing any startup diagnostics from the mock server.
        if let Some(stderr) = server.stderr.take() {
            std::thread::spawn(move || {
                use std::io::Read;
                let mut stderr = stderr;
                let mut buf = Vec::new();
                if stderr.read_to_end(&mut buf).is_ok() && !buf.is_empty() {
                    tracing::debug!(output = %String::from_utf8_lossy(&buf), "server startup output");
                }
            });
        }

        // Get the Codex agent executor and prepare the launch command
        let agent = agent_by_name("codex").expect("Codex agent not available");

        let launch_config = AgentLaunchConfig::new(&home_dir).prompt("Create a test file")
            .interactive(false)
            .working_dir(workspace.clone())
            .env("HOME", home_dir.to_string_lossy().to_string())
            .llm_api(format!("http://127.0.0.1:{}/v1", port))
            .llm_api_key(mock_api_key)
            .copy_credentials(false)
            .unrestricted(true)
            .model("gpt-4o-mini");

        let mut cmd = agent
            .prepare_launch(launch_config)
            .await
            .expect("Failed to prepare Codex launch command");

        // Set up environment variables needed for testing
        setup_test_env(cmd.as_std_mut());

        // Capture the final command for debugging when tests fail locally.
        let std_cmd = cmd.as_std_mut();
        let program = std_cmd.get_program().to_owned();
        let args: Vec<OsString> = std_cmd.get_args().map(|a| a.to_os_string()).collect();
        tracing::info!(?program, ?args, "launching codex agent");

        // `prepare_launch` sets stdin/stdout/stderr to piped already; no extra work needed here.

        let mut child = cmd.spawn().expect("Failed to spawn codex process");

        // Set up a timeout
        let timeout_duration = time::Duration::from_secs(30);
        let start = time::Instant::now();

        // Read output with timeout
        let mut stdout_data = Vec::new();
        let mut stderr_data = Vec::new();

        // Use tokio::select to implement timeout
        let result = tokio::time::timeout(timeout_duration, async {
            if let Some(stdout) = child.stdout.as_mut() {
                let _ = stdout.read_to_end(&mut stdout_data).await;
            }
            if let Some(stderr) = child.stderr.as_mut() {
                let _ = stderr.read_to_end(&mut stderr_data).await;
            }
            child.wait().await
        })
        .await;

        // Kill the child process if it's still running
        let _ = child.kill().await;

        // Stop server
        let _ = server.kill();

        // Check result
        match result {
            Ok(Ok(status)) => {
                let stdout_str = String::from_utf8_lossy(&stdout_data);
                let stderr_str = String::from_utf8_lossy(&stderr_data);

                tracing::info!(status = %status, "agent exited");
                tracing::debug!(stdout = %stdout_str, "agent stdout");
                tracing::debug!(stderr = %stderr_str, "agent stderr");

                // Verify the agent launched successfully and connected to mock server
                // The scenario should create a test.txt file
                assert!(
                    status.success(),
                    "Agent should exit successfully. Status: {}, Stdout: {}, Stderr: {}",
                    status,
                    stdout_str,
                    stderr_str
                );

                // Verify the agent received a response (mock server provides deterministic responses)
                // The exact output depends on how the agent formats the response, but it should not be empty
                assert!(
                    !stdout_str.trim().is_empty() || !stderr_str.trim().is_empty(),
                    "Agent should receive some response from mock server. Stdout: '{}', Stderr: '{}'",
                    stdout_str,
                    stderr_str
                );

                // Verify no onboarding screens were shown (which would cause different output/errors)
                assert!(
                    !stdout_str.contains("Terms of Service")
                        && !stdout_str.contains("Enter your API key")
                        && !stderr_str.contains("authentication required"),
                    "Agent should not show onboarding screens. Stdout: {}, Stderr: {}",
                    stdout_str,
                    stderr_str
                );
            }
            Ok(Err(e)) => {
                panic!("Agent process failed: {}", e);
            }
            Err(_) => {
                panic!(
                    "Agent process timed out after {} seconds",
                    start.elapsed().as_secs()
                );
            }
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!(
            "test_codex_with_mock_server exceeded overall timeout of {} seconds",
            overall_timeout.as_secs()
        )
    });
}
