// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

/// Codex CLI agent implementation
use crate::session::{export_directory, import_directory};
use crate::traits::*;
use async_trait::async_trait;
use regex::Regex;
use std::path::{Path, PathBuf};
use tokio::process::{Child, Command};
use tracing::{debug, info, warn};

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
            config.prompt.chars().take(50).collect::<String>()
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

        // Add model specification
        let model = config.model.as_deref().unwrap_or("gpt-5-codex");
        cmd.arg("--model");
        cmd.arg(model);

        if config.web_search {
            cmd.arg("--search");
        }

        // Configure stdio for piped I/O
        use std::process::Stdio;
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        // Add the prompt as argument if provided
        if !config.prompt.is_empty() {
            cmd.arg(&config.prompt);
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

    #[ah_test_utils::logged_test]
    fn test_parse_version() {
        let output = "codex 0.46.0";
        let result = CodexAgent::parse_version(output);
        assert!(result.is_ok());
        let version = result.unwrap();
        assert_eq!(version.version, "0.46.0");
    }

    #[ah_test_utils::logged_test]
    fn test_parse_version_simple() {
        let output = "0.46.0";
        let result = CodexAgent::parse_version(output);
        assert!(result.is_ok());
        let version = result.unwrap();
        assert_eq!(version.version, "0.46.0");
    }

    #[ah_test_utils::logged_tokio_test]
    async fn test_agent_name() {
        let agent = CodexAgent::new();
        assert_eq!(agent.name(), "codex");
    }

    #[ah_test_utils::logged_tokio_test]
    async fn test_config_dir() {
        let agent = CodexAgent::new();
        let home = PathBuf::from("/home/user");
        let config = agent.config_dir(&home);
        assert_eq!(config, PathBuf::from("/home/user/.config/codex"));
    }
}
