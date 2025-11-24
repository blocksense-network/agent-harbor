// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use ah_cli::{AgentCommands, Cli, Commands, Parser, config};
use ah_domain_types::LogLevel;
use ah_logging::{Level, LogFormat, init_to_standard_file};
use ah_tui::view::TuiDependencies;
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Load configuration from all sources (system, user, repo, repo-user, env, CLI)
    // Convert CLI arguments to JSON overrides following TOML naming conventions
    let cli_overrides = ::serde_json::to_value(&cli)?;
    let config_result = config::load_config(
        cli.repo.as_deref(),
        cli.config.as_deref(),
        Some(&cli_overrides),
    )?;

    // Use configuration for application behavior
    tracing::debug!("Configuration loaded successfully");

    // Set up centralized logging to file with platform-specific path
    let log_level = cli.log_level.as_ref().unwrap_or({
        // Default log level (same as clap default)
        if cfg!(debug_assertions) {
            &LogLevel::Debug
        } else {
            &LogLevel::Info
        }
    });
    let default_level = match *log_level {
        LogLevel::Error => Level::ERROR,
        LogLevel::Warn => Level::WARN,
        LogLevel::Info => Level::INFO,
        LogLevel::Debug => Level::DEBUG,
        LogLevel::Trace => Level::TRACE,
    };
    init_to_standard_file("ah-cli", default_level, LogFormat::Plaintext)?;

    // Helper function to get TUI dependencies for record/replay commands
    fn get_record_tui_dependencies(
        cli: &Cli,
        config: &ah_cli::config::Config,
    ) -> Result<TuiDependencies> {
        // For now, we'll use a simplified setup for record/replay
        // This could be expanded to support remote connections if needed
        use ah_cli::tui::TuiArgs;
        TuiArgs::get_tui_dependencies(
            cli.repo.clone(),
            None,
            None,
            None,
            None,
            cli.fs_snapshots.clone(),
            cli.experimental_features.clone(),
            config.tui().clone(),
        )
    }

    match cli.command {
        Commands::Config { subcommand } => subcommand.run(cli.config.as_deref()).await,
        Commands::Task { subcommand } => subcommand.run().await,
        Commands::Agent { ref subcommand } => match subcommand {
            AgentCommands::Fs {
                subcommand: ref cmd,
            } => (*cmd).clone().run().await,
            AgentCommands::Sandbox(ref args) => (*args).clone().run(cli.fs_snapshots).await,
            AgentCommands::Start(ref args) => (*args).clone().run().await,
            AgentCommands::Record(args) => {
                let deps = get_record_tui_dependencies(&cli, &config_result.config)?;
                ah_tui::record::execute(deps, args.clone()).await
            }
            AgentCommands::Replay(args) => ah_tui::replay::execute(args.clone()).await,
            AgentCommands::BranchPoints(args) => {
                ah_tui::record::execute_branch_points(args.clone()).await
            }
        },
        Commands::Tui(args) => {
            args.run(
                cli.fs_snapshots,
                cli.experimental_features.clone(),
                &config_result.config,
            )
            .await
        }
        Commands::Health(args) => args.run().await,
    }
}
