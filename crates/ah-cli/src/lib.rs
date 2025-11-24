// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Agent Harbor CLI library

pub mod agent;
pub mod config;
pub mod config_commands;
pub mod health;
pub mod sandbox;
pub mod task;
pub mod test_config;
pub mod transport;
pub mod tui;

// Re-export CLI types for testing
pub use clap::{Parser, Subcommand, ValueEnum};

// Re-export domain types
pub use ah_domain_types::{AgentSoftware, ExperimentalFeature, LogLevel};
pub use tui::FsSnapshotsType;

// Re-export TUI types for record/replay functionality
pub use ah_tui::record;
pub use ah_tui::replay;

#[derive(Parser, serde::Serialize)]
#[command(name = "ah")]
#[command(about = "Agent Harbor CLI")]
#[command(version, author, long_about = None)]
#[serde(rename_all = "kebab-case")]
pub struct Cli {
    /// Additional configuration file to load (sits just below CLI flags in precedence order)
    #[arg(long, help = "Additional configuration file to load")]
    #[serde(skip)]
    pub config: Option<String>,

    /// Set the log level
    #[arg(
        long,
        help = "Set the log level (default: info in release, debug in debug builds)"
    )]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_level: Option<LogLevel>,

    /// Target repository (filesystem path in local runs; git URL may be used by some servers). If omitted, AH auto-detects a VCS root by walking parent directories and checking all supported VCS.
    #[arg(long)]
    #[serde(skip)]
    pub repo: Option<String>,

    /// Filesystem snapshot provider to use
    #[arg(
        long,
        value_enum,
        help = "Filesystem snapshot provider (default: auto)",
        global = true
    )]
    #[serde(skip_serializing_if = "Cli::should_skip_fs_snapshots")]
    pub fs_snapshots: Option<FsSnapshotsType>,

    /// Enable experimental features (agents, modes, etc.)
    #[arg(
        long,
        help = "Enable experimental features like new agents or modes (default: none). Can be specified multiple times or as comma-separated values.",
        global = true,
        value_enum,
        value_delimiter = ','
    )]
    #[serde(skip_serializing_if = "Cli::should_skip_experimental_features")]
    pub experimental_features: Option<Vec<ExperimentalFeature>>,

    #[command(subcommand)]
    #[serde(skip)]
    pub command: Commands,
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)] // Boxing subcommands would complicate downstream matches; acceptable for CLI
pub enum Commands {
    /// Configuration management commands
    Config {
        #[command(subcommand)]
        subcommand: config_commands::ConfigCommands,
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

/// Trait for converting CLI structs to JSON overrides
///
/// This trait allows CLI argument structs to be converted to JSON values
/// that can be used as configuration overrides, following the TOML naming
/// conventions described in Configuration.md (dashes preserved).
pub trait ToJsonOverrides {
    fn to_json_overrides(&self) -> serde_json::Value;
}

/// Trait for subcommand arguments that contribute to configuration
///
/// Subcommands that have configuration-relevant options can implement this
/// trait to specify how their options should be mapped into the configuration
/// JSON structure.
pub trait SubcommandOverrides {
    /// The configuration path where this subcommand's options should be placed
    /// (e.g., "tui" for TUI options, "repo.init" for repo init options)
    fn config_path(&self) -> &'static str;

    /// Convert this subcommand's config-relevant options to JSON
    /// Only include fields that were explicitly provided by the user
    fn to_config_json(&self) -> serde_json::Value;
}

impl ToJsonOverrides for Cli {
    fn to_json_overrides(&self) -> serde_json::Value {
        // Start with global options from the Cli struct
        let mut global_json = serde_json::to_value(self).unwrap_or_default();

        // Extract subcommand options and merge them into the JSON
        if let serde_json::Value::Object(ref mut map) = global_json {
            self.add_subcommand_overrides(map);
        }

        global_json
    }
}

impl Cli {
    /// Add subcommand-specific configuration overrides to the JSON object
    ///
    /// This uses the SubcommandOverrides trait to automatically extract
    /// configuration options from subcommands that implement it.
    fn add_subcommand_overrides(&self, json_map: &mut serde_json::Map<String, serde_json::Value>) {
        match &self.command {
            Commands::Tui(tui_args) => {
                self.merge_subcommand_config(json_map, tui_args);
            }
            Commands::Health(health_args) => {
                self.merge_subcommand_config(json_map, health_args);
            }
            // Add more subcommands as they implement SubcommandOverrides
            _ => {}
        }
    }

    /// Merge configuration from a subcommand that implements SubcommandOverrides
    fn merge_subcommand_config<T: SubcommandOverrides>(
        &self,
        json_map: &mut serde_json::Map<String, serde_json::Value>,
        subcommand: &T,
    ) {
        let config_json = subcommand.to_config_json();

        // Only add if the subcommand actually has config options
        if !config_json.is_null()
            && config_json != serde_json::Value::Object(serde_json::Map::new())
        {
            let path = subcommand.config_path();
            let nested_value = Self::create_nested_value(path, config_json);
            Self::merge_nested_value(json_map, nested_value);
        }
    }

    /// Create a nested JSON value from a dot-separated path
    /// e.g., "tui" -> {"tui": config_json}
    /// e.g., "repo.init" -> {"repo": {"init": config_json}}
    fn create_nested_value(path: &str, config_json: serde_json::Value) -> serde_json::Value {
        let parts: Vec<&str> = path.split('.').collect();
        let mut value = config_json;

        // Build nested structure from inside out
        for part in parts.iter().rev() {
            let mut map = serde_json::Map::new();
            map.insert(part.to_string(), value);
            value = serde_json::Value::Object(map);
        }

        value
    }

    /// Deep merge a nested value into the root JSON map
    fn merge_nested_value(
        root_map: &mut serde_json::Map<String, serde_json::Value>,
        nested_value: serde_json::Value,
    ) {
        if let serde_json::Value::Object(nested_map) = nested_value {
            for (key, value) in nested_map {
                if let Some(existing_value) = root_map.get_mut(&key) {
                    // Check if both are objects for merging
                    if existing_value.is_object() && value.is_object() {
                        if let (
                            serde_json::Value::Object(ref mut existing_obj),
                            serde_json::Value::Object(ref new_obj),
                        ) = (existing_value, &value)
                        {
                            for (inner_key, inner_value) in new_obj {
                                existing_obj.insert(inner_key.clone(), inner_value.clone());
                            }
                        }
                    } else {
                        // Otherwise, replace
                        *existing_value = value;
                    }
                } else {
                    root_map.insert(key, value);
                }
            }
        }
    }
}

impl Cli {
    /// Skip fs_snapshots serialization if it equals the default "auto" value or None
    fn should_skip_fs_snapshots(fs_snapshots: &Option<FsSnapshotsType>) -> bool {
        match fs_snapshots {
            Some(FsSnapshotsType::Auto) => true, // Skip if it's the default
            None => true,                        // Skip if None (not provided)
            _ => false,                          // Include if explicitly set to something else
        }
    }

    /// Skip experimental_features serialization if it's empty or None
    fn should_skip_experimental_features(
        experimental_features: &Option<Vec<ExperimentalFeature>>,
    ) -> bool {
        match experimental_features {
            Some(vec) if vec.is_empty() => true, // Skip if empty
            None => true,                        // Skip if None
            _ => false,                          // Include if it has values
        }
    }
}
