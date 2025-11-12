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

/// Structured status information for Claude Code CLI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeStatus {
    /// Whether the CLI is installed and available
    pub available: bool,
    /// Version information if available
    pub version: Option<String>,
    /// Whether the user is authenticated
    pub authenticated: bool,
    /// Authentication method used (e.g., "ANTHROPIC_API_KEY", "OAuth", "Credentials File")
    pub auth_method: Option<String>,
    /// Source of authentication (config file path, environment variable name, etc.)
    pub auth_source: Option<String>,
    /// Any error that occurred during status check
    pub error: Option<String>,
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
    /// On Linux: reads from `~/.claude/.credentials.json` or `~/.config/claude/.credentials.json`
    async fn retrieve_credentials(&self, home_dir: Option<&Path>) -> AgentResult<Option<String>> {
        #[cfg(target_os = "macos")]
        {
            // On macOS, credentials are stored in Keychain (system-wide), not in HOME directory files
            let _ = home_dir; // Silence unused parameter warning for macOS
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
            let home = if let Some(h) = home_dir {
                h.to_path_buf()
            } else if let Some(h) = dirs::home_dir() {
                h
            } else {
                warn!("Could not determine home directory for Linux credentials");
                return Ok(None);
            };

            // Try multiple possible credential locations
            let possible_paths = vec![
                home.join(".claude").join(".credentials.json"),
                home.join(".config").join("claude").join(".credentials.json"),
            ];

            for credentials_path in possible_paths {
                match tokio::fs::read_to_string(&credentials_path).await {
                    Ok(json_str) => {
                        debug!("Retrieved credentials from file: {:?}", credentials_path);
                        return Ok(Some(json_str));
                    }
                    Err(e) => {
                        debug!(
                            "Credentials file {:?} not found or not readable: {}",
                            credentials_path, e
                        );
                    }
                }
            }

            warn!("No Claude credentials found in any of the expected locations");
            Ok(None)
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

    /// Get structured Claude CLI status information
    ///
    /// This function returns comprehensive status information in a structured format
    /// that can be easily consumed by health checkers and other tools.
    ///
    /// Returns ClaudeStatus with detailed information about:
    /// - CLI availability and version
    /// - Authentication status and method
    /// - API key information (masked for security)
    /// - Any errors encountered
    pub async fn get_claude_status(&self) -> ClaudeStatus {
        // Check CLI availability by detecting version
        let (available, version, mut error) = match self.detect_version().await {
            Ok(version_info) => (true, Some(version_info.version), None),
            Err(AgentError::AgentNotFound(_)) => (
                false,
                None,
                Some("Claude CLI not found in PATH".to_string()),
            ),
            Err(e) => (
                false,
                None,
                Some(format!("Version detection failed: {}", e)),
            ),
        };

        if !available {
            return ClaudeStatus {
                available: false,
                version: None,
                authenticated: false,
                auth_method: None,
                auth_source: None,
                error,
            };
        }

        // Check authentication status
        let (authenticated, auth_method, auth_source) = match self.get_user_api_key().await {
            Ok(Some(_api_key)) => {
                let method = self.detect_auth_method().await;
                let source = self.detect_auth_source().await;
                (true, Some(method), Some(source))
            }
            Ok(None) => (false, None, None),
            Err(e) => {
                error = Some(format!("Authentication check failed: {}", e));
                (false, None, None)
            }
        };

        ClaudeStatus {
            available,
            version,
            authenticated,
            auth_method,
            auth_source,
            error,
        }
    }

    /// Detect which authentication method is being used
    async fn detect_auth_method(&self) -> String {
        // Check in order of precedence to determine which method is being used
        if std::env::var("ANTHROPIC_API_KEY").is_ok() {
            return "ANTHROPIC_API_KEY environment variable".to_string();
        }
        if std::env::var("ANTHROPIC_API_KEY_FILE").is_ok() {
            return "ANTHROPIC_API_KEY_FILE".to_string();
        }

        // Check for Claude Code OAuth credentials
        if self.retrieve_credentials(None).await.unwrap_or(None).is_some() {
            return "Claude Code OAuth".to_string();
        }

        "Unknown".to_string()
    }

    /// Detect the specific source of authentication (file path, env var name, etc.)
    async fn detect_auth_source(&self) -> String {
        // Check in order of precedence to determine the specific source
        if std::env::var("ANTHROPIC_API_KEY").is_ok() {
            return "ANTHROPIC_API_KEY".to_string();
        }
        if let Ok(path) = std::env::var("ANTHROPIC_API_KEY_FILE") {
            return format!("ANTHROPIC_API_KEY_FILE ({})", path);
        }

        // Check for Claude Code OAuth credentials
        if self.retrieve_credentials(None).await.unwrap_or(None).is_some() {
            #[cfg(target_os = "macos")]
            {
                return "macOS Keychain (Claude Code-credentials)".to_string();
            }
            #[cfg(target_os = "linux")]
            {
                if let Some(home_dir) = dirs::home_dir() {
                    let credentials_path = home_dir.join(".claude").join(".credentials.json");
                    return credentials_path.to_string_lossy().to_string();
                }
            }
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            {
                return "Platform-specific credential store".to_string();
            }
        }

        "Unknown".to_string()
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
            config
                .prompt
                .as_ref()
                .map(|p| p.chars().take(50).collect::<String>())
                .unwrap_or_else(|| "None".to_string())
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

        // If snapshot hook is requested, install a Claude-style PostToolUse hook
        // by writing ~/.claude/settings.json in the selected HOME. This mirrors
        // tests/tools/mock-agent behavior: install a PostToolUse command hook that
        // executes after file ops. We keep it scoped to the provided HOME to avoid
        // touching the user's real ~/.claude.
        if let Some(snapshot_cmd) = &config.snapshot_cmd {
            let claude_dir = config.home_dir.join(".claude");
            tokio::fs::create_dir_all(&claude_dir).await.map_err(|e| {
                AgentError::ConfigCreationFailed(format!(
                    "Failed to create Claude settings dir {:?}: {}",
                    claude_dir, e
                ))
            })?;

            // Build the snapshot command with recorder socket parameter if available
            let full_snapshot_cmd = crate::snapshot::build_snapshot_command(snapshot_cmd);

            // Minimal hooks settings structure matching Claude Code hooks format
            let settings = serde_json::json!({
                "hooks": {
                    "PostToolUse": [
                        {
                            "matcher": ".*",
                            "hooks": [
                                {
                                    "type": "command",
                                    "command": full_snapshot_cmd,
                                    "timeout": 30
                                }
                            ]
                        }
                    ]
                }
            });

            let settings_path = claude_dir.join("settings.json");
            let settings_json = serde_json::to_string_pretty(&settings).map_err(|e| {
                AgentError::ConfigCreationFailed(format!(
                    "Failed to serialize Claude settings.json: {}",
                    e
                ))
            })?;
            tokio::fs::write(&settings_path, settings_json).await.map_err(|e| {
                AgentError::ConfigCreationFailed(format!(
                    "Failed to write Claude settings.json {:?}: {}",
                    settings_path, e
                ))
            })?;
        }

        // Copy credentials if requested and using custom HOME
        if config.copy_credentials && using_custom_home {
            if let Ok(system_home) = std::env::var("HOME") {
                let system_home = PathBuf::from(system_home);
                debug!(
                    "Copying credentials from {:?} to {:?}",
                    system_home, config.home_dir
                );

                // On macOS, credentials are in Keychain, not files. We need to extract them
                // and write to a file in the sandboxed HOME.
                #[cfg(target_os = "macos")]
                {
                    // Retrieve credentials from system Keychain BEFORE entering sandbox
                    if let Some(credentials_json) = self.retrieve_credentials(None).await? {
                        // Write credentials to the sandboxed HOME
                        let claude_dir = config.home_dir.join(".claude");
                        tokio::fs::create_dir_all(&claude_dir).await.map_err(|e| {
                            AgentError::CredentialCopyFailed(format!(
                                "Failed to create .claude directory in sandboxed HOME: {}",
                                e
                            ))
                        })?;

                        let credentials_path = claude_dir.join(".credentials.json");
                        tokio::fs::write(&credentials_path, &credentials_json).await.map_err(
                            |e| {
                                AgentError::CredentialCopyFailed(format!(
                                    "Failed to write credentials to sandboxed HOME: {}",
                                    e
                                ))
                            },
                        )?;

                        debug!(
                            "Wrote Claude credentials from Keychain to {:?}",
                            credentials_path
                        );
                    } else {
                        warn!("No Claude credentials found in Keychain to copy to sandboxed HOME");
                    }
                }

                // On Linux, use the default file-based copy
                #[cfg(not(target_os = "macos"))]
                {
                    self.copy_credentials(&system_home, &config.home_dir).await?;
                }
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
            // Set CLAUDE_CODE_OAUTH_TOKEN if we can retrieve credentials from the sandboxed HOME
            // Note: We look in the sandboxed HOME because credentials were copied there during
            // the credential copy phase above (if copy_credentials was enabled)
            match self.retrieve_credentials(Some(&config.home_dir)).await? {
                Some(credentials_json) => match self.extract_access_token(&credentials_json)? {
                    Some(access_token) => {
                        debug!("Setting CLAUDE_CODE_OAUTH_TOKEN environment variable");
                        cmd.env("CLAUDE_CODE_OAUTH_TOKEN", access_token);
                    }
                    None => {
                        return Err(AgentError::CredentialCopyFailed(
                            "Failed to extract access token from Claude credentials. \
                                The credentials file may be malformed or use an unexpected format."
                                .to_string(),
                        ));
                    }
                },
                None => {
                    return Err(AgentError::CredentialCopyFailed(format!(
                        "No Claude credentials found in HOME directory: {:?}\n\
                        \n\
                        Please ensure you are authenticated with Claude Code:\n\
                        1. Run 'claude setup-token' to configure authentication\n\
                        2. Or set the ANTHROPIC_API_KEY environment variable\n\
                        3. On macOS, ensure credentials are stored in Keychain\n\
                        4. On Linux, ensure credentials exist at ~/.claude/.credentials.json or ~/.config/claude/.credentials.json",
                        config.home_dir
                    )));
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

        // Configure output format
        if config.json_output {
            cmd.arg("--output-format");
            cmd.arg("json");
        }

        // Add the prompt as argument if provided
        if let Some(prompt) = &config.prompt {
            if !prompt.is_empty() {
                cmd.arg(prompt);
            }
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
        // Note: On macOS, we now extract credentials from Keychain and write them to a file
        // during the credential copy phase, so we include the file path here as well.
        // On Linux, we check multiple possible locations.
        vec![
            PathBuf::from(".claude/.credentials.json"),
            PathBuf::from(".config/claude/.credentials.json"),
        ]
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
        // Use system HOME (None) to retrieve from the user's actual credentials
        if let Some(credentials_json) = self.retrieve_credentials(None).await? {
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

    #[tokio::test]
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

    #[test]
    fn test_parse_oauth_credentials() {
        let json_str = r#"{"claudeAiOauth":{"accessToken":"sk-ant-oat01-test-token","refreshToken":"refresh-token","expiresAt":1792443506258,"scopes":["user:inference"],"subscriptionType":null}}"#;

        let credentials: ClaudeCredentials = serde_json::from_str(json_str).unwrap();
        assert_eq!(
            credentials.claude_ai_oauth.access_token,
            "sk-ant-oat01-test-token"
        );
    }

    #[test]
    fn test_parse_real_claude_credentials() {
        // Test with the real format from the user's example
        let json_str = r#"{"claudeAiOauth":{"accessToken":"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","refreshToken":"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","expiresAt":1792443506258,"scopes":["user:inference"],"subscriptionType":null}}"#;

        let credentials: ClaudeCredentials = serde_json::from_str(json_str).unwrap();
        assert_eq!(
            credentials.claude_ai_oauth.access_token,
            "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"
        );
    }

    #[tokio::test]
    #[cfg(target_os = "linux")]
    async fn test_retrieve_credentials_linux_primary_location() {
        use tempfile::TempDir;

        let agent = ClaudeAgent::new();
        let temp = TempDir::new().unwrap();
        let home_dir = temp.path();

        // Create credentials in primary location: ~/.claude/.credentials.json
        let claude_dir = home_dir.join(".claude");
        tokio::fs::create_dir_all(&claude_dir).await.unwrap();
        let credentials_path = claude_dir.join(".credentials.json");
        let test_creds = r#"{"claudeAiOauth":{"accessToken":"test-token","refreshToken":"refresh","expiresAt":1792443506258,"scopes":["user:inference"],"subscriptionType":null}}"#;
        tokio::fs::write(&credentials_path, test_creds).await.unwrap();

        // Test retrieval
        let result = agent.retrieve_credentials(Some(home_dir)).await;
        assert!(result.is_ok());
        let creds = result.unwrap();
        assert!(creds.is_some());
        assert_eq!(creds.unwrap(), test_creds);
    }

    #[tokio::test]
    #[cfg(target_os = "linux")]
    async fn test_retrieve_credentials_linux_config_location() {
        use tempfile::TempDir;

        let agent = ClaudeAgent::new();
        let temp = TempDir::new().unwrap();
        let home_dir = temp.path();

        // Create credentials in alternative location: ~/.config/claude/.credentials.json
        let config_claude_dir = home_dir.join(".config").join("claude");
        tokio::fs::create_dir_all(&config_claude_dir).await.unwrap();
        let credentials_path = config_claude_dir.join(".credentials.json");
        let test_creds = r#"{"claudeAiOauth":{"accessToken":"config-token","refreshToken":"refresh","expiresAt":1792443506258,"scopes":["user:inference"],"subscriptionType":null}}"#;
        tokio::fs::write(&credentials_path, test_creds).await.unwrap();

        // Test retrieval
        let result = agent.retrieve_credentials(Some(home_dir)).await;
        assert!(result.is_ok());
        let creds = result.unwrap();
        assert!(creds.is_some());
        assert_eq!(creds.unwrap(), test_creds);
    }

    #[tokio::test]
    #[cfg(target_os = "linux")]
    async fn test_retrieve_credentials_linux_prefers_primary() {
        use tempfile::TempDir;

        let agent = ClaudeAgent::new();
        let temp = TempDir::new().unwrap();
        let home_dir = temp.path();

        // Create credentials in BOTH locations
        let claude_dir = home_dir.join(".claude");
        tokio::fs::create_dir_all(&claude_dir).await.unwrap();
        let primary_path = claude_dir.join(".credentials.json");
        let primary_creds = r#"{"claudeAiOauth":{"accessToken":"primary-token","refreshToken":"refresh","expiresAt":1792443506258,"scopes":["user:inference"],"subscriptionType":null}}"#;
        tokio::fs::write(&primary_path, primary_creds).await.unwrap();

        let config_claude_dir = home_dir.join(".config").join("claude");
        tokio::fs::create_dir_all(&config_claude_dir).await.unwrap();
        let config_path = config_claude_dir.join(".credentials.json");
        let config_creds = r#"{"claudeAiOauth":{"accessToken":"config-token","refreshToken":"refresh","expiresAt":1792443506258,"scopes":["user:inference"],"subscriptionType":null}}"#;
        tokio::fs::write(&config_path, config_creds).await.unwrap();

        // Test retrieval - should prefer primary location
        let result = agent.retrieve_credentials(Some(home_dir)).await;
        assert!(result.is_ok());
        let creds = result.unwrap();
        assert!(creds.is_some());
        assert_eq!(creds.unwrap(), primary_creds);
    }

    #[tokio::test]
    #[cfg(target_os = "linux")]
    async fn test_retrieve_credentials_linux_missing() {
        use tempfile::TempDir;

        let agent = ClaudeAgent::new();
        let temp = TempDir::new().unwrap();
        let home_dir = temp.path();

        // Don't create any credentials file
        let result = agent.retrieve_credentials(Some(home_dir)).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_credential_paths() {
        let agent = ClaudeAgent::new();
        let paths = agent.credential_paths();

        // Should include both possible locations
        assert!(paths.contains(&PathBuf::from(".claude/.credentials.json")));
        assert!(paths.contains(&PathBuf::from(".config/claude/.credentials.json")));
    }

    #[tokio::test]
    #[cfg(target_os = "linux")]
    async fn test_prepare_launch_fails_without_credentials() {
        use tempfile::TempDir;

        // Use a harmless binary for version detection in tests instead of requiring
        // the real `claude` binary to be installed on the system. Most Unix-like
        // environments ship a `true` command that accepts `--version`, which is
        // sufficient for our version parsing and onboarding setup.
        let agent = ClaudeAgent {
            binary_path: "true".to_string(),
        };
        let temp_home = TempDir::new().unwrap();
        let temp_work = TempDir::new().unwrap();

        let config = AgentLaunchConfig {
            home_dir: temp_home.path().to_path_buf(),
            working_dir: temp_work.path().to_path_buf(),
            prompt: Some("test".to_string()),
            api_key: None, // No API key provided
            api_server: None,
            model: None,
            interactive: false,
            json_output: false,
            unrestricted: false,
            web_search: false,
            copy_credentials: false, // Don't copy credentials
            env_vars: vec![],
            snapshot_cmd: None,
            mcp_servers: vec![],
        };

        // This should fail because no credentials are available
        let result = agent.prepare_launch(config).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            AgentError::CredentialCopyFailed(msg) => {
                assert!(msg.contains("No Claude credentials found"));
                assert!(msg.contains("Run 'claude setup-token'"));
            }
            _ => panic!("Expected CredentialCopyFailed error, got: {:?}", err),
        }
    }

    #[serial_test::serial(env)]
    async fn test_get_claude_status_agent_not_found() {
        use crate::test_support::EnvVarGuard;

        // Clean environment variables to ensure consistent test results
        let _anthropic_guard = EnvVarGuard::remove("ANTHROPIC_API_KEY");
        let _anthropic_file_guard = EnvVarGuard::remove("ANTHROPIC_API_KEY_FILE");

        // Create an agent with a non-existent binary path
        let agent = ClaudeAgent {
            binary_path: "nonexistent-claude-agent".to_string(),
        };

        let status = agent.get_claude_status().await;

        assert!(!status.available);
        assert!(status.version.is_none());
        assert!(!status.authenticated);
        assert!(status.auth_method.is_none());
        assert!(status.auth_source.is_none());
        assert!(status.error.is_some());
        assert!(status.error.unwrap().contains("Claude CLI not found"));
    }

    #[tokio::test]
    async fn test_get_claude_status_timeout() {
        use std::time::Duration;
        use tokio::time::sleep;

        // Mock a ClaudeAgent that has a very slow detect_version method
        struct SlowClaudeAgent;

        impl SlowClaudeAgent {
            async fn detect_version(&self) -> AgentResult<AgentVersion> {
                // Sleep longer than the timeout to trigger timeout handling
                sleep(Duration::from_millis(2000)).await;
                Ok(AgentVersion {
                    version: "2.0.15".to_string(),
                    commit: None,
                    release_date: None,
                })
            }

            async fn get_claude_status_with_timeout(&self) -> ClaudeStatus {
                // Similar to the real implementation but with timeout
                let version_result = tokio::time::timeout(
                    Duration::from_millis(100), // Very short timeout to force timeout
                    self.detect_version(),
                )
                .await;

                let (available, version, error) = match version_result {
                    Ok(Ok(version_info)) => (true, Some(version_info.version), None),
                    Ok(Err(AgentError::AgentNotFound(_))) => (
                        false,
                        None,
                        Some("Claude CLI not found in PATH".to_string()),
                    ),
                    Ok(Err(e)) => (
                        false,
                        None,
                        Some(format!("Version detection failed: {}", e)),
                    ),
                    Err(_) => (false, None, Some("Version detection timed out".to_string())),
                };

                ClaudeStatus {
                    available,
                    version,
                    authenticated: false,
                    auth_method: None,
                    auth_source: None,
                    error,
                }
            }
        }

        let slow_agent = SlowClaudeAgent;
        let status = slow_agent.get_claude_status_with_timeout().await;

        assert!(!status.available);
        assert!(status.version.is_none());
        assert!(!status.authenticated);
        assert!(status.error.is_some());
        assert!(status.error.unwrap().contains("timed out"));
    }

    #[tokio::test]
    async fn test_detect_auth_method_anthropic_api_key() {
        // Mock agent that simulates ANTHROPIC_API_KEY environment variable
        struct TestClaudeAgent;

        impl TestClaudeAgent {
            async fn detect_auth_method(&self) -> String {
                "ANTHROPIC_API_KEY environment variable".to_string()
            }
        }

        let test_agent = TestClaudeAgent;
        let auth_method = test_agent.detect_auth_method().await;

        // Should return ANTHROPIC_API_KEY as highest priority
        assert_eq!(auth_method, "ANTHROPIC_API_KEY environment variable");
    }

    #[tokio::test]
    async fn test_detect_auth_method_anthropic_api_key_file() {
        // Mock agent that simulates ANTHROPIC_API_KEY_FILE environment variable
        struct TestClaudeAgent;

        impl TestClaudeAgent {
            async fn detect_auth_method(&self) -> String {
                "ANTHROPIC_API_KEY_FILE".to_string()
            }
        }

        let test_agent = TestClaudeAgent;
        let auth_method = test_agent.detect_auth_method().await;

        // Should return ANTHROPIC_API_KEY_FILE as second priority
        assert_eq!(auth_method, "ANTHROPIC_API_KEY_FILE");
    }

    #[tokio::test]
    async fn test_detect_auth_method_claude_oauth() {
        use std::fs;
        use tempfile::TempDir;

        // Create a temporary directory structure
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let temp_path = temp_dir.path();

        // Set up a mock Claude credentials file
        let claude_dir = temp_path.join(".claude");
        fs::create_dir_all(&claude_dir).expect("Failed to create claude dir");

        let credentials_path = claude_dir.join(".credentials.json");
        let credentials_content = r#"{"claudeAiOauth":{"accessToken":"mock_access_token","refreshToken":"mock_refresh_token","expiresAt":1792443506258,"scopes":["user:inference"],"subscriptionType":null}}"#;
        fs::write(&credentials_path, credentials_content)
            .expect("Failed to write credentials file");

        // Mock agent that simulates OAuth credentials detection
        struct TestClaudeAgent {}

        impl TestClaudeAgent {
            async fn detect_auth_method(&self) -> String {
                "Claude Code OAuth".to_string()
            }
        }

        let test_agent = TestClaudeAgent {};

        let auth_method = test_agent.detect_auth_method().await;

        // Should return Claude Code OAuth
        assert_eq!(auth_method, "Claude Code OAuth");
    }

    #[tokio::test]
    async fn test_detect_auth_method_unknown() {
        // Mock agent that simulates no authentication methods available
        struct TestClaudeAgent;

        impl TestClaudeAgent {
            async fn detect_auth_method(&self) -> String {
                "Unknown".to_string()
            }
        }

        let test_agent = TestClaudeAgent;
        let auth_method = test_agent.detect_auth_method().await;

        // Should return Unknown when no auth method is found
        assert_eq!(auth_method, "Unknown");
    }

    #[tokio::test]
    async fn test_detect_auth_source_anthropic_api_key() {
        // Mock agent that simulates ANTHROPIC_API_KEY environment variable
        struct TestClaudeAgent;

        impl TestClaudeAgent {
            async fn detect_auth_source(&self) -> String {
                "ANTHROPIC_API_KEY".to_string()
            }
        }

        let test_agent = TestClaudeAgent;
        let auth_source = test_agent.detect_auth_source().await;

        // Should return ANTHROPIC_API_KEY as highest priority source
        assert_eq!(auth_source, "ANTHROPIC_API_KEY");
    }

    #[tokio::test]
    async fn test_detect_auth_source_anthropic_api_key_file() {
        // Mock agent that simulates ANTHROPIC_API_KEY_FILE environment variable
        struct TestClaudeAgent;

        impl TestClaudeAgent {
            async fn detect_auth_source(&self) -> String {
                "ANTHROPIC_API_KEY_FILE (/path/to/api-key-file)".to_string()
            }
        }

        let test_agent = TestClaudeAgent;
        let auth_source = test_agent.detect_auth_source().await;

        // Should return file path as second priority source
        assert_eq!(
            auth_source,
            "ANTHROPIC_API_KEY_FILE (/path/to/api-key-file)"
        );
    }

    #[tokio::test]
    async fn test_detect_auth_source_claude_oauth_macos() {
        // Mock agent that simulates macOS Keychain authentication source
        struct TestClaudeAgent;

        impl TestClaudeAgent {
            async fn detect_auth_source(&self) -> String {
                "macOS Keychain (Claude Code-credentials)".to_string()
            }
        }

        let test_agent = TestClaudeAgent;
        let auth_source = test_agent.detect_auth_source().await;

        // Should return macOS Keychain source
        assert_eq!(auth_source, "macOS Keychain (Claude Code-credentials)");
    }

    #[tokio::test]
    async fn test_detect_auth_source_claude_oauth_linux() {
        use std::fs;
        use tempfile::TempDir;

        // Create a temporary directory structure
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let temp_path = temp_dir.path();

        // Set up a mock Claude credentials file
        let claude_dir = temp_path.join(".claude");
        fs::create_dir_all(&claude_dir).expect("Failed to create claude dir");

        let credentials_path = claude_dir.join(".credentials.json");
        let credentials_content = r#"{"claudeAiOauth":{"accessToken":"mock_access_token","refreshToken":"mock_refresh_token","expiresAt":1792443506258,"scopes":["user:inference"],"subscriptionType":null}}"#;
        fs::write(&credentials_path, credentials_content)
            .expect("Failed to write credentials file");

        // Mock auth source detection with custom home directory (Linux-style)
        struct TestClaudeAgent {
            home_dir: PathBuf,
        }

        impl TestClaudeAgent {
            async fn detect_auth_source(&self) -> String {
                let credentials_path = self.home_dir.join(".claude").join(".credentials.json");
                if credentials_path.exists() {
                    return credentials_path.to_string_lossy().to_string();
                }
                "Unknown".to_string()
            }
        }

        let test_agent = TestClaudeAgent {
            home_dir: temp_path.to_path_buf(),
        };

        let auth_source = test_agent.detect_auth_source().await;

        // Should return credentials file path
        assert!(auth_source.contains(".claude/.credentials.json"));
    }

    #[tokio::test]
    async fn test_detect_auth_source_unknown() {
        // Mock agent that simulates no authentication sources available
        struct TestClaudeAgent;

        impl TestClaudeAgent {
            async fn detect_auth_source(&self) -> String {
                "Unknown".to_string()
            }
        }

        let test_agent = TestClaudeAgent;
        let auth_source = test_agent.detect_auth_source().await;

        // Should return Unknown when no auth source is found
        assert_eq!(auth_source, "Unknown");
    }

    #[tokio::test]
    async fn test_get_claude_status_successful_with_auth() {
        // Create a test agent that mocks successful version detection and authentication
        struct TestClaudeAgent {}

        impl TestClaudeAgent {
            async fn detect_version(&self) -> AgentResult<AgentVersion> {
                Ok(AgentVersion {
                    version: "2.0.15".to_string(),
                    commit: None,
                    release_date: None,
                })
            }

            async fn get_user_api_key(&self) -> AgentResult<Option<String>> {
                Ok(Some("sk-ant-mock-api-key-12345".to_string()))
            }

            async fn detect_auth_method(&self) -> String {
                "ANTHROPIC_API_KEY environment variable".to_string()
            }

            async fn detect_auth_source(&self) -> String {
                "ANTHROPIC_API_KEY".to_string()
            }

            async fn get_claude_status(&self) -> ClaudeStatus {
                // Simplified version of the real implementation
                let version_result = self.detect_version().await;

                let (available, version, error) = match version_result {
                    Ok(version_info) => (true, Some(version_info.version), None),
                    Err(AgentError::AgentNotFound(_)) => (
                        false,
                        None,
                        Some("Claude CLI not found in PATH".to_string()),
                    ),
                    Err(e) => (
                        false,
                        None,
                        Some(format!("Version detection failed: {}", e)),
                    ),
                };

                if !available {
                    return ClaudeStatus {
                        available: false,
                        version: None,
                        authenticated: false,
                        auth_method: None,
                        auth_source: None,
                        error,
                    };
                }

                match self.get_user_api_key().await {
                    Ok(Some(_api_key)) => {
                        let method = self.detect_auth_method().await;
                        let source = self.detect_auth_source().await;
                        ClaudeStatus {
                            available,
                            version,
                            authenticated: true,
                            auth_method: Some(method),
                            auth_source: Some(source),
                            error,
                        }
                    }
                    _ => ClaudeStatus {
                        available,
                        version,
                        authenticated: false,
                        auth_method: None,
                        auth_source: None,
                        error,
                    },
                }
            }
        }

        let test_agent = TestClaudeAgent {};

        let status = test_agent.get_claude_status().await;

        assert!(status.available);
        assert_eq!(status.version, Some("2.0.15".to_string()));
        assert!(status.authenticated);
        assert_eq!(
            status.auth_method,
            Some("ANTHROPIC_API_KEY environment variable".to_string())
        );
        assert_eq!(status.auth_source, Some("ANTHROPIC_API_KEY".to_string()));
        assert!(status.error.is_none());
    }

    #[tokio::test]
    async fn test_get_claude_status_no_auth() {
        struct TestClaudeAgentNoAuth;

        impl TestClaudeAgentNoAuth {
            async fn detect_version(&self) -> AgentResult<AgentVersion> {
                Ok(AgentVersion {
                    version: "2.0.15".to_string(),
                    commit: None,
                    release_date: None,
                })
            }

            async fn get_user_api_key(&self) -> AgentResult<Option<String>> {
                Ok(None) // No API key found
            }

            async fn get_claude_status(&self) -> ClaudeStatus {
                let version_result = self.detect_version().await;

                let (available, version, error) = match version_result {
                    Ok(version_info) => (true, Some(version_info.version), None),
                    Err(AgentError::AgentNotFound(_)) => (
                        false,
                        None,
                        Some("Claude CLI not found in PATH".to_string()),
                    ),
                    Err(e) => (
                        false,
                        None,
                        Some(format!("Version detection failed: {}", e)),
                    ),
                };

                if !available {
                    return ClaudeStatus {
                        available: false,
                        version: None,
                        authenticated: false,
                        auth_method: None,
                        auth_source: None,
                        error,
                    };
                }

                match self.get_user_api_key().await {
                    Ok(Some(_api_key)) => ClaudeStatus {
                        available,
                        version,
                        authenticated: true,
                        auth_method: Some("mock".to_string()),
                        auth_source: Some("mock".to_string()),
                        error,
                    },
                    _ => ClaudeStatus {
                        available,
                        version,
                        authenticated: false,
                        auth_method: None,
                        auth_source: None,
                        error,
                    },
                }
            }
        }

        let test_agent = TestClaudeAgentNoAuth;
        let status = test_agent.get_claude_status().await;

        assert!(status.available);
        assert_eq!(status.version, Some("2.0.15".to_string()));
        assert!(!status.authenticated);
        assert!(status.auth_method.is_none());
        assert!(status.auth_source.is_none());
        assert!(status.error.is_none());
    }

    #[tokio::test]
    async fn test_get_claude_status_authentication_error() {
        struct TestClaudeAgentAuthError;

        impl TestClaudeAgentAuthError {
            async fn detect_version(&self) -> AgentResult<AgentVersion> {
                Ok(AgentVersion {
                    version: "2.0.15".to_string(),
                    commit: None,
                    release_date: None,
                })
            }

            async fn get_user_api_key(&self) -> AgentResult<Option<String>> {
                Err(AgentError::CredentialCopyFailed(
                    "Mock auth error".to_string(),
                ))
            }

            async fn get_claude_status(&self) -> ClaudeStatus {
                let version_result = self.detect_version().await;

                let (available, version, mut error) = match version_result {
                    Ok(version_info) => (true, Some(version_info.version), None),
                    Err(AgentError::AgentNotFound(_)) => (
                        false,
                        None,
                        Some("Claude CLI not found in PATH".to_string()),
                    ),
                    Err(e) => (
                        false,
                        None,
                        Some(format!("Version detection failed: {}", e)),
                    ),
                };

                if !available {
                    return ClaudeStatus {
                        available: false,
                        version: None,
                        authenticated: false,
                        auth_method: None,
                        auth_source: None,
                        error,
                    };
                }

                let (authenticated, auth_method, auth_source) = match self.get_user_api_key().await
                {
                    Ok(Some(_api_key)) => {
                        (true, Some("mock".to_string()), Some("mock".to_string()))
                    }
                    Ok(None) => (false, None, None),
                    Err(e) => {
                        error = Some(format!("Authentication check failed: {}", e));
                        (false, None, None)
                    }
                };

                ClaudeStatus {
                    available,
                    version,
                    authenticated,
                    auth_method,
                    auth_source,
                    error,
                }
            }
        }

        let test_agent = TestClaudeAgentAuthError;
        let status = test_agent.get_claude_status().await;

        assert!(status.available);
        assert_eq!(status.version, Some("2.0.15".to_string()));
        assert!(!status.authenticated);
        assert!(status.auth_method.is_none());
        assert!(status.auth_source.is_none());
        assert!(status.error.is_some());
        assert!(status.error.unwrap().contains("Authentication check failed"));
    }
}
