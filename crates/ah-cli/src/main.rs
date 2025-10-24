// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use ah_cli::{health, AgentCommands, Cli, Commands, Parser};
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing - controlled by RUST_LOG environment variable
    // Try to initialize tracing, but don't fail if it doesn't work (e.g., in test environments)
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Config { subcommand } => subcommand.run(cli.config.as_deref()).await,
        Commands::Task { subcommand } => subcommand.run().await,
        Commands::Agent { subcommand } => match subcommand {
            AgentCommands::Fs { subcommand: cmd } => cmd.run().await,
            AgentCommands::Sandbox(args) => args.run().await,
            AgentCommands::Start(args) => args.run().await,
            AgentCommands::Record(args) => ah_cli::agent::record::execute(args).await,
            AgentCommands::Replay(args) => ah_cli::agent::replay::execute(args).await,
            AgentCommands::BranchPoints(args) => {
                ah_cli::agent::record::execute_branch_points(args).await
            }
        },
        Commands::Tui(args) => args.run().await,
        Commands::Health(args) => args.run().await,
    }
}
