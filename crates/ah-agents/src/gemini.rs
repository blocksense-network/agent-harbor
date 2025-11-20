// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use crate::common::AgentStatus;
use crate::session::{export_directory, import_directory};
use crate::traits::*;
use async_trait::async_trait;
use regex::Regex;
use std::path::{Path, PathBuf};
use tokio::process::Command;
use tracing::{debug, info, warn};

pub struct GeminiAgent {
    binary_path: String,
}

impl GeminiAgent {
    pub fn new() -> Self {
        let binary_path = which::which("gemini")
            .ok()
            .and_then(|p| p.to_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "gemini".to_string());

        Self { binary_path }
    }

    /// Parse version from `gemini --version` output
    fn parse_version(output: &str) -> AgentResult<AgentVersion> {
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

    /// Read authentication type from Gemini settings.json
    ///
    /// Checks the security.auth.selectedType field to determine which API key
    /// environment variable should be used:
    /// - "gemini-api-key" -> GEMINI_API_KEY
    /// - "vertex-ai" -> GOOGLE_API_KEY
    async fn get_auth_type(&self, home_dir: &Path) -> AgentResult<Option<String>> {
        let config_dir = self.config_dir(home_dir);
        let settings_path = config_dir.join("settings.json");

        if !settings_path.exists() {
            debug!("No settings.json found at {:?}", settings_path);
            return Ok(None);
        }

        let content = tokio::fs::read_to_string(&settings_path).await.map_err(|e| {
            AgentError::ConfigurationError(format!("Failed to read settings file: {}", e))
        })?;

        let settings: serde_json::Value = serde_json::from_str(&content).map_err(|e| {
            AgentError::ConfigurationError(format!("Failed to parse settings.json: {}", e))
        })?;

        // Navigate to security.auth.selectedType
        let auth_type = settings
            .get("security")
            .and_then(|s| s.get("auth"))
            .and_then(|a| a.get("selectedType"))
            .and_then(|t| t.as_str())
            .map(|s| s.to_string());

        debug!(
            "Found authentication type in settings.json: {:?}",
            auth_type
        );
        Ok(auth_type)
    }

    /// Detect authentication method and source details synchronously
    ///
    /// Returns a tuple of (auth_method, auth_source) providing information about
    /// the authentication mechanism being used. This is a synchronous method
    /// to avoid hanging in status checks.
    fn detect_auth_details_sync(&self) -> (String, String) {
        let home_dir = if let Ok(home) = std::env::var("HOME") {
            PathBuf::from(home)
        } else {
            return ("Unknown".to_string(), "Unknown".to_string());
        };

        // Try to read authentication type from settings.json synchronously
        let config_dir = self.config_dir(&home_dir);
        let settings_path = config_dir.join("settings.json");

        let auth_type = if settings_path.exists() {
            match std::fs::read_to_string(&settings_path) {
                Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
                    Ok(settings) => settings
                        .get("security")
                        .and_then(|s| s.get("auth"))
                        .and_then(|a| a.get("selectedType"))
                        .and_then(|t| t.as_str())
                        .map(|s| s.to_string()),
                    Err(_) => None,
                },
                Err(_) => None,
            }
        } else {
            None
        };

        // Check authentication based on configured type or fallback to env vars
        if let Some(auth_type) = auth_type {
            match auth_type.as_str() {
                "gemini-api-key" => {
                    if std::env::var("GEMINI_API_KEY").is_ok() {
                        return (auth_type, "GEMINI_API_KEY".to_string());
                    }
                }
                "vertex-ai" => {
                    if std::env::var("GOOGLE_API_KEY").is_ok() {
                        return (auth_type, "GOOGLE_API_KEY".to_string());
                    }
                }
                "oauth-personal" => {
                    let oauth_creds_path = config_dir.join("oauth_creds.json");
                    if oauth_creds_path.exists() {
                        return (auth_type, oauth_creds_path.to_string_lossy().to_string());
                    }
                }
                _ => {}
            }
            return (auth_type, "configured but invalid".to_string());
        }

        // Fall back to checking environment variables without configuration
        if std::env::var("GEMINI_API_KEY").is_ok() {
            return ("gemini-api-key".to_string(), "GEMINI_API_KEY".to_string());
        }
        if std::env::var("GOOGLE_API_KEY").is_ok() {
            return ("vertex-ai".to_string(), "GOOGLE_API_KEY".to_string());
        }

        // Check for OAuth credentials
        let oauth_creds_path = config_dir.join("oauth_creds.json");
        if oauth_creds_path.exists() {
            return (
                "oauth-personal".to_string(),
                oauth_creds_path.to_string_lossy().to_string(),
            );
        }

        ("Unknown".to_string(), "Unknown".to_string())
    }

    /// Set up Gemini configuration to skip onboarding prompts
    ///
    /// This modifies or creates the settings.json file to disable:
    /// - Auto-update checks
    /// - Update notifications
    /// - IDE integration nudges
    /// - UI tips
    ///
    /// This is useful for automated/unattended agent execution.
    async fn setup_onboarding_skip(&self, home_dir: &Path, _working_dir: &Path) -> AgentResult<()> {
        let config_dir = self.config_dir(home_dir);

        // Create config directory if it doesn't exist
        tokio::fs::create_dir_all(&config_dir).await.map_err(|e| {
            AgentError::ConfigurationError(format!("Failed to create config directory: {}", e))
        })?;

        let settings_path = config_dir.join("settings.json");

        let mut settings: serde_json::Value = if settings_path.exists() {
            let content = tokio::fs::read_to_string(&settings_path).await.map_err(|e| {
                AgentError::ConfigurationError(format!("Failed to read settings file: {}", e))
            })?;

            serde_json::from_str(&content).unwrap_or_else(|e| {
                warn!(
                    "Failed to parse existing settings.json: {}. Creating new settings.",
                    e
                );
                serde_json::json!({})
            })
        } else {
            serde_json::json!({})
        };

        // Define onboarding skip configuration
        let onboarding_settings = serde_json::json!({
            "general": {
                "disableAutoUpdate": true,
                "disableUpdateNag": true
            },
            "ide": {
                "enabled": false,
                "hasSeenNudge": true
            },
            "ui": {
                "hideTips": true
            }
        });

        // Deep merge the settings
        if let serde_json::Value::Object(ref mut map) = settings {
            if let serde_json::Value::Object(onboarding_map) = onboarding_settings {
                for (key, value) in onboarding_map {
                    if let Some(serde_json::Value::Object(existing_obj)) = map.get_mut(&key) {
                        if let serde_json::Value::Object(new_obj) = value {
                            // Merge nested objects
                            for (nested_key, nested_value) in new_obj {
                                existing_obj.insert(nested_key, nested_value);
                            }
                        }
                    } else {
                        // Insert new top-level key
                        map.insert(key, value);
                    }
                }
            }
        }

        // Write the updated settings back to file
        let settings_json = serde_json::to_string_pretty(&settings).map_err(|e| {
            AgentError::ConfigurationError(format!("Failed to serialize settings: {}", e))
        })?;

        tokio::fs::write(&settings_path, settings_json).await.map_err(|e| {
            AgentError::ConfigurationError(format!("Failed to write settings file: {}", e))
        })?;

        info!(
            "Gemini onboarding skip configuration written to {:?}",
            settings_path
        );
        Ok(())
    }

    /// Get structured Gemini CLI status information
    ///
    /// This function returns comprehensive status information in a structured format
    /// that can be easily consumed by health checkers and other tools.
    ///
    /// Returns AgentStatus with detailed information about:
    /// - CLI availability and version
    /// - Authentication status and method
    /// - Authentication source information
    /// - Any errors encountered
    pub async fn get_gemini_status(&self) -> AgentStatus {
        // Check CLI availability by detecting version
        let (available, version, error) = match self.detect_version().await {
            Ok(version_info) => (true, Some(version_info.version), None),
            Err(AgentError::AgentNotFound(_)) => (
                false,
                None,
                Some("Gemini CLI not found in PATH".to_string()),
            ),
            Err(e) => (false, None, Some(e.to_string())),
        };

        if !available {
            return AgentStatus {
                available: false,
                version: None,
                authenticated: false,
                auth_method: None,
                auth_source: None,
                error,
            };
        }

        // Check authentication status using synchronous method to avoid hanging
        let (auth_method, auth_source) = self.detect_auth_details_sync();
        let authenticated = auth_method != "Unknown" && auth_source != "Unknown";

        AgentStatus {
            available,
            version,
            authenticated,
            auth_method: if authenticated {
                Some(auth_method)
            } else {
                None
            },
            auth_source: if authenticated {
                Some(auth_source)
            } else {
                None
            },
            error,
        }
    }
}

impl Default for GeminiAgent {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AgentExecutor for GeminiAgent {
    fn name(&self) -> &'static str {
        "gemini"
    }

    async fn detect_version(&self) -> AgentResult<AgentVersion> {
        debug!("Detecting Gemini CLI version");

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
            "Preparing Gemini CLI launch with prompt: {:?}",
            config.prompt.as_ref().map(|p| p.chars().take(50).collect::<String>())
        );

        let env_gemini_key = std::env::var("GEMINI_API_KEY").ok();
        let env_google_key = std::env::var("GOOGLE_API_KEY").ok();
        if env_gemini_key.is_some() {
            debug!("Detected GEMINI_API_KEY from environment");
        }
        if env_google_key.is_some() {
            debug!("Detected GOOGLE_API_KEY from environment");
        }

        // Determine if we're using a custom HOME directory
        let using_custom_home = if let Ok(system_home) = std::env::var("HOME") {
            PathBuf::from(system_home) != config.home_dir
        } else {
            true // If no system HOME, we're definitely using custom
        };

        // Copy credentials if requested and home_dir differs from system HOME
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

        // Set up onboarding skip configuration for custom HOME
        if using_custom_home {
            debug!(
                "Creating Gemini configuration to skip onboarding in {:?}",
                config.home_dir
            );
            self.setup_onboarding_skip(&config.home_dir, &config.working_dir).await?;
        }

        // Read authentication type from settings.json after credentials have been copied
        let auth_type = if config.copy_credentials || using_custom_home {
            self.get_auth_type(&config.home_dir).await?
        } else {
            None
        };

        let mut cmd = tokio::process::Command::new(&self.binary_path);

        cmd.env("HOME", &config.home_dir);

        cmd.current_dir(&config.working_dir);

        // Add custom API server if specified
        // Note: Gemini CLI might use different environment variables than OpenAI
        // Research needed: GEMINI_API_BASE or similar
        if let Some(_api_server) = &config.api_server {
            // Skipping for now, as my research on Gemini's `--proxy` flag concluded
            // with bad behavior of gemini when using custom API servers
            warn!("Custom API server is not supported yet");
        }

        // Add API key based on authentication type from settings.json
        if let Some(config_api_key) = &config.api_key {
            // Determine which environment variable to use based on auth type
            let (env_var_name, existing_env_key) = match auth_type.as_deref() {
                Some("vertex-ai") => {
                    info!("Using GOOGLE_API_KEY for vertex-ai authentication");
                    ("GOOGLE_API_KEY", &env_google_key)
                }
                Some("gemini-api-key") | None => {
                    info!("Using GEMINI_API_KEY for gemini-api-key authentication");
                    ("GEMINI_API_KEY", &env_gemini_key)
                }
                Some(other) => {
                    warn!(
                        "Unknown authentication type '{}', defaulting to GEMINI_API_KEY",
                        other
                    );
                    ("GEMINI_API_KEY", &env_gemini_key)
                }
            };

            // Check if config API key differs from environment variable
            if let Some(ref env_key) = existing_env_key {
                if env_key != config_api_key {
                    info!(
                        "{} differs from environment variable, using config value",
                        env_var_name
                    );
                }
            }

            cmd.env(env_var_name, config_api_key);
        }

        // Add additional environment variables
        for (key, value) in &config.env_vars {
            cmd.env(key, value);
        }

        // Configure stdio based on interactive mode
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

        if config.interactive {
            cmd.arg("--prompt-interactive");
        }

        if let Some(model) = &config.model {
            cmd.arg("--model");
            cmd.arg(model);
        }

        if config.unrestricted {
            cmd.arg("--approval-mode");
            cmd.arg("yolo");
        }

        if config.json_output {
            cmd.arg("--output-format").arg("json");
        } else {
            cmd.arg("--output-format").arg("text");
        }

        if let Some(prompt) = &config.prompt {
            if !prompt.is_empty() {
                cmd.arg(prompt);
            }
        }

        debug!("Gemini CLI command prepared successfully");
        Ok(cmd)
    }

    /// Platform-specific credential paths for Gemini CLI
    ///
    /// Gemini stores credentials and configuration in ~/.gemini/:
    /// - google_accounts.json: Google account authentication
    /// - oauth_creds.json: OAuth credentials
    /// - settings.json: User preferences
    fn credential_paths(&self) -> Vec<PathBuf> {
        vec![
            PathBuf::from(".gemini/google_accounts.json"),
            PathBuf::from(".gemini/oauth_creds.json"),
            PathBuf::from(".gemini/settings.json"),
        ]
    }

    // 1. Check direct environment variable (GEMINI_API_KEY)
    // 2. Check direct environment variable (GOOGLE_API_KEY)
    // 3. Try to extract OAuth tokens from Gemini's credential files and exchange them ( not implemented yet )
    async fn get_user_api_key(&self) -> AgentResult<Option<String>> {
        if let Ok(gemini_api_key) = std::env::var("GEMINI_API_KEY") {
            if !gemini_api_key.trim().is_empty() {
                debug!("Found Gemini API key in environment variable");
                return Ok(Some(gemini_api_key));
            }
        }

        if let Ok(google_api_key) = std::env::var("GOOGLE_API_KEY") {
            if !google_api_key.trim().is_empty() {
                debug!("Found Google API key in environment variable");
                return Ok(Some(google_api_key));
            }
        }
        // TODO: On first research, could not find a way to extract OAuth tokens from Gemini's credential files

        debug!("No user API key found");
        return Ok(None);
    }

    async fn export_session(&self, home_dir: &Path) -> AgentResult<PathBuf> {
        info!("Exporting Gemini session from {:?}", home_dir);

        let config_dir = self.config_dir(home_dir);
        let archive_path = home_dir.join("gemini-session.tar.gz");

        export_directory(&config_dir, &archive_path).await?;

        debug!("Gemini session exported to {:?}", archive_path);
        Ok(archive_path)
    }

    async fn import_session(&self, session_archive: &Path, home_dir: &Path) -> AgentResult<()> {
        info!(
            "Importing Gemini session from {:?} to {:?}",
            session_archive, home_dir
        );

        let config_dir = self.config_dir(home_dir);
        import_directory(session_archive, &config_dir).await?;

        debug!("Gemini session imported successfully");
        Ok(())
    }

    fn parse_output(&self, _raw_output: &[u8]) -> AgentResult<Vec<AgentEvent>> {
        // TODO: Implement actual Gemini output parsing based on its format
        debug!("Parsing Gemini CLI output not yet implemented");

        Ok(Vec::new())
    }

    fn config_dir(&self, home: &Path) -> PathBuf {
        home.join(".gemini")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version() {
        let output = "gemini 1.0.0";
        let result = GeminiAgent::parse_version(output);
        assert!(result.is_ok());
        let version = result.unwrap();
        assert_eq!(version.version, "1.0.0");
    }

    #[test]
    fn test_parse_version_with_prefix() {
        let output = "gemini version 0.2.2";
        let result = GeminiAgent::parse_version(output);
        assert!(result.is_ok());
        let version = result.unwrap();
        assert_eq!(version.version, "0.2.2");
    }

    #[test]
    fn test_parse_version_simple() {
        let output = "1.2.3";
        let result = GeminiAgent::parse_version(output);
        assert!(result.is_ok());
        let version = result.unwrap();
        assert_eq!(version.version, "1.2.3");
    }

    #[tokio::test]
    async fn test_agent_name() {
        let agent = GeminiAgent::new();
        assert_eq!(agent.name(), "gemini");
    }

    #[tokio::test]
    async fn test_config_dir() {
        let agent = GeminiAgent::new();
        let home = PathBuf::from("/home/user");
        let config = agent.config_dir(&home);
        assert_eq!(config, PathBuf::from("/home/user/.gemini"));
    }

    #[tokio::test]
    async fn test_get_auth_type_gemini_api_key() {
        let agent = GeminiAgent::new();
        let temp_dir = tempfile::tempdir().unwrap();
        let home_dir = temp_dir.path();

        let gemini_dir = home_dir.join(".gemini");
        tokio::fs::create_dir_all(&gemini_dir).await.unwrap();

        // Create settings.json with gemini-api-key auth type
        let settings_content = serde_json::json!({
            "security": {
                "auth": {
                    "selectedType": "gemini-api-key"
                }
            }
        });
        let settings_path = gemini_dir.join("settings.json");
        tokio::fs::write(
            &settings_path,
            serde_json::to_string_pretty(&settings_content).unwrap(),
        )
        .await
        .unwrap();

        let auth_type = agent.get_auth_type(home_dir).await.unwrap();
        assert_eq!(auth_type, Some("gemini-api-key".to_string()));
    }

    #[tokio::test]
    async fn test_get_auth_type_vertex_ai() {
        let agent = GeminiAgent::new();
        let temp_dir = tempfile::tempdir().unwrap();
        let home_dir = temp_dir.path();

        let gemini_dir = home_dir.join(".gemini");
        tokio::fs::create_dir_all(&gemini_dir).await.unwrap();

        // Create settings.json with vertex-ai auth type
        let settings_content = serde_json::json!({
            "security": {
                "auth": {
                    "selectedType": "vertex-ai"
                }
            }
        });
        let settings_path = gemini_dir.join("settings.json");
        tokio::fs::write(
            &settings_path,
            serde_json::to_string_pretty(&settings_content).unwrap(),
        )
        .await
        .unwrap();

        let auth_type = agent.get_auth_type(home_dir).await.unwrap();
        assert_eq!(auth_type, Some("vertex-ai".to_string()));
    }

    #[tokio::test]
    async fn test_get_auth_type_no_settings() {
        let agent = GeminiAgent::new();
        let temp_dir = tempfile::tempdir().unwrap();
        let home_dir = temp_dir.path();

        let auth_type = agent.get_auth_type(home_dir).await.unwrap();
        assert_eq!(auth_type, None);
    }

    #[tokio::test]
    async fn test_get_auth_type_malformed_settings() {
        let agent = GeminiAgent::new();
        let temp_dir = tempfile::tempdir().unwrap();
        let home_dir = temp_dir.path();

        let gemini_dir = home_dir.join(".gemini");
        tokio::fs::create_dir_all(&gemini_dir).await.unwrap();

        // Create malformed settings.json
        let settings_path = gemini_dir.join("settings.json");
        tokio::fs::write(&settings_path, "{ invalid json }").await.unwrap();

        let result = agent.get_auth_type(home_dir).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_credential_paths() {
        let agent = GeminiAgent::new();
        let paths = agent.credential_paths();
        assert_eq!(paths.len(), 3);
        assert!(paths.contains(&PathBuf::from(".gemini/google_accounts.json")));
        assert!(paths.contains(&PathBuf::from(".gemini/oauth_creds.json")));
        assert!(paths.contains(&PathBuf::from(".gemini/settings.json")));
    }

    // Tests for detect_auth_details_sync method

    #[test]
    #[serial_test::serial(env)]
    fn test_detect_auth_details_sync_gemini_api_key() {
        use crate::test_support::EnvVarGuard;

        // Set up temporary home directory for test
        let temp_dir = tempfile::tempdir().unwrap();
        let home_dir = temp_dir.path();
        let _home_guard = EnvVarGuard::set("HOME", home_dir);

        // Create .gemini directory and settings.json
        let gemini_dir = home_dir.join(".gemini");
        std::fs::create_dir_all(&gemini_dir).unwrap();

        let settings_content = serde_json::json!({
            "security": {
                "auth": {
                    "selectedType": "gemini-api-key"
                }
            }
        });
        let settings_path = gemini_dir.join("settings.json");
        std::fs::write(
            &settings_path,
            serde_json::to_string_pretty(&settings_content).unwrap(),
        )
        .unwrap();

        // Set environment variable
        let _api_key_guard = EnvVarGuard::set("GEMINI_API_KEY", "test-api-key");

        let agent = GeminiAgent::new();
        let (auth_method, auth_source) = agent.detect_auth_details_sync();

        assert_eq!(auth_method, "gemini-api-key");
        assert_eq!(auth_source, "GEMINI_API_KEY");
    }

    #[test]
    #[serial_test::serial(env)]
    fn test_detect_auth_details_sync_vertex_ai() {
        use crate::test_support::EnvVarGuard;

        let temp_dir = tempfile::tempdir().unwrap();
        let home_dir = temp_dir.path();
        let _home_guard = EnvVarGuard::set("HOME", home_dir);

        let gemini_dir = home_dir.join(".gemini");
        std::fs::create_dir_all(&gemini_dir).unwrap();

        let settings_content = serde_json::json!({
            "security": {
                "auth": {
                    "selectedType": "vertex-ai"
                }
            }
        });
        let settings_path = gemini_dir.join("settings.json");
        std::fs::write(
            &settings_path,
            serde_json::to_string_pretty(&settings_content).unwrap(),
        )
        .unwrap();

        let _google_api_key_guard = EnvVarGuard::set("GOOGLE_API_KEY", "test-google-key");

        let agent = GeminiAgent::new();
        let (auth_method, auth_source) = agent.detect_auth_details_sync();

        assert_eq!(auth_method, "vertex-ai");
        assert_eq!(auth_source, "GOOGLE_API_KEY");
    }

    #[test]
    #[serial_test::serial(env)]
    fn test_detect_auth_details_sync_oauth_personal() {
        use crate::test_support::EnvVarGuard;

        let temp_dir = tempfile::tempdir().unwrap();
        let home_dir = temp_dir.path();
        let _home_guard = EnvVarGuard::set("HOME", home_dir);

        let gemini_dir = home_dir.join(".gemini");
        std::fs::create_dir_all(&gemini_dir).unwrap();

        let settings_content = serde_json::json!({
            "security": {
                "auth": {
                    "selectedType": "oauth-personal"
                }
            }
        });
        let settings_path = gemini_dir.join("settings.json");
        std::fs::write(
            &settings_path,
            serde_json::to_string_pretty(&settings_content).unwrap(),
        )
        .unwrap();

        // Create oauth_creds.json file
        let oauth_creds_path = gemini_dir.join("oauth_creds.json");
        std::fs::write(&oauth_creds_path, r#"{"token": "test"}"#).unwrap();

        let agent = GeminiAgent::new();
        let (auth_method, auth_source) = agent.detect_auth_details_sync();

        assert_eq!(auth_method, "oauth-personal");
        assert_eq!(auth_source, oauth_creds_path.to_string_lossy());
    }

    #[test]
    #[serial_test::serial(env)]
    fn test_detect_auth_details_sync_configured_but_invalid() {
        use crate::test_support::EnvVarGuard;

        let temp_dir = tempfile::tempdir().unwrap();
        let home_dir = temp_dir.path();
        let _home_guard = EnvVarGuard::set("HOME", home_dir);

        // Clean environment variables
        let _gemini_guard = EnvVarGuard::remove("GEMINI_API_KEY");
        let _google_guard = EnvVarGuard::remove("GOOGLE_API_KEY");

        let gemini_dir = home_dir.join(".gemini");
        std::fs::create_dir_all(&gemini_dir).unwrap();

        let settings_content = serde_json::json!({
            "security": {
                "auth": {
                    "selectedType": "gemini-api-key"
                }
            }
        });
        let settings_path = gemini_dir.join("settings.json");
        std::fs::write(
            &settings_path,
            serde_json::to_string_pretty(&settings_content).unwrap(),
        )
        .unwrap();

        let agent = GeminiAgent::new();
        let (auth_method, auth_source) = agent.detect_auth_details_sync();

        assert_eq!(auth_method, "gemini-api-key");
        assert_eq!(auth_source, "configured but invalid");
    }

    #[test]
    #[serial_test::serial(env)]
    fn test_detect_auth_details_sync_fallback_gemini_api_key() {
        use crate::test_support::EnvVarGuard;

        let temp_dir = tempfile::tempdir().unwrap();
        let home_dir = temp_dir.path();
        let _home_guard = EnvVarGuard::set("HOME", home_dir);

        // No settings.json file, so should fallback to env vars
        let _gemini_api_key_guard = EnvVarGuard::set("GEMINI_API_KEY", "test-key");
        let _google_guard = EnvVarGuard::remove("GOOGLE_API_KEY");

        let agent = GeminiAgent::new();
        let (auth_method, auth_source) = agent.detect_auth_details_sync();

        assert_eq!(auth_method, "gemini-api-key");
        assert_eq!(auth_source, "GEMINI_API_KEY");
    }

    #[test]
    #[serial_test::serial(env)]
    fn test_detect_auth_details_sync_fallback_google_api_key() {
        use crate::test_support::EnvVarGuard;

        let temp_dir = tempfile::tempdir().unwrap();
        let home_dir = temp_dir.path();
        let _home_guard = EnvVarGuard::set("HOME", home_dir);

        // No settings.json file, so should fallback to env vars
        let _gemini_guard = EnvVarGuard::remove("GEMINI_API_KEY");
        let _google_api_key_guard = EnvVarGuard::set("GOOGLE_API_KEY", "test-google-key");

        let agent = GeminiAgent::new();
        let (auth_method, auth_source) = agent.detect_auth_details_sync();

        assert_eq!(auth_method, "vertex-ai");
        assert_eq!(auth_source, "GOOGLE_API_KEY");
    }

    #[test]
    #[serial_test::serial(env)]
    fn test_detect_auth_details_sync_fallback_oauth() {
        use crate::test_support::EnvVarGuard;

        let temp_dir = tempfile::tempdir().unwrap();
        let home_dir = temp_dir.path();
        let _home_guard = EnvVarGuard::set("HOME", home_dir);

        // Clean environment variables
        let _gemini_guard = EnvVarGuard::remove("GEMINI_API_KEY");
        let _google_guard = EnvVarGuard::remove("GOOGLE_API_KEY");

        let gemini_dir = home_dir.join(".gemini");
        std::fs::create_dir_all(&gemini_dir).unwrap();

        // Create oauth_creds.json file without settings.json
        let oauth_creds_path = gemini_dir.join("oauth_creds.json");
        std::fs::write(&oauth_creds_path, r#"{"token": "test"}"#).unwrap();

        let agent = GeminiAgent::new();
        let (auth_method, auth_source) = agent.detect_auth_details_sync();

        assert_eq!(auth_method, "oauth-personal");
        assert_eq!(auth_source, oauth_creds_path.to_string_lossy());
    }

    #[test]
    #[serial_test::serial(env)]
    fn test_detect_auth_details_sync_no_auth() {
        use crate::test_support::EnvVarGuard;

        let temp_dir = tempfile::tempdir().unwrap();
        let home_dir = temp_dir.path();
        let _home_guard = EnvVarGuard::set("HOME", home_dir);

        // Clean environment variables
        let _gemini_guard = EnvVarGuard::remove("GEMINI_API_KEY");
        let _google_guard = EnvVarGuard::remove("GOOGLE_API_KEY");

        // Create empty gemini dir but no credentials
        let gemini_dir = home_dir.join(".gemini");
        std::fs::create_dir_all(&gemini_dir).unwrap();

        let agent = GeminiAgent::new();
        let (auth_method, auth_source) = agent.detect_auth_details_sync();

        assert_eq!(auth_method, "Unknown");
        assert_eq!(auth_source, "Unknown");
    }

    #[test]
    #[serial_test::serial(env)]
    fn test_detect_auth_details_sync_no_home() {
        use crate::test_support::EnvVarGuard;

        let _home_guard = EnvVarGuard::remove("HOME");

        let agent = GeminiAgent::new();
        let (auth_method, auth_source) = agent.detect_auth_details_sync();

        assert_eq!(auth_method, "Unknown");
        assert_eq!(auth_source, "Unknown");
    }

    #[test]
    #[serial_test::serial(env)]
    fn test_detect_auth_details_sync_malformed_settings() {
        use crate::test_support::EnvVarGuard;

        let temp_dir = tempfile::tempdir().unwrap();
        let home_dir = temp_dir.path();
        let _home_guard = EnvVarGuard::set("HOME", home_dir);

        let gemini_dir = home_dir.join(".gemini");
        std::fs::create_dir_all(&gemini_dir).unwrap();

        // Create malformed settings.json
        let settings_path = gemini_dir.join("settings.json");
        std::fs::write(&settings_path, "{ invalid json }").unwrap();

        // Should fallback to env vars when settings parsing fails
        let _gemini_api_key_guard = EnvVarGuard::set("GEMINI_API_KEY", "test-key");

        let agent = GeminiAgent::new();
        let (auth_method, auth_source) = agent.detect_auth_details_sync();

        assert_eq!(auth_method, "gemini-api-key");
        assert_eq!(auth_source, "GEMINI_API_KEY");
    }

    #[test]
    #[serial_test::serial(env)]
    fn test_detect_auth_details_sync_unknown_auth_type() {
        use crate::test_support::EnvVarGuard;

        let temp_dir = tempfile::tempdir().unwrap();
        let home_dir = temp_dir.path();
        let _home_guard = EnvVarGuard::set("HOME", home_dir);

        let gemini_dir = home_dir.join(".gemini");
        std::fs::create_dir_all(&gemini_dir).unwrap();

        let settings_content = serde_json::json!({
            "security": {
                "auth": {
                    "selectedType": "unknown-auth-type"
                }
            }
        });
        let settings_path = gemini_dir.join("settings.json");
        std::fs::write(
            &settings_path,
            serde_json::to_string_pretty(&settings_content).unwrap(),
        )
        .unwrap();

        let agent = GeminiAgent::new();
        let (auth_method, auth_source) = agent.detect_auth_details_sync();

        assert_eq!(auth_method, "unknown-auth-type");
        assert_eq!(auth_source, "configured but invalid");
    }

    // Tests for get_gemini_status method

    // Mock GeminiAgent for testing get_gemini_status
    struct MockGeminiAgent {
        binary_path: String,
        should_timeout: bool,
        should_fail: bool,
        should_not_exist: bool,
        version_to_return: Option<String>,
    }

    impl MockGeminiAgent {
        fn new() -> Self {
            Self {
                binary_path: "gemini".to_string(),
                should_timeout: false,
                should_fail: false,
                should_not_exist: false,
                version_to_return: Some("1.0.0".to_string()),
            }
        }

        fn with_timeout(mut self) -> Self {
            self.should_timeout = true;
            self
        }

        fn with_failure(mut self) -> Self {
            self.should_fail = true;
            self
        }

        fn with_not_found(mut self) -> Self {
            self.should_not_exist = true;
            self
        }

        fn with_version(mut self, version: &str) -> Self {
            self.version_to_return = Some(version.to_string());
            self
        }

        async fn detect_version(&self) -> AgentResult<AgentVersion> {
            if self.should_not_exist {
                return Err(AgentError::AgentNotFound(self.binary_path.clone()));
            }

            if self.should_fail {
                return Err(AgentError::ProcessSpawnFailed(std::io::Error::other(
                    "Test failure",
                )));
            }

            if self.should_timeout {
                // Simulate a hanging operation
                tokio::time::sleep(tokio::time::Duration::from_millis(3000)).await;
            }

            Ok(AgentVersion {
                version: self.version_to_return.clone().unwrap_or("1.0.0".to_string()),
                commit: None,
                release_date: None,
            })
        }

        fn detect_auth_details_sync(&self) -> (String, String) {
            // Use the real implementation
            let agent = GeminiAgent::new();
            agent.detect_auth_details_sync()
        }

        async fn get_gemini_status(&self) -> AgentStatus {
            // Check CLI availability by detecting version with timeout
            let (available, version, error) = match tokio::time::timeout(
                std::time::Duration::from_millis(1500),
                self.detect_version(),
            )
            .await
            {
                Ok(Ok(version_info)) => (true, Some(version_info.version), None),
                Ok(Err(AgentError::AgentNotFound(_))) => (
                    false,
                    None,
                    Some("Gemini CLI not found in PATH".to_string()),
                ),
                Ok(Err(e)) => (false, None, Some(e.to_string())),
                Err(_) => (false, None, Some("Version detection timed out".to_string())),
            };

            if !available {
                return AgentStatus {
                    available: false,
                    version: None,
                    authenticated: false,
                    auth_method: None,
                    auth_source: None,
                    error,
                };
            }

            // Check authentication status using synchronous method to avoid hanging
            let (auth_method, auth_source) = self.detect_auth_details_sync();
            let authenticated = auth_method != "Unknown" && auth_source != "Unknown";

            AgentStatus {
                available,
                version,
                authenticated,
                auth_method: if authenticated {
                    Some(auth_method)
                } else {
                    None
                },
                auth_source: if authenticated {
                    Some(auth_source)
                } else {
                    None
                },
                error,
            }
        }
    }

    #[tokio::test]
    async fn test_get_gemini_status_agent_not_found() {
        let mock_agent = MockGeminiAgent::new().with_not_found();
        let status = mock_agent.get_gemini_status().await;

        assert!(!status.available);
        assert_eq!(status.version, None);
        assert!(!status.authenticated);
        assert_eq!(status.auth_method, None);
        assert_eq!(status.auth_source, None);
        assert_eq!(
            status.error,
            Some("Gemini CLI not found in PATH".to_string())
        );
    }

    #[tokio::test]
    async fn test_get_gemini_status_timeout() {
        let mock_agent = MockGeminiAgent::new().with_timeout();
        let status = mock_agent.get_gemini_status().await;

        assert!(!status.available);
        assert_eq!(status.version, None);
        assert!(!status.authenticated);
        assert_eq!(status.auth_method, None);
        assert_eq!(status.auth_source, None);
        assert_eq!(
            status.error,
            Some("Version detection timed out".to_string())
        );
    }

    #[tokio::test]
    async fn test_get_gemini_status_version_detection_failed() {
        let mock_agent = MockGeminiAgent::new().with_failure();
        let status = mock_agent.get_gemini_status().await;

        assert!(!status.available);
        assert_eq!(status.version, None);
        assert!(!status.authenticated);
        assert_eq!(status.auth_method, None);
        assert_eq!(status.auth_source, None);
        assert_eq!(
            status.error,
            Some("Failed to spawn agent process: Test failure".to_string())
        );
    }

    #[tokio::test]
    #[serial_test::serial(env)]
    async fn test_get_gemini_status_successful_with_auth() {
        use crate::test_support::EnvVarGuard;

        let temp_dir = tempfile::tempdir().unwrap();
        let home_dir = temp_dir.path();
        let _home_guard = EnvVarGuard::set("HOME", home_dir);

        // Set up authentication
        let _gemini_api_key_guard = EnvVarGuard::set("GEMINI_API_KEY", "test-key");

        let mock_agent = MockGeminiAgent::new().with_version("1.2.3");
        let status = mock_agent.get_gemini_status().await;

        assert!(status.available);
        assert_eq!(status.version, Some("1.2.3".to_string()));
        assert!(status.authenticated);
        assert_eq!(status.auth_method, Some("gemini-api-key".to_string()));
        assert_eq!(status.auth_source, Some("GEMINI_API_KEY".to_string()));
        assert_eq!(status.error, None);
    }

    #[tokio::test]
    #[serial_test::serial(env)]
    async fn test_get_gemini_status_successful_no_auth() {
        use crate::test_support::EnvVarGuard;

        let temp_dir = tempfile::tempdir().unwrap();
        let home_dir = temp_dir.path();
        let _home_guard = EnvVarGuard::set("HOME", home_dir);

        // Clean environment variables
        let _gemini_guard = EnvVarGuard::remove("GEMINI_API_KEY");
        let _google_guard = EnvVarGuard::remove("GOOGLE_API_KEY");

        let mock_agent = MockGeminiAgent::new().with_version("2.0.0");
        let status = mock_agent.get_gemini_status().await;

        assert!(status.available);
        assert_eq!(status.version, Some("2.0.0".to_string()));
        assert!(!status.authenticated);
        assert_eq!(status.auth_method, None);
        assert_eq!(status.auth_source, None);
        assert_eq!(status.error, None);
    }

    #[tokio::test]
    #[serial_test::serial(env)]
    async fn test_get_gemini_status_with_vertex_ai_auth() {
        use crate::test_support::EnvVarGuard;

        let temp_dir = tempfile::tempdir().unwrap();
        let home_dir = temp_dir.path();
        let _home_guard = EnvVarGuard::set("HOME", home_dir);

        let gemini_dir = home_dir.join(".gemini");
        std::fs::create_dir_all(&gemini_dir).unwrap();

        let settings_content = serde_json::json!({
            "security": {
                "auth": {
                    "selectedType": "vertex-ai"
                }
            }
        });
        let settings_path = gemini_dir.join("settings.json");
        std::fs::write(
            &settings_path,
            serde_json::to_string_pretty(&settings_content).unwrap(),
        )
        .unwrap();

        let _google_api_key_guard = EnvVarGuard::set("GOOGLE_API_KEY", "test-google-key");

        let mock_agent = MockGeminiAgent::new().with_version("0.5.0");
        let status = mock_agent.get_gemini_status().await;

        assert!(status.available);
        assert_eq!(status.version, Some("0.5.0".to_string()));
        assert!(status.authenticated);
        assert_eq!(status.auth_method, Some("vertex-ai".to_string()));
        assert_eq!(status.auth_source, Some("GOOGLE_API_KEY".to_string()));
        assert_eq!(status.error, None);
    }

    #[tokio::test]
    #[serial_test::serial(env)]
    async fn test_get_gemini_status_with_oauth_auth() {
        use crate::test_support::EnvVarGuard;

        let temp_dir = tempfile::tempdir().unwrap();
        let home_dir = temp_dir.path();
        let _home_guard = EnvVarGuard::set("HOME", home_dir);

        // Clean environment variables
        let _gemini_guard = EnvVarGuard::remove("GEMINI_API_KEY");
        let _google_guard = EnvVarGuard::remove("GOOGLE_API_KEY");

        let gemini_dir = home_dir.join(".gemini");
        std::fs::create_dir_all(&gemini_dir).unwrap();

        // Create oauth_creds.json file
        let oauth_creds_path = gemini_dir.join("oauth_creds.json");
        std::fs::write(&oauth_creds_path, r#"{"token": "test"}"#).unwrap();

        let mock_agent = MockGeminiAgent::new().with_version("1.5.0");
        let status = mock_agent.get_gemini_status().await;

        assert!(status.available);
        assert_eq!(status.version, Some("1.5.0".to_string()));
        assert!(status.authenticated);
        assert_eq!(status.auth_method, Some("oauth-personal".to_string()));
        assert_eq!(
            status.auth_source,
            Some(oauth_creds_path.to_string_lossy().to_string())
        );
        assert_eq!(status.error, None);
    }
}
