// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

/// Claude Code agent implementation
use crate::session::{export_directory, import_directory};
use crate::traits::*;
use async_trait::async_trait;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::process::{Child, Command};
use tracing::{debug, info, warn};

/// Claude Code agent executor
pub struct ClaudeAgent {
    binary_path: String,
}

/// Claude Code OAuth credentials structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeCredentials {
    #[serde(rename = "claudeAiOauth")]
    pub claude_ai_oauth: ClaudeAiOauth,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeAiOauth {
    #[serde(rename = "accessToken")]
    pub access_token: String,
    #[serde(rename = "refreshToken")]
    pub refresh_token: String,
    #[serde(rename = "expiresAt")]
    pub expires_at: u64,
    pub scopes: Vec<String>,
    #[serde(rename = "subscriptionType")]
    pub subscription_type: Option<String>,
}

impl ClaudeAgent {
    pub fn new() -> Self {
        Self {
            binary_path: "claude".to_string(),
        }
    }

    /// Retrieve Claude Code credentials from platform-specific sources
    ///
    /// On macOS: executes `security find-generic-password -s "Claude Code-credentials" -w`
    /// On Linux: reads from `~/.claude/.credentials.json`
    async fn retrieve_credentials(&self) -> AgentResult<Option<String>> {
        #[cfg(target_os = "macos")]
        {
            debug!("Retrieving Claude credentials from macOS Keychain");
            match Command::new("security")
                .args([
                    "find-generic-password",
                    "-s",
                    "Claude Code-credentials",
                    "-w",
                ])
                .output()
                .await
            {
                Ok(output) => {
                    if output.status.success() {
                        let json_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
                        debug!("Retrieved credentials from Keychain");
                        Ok(Some(json_str))
                    } else {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        warn!("Failed to retrieve credentials from Keychain: {}", stderr);
                        Ok(None)
                    }
                }
                Err(e) => {
                    warn!("Failed to execute security command: {}", e);
                    Ok(None)
                }
            }
        }

        #[cfg(target_os = "linux")]
        {
            debug!("Retrieving Claude credentials from Linux credentials file");
            if let Some(home_dir) = dirs::home_dir() {
                let credentials_path = home_dir.join(".claude").join(".credentials.json");
                match tokio::fs::read_to_string(&credentials_path).await {
                    Ok(json_str) => {
                        debug!("Retrieved credentials from file: {:?}", credentials_path);
                        Ok(Some(json_str))
                    }
                    Err(e) => {
                        warn!(
                            "Failed to read credentials file {:?}: {}",
                            credentials_path, e
                        );
                        Ok(None)
                    }
                }
            } else {
                warn!("Could not determine home directory for Linux credentials");
                Ok(None)
            }
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            warn!("Credential retrieval not implemented for this platform");
            Ok(None)
        }
    }

    /// Extract access token from Claude credentials JSON
    fn extract_access_token(&self, credentials_json: &str) -> AgentResult<Option<String>> {
        match serde_json::from_str::<ClaudeCredentials>(credentials_json) {
            Ok(credentials) => {
                debug!("Successfully parsed Claude credentials");
                Ok(Some(credentials.claude_ai_oauth.access_token))
            }
            Err(e) => {
                warn!("Failed to parse Claude credentials JSON: {}", e);
                // Try to extract accessToken directly from JSON in case the structure is different
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(credentials_json) {
                    if let Some(claude_ai_oauth) = value.get("claudeAiOauth") {
                        if let Some(access_token) = claude_ai_oauth.get("accessToken") {
                            if let Some(token_str) = access_token.as_str() {
                                debug!("Extracted access token from raw JSON");
                                return Ok(Some(token_str.to_string()));
                            }
                        }
                    }
                }
                Err(AgentError::CredentialCopyFailed(format!(
                    "Failed to extract access token from credentials: {}",
                    e
                )))
            }
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
                    "projectOnboardingSeenCount": 1,
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
            "isQualifiedForDataSharing": false,
            "fallbackAvailableWarningThreshold": 0.5,
            "bypassPermissionsModeAccepted": true
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
        } else {
            // Set CLAUDE_CODE_OAUTH_TOKEN if we can retrieve credentials
            if let Some(credentials_json) = self.retrieve_credentials().await? {
                if let Some(access_token) = self.extract_access_token(&credentials_json)? {
                    debug!("Setting CLAUDE_CODE_OAUTH_TOKEN environment variable");
                    cmd.env("CLAUDE_CODE_OAUTH_TOKEN", access_token);
                }
            }
        }

        // Add additional environment variables
        for (key, value) in &config.env_vars {
            cmd.env(key, value);
        }

        // Configure stdio - inherit from parent for interactive mode, pipe for non-interactive
        use std::process::Stdio;
        if config.interactive {
            cmd.stdin(Stdio::inherit());
            cmd.stdout(Stdio::inherit());
            cmd.stderr(Stdio::inherit());
        } else {
            cmd.stdin(Stdio::piped());
            cmd.stdout(Stdio::piped());
            cmd.stderr(Stdio::piped());
        }

        // Add flags for non-interactive or permission bypassing
        if !config.interactive {
            cmd.arg("--print");
        }

        if config.unrestricted {
            // For interactive mode with unrestricted flag, bypass permission prompts
            cmd.arg("--dangerously-skip-permissions");
        }

        // Add model specification if provided
        if let Some(model) = &config.model {
            cmd.arg("--model");
            cmd.arg(model);
        }

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

    /// Platform-specific credential paths for Claude Code
    fn credential_paths(&self) -> Vec<PathBuf> {
        #[cfg(target_os = "macos")]
        {
            // On macOS, Claude Code uses Keychain for credentials, no files to copy
            vec![]
        }

        #[cfg(not(target_os = "macos"))]
        {
            // On other platforms (Linux, etc.), credentials may be stored in files
            vec![PathBuf::from(".claude/.credentials.json")]
        }
    }

    async fn get_user_api_key(&self) -> AgentResult<Option<String>> {
        // 1. Check direct environment variable (Claude-specific: ANTHROPIC_API_KEY)
        if let Ok(api_key) = std::env::var("ANTHROPIC_API_KEY") {
            if !api_key.trim().is_empty() {
                debug!("Found Anthropic API key in environment variable");
                return Ok(Some(api_key));
            }
        }

        // 2. Check environment variable pointing to file (Claude-specific: ANTHROPIC_API_KEY_FILE)
        if let Ok(file_path) = std::env::var("ANTHROPIC_API_KEY_FILE") {
            match tokio::fs::read_to_string(&file_path).await {
                Ok(content) => {
                    let api_key = content.trim().to_string();
                    if !api_key.is_empty() {
                        debug!(
                            "Found Anthropic API key in file specified by ANTHROPIC_API_KEY_FILE"
                        );
                        return Ok(Some(api_key));
                    }
                }
                Err(e) => {
                    warn!("Failed to read API key file {}: {}", file_path, e);
                }
            }
        }

        // 3. Try to extract access token from Claude Code OAuth credentials
        if let Some(credentials_json) = self.retrieve_credentials().await? {
            match serde_json::from_str::<ClaudeCredentials>(&credentials_json) {
                Ok(credentials) => {
                    debug!("Successfully parsed Claude OAuth credentials");
                    // Return the access token which can be used directly in Authorization header
                    Ok(Some(credentials.claude_ai_oauth.access_token))
                }
                Err(e) => {
                    warn!("Failed to parse Claude OAuth credentials JSON: {}", e);
                    Ok(None)
                }
            }
        } else {
            debug!("No Claude credentials found in system");
            Ok(None)
        }
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

    #[ah_test_utils::logged_test]
    fn test_parse_version() {
        let output = "claude version 2.0.15";
        let result = ClaudeAgent::parse_version(output);
        assert!(result.is_ok());
        let version = result.unwrap();
        assert_eq!(version.version, "2.0.15");
    }

    #[ah_test_utils::logged_test]
    fn test_parse_version_simple() {
        let output = "2.0.15";
        let result = ClaudeAgent::parse_version(output);
        assert!(result.is_ok());
        let version = result.unwrap();
        assert_eq!(version.version, "2.0.15");
    }

    #[ah_test_utils::logged_tokio_test]
    async fn test_agent_name() {
        let agent = ClaudeAgent::new();
        assert_eq!(agent.name(), "claude");
    }

    #[ah_test_utils::logged_tokio_test]
    async fn test_config_dir() {
        let agent = ClaudeAgent::new();
        let home = PathBuf::from("/home/user");
        let config = agent.config_dir(&home);
        assert_eq!(config, PathBuf::from("/home/user/.claude"));
    }

    #[ah_test_utils::logged_tokio_test]
    async fn test_extract_access_token() {
        let agent = ClaudeAgent::new();

        // Test with the expected JSON structure
        let credentials_json = r#"{"claudeAiOauth":{"accessToken":"test_access_token_123","refreshToken":"refresh_123","expiresAt":1792443506258,"scopes":["user:inference"],"subscriptionType":null}}"#;
        let result = agent.extract_access_token(credentials_json);
        assert!(result.is_ok());
        let token = result.unwrap();
        assert_eq!(token, Some("test_access_token_123".to_string()));

        // Test with direct accessToken field (fallback)
        let credentials_json_direct = r#"{"claudeAiOauth":{"accessToken":"direct_token_456"}}"#;
        let result_direct = agent.extract_access_token(credentials_json_direct);
        assert!(result_direct.is_ok());
        let token_direct = result_direct.unwrap();
        assert_eq!(token_direct, Some("direct_token_456".to_string()));

        // Test with invalid JSON
        let invalid_json = r#"{"invalid": "json"}"#;
        let result_invalid = agent.extract_access_token(invalid_json);
        assert!(result_invalid.is_err());
    }

    #[ah_test_utils::logged_test]
    fn test_parse_oauth_credentials() {
        let json_str = r#"{"claudeAiOauth":{"accessToken":"sk-ant-oat01-test-token","refreshToken":"refresh-token","expiresAt":1792443506258,"scopes":["user:inference"],"subscriptionType":null}}"#;

        let credentials: ClaudeCredentials = serde_json::from_str(json_str).unwrap();
        assert_eq!(
            credentials.claude_ai_oauth.access_token,
            "sk-ant-oat01-test-token"
        );
    }

    #[ah_test_utils::logged_test]
    fn test_parse_real_claude_credentials() {
        // Test with the real format from the user's example
        let json_str = r#"{"claudeAiOauth":{"accessToken":"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","refreshToken":"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","expiresAt":1792443506258,"scopes":["user:inference"],"subscriptionType":null}}"#;

        let credentials: ClaudeCredentials = serde_json::from_str(json_str).unwrap();
        assert_eq!(
            credentials.claude_ai_oauth.access_token,
            "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"
        );
    }
}
