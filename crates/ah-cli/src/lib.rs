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
pub use tui::FsSnapshotsType;

// Re-export TUI types for record/replay functionality
pub use ah_tui::record;
pub use ah_tui::replay;

#[derive(Parser)]
#[command(name = "ah")]
#[command(about = "Agent Harbor CLI")]
#[command(version, author, long_about = None)]
pub struct Cli {
    /// Additional configuration file to load (sits just below CLI flags in precedence order)
    #[arg(long, help = "Additional configuration file to load")]
    pub config: Option<String>,

    /// Set the log level (debug, info, warn, error)
    #[arg(long, help = "Set the log level")]
    #[arg(default_value = if cfg!(debug_assertions) { "debug" } else { "info" })]
    #[arg(value_parser = clap::builder::PossibleValuesParser::new(["debug", "info", "warn", "error"]))]
    pub log_level: String,

    /// Target repository (filesystem path in local runs; git URL may be used by some servers). If omitted, AH auto-detects a VCS root by walking parent directories and checking all supported VCS.
    #[arg(long)]
    pub repo: Option<String>,

    /// Filesystem snapshot provider to use
    #[arg(
        long,
        value_enum,
        default_value = "auto",
        help = "Filesystem snapshot provider (auto, zfs, btrfs, agentfs, git, disable)"
    )]
    pub fs_snapshots: FsSnapshotsType,

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
    Record(record::RecordArgs),
    /// Replay a recorded agent session
    Replay(replay::ReplayArgs),
    /// Extract branch points from a recorded session
    BranchPoints(record::BranchPointsArgs),
}
