// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use ah_cli::{AgentCommands, Cli, Commands, Parser};
use ah_domain_types::LogLevel;
use ah_logging::{Level, LogFormat, init_to_standard_file};
use ah_tui::view::TuiDependencies;
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Set up centralized logging to file with platform-specific path
    let default_level = match cli.log_level {
        LogLevel::Error => Level::ERROR,
        LogLevel::Warn => Level::WARN,
        LogLevel::Info => Level::INFO,
        LogLevel::Debug => Level::DEBUG,
        LogLevel::Trace => Level::TRACE,
    };
    init_to_standard_file("ah-cli", default_level, LogFormat::Plaintext)?;

    // Helper function to get TUI dependencies for record/replay commands
    fn get_record_tui_dependencies(cli: &Cli) -> Result<TuiDependencies> {
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
                let deps = get_record_tui_dependencies(&cli)?;
                ah_tui::record::execute(deps, args.clone()).await
            }
            AgentCommands::Replay(args) => ah_tui::replay::execute(args.clone()).await,
            AgentCommands::BranchPoints(args) => {
                ah_tui::record::execute_branch_points(args.clone()).await
            }
        },
        Commands::Tui(args) => args.run(cli.fs_snapshots, cli.experimental_features.clone()).await,
        Commands::Health(args) => args.run().await,
    }
}
