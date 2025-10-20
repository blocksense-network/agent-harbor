// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Agent Harbor CLI library

pub mod agent;
pub mod config;
pub mod health;
pub mod sandbox;
pub mod task;
pub mod test_config;
pub mod transport;
pub mod tui;

// Re-export CLI types for testing
pub use clap::{Parser, Subcommand};

// Re-export agent types for backward compatibility
pub use agent::start::CliAgentType as AgentType;

#[derive(Parser)]
#[command(name = "ah")]
#[command(about = "Agent Harbor CLI")]
#[command(version, author, long_about = None)]
pub struct Cli {
    /// Additional configuration file to load (sits just below CLI flags in precedence order)
    #[arg(long, help = "Additional configuration file to load")]
    pub config: Option<String>,

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
    /// Health check commands
    Health(health::HealthArgs),
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
    /// Replay a recorded agent session
    Replay(agent::replay::ReplayArgs),
    /// Extract branch points from a recorded session
    BranchPoints(agent::record::BranchPointsArgs),
}
