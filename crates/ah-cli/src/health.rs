// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Health check commands

use ah_agents::traits::AgentExecutor;
use ah_mux::TmuxMultiplexer;
use ah_mux::detection::detect_terminal_environments;
use ah_mux_core::Multiplexer;
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

    /// Include sensitive credential information in output
    #[arg(
        long,
        help = "Include sensitive credential information in output (WARNING: exposes tokens/secrets)"
    )]
    with_credentials: bool,
}

impl HealthArgs {
    /// Run the health check command
    #[allow(clippy::disallowed_methods)]
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

        // Agent health checks
        self.print_agent_health().await?;
        println!();

        Ok(())
    }

    /// Run health check with JSON output
    async fn run_json(&self) -> anyhow::Result<()> {
        let health_report = serde_json::json!({
            "terminal_environment": self.get_terminal_environment_json()?,
            "agents": self.get_agent_health_json().await?
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

        let multiplexers = vec![("tmux", TmuxMultiplexer::new())];

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

        let multiplexer_checks = vec![("tmux", TmuxMultiplexer::new())];

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

    /// Print agent health information
    async fn print_agent_health(&self) -> anyhow::Result<()> {
        println!("🤖 Agent Health");
        println!("{:-<40}", "");

        // Check Cursor CLI status
        self.print_cursor_health().await?;

        // Check Gemini CLI status
        self.print_gemini_health().await?;

        // Check Copilot CLI status
        self.print_copilot_health().await?;

        Ok(())
    }

    /// Print Cursor CLI health information
    async fn print_cursor_health(&self) -> anyhow::Result<()> {
        println!("Cursor CLI:");

        // Check if cursor-agent is available
        let cursor_available =
            std::process::Command::new("cursor-agent").arg("--version").output().is_ok();

        if !cursor_available {
            println!("  ❌ cursor-agent not found in PATH");
            return Ok(());
        }

        println!("  ✅ cursor-agent available");

        // Check for database and extract token
        match self.check_cursor_login_status() {
            Ok(Some(token)) => {
                if self.with_credentials {
                    println!("  ✅ Logged in (session token: {})", token);
                    println!(
                        "  ⚠️  Note: This is a session token, not necessarily a Cursor API key"
                    );
                } else {
                    println!(
                        "  ✅ Logged in (session token present, use --with-credentials to display)"
                    );
                    println!(
                        "  ⚠️  Note: Session tokens may not work with Cursor CLI --api-key flag"
                    );
                }
            }
            Ok(None) => {
                println!("  ⚠️  Not logged in (no access token found)");
            }
            Err(e) => {
                println!("  ❌ Failed to check login status: {}", e);
            }
        }

        Ok(())
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

    /// Print Gemini CLI health information
    async fn print_gemini_health(&self) -> anyhow::Result<()> {
        println!("Gemini CLI:");

        let gemini_agent = ah_agents::gemini();

        // Use the structured status function with timeout for consistency
        let status_result = tokio::time::timeout(
            std::time::Duration::from_millis(1000),
            gemini_agent.get_gemini_status(),
        )
        .await;

        match status_result {
            Ok(status) => {
                if !status.available {
                    if let Some(error) = &status.error {
                        println!("  ❌ {}", error);
                    } else {
                        println!("  ❌ gemini not available");
                    }
                    return Ok(());
                }

                // Display availability and version
                if let Some(version) = &status.version {
                    println!("  ✅ gemini available (v{})", version);
                } else {
                    println!("  ✅ gemini available");
                }

                // Display authentication status
                if status.authenticated {
                    if let Some(auth_method) = &status.auth_method {
                        println!("  ✅ Authenticated via {}", auth_method);
                    } else {
                        println!("  ✅ Authenticated");
                    }

                    if let Some(auth_source) = &status.auth_source {
                        if self.with_credentials {
                            println!("  ✅ Authentication source: {}", auth_source);
                        } else {
                            println!(
                                "  ✅ Authentication source found (use --with-credentials to display details)"
                            );
                        }
                    }
                } else {
                    println!("  ⚠️  Not authenticated");
                    if let Some(error) = &status.error {
                        if error.contains("Authentication") {
                            println!("  ❌ Authentication error: {}", error);
                        }
                    }
                }
            }
            Err(_) => {
                println!("  ❌ gemini status check timed out");
            }
        }

        Ok(())
    }

    /// Print Copilot CLI health information
    async fn print_copilot_health(&self) -> anyhow::Result<()> {
        println!("Copilot CLI:");

        let copilot_agent = ah_agents::copilot_cli();

        // Use the structured status function with timeout for consistency
        let status_result = tokio::time::timeout(
            std::time::Duration::from_millis(1000),
            copilot_agent.get_copilot_status(),
        )
        .await;

        match status_result {
            Ok(status) => {
                if !status.available {
                    if let Some(error) = &status.error {
                        println!("  ❌ {}", error);
                    } else {
                        println!("  ❌ copilot not available");
                    }
                    return Ok(());
                }

                // Display availability and version
                if let Some(version) = &status.version {
                    println!("  ✅ copilot available (v{})", version);
                } else {
                    println!("  ✅ copilot available");
                }

                // Display authentication status
                if status.authenticated {
                    if let Some(auth_method) = &status.auth_method {
                        println!("  ✅ Authenticated via {}", auth_method);
                    } else {
                        println!("  ✅ Authenticated");
                    }

                    if let Some(auth_source) = &status.auth_source {
                        if self.with_credentials {
                            println!("  ✅ Authentication source: {}", auth_source);
                        } else {
                            println!(
                                "  ✅ Authentication source found (use --with-credentials to display details)"
                            );
                        }
                    }
                } else {
                    println!("  ⚠️  Not authenticated (no API key found)");
                    println!("  💡 Try setting GH_TOKEN or GITHUB_TOKEN environment variable");
                }

                // Display any authentication errors
                if let Some(error) = &status.error {
                    if status.authenticated {
                        println!("  ⚠️  Note: {}", error);
                    } else {
                        println!("  ❌ Authentication error: {}", error);
                    }
                }
            }
            Err(_) => {
                println!("  ❌ copilot status check timed out");
            }
        }

        Ok(())
    }

    /// Get agent health information as JSON
    async fn get_agent_health_json(&self) -> anyhow::Result<serde_json::Value> {
        let cursor_status = self.get_cursor_health_json().await?;
        let gemini_status = self.get_gemini_health_json().await?;
        let copilot_status = self.get_copilot_health_json().await?;

        Ok(serde_json::json!({
            "cursor_cli": cursor_status,
            "gemini_cli": gemini_status,
            "copilot_cli": copilot_status
        }))
    }

    /// Get Cursor CLI health information as JSON
    async fn get_cursor_health_json(&self) -> anyhow::Result<serde_json::Value> {
        // Check if cursor-agent is available
        let cursor_available =
            std::process::Command::new("cursor-agent").arg("--version").output().is_ok();

        let mut cursor_info = serde_json::json!({
            "available": cursor_available
        });

        if cursor_available {
            match self.check_cursor_login_status() {
                Ok(Some(token)) => {
                    cursor_info["logged_in"] = serde_json::Value::Bool(true);
                    cursor_info["session_token_length"] =
                        serde_json::Value::Number(token.len().into());
                    cursor_info["note"] = serde_json::Value::String("This is a session token extracted from Cursor's local database. It may not work with Cursor CLI's --api-key flag for API key authentication.".to_string());
                    if self.with_credentials {
                        cursor_info["session_token"] = serde_json::Value::String(token);
                    }
                }
                Ok(None) => {
                    cursor_info["logged_in"] = serde_json::Value::Bool(false);
                }
                Err(e) => {
                    cursor_info["error"] = serde_json::Value::String(e.to_string());
                }
            }
        }

        Ok(cursor_info)
    }

    /// Get Gemini CLI health information as JSON
    async fn get_gemini_health_json(&self) -> anyhow::Result<serde_json::Value> {
        let gemini_agent = ah_agents::gemini();

        // Use the structured status function to get comprehensive information
        let status = match tokio::time::timeout(
            std::time::Duration::from_millis(1000),
            gemini_agent.get_gemini_status(),
        )
        .await
        {
            Ok(status) => status,
            Err(_) => {
                // Return timeout error status
                return Ok(serde_json::json!({
                    "available": false,
                    "authenticated": false,
                    "timeout_error": "Status check timed out"
                }));
            }
        };

        let mut gemini_info = serde_json::json!({
            "available": status.available,
            "authenticated": status.authenticated
        });

        if let Some(version) = &status.version {
            gemini_info["version"] = serde_json::Value::String(version.clone());
        }

        if let Some(auth_method) = &status.auth_method {
            gemini_info["auth_method"] = serde_json::Value::String(auth_method.clone());
        }

        if let Some(auth_source) = &status.auth_source {
            if self.with_credentials {
                gemini_info["auth_source"] = serde_json::Value::String(auth_source.clone());
            } else {
                gemini_info["auth_source_available"] = serde_json::Value::Bool(true);
            }
        }

        if let Some(error) = &status.error {
            if status.available {
                gemini_info["auth_error"] = serde_json::Value::String(error.clone());
            } else {
                gemini_info["version_error"] = serde_json::Value::String(error.clone());
            }
        }

        Ok(gemini_info)
    }

    /// Get Copilot CLI health information as JSON
    async fn get_copilot_health_json(&self) -> anyhow::Result<serde_json::Value> {
        let copilot_agent = ah_agents::copilot_cli();

        // Use the structured status function to get comprehensive information
        let status = copilot_agent.get_copilot_status().await;

        let mut copilot_info = serde_json::json!({
            "available": status.available,
            "authenticated": status.authenticated
        });

        if let Some(version) = &status.version {
            copilot_info["version"] = serde_json::Value::String(version.clone());
        }

        if let Some(auth_method) = &status.auth_method {
            copilot_info["auth_method"] = serde_json::Value::String(auth_method.clone());
        }

        if let Some(auth_source) = &status.auth_source {
            if self.with_credentials {
                copilot_info["auth_source"] = serde_json::Value::String(auth_source.clone());
            } else {
                copilot_info["auth_source_available"] = serde_json::Value::Bool(true);
            }
        }

        if let Some(error) = &status.error {
            if status.available {
                copilot_info["auth_error"] = serde_json::Value::String(error.clone());
            } else {
                copilot_info["version_error"] = serde_json::Value::String(error.clone());
            }
        }

        Ok(copilot_info)
    }
}
