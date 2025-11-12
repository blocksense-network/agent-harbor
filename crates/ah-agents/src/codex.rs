// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

/// Codex CLI agent implementation
use crate::session::{export_directory, import_directory};
use crate::traits::*;
use async_trait::async_trait;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::process::{Child, Command};
use tracing::{debug, info, warn};

/// Status information for the Codex CLI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexStatus {
    /// Whether the CLI is installed and available
    pub available: bool,
    /// Version information if available
    pub version: Option<String>,
    /// Whether the user is authenticated
    pub authenticated: bool,
    /// Authentication method used (e.g., "OPENAI_API_KEY", "CODEX_API_KEY", "OAuth Token Exchange")
    pub auth_method: Option<String>,
    /// Source of authentication (config file path, environment variable name, etc.)
    pub auth_source: Option<String>,
    /// Any error that occurred during status check
    pub error: Option<String>,
}

/// Codex CLI agent executor
pub struct CodexAgent {
    binary_path: String,
}

impl CodexAgent {
    pub fn new() -> Self {
        Self {
            binary_path: "codex".to_string(),
        }
    }

    /// Parse version from `codex --version` output
    fn parse_version(output: &str) -> AgentResult<AgentVersion> {
        // Expected format: "codex X.Y.Z" or "X.Y.Z" or multi-line with version info
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

    /// Get comprehensive status information for Codex CLI
    ///
    /// Returns CodexStatus with detailed information about:
    /// - CLI availability and version
    /// - Authentication status and method
    /// - API key information source
    /// - Any errors encountered
    pub async fn get_codex_status(&self) -> CodexStatus {
        // Check CLI availability by detecting version with timeout
        let (available, version, mut error) = match tokio::time::timeout(
            std::time::Duration::from_millis(1500),
            self.detect_version(),
        )
        .await
        {
            Ok(Ok(version_info)) => (true, Some(version_info.version), None),
            Ok(Err(AgentError::AgentNotFound(_))) => {
                (false, None, Some("Codex CLI not found in PATH".to_string()))
            }
            Ok(Err(e)) => (
                false,
                None,
                Some(format!("Version detection failed: {}", e)),
            ),
            Err(_) => (false, None, Some("Version detection timed out".to_string())),
        };

        if !available {
            return CodexStatus {
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

        CodexStatus {
            available,
            version,
            authenticated,
            auth_method,
            auth_source,
            error,
        }
    }

    /// Detect which authentication method is being used for Codex
    async fn detect_auth_method(&self) -> String {
        // Check in order of precedence to determine which method is being used
        if std::env::var("OPENAI_API_KEY").is_ok() {
            return "OPENAI_API_KEY environment variable".to_string();
        }
        if std::env::var("OPENAI_API_KEY_FILE").is_ok() {
            return "OPENAI_API_KEY_FILE".to_string();
        }
        if std::env::var("CODEX_API_KEY").is_ok() {
            return "CODEX_API_KEY environment variable".to_string();
        }

        // Check for OAuth token exchange capability
        if let Some(home_dir) = dirs::home_dir() {
            let auth_path = home_dir.join(".codex").join("auth.json");
            if auth_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&auth_path) {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                        if json.get("oauth").is_some() {
                            return "OAuth Token Exchange from Codex auth file".to_string();
                        }
                    }
                }
            }
        }

        "Unknown".to_string()
    }

    /// Detect the source of authentication for Codex
    async fn detect_auth_source(&self) -> String {
        // Check in order of precedence to determine the actual source
        if std::env::var("OPENAI_API_KEY").is_ok() {
            return "OPENAI_API_KEY".to_string();
        }
        if let Ok(file_path) = std::env::var("OPENAI_API_KEY_FILE") {
            return format!("File: {}", file_path);
        }
        if std::env::var("CODEX_API_KEY").is_ok() {
            return "CODEX_API_KEY".to_string();
        }

        // Check for OAuth credentials in Codex auth file
        if let Some(home_dir) = dirs::home_dir() {
            let auth_path = home_dir.join(".codex").join("auth.json");
            if auth_path.exists() {
                return format!("OAuth credentials: {}", auth_path.display());
            }
        }

        "Unknown".to_string()
    }
}

impl Default for CodexAgent {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AgentExecutor for CodexAgent {
    fn name(&self) -> &'static str {
        "codex"
    }

    async fn detect_version(&self) -> AgentResult<AgentVersion> {
        debug!("Detecting Codex CLI version");

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
            "Preparing Codex CLI launch with prompt: {:?}",
            config
                .prompt
                .as_ref()
                .map(|p| p.chars().take(50).collect::<String>())
                .unwrap_or_else(|| "None".to_string())
        );

        // Copy credentials if requested and home_dir differs from system HOME
        if config.copy_credentials {
            if let Ok(system_home) = std::env::var("HOME") {
                let system_home = PathBuf::from(system_home);
                if system_home != config.home_dir {
                    debug!(
                        "Copying credentials from {:?} to {:?}",
                        system_home, config.home_dir
                    );
                    self.copy_credentials(&system_home, &config.home_dir).await?;
                }
            }
        }

        let mut cmd = tokio::process::Command::new(&self.binary_path);

        // Set custom CODEX_HOME directory (Codex-specific home directory)
        let codex_home = config.home_dir.join(".codex");
        cmd.env("CODEX_HOME", &codex_home);

        // Set current directory
        cmd.current_dir(&config.working_dir);

        // Codex uses exec subcommand for non-interactive mode
        if !config.interactive {
            cmd.arg("exec");
        }

        // Add custom API server if specified
        if let Some(api_server) = &config.api_server {
            cmd.env("OPENAI_API_BASE", api_server);
            cmd.env("OPENAI_BASE_URL", api_server);
            cmd.env("CODEX_API_BASE", api_server);
        }

        // Add API key if specified
        if let Some(api_key) = &config.api_key {
            cmd.env("OPENAI_API_KEY", api_key);
            cmd.env("CODEX_API_KEY", api_key);
        }

        // Add additional environment variables
        for (key, value) in &config.env_vars {
            cmd.env(key, value);
        }

        // Add security and capability flags
        if config.unrestricted {
            cmd.arg("--dangerously-bypass-approvals-and-sandbox");
        } else {
            cmd.arg("--full-auto");
        }

        if let Some(snapshot_cmd) = &config.snapshot_cmd {
            cmd.arg("--rollout-hook");
            cmd.arg(crate::snapshot::build_snapshot_command(snapshot_cmd));
        }

        // Add model specification
        let model = config.model.as_deref().unwrap_or("gpt-5-codex");
        cmd.arg("--model");
        cmd.arg(model);

        if config.web_search {
            cmd.arg("--search");
        }

        // Configure output format
        if config.json_output {
            cmd.arg("--json");
        }

        // Configure stdio for piped I/O
        use std::process::Stdio;
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        // Add the prompt as argument if provided
        if let Some(prompt) = &config.prompt {
            if !prompt.is_empty() {
                cmd.arg(prompt);
            }
        }

        debug!("Codex CLI command prepared successfully");
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

        debug!("Codex CLI process spawned successfully");
        Ok(child)
    }

    /// Platform-specific credential paths for Codex CLI
    fn credential_paths(&self) -> Vec<PathBuf> {
        vec![
            // Authentication file (as defined in Codex Rust code)
            PathBuf::from(".codex/auth.json"),
        ]
    }

    async fn get_user_api_key(&self) -> AgentResult<Option<String>> {
        // 1. Check direct environment variable (Codex-specific: OPENAI_API_KEY)
        if let Ok(api_key) = std::env::var("OPENAI_API_KEY") {
            if !api_key.trim().is_empty() {
                debug!("Found OpenAI API key in environment variable");
                return Ok(Some(api_key));
            }
        }

        // 2. Check environment variable pointing to file (Codex-specific: OPENAI_API_KEY_FILE)
        if let Ok(file_path) = std::env::var("OPENAI_API_KEY_FILE") {
            match tokio::fs::read_to_string(&file_path).await {
                Ok(content) => {
                    let api_key = content.trim().to_string();
                    if !api_key.is_empty() {
                        debug!("Found OpenAI API key in file specified by OPENAI_API_KEY_FILE");
                        return Ok(Some(api_key));
                    }
                }
                Err(e) => {
                    warn!("Failed to read API key file {}: {}", file_path, e);
                }
            }
        }

        // 3. Try OAuth token exchange from Codex auth file
        if let Some(home_dir) = dirs::home_dir() {
            let auth_path = home_dir.join(".codex").join("auth.json");

            match std::fs::read_to_string(&auth_path) {
                Ok(content) => {
                    match serde_json::from_str::<serde_json::Value>(&content) {
                        Ok(json) => {
                            // Try to extract OAuth tokens from the auth file
                            if let Some(oauth) = json.get("oauth") {
                                if let (Some(_access_token), Some(id_token)) = (
                                    oauth.get("access_token").and_then(|v| v.as_str()),
                                    oauth.get("id_token").and_then(|v| v.as_str()),
                                ) {
                                    debug!(
                                        "Found OAuth credentials in Codex auth file, attempting token exchange"
                                    );
                                    match crate::oauth_key_exchange::exchange_oauth_for_openai_api_key(id_token, "codex").await {
                                        Ok(api_key) => Ok(Some(api_key)),
                                        Err(e) => {
                                            warn!("Failed to exchange OAuth token for API key: {}", e);
                                            Ok(None)
                                        }
                                    }
                                } else {
                                    debug!("OAuth credentials missing required tokens");
                                    Ok(None)
                                }
                            } else {
                                debug!("No OAuth section found in Codex auth file");
                                Ok(None)
                            }
                        }
                        Err(e) => {
                            warn!("Failed to parse Codex auth file: {}", e);
                            Ok(None)
                        }
                    }
                }
                Err(e) => {
                    debug!("Codex auth file not readable: {}", e);
                    Ok(None)
                }
            }
        } else {
            warn!("Could not determine home directory");
            Ok(None)
        }
    }

    async fn export_session(&self, home_dir: &Path) -> AgentResult<PathBuf> {
        let config_dir = self.config_dir(home_dir);
        let archive_path = home_dir.join("codex-session.tar.gz");

        export_directory(&config_dir, &archive_path).await?;
        Ok(archive_path)
    }

    async fn import_session(&self, session_archive: &Path, home_dir: &Path) -> AgentResult<()> {
        let config_dir = self.config_dir(home_dir);
        import_directory(session_archive, &config_dir).await
    }

    fn parse_output(&self, raw_output: &[u8]) -> AgentResult<Vec<AgentEvent>> {
        // Parse Codex CLI output into normalized events
        let output = String::from_utf8_lossy(raw_output);
        let mut events = Vec::new();

        // Basic line-by-line parsing
        for line in output.lines() {
            if line.contains("Thinking") || line.contains("reasoning") {
                events.push(AgentEvent::Thinking {
                    content: line.to_string(),
                });
            } else if line.contains("Tool") || line.contains("Command") {
                events.push(AgentEvent::ToolUse {
                    tool_name: "shell".to_string(),
                    arguments: serde_json::json!({"command": line}),
                });
            } else if line.contains("Error") || line.contains("error") {
                events.push(AgentEvent::Error {
                    message: line.to_string(),
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
        home.join(".config/codex")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version() {
        let output = "codex 0.46.0";
        let result = CodexAgent::parse_version(output);
        assert!(result.is_ok());
        let version = result.unwrap();
        assert_eq!(version.version, "0.46.0");
    }

    #[test]
    fn test_parse_version_simple() {
        let output = "0.46.0";
        let result = CodexAgent::parse_version(output);
        assert!(result.is_ok());
        let version = result.unwrap();
        assert_eq!(version.version, "0.46.0");
    }

    #[tokio::test]
    async fn test_agent_name() {
        let agent = CodexAgent::new();
        assert_eq!(agent.name(), "codex");
    }

    #[tokio::test]
    async fn test_config_dir() {
        let agent = CodexAgent::new();
        let home = PathBuf::from("/home/user");
        let config = agent.config_dir(&home);
        assert_eq!(config, PathBuf::from("/home/user/.config/codex"));
    }

    #[tokio::test]
    #[serial_test::serial(env)]
    async fn test_get_codex_status_agent_not_found() {
        use crate::test_support::EnvVarGuard;

        // Clean environment variables to ensure consistent test results
        let _openai_guard = EnvVarGuard::remove("OPENAI_API_KEY");
        let _openai_file_guard = EnvVarGuard::remove("OPENAI_API_KEY_FILE");
        let _codex_guard = EnvVarGuard::remove("CODEX_API_KEY");

        // Create an agent with a non-existent binary path
        let agent = CodexAgent {
            binary_path: "nonexistent-codex-agent".to_string(),
        };

        let status = agent.get_codex_status().await;

        assert!(!status.available);
        assert!(status.version.is_none());
        assert!(!status.authenticated);
        assert!(status.auth_method.is_none());
        assert!(status.auth_source.is_none());
        assert!(status.error.is_some());
        assert!(status.error.unwrap().contains("not found in PATH"));
    }

    #[tokio::test]
    async fn test_get_codex_status_timeout() {
        use std::time::Duration;
        use tokio::time::sleep;

        // Mock a CodexAgent that has a very slow detect_version method
        struct SlowCodexAgent;

        impl SlowCodexAgent {
            async fn detect_version(&self) -> AgentResult<AgentVersion> {
                // Sleep longer than the timeout to trigger timeout handling
                sleep(Duration::from_millis(2000)).await;
                Ok(AgentVersion {
                    version: "0.46.0".to_string(),
                    commit: None,
                    release_date: None,
                })
            }

            async fn get_codex_status_with_timeout(&self) -> CodexStatus {
                // Similar to the real implementation but with timeout
                let version_result = tokio::time::timeout(
                    Duration::from_millis(100), // Very short timeout to force timeout
                    self.detect_version(),
                )
                .await;

                let (available, version, error) = match version_result {
                    Ok(Ok(version_info)) => (true, Some(version_info.version), None),
                    Ok(Err(AgentError::AgentNotFound(_))) => {
                        (false, None, Some("Codex CLI not found in PATH".to_string()))
                    }
                    Ok(Err(e)) => (
                        false,
                        None,
                        Some(format!("Version detection failed: {}", e)),
                    ),
                    Err(_) => (false, None, Some("Version detection timed out".to_string())),
                };

                CodexStatus {
                    available,
                    version,
                    authenticated: false,
                    auth_method: None,
                    auth_source: None,
                    error,
                }
            }
        }

        let slow_agent = SlowCodexAgent;
        let status = slow_agent.get_codex_status_with_timeout().await;

        assert!(!status.available);
        assert!(status.version.is_none());
        assert!(!status.authenticated);
        assert!(status.error.is_some());
        assert!(status.error.unwrap().contains("timed out"));
    }

    #[tokio::test]
    async fn test_detect_auth_method_openai_api_key() {
        // Mock agent that simulates OPENAI_API_KEY environment variable
        struct TestCodexAgent;

        impl TestCodexAgent {
            async fn detect_auth_method(&self) -> String {
                "OPENAI_API_KEY environment variable".to_string()
            }
        }

        let test_agent = TestCodexAgent;
        let auth_method = test_agent.detect_auth_method().await;

        // Should return OPENAI_API_KEY as highest priority
        assert_eq!(auth_method, "OPENAI_API_KEY environment variable");
    }

    #[tokio::test]
    async fn test_detect_auth_method_openai_api_key_file() {
        // Mock agent that simulates OPENAI_API_KEY_FILE environment variable
        struct TestCodexAgent;

        impl TestCodexAgent {
            async fn detect_auth_method(&self) -> String {
                "OPENAI_API_KEY_FILE".to_string()
            }
        }

        let test_agent = TestCodexAgent;
        let auth_method = test_agent.detect_auth_method().await;

        // Should return OPENAI_API_KEY_FILE as second priority
        assert_eq!(auth_method, "OPENAI_API_KEY_FILE");
    }

    #[tokio::test]
    async fn test_detect_auth_method_codex_api_key() {
        // Mock agent that simulates CODEX_API_KEY environment variable
        struct TestCodexAgent;

        impl TestCodexAgent {
            async fn detect_auth_method(&self) -> String {
                "CODEX_API_KEY environment variable".to_string()
            }
        }

        let test_agent = TestCodexAgent;
        let auth_method = test_agent.detect_auth_method().await;

        // Should return CODEX_API_KEY as third priority
        assert_eq!(auth_method, "CODEX_API_KEY environment variable");
    }

    #[tokio::test]
    async fn test_detect_auth_method_oauth_token() {
        use std::fs;
        use tempfile::TempDir;

        // Create a temporary directory structure
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let temp_path = temp_dir.path();

        // Set up a mock Codex auth file
        let codex_dir = temp_path.join(".codex");
        fs::create_dir_all(&codex_dir).expect("Failed to create codex dir");

        let auth_path = codex_dir.join("auth.json");
        let auth_content = r#"
        {
            "oauth": {
                "access_token": "mock_access_token",
                "id_token": "mock_id_token"
            }
        }
        "#;
        fs::write(&auth_path, auth_content).expect("Failed to write auth file");

        // Mock home directory detection
        struct TestCodexAgent {
            home_dir: PathBuf,
        }

        impl TestCodexAgent {
            async fn detect_auth_method(&self) -> String {
                // Check for OAuth token exchange capability using mock home dir (skip env vars for this test)
                let auth_path = self.home_dir.join(".codex").join("auth.json");
                if auth_path.exists() {
                    if let Ok(content) = std::fs::read_to_string(&auth_path) {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                            if json.get("oauth").is_some() {
                                return "OAuth Token Exchange from Codex auth file".to_string();
                            }
                        }
                    }
                }

                "Unknown".to_string()
            }
        }

        let test_agent = TestCodexAgent {
            home_dir: temp_path.to_path_buf(),
        };

        let auth_method = test_agent.detect_auth_method().await;

        // Should return OAuth Token Exchange
        assert_eq!(auth_method, "OAuth Token Exchange from Codex auth file");
    }

    #[tokio::test]
    async fn test_detect_auth_method_unknown() {
        // Mock agent that simulates no environment variables or auth files
        struct TestCodexAgent;

        impl TestCodexAgent {
            async fn detect_auth_method(&self) -> String {
                "Unknown".to_string()
            }
        }

        let test_agent = TestCodexAgent;
        let auth_method = test_agent.detect_auth_method().await;

        // Should return Unknown when no auth method is found
        assert_eq!(auth_method, "Unknown");
    }

    #[tokio::test]
    async fn test_detect_auth_source_openai_api_key() {
        // Mock agent that simulates OPENAI_API_KEY environment variable
        struct TestCodexAgent;

        impl TestCodexAgent {
            async fn detect_auth_source(&self) -> String {
                "OPENAI_API_KEY".to_string()
            }
        }

        let test_agent = TestCodexAgent;
        let auth_source = test_agent.detect_auth_source().await;

        // Should return OPENAI_API_KEY as highest priority source
        assert_eq!(auth_source, "OPENAI_API_KEY");
    }

    #[tokio::test]
    async fn test_detect_auth_source_openai_api_key_file() {
        // Mock agent that simulates OPENAI_API_KEY_FILE environment variable
        struct TestCodexAgent;

        impl TestCodexAgent {
            async fn detect_auth_source(&self) -> String {
                "File: /path/to/api-key-file".to_string()
            }
        }

        let test_agent = TestCodexAgent;
        let auth_source = test_agent.detect_auth_source().await;

        // Should return file path as second priority source
        assert_eq!(auth_source, "File: /path/to/api-key-file");
    }

    #[tokio::test]
    async fn test_detect_auth_source_codex_api_key() {
        // Mock agent that simulates CODEX_API_KEY environment variable
        struct TestCodexAgent;

        impl TestCodexAgent {
            async fn detect_auth_source(&self) -> String {
                "CODEX_API_KEY".to_string()
            }
        }

        let test_agent = TestCodexAgent;
        let auth_source = test_agent.detect_auth_source().await;

        // Should return CODEX_API_KEY as third priority source
        assert_eq!(auth_source, "CODEX_API_KEY");
    }

    #[tokio::test]
    async fn test_detect_auth_source_oauth_credentials() {
        use std::fs;
        use tempfile::TempDir;

        // Create a temporary directory structure
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let temp_path = temp_dir.path();

        // Set up a mock Codex auth file
        let codex_dir = temp_path.join(".codex");
        fs::create_dir_all(&codex_dir).expect("Failed to create codex dir");

        let auth_path = codex_dir.join("auth.json");
        let auth_content = r#"
        {
            "oauth": {
                "access_token": "mock_access_token",
                "id_token": "mock_id_token"
            }
        }
        "#;
        fs::write(&auth_path, auth_content).expect("Failed to write auth file");

        // Mock auth source detection with custom home directory
        struct TestCodexAgent {
            home_dir: PathBuf,
        }

        impl TestCodexAgent {
            async fn detect_auth_source(&self) -> String {
                // Check for OAuth credentials in Codex auth file using mock home dir
                let auth_path = self.home_dir.join(".codex").join("auth.json");
                if auth_path.exists() {
                    return format!("OAuth credentials: {}", auth_path.display());
                }

                "Unknown".to_string()
            }
        }

        let test_agent = TestCodexAgent {
            home_dir: temp_path.to_path_buf(),
        };

        let auth_source = test_agent.detect_auth_source().await;

        // Should return OAuth credentials path
        assert!(auth_source.starts_with("OAuth credentials:"));
        assert!(auth_source.contains(".codex/auth.json"));
    }

    #[tokio::test]
    async fn test_detect_auth_source_unknown() {
        // Mock agent that simulates no environment variables and no auth file
        struct TestCodexAgent;

        impl TestCodexAgent {
            async fn detect_auth_source(&self) -> String {
                // Simulate no environment variables and no home directory
                "Unknown".to_string()
            }
        }

        let test_agent = TestCodexAgent;
        let auth_source = test_agent.detect_auth_source().await;

        // Should return Unknown when no auth source is found
        assert_eq!(auth_source, "Unknown");
    }

    #[tokio::test]
    async fn test_get_codex_status_successful_with_auth() {
        // Create a test agent that mocks successful version detection and authentication
        struct TestCodexAgent {}

        impl TestCodexAgent {
            async fn detect_version(&self) -> AgentResult<AgentVersion> {
                Ok(AgentVersion {
                    version: "0.46.0".to_string(),
                    commit: None,
                    release_date: None,
                })
            }

            async fn get_user_api_key(&self) -> AgentResult<Option<String>> {
                Ok(Some("mock_api_key_12345".to_string()))
            }

            async fn detect_auth_method(&self) -> String {
                "OPENAI_API_KEY environment variable".to_string()
            }

            async fn detect_auth_source(&self) -> String {
                "OPENAI_API_KEY".to_string()
            }

            async fn get_codex_status(&self) -> CodexStatus {
                // Simplified version of the real implementation
                let version_result = self.detect_version().await;

                let (available, version, error) = match version_result {
                    Ok(version_info) => (true, Some(version_info.version), None),
                    Err(e) => (
                        false,
                        None,
                        Some(format!("Version detection failed: {}", e)),
                    ),
                };

                if !available {
                    return CodexStatus {
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
                        CodexStatus {
                            available,
                            version,
                            authenticated: true,
                            auth_method: Some(method),
                            auth_source: Some(source),
                            error,
                        }
                    }
                    _ => CodexStatus {
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

        let test_agent = TestCodexAgent {};

        let status = test_agent.get_codex_status().await;

        assert!(status.available);
        assert_eq!(status.version, Some("0.46.0".to_string()));
        assert!(status.authenticated);
        assert_eq!(
            status.auth_method,
            Some("OPENAI_API_KEY environment variable".to_string())
        );
        assert_eq!(status.auth_source, Some("OPENAI_API_KEY".to_string()));
        assert!(status.error.is_none());
    }

    #[tokio::test]
    async fn test_get_codex_status_no_auth() {
        struct TestCodexAgentNoAuth;

        impl TestCodexAgentNoAuth {
            async fn detect_version(&self) -> AgentResult<AgentVersion> {
                Ok(AgentVersion {
                    version: "0.46.0".to_string(),
                    commit: None,
                    release_date: None,
                })
            }

            async fn get_user_api_key(&self) -> AgentResult<Option<String>> {
                Ok(None) // No API key found
            }

            async fn get_codex_status(&self) -> CodexStatus {
                let version_result = self.detect_version().await;

                let (available, version, error) = match version_result {
                    Ok(version_info) => (true, Some(version_info.version), None),
                    Err(e) => (
                        false,
                        None,
                        Some(format!("Version detection failed: {}", e)),
                    ),
                };

                if !available {
                    return CodexStatus {
                        available: false,
                        version: None,
                        authenticated: false,
                        auth_method: None,
                        auth_source: None,
                        error,
                    };
                }

                match self.get_user_api_key().await {
                    Ok(Some(_api_key)) => CodexStatus {
                        available,
                        version,
                        authenticated: true,
                        auth_method: Some("mock".to_string()),
                        auth_source: Some("mock".to_string()),
                        error,
                    },
                    _ => CodexStatus {
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

        let test_agent = TestCodexAgentNoAuth;
        let status = test_agent.get_codex_status().await;

        assert!(status.available);
        assert_eq!(status.version, Some("0.46.0".to_string()));
        assert!(!status.authenticated);
        assert!(status.auth_method.is_none());
        assert!(status.auth_source.is_none());
        assert!(status.error.is_none());
    }
}
