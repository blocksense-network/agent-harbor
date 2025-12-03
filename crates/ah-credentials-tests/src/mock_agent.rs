// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Mock agent implementations for testing credential acquisition

use ah_credentials::types::AgentType;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;

/// Mock agent runner for testing credential acquisition pipelines
pub struct MockAgentRunner {
    pub agent_type: AgentType,
    pub executable_path: PathBuf,
    pub expected_credentials: serde_json::Value,
}

impl MockAgentRunner {
    /// Create a new mock agent runner
    pub fn new(
        agent_type: AgentType,
        executable_path: PathBuf,
        expected_credentials: serde_json::Value,
    ) -> Self {
        Self {
            agent_type,
            executable_path,
            expected_credentials,
        }
    }

    /// Run the mock agent and simulate credential acquisition
    pub async fn run(&self, home_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
        // Set up environment
        let mut env = std::env::vars().collect::<HashMap<_, _>>();
        env.insert("HOME".to_string(), home_dir.to_string_lossy().to_string());
        env.insert(
            "MOCK_AGENT_TYPE".to_string(),
            format!("{:?}", self.agent_type),
        );

        // Run the mock agent
        let mut child = Command::new(&self.executable_path)
            .envs(&env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // Simulate user interaction by writing to stdin
        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            stdin.write_all(b"mock-user-input\n").await?;
            stdin.write_all(b"mock-password\n").await?;
            stdin.write_all(b"yes\n").await?; // Confirm
        }

        // Wait for completion
        let status = child.wait().await?;
        if !status.success() {
            return Err(format!("Mock agent exited with status: {}", status).into());
        }

        Ok(())
    }

    /// Create expected credential files in the home directory
    pub async fn setup_expected_files(
        &self,
        home_dir: &Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match self.agent_type {
            AgentType::Codex => {
                let codex_dir = home_dir.join(".codex");
                tokio::fs::create_dir_all(&codex_dir).await?;
                let auth_file = codex_dir.join("auth.json");
                tokio::fs::write(
                    &auth_file,
                    serde_json::to_string_pretty(&self.expected_credentials)?,
                )
                .await?;
            }
            AgentType::Claude => {
                let config_dir = home_dir.join(".config").join("claude");
                tokio::fs::create_dir_all(&config_dir).await?;
                let auth_file = config_dir.join("auth.json");
                tokio::fs::write(
                    &auth_file,
                    serde_json::to_string_pretty(&self.expected_credentials)?,
                )
                .await?;
            }
            AgentType::Cursor => {
                let config_dir = home_dir.join(".config").join("cursor");
                tokio::fs::create_dir_all(&config_dir).await?;
                let auth_file = config_dir.join("auth.json");
                tokio::fs::write(
                    &auth_file,
                    serde_json::to_string_pretty(&self.expected_credentials)?,
                )
                .await?;
            }
        }

        Ok(())
    }
}

/// Create a simple mock agent script for testing
pub async fn create_mock_agent_script(
    agent_type: &AgentType,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let script_content = format!(
        r#"#!/bin/bash
# Mock agent script for testing

echo "Mock {} agent starting..." >&2
echo "Please enter your credentials:" >&2

# Read mock input
read username
read password
read confirm

echo "Credentials received. Authenticating..." >&2
sleep 1
echo "Authentication successful!" >&2

exit 0
"#,
        format!("{:?}", agent_type).to_lowercase()
    );

    let temp_dir = tempfile::tempdir()?;
    let script_path = temp_dir.path().join(format!(
        "mock-{}-agent",
        format!("{:?}", agent_type).to_lowercase()
    ));

    tokio::fs::write(&script_path, script_content).await?;

    // Make executable
    use std::os::unix::fs::PermissionsExt;
    let mut perms = tokio::fs::metadata(&script_path).await?.permissions();
    perms.set_mode(0o755);
    tokio::fs::set_permissions(&script_path, perms).await?;

    // Note: We need to keep temp_dir alive, so we return the path
    // In real usage, the caller should keep the TempDir alive
    Ok(script_path)
}
