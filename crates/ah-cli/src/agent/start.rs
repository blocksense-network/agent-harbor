// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Agent start command implementation

use ah_agents::{AgentExecutor, AgentLaunchConfig};
use ah_core::agent_executor::ah_full_path;
use ah_domain_types::AgentSoftware as AgentType;
use anyhow::Context;
use clap::Args;
use reqwest::Client;
use serde_json::json;
use std::path::PathBuf;
use std::process as stdprocess;
use uuid;

// Import snapshot types from parent and traits
use crate::tui::FsSnapshotsType;
use ah_domain_types::{AgentSoftware, OutputFormat};
use ah_fs_snapshots::WorkingCopyMode;

/// Agent start command arguments
#[derive(Args, Clone)]
pub struct AgentStartArgs {
    /// Agent type to start
    #[arg(long, default_value = "mock")]
    pub agent: AgentSoftware,

    /// Enable non-interactive mode (e.g., codex exec)
    #[arg(long)]
    pub non_interactive: bool,

    /// Output format: text, text-normalized, json, or json-normalized
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,

    /// Custom LLM API URI for agent backend
    #[arg(long, value_name = "URI")]
    pub llm_api: Option<String>,

    /// API key for custom LLM API
    #[arg(long, value_name = "KEY")]
    pub llm_api_key: Option<String>,

    /// LLM API proxy URL to use for routing requests
    #[arg(long, value_name = "URL")]
    pub llm_api_proxy_url: Option<String>,

    /// Working copy mode
    #[arg(long, value_enum, default_value = "auto")]
    pub working_copy: WorkingCopyMode,

    /// Filesystem snapshot provider to use
    #[arg(long, value_enum, default_value = "auto")]
    pub fs_snapshots: FsSnapshotsType,

    /// Working directory for agent execution
    #[arg(long)]
    pub cwd: Option<PathBuf>,

    /// Restore workspace from filesystem snapshot (enables fast task launch)
    #[arg(long)]
    pub from_snapshot: Option<String>,

    /// Enable sandbox mode
    #[arg(long)]
    pub sandbox: bool,

    /// Sandbox type (when sandbox is enabled)
    #[arg(long, default_value = "local")]
    pub sandbox_type: String,

    /// Allow network access in sandbox
    #[arg(long, value_parser = parse_bool)]
    pub allow_network: Option<bool>,

    /// Allow container access in sandbox
    #[arg(long, value_parser = parse_bool)]
    pub allow_containers: Option<bool>,

    /// Allow KVM access in sandbox
    #[arg(long, value_parser = parse_bool)]
    pub allow_kvm: Option<bool>,

    /// Enable seccomp filtering
    #[arg(long, value_parser = parse_bool)]
    pub seccomp: Option<bool>,

    /// Enable seccomp debugging
    #[arg(long, value_parser = parse_bool)]
    pub seccomp_debug: Option<bool>,

    /// Allow web search capabilities for agents that support it
    #[arg(long)]
    pub allow_web_search: bool,

    /// Model to use for the agent (can be overridden by agent-specific flags)
    #[arg(long)]
    pub model: Option<String>,

    /// Model to use specifically for Codex agent (overrides --model)
    #[arg(long)]
    pub codex_model: Option<String>,

    /// Model to use specifically for Copilot agent (overrides --model)
    #[arg(long)]
    pub copilot_model: Option<String>,

    /// Model to use specifically for Claude agent (overrides --model)
    #[arg(long)]
    pub claude_model: Option<String>,

    /// Model to use specifically for Gemini agent (overrides --model)
    #[arg(long)]
    pub gemini_model: Option<String>,

    /// Additional writable paths to bind mount
    #[arg(long)]
    pub mount_rw: Vec<PathBuf>,

    /// Paths to promote to copy-on-write overlays
    #[arg(long)]
    pub overlay: Vec<PathBuf>,

    /// Custom prompt text to pass to the agent (overrides task/session prompt)
    #[arg(long, value_name = "TEXT")]
    pub prompt: Option<String>,

    /// Additional flags to pass to the agent (space-separated)
    #[arg(long, value_name = "FLAGS")]
    pub agent_flags: Option<String>,
}

/// Parse boolean values from command line (true/false, yes/no, 1/0, y/n)
fn parse_bool(s: &str) -> Result<bool, String> {
    match s.to_lowercase().as_str() {
        "true" | "yes" | "1" | "y" => Ok(true),
        "false" | "no" | "0" | "n" => Ok(false),
        _ => Err(format!(
            "Invalid boolean value: {}. Use true/false, yes/no, 1/0, or y/n",
            s
        )),
    }
}

impl AgentStartArgs {
    /// Resolve a usable Python interpreter name.
    /// Prefer `python`, fall back to `python3`. Honor `PYTHON` if set.
    #[allow(dead_code)] // Reserved for future use
    fn resolve_python_interpreter() -> anyhow::Result<String> {
        if let Ok(py) = std::env::var("PYTHON") {
            if !py.trim().is_empty() {
                return Ok(py);
            }
        }

        for candidate in ["python", "python3"] {
            let ok = stdprocess::Command::new(candidate)
                .arg("--version")
                .stdout(stdprocess::Stdio::null())
                .stderr(stdprocess::Stdio::null())
                .status();
            if let Ok(status) = ok {
                if status.success() {
                    return Ok(candidate.to_string());
                }
            }
        }

        Err(anyhow::anyhow!(
            "No Python interpreter found. Install Python or set $PYTHON"
        ))
    }
    /// Run the agent start command
    pub async fn run(self) -> anyhow::Result<()> {
        // Convert CLI agent type to core agent type
        let agent_type: AgentType = self.agent.clone();

        // Validate command-line arguments
        self.validate_args()?;

        // Get the agent executor from the abstraction layer
        let agent: Box<dyn AgentExecutor> = match agent_type {
            AgentType::Claude => Box::new(ah_agents::claude()),
            AgentType::Codex => Box::new(ah_agents::codex()),
            AgentType::Copilot => Box::new(ah_agents::copilot_cli()),
            AgentType::CursorCli => Box::new(ah_agents::cursor_cli()),
            AgentType::Gemini => Box::new(ah_agents::gemini()),
            // For agents not yet implemented in ah-agents, fall back to old logic
            AgentType::Opencode | AgentType::Qwen | AgentType::Goose => {
                return self.run_legacy_agent(agent_type).await;
            }
        };

        // Handle LLM API proxy configuration
        let config = if let Some(proxy_url) = &self.llm_api_proxy_url {
            let session_api_key = self
                .prepare_proxy_session(proxy_url, agent.as_ref(), agent_type.clone())
                .await?;
            let mut config = self.build_agent_config(agent_type)?;
            config = config.api_server(proxy_url.clone());
            config = config.api_key(session_api_key);
            config
        } else {
            self.build_agent_config(agent_type)?
        };

        // Execute the agent (replace current process)
        if self.sandbox {
            #[cfg(target_os = "macos")]
            {
                use ah_macos_launcher::launch_in_sandbox;

                // Prepare command to get env vars and args
                let cmd = agent.prepare_launch(config.clone()).await?;

                // Apply environment variables to current process so they propagate
                for (key, val) in cmd.as_std().get_envs() {
                    if let Some(val_str) = val {
                        std::env::set_var(key, val_str);
                    } else {
                        std::env::remove_var(key);
                    }
                }

                // Extract program and args
                let program = cmd.as_std().get_program().to_string_lossy().to_string();
                let args: Vec<String> =
                    cmd.as_std().get_args().map(|s| s.to_string_lossy().to_string()).collect();

                // Construct full command vector (program + args)
                let mut full_cmd = vec![program];
                full_cmd.extend(args);

                let launcher_config = crate::sandbox::configure_macos_launcher(
                    full_cmd,
                    self.allow_network.unwrap_or(false),
                    Some(&config.working_dir),
                    &self.mount_rw,
                );

                // Launch in sandbox
                launch_in_sandbox(launcher_config).context("Failed to launch agent in sandbox")?;

                // This should never return on success
                unreachable!("launch_in_sandbox should replace the current process")
            }

            #[cfg(target_os = "linux")]
            {
                // Linux sandbox implementation for real agents
                use sandbox_core::ProcessConfig;

                // Validate sandbox type
                if self.sandbox_type != "local" {
                    return Err(anyhow::anyhow!(
                        "Only 'local' sandbox type is currently supported, got '{}'",
                        self.sandbox_type
                    ));
                }

                // Create sandbox configuration from CLI parameters
                #[allow(deprecated)]
                let mut sandbox = crate::sandbox::create_sandbox_from_args(
                    self.allow_network.unwrap_or(false),
                    self.allow_containers.unwrap_or(false),
                    self.allow_kvm.unwrap_or(false),
                    self.seccomp.unwrap_or(false),
                    self.seccomp_debug.unwrap_or(false),
                    &self.mount_rw,
                    &self.overlay,
                    Some(&config.working_dir),
                )?;

                // Prepare command
                let cmd = agent.prepare_launch(config.clone()).await?;
                let program = cmd.as_std().get_program().to_string_lossy().to_string();
                let args: Vec<String> =
                    cmd.as_std().get_args().map(|s| s.to_string_lossy().to_string()).collect();
                let mut full_cmd = vec![program];
                full_cmd.extend(args);

                // Extract env vars
                let mut env_vars = Vec::new();
                for (key, val) in cmd.as_std().get_envs() {
                    if let Some(val_str) = val {
                        env_vars.push((
                            key.to_string_lossy().to_string(),
                            val_str.to_string_lossy().to_string(),
                        ));
                    }
                }

                let allow_network = self.allow_network.unwrap_or(false);
                let process_config = ProcessConfig {
                    command: full_cmd,
                    working_dir: Some(config.working_dir.to_string_lossy().to_string()),
                    env: env_vars,
                    tmpfs_size: None,    // Use default tmpfs size for /tmp isolation
                    net_isolation: true, // Network isolation enabled by default
                    allow_internet: allow_network, // Internet access via slirp4netns if requested
                    agentfs_overlay: None, // AgentFS bind-mount not configured for agent start
                };

                sandbox = sandbox.with_process_config(process_config);

                let exec_result = sandbox.exec_process().await;

                if let Err(err) = sandbox.stop() {
                    tracing::warn!(error = %err, "Sandbox stop cleanup encountered an error");
                }

                if let Err(err) = sandbox.cleanup().await {
                    tracing::warn!(error = %err, "Sandbox filesystem cleanup encountered an error");
                }

                exec_result.context("Failed to execute agent in sandbox")?;
                Ok(())
            }

            #[cfg(not(any(target_os = "linux", target_os = "macos")))]
            {
                tracing::warn!(
                    "Sandboxing is not supported on this platform. Running without sandbox."
                );
                agent.exec(config).await?;
                unreachable!("exec() should replace the current process")
            }
        } else {
            agent.exec(config).await?;
            unreachable!("exec() should replace the current process")
        }
    }

    /// Prepare a session with the LLM API proxy
    async fn prepare_proxy_session(
        &self,
        proxy_url: &str,
        agent: &dyn AgentExecutor,
        agent_type: AgentType,
    ) -> anyhow::Result<String> {
        // Generate or use provided API key for session
        let session_api_key = self.llm_api_key.clone().unwrap_or_else(|| {
            // Generate a random API key for the session
            format!("sk-session-{}", uuid::Uuid::new_v4().simple())
        });

        // Get API key from the agent
        let api_key = agent.get_user_api_key().await?.unwrap_or_default();

        if api_key.is_empty() {
            return Err(anyhow::anyhow!(
                "No API credentials found for agent {:?}. Please ensure credentials are available.",
                agent_type
            ));
        }

        // Determine provider configuration based on agent type
        let (provider_name, base_url, model_mappings) = match agent_type {
            AgentType::Codex => (
                "openai",
                "https://api.openai.com/v1",
                vec![
                    serde_json::json!({"source_pattern": "gpt-4", "provider": "openai", "model": "gpt-4o"}),
                    serde_json::json!({"source_pattern": "gpt-3.5", "provider": "openai", "model": "gpt-3.5-turbo"}),
                ],
            ),
            AgentType::Claude => (
                "anthropic",
                "https://api.anthropic.com",
                vec![
                    serde_json::json!({"source_pattern": "claude", "provider": "anthropic", "model": "claude-3-5-sonnet-20241022"}),
                    serde_json::json!({"source_pattern": "haiku", "provider": "anthropic", "model": "claude-3-5-haiku-20241022"}),
                    serde_json::json!({"source_pattern": "opus", "provider": "anthropic", "model": "claude-3-opus-20240229"}),
                    serde_json::json!({"source_pattern": "sonnet", "provider": "anthropic", "model": "claude-3-5-sonnet-20241022"}),
                ],
            ),
            AgentType::Copilot => (
                "github",
                "https://api.github.com",
                vec![
                    serde_json::json!({"source_pattern": "copilot", "provider": "github", "model": "claude-sonnet-4-5-20250929"}),
                ],
            ),
            AgentType::Gemini => (
                "google",
                "https://generativelanguage.googleapis.com/v1beta",
                vec![
                    serde_json::json!({"source_pattern": "gemini", "provider": "google", "model": "gemini-2.5-pro"}),
                ],
            ),
            // For other agents, use OpenRouter as fallback
            _ => (
                "openrouter",
                "https://openrouter.ai/api/v1",
                vec![
                    serde_json::json!({"source_pattern": "gpt-4", "provider": "openrouter", "model": "openai/gpt-4o"}),
                ],
            ),
        };

        // Prepare session with proxy using new API format
        let client = Client::new();
        let prepare_url = format!("{}/prepare-session", proxy_url.trim_end_matches('/'));

        let request_body = json!({
            "api_key": session_api_key,
            "providers": [
                {
                    "name": provider_name,
                    "base_url": base_url,
                    "headers": {
                        "authorization": format!("Bearer {}", api_key)
                    }
                }
            ],
            "model_mappings": model_mappings,
            "default_provider": provider_name
        });

        let response = client
            .post(&prepare_url)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to prepare proxy session: {}", e))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Proxy session preparation failed: {}",
                error_text
            ));
        }

        let response_body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to parse proxy response: {}", e))?;

        tracing::info!("Proxy session prepared successfully");
        if let Some(session_id) = response_body.get("session_id").and_then(|v| v.as_str()) {
            tracing::info!(session_id = %session_id, "Proxy session created");
        }

        Ok(session_api_key)
    }

    /// Validate command-line arguments
    fn validate_args(&self) -> anyhow::Result<()> {
        // --non-interactive requires --prompt
        if self.non_interactive && self.prompt.is_none() {
            return Err(anyhow::anyhow!(
                "The --prompt parameter is required when --non-interactive is specified.\n\
                 In non-interactive mode, the agent needs a prompt to execute.\n\
                 Use --prompt \"your task description\" or omit --non-interactive for interactive mode."
            ));
        }

        Ok(())
    }

    /// Build AgentLaunchConfig from command line arguments
    fn build_agent_config(&self, agent_type: AgentType) -> anyhow::Result<AgentLaunchConfig> {
        let mut config = AgentLaunchConfig::new(self.build_home_dir(agent_type.clone())?);

        // Set prompt if provided
        if let Some(prompt) = &self.prompt {
            config = config.prompt(prompt.clone());
        }

        // Set interactive mode based on flags
        config = config.interactive(!self.non_interactive);

        // Set API server if provided (will be overridden if using proxy)
        if let Some(api) = &self.llm_api {
            config = config.api_server(api.clone());
        }

        // Set API key if provided (will be overridden if using proxy)
        if let Some(key) = &self.llm_api_key {
            config = config.api_key(key.clone());
        }

        // Set working directory
        let cwd = self
            .cwd
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));
        config = config.working_dir(cwd);

        // Add additional environment variables from agent_flags if they look like KEY=VALUE
        if let Some(flags) = &self.agent_flags {
            for flag in flags.split_whitespace() {
                if let Some((key, value)) = flag.split_once('=') {
                    config = config.env(key, value);
                }
            }
        }

        // Enable credential copying by default
        config = config.copy_credentials(true);

        // Set unrestricted mode when running in sandbox
        // TODO: Remove this once sandbox is properly integrated
        //if self.sandbox {
        config = config.unrestricted(true);
        //}

        // Enable web search if requested
        if self.allow_web_search {
            config = config.web_search(true);
        }

        // Configure output format
        // Map OutputFormat to json_output flag for agents that support it
        match self.output {
            OutputFormat::Json | OutputFormat::JsonNormalized => {
                config = config.json_output(true);
            }
            OutputFormat::Text | OutputFormat::TextNormalized => {
                config = config.json_output(false);
            }
        }

        // Set model based on agent type and precedence rules
        let model = match agent_type {
            AgentType::Codex => {
                // codex-model takes precedence over model
                self.codex_model
                    .clone()
                    .or_else(|| self.model.clone())
                    .unwrap_or_else(|| "gpt-5-codex".to_string())
            }
            AgentType::Claude => {
                // claude-model takes precedence over model
                self.claude_model
                    .clone()
                    .or_else(|| self.model.clone())
                    .unwrap_or_else(|| "sonnet".to_string())
            }
            AgentType::Copilot => {
                // copilot-model takes precedence over model
                self.copilot_model
                    .clone()
                    .or_else(|| self.model.clone())
                    .unwrap_or_else(|| "claude-sonnet-4.5".to_string())
            }

            AgentType::Gemini => {
                // gemini-model takes precedence over model
                self.gemini_model
                    .clone()
                    .or_else(|| self.model.clone())
                    .unwrap_or_else(|| "gemini-2.5-pro".to_string())
            }
            // For other agents, use the general model flag or None
            _ => self.model.clone().unwrap_or_default(),
        };

        if !model.is_empty() {
            config = config.model(model);
        }

        // Set snapshot command to use full path to ah agent fs snapshot
        if let Ok(ah_path) = ah_full_path() {
            config = config.snapshot_cmd(format!("{} agent fs snapshot", ah_path));
        }

        Ok(config)
    }

    /// Build the home directory path for the agent
    fn build_home_dir(&self, agent_type: AgentType) -> anyhow::Result<PathBuf> {
        let base_dir = std::env::var("AH_HOME").map(PathBuf::from).unwrap_or_else(|_| {
            dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp")).join(".agent-harbor")
        });

        let agent_name = match agent_type {
            AgentType::Claude => "claude",
            AgentType::Codex => "codex",
            AgentType::CursorCli => "cursor-cli",
            AgentType::Copilot => "copilot",
            AgentType::Gemini => "gemini",
            _ => "unknown",
        };

        Ok(base_dir.join("agents").join(agent_name))
    }

    /// Run legacy agent implementations (not yet migrated to ah-agents)
    async fn run_legacy_agent(&self, agent_type: AgentType) -> anyhow::Result<()> {
        match agent_type {
            AgentType::Opencode => anyhow::bail!("OpenCode agent is not yet implemented"),
            AgentType::Qwen => anyhow::bail!("Qwen agent is not yet implemented"),
            AgentType::Goose => anyhow::bail!("Goose agent is not yet implemented"),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bool() {
        assert!(parse_bool("true").unwrap());
        assert!(!parse_bool("false").unwrap());
        assert!(parse_bool("yes").unwrap());
        assert!(!parse_bool("no").unwrap());
        assert!(parse_bool("1").unwrap());
        assert!(!parse_bool("0").unwrap());
        assert!(parse_bool("y").unwrap());
        assert!(!parse_bool("n").unwrap());

        assert!(parse_bool("invalid").is_err());
    }
}
