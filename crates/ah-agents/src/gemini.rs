/// Google Gemini agent implementation
use crate::credentials::{copy_files, gemini_credential_paths};
use crate::session::{export_directory, import_directory};
use crate::traits::*;
use async_trait::async_trait;
use regex::Regex;
use std::path::{Path, PathBuf};
use tokio::process::Command;
use tracing::{debug, info, warn};

/// Google Gemini agent executor
pub struct GeminiAgent {
    binary_path: String,
}

impl GeminiAgent {
    pub fn new() -> Self {
        Self {
            binary_path: "gemini".to_string(),
        }
    }

    /// Parse version from `gemini --version` output
    fn parse_version(output: &str) -> AgentResult<AgentVersion> {
        // Expected format: "gemini version X.Y.Z" or "X.Y.Z"
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

    async fn setup_onboarding_skip(&self, home_dir: &Path, _working_dir: &Path) -> AgentResult<()> {
        // Find the Gemini settings file in the home directory
        // The settings file is typically located in the config directory
        let config_dir = self.config_dir(home_dir);

        // Create config directory if it doesn't exist
        tokio::fs::create_dir_all(&config_dir).await.map_err(|e| {
            AgentError::ConfigurationError(format!("Failed to create config directory: {}", e))
        })?;

        // Look for settings.json in the config directory
        let settings_path = config_dir.join("settings.json");

        // Read existing settings or create new object
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

        // Extend settings with onboarding skip configuration
        // Using merge strategy to preserve existing settings
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
        debug!("Detecting Gemini version");

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
        println!("CONFIG: {:?}", config);
        info!(
            "Preparing Gemini launch with prompt: {:?}",
            config.prompt.chars().take(50).collect::<String>()
        );

        // If GEMINI_API_KEY is set in the environment, use it with priority over
        // the credentials from logging.
        let env_api_key = std::env::var("GEMINI_API_KEY").unwrap_or_default();
        if !env_api_key.is_empty() {
            info!("Detected GEMINI_API_KEY from environment.");
        }

        // Check if we're using a custom HOME directory
        let using_custom_home = if let Ok(system_home) = std::env::var("HOME") {
            PathBuf::from(system_home) != config.home_dir
        } else {
            true // If no system HOME, we're definitely using custom
        };

        // Copy credentials if requested and home_dir differs from system HOME
        if config.copy_credentials && using_custom_home {
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

        // Set up onboarding skip configuration for custom HOME
        if using_custom_home {
            debug!(
                "Creating Gemini configuration to skip onboarding in {:?}",
                config.home_dir
            );
            self.setup_onboarding_skip(&config.home_dir, &config.working_dir).await?;
        }

        let mut cmd = tokio::process::Command::new(&self.binary_path);

        // Set custom HOME directory for environment isolation
        cmd.env("HOME", &config.home_dir);

        // Set current working directory
        cmd.current_dir(&config.working_dir);

        // TODO: Reserch and fix
        // // Add custom API server if specified
        // // Note: Gemini might use different environment variables than OpenAI
        // if let Some(api_server) = &config.api_server {
        //     cmd.env("GEMINI_API_BASE", api_server);
        //     cmd.env("GEMINI_BASE_URL", api_server);
        // }

        // Add API key if specified
        if let Some(config_api_key) = &config.api_key {
            // check config_api_key == env_api_key
            // if they defer, use the one from the config
            if env_api_key != *config_api_key {
                info!("GEMINI_API_KEY differs from environment variable, using config value");
            }
            cmd.env("GEMINI_API_KEY", config_api_key);
            // If they match, the env var takes precedence
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

        if config.interactive {
            cmd.arg("--prompt-interactive");
        }

        // Add model specification if provided
        if let Some(model) = &config.model {
            cmd.arg("--model");
            cmd.arg(model);
        }

        if config.unrestricted {
            // For interactive mode with unrestricted flag, bypass permission prompts
            cmd.arg("--yolo");
        }

        if config.json_output {
            cmd.arg("--output-format").arg("json");
        } else {
            cmd.arg("--output-format").arg("text");
        }

        // Add the prompt as argument if provided
        if !config.prompt.is_empty() {
            cmd.arg(&config.prompt);
        }

        debug!("Gemini command prepared successfully");
        println!("Gemini command: {:?}", cmd);
        Ok(cmd)
    }

    async fn copy_credentials(&self, src_home: &Path, dst_home: &Path) -> AgentResult<()> {
        #[cfg(target_os = "macos")]
        {
            warn!("copy_credentials not implemented yet on Mac")
        }
        #[cfg(target_os = "linux")]
        {
            info!(
                "Copying Gemini credentials from {:?} to {:?}",
                src_home, dst_home
            );

            let paths = gemini_credential_paths();
            copy_files(&paths, src_home, dst_home).await?;

            debug!("Gemini credentials copied successfully");
            Ok(())
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            warn!("Credential copy not implemented for this platform");
            Ok(None)
        }
    }

    async fn export_session(&self, home_dir: &Path) -> AgentResult<PathBuf> {
        warn!("Not implemented yet");
        Err(AgentError::Other(anyhow::anyhow!("Not implemented yet")))
        // info!("Exporting Gemini session from {:?}", home_dir);

        // let config_dir = self.config_dir(home_dir);
        // let archive_path = home_dir.join("gemini-session.tar.gz");

        // export_directory(&config_dir, &archive_path).await?;

        // debug!("Gemini session exported to {:?}", archive_path);
        // Ok(archive_path)
    }

    async fn import_session(&self, session_archive: &Path, home_dir: &Path) -> AgentResult<()> {
        warn!("Not implemented yet");
        Err(AgentError::Other(anyhow::anyhow!("Not implemented yet")))
        // info!(
        //     "Importing Gemini session from {:?} to {:?}",
        //     session_archive, home_dir
        // );

        // let config_dir = self.config_dir(home_dir);
        // import_directory(session_archive, &config_dir).await?;

        // debug!("Gemini session imported successfully");
        // Ok(())
    }

    fn parse_output(&self, raw_output: &[u8]) -> AgentResult<Vec<AgentEvent>> {
        warn!("Not implemented yet");
        Err(AgentError::Other(anyhow::anyhow!("Not implemented yet")))
        // // Parse Gemini output into normalized events
        // // TODO: Implement actual Gemini output parsing based on its format
        // let output = String::from_utf8_lossy(raw_output);
        // let mut events = Vec::new();

        // // Basic line-by-line parsing (placeholder implementation)
        // for line in output.lines() {
        //     if line.contains("Thinking") || line.contains("reasoning") {
        //         events.push(AgentEvent::Thinking {
        //             content: line.to_string(),
        //         });
        //     } else if line.contains("Tool") || line.contains("Command") {
        //         events.push(AgentEvent::ToolUse {
        //             tool_name: "shell".to_string(),
        //             arguments: serde_json::json!({"command": line}),
        //         });
        //     } else if line.contains("Error") || line.contains("error") {
        //         events.push(AgentEvent::Error {
        //             message: line.to_string(),
        //         });
        //     } else if !line.trim().is_empty() {
        //         events.push(AgentEvent::Output {
        //             content: line.to_string(),
        //         });
        //     }
        // }

        // Ok(events)
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
}
