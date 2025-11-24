// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only
#![allow(clippy::disallowed_methods)] // CLI commands intentionally print to stdout/stderr

//! Configuration management commands
use anyhow::Result;
use clap::Subcommand;
use config_core::{load_all, paths};

#[derive(Subcommand)]
pub enum ConfigCommands {
    /// Show current configuration values
    Show {
        /// Show configuration for specific key
        key: Option<String>,
        /// Show origin information for each value
        #[arg(long)]
        show_origin: bool,
    },
    /// Get the value of a configuration key
    Get {
        /// Configuration key to get
        key: String,
    },
    /// Set a configuration value
    Set {
        /// Configuration key to set
        key: String,
        /// Value to set
        value: String,
        /// Scope to set the value in (user, repo, repo-user)
        #[arg(long, default_value = "user")]
        scope: String,
    },
    /// Explain where a configuration value comes from
    Explain {
        /// Configuration key to explain
        key: String,
    },
}

impl ConfigCommands {
    pub async fn run(self, global_config: Option<&str>) -> Result<()> {
        match self {
            ConfigCommands::Show { key, show_origin } => {
                show_config(key.as_deref(), show_origin, global_config).await
            }
            ConfigCommands::Get { key } => get_config_value(&key, global_config).await,
            ConfigCommands::Set { key, value, scope } => {
                set_config_value(&key, &value, &scope, global_config).await
            }
            ConfigCommands::Explain { key } => explain_config(&key, global_config).await,
        }
    }
}

async fn show_config(
    key_filter: Option<&str>,
    show_origin: bool,
    config_file: Option<&str>,
) -> Result<()> {
    let mut paths = paths::discover_paths(None); // TODO: Get repo root from context
    if let Some(config_path) = config_file {
        paths.cli_config = Some(std::path::PathBuf::from(config_path));
    }
    let resolved = load_all(&paths, None)?;

    println!("Configuration:");
    if let Some(filter) = key_filter {
        // Show specific key
        if let Some(value) = get_nested_value(&resolved.json, filter) {
            if show_origin {
                if let Some(scope) = resolved.provenance.winner.get(filter) {
                    println!("{}={} (from {})", filter, value, format_scope(*scope));
                } else {
                    println!("{}={}", filter, value);
                }
            } else {
                println!("{}={}", filter, value);
            }
        } else {
            println!("Configuration key '{}' not found", filter);
        }
    } else {
        // Show all configuration
        print_json_with_provenance(&resolved.json, "", show_origin, &resolved.provenance);
    }

    Ok(())
}

async fn get_config_value(key: &str, config_file: Option<&str>) -> Result<()> {
    let mut paths = paths::discover_paths(None);
    if let Some(config_path) = config_file {
        paths.cli_config = Some(std::path::PathBuf::from(config_path));
    }
    let resolved = load_all(&paths, None)?;

    if let Some(value) = get_nested_value(&resolved.json, key) {
        println!("{}", value);
    } else {
        println!("Configuration key '{}' not found", key);
        std::process::exit(1);
    }

    Ok(())
}

async fn set_config_value(
    key: &str,
    value: &str,
    scope: &str,
    config_file: Option<&str>,
) -> Result<()> {
    let mut paths = paths::discover_paths(None);
    if let Some(config_path) = config_file {
        paths.cli_config = Some(std::path::PathBuf::from(config_path));
    }
    let resolved = load_all(&paths, None)?;

    // Check if the key is enforced
    if resolved.provenance.enforced.contains(key) {
        anyhow::bail!(
            "Cannot set '{}': key is enforced by system configuration",
            key
        );
    }

    // Validate that the key exists in schema by trying to set it in a temp config
    let temp_json = serde_json::json!({});
    let mut test_config = temp_json.clone();
    config_core::merge::insert_dotted(
        &mut test_config,
        key,
        serde_json::Value::String(value.to_string()),
    );

    if let Err(e) = config_core::loader::validate_against_schema(&test_config) {
        anyhow::bail!("Invalid configuration: {}", e);
    }

    // TODO: Actually write to the appropriate config file based on scope
    println!("Setting {}={} in {} scope", key, value, scope);
    println!("Note: Configuration writing not yet implemented");

    Ok(())
}

async fn explain_config(key: &str, config_file: Option<&str>) -> Result<()> {
    let mut paths = paths::discover_paths(None);
    if let Some(config_path) = config_file {
        paths.cli_config = Some(std::path::PathBuf::from(config_path));
    }
    let resolved = load_all(&paths, None)?;

    if let Some(scope) = resolved.provenance.winner.get(key) {
        println!("Configuration key: {}", key);
        println!("Winning scope: {}", format_scope(*scope));

        if resolved.provenance.enforced.contains(key) {
            println!("Status: ENFORCED (cannot be overridden)");
        }

        if let Some(changes) = resolved.provenance.changes.get(key) {
            println!("Change history:");
            for (change_scope, value) in changes {
                println!("  {}: {}", format_scope(*change_scope), value);
            }
        }
    } else {
        println!("Configuration key '{}' not found", key);
    }

    Ok(())
}

fn get_nested_value(json: &serde_json::Value, path: &str) -> Option<String> {
    let mut current = json;
    for part in path.split('.') {
        current = current.get(part)?;
    }
    Some(format!("{}", current))
}

fn print_json_with_provenance(
    json: &serde_json::Value,
    prefix: &str,
    show_origin: bool,
    provenance: &config_core::provenance::Provenance,
) {
    match json {
        serde_json::Value::Object(obj) => {
            for (key, value) in obj {
                let full_key = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{}.{}", prefix, key)
                };
                print_json_with_provenance(value, &full_key, show_origin, provenance);
            }
        }
        _ => {
            if show_origin {
                if let Some(scope) = provenance.winner.get(prefix) {
                    println!("{}={} (from {})", prefix, json, format_scope(*scope));
                } else {
                    println!("{}={}", prefix, json);
                }
            } else {
                println!("{}={}", prefix, json);
            }
        }
    }
}

fn format_scope(scope: config_core::Scope) -> &'static str {
    match scope {
        config_core::Scope::System => "system",
        config_core::Scope::User => "user",
        config_core::Scope::Repo => "repo",
        config_core::Scope::RepoUser => "repo-user",
        config_core::Scope::Env => "environment",
        config_core::Scope::CliConfig => "cli-config",

        config_core::Scope::Flags => "command-line",
    }
}
