//! Agents Workflow CLI library

pub mod agent;
pub mod config;
pub mod sandbox;
pub mod task;
pub mod test_config;
pub mod transport;
pub mod tui;

// Re-export CLI types for testing
pub use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "ah")]
#[command(about = "Agents Workflow CLI")]
#[command(version, author, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Configuration management commands
    Config {
        #[command(subcommand)]
        subcommand: config::ConfigCommands,
    },
    /// Task management commands
    Task {
        #[command(subcommand)]
        subcommand: task::TaskCommands,
    },
    /// Agent-related commands
    Agent {
        #[command(subcommand)]
        subcommand: AgentCommands,
    },
    /// Launch the Terminal User Interface
    Tui(tui::TuiArgs),
}

#[derive(Subcommand)]
pub enum AgentCommands {
    /// AgentFS filesystem operations
    Fs {
        #[command(subcommand)]
        subcommand: agent::fs::AgentFsCommands,
    },
    /// Run a command in a local sandbox
    Sandbox(sandbox::SandboxRunArgs),
    /// Start an agent session
    Start(agent::start::AgentStartArgs),
    /// Record an agent session with PTY capture
    Record(agent::record::RecordArgs),
    /// Extract branch points from a recorded session
    BranchPoints(agent::record::BranchPointsArgs),
}
