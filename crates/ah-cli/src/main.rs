use anyhow::Result;
use ah_cli::{AgentCommands, Cli, Commands, Parser};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Task { subcommand } => subcommand.run().await,
        Commands::Agent { subcommand } => match subcommand {
            AgentCommands::Fs { subcommand: cmd } => cmd.run().await,
            AgentCommands::Sandbox(args) => args.run().await,
            AgentCommands::Start(args) => args.run().await,
        },
        Commands::Tui(args) => args.run().await,
    }
}
