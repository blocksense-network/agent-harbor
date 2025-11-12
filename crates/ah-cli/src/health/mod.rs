// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Health check commands

mod types;

use ah_agents::traits::AgentExecutor;
use ah_mux::TmuxMultiplexer;
use ah_mux::detection::detect_terminal_environments;
use ah_mux_core::Multiplexer;
use clap::Args;
use std::collections::HashMap;
use types::{AgentHealthStatus, HealthFormatter, HumanReadableFormatter, JsonFormatter};

/// Arguments for the health command
#[derive(Args)]
#[command(about = "Perform diagnostic health checks")]
pub struct HealthArgs {
    /// Supported agent types to check (default: all)
    #[arg(
        long,
        value_delimiter = ',',
        help = "Supported agent types (default: all)"
    )]
    supported_agents: Option<Vec<String>>,

    /// Output in JSON format
    #[arg(long, help = "Output in JSON format")]
    json: bool,

    /// Suppress warnings, only show errors
    #[arg(long, help = "Suppress warnings, only show errors")]
    quiet: bool,

    /// Include sensitive credential information in output
    #[arg(
        long,
        help = "Include sensitive credential information in output (WARNING: exposes tokens/secrets)"
    )]
    with_credentials: bool,
}

impl HealthArgs {
    /// Run the health check command
    pub async fn run(self) -> anyhow::Result<()> {
        // Collect terminal environment info
        let environments: Vec<String> = detect_terminal_environments()
            .iter()
            .map(|env| env.display_name().to_string())
            .collect();

        let multiplexers = self.get_multiplexer_status();

        // Collect agent health status
        let agent_statuses = self.collect_agent_health().await;

        // Check if any agent has authentication issues for exit code
        let has_auth_issues =
            agent_statuses.iter().any(|status| status.available && !status.authenticated);

        if self.json {
            let formatter = JsonFormatter;
            let report = formatter.format_full_report(
                &environments,
                &multiplexers,
                &agent_statuses,
                self.with_credentials,
            );
            println!("{}", serde_json::to_string_pretty(&report)?);
        } else {
            println!("🔍 Agent Harbor Health Check");
            println!("==============================");
            println!();

            let formatter = HumanReadableFormatter;
            print!(
                "{}",
                formatter.format_terminal_info(&environments, &multiplexers)
            );
            print!(
                "{}",
                formatter.format_agents_summary(&agent_statuses, self.with_credentials)
            );
        }

        // Exit with non-zero code if any requested agent tool is present but unauthenticated
        // unless --quiet is set (which permits soft warnings according to spec)
        if has_auth_issues && !self.quiet {
            std::process::exit(1);
        }

        Ok(())
    }

    /// Get terminal multiplexer status
    fn get_multiplexer_status(&self) -> HashMap<String, String> {
        let mut multiplexers = HashMap::new();

        // Check tmux
        let tmux_status = match TmuxMultiplexer::new() {
            Ok(multiplexer) => {
                if multiplexer.is_available() {
                    "available"
                } else {
                    "not_available"
                }
            }
            Err(_) => "failed_to_initialize",
        };
        multiplexers.insert("tmux".to_string(), tmux_status.to_string());

        // Check zellij
        let zellij_status = if which::which("zellij").is_ok() {
            "available"
        } else {
            "not_available"
        };
        multiplexers.insert("zellij".to_string(), zellij_status.to_string());

        // Check screen
        let screen_status = if which::which("screen").is_ok() {
            "available"
        } else {
            "not_available"
        };
        multiplexers.insert("screen".to_string(), screen_status.to_string());

        multiplexers
    }

    /// Collect health status for all supported agents
    async fn collect_agent_health(&self) -> Vec<AgentHealthStatus> {
        let mut agent_statuses = Vec::new();

        // Determine which agents to check based on --supported-agents flag
        let agents_to_check = self.get_agents_to_check();

        // Check each requested agent type
        for agent_type in agents_to_check {
            match agent_type.as_str() {
                "cursor" => {
                    if let Some(status) = self.get_cursor_health_status().await {
                        agent_statuses.push(status);
                    }
                }
                "codex" => {
                    if let Some(status) = self.get_codex_health_status().await {
                        agent_statuses.push(status);
                    }
                }
                "claude" => {
                    if let Some(status) = self.get_claude_health_status().await {
                        agent_statuses.push(status);
                    }
                }
                "copilot" => {
                    if let Some(status) = self.get_copilot_health_status().await {
                        agent_statuses.push(status);
                    }
                }
                "gemini" => {
                    if let Some(status) = self.get_gemini_health_status().await {
                        agent_statuses.push(status);
                    }
                }
                _ => {
                    // Unknown agent type - create a status indicating this
                    agent_statuses.push(
                        AgentHealthStatus::new(format!("{} (unknown)", agent_type))
                            .with_error("Unknown agent type"),
                    );
                }
            }
        }

        agent_statuses
    }

    /// Determine which agents to check based on configuration and flags
    fn get_agents_to_check(&self) -> Vec<String> {
        if let Some(ref supported_agents) = self.supported_agents {
            // If specific agents are requested, check those
            supported_agents.clone()
        } else {
            // Default: check all known agents
            vec![
                "cursor".to_string(),
                "codex".to_string(),
                "claude".to_string(),
                "copilot".to_string(),
                "gemini".to_string(),
            ]
        }
    }

    /// Get Cursor CLI health status
    async fn get_cursor_health_status(&self) -> Option<AgentHealthStatus> {
        let mut status = AgentHealthStatus::new("Cursor CLI");

        // Check if cursor-agent is available
        let cursor_available =
            std::process::Command::new("cursor-agent").arg("--version").output().is_ok();

        if !cursor_available {
            return Some(status.with_error("cursor-agent not found in PATH"));
        }

        status = status.with_availability(true, None);

        // Check for database and extract token
        match self.check_cursor_login_status() {
            Ok(Some(token)) => {
                status =
                    status.with_auth(true, Some("Session Token".to_string()), Some(token.clone()));
                status =
                    status.with_note("This is a session token, not necessarily a Cursor API key");
            }
            Ok(None) => {
                status = status.with_auth(false, None, None);
                status = status.with_note("Not logged in (no access token found)");
            }
            Err(e) => {
                status = status.with_error(format!("Failed to check login status: {}", e));
            }
        }

        Some(status)
    }

    /// Get Gemini CLI health status
    async fn get_gemini_health_status(&self) -> Option<AgentHealthStatus> {
        let gemini_agent = ah_agents::gemini();

        // Use structured status function with timeout for consistency
        let status_result = tokio::time::timeout(
            std::time::Duration::from_millis(1000),
            gemini_agent.get_gemini_status(),
        )
        .await;

        match status_result {
            Ok(gemini_status) => {
                let mut status = AgentHealthStatus::new("Gemini CLI")
                    .with_availability(gemini_status.available, gemini_status.version)
                    .with_auth(
                        gemini_status.authenticated,
                        gemini_status.auth_method,
                        gemini_status.auth_source,
                    );

                if let Some(error) = gemini_status.error {
                    status = status.with_error(error);
                }

                Some(status)
            }
            Err(_) => Some(AgentHealthStatus::new("Gemini CLI").with_timeout()),
        }
    }

    /// Get Copilot CLI health status
    async fn get_copilot_health_status(&self) -> Option<AgentHealthStatus> {
        let copilot_agent = ah_agents::copilot_cli();

        // Use structured status function with timeout for consistency
        let status_result = tokio::time::timeout(
            std::time::Duration::from_millis(1000),
            copilot_agent.get_copilot_status(),
        )
        .await;

        match status_result {
            Ok(copilot_status) => {
                let mut status = AgentHealthStatus::new("Copilot CLI")
                    .with_availability(copilot_status.available, copilot_status.version)
                    .with_auth(
                        copilot_status.authenticated,
                        copilot_status.auth_method,
                        copilot_status.auth_source,
                    );

                if let Some(error) = copilot_status.error {
                    status = status.with_error(error);
                }

                if !copilot_status.authenticated && copilot_status.available {
                    status = status
                        .with_note("Try setting GH_TOKEN or GITHUB_TOKEN environment variable");
                }

                Some(status)
            }
            Err(_) => Some(AgentHealthStatus::new("Copilot CLI").with_timeout()),
        }
    }

    /// Get Codex health status
    async fn get_codex_health_status(&self) -> Option<AgentHealthStatus> {
        let codex_agent = ah_agents::codex();

        // Use structured status function with timeout for consistency
        let status_result = tokio::time::timeout(
            std::time::Duration::from_millis(1000),
            codex_agent.get_codex_status(),
        )
        .await;

        match status_result {
            Ok(codex_status) => {
                let mut status = AgentHealthStatus::new("Codex CLI")
                    .with_availability(codex_status.available, codex_status.version)
                    .with_auth(
                        codex_status.authenticated,
                        codex_status.auth_method,
                        codex_status.auth_source,
                    );

                if let Some(error) = codex_status.error {
                    status = status.with_error(error);
                }

                if !codex_status.authenticated && codex_status.available {
                    status = status
                        .with_note("Try setting OPENAI_API_KEY environment variable or logging in with Codex auth");
                }

                Some(status)
            }
            Err(_) => Some(AgentHealthStatus::new("Codex CLI").with_timeout()),
        }
    }

    /// Get Claude health status
    async fn get_claude_health_status(&self) -> Option<AgentHealthStatus> {
        let claude_agent = ah_agents::claude();

        // Use structured status function with timeout for consistency
        let status_result = tokio::time::timeout(
            std::time::Duration::from_millis(1000),
            claude_agent.get_claude_status(),
        )
        .await;

        match status_result {
            Ok(claude_status) => {
                let mut status = AgentHealthStatus::new("Claude CLI")
                    .with_availability(claude_status.available, claude_status.version)
                    .with_auth(
                        claude_status.authenticated,
                        claude_status.auth_method,
                        claude_status.auth_source,
                    );

                if let Some(error) = claude_status.error {
                    status = status.with_error(error);
                }

                if !claude_status.authenticated && claude_status.available {
                    status = status
                        .with_note("Try setting ANTHROPIC_API_KEY environment variable or logging in with Claude Code");
                }

                Some(status)
            }
            Err(_) => Some(AgentHealthStatus::new("Claude CLI").with_timeout()),
        }
    }

    /// Check Cursor CLI login status and extract access token
    fn check_cursor_login_status(&self) -> anyhow::Result<Option<String>> {
        // Use the cursor agent to check login status
        let cursor_agent = ah_agents::cursor_cli();
        match cursor_agent.check_cursor_login_status() {
            Ok(result) => Ok(result),
            Err(e) => Err(anyhow::anyhow!(
                "Failed to check cursor login status: {}",
                e
            )),
        }
    }
}
