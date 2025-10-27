// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Agent start command implementation

use ah_agents::{AgentExecutor, AgentLaunchConfig, credentials};
use ah_core::agent_types::AgentType;
use anyhow::Context;
use clap::{Args, ValueEnum};
use reqwest::Client;
use serde_json::json;
use std::path::PathBuf;
use std::process::Stdio;
use uuid;

/// CLI-specific agent type enum that derives clap::ValueEnum
#[derive(Clone, Debug, PartialEq, ValueEnum)]
pub enum CliAgentType {
    /// Mock agent for testing
    Mock,
    /// OpenAI Codex CLI agent
    Codex,
    /// Anthropic Claude Code agent
    Claude,
    /// Google Gemini CLI agent
    Gemini,
    /// OpenCode agent
    Opencode,
    /// Qwen Code agent
    Qwen,
    /// Cursor CLI agent
    CursorCli,
    /// Goose agent
    Goose,
}

impl From<CliAgentType> for AgentType {
    fn from(cli: CliAgentType) -> Self {
        match cli {
            CliAgentType::Mock => AgentType::Mock,
            CliAgentType::Codex => AgentType::Codex,
            CliAgentType::Claude => AgentType::Claude,
            CliAgentType::Gemini => AgentType::Gemini,
            CliAgentType::Opencode => AgentType::Opencode,
            CliAgentType::Qwen => AgentType::Qwen,
            CliAgentType::CursorCli => AgentType::CursorCli,
            CliAgentType::Goose => AgentType::Goose,
        }
    }
}

/// Working copy mode for agent execution
#[derive(Clone, Debug, PartialEq, ValueEnum)]
pub enum WorkingCopyMode {
    /// Execute agent directly in the current working directory
    InPlace,
    /// Use filesystem snapshots for workspace isolation
    Snapshots,
}

/// Output format for agent execution
#[derive(Clone, Debug, PartialEq, ValueEnum)]
pub enum OutputFormat {
    /// Display agent output unmodified (default)
    Text,
    /// Display textual output with consistent structure regardless of agent type
    #[clap(name = "text-normalized")]
    TextNormalized,
    /// Display JSON output if available (e.g., codex --json)
    Json,
    /// Map JSON to agent-harbor defined schema consistent across agent types
    #[clap(name = "json-normalized")]
    JsonNormalized,
}

/// Agent start command arguments
#[derive(Args)]
pub struct AgentStartArgs {
    /// Agent type to start
    #[arg(long, default_value = "mock")]
    pub agent: CliAgentType,

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
    #[arg(long, default_value = "in-place")]
    pub working_copy: WorkingCopyMode,

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

    /// Model to use specifically for Claude agent (overrides --model)
    #[arg(long)]
    pub claude_model: Option<String>,

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
    /// Run the agent start command
    pub async fn run(self) -> anyhow::Result<()> {
        // Convert CLI agent type to core agent type
        let agent_type: AgentType = self.agent.clone().into();

        // Handle mock agent separately (doesn't use abstraction layer)
        if matches!(agent_type, AgentType::Mock) {
            return self.run_mock_agent().await;
        }

        // Get the agent executor from the abstraction layer
        let agent: Box<dyn AgentExecutor> = match agent_type {
            AgentType::Claude => Box::new(ah_agents::claude()),
            AgentType::Codex => Box::new(ah_agents::codex()),
            AgentType::CursorCli => Box::new(ah_agents::cursor_cli()),
            // For agents not yet implemented in ah-agents, fall back to old logic
            AgentType::Gemini | AgentType::Opencode | AgentType::Qwen | AgentType::Goose => {
                return self.run_legacy_agent(agent_type).await;
            }
            AgentType::Mock => unreachable!(), // handled above
        };

        // Handle LLM API proxy configuration
        let mut config = if let Some(proxy_url) = &self.llm_api_proxy_url {
            let session_api_key =
                self.prepare_proxy_session(proxy_url, &agent, agent_type.clone()).await?;
            let mut config = self.build_agent_config(agent_type)?;
            config = config.api_server(proxy_url.clone());
            config = config.api_key(session_api_key);
            config
        } else {
            self.build_agent_config(agent_type)?
        };

        // Execute the agent (replace current process)
        agent.exec(config).await?;

        // This should never return on success
        unreachable!("exec() should replace the current process")
    }

    /// Prepare a session with the LLM API proxy
    async fn prepare_proxy_session(
        &self,
        proxy_url: &str,
        agent: &Box<dyn AgentExecutor>,
        agent_type: AgentType,
    ) -> anyhow::Result<String> {
        // Generate or use provided API key for session
        let session_api_key = self.llm_api_key.clone().unwrap_or_else(|| {
            // Generate a random API key for the session
            format!("sk-session-{}", uuid::Uuid::new_v4().simple())
        });

        // Get API key from the agent
        let api_key = agent.get_user_api_key().await?.unwrap_or_else(|| String::new());

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

        println!("✅ Proxy session prepared successfully");
        if let Some(session_id) = response_body.get("session_id").and_then(|v| v.as_str()) {
            println!("   Session ID: {}", session_id);
        }

        Ok(session_api_key)
    }

    /// Build AgentLaunchConfig from command line arguments
    fn build_agent_config(&self, agent_type: AgentType) -> anyhow::Result<AgentLaunchConfig> {
        let mut config = AgentLaunchConfig::new(
            self.prompt.clone().unwrap_or_else(|| "Continue working".to_string()),
            self.build_home_dir(agent_type.clone())?,
        );

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
        if self.sandbox {
            config = config.unrestricted(true);
        }

        // Enable web search if requested
        if self.allow_web_search {
            config = config.web_search(true);
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
            // For other agents, use the general model flag or None
            _ => self.model.clone().unwrap_or_default(),
        };

        if !model.is_empty() {
            config = config.model(model);
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
            _ => "unknown",
        };

        Ok(base_dir.join("agents").join(agent_name))
    }

    /// Run legacy agent implementations (not yet migrated to ah-agents)
    async fn run_legacy_agent(&self, agent_type: AgentType) -> anyhow::Result<()> {
        match agent_type {
            AgentType::Gemini => self.run_mock_agent().await,
            AgentType::Opencode => self.run_mock_agent().await,
            AgentType::Qwen => self.run_mock_agent().await,
            AgentType::Goose => self.run_mock_agent().await,
            _ => Ok(()),
        }
    }

    /// Run the mock agent for testing purposes
    async fn run_mock_agent(&self) -> anyhow::Result<()> {
        use tokio::process::Command;

        // Determine the working directory
        let cwd = if let Some(cwd) = &self.cwd {
            cwd.clone()
        } else {
            std::env::current_dir()?
        };

        // Handle workspace preparation for snapshots mode
        let actual_cwd = if self.working_copy == WorkingCopyMode::Snapshots {
            // Prepare a snapshot-based workspace
            match crate::sandbox::prepare_workspace_with_fallback(&cwd).await {
                Ok(prepared_workspace) => prepared_workspace.exec_path,
                Err(e) => {
                    return Err(e.into());
                }
            }
        } else {
            cwd.clone()
        };

        // Handle sandbox mode
        if self.sandbox {
            return self.run_mock_agent_in_sandbox(actual_cwd).await;
        }

        // Build the command to run the mock agent
        let mut cmd = Command::new("python");
        cmd.arg("-m")
            .arg("src.cli")
            .current_dir(&actual_cwd)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        // Parse agent flags
        let agent_flags: Vec<&str> = self
            .agent_flags
            .as_ref()
            .map(|s| s.split_whitespace().collect())
            .unwrap_or_default();

        // Check if this is a scenario run or demo run
        let is_scenario_run = agent_flags.iter().any(|flag| flag.contains("--scenario"));

        if is_scenario_run {
            cmd.arg("run");
            // Add all the agent flags
            for flag in &agent_flags {
                cmd.arg(flag);
            }
        } else {
            // Run in demo mode
            cmd.arg("demo").arg("--workspace").arg(&cwd);
            // Add any additional flags (like --tui-testing-uri)
            for flag in &agent_flags {
                cmd.arg(flag);
            }
        }

        // Note: TUI_TESTING_URI should only be passed when explicitly requested
        // We don't automatically pass it from environment to avoid test interference

        // Note: PYTHONPATH and PATH are set by test helper functions, not in production code
        // to avoid assuming we're running within the workspace

        // Always pass the TUI_TESTING_URI environment variable to the mock agent
        if let Ok(tui_testing_uri) = std::env::var("TUI_TESTING_URI") {
            cmd.env("TUI_TESTING_URI", &tui_testing_uri);
        }

        // Execute the mock agent
        let status = cmd.status().await?;

        if status.success() {
            Ok(())
        } else {
            anyhow::bail!("Mock agent exited with status: {}", status);
        }
    }

    /// Run the mock agent inside a sandbox environment
    async fn run_mock_agent_in_sandbox(
        &self,
        actual_cwd: std::path::PathBuf,
    ) -> anyhow::Result<()> {
        #[cfg(target_os = "linux")]
        {
            use sandbox_core::{ProcessConfig, ProcessManager, Sandbox};

            // Validate sandbox type
            if self.sandbox_type != "local" {
                return Err(anyhow::anyhow!(
                    "Only 'local' sandbox type is currently supported, got '{}'",
                    self.sandbox_type
                ));
            }

            // Create sandbox configuration from CLI parameters
            let mut sandbox = crate::sandbox::create_sandbox_from_args(
                &self.allow_network.map(|b| if b { "yes" } else { "no" }).unwrap_or("no"),
                &self.allow_containers.map(|b| if b { "yes" } else { "no" }).unwrap_or("no"),
                &self.allow_kvm.map(|b| if b { "yes" } else { "no" }).unwrap_or("no"),
                &self.seccomp.map(|b| if b { "yes" } else { "no" }).unwrap_or("no"),
                &self.seccomp_debug.map(|b| if b { "yes" } else { "no" }).unwrap_or("no"),
                &self.mount_rw,
                &self.overlay,
            )?;

            // Start the sandbox (sets up namespaces, cgroups, etc.)
            sandbox.start().await.context("Failed to start sandbox environment")?;

            // Configure the process to run the mock agent
            let mut agent_cmd = vec![
                "python".to_string(),
                "-m".to_string(),
                "src.cli".to_string(),
            ];

            // Parse agent flags
            let agent_flags: Vec<String> = self
                .agent_flags
                .as_ref()
                .map(|s| s.split_whitespace().map(|s| s.to_string()).collect())
                .unwrap_or_default();

            // Check if this is a scenario run or demo run
            let is_scenario_run = agent_flags.iter().any(|flag| flag.contains("--scenario"));

            if is_scenario_run {
                agent_cmd.push("run".to_string());
                // Add all the agent flags
                for flag in &agent_flags {
                    agent_cmd.push(flag.clone());
                }
            } else {
                // Run in demo mode
                agent_cmd.push("demo".to_string());
                agent_cmd.push("--workspace".to_string());
                agent_cmd.push(actual_cwd.to_string_lossy().to_string());
                // Add any additional flags (like --tui-testing-uri)
                for flag in &agent_flags {
                    agent_cmd.push(flag.clone());
                }
            }

            // Create process configuration
            let config = self.build_agent_config(AgentType::from(self.agent.clone()))?;
            let process_config = ProcessConfig {
                command: agent_cmd,
                working_dir: Some(actual_cwd.to_string_lossy().to_string()),
                env: config.env_vars,
            };

            // Set up the process manager with the agent command
            let process_manager = sandbox_core::ProcessManager::with_config(process_config);

            // Execute the agent in the sandbox
            process_manager
                .exec_as_pid1()
                .context("Failed to execute mock agent in sandbox")?;

            // Clean up the sandbox
            sandbox.stop().context("Failed to clean up sandbox environment")?;

            Ok(())
        }
        #[cfg(not(target_os = "linux"))]
        {
            Err(anyhow::anyhow!(
                "Sandbox functionality is only available on Linux"
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bool() {
        assert_eq!(parse_bool("true").unwrap(), true);
        assert_eq!(parse_bool("false").unwrap(), false);
        assert_eq!(parse_bool("yes").unwrap(), true);
        assert_eq!(parse_bool("no").unwrap(), false);
        assert_eq!(parse_bool("1").unwrap(), true);
        assert_eq!(parse_bool("0").unwrap(), false);
        assert_eq!(parse_bool("y").unwrap(), true);
        assert_eq!(parse_bool("n").unwrap(), false);

        assert!(parse_bool("invalid").is_err());
    }
}
