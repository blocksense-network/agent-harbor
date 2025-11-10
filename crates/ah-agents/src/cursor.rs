// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

/// Cursor CLI agent implementation
use crate::session::{export_directory, import_directory};
use crate::traits::*;
use async_trait::async_trait;
use regex::Regex;
use std::path::{Path, PathBuf};
use tokio::process::{Child, Command};
use tracing::{debug, info, warn};

/// Cursor CLI agent executor
pub struct CursorAgent {
    binary_path: String,
}

impl CursorAgent {
    pub fn new() -> Self {
        Self {
            binary_path: "cursor-agent".to_string(),
        }
    }

    /// Parse version from `cursor-agent --version` output
    fn parse_version(output: &str) -> AgentResult<AgentVersion> {
        // Expected formats: "cursor-agent version 2025.09.18-39624ef" or "2025.09.18-39624ef"
        let version_regex = Regex::new(r"(\d{4}\.\d{2}\.\d{2}-[a-f0-9]+)").map_err(|e| {
            AgentError::VersionDetectionFailed(format!("Regex compilation failed: {}", e))
        })?;

        if let Some(caps) = version_regex.captures(output) {
            let version = caps[1].to_string();
            Ok(AgentVersion {
                version,
                commit: None, // Version already includes commit hash
                release_date: None,
            })
        } else {
            Err(AgentError::VersionDetectionFailed(format!(
                "Could not parse version from output: {}",
                output
            )))
        }
    }

    /// Set up Cursor CLI configuration for authentication
    async fn setup_auth_config(&self, home_dir: &Path, api_key: Option<&str>) -> AgentResult<()> {
        debug!("Setting up Cursor CLI authentication configuration");

        // Create Cursor configuration directory structure
        let cursor_config_dir = Self::get_cursor_global_storage_path(home_dir);
        tokio::fs::create_dir_all(&cursor_config_dir).await.map_err(|e| {
            AgentError::ConfigCreationFailed(format!(
                "Failed to create Cursor config directory: {}",
                e
            ))
        })?;

        // If API key is provided explicitly, use it
        if let Some(_key) = api_key {
            info!("Using provided API key for Cursor CLI authentication");
            // The API key will be set via environment variable or command line flag
            // No additional configuration file setup needed for basic auth
        } else {
            // First extract credentials from the user's system Cursor database
            let system_home = std::env::var("HOME").unwrap_or_default();
            let system_cursor_dir = Self::get_cursor_global_storage_path(Path::new(&system_home));
            let system_db_path = system_cursor_dir.join("state.vscdb");

            // Extract all cursorAuth data from the system database
            let extracted_credentials = if system_db_path.exists() {
                match Self::extract_all_cursor_auth_data(&system_db_path) {
                    Ok(creds) => {
                        info!(
                            "Extracted {} authentication records from system Cursor database",
                            creds.len()
                        );
                        creds
                    }
                    Err(e) => {
                        warn!("Failed to extract credentials from system database: {}", e);
                        std::collections::HashMap::new()
                    }
                }
            } else {
                warn!("System Cursor database not found at: {:?}", system_db_path);
                std::collections::HashMap::new()
            };

            // Copy our minimal authentication database template
            let template_db_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap_or(Path::new("."))
                .join("resources/cursor/minimal-auth-db.vscdb");

            let target_db_path = cursor_config_dir.join("state.vscdb");

            if template_db_path.exists() {
                info!("Copying minimal Cursor authentication database template to sandbox");
                tokio::fs::copy(&template_db_path, &target_db_path).await.map_err(|e| {
                    AgentError::ConfigCreationFailed(format!(
                        "Failed to copy Cursor auth database template: {}",
                        e
                    ))
                })?;

                // Populate the copied database with the extracted credentials
                if !extracted_credentials.is_empty() {
                    match Self::populate_database_with_credentials(
                        &target_db_path,
                        &extracted_credentials,
                    ) {
                        Ok(_) => info!(
                            "Successfully populated sandbox database with {} authentication records",
                            extracted_credentials.len()
                        ),
                        Err(e) => warn!(
                            "Failed to populate sandbox database with credentials: {}",
                            e
                        ),
                    }
                }

                // With the database populated, Cursor CLI should find credentials automatically.
                // We don't need to use --api-key unless explicitly provided via config.
                debug!("Cursor database populated with authentication credentials");
            } else {
                warn!(
                    "Cursor authentication database template not found at: {:?}",
                    template_db_path
                );
            }
        }

        debug!("Cursor CLI authentication configuration completed");
        Ok(())
    }
}

impl Default for CursorAgent {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AgentExecutor for CursorAgent {
    fn name(&self) -> &'static str {
        "cursor-cli"
    }

    async fn detect_version(&self) -> AgentResult<AgentVersion> {
        debug!("Detecting Cursor CLI version");

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
            "Preparing Cursor CLI launch with prompt: {:?}",
            config
                .prompt
                .as_ref()
                .map(|p| p.chars().take(50).collect::<String>())
                .unwrap_or_default()
        );

        // Check if we're using a custom HOME directory
        let using_custom_home = if let Ok(system_home) = std::env::var("HOME") {
            PathBuf::from(system_home) != config.home_dir
        } else {
            true // If no system HOME, we're definitely using custom
        };

        // Set up authentication config if copying credentials and using custom HOME
        if config.copy_credentials && using_custom_home {
            self.setup_auth_config(&config.home_dir, config.api_key.as_deref()).await?;
        }

        let mut cmd = tokio::process::Command::new(&self.binary_path);

        // Set custom HOME directory
        cmd.env("HOME", &config.home_dir);

        // Set current directory
        cmd.current_dir(&config.working_dir);

        // Use --print for non-interactive mode when not in interactive mode
        if !config.interactive {
            cmd.arg("--print");
        }

        // Add API key if specified (command-line flag takes precedence over env var)
        if let Some(api_key) = &config.api_key {
            cmd.arg("--api-key").arg(api_key);
        } else if let Ok(env_key) = std::env::var("CURSOR_API_KEY") {
            cmd.env("CURSOR_API_KEY", env_key);
        }

        // Add additional environment variables
        for (key, value) in &config.env_vars {
            cmd.env(key, value);
        }

        // Configure output format
        if config.json_output {
            cmd.arg("--output-format");
            cmd.arg("json");
        }

        // NOTE: Don't set up piped stdio for cursor-agent - let it inherit stdio from parent
        // This works better with exec() and doesn't seem to be needed for cursor-agent

        // Add the prompt as argument
        if let Some(prompt) = &config.prompt {
            if !prompt.is_empty() {
                cmd.arg(prompt);
            }
        }

        debug!("Cursor CLI command prepared successfully");
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

        debug!("Cursor CLI process spawned successfully");
        Ok(child)
    }

    async fn export_session(&self, home_dir: &Path) -> AgentResult<PathBuf> {
        let config_dir = self.config_dir(home_dir);
        let archive_path = home_dir.join("cursor-session.tar.gz");

        export_directory(&config_dir, &archive_path).await?;
        Ok(archive_path)
    }

    async fn import_session(&self, session_archive: &Path, home_dir: &Path) -> AgentResult<()> {
        let config_dir = self.config_dir(home_dir);
        import_directory(session_archive, &config_dir).await
    }

    fn parse_output(&self, raw_output: &[u8]) -> AgentResult<Vec<AgentEvent>> {
        // Parse Cursor CLI output into normalized events
        let output = String::from_utf8_lossy(raw_output);
        let mut events = Vec::new();

        // Basic line-by-line parsing for Cursor CLI output
        for line in output.lines() {
            if line.contains("Thinking") || line.contains("reasoning") {
                events.push(AgentEvent::Thinking {
                    content: line.to_string(),
                });
            } else if line.contains("Tool") || line.contains("Command") || line.contains("Running")
            {
                events.push(AgentEvent::ToolUse {
                    tool_name: "shell".to_string(),
                    arguments: serde_json::json!({"command": line}),
                });
            } else if line.contains("Error") || line.contains("error") || line.contains("Failed") {
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
        home.join(".cursor")
    }

    async fn get_user_api_key(&self) -> AgentResult<Option<String>> {
        // For Cursor CLI, API key extraction from database doesn't work as expected.
        // The extracted tokens are session tokens, not API keys that work with --api-key flag.
        Ok(None)
    }

    fn credential_paths(&self) -> Vec<PathBuf> {
        vec![]
    }
}

impl CursorAgent {
    /// Get the platform-specific Cursor globalStorage directory path
    fn get_cursor_global_storage_path(home_dir: &Path) -> PathBuf {
        // Cursor stores its globalStorage in different locations per platform
        if cfg!(target_os = "macos") {
            home_dir.join("Library/Application Support/Cursor/User/globalStorage")
        } else if cfg!(target_os = "windows") {
            home_dir.join("AppData/Roaming/Cursor/User/globalStorage")
        } else {
            // Linux and other Unix-like systems
            home_dir.join(".config/Cursor/User/globalStorage")
        }
    }

    /// Extract all cursorAuth data from a Cursor database
    fn extract_all_cursor_auth_data(
        db_path: &Path,
    ) -> AgentResult<std::collections::HashMap<String, String>> {
        use rusqlite::Connection;

        let conn = Connection::open(db_path).map_err(|e| {
            AgentError::Other(anyhow::anyhow!("Failed to open Cursor database: {}", e))
        })?;

        let mut stmt = conn
            .prepare("SELECT key, value FROM ItemTable WHERE key LIKE 'cursorAuth/%'")
            .map_err(|e| {
                AgentError::Other(anyhow::anyhow!("Failed to prepare SQL statement: {}", e))
            })?;

        let mut cursor_auth_data = std::collections::HashMap::new();
        let rows = stmt
            .query_map([], |row| {
                let key: String = row.get(0)?;
                let value: String = row.get(1)?;
                Ok((key, value))
            })
            .map_err(|e| {
                AgentError::Other(anyhow::anyhow!("Failed to query cursorAuth data: {}", e))
            })?;

        for (key, value) in rows.flatten() {
            cursor_auth_data.insert(key, value);
        }

        Ok(cursor_auth_data)
    }

    /// Populate a database with cursorAuth credentials
    fn populate_database_with_credentials(
        db_path: &Path,
        credentials: &std::collections::HashMap<String, String>,
    ) -> AgentResult<()> {
        use rusqlite::Connection;

        let conn = Connection::open(db_path).map_err(|e| {
            AgentError::Other(anyhow::anyhow!("Failed to open target database: {}", e))
        })?;

        let mut stmt = conn
            .prepare("INSERT OR REPLACE INTO ItemTable (key, value) VALUES (?, ?)")
            .map_err(|e| {
                AgentError::Other(anyhow::anyhow!("Failed to prepare INSERT statement: {}", e))
            })?;

        for (key, value) in credentials {
            stmt.execute([key, value]).map_err(|e| {
                AgentError::Other(anyhow::anyhow!(
                    "Failed to insert credential {}: {}",
                    key,
                    e
                ))
            })?;
        }

        Ok(())
    }

    /// Check Cursor CLI login status and extract authentication token
    ///
    /// Note: This extracts the session access token from Cursor's local database.
    /// This is different from API keys that may be used for CI/CD authentication.
    /// The extracted token may not work with Cursor CLI's --api-key flag.
    pub fn check_cursor_login_status(&self) -> AgentResult<Option<String>> {
        use rusqlite::Connection;

        // Determine the correct database path based on platform
        let db_path = if cfg!(target_os = "macos") {
            std::env::var("HOME").map(|home| {
                PathBuf::from(home)
                    .join("Library/Application Support/Cursor/User/globalStorage/state.vscdb")
            })
        } else if cfg!(target_os = "windows") {
            std::env::var("APPDATA").map(|app_data| {
                PathBuf::from(app_data).join("Cursor\\User\\globalStorage\\state.vscdb")
            })
        } else {
            // linux and others
            std::env::var("HOME").map(|home| {
                PathBuf::from(home).join(".config/Cursor/User/globalStorage/state.vscdb")
            })
        };

        let db_path = match db_path {
            Ok(path) => path,
            Err(_) => return Ok(None),
        };

        if !db_path.exists() {
            return Ok(None);
        }

        let conn = Connection::open(&db_path).map_err(|e| {
            AgentError::Other(anyhow::anyhow!("Failed to open Cursor database: {}", e))
        })?;

        // First try to get all keys to see what's available (not just cursorAuth)
        let mut stmt = conn.prepare("SELECT key, value FROM ItemTable").map_err(|e| {
            AgentError::Other(anyhow::anyhow!("Failed to prepare SQL statement: {}", e))
        })?;

        let mut all_data = std::collections::HashMap::new();
        let rows = stmt
            .query_map([], |row| {
                let key: String = row.get(0)?;
                let value: String = row.get(1)?;
                Ok((key, value))
            })
            .map_err(|e| {
                AgentError::Other(anyhow::anyhow!("Failed to query database data: {}", e))
            })?;

        for (key, value) in rows.flatten() {
            all_data.insert(key, value);
        }

        // Filter to cursorAuth keys for logging
        let cursor_auth_keys: Vec<_> =
            all_data.keys().filter(|k| k.starts_with("cursorAuth/")).collect();
        debug!("Found cursorAuth keys: {:?}", cursor_auth_keys);

        // Also check for any API key related keys
        let api_related_keys: Vec<_> = all_data
            .keys()
            .filter(|k| k.contains("api") || k.contains("key") || k.contains("token"))
            .collect();
        debug!("Found API/key/token related keys: {:?}", api_related_keys);

        // Try different token types in order of preference
        // 1. Look for an API key first (if it exists)
        if let Some(api_key) = all_data.get("cursorAuth/apiKey") {
            debug!("Found API key in database");
            return Ok(Some(api_key.clone()));
        }

        // 2. Try access token (what we were using before)
        if let Some(access_token) = all_data.get("cursorAuth/accessToken") {
            debug!(
                "Found access token in database: {}...{}",
                &access_token[..20],
                &access_token[access_token.len() - 20..]
            );
            return Ok(Some(access_token.clone()));
        }

        // 3. Check if there's a refresh token that might work
        if let Some(refresh_token) = all_data.get("cursorAuth/refreshToken") {
            debug!("Found refresh token in database");
            return Ok(Some(refresh_token.clone()));
        }

        debug!("No suitable token found in cursorAuth data");
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version() {
        let output = "cursor-agent version 2025.09.18-39624ef";
        let result = CursorAgent::parse_version(output);
        assert!(result.is_ok());
        let version = result.unwrap();
        assert_eq!(version.version, "2025.09.18-39624ef");
    }

    #[test]
    fn test_parse_version_simple() {
        let output = "2025.09.18-39624ef";
        let result = CursorAgent::parse_version(output);
        assert!(result.is_ok());
        let version = result.unwrap();
        assert_eq!(version.version, "2025.09.18-39624ef");
    }

    #[tokio::test]
    async fn test_agent_name() {
        let agent = CursorAgent::new();
        assert_eq!(agent.name(), "cursor-cli");
    }

    #[tokio::test]
    async fn test_config_dir() {
        let agent = CursorAgent::new();
        let home = PathBuf::from("/home/user");
        let config = agent.config_dir(&home);
        assert_eq!(config, PathBuf::from("/home/user/.cursor"));
    }
}
