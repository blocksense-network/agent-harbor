use ah_cli::{AgentCommands, Cli, Commands, Parser};
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Config { subcommand } => subcommand.run().await,
        Commands::Task { subcommand } => subcommand.run().await,
        Commands::Agent { subcommand } => match subcommand {
            AgentCommands::Fs { subcommand: cmd } => cmd.run().await,
            AgentCommands::Sandbox(args) => args.run().await,
            AgentCommands::Start(args) => args.run().await,
            AgentCommands::Record(args) => ah_cli::agent::record::execute(args).await,
        },
        Commands::Tui(args) => args.run().await,
    }
}
