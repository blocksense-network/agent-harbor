// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use ah_cli::{AgentCommands, Cli, Commands, config};
use ah_logging::CliLogLevel;
use ah_tui::view::TuiDependencies;
use anyhow::Result;
use clap::{CommandFactory, FromArgMatches};

#[tokio::main]
async fn main() -> Result<()> {
    let command = Cli::command();
    let matches = command.clone().get_matches();
    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());

    // Load configuration from all sources (system, user, repo, repo-user, env, CLI)
    // Convert CLI arguments to JSON overrides following TOML naming conventions
    let cli_overrides = ::serde_json::to_value(&cli)?;
    let config_result = config::load_config(
        cli.repo.as_deref(),
        cli.config.as_deref(),
        Some(&cli_overrides),
    )?;

    // Determine if this is a TUI command that should log to file
    let is_tui_command = matches!(
        &cli.command,
        Commands::Tui(_)
            | Commands::Agent {
                subcommand: AgentCommands::Record(_)
                    | AgentCommands::Start(_)
                    | AgentCommands::Replay(_)
                    | AgentCommands::BranchPoints(_)
            }
    );

    // Set up centralized logging using the merged configuration
    let default_level = if cfg!(debug_assertions) {
        CliLogLevel::Debug
    } else {
        CliLogLevel::Info
    };
    config_result.config.logging().to_cli_logging_args().init_with_default_level(
        "agent-harbor",
        is_tui_command,
        default_level,
    )?;

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
        Commands::Task { subcommand } => {
            subcommand
                .run(
                    cli.config.as_deref(),
                    cli.repo.clone(),
                    cli.fs_snapshots.clone(),
                )
                .await
        }
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
