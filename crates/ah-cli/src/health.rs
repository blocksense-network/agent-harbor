// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Health check commands

use ah_mux::detection::{detect_terminal_environments, TerminalEnvironment};
use ah_mux_core::Multiplexer;
use ah_mux::TmuxMultiplexer;
use clap::Args;
use std::collections::HashMap;

/// Arguments for the health command
#[derive(Args)]
#[command(about = "Perform diagnostic health checks")]
pub struct HealthArgs {
    /// Supported agent types to check (default: all)
    #[arg(long, help = "Supported agent types (default: all)")]
    supported_agents: Option<String>,

    /// Output in JSON format
    #[arg(long, help = "Output in JSON format")]
    json: bool,

    /// Suppress warnings, only show errors
    #[arg(long, help = "Suppress warnings, only show errors")]
    quiet: bool,
}

impl HealthArgs {
    /// Run the health check command
    pub async fn run(self) -> anyhow::Result<()> {
        if self.json {
            self.run_json().await
        } else {
            self.run_human_readable().await
        }
    }

    /// Run health check with human-readable output
    async fn run_human_readable(&self) -> anyhow::Result<()> {
        println!("🔍 Agent Harbor Health Check");
        println!("==============================");
        println!();

        // Terminal environment detection
        self.print_terminal_environment()?;

        // TODO: Implement agent health checks
        println!("⚠️  Agent health checks not yet implemented");
        println!();

        Ok(())
    }

    /// Run health check with JSON output
    async fn run_json(&self) -> anyhow::Result<()> {
        let health_report = serde_json::json!({
            "terminal_environment": self.get_terminal_environment_json()?,
            "agents": {
                "status": "not_implemented",
                "message": "Agent health checks not yet implemented"
            }
        });

        println!("{}", serde_json::to_string_pretty(&health_report)?);
        Ok(())
    }

    /// Print terminal environment information
    fn print_terminal_environment(&self) -> anyhow::Result<()> {
        println!("📺 Terminal Environment");
        println!("{:-<40}", "");

        let environments = detect_terminal_environments();

        if environments.is_empty() {
            println!("❌ No terminal environment detected");
            return Ok(());
        }

        println!("✅ Detected environments (outermost to innermost):");
        for (i, env) in environments.iter().enumerate() {
            let indent = "  ".repeat(i);
            println!("{}{}", indent, env.display_name());
        }
        println!();

        // Multiplexer availability
        println!("🔧 Terminal Multiplexer Availability");
        println!("{:-<40}", "");

        let multiplexers = vec![
            ("tmux", TmuxMultiplexer::new()),
        ];

        for (name, multiplexer_result) in multiplexers {
            match multiplexer_result {
                Ok(multiplexer) => {
                    if multiplexer.is_available() {
                        println!("✅ {}: Available", name);
                    } else {
                        println!("❌ {}: Not available", name);
                    }
                }
                Err(_) => {
                    println!("❌ {}: Failed to initialize", name);
                }
            }
        }

        Ok(())
    }

    /// Get terminal environment information as JSON
    fn get_terminal_environment_json(&self) -> anyhow::Result<serde_json::Value> {
        let environments: Vec<String> = detect_terminal_environments()
            .iter()
            .map(|env| env.display_name().to_string())
            .collect();

        let mut multiplexers = HashMap::new();

        let multiplexer_checks = vec![
            ("tmux", TmuxMultiplexer::new()),
        ];

        for (name, multiplexer_result) in multiplexer_checks {
            let status = match multiplexer_result {
                Ok(multiplexer) => {
                    if multiplexer.is_available() {
                        "available"
                    } else {
                        "not_available"
                    }
                }
                Err(_) => "failed_to_initialize",
            };
            multiplexers.insert(name.to_string(), status);
        }

        Ok(serde_json::json!({
            "detected_environments": environments,
            "multiplexer_availability": multiplexers
        }))
    }
}

