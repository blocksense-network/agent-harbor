//! Agent start command implementation

use crate::sandbox::{parse_bool_flag, prepare_workspace_with_fallback};
use anyhow::Context;
use clap::{Args, ValueEnum};
use std::path::PathBuf;
use std::process::Stdio;

/// Supported agent types
#[derive(Clone, Debug, PartialEq, ValueEnum)]
pub enum AgentType {
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
    pub agent: AgentType,

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

    /// Working copy mode
    #[arg(long, default_value = "in-place")]
    pub working_copy: WorkingCopyMode,

    /// Working directory for agent execution
    #[arg(long)]
    pub cwd: Option<PathBuf>,

    /// Task ID to associate with this agent session
    #[arg(long)]
    pub task_id: Option<String>,

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

    /// Additional writable paths to bind mount
    #[arg(long)]
    pub mount_rw: Vec<PathBuf>,

    /// Paths to promote to copy-on-write overlays
    #[arg(long)]
    pub overlay: Vec<PathBuf>,

    /// Custom prompt text to pass to the agent (overrides task/session prompt)
    #[arg(long, value_name = "TEXT")]
    pub prompt: Option<String>,

    /// Additional flags to pass to the agent
    #[arg(long, value_name = "FLAG")]
    pub agent_flags: Vec<String>,
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
        // For milestone 2.4.1, we support the "mock" agent for E2E testing
        if matches!(self.agent, AgentType::Mock) {
            return self.run_mock_agent().await;
        }

        // For milestone 2.4.4, we support the "codex" agent in non-interactive mode
        if matches!(self.agent, AgentType::Codex) && self.non_interactive {
            return self.run_codex_agent().await;
        }

        // For Claude Code agent
        if matches!(self.agent, AgentType::Claude) {
            return self.run_claude_agent().await;
        }

        // For Gemini CLI agent
        if matches!(self.agent, AgentType::Gemini) {
            return self.run_gemini_agent().await;
        }

        // For OpenCode agent
        if matches!(self.agent, AgentType::Opencode) {
            return self.run_opencode_agent().await;
        }

        // For Qwen Code agent
        if matches!(self.agent, AgentType::Qwen) {
            return self.run_qwen_agent().await;
        }

        // For Cursor CLI agent
        if matches!(self.agent, AgentType::CursorCli) {
            return self.run_cursor_cli_agent().await;
        }

        // For Goose agent
        if matches!(self.agent, AgentType::Goose) {
            return self.run_goose_agent().await;
        }

        // For other agents, this is still a placeholder that will be replaced
        // when milestone 2.4 is implemented.
        eprintln!(
            "Agent start command is not yet implemented for agent '{:?}'.",
            self.agent
        );
        eprintln!("This is a placeholder for milestone 2.4 implementation.");
        eprintln!("E2E tests in milestone 2.4.1 will validate this command once implemented.");

        // Print the parsed arguments for debugging
        eprintln!("Parsed arguments:");
        eprintln!("  agent: {:?}", self.agent);
        eprintln!("  prompt: {:?}", self.prompt);
        eprintln!("  non_interactive: {}", self.non_interactive);
        eprintln!("  output: {:?}", self.output);
        eprintln!("  llm_api: {:?}", self.llm_api);
        eprintln!("  llm_api_key: {:?}", self.llm_api_key);
        eprintln!("  working_copy: {:?}", self.working_copy);
        eprintln!("  cwd: {:?}", self.cwd);
        eprintln!("  task_id: {:?}", self.task_id);
        eprintln!("  sandbox: {}", self.sandbox);
        eprintln!("  agent_flags: {:?}", self.agent_flags);

        if self.sandbox {
            eprintln!("  sandbox_type: {}", self.sandbox_type);
            eprintln!("  allow_network: {:?}", self.allow_network);
            eprintln!("  allow_containers: {:?}", self.allow_containers);
            eprintln!("  allow_kvm: {:?}", self.allow_kvm);
            eprintln!("  seccomp: {:?}", self.seccomp);
            eprintln!("  seccomp_debug: {:?}", self.seccomp_debug);
            eprintln!("  mount_rw: {:?}", self.mount_rw);
            eprintln!("  overlay: {:?}", self.overlay);
        }

        // Exit with success for now - the E2E tests will validate the actual behavior
        Ok(())
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
            eprintln!("Preparing workspace with filesystem snapshots...");
            match crate::sandbox::prepare_workspace_with_fallback(&cwd).await {
                Ok(prepared_workspace) => {
                    eprintln!("Workspace prepared at: {:?}", prepared_workspace.exec_path);
                    prepared_workspace.exec_path
                }
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

        // Check if this is a scenario run or demo run
        let is_scenario_run = self.agent_flags.iter().any(|flag| flag == "--scenario");

        if is_scenario_run {
            cmd.arg("run");
            // Add all the agent flags
            for flag in &self.agent_flags {
                cmd.arg(flag);
            }
        } else {
            // Run in demo mode
            cmd.arg("demo").arg("--workspace").arg(&cwd);
            // Add any additional flags (like --tui-testing-uri)
            for flag in &self.agent_flags {
                cmd.arg(flag);
            }
        }

        // Note: TUI_TESTING_URI should only be passed when explicitly requested
        // We don't automatically pass it from environment to avoid test interference

        // Set PYTHONPATH to find the mock agent
        // Try to find the workspace root relative to the current executable
        let mut pythonpath_set = false;
        if let Ok(current_exe) = std::env::current_exe() {
            if let Some(workspace_root) =
                current_exe.parent().and_then(|p| p.parent()).and_then(|p| p.parent())
            {
                let pythonpath = format!("{}/tests/tools/mock-agent", workspace_root.display());
                eprintln!("Setting PYTHONPATH to: {}", pythonpath);
                cmd.env("PYTHONPATH", pythonpath);
                pythonpath_set = true;
            }
        }

        // Fallback: try CARGO_MANIFEST_DIR
        if !pythonpath_set {
            if let Ok(cargo_manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
                let workspace_root =
                    std::path::Path::new(&cargo_manifest_dir).parent().unwrap().parent().unwrap();
                let pythonpath = format!("{}/tests/tools/mock-agent", workspace_root.display());
                eprintln!("Setting PYTHONPATH to (fallback): {}", pythonpath);
                cmd.env("PYTHONPATH", pythonpath);
                pythonpath_set = true;
            }
        }

        if !pythonpath_set {
            eprintln!("Warning: Could not determine PYTHONPATH for mock agent");
        }

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

            eprintln!("Running mock agent in sandbox environment...");

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

            // Check if this is a scenario run or demo run
            let is_scenario_run = self.agent_flags.iter().any(|flag| flag == "--scenario");

            if is_scenario_run {
                agent_cmd.push("run".to_string());
                // Add all the agent flags
                for flag in &self.agent_flags {
                    agent_cmd.push(flag.clone());
                }
            } else {
                // Run in demo mode
                agent_cmd.push("demo".to_string());
                agent_cmd.push("--workspace".to_string());
                agent_cmd.push(actual_cwd.to_string_lossy().to_string());
                // Add any additional flags (like --tui-testing-uri)
                for flag in &self.agent_flags {
                    agent_cmd.push(flag.clone());
                }
            }

            // Create process configuration
            let process_config = ProcessConfig {
                command: agent_cmd,
                working_dir: Some(actual_cwd.to_string_lossy().to_string()),
                env: self.build_agent_env(),
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

    /// Build environment variables for the agent process
    fn build_agent_env(&self) -> Vec<(String, String)> {
        let mut env = Vec::new();

        // Set PYTHONPATH to find the mock agent
        if let Ok(current_exe) = std::env::current_exe() {
            if let Some(workspace_root) =
                current_exe.parent().and_then(|p| p.parent()).and_then(|p| p.parent())
            {
                let pythonpath = format!("{}/tests/tools/mock-agent", workspace_root.display());
                env.push(("PYTHONPATH".to_string(), pythonpath));
            }
        }

        // Fallback: try CARGO_MANIFEST_DIR
        if env.is_empty() {
            if let Ok(cargo_manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
                let workspace_root =
                    std::path::Path::new(&cargo_manifest_dir).parent().unwrap().parent().unwrap();
                let pythonpath = format!("{}/tests/tools/mock-agent", workspace_root.display());
                env.push(("PYTHONPATH".to_string(), pythonpath));
            }
        }

        // Always pass the TUI_TESTING_URI environment variable to the mock agent
        if let Ok(tui_testing_uri) = std::env::var("TUI_TESTING_URI") {
            env.push(("TUI_TESTING_URI".to_string(), tui_testing_uri));
        }

        // Set AH_HOME for database operations
        if let Ok(ah_home_dir) = std::env::var("AH_HOME") {
            env.push(("AH_HOME".to_string(), ah_home_dir));
        }

        // Set git environment variables for consistent behavior
        env.push(("GIT_CONFIG_NOSYSTEM".to_string(), "1".to_string()));
        env.push(("GIT_TERMINAL_PROMPT".to_string(), "0".to_string()));
        env.push(("GIT_ASKPASS".to_string(), "echo".to_string()));
        env.push(("SSH_ASKPASS".to_string(), "echo".to_string()));

        env
    }

    /// Run the Codex agent in non-interactive mode
    async fn run_codex_agent(&self) -> anyhow::Result<()> {
        use tokio::process::Command;

        eprintln!("Starting Codex agent in non-interactive mode...");

        // Determine the working directory
        let cwd = if let Some(cwd) = &self.cwd {
            cwd.clone()
        } else {
            std::env::current_dir()?
        };

        // Build the codex command with exec
        let mut cmd = Command::new("codex");

        // Add the prompt as a positional argument if provided, otherwise use exec mode
        if let Some(prompt) = &self.prompt {
            cmd.arg(prompt);
        } else {
            cmd.arg("exec");
        }

        // Handle output format
        match self.output {
            OutputFormat::Json => {
                cmd.arg("--json");
            }
            OutputFormat::Text | OutputFormat::TextNormalized => {
                // No additional arguments needed for text output
            }
            OutputFormat::JsonNormalized => {
                // For now, treat as json - normalization logic would be implemented later
                cmd.arg("--json");
            }
        }

        // Set working directory
        cmd.current_dir(cwd);

        // Set environment variables for Codex
        if let Ok(home) = std::env::var("HOME") {
            cmd.env("CODEX_HOME", format!("{}/.codex", home));
        }

        // Set custom LLM API if provided
        if let Some(llm_api) = &self.llm_api {
            cmd.env("CODEX_API_BASE", llm_api);
            let api_key = self.llm_api_key.as_deref().unwrap_or("");
            cmd.env("CODEX_API_KEY", api_key);
        }

        let json_flag = matches!(self.output, OutputFormat::Json | OutputFormat::JsonNormalized);
        eprintln!("Running codex command: codex exec{}", if json_flag { " --json" } else { "" });

        // Execute the command
        let status = cmd.status().await?;

        if status.success() {
            eprintln!("Codex agent completed successfully");
            Ok(())
        } else {
            eprintln!("Codex agent exited with status: {}", status);
            anyhow::bail!("Codex agent exited with status: {}", status);
        }
    }

    /// Run the Claude Code agent
    async fn run_claude_agent(&self) -> anyhow::Result<()> {
        use tokio::process::Command;

        eprintln!("Starting Claude Code agent...");

        // Determine the working directory
        let cwd = if let Some(cwd) = &self.cwd {
            cwd.clone()
        } else {
            std::env::current_dir()?
        };

        // Build the claude command
        let mut cmd = Command::new("claude");

        // Add the prompt as a positional argument if provided
        if let Some(prompt) = &self.prompt {
            cmd.arg(prompt);
        }

        // For now, Claude Code doesn't have a non-interactive mode like Codex
        // We just pass through any agent flags as arguments
        for flag in &self.agent_flags {
            cmd.arg(flag);
        }

        // Set working directory
        cmd.current_dir(cwd);

        // Set environment variables for Claude Code
        if let Ok(home) = std::env::var("HOME") {
            // Claude Code uses standard home directory
        }

        // Set custom LLM API if provided
        if let Some(llm_api) = &self.llm_api {
            cmd.env("ANTHROPIC_BASE_URL", llm_api);
            let api_key = self.llm_api_key.as_deref().unwrap_or("");
            cmd.env("ANTHROPIC_API_KEY", api_key);
        }

        let mut cmd_parts = vec!["claude".to_string()];
        if let Some(prompt) = &self.prompt {
            cmd_parts.push(prompt.clone());
        }
        for flag in &self.agent_flags {
            cmd_parts.push(flag.clone());
        }
        eprintln!("Running claude command: {}", cmd_parts.join(" "));

        // Execute the command
        let status = cmd.status().await?;

        if status.success() {
            eprintln!("Claude Code agent completed successfully");
            Ok(())
        } else {
            eprintln!("Claude Code agent exited with status: {}", status);
            anyhow::bail!("Claude Code agent exited with status: {}", status);
        }
    }

    /// Run the Gemini CLI agent
    async fn run_gemini_agent(&self) -> anyhow::Result<()> {
        use tokio::process::Command;

        eprintln!("Starting Gemini CLI agent...");

        // Determine the working directory
        let cwd = if let Some(cwd) = &self.cwd {
            cwd.clone()
        } else {
            std::env::current_dir()?
        };

        // Build the gemini command
        let mut cmd = Command::new("gemini");

        // Add the prompt as a flag if provided
        if let Some(prompt) = &self.prompt {
            cmd.arg("--prompt").arg(prompt);
        }

        // For now, Gemini CLI doesn't have a non-interactive mode like Codex
        // We just pass through any agent flags as arguments
        for flag in &self.agent_flags {
            cmd.arg(flag);
        }

        // Set working directory
        cmd.current_dir(cwd);

        // Set environment variables for Gemini CLI
        if let Ok(home) = std::env::var("HOME") {
            // Gemini CLI uses standard home directory
        }

        // Set custom LLM API if provided
        if let Some(llm_api) = &self.llm_api {
            // Gemini CLI typically uses Google AI API, so we set the base URL
            cmd.env("GOOGLE_AI_BASE_URL", llm_api);
            let api_key = self.llm_api_key.as_deref().unwrap_or("");
            cmd.env("GOOGLE_API_KEY", api_key);
        }

        let mut cmd_parts = vec!["gemini".to_string()];
        if let Some(prompt) = &self.prompt {
            cmd_parts.push("--prompt".to_string());
            cmd_parts.push(prompt.clone());
        }
        for flag in &self.agent_flags {
            cmd_parts.push(flag.clone());
        }
        eprintln!("Running gemini command: {}", cmd_parts.join(" "));

        // Execute the command
        let status = cmd.status().await?;

        if status.success() {
            eprintln!("Gemini CLI agent completed successfully");
            Ok(())
        } else {
            eprintln!("Gemini CLI agent exited with status: {}", status);
            anyhow::bail!("Gemini CLI agent exited with status: {}", status);
        }
    }

    /// Run the OpenCode agent
    async fn run_opencode_agent(&self) -> anyhow::Result<()> {
        use tokio::process::Command;

        eprintln!("Starting OpenCode agent...");

        // Determine the working directory
        let cwd = if let Some(cwd) = &self.cwd {
            cwd.clone()
        } else {
            std::env::current_dir()?
        };

        // Build the opencode command
        let mut cmd = Command::new("opencode");

        // Add the prompt as a flag if provided
        if let Some(prompt) = &self.prompt {
            cmd.arg("--prompt").arg(prompt);
        }

        // For now, OpenCode doesn't have a non-interactive mode like Codex
        // We just pass through any agent flags as arguments
        for flag in &self.agent_flags {
            cmd.arg(flag);
        }

        // Set working directory
        cmd.current_dir(cwd);

        // Set environment variables for OpenCode
        if let Ok(home) = std::env::var("HOME") {
            // OpenCode uses standard home directory
        }

        // Set custom LLM API if provided
        if let Some(llm_api) = &self.llm_api {
            // OpenCode may use various providers, set generic environment variables
            let api_key = self.llm_api_key.as_deref().unwrap_or("");
            cmd.env("OPENCODE_API_KEY", api_key);
            cmd.env("OPENCODE_API_BASE", llm_api);
        }

        let mut cmd_parts = vec!["opencode".to_string()];
        if let Some(prompt) = &self.prompt {
            cmd_parts.push("--prompt".to_string());
            cmd_parts.push(prompt.clone());
        }
        for flag in &self.agent_flags {
            cmd_parts.push(flag.clone());
        }
        eprintln!("Running opencode command: {}", cmd_parts.join(" "));

        // Execute the command
        let status = cmd.status().await?;

        if status.success() {
            eprintln!("OpenCode agent completed successfully");
            Ok(())
        } else {
            eprintln!("OpenCode agent exited with status: {}", status);
            anyhow::bail!("OpenCode agent exited with status: {}", status);
        }
    }

    /// Run the Qwen Code agent
    async fn run_qwen_agent(&self) -> anyhow::Result<()> {
        use tokio::process::Command;

        eprintln!("Starting Qwen Code agent...");

        // Determine the working directory
        let cwd = if let Some(cwd) = &self.cwd {
            cwd.clone()
        } else {
            std::env::current_dir()?
        };

        // Build the qwen command
        let mut cmd = Command::new("qwen");

        // Add the prompt as a flag if provided
        if let Some(prompt) = &self.prompt {
            cmd.arg("--prompt").arg(prompt);
        }

        // For now, Qwen Code doesn't have a non-interactive mode like Codex
        // We just pass through any agent flags as arguments
        for flag in &self.agent_flags {
            cmd.arg(flag);
        }

        // Set working directory
        cmd.current_dir(cwd);

        // Set environment variables for Qwen Code
        if let Ok(home) = std::env::var("HOME") {
            // Qwen Code uses standard home directory
        }

        // Set custom LLM API if provided
        if let Some(llm_api) = &self.llm_api {
            // Qwen Code may use various providers, set generic environment variables
            let api_key = self.llm_api_key.as_deref().unwrap_or("");
            cmd.env("QWEN_API_KEY", api_key);
            cmd.env("QWEN_API_BASE", llm_api);
        }

        let mut cmd_parts = vec!["qwen".to_string()];
        if let Some(prompt) = &self.prompt {
            cmd_parts.push("--prompt".to_string());
            cmd_parts.push(prompt.clone());
        }
        for flag in &self.agent_flags {
            cmd_parts.push(flag.clone());
        }
        eprintln!("Running qwen command: {}", cmd_parts.join(" "));

        // Execute the command
        let status = cmd.status().await?;

        if status.success() {
            eprintln!("Qwen Code agent completed successfully");
            Ok(())
        } else {
            eprintln!("Qwen Code agent exited with status: {}", status);
            anyhow::bail!("Qwen Code agent exited with status: {}", status);
        }
    }

    /// Run the Cursor CLI agent
    async fn run_cursor_cli_agent(&self) -> anyhow::Result<()> {
        use tokio::process::Command;

        eprintln!("Starting Cursor CLI agent...");

        // Determine the working directory
        let cwd = if let Some(cwd) = &self.cwd {
            cwd.clone()
        } else {
            std::env::current_dir()?
        };

        // Build the cursor-cli command
        let mut cmd = Command::new("cursor-cli");

        // Add the prompt as a flag if provided
        if let Some(prompt) = &self.prompt {
            cmd.arg("--prompt").arg(prompt);
        }

        // For now, Cursor CLI doesn't have a non-interactive mode like Codex
        // We just pass through any agent flags as arguments
        for flag in &self.agent_flags {
            cmd.arg(flag);
        }

        // Set working directory
        cmd.current_dir(cwd);

        // Set environment variables for Cursor CLI
        if let Ok(home) = std::env::var("HOME") {
            // Cursor CLI uses standard home directory
        }

        // Set custom LLM API if provided
        if let Some(llm_api) = &self.llm_api {
            // Cursor CLI may use various providers, set generic environment variables
            let api_key = self.llm_api_key.as_deref().unwrap_or("");
            cmd.env("CURSOR_API_KEY", api_key);
            cmd.env("CURSOR_API_BASE", llm_api);
        }

        let mut cmd_parts = vec!["cursor-cli".to_string()];
        if let Some(prompt) = &self.prompt {
            cmd_parts.push("--prompt".to_string());
            cmd_parts.push(prompt.clone());
        }
        for flag in &self.agent_flags {
            cmd_parts.push(flag.clone());
        }
        eprintln!("Running cursor-cli command: {}", cmd_parts.join(" "));

        // Execute the command
        let status = cmd.status().await?;

        if status.success() {
            eprintln!("Cursor CLI agent completed successfully");
            Ok(())
        } else {
            eprintln!("Cursor CLI agent exited with status: {}", status);
            anyhow::bail!("Cursor CLI agent exited with status: {}", status);
        }
    }

    /// Run the Goose agent
    async fn run_goose_agent(&self) -> anyhow::Result<()> {
        use tokio::process::Command;

        eprintln!("Starting Goose agent...");

        // Determine the working directory
        let cwd = if let Some(cwd) = &self.cwd {
            cwd.clone()
        } else {
            std::env::current_dir()?
        };

        // Build the goose command
        let mut cmd = Command::new("goose");

        // Add the run subcommand and prompt flag if provided
        if let Some(prompt) = &self.prompt {
            cmd.arg("run").arg("-t").arg(prompt);
        }

        // For now, Goose doesn't have a non-interactive mode like Codex
        // We just pass through any agent flags as arguments
        for flag in &self.agent_flags {
            cmd.arg(flag);
        }

        // Set working directory
        cmd.current_dir(cwd);

        // Set environment variables for Goose
        if let Ok(home) = std::env::var("HOME") {
            // Goose uses standard home directory
        }

        // Set custom LLM API if provided
        if let Some(llm_api) = &self.llm_api {
            // Goose uses GOOSE_* environment variables for provider configuration
            let api_key = self.llm_api_key.as_deref().unwrap_or("");
            cmd.env("GOOSE_API_KEY", api_key);
            cmd.env("GOOSE_API_BASE", llm_api);
        }

        let mut cmd_parts = vec!["goose".to_string()];
        if let Some(prompt) = &self.prompt {
            cmd_parts.push("run".to_string());
            cmd_parts.push("-t".to_string());
            cmd_parts.push(prompt.clone());
        }
        for flag in &self.agent_flags {
            cmd_parts.push(flag.clone());
        }
        eprintln!("Running goose command: {}", cmd_parts.join(" "));

        // Execute the command
        let status = cmd.status().await?;

        if status.success() {
            eprintln!("Goose agent completed successfully");
            Ok(())
        } else {
            eprintln!("Goose agent exited with status: {}", status);
            anyhow::bail!("Goose agent exited with status: {}", status);
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
