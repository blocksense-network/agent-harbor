// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use crate::session::{export_directory, import_directory};
use crate::traits::*;
use async_trait::async_trait;
use regex::Regex;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::{Child, Command};
use tracing::{debug, info, warn};

pub struct CopilotAgent {
    binary_path: String,
}

impl CopilotAgent {
    pub fn new() -> Self {
        Self {
            binary_path: "copilot".to_string(),
        }
    }

    /// Parse version from `copilot --version` output
    fn parse_version(output: &str) -> AgentResult<AgentVersion> {
        // Typical format:
        //   0.0.341
        //   Commit: 5725358
        let version_regex = Regex::new(r"(\d+\.\d+\.\d+)").map_err(|e| {
            AgentError::VersionDetectionFailed(format!("Regex compilation failed: {}", e))
        })?;

        let commit_regex = Regex::new(r"Commit:\s*([a-fA-F0-9]+)").map_err(|e| {
            AgentError::VersionDetectionFailed(format!("Commit regex compilation failed: {}", e))
        })?;

        let version = if let Some(caps) = version_regex.captures(output) {
            caps[0].to_string()
        } else {
            return Err(AgentError::VersionDetectionFailed(format!(
                "Could not parse version from output: {}",
                output
            )));
        };

        let commit = commit_regex.captures(output).map(|caps| caps[1].to_string());

        Ok(AgentVersion {
            version,
            commit,
            release_date: None,
        })
    }

    async fn read_token_file(path: &str) -> Option<String> {
        match tokio::fs::read_to_string(path).await {
            Ok(content) => {
                let token = content.trim().to_string();
                if token.is_empty() { None } else { Some(token) }
            }
            Err(e) => {
                warn!("Failed to read token file {}: {}", path, e);
                None
            }
        }
    }

    /// Best-effort extraction of oauth_token from gh hosts.yml without pulling a YAML parser
    fn extract_token_from_gh_hosts_yaml(contents: &str) -> Option<String> {
        // Look for a line like: oauth_token: ghp_abc... or oauth_token: github_pat_...
        let re = Regex::new(r"oauth_token:\s*([A-Za-z0-9_\.-]+)").ok()?;
        re.captures(contents)
            .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
            .filter(|s| !s.is_empty())
    }

    async fn setup_onboarding_skip(&self, home_dir: &Path, working_dir: &Path) -> AgentResult<()> {
        debug!("Setting up Copilot CLI configuration to skip onboarding");

        let config_dir = self.config_dir(home_dir);

        tokio::fs::create_dir_all(&config_dir).await.map_err(|e| {
            AgentError::ConfigCreationFailed(format!(
                "Failed to create Copilot config directory: {}",
                e
            ))
        })?;

        let cfg_path = config_dir.join("config.json");

        let mut obj = serde_json::Map::new();
        if cfg_path.exists() {
            match tokio::fs::read_to_string(&cfg_path).await {
                Ok(content) => {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                        if let Some(map) = val.as_object() {
                            obj = map.clone();
                        }
                    }
                }
                Err(e) => warn!("Failed to read existing Copilot config.json: {}", e),
            }
        }

        obj.insert(
            "theme".to_string(),
            serde_json::Value::String("dark".to_string()),
        );

        let mut folders: Vec<serde_json::Value> = obj
            .get("trusted_folders")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let mut ensure = |s: &str| {
            if !folders.iter().any(|v| v.as_str() == Some(s)) {
                folders.push(serde_json::Value::String(s.to_string()));
            }
        };

        ensure(".");
        ensure("./config.json");
        ensure(&working_dir.to_string_lossy());

        obj.insert(
            "trusted_folders".to_string(),
            serde_json::Value::Array(folders),
        );

        let out = serde_json::to_string_pretty(&serde_json::Value::Object(obj)).map_err(|e| {
            AgentError::ConfigCreationFailed(format!(
                "Failed to serialize Copilot config.json: {}",
                e
            ))
        })?;

        tokio::fs::write(&cfg_path, out).await.map_err(|e| {
            AgentError::ConfigCreationFailed(format!(
                "Failed to write Copilot config.json at {:?}: {}",
                cfg_path, e
            ))
        })?;

        debug!("Created/updated Copilot configuration at {:?}", cfg_path);
        Ok(())
    }
}

impl Default for CopilotAgent {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AgentExecutor for CopilotAgent {
    fn name(&self) -> &'static str {
        "copilot-cli"
    }

    async fn detect_version(&self) -> AgentResult<AgentVersion> {
        debug!("Detecting GitHub Copilot CLI version");

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
            "Preparing Copilot CLI launch with prompt: {:?}",
            config
                .prompt
                .as_deref()
                .unwrap_or("<empty>")
                .chars()
                .take(50)
                .collect::<String>()
        );

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

        if using_custom_home {
            debug!(
                "Creating Copilot configuration to skip onboarding in {:?}",
                config.home_dir
            );
            self.setup_onboarding_skip(&config.home_dir, &config.working_dir).await?;
        }

        let mut cmd = tokio::process::Command::new(&self.binary_path);

        cmd.env("HOME", &config.home_dir);

        cmd.env("XDG_CONFIG_HOME", &config.home_dir);
        cmd.env("XDG_STATE_HOME", &config.home_dir);

        cmd.current_dir(&config.working_dir);

        if config.interactive {
            cmd.stdin(Stdio::inherit());
            cmd.stdout(Stdio::inherit());
            cmd.stderr(Stdio::inherit());
        } else {
            cmd.stdin(Stdio::piped());
            cmd.stdout(Stdio::piped());
            cmd.stderr(Stdio::piped());
        }

        if let Some(api_server) = &config.api_server {
            cmd.env("COPILOT_API_BASE", api_server);
        }

        if let Some(api_key) = &config.api_key {
            cmd.env("GH_TOKEN", api_key);
            cmd.env("GITHUB_TOKEN", api_key);
        }

        // Additional environment variables from config
        for (key, value) in &config.env_vars {
            cmd.env(key, value);
        }

        if !config.interactive {
            cmd.arg("--allow-all-tools");
            cmd.env("COPILOT_ALLOW_ALL", "true");
            cmd.arg("--add-dir");
            cmd.arg(config.working_dir.as_os_str());
        }

        if config.unrestricted {
            cmd.arg("--allow-all-paths");
            cmd.env("COPILOT_ALLOW_ALL", "true");
            cmd.arg("--allow-all-tools");
            cmd.env("COPILOT_ALLOW_ALL", "true");
        }

        if let Some(model) = &config.model {
            cmd.arg("--model");
            cmd.arg(model);
            cmd.env("COPILOT_MODEL", model);
        }

        if config.json_output {
            warn!("JSON output is not yet supported by Copilot CLI");
        }

        if let Some(prompt) = &config.prompt {
            cmd.arg("-p").arg(prompt);
        }

        debug!("Copilot CLI command prepared successfully");

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

        debug!("Copilot CLI process spawned successfully");
        Ok(child)
    }

    fn credential_paths(&self) -> Vec<PathBuf> {
        vec![
            PathBuf::from(".config/gh/hosts.yml"),
            PathBuf::from(".copilot/config.json"),
        ]
    }

    async fn get_user_api_key(&self) -> AgentResult<Option<String>> {
        // 1) Direct environment variables
        if let Ok(token) = std::env::var("GH_TOKEN") {
            if !token.trim().is_empty() {
                debug!("Found GH_TOKEN in environment");
                return Ok(Some(token));
            }
        }
        if let Ok(token) = std::env::var("GITHUB_TOKEN") {
            if !token.trim().is_empty() {
                debug!("Found GITHUB_TOKEN in environment");
                return Ok(Some(token));
            }
        }

        // 2) File path specified via env
        if let Ok(path) = std::env::var("GH_TOKEN_FILE") {
            if let Some(token) = Self::read_token_file(&path).await {
                return Ok(Some(token));
            }
        }
        if let Ok(path) = std::env::var("GITHUB_TOKEN_FILE") {
            if let Some(token) = Self::read_token_file(&path).await {
                return Ok(Some(token));
            }
        }

        // 3) Parse gh hosts.yml for oauth_token
        if let Some(home_dir) = dirs::home_dir() {
            let hosts = home_dir.join(".config/gh/hosts.yml");
            if hosts.exists() {
                match std::fs::read_to_string(&hosts) {
                    Ok(contents) => {
                        if let Some(token) = Self::extract_token_from_gh_hosts_yaml(&contents) {
                            debug!("Extracted oauth_token from gh hosts.yml");
                            return Ok(Some(token));
                        }
                    }
                    Err(e) => warn!("Failed to read {:?}: {}", hosts, e),
                }
            }
        }

        // 4) Copilot CLI config.json tokens
        if let Some(home_dir) = dirs::home_dir() {
            let cfg = home_dir.join(".copilot/config.json");
            if cfg.exists() {
                match std::fs::read_to_string(&cfg) {
                    Ok(contents) => {
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&contents) {
                            // Copilot config can store last_logged_in_user as:
                            // - a string login, OR
                            // - an object with { host, login }
                            let (mut host_opt, mut login_opt): (Option<String>, Option<String>) =
                                (None, None);

                            if let Some(llu) = val.get("last_logged_in_user") {
                                if let Some(s) = llu.as_str() {
                                    // Old/simple format: just the login string
                                    login_opt = Some(s.to_string());
                                } else if let Some(o) = llu.as_object() {
                                    login_opt = o
                                        .get("login")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string());
                                    host_opt = o
                                        .get("host")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string());
                                }
                            }

                            let tokens_obj = val.get("copilot_tokens").and_then(|v| v.as_object());

                            // Utility function to try retrieving a token by (host, login) tuple
                            let mut try_get_token = |host: &str, login: &str| -> Option<String> {
                                let key = format!("{}:{}", host, login);
                                tokens_obj
                                    .and_then(|obj| obj.get(&key))
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.trim().to_string())
                                    .filter(|s| !s.is_empty())
                            };

                            // First, try last_logged_in_user if present
                            if let Some(login) = login_opt.clone() {
                                let host = host_opt
                                    .clone()
                                    .unwrap_or_else(|| "https://github.com".to_string());
                                if let Some(token) = try_get_token(&host, &login) {
                                    debug!("Found token for {} in Copilot config.json", login);
                                    return Ok(Some(token));
                                }
                            }

                            // Fallback: iterate logged_in_users array and try each
                            if let Some(users) =
                                val.get("logged_in_users").and_then(|v| v.as_array())
                            {
                                for u in users {
                                    if let Some(o) = u.as_object() {
                                        if let (Some(login), host) = (
                                            o.get("login").and_then(|v| v.as_str()),
                                            o.get("host")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("https://github.com"),
                                        ) {
                                            if let Some(token) = try_get_token(host, login) {
                                                debug!(
                                                    "Found token for {} in Copilot config.json",
                                                    login
                                                );
                                                return Ok(Some(token));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => warn!("Failed to read Copilot config.json at {:?}: {}", cfg, e),
                }
            }
        }

        Ok(None)
    }

    async fn export_session(&self, home_dir: &Path) -> AgentResult<PathBuf> {
        let config_dir = self.config_dir(home_dir);
        let archive_path = home_dir.join("copilot-session.tar.gz");

        export_directory(&config_dir, &archive_path).await?;
        Ok(archive_path)
    }

    async fn import_session(&self, session_archive: &Path, home_dir: &Path) -> AgentResult<()> {
        let config_dir = self.config_dir(home_dir);
        import_directory(session_archive, &config_dir).await
    }

    fn parse_output(&self, raw_output: &[u8]) -> AgentResult<Vec<AgentEvent>> {
        // Parse Copilot CLI output into normalized events.
        // Heuristic approach until a formal stream format is adopted.
        let output = String::from_utf8_lossy(raw_output);
        let mut events = Vec::new();

        for line in output.lines() {
            let lt = line.trim();
            if lt.is_empty() {
                continue;
            }

            if lt.contains("Thinking") || lt.contains("reasoning") {
                events.push(AgentEvent::Thinking {
                    content: lt.to_string(),
                });
            } else if lt.contains("Tool")
                || lt.contains("Command")
                || lt.contains("Running")
                || lt.contains("shell(")
            {
                events.push(AgentEvent::ToolUse {
                    tool_name: "shell".to_string(),
                    arguments: serde_json::json!({"line": lt}),
                });
            } else if lt.to_lowercase().contains("error") || lt.to_lowercase().contains("failed") {
                events.push(AgentEvent::Error {
                    message: lt.to_string(),
                });
            } else {
                events.push(AgentEvent::Output {
                    content: lt.to_string(),
                });
            }
        }

        Ok(events)
    }

    fn config_dir(&self, home: &Path) -> PathBuf {
        home.join(".copilot")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;

    #[test]
    fn test_parse_version() {
        let output = "copilot version 1.2.3";
        let result = CopilotAgent::parse_version(output);
        assert!(result.is_ok());
        let version = result.unwrap();
        assert_eq!(version.version, "1.2.3");
    }

    #[test]
    fn test_parse_version_simple() {
        let output = "1.2.3";
        let result = CopilotAgent::parse_version(output);
        assert!(result.is_ok());
        let version = result.unwrap();
        assert_eq!(version.version, "1.2.3");
        assert_eq!(version.commit, None);
    }

    #[test]
    fn test_parse_version_with_commit() {
        let output = "0.0.341\nCommit: 5725358";
        let result = CopilotAgent::parse_version(output);
        assert!(result.is_ok());
        let version = result.unwrap();
        assert_eq!(version.version, "0.0.341");
        assert_eq!(version.commit, Some("5725358".to_string()));
    }

    #[tokio::test]
    async fn test_agent_name() {
        let agent = CopilotAgent::new();
        assert_eq!(agent.name(), "copilot-cli");
    }

    #[tokio::test]
    async fn test_config_dir() {
        let agent = CopilotAgent::new();
        let home = PathBuf::from("/home/user");
        let config = agent.config_dir(&home);
        assert_eq!(config, PathBuf::from("/home/user/.copilot"));
    }

    #[tokio::test]
    async fn test_credential_paths() {
        let agent = CopilotAgent::new();
        let paths = agent.credential_paths();
        assert!(
            paths
                .iter()
                .any(|p| p.as_path() == std::path::Path::new(".config/gh/hosts.yml"))
        );
    }

    #[tokio::test]
    async fn test_setup_skip_onboarding_config_creates_file() {
        let agent = CopilotAgent::new();
        let temp = tempfile::TempDir::new().unwrap();

        let working_dir = temp.path().join("repo");
        tokio::fs::create_dir_all(&working_dir).await.unwrap();

        agent
            .setup_onboarding_skip(temp.path(), &working_dir)
            .await
            .expect("setup should succeed");

        let cfg_path = temp.path().join(".copilot/config.json");
        assert!(cfg_path.exists());

        let s = fs::read_to_string(&cfg_path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v.get("theme").and_then(|v| v.as_str()), Some("dark"));
        let arr = v.get("trusted_folders").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        let items: Vec<_> = arr.iter().filter_map(|x| x.as_str()).collect();
        assert!(items.contains(&"."));
        assert!(items.contains(&"./config.json"));
        let wd_s = working_dir.to_string_lossy().to_string();
        assert!(items.iter().any(|s| *s == wd_s));
    }

    #[tokio::test]
    async fn test_get_user_api_key_sources() {
        let agent = CopilotAgent::new();

        // Save previous env to restore at the end
        let prev_home = env::var_os("HOME");
        let prev_gh_token = env::var_os("GH_TOKEN");
        let prev_github_token = env::var_os("GITHUB_TOKEN");
        let prev_gh_token_file = env::var_os("GH_TOKEN_FILE");
        let prev_github_token_file = env::var_os("GITHUB_TOKEN_FILE");

        let temp_home = tempfile::TempDir::new().unwrap();
        env::set_var("HOME", temp_home.path());

        // Ensure clean env
        env::remove_var("GH_TOKEN");
        env::remove_var("GITHUB_TOKEN");
        env::remove_var("GH_TOKEN_FILE");
        env::remove_var("GITHUB_TOKEN_FILE");

        // 0) No tokens anywhere
        let token = agent.get_user_api_key().await.unwrap();
        assert_eq!(token, None);

        // 1) GH_TOKEN takes precedence
        env::set_var("GH_TOKEN", "token_env1");
        let token = agent.get_user_api_key().await.unwrap();
        assert_eq!(token.as_deref(), Some("token_env1"));
        env::remove_var("GH_TOKEN");

        // 2) GITHUB_TOKEN
        env::set_var("GITHUB_TOKEN", "token_env2");
        let token = agent.get_user_api_key().await.unwrap();
        assert_eq!(token.as_deref(), Some("token_env2"));
        env::remove_var("GITHUB_TOKEN");

        // 3) GH_TOKEN_FILE
        let token_file1 = temp_home.path().join("gh_token_file.txt");
        fs::write(&token_file1, "filetoken1\n").unwrap();
        env::set_var("GH_TOKEN_FILE", &token_file1);
        let token = agent.get_user_api_key().await.unwrap();
        assert_eq!(token.as_deref(), Some("filetoken1"));
        env::remove_var("GH_TOKEN_FILE");

        // 4) GITHUB_TOKEN_FILE
        let token_file2 = temp_home.path().join("github_token_file.txt");
        fs::write(&token_file2, "filetoken2").unwrap();
        env::set_var("GITHUB_TOKEN_FILE", &token_file2);
        let token = agent.get_user_api_key().await.unwrap();
        assert_eq!(token.as_deref(), Some("filetoken2"));
        env::remove_var("GITHUB_TOKEN_FILE");

        // 5) gh hosts.yml oauth_token
        let gh_dir = temp_home.path().join(".config/gh");
        std::fs::create_dir_all(&gh_dir).unwrap();
        let hosts_yml = gh_dir.join("hosts.yml");
        let hosts_content = "github.com:\n  oauth_token: ghp_yaml_token_123\n";
        fs::write(&hosts_yml, hosts_content).unwrap();
        let token = agent.get_user_api_key().await.unwrap();
        assert_eq!(token.as_deref(), Some("ghp_yaml_token_123"));
        // Remove to allow testing next source
        std::fs::remove_file(&hosts_yml).unwrap();

        // 6) Copilot config.json with last_logged_in_user as string
        let copilot_dir = temp_home.path().join(".copilot");
        std::fs::create_dir_all(&copilot_dir).unwrap();
        let cfg_path = copilot_dir.join("config.json");
        let cfg1 = serde_json::json!({
            "last_logged_in_user": "alice",
            "copilot_tokens": {
                "https://github.com:alice": "token_alice"
            }
        });
        fs::write(&cfg_path, serde_json::to_string_pretty(&cfg1).unwrap()).unwrap();
        let token = agent.get_user_api_key().await.unwrap();
        assert_eq!(token.as_deref(), Some("token_alice"));

        // 7) Copilot config.json fallback via logged_in_users
        let cfg2 = serde_json::json!({
            "logged_in_users": [
                { "login": "bob", "host": "https://github.com" }
            ],
            "copilot_tokens": {
                "https://github.com:bob": "token_bob"
            }
        });
        fs::write(&cfg_path, serde_json::to_string_pretty(&cfg2).unwrap()).unwrap();
        let token = agent.get_user_api_key().await.unwrap();
        assert_eq!(token.as_deref(), Some("token_bob"));

        // Restore env
        match prev_home {
            Some(v) => env::set_var("HOME", v),
            None => env::remove_var("HOME"),
        }
        match prev_gh_token {
            Some(v) => env::set_var("GH_TOKEN", v),
            None => env::remove_var("GH_TOKEN"),
        }
        match prev_github_token {
            Some(v) => env::set_var("GITHUB_TOKEN", v),
            None => env::remove_var("GITHUB_TOKEN"),
        }
        match prev_gh_token_file {
            Some(v) => env::set_var("GH_TOKEN_FILE", v),
            None => env::remove_var("GH_TOKEN_FILE"),
        }
        match prev_github_token_file {
            Some(v) => env::set_var("GITHUB_TOKEN_FILE", v),
            None => env::remove_var("GITHUB_TOKEN_FILE"),
        }
    }
}
