// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

/// Claude Code agent implementation
use crate::credentials::{claude_credential_paths, copy_files};
use crate::session::{export_directory, import_directory};
use crate::traits::*;
use async_trait::async_trait;
use regex::Regex;
use std::path::{Path, PathBuf};
use tokio::process::{Child, Command};
use tracing::{debug, info};

/// Claude Code agent executor
pub struct ClaudeAgent {
    binary_path: String,
}

impl ClaudeAgent {
    pub fn new() -> Self {
        Self {
            binary_path: "claude".to_string(),
        }
    }

    /// Parse version from `claude --version` output
    fn parse_version(output: &str) -> AgentResult<AgentVersion> {
        // Expected format: "claude version X.Y.Z" or "X.Y.Z"
        let version_regex = Regex::new(r"(\d+\.\d+\.\d+)").map_err(|e| {
            AgentError::VersionDetectionFailed(format!("Regex compilation failed: {}", e))
        })?;

        if let Some(caps) = version_regex.captures(output) {
            let version = caps[1].to_string();
            Ok(AgentVersion {
                version,
                commit: None,
                release_date: None,
            })
        } else {
            Err(AgentError::VersionDetectionFailed(format!(
                "Could not parse version from output: {}",
                output
            )))
        }
    }

    /// Set up Claude Code configuration to skip onboarding screens
    /// This creates a comprehensive .claude.json configuration file based on
    /// scripts/manual-test-agent-start.py _create_claude_config function
    async fn setup_onboarding_skip(&self, home_dir: &Path, repo_dir: &Path) -> AgentResult<()> {
        use std::time::SystemTime;

        debug!("Setting up Claude onboarding skip configuration");

        // Create .claude directory
        let claude_dir = home_dir.join(".claude");
        tokio::fs::create_dir_all(&claude_dir).await.map_err(|e| {
            AgentError::ConfigCreationFailed(format!("Failed to create .claude directory: {}", e))
        })?;

        // Get current timestamp in ISO 8601 format with microseconds
        let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_err(|e| {
            AgentError::ConfigCreationFailed(format!("Failed to get system time: {}", e))
        })?;
        let timestamp_ms = now.as_millis();
        let timestamp_iso = chrono::DateTime::from_timestamp(
            now.as_secs() as i64,
            (now.as_nanos() % 1_000_000_000) as u32,
        )
        .ok_or_else(|| AgentError::ConfigCreationFailed("Failed to create timestamp".to_string()))?
        .format("%Y-%m-%dT%H:%M:%S%.6fZ")
        .to_string();

        // Get Claude version for configuration
        let claude_version = self
            .detect_version()
            .await
            .map(|v| v.version)
            .unwrap_or_else(|_| "1.0.0".to_string());

        // Create comprehensive configuration matching manual-test-agent-start.py
        let config = serde_json::json!({
            "numStartups": 2,
            "installMethod": "unknown",
            "autoUpdates": false,
            "customApiKeyResponses": {
                "approved": ["sk-your-api-key"],
                "rejected": []
            },
            "promptQueueUseCount": 3,
            "cachedStatsigGates": {
                "tengu_disable_bypass_permissions_mode": false,
                "tengu_use_file_checkpoints": false
            },
            "firstStartTime": timestamp_iso,
            "userID": "",
            "projects": {
                repo_dir.to_string_lossy().to_string(): {
                    "allowedTools": [],
                    "history": [{
                        "display": "print the current time",
                        "pastedContents": {}
                    }],
                    "mcpContextUris": [],
                    "mcpServers": {},
                    "enabledMcpjsonServers": [],
                    "disabledMcpjsonServers": [],
                    "hasTrustDialogAccepted": true,
                    "projectOnboardingSeenCount": 0,
                    "hasClaudeMdExternalIncludesApproved": true,
                    "hasClaudeMdExternalIncludesWarningShown": true,
                    "hasCompletedProjectOnboarding": true,
                    "lastTotalWebSearchRequests": 0,
                    "lastCost": 0,
                    "lastAPIDuration": 15,
                    "lastToolDuration": 0,
                    "lastDuration": 13312,
                    "lastLinesAdded": 0,
                    "lastLinesRemoved": 0,
                    "lastTotalInputTokens": 0,
                    "lastTotalOutputTokens": 0,
                    "lastTotalCacheCreationInputTokens": 0,
                    "lastTotalCacheReadInputTokens": 0,
                    "lastSessionId": "9fdef27f-462a-4c46-ae37-7623a8b1d951"
                }
            },
            "sonnet45MigrationComplete": true,
            "changelogLastFetched": timestamp_ms,
            "shiftEnterKeyBindingInstalled": true,
            "hasCompletedOnboarding": true,
            "lastOnboardingVersion": claude_version,
            "hasOpusPlanDefault": false,
            "lastReleaseNotesSeen": claude_version,
            "hasIdeOnboardingBeenShown": {
                "cursor": true
            },
            "isQualifiedForDataSharing": false
        });

        // Write .claude.json configuration file
        let config_path = home_dir.join(".claude.json");
        let config_json = serde_json::to_string_pretty(&config).map_err(|e| {
            AgentError::ConfigCreationFailed(format!("Failed to serialize config: {}", e))
        })?;

        tokio::fs::write(&config_path, config_json).await.map_err(|e| {
            AgentError::ConfigCreationFailed(format!("Failed to write .claude.json: {}", e))
        })?;

        debug!("Created Claude configuration at {:?}", config_path);
        Ok(())
    }
}

impl Default for ClaudeAgent {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AgentExecutor for ClaudeAgent {
    fn name(&self) -> &'static str {
        "claude"
    }

    async fn detect_version(&self) -> AgentResult<AgentVersion> {
        debug!("Detecting Claude Code version");

        let output =
            Command::new(&self.binary_path).arg("--version").output().await.map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    AgentError::AgentNotFound(self.binary_path.clone())
                } else {
                    AgentError::VersionDetectionFailed(format!(
                        "Failed to execute version command: {}",
                        e
                    ))
                }
            })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Try stdout first, then stderr
        let version_output = if !stdout.trim().is_empty() {
            stdout.to_string()
        } else {
            stderr.to_string()
        };

        Self::parse_version(&version_output)
    }

    async fn prepare_launch(
        &self,
        config: AgentLaunchConfig,
    ) -> AgentResult<tokio::process::Command> {
        info!(
            "Preparing Claude Code launch with prompt: {:?}",
            config.prompt.chars().take(50).collect::<String>()
        );

        // Check if we're using a custom HOME directory
        let using_custom_home = if let Ok(system_home) = std::env::var("HOME") {
            PathBuf::from(system_home) != config.home_dir
        } else {
            true // If no system HOME, we're definitely using custom
        };

        // Set up onboarding skip configuration for custom HOME
        if using_custom_home {
            debug!(
                "Creating Claude configuration to skip onboarding in {:?}",
                config.home_dir
            );
            self.setup_onboarding_skip(&config.home_dir, &config.working_dir).await?;
        }

        // Copy credentials if requested and using custom HOME
        if config.copy_credentials && using_custom_home {
            if let Ok(system_home) = std::env::var("HOME") {
                let system_home = PathBuf::from(system_home);
                debug!(
                    "Copying credentials from {:?} to {:?}",
                    system_home, config.home_dir
                );
                self.copy_credentials(&system_home, &config.home_dir).await?;
            }
        }

        let mut cmd = tokio::process::Command::new(&self.binary_path);

        // Set custom HOME directory
        cmd.env("HOME", &config.home_dir);

        // Set current directory
        cmd.current_dir(&config.working_dir);

        // Add custom API server if specified
        if let Some(api_server) = &config.api_server {
            cmd.env("ANTHROPIC_BASE_URL", api_server);
        }

        // Add API key if specified
        if let Some(api_key) = &config.api_key {
            cmd.env("ANTHROPIC_API_KEY", api_key);
        }

        // Add additional environment variables
        for (key, value) in &config.env_vars {
            cmd.env(key, value);
        }

        // Configure stdio for piped I/O
        use std::process::Stdio;
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        // Add the prompt as argument
        if !config.prompt.is_empty() {
            cmd.arg(&config.prompt);
        }

        debug!("Claude Code command prepared successfully");
        Ok(cmd)
    }

    async fn launch(&self, config: AgentLaunchConfig) -> AgentResult<Child> {
        let mut cmd = self.prepare_launch(config).await?;
        let child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AgentError::AgentNotFound(self.binary_path.clone())
            } else {
                AgentError::ProcessSpawnFailed(e)
            }
        })?;

        debug!("Claude Code process spawned successfully");
        Ok(child)
    }

    async fn copy_credentials(&self, src_home: &Path, dst_home: &Path) -> AgentResult<()> {
        info!(
            "Copying Claude Code credentials from {:?} to {:?}",
            src_home, dst_home
        );

        let paths = claude_credential_paths();
        copy_files(&paths, src_home, dst_home).await?;

        debug!("Claude Code credentials copied successfully");
        Ok(())
    }

    async fn export_session(&self, home_dir: &Path) -> AgentResult<PathBuf> {
        let config_dir = self.config_dir(home_dir);
        let archive_path = home_dir.join("claude-session.tar.gz");

        export_directory(&config_dir, &archive_path).await?;
        Ok(archive_path)
    }

    async fn import_session(&self, session_archive: &Path, home_dir: &Path) -> AgentResult<()> {
        let config_dir = self.config_dir(home_dir);
        import_directory(session_archive, &config_dir).await
    }

    fn parse_output(&self, raw_output: &[u8]) -> AgentResult<Vec<AgentEvent>> {
        // Parse Claude Code output into normalized events
        // This is a simplified implementation - real parsing would be more complex
        let output = String::from_utf8_lossy(raw_output);
        let mut events = Vec::new();

        // Basic line-by-line parsing
        for line in output.lines() {
            if line.contains("Thinking:") {
                events.push(AgentEvent::Thinking {
                    content: line.to_string(),
                });
            } else if line.contains("Tool:") {
                events.push(AgentEvent::ToolUse {
                    tool_name: "unknown".to_string(),
                    arguments: serde_json::json!({}),
                });
            } else if !line.trim().is_empty() {
                events.push(AgentEvent::Output {
                    content: line.to_string(),
                });
            }
        }

        Ok(events)
    }

    fn config_dir(&self, home: &Path) -> PathBuf {
        home.join(".claude")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version() {
        let output = "claude version 2.0.15";
        let result = ClaudeAgent::parse_version(output);
        assert!(result.is_ok());
        let version = result.unwrap();
        assert_eq!(version.version, "2.0.15");
    }

    #[test]
    fn test_parse_version_simple() {
        let output = "2.0.15";
        let result = ClaudeAgent::parse_version(output);
        assert!(result.is_ok());
        let version = result.unwrap();
        assert_eq!(version.version, "2.0.15");
    }

    #[tokio::test]
    async fn test_agent_name() {
        let agent = ClaudeAgent::new();
        assert_eq!(agent.name(), "claude");
    }

    #[tokio::test]
    async fn test_config_dir() {
        let agent = ClaudeAgent::new();
        let home = PathBuf::from("/home/user");
        let config = agent.config_dir(&home);
        assert_eq!(config, PathBuf::from("/home/user/.claude"));
    }
}
