// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use ah_domain_types::ExperimentalFeature;
use ah_logging::CliLoggingArgs;
use clap::Subcommand;
use serde_json::{Map, Value};

pub mod acp;
pub mod agent;
pub mod config;
pub mod config_commands;
pub mod credentials;
pub mod health;
pub mod sandbox;
pub mod task;
pub mod test_config;
pub mod transport;
pub mod tui;

pub trait SubcommandOverrides {
    fn config_path(&self) -> &'static str;
    fn to_config_json(&self) -> serde_json::Value;
}

pub trait ToJsonOverrides {
    fn to_json_overrides(&self) -> serde_json::Value;
}

#[derive(clap::Parser)]
#[command(
    name = "ah",
    about = "Agent Harbor CLI",
    version,
    propagate_version = true
)]
pub struct Cli {
    #[arg(long)]
    pub config: Option<String>,
    #[command(flatten)]
    pub logging: CliLoggingArgs,
    #[arg(long)]
    pub repo: Option<String>,
    #[arg(long = "fs-snapshots", value_enum, global = true)]
    pub fs_snapshots: Option<tui::FsSnapshotsType>,
    #[arg(long = "experimental-features", value_enum, num_args = 1.., value_delimiter = ',')]
    pub experimental_features: Option<Vec<ExperimentalFeature>>,
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Acp(acp::AcpArgs),
    Tui(tui::TuiArgs),
    Task {
        #[command(subcommand)]
        subcommand: task::TaskCommands,
    },
    Agent {
        #[command(subcommand)]
        subcommand: Box<AgentCommands>,
    },
    Config {
        #[command(subcommand)]
        subcommand: config_commands::ConfigCommands,
    },
    Health(health::HealthArgs),
    /// Credential management commands
    Credentials(credentials::CredentialsArgs),
}

#[derive(Subcommand)]
pub enum AgentCommands {
    Fs {
        #[command(subcommand)]
        subcommand: agent::fs::AgentFsCommands,
    },
    Sandbox(sandbox::SandboxRunArgs),
    Start(agent::start::AgentStartArgs),
    Record(ah_tui::record::RecordArgs),
    Replay(ah_tui::replay::ReplayArgs),
    BranchPoints(ah_tui::record::BranchPointsArgs),
}

impl ToJsonOverrides for Cli {
    fn to_json_overrides(&self) -> serde_json::Value {
        let mut map = Map::new();
        if let Some(level) = self.logging.log_level {
            map.insert("log-level".into(), serde_json::to_value(level).unwrap());
        }
        if let Some(fs) = &self.fs_snapshots {
            map.insert("fs-snapshots".into(), serde_json::to_value(fs).unwrap());
        }
        if let Some(features) = &self.experimental_features {
            if !features.is_empty() {
                map.insert(
                    "experimental-features".into(),
                    serde_json::to_value(features).unwrap(),
                );
            }
        }
        self.add_subcommand_overrides(&mut map);
        serde_json::Value::Object(map)
    }
}

impl Cli {
    pub fn to_json_overrides(&self) -> serde_json::Value {
        ToJsonOverrides::to_json_overrides(self)
    }

    fn add_subcommand_overrides(&self, map: &mut Map<String, Value>) {
        match &self.command {
            Commands::Tui(args) => self.merge_subcommand_config(map, args),
            Commands::Health(args) => self.merge_subcommand_config(map, args),
            Commands::Acp(_) => {}
            Commands::Task { .. } => {}
            Commands::Agent { .. } => {}
            Commands::Config { .. } => {}
            Commands::Credentials(_) => {}
        }
    }

    fn merge_subcommand_config<T: SubcommandOverrides>(
        &self,
        map: &mut Map<String, Value>,
        sub: &T,
    ) {
        let path = sub.config_path();
        let value = sub.to_config_json();
        if value.is_null() {
            return;
        }
        let mut segments = path.split('.').peekable();
        let mut current = map;
        while let Some(seg) = segments.next() {
            if segments.peek().is_none() {
                current.insert(seg.to_string(), value.clone());
            } else {
                current = current
                    .entry(seg.to_string())
                    .or_insert_with(|| Value::Object(Map::new()))
                    .as_object_mut()
                    .expect("config path should lead to JSON object");
            }
        }
    }
}

pub use clap::Parser;
