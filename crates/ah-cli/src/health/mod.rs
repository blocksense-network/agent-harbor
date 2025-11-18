// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only
#![allow(clippy::disallowed_methods)] // Health command prints human-readable output by design

//! Health check commands
mod types;

use ah_mux::TmuxMultiplexer;
use ah_mux::detection::detect_terminal_environments;
use ah_mux_core::Multiplexer;
use clap::Args;
use std::collections::HashMap;
use tracing::{debug, info, warn};
use types::{AgentHealthStatus, HealthFormatter, HumanReadableFormatter, JsonFormatter};

/// Arguments for the health command
#[derive(Args, Debug)]
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

    /// Show credential paths and related info in output (does NOT expose actual tokens/secrets yet)
    #[arg(
        long,
        help = "Show credential paths and related info in output (does NOT expose actual tokens/secrets yet; planned for future)"
    )]
    with_credentials: bool,
}

impl HealthArgs {
    /// Run the health check command
    pub async fn run(self) -> anyhow::Result<()> {
        info!("Starting health check");

        // Collect terminal environment info
        let environments: Vec<String> = detect_terminal_environments()
            .iter()
            .map(|env| env.display_name().to_string())
            .collect();
        debug!(environments = ?environments, "Detected terminal environments");

        let multiplexers = self.get_multiplexer_status();
        debug!(multiplexers = ?multiplexers, "Checked multiplexer status");

        // Collect agent health status
        let agent_statuses = self.collect_agent_health().await;
        debug!(
            agent_count = agent_statuses.len(),
            "Collected agent health statuses"
        );

        // Check if any agent has authentication issues for exit code
        let has_auth_issues =
            agent_statuses.iter().any(|status| status.available && !status.authenticated);

        if has_auth_issues {
            warn!("Some agents are available but not authenticated");
        }

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
            info!("Exiting with code 1 due to authentication issues");
            std::process::exit(1);
        }

        info!("Health check completed successfully");
        Ok(())
    }

    /// Get terminal multiplexer status
    fn get_multiplexer_status(&self) -> HashMap<String, String> {
        debug!("Checking terminal multiplexer availability");
        let mut multiplexers = HashMap::new();

        // Check tmux
        let tmux_status = match TmuxMultiplexer::new() {
            Ok(multiplexer) => {
                if multiplexer.is_available() {
                    debug!("tmux is available");
                    "available"
                } else {
                    debug!("tmux is not available");
                    "not_available"
                }
            }
            Err(e) => {
                debug!(error = %e, "Failed to initialize tmux multiplexer");
                "failed_to_initialize"
            }
        };
        multiplexers.insert("tmux".to_string(), tmux_status.to_string());

        // Check zellij
        let zellij_status = if which::which("zellij").is_ok() {
            debug!("zellij is available");
            "available"
        } else {
            debug!("zellij is not available");
            "not_available"
        };
        multiplexers.insert("zellij".to_string(), zellij_status.to_string());

        // Check screen
        let screen_status = if which::which("screen").is_ok() {
            debug!("screen is available");
            "available"
        } else {
            debug!("screen is not available");
            "not_available"
        };
        multiplexers.insert("screen".to_string(), screen_status.to_string());

        debug!(multiplexers = ?multiplexers, "Multiplexer status check completed");
        multiplexers
    }

    /// Collect health status for all supported agents
    async fn collect_agent_health(&self) -> Vec<AgentHealthStatus> {
        let mut agent_statuses = Vec::new();

        // Determine which agents to check based on --supported-agents flag
        let agents_to_check = self.get_agents_to_check();
        debug!(agents_to_check = ?agents_to_check, "Collecting health status for agents");

        // Check each requested agent type
        for agent_type in agents_to_check {
            debug!(agent_type = %agent_type, "Checking agent health");
            match agent_type.as_str() {
                "cursor" => {
                    if let Some(status) = self.get_cursor_health_status().await {
                        debug!(
                            agent = "cursor",
                            available = status.available,
                            authenticated = status.authenticated,
                            "Cursor health check completed"
                        );
                        agent_statuses.push(status);
                    }
                }
                "codex" => {
                    if let Some(status) = self.get_codex_health_status().await {
                        debug!(
                            agent = "codex",
                            available = status.available,
                            authenticated = status.authenticated,
                            "Codex health check completed"
                        );
                        agent_statuses.push(status);
                    }
                }
                "claude" => {
                    if let Some(status) = self.get_claude_health_status().await {
                        debug!(
                            agent = "claude",
                            available = status.available,
                            authenticated = status.authenticated,
                            "Claude health check completed"
                        );
                        agent_statuses.push(status);
                    }
                }
                "copilot" => {
                    if let Some(status) = self.get_copilot_health_status().await {
                        debug!(
                            agent = "copilot",
                            available = status.available,
                            authenticated = status.authenticated,
                            "Copilot health check completed"
                        );
                        agent_statuses.push(status);
                    }
                }
                "gemini" => {
                    if let Some(status) = self.get_gemini_health_status().await {
                        debug!(
                            agent = "gemini",
                            available = status.available,
                            authenticated = status.authenticated,
                            "Gemini health check completed"
                        );
                        agent_statuses.push(status);
                    }
                }
                _ => {
                    // Unknown agent type - create a status indicating this
                    warn!(agent_type = %agent_type, "Unknown agent type requested");
                    agent_statuses.push(
                        AgentHealthStatus::new(format!("{} (unknown)", agent_type))
                            .with_error("Unknown agent type"),
                    );
                }
            }
        }

        info!(
            total_agents = agent_statuses.len(),
            "Agent health collection completed"
        );
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
        debug!("Starting Cursor CLI health check");
        let cursor_agent = ah_agents::cursor_cli();

        // Use structured status function; get_cursor_status has internal timeout of 1500ms
        let cursor_status = cursor_agent.get_cursor_status().await;

        let mut status = AgentHealthStatus::new("Cursor CLI")
            .with_availability(cursor_status.available, cursor_status.version)
            .with_auth(
                cursor_status.authenticated,
                cursor_status.auth_method,
                cursor_status.auth_source,
            );

        if let Some(error) = cursor_status.error {
            debug!(error = %error, "Cursor CLI health check encountered error");
            status = status.with_error(error);
        }

        if cursor_status.authenticated {
            debug!("Cursor CLI is authenticated");
            status = status.with_note("This is a session token, not necessarily a Cursor API key");
        } else if cursor_status.available {
            debug!("Cursor CLI is available but not authenticated");
            status = status.with_note("Not logged in (no access token found)");
        }

        Some(status)
    }

    /// Get Gemini CLI health status
    async fn get_gemini_health_status(&self) -> Option<AgentHealthStatus> {
        debug!("Starting Gemini CLI health check");
        let gemini_agent = ah_agents::gemini();

        // Use structured status function
        let gemini_status = gemini_agent.get_gemini_status().await;

        let mut status = AgentHealthStatus::new("Gemini CLI")
            .with_availability(gemini_status.available, gemini_status.version)
            .with_auth(
                gemini_status.authenticated,
                gemini_status.auth_method,
                gemini_status.auth_source,
            );

        if let Some(error) = gemini_status.error {
            debug!(error = %error, "Gemini CLI health check encountered error");
            status = status.with_error(error);
        }

        if gemini_status.authenticated {
            debug!("Gemini CLI is authenticated");
        } else if gemini_status.available {
            debug!("Gemini CLI is available but not authenticated");
        }

        Some(status)
    }

    /// Get Copilot CLI health status
    async fn get_copilot_health_status(&self) -> Option<AgentHealthStatus> {
        debug!("Starting Copilot CLI health check");
        let copilot_agent = ah_agents::copilot_cli();

        // Use structured status function
        let copilot_status = copilot_agent.get_copilot_status().await;

        let mut status = AgentHealthStatus::new("Copilot CLI")
            .with_availability(copilot_status.available, copilot_status.version)
            .with_auth(
                copilot_status.authenticated,
                copilot_status.auth_method,
                copilot_status.auth_source,
            );

        if let Some(error) = copilot_status.error {
            debug!(error = %error, "Copilot CLI health check encountered error");
            status = status.with_error(error);
        }

        if !copilot_status.authenticated && copilot_status.available {
            debug!("Copilot CLI is available but not authenticated");
            status = status.with_note("Try setting GH_TOKEN or GITHUB_TOKEN environment variable");
        } else if copilot_status.authenticated {
            debug!("Copilot CLI is authenticated");
        }

        Some(status)
    }

    /// Get Codex health status
    async fn get_codex_health_status(&self) -> Option<AgentHealthStatus> {
        debug!("Starting Codex CLI health check");
        let codex_agent = ah_agents::codex();

        // Use structured status function
        let codex_status = codex_agent.get_codex_status().await;

        let mut status = AgentHealthStatus::new("Codex CLI")
            .with_availability(codex_status.available, codex_status.version)
            .with_auth(
                codex_status.authenticated,
                codex_status.auth_method,
                codex_status.auth_source,
            );

        if let Some(error) = codex_status.error {
            debug!(error = %error, "Codex CLI health check encountered error");
            status = status.with_error(error);
        }

        if !codex_status.authenticated && codex_status.available {
            debug!("Codex CLI is available but not authenticated");
            status = status.with_note(
                "Try setting OPENAI_API_KEY environment variable or logging in with Codex auth",
            );
        } else if codex_status.authenticated {
            debug!("Codex CLI is authenticated");
        }

        Some(status)
    }

    /// Get Claude health status
    async fn get_claude_health_status(&self) -> Option<AgentHealthStatus> {
        debug!("Starting Claude CLI health check");
        let claude_agent = ah_agents::claude();

        // Use structured status function
        let claude_status = claude_agent.get_claude_status().await;

        let mut status = AgentHealthStatus::new("Claude CLI")
            .with_availability(claude_status.available, claude_status.version)
            .with_auth(
                claude_status.authenticated,
                claude_status.auth_method,
                claude_status.auth_source,
            );

        if let Some(error) = claude_status.error {
            debug!(error = %error, "Claude CLI health check encountered error");
            status = status.with_error(error);
        }

        if !claude_status.authenticated && claude_status.available {
            debug!("Claude CLI is available but not authenticated");
            status = status.with_note(
                "Try setting ANTHROPIC_API_KEY environment variable or logging in with Claude Code",
            );
        } else if claude_status.authenticated {
            debug!("Claude CLI is authenticated");
        }

        Some(status)
    }
}
