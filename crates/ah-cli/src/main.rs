// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use ah_cli::{AgentCommands, Cli, Commands, Parser, health};
use ah_tui::view::TuiDependencies;
use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Set up centralized logging to file
    setup_logging(&cli.log_level)?;

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
        )
    }

    match cli.command {
        Commands::Config { subcommand } => subcommand.run(cli.config.as_deref()).await,
        Commands::Task { subcommand } => subcommand.run().await,
        Commands::Agent { ref subcommand } => match subcommand {
            AgentCommands::Fs {
                subcommand: ref cmd,
            } => (*cmd).clone().run().await,
            AgentCommands::Sandbox(ref args) => (*args).clone().run().await,
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
        Commands::Tui(args) => args.run(cli.fs_snapshots).await,
        Commands::Health(args) => args.run().await,
    }
}

/// Set up centralized logging to a file
fn setup_logging(log_level: &str) -> Result<()> {
    // Determine log file path based on OS
    let log_path = get_log_file_path();

    // Create parent directory if it doesn't exist
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create log directory: {}", parent.display()))?;
    }

    // Create or open the log file
    let log_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("Failed to open log file: {}", log_path.display()))?;

    // Set up the tracing subscriber to write to the file
    let filter = format!("{},ah_cli=debug,ah_recorder=debug,ah_tui=debug", log_level);
    tracing_subscriber::fmt()
        .with_writer(log_file)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&filter)),
        )
        .try_init()
        .map_err(|e| anyhow::anyhow!("Failed to initialize logging: {}", e))?;

    Ok(())
}

/// Get the standard log file path for the current OS
fn get_log_file_path() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        // Windows: %APPDATA%\agent-harbor\agent-harbor.log
        let mut path = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("C:\\Users\\Default\\AppData\\Roaming"));
        path.push("agent-harbor");
        path.push("agent-harbor.log");
        path
    }

    #[cfg(target_os = "macos")]
    {
        // macOS: ~/Library/Logs/agent-harbor.log
        let mut path = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        path.push("Library");
        path.push("Logs");
        path.push("agent-harbor.log");
        path
    }

    #[cfg(target_os = "linux")]
    {
        // Linux: ~/.local/share/agent-harbor/agent-harbor.log
        let mut path = dirs::data_dir()
            .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp")));
        path.push("agent-harbor");
        path.push("agent-harbor.log");
        path
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        // Fallback for other OSes
        let mut path = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        path.push("agent-harbor.log");
        path
    }
}
