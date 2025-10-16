/// Integration tests using the mock LLM API server
///
/// These tests launch real agent CLIs (claude, codex) with a mock API server
/// to verify full end-to-end functionality including credential setup and onboarding bypass.
///
/// NOTE: These tests demonstrate that agents can be launched with custom HOME directories
/// and mock API servers. The comprehensive_playbook.json uses generic tool names (write_file, read_file)
/// which don't match the actual tool names used by Claude Code (Write, Read) or Codex.
/// The tests verify that:
/// 1. Agents launch without onboarding screens
/// 2. Agents connect to the mock server successfully
/// 3. Agents process prompts and make API requests
///
/// For actual file operations, a playbook with agent-specific tool names would be needed.
// Unused imports removed - tests use direct Command execution for fine-grained control
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::{fs, thread, time};
use tempfile::TempDir;
use tokio::io::AsyncReadExt;

/// Start the mock API server in the background
fn start_mock_server(port: u16, tools_profile: &str) -> std::process::Child {
    let server_script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/tools/mock-agent/start_test_server.py");

    let playbook = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/tools/mock-agent/examples/comprehensive_playbook.json");

    Command::new("python3")
        .arg(&server_script)
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(port.to_string())
        .arg("--playbook")
        .arg(&playbook)
        .arg("--tools-profile")
        .arg(tools_profile)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to start mock server")
}

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

/// Create minimal Codex config to bypass onboarding
/// Codex setup is handled here because CodexAgent doesn't implement automatic onboarding skip yet
fn setup_codex_config(home_dir: &PathBuf) {
    let codex_dir = home_dir.join(".config").join("codex");
    fs::create_dir_all(&codex_dir).expect("Failed to create .config/codex directory");

    // Create minimal config.toml
    let config_toml = r#"
[user]
id = "test-user-integration"

[api]
# API configuration will be overridden by environment variables
"#;

    fs::write(codex_dir.join("config.toml"), config_toml)
        .expect("Failed to write config.toml");
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
async fn test_claude_with_mock_server() {
    // Check if claude is available
    if Command::new("claude").arg("--version").output().is_err() {
        eprintln!("Skipping test: claude not found in PATH");
        return;
    }

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let home_dir = temp_dir.path().join("agent_home");
    let workspace = temp_dir.path().join("workspace");
    fs::create_dir_all(&home_dir).expect("Failed to create home dir");
    fs::create_dir_all(&workspace).expect("Failed to create workspace");

    // Note: Claude onboarding skip configuration is automatically created by ClaudeAgent::launch
    // when using a custom HOME directory

    // Start mock server with claude tools profile
    let port = 18081;
    let mut server = start_mock_server(port, "claude");

    // Wait for server to be ready
    assert!(
        wait_for_server(port, 10),
        "Mock server failed to start within 10 seconds"
    );

    // Give server a moment to fully initialize
    thread::sleep(time::Duration::from_secs(1));

    // Build command manually to use --print and --dangerously-skip-permissions
    // Use a prompt that matches the comprehensive_playbook.json rules
    let mut cmd = std::process::Command::new("claude");
    cmd.arg("--dangerously-skip-permissions")
        .arg("--print")
        .arg("Create hello.py with a print statement")
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

            println!("Agent exited with status: {}", status);
            println!("Stdout: {}", stdout_str);
            println!("Stderr: {}", stderr_str);

            // Verify the agent launched successfully and connected to mock server
            // The output "Acknowledged. (no matching rule)" indicates the agent connected
            // to the mock server but the playbook didn't have a matching rule for the prompt
            assert!(
                status.success() || stdout_str.contains("Acknowledged"),
                "Agent should exit successfully or acknowledge the prompt. Status: {}, Stdout: {}, Stderr: {}",
                status,
                stdout_str,
                stderr_str
            );

            // Verify no onboarding screens were shown (which would cause different output/errors)
            assert!(
                !stdout_str.contains("Terms of Service") &&
                !stdout_str.contains("Enter your API key") &&
                !stderr_str.contains("authentication required"),
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
async fn test_codex_with_mock_server() {
    // Check if codex is available
    if Command::new("codex").arg("--version").output().is_err() {
        eprintln!("Skipping test: codex not found in PATH");
        return;
    }

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let home_dir = temp_dir.path().join("agent_home");
    let workspace = temp_dir.path().join("workspace");
    fs::create_dir_all(&home_dir).expect("Failed to create home dir");
    fs::create_dir_all(&workspace).expect("Failed to create workspace");

    // Set up Codex config to bypass onboarding
    setup_codex_config(&home_dir);

    // Start mock server with codex tools profile
    let port = 18082;
    let mut server = start_mock_server(port, "codex");

    // Wait for server to be ready
    assert!(
        wait_for_server(port, 10),
        "Mock server failed to start within 10 seconds"
    );

    // Give server a moment to fully initialize
    thread::sleep(time::Duration::from_secs(1));

    // Build command manually to use exec mode and --skip-git-repo-check
    // Use a prompt that matches the comprehensive_playbook.json rules
    let mut cmd = std::process::Command::new("codex");
    cmd.arg("exec")
        .arg("--skip-git-repo-check")
        .arg("Create hello.py with a print statement")
        .env("HOME", &home_dir)
        .env("OPENAI_API_KEY", "mock-key")
        .env("OPENAI_BASE_URL", format!("http://127.0.0.1:{}/v1", port))
        .current_dir(&workspace)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    // Set up environment variables needed for testing
    setup_test_env(&mut cmd);

    let mut child = tokio::process::Command::from(cmd)
        .spawn()
        .expect("Failed to spawn codex process");

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

            println!("Agent exited with status: {}", status);
            println!("Stdout: {}", stdout_str);
            println!("Stderr: {}", stderr_str);

            // Verify the agent launched successfully and connected to mock server
            // The output "Acknowledged. (no matching rule)" indicates the agent connected
            // to the mock server but the playbook didn't have a matching rule for the prompt
            assert!(
                status.success() || stdout_str.contains("Acknowledged"),
                "Agent should exit successfully or acknowledge the prompt. Status: {}, Stdout: {}, Stderr: {}",
                status,
                stdout_str,
                stderr_str
            );

            // Verify no onboarding screens were shown (which would cause different output/errors)
            assert!(
                !stdout_str.contains("Terms of Service") &&
                !stdout_str.contains("Enter your API key") &&
                !stderr_str.contains("authentication required"),
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
