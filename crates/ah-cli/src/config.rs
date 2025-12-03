// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Root configuration struct and loading infrastructure for Agent Harbor CLI.
//!
//! This module provides the main `Config` struct that composes all subsystem
//! configurations using Serde flattening. Configuration is loaded once at
//! application startup and distributed to subsystems as typed objects.

use ah_config_types::sandbox::SandboxConfig;
use ah_core::task_config::TaskConfig;
use ah_fs_snapshots::fs_snapshots_config::FsSnapshotsConfig;
use ah_logging::logging_config::LoggingConfig;
use ah_rest_client::network_config::NetworkConfig;
use ah_tui::tui_config::TuiConfig;
use anyhow::Result;
use config_core::{load_all, paths};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Root configuration struct that composes all subsystem configurations.
/// Uses Serde flattening to merge subsystem configs into the final JSON structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    // Startup configuration - early decisions made at app launch
    #[serde(flatten)]
    pub startup: StartupConfig,

    // TUI-specific configuration
    #[serde(flatten)]
    pub tui: TuiConfig,

    // Repository/project configuration
    pub repo: Option<RepoConfig>,

    // Browser automation configuration
    #[serde(flatten)]
    pub browser_automation: BrowserAutomationConfig,

    // Filesystem/snapshot configuration
    #[serde(flatten)]
    pub fs_snapshots: FsSnapshotsConfig,

    // Network configuration
    #[serde(flatten)]
    pub network: NetworkConfig,

    // Task/execution configuration
    #[serde(flatten)]
    pub task: TaskConfig,

    // Logging configuration
    #[serde(flatten)]
    pub logging: LoggingConfig,

    /// Sandbox configuration for Linux local sandboxing.
    /// See specs/Public/Sandboxing/Local-Sandboxing-on-Linux.md
    pub sandbox: Option<SandboxConfig>,
}

/// Startup configuration - decisions made before UI initialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupConfig {
    /// Default UI to launch (tui/webui)
    pub ui: Option<String>,
    /// Remote server name/URL (determines local vs remote mode)
    #[serde(rename = "remote-server")]
    pub remote_server: Option<String>,
}

/// Repository/project configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct RepoConfig {
    /// Supported agents configuration
    pub supported_agents: Option<String>,
    /// Repository initialization settings
    pub init: Option<RepoInitConfig>,
}

/// Repository initialization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct RepoInitConfig {
    /// VCS type for repo initialization
    pub vcs: Option<String>,
    /// Development environment for repo initialization
    pub devenv: Option<String>,
    /// Devcontainer enablement
    pub devcontainer: Option<bool>,
    /// Direnv enablement
    pub direnv: Option<bool>,
    /// Task runner for repo initialization
    pub task_runner: Option<String>,
}

/// Browser automation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserAutomationConfig {
    /// Enable/disable browser automation
    #[serde(rename = "browser-automation")]
    pub enabled: Option<bool>,
    /// Browser profile name
    #[serde(rename = "browser-profile")]
    pub profile: Option<String>,
    /// ChatGPT username
    #[serde(rename = "chatgpt-username")]
    pub chatgpt_username: Option<String>,
    /// Codex workspace identifier
    #[serde(rename = "codex-workspace")]
    pub codex_workspace: Option<String>,
}

/// Configuration loading result with provenance information
pub struct ConfigResult {
    pub config: Config,
    pub provenance: config_core::provenance::Provenance,
}

/// Load and merge configuration from all sources (system, user, repo, repo-user, env, CLI)
pub fn load_config(
    repo_path: Option<&str>,
    cli_config: Option<&str>,
    cli_overrides: Option<&serde_json::Value>,
) -> Result<ConfigResult> {
    // Discover configuration file paths
    let repo_path_buf = repo_path.map(PathBuf::from);
    let repo_path_ref = repo_path_buf.as_deref();
    let mut paths = paths::discover_paths(repo_path_ref);

    // Override CLI config if specified
    if let Some(config_path) = cli_config {
        paths.cli_config = Some(PathBuf::from(config_path));
    }

    // Load and merge all configuration layers
    let resolved = load_all(&paths, cli_overrides)?;

    // Extract typed configuration from the merged JSON
    let config: Config = serde_json::from_value(resolved.json.clone())?;

    Ok(ConfigResult {
        config,
        provenance: resolved.provenance,
    })
}

/// Extract subsystem configuration objects from the root config
impl Config {
    /// Get startup configuration
    pub fn startup(&self) -> &StartupConfig {
        &self.startup
    }

    /// Get TUI configuration
    pub fn tui(&self) -> &TuiConfig {
        &self.tui
    }

    /// Get repository configuration
    pub fn repo(&self) -> Option<&RepoConfig> {
        self.repo.as_ref()
    }

    /// Get browser automation configuration
    pub fn browser_automation(&self) -> &BrowserAutomationConfig {
        &self.browser_automation
    }

    /// Get filesystem snapshots configuration
    pub fn fs_snapshots(&self) -> &FsSnapshotsConfig {
        &self.fs_snapshots
    }

    /// Get network configuration
    pub fn network(&self) -> &NetworkConfig {
        &self.network
    }

    /// Get task configuration
    pub fn task(&self) -> &TaskConfig {
        &self.task
    }

    /// Get logging configuration
    pub fn logging(&self) -> &LoggingConfig {
        &self.logging
    }

    /// Get sandbox configuration
    pub fn sandbox(&self) -> Option<&SandboxConfig> {
        self.sandbox.as_ref()
    }
}

#[cfg(test)]
mod tests {
    /// These tests aim to verify the correctness of our implementation
    /// against the specification in `specs/Public/Configuration.md`.
    use super::*;
    use ah_logging::CliLogLevel;
    use serial_test::serial;

    fn logging_args() -> ah_logging::CliLoggingArgs {
        ah_logging::CliLoggingArgs::default()
    }

    fn logging_with_level(level: ah_logging::CliLogLevel) -> ah_logging::CliLoggingArgs {
        ah_logging::CliLoggingArgs {
            log_level: Some(level),
            ..Default::default()
        }
    }

    #[test]
    #[serial]
    fn test_config_deserialization() {
        let json = serde_json::json!({
            "ui": "tui",
            "remote-server": "https://example.com",
            "terminal-multiplexer": "tmux",
            "editor": "vim",
            "tui-font-style": "nerdfont",
            "tui-font": "Fira Code",
            "repo": {
                "supported-agents": "all",
                "init": {
                    "vcs": "git"
                }
            },
            "browser-automation": true,
            "browser-profile": "work",
            "fs-snapshots": "auto",
            "working-copy": "cow-overlay",
            "service-base-url": "https://ah.example.com",
            "notifications": true,
            "log-level": "info"
        });

        let config: Config = serde_json::from_value(json).unwrap();

        assert_eq!(config.startup.ui.as_deref(), Some("tui"));
        assert_eq!(
            config.startup.remote_server.as_deref(),
            Some("https://example.com")
        );
        assert_eq!(config.tui.terminal_multiplexer.as_deref(), Some("tmux"));
        assert_eq!(
            config.repo.as_ref().unwrap().supported_agents.as_deref(),
            Some("all")
        );
        assert_eq!(config.browser_automation.enabled, Some(true));
        assert_eq!(config.fs_snapshots.provider.as_deref(), Some("auto"));
        assert_eq!(config.logging.level, Some(CliLogLevel::Info));
    }

    #[test]
    #[serial]
    fn test_config_subsystem_access() {
        let config = Config {
            startup: StartupConfig {
                ui: Some("webui".to_string()),
                remote_server: Some("https://test.com".to_string()),
            },
            tui: TuiConfig {
                terminal_multiplexer: Some("screen".to_string()),
                ..TuiConfig::default()
            },
            repo: Some(RepoConfig {
                supported_agents: Some("all".to_string()),
                init: Some(RepoInitConfig {
                    vcs: Some("git".to_string()),
                    devenv: Some("nix".to_string()),
                    devcontainer: Some(true),
                    direnv: Some(false),
                    task_runner: Some("just".to_string()),
                }),
            }),
            browser_automation: BrowserAutomationConfig {
                enabled: Some(true),
                profile: Some("work".to_string()),
                chatgpt_username: Some("user@example.com".to_string()),
                codex_workspace: Some("main".to_string()),
            },
            fs_snapshots: FsSnapshotsConfig {
                provider: Some("zfs".to_string()),
                working_copy: Some("worktree".to_string()),
            },
            network: NetworkConfig {
                service_base_url: Some("https://service.example.com".to_string()),
            },
            task: TaskConfig {
                notifications: Some(true),
                task_editor_use_vcs_comment_string: Some(true),
                task_template: Some("/path/to/template".to_string()),
            },
            logging: LoggingConfig {
                level: Some(CliLogLevel::Debug),
                ..LoggingConfig::default()
            },
            sandbox: None,
        };

        // Test subsystem access methods
        assert_eq!(config.startup().ui.as_deref(), Some("webui"));
        assert_eq!(
            config.startup().remote_server.as_deref(),
            Some("https://test.com")
        );
        assert_eq!(config.tui().terminal_multiplexer.as_deref(), Some("screen"));
        assert_eq!(
            config.repo().unwrap().supported_agents.as_deref(),
            Some("all")
        );
        assert_eq!(config.browser_automation().enabled, Some(true));
        assert_eq!(config.fs_snapshots().provider.as_deref(), Some("zfs"));
        assert_eq!(
            config.network().service_base_url.as_deref(),
            Some("https://service.example.com")
        );
        assert_eq!(config.task().notifications, Some(true));
        assert_eq!(config.logging().level, Some(CliLogLevel::Debug));
    }

    #[test]
    #[serial]
    fn test_config_toml_loading_and_precedence() {
        use std::fs;
        use tempfile::TempDir;

        // Create a temporary directory structure
        let temp_dir = TempDir::new().unwrap();
        let repo_dir = temp_dir.path().join("test-repo");
        fs::create_dir(&repo_dir).unwrap();

        // Create system-level config
        let system_config = r#"
ui = "tui"
terminal-multiplexer = "screen"
log-level = "warn"
"#;
        let system_path = temp_dir.path().join("system-config.toml");
        fs::write(&system_path, system_config).unwrap();

        // Create user-level config (should override system)
        let user_config = r#"
ui = "webui"
terminal-multiplexer = "tmux"
editor = "vim"
log-level = "info"
"#;
        let user_path = temp_dir.path().join("user-config.toml");
        fs::write(&user_path, user_config).unwrap();

        // Create repo-level config (should override user for repo-specific settings)
        let repo_config = r#"
repo.supported-agents = "all"
repo.init.vcs = "git"
terminal-multiplexer = "zellij"
editor = "nano"
"#;
        let repo_config_path = repo_dir.join(".agents").join("config.toml");
        fs::create_dir_all(repo_config_path.parent().unwrap()).unwrap();
        fs::write(&repo_config_path, repo_config).unwrap();

        // Create repo-user-level config (highest precedence for repo settings)
        let repo_user_config = r#"
editor = "helix"
log-level = "debug"
"#;
        let repo_user_config_path = repo_dir.join(".agents").join("config.user.toml");
        fs::write(&repo_user_config_path, repo_user_config).unwrap();

        // Create mock paths that point to our temp files
        let mock_paths = config_core::paths::Paths {
            system: system_path.clone(),
            user: user_path.clone(),
            repo: Some(repo_config_path.clone()),
            repo_user: Some(repo_user_config_path.clone()),
            cli_config: None,
        };

        // Load configuration without environment variables or CLI flags
        let resolved = load_all(&mock_paths, None).unwrap();

        // Extract typed config
        let config: Config = serde_json::from_value(resolved.json).unwrap();

        // Verify precedence: repo-user > repo > user > system

        // UI: user "webui" should override system "tui" (no repo override)
        assert_eq!(config.startup.ui.as_deref(), Some("webui"));

        // Terminal multiplexer: repo "zellij" should override user "tmux"
        assert_eq!(config.tui.terminal_multiplexer.as_deref(), Some("zellij"));

        // Editor: repo-user "helix" should override repo "nano" and user "vim"
        assert_eq!(config.tui.editor.as_deref(), Some("helix"));

        // Log level: repo-user "debug" should override user "info" and system "warn"
        assert_eq!(config.logging.level, Some(CliLogLevel::Debug));

        // Repo config should be loaded (nested structure)
        assert!(config.repo.is_some());
        let repo_config = config.repo.as_ref().unwrap();
        assert_eq!(repo_config.supported_agents.as_deref(), Some("all"));
        assert!(repo_config.init.is_some());
        let init_config = repo_config.init.as_ref().unwrap();
        assert_eq!(init_config.vcs.as_deref(), Some("git"));
    }

    #[test]
    #[serial]
    fn test_admin_enforcement() {
        use std::fs;
        use tempfile::TempDir;

        // Create a temporary directory structure
        let temp_dir = TempDir::new().unwrap();
        let repo_dir = temp_dir.path().join("test-repo");
        fs::create_dir(&repo_dir).unwrap();

        // Create system-level config with enforced keys
        let system_config = r#"
# Enforced keys that cannot be overridden by lower scopes
enforced = ["remote-server", "log-level"]

remote-server = "admin-server"
log-level = "warn"
ui = "tui"
"#;
        let system_path = temp_dir.path().join("system-config.toml");
        fs::write(&system_path, system_config).unwrap();

        // Create user-level config that tries to override enforced keys
        let user_config = r#"
remote-server = "user-server"  # This should be masked/ignored
log-level = "debug"            # This should be masked/ignored
ui = "webui"                   # This can be overridden
editor = "vim"
"#;
        let user_path = temp_dir.path().join("user-config.toml");
        fs::write(&user_path, user_config).unwrap();

        // Create repo-level config that also tries to override
        let repo_config = r#"
remote-server = "repo-server"  # This should be masked/ignored
ui = "tui"                     # This can be overridden
terminal-multiplexer = "tmux"
"#;
        let repo_config_path = repo_dir.join(".agents").join("config.toml");
        fs::create_dir_all(repo_config_path.parent().unwrap()).unwrap();
        fs::write(&repo_config_path, repo_config).unwrap();

        // Create mock paths
        let mock_paths = config_core::paths::Paths {
            system: system_path.clone(),
            user: user_path.clone(),
            repo: Some(repo_config_path.clone()),
            repo_user: None,
            cli_config: None,
        };

        // Load configuration
        let resolved = load_all(&mock_paths, None).unwrap();

        // Verify enforcement behavior
        let config: Config = serde_json::from_value(resolved.json).unwrap();

        // Enforced keys should retain system values
        assert_eq!(
            config.startup.remote_server.as_deref(),
            Some("admin-server")
        );
        assert_eq!(config.logging.level, Some(CliLogLevel::Warn));

        // Non-enforced keys should be overridden by lower scopes
        assert_eq!(config.startup.ui.as_deref(), Some("tui")); // repo overrides user
        assert_eq!(config.tui.terminal_multiplexer.as_deref(), Some("tmux")); // from repo
        assert_eq!(config.tui.editor.as_deref(), Some("vim")); // from user

        // Verify provenance marks enforced keys
        assert!(resolved.provenance.enforced.contains("remote-server"));
        assert!(resolved.provenance.enforced.contains("log-level"));
        assert!(!resolved.provenance.enforced.contains("ui"));
    }

    #[test]
    #[serial]
    fn test_provenance_tracking() {
        use std::fs;
        use tempfile::TempDir;

        // Create a temporary directory structure
        let temp_dir = TempDir::new().unwrap();
        let repo_dir = temp_dir.path().join("test-repo");
        fs::create_dir(&repo_dir).unwrap();

        // Create system-level config
        let system_config = r#"
ui = "tui"
log-level = "warn"
"#;
        let system_path = temp_dir.path().join("system-config.toml");
        fs::write(&system_path, system_config).unwrap();

        // Create user-level config (overrides system)
        let user_config = r#"
ui = "webui"
editor = "vim"
"#;
        let user_path = temp_dir.path().join("user-config.toml");
        fs::write(&user_path, user_config).unwrap();

        // Create repo-level config (overrides user for some keys)
        let repo_config = r#"
terminal-multiplexer = "tmux"
editor = "nano"  # overrides user
"#;
        let repo_config_path = repo_dir.join(".agents").join("config.toml");
        fs::create_dir_all(repo_config_path.parent().unwrap()).unwrap();
        fs::write(&repo_config_path, repo_config).unwrap();

        // Create repo-user-level config (highest precedence)
        let repo_user_config = r#"
editor = "helix"  # overrides repo
log-level = "debug"  # overrides system
"#;
        let repo_user_config_path = repo_dir.join(".agents").join("config.user.toml");
        fs::write(&repo_user_config_path, repo_user_config).unwrap();

        // Set environment variables (override repo-user)
        std::env::set_var("AH_UI", "tui");
        std::env::set_var("AH_TERMINAL_MULTIPLEXER", "screen");

        // Create mock paths
        let mock_paths = config_core::paths::Paths {
            system: system_path.clone(),
            user: user_path.clone(),
            repo: Some(repo_config_path.clone()),
            repo_user: Some(repo_user_config_path.clone()),
            cli_config: None,
        };

        // Load configuration
        let resolved = load_all(&mock_paths, None).unwrap();

        // Verify provenance tracking for key progression
        use config_core::Scope::*;

        // UI: set in system -> user -> env (env wins)
        assert_eq!(resolved.provenance.winner["ui"], Env);
        let ui_changes = &resolved.provenance.changes["ui"];
        assert_eq!(ui_changes.len(), 3);
        assert_eq!(ui_changes[0], (System, serde_json::json!("tui")));
        assert_eq!(ui_changes[1], (User, serde_json::json!("webui")));
        assert_eq!(ui_changes[2], (Env, serde_json::json!("tui")));

        // Editor: set in user -> repo -> repo-user (repo-user wins)
        assert_eq!(resolved.provenance.winner["editor"], RepoUser);
        let editor_changes = &resolved.provenance.changes["editor"];
        assert_eq!(editor_changes.len(), 3);
        assert_eq!(editor_changes[0], (User, serde_json::json!("vim")));
        assert_eq!(editor_changes[1], (Repo, serde_json::json!("nano")));
        assert_eq!(editor_changes[2], (RepoUser, serde_json::json!("helix")));

        // Terminal multiplexer: set in repo -> env (env wins)
        assert_eq!(resolved.provenance.winner["terminal-multiplexer"], Env);
        let tmux_changes = &resolved.provenance.changes["terminal-multiplexer"];
        assert_eq!(tmux_changes.len(), 2);
        assert_eq!(tmux_changes[0], (Repo, serde_json::json!("tmux")));
        assert_eq!(tmux_changes[1], (Env, serde_json::json!("screen")));

        // Log level: set in system -> repo-user (repo-user wins)
        assert_eq!(resolved.provenance.winner["log-level"], RepoUser);
        let log_changes = &resolved.provenance.changes["log-level"];
        assert_eq!(log_changes.len(), 2);
        assert_eq!(log_changes[0], (System, serde_json::json!("warn")));
        assert_eq!(log_changes[1], (RepoUser, serde_json::json!("debug")));

        // Verify final configuration values
        let config: Config = serde_json::from_value(resolved.json).unwrap();
        assert_eq!(config.startup.ui.as_deref(), Some("tui")); // env wins
        assert_eq!(config.tui.editor.as_deref(), Some("helix")); // repo-user wins
        assert_eq!(config.tui.terminal_multiplexer.as_deref(), Some("screen")); // env wins
        assert_eq!(config.logging.level, Some(CliLogLevel::Debug)); // repo-user wins

        // Clean up environment variables
        std::env::remove_var("AH_UI");
        std::env::remove_var("AH_TERMINAL_MULTIPLEXER");
    }

    #[test]
    #[serial]
    fn test_cli_serialization_with_provided_options() {
        use crate::{Cli, Commands};

        // Test with all config-relevant options provided
        let cli = Cli {
            config: None,
            logging: logging_with_level(ah_logging::CliLogLevel::Debug),
            repo: None,
            fs_snapshots: Some(crate::tui::FsSnapshotsType::Git),
            experimental_features: Some(vec![ah_domain_types::ExperimentalFeature::Gemini]),
            command: Commands::Config {
                subcommand: crate::config_commands::ConfigCommands::Show {
                    key: None,
                    show_origin: false,
                },
            },
        };

        let json = cli.to_json_overrides();
        let expected = serde_json::json!({
            "log-level": "debug",
            "fs-snapshots": "git",
            "experimental-features": ["gemini"]
        });

        assert_eq!(json, expected);
    }

    #[test]
    #[serial]
    fn test_cli_serialization_with_no_options() {
        use crate::{Cli, Commands};

        // Test with no config-relevant options provided
        let cli = Cli {
            config: None,
            logging: logging_args(),
            repo: None,
            fs_snapshots: None,
            experimental_features: None,
            command: Commands::Config {
                subcommand: crate::config_commands::ConfigCommands::Show {
                    key: None,
                    show_origin: false,
                },
            },
        };

        let json = cli.to_json_overrides();
        let expected = serde_json::json!({}); // Empty object when no options provided

        assert_eq!(json, expected);
    }

    #[test]
    #[serial]
    fn test_cli_serialization_with_partial_options() {
        use crate::{Cli, Commands};

        // Test with only some options provided
        let cli = Cli {
            config: Some("config.toml".to_string()), // This should be skipped
            logging: logging_with_level(ah_logging::CliLogLevel::Info),
            repo: Some("/path/to/repo".to_string()), // This should be skipped
            fs_snapshots: None,                      // Not provided
            experimental_features: Some(vec![]),     // Empty vec provided
            command: Commands::Config {
                subcommand: crate::config_commands::ConfigCommands::Show {
                    key: None,
                    show_origin: false,
                },
            },
        };

        let json = cli.to_json_overrides();
        let expected = serde_json::json!({
            "log-level": "info"
            // fs-snapshots and experimental-features not included since None/empty
        });

        assert_eq!(json, expected);
    }

    #[test]
    #[serial]
    fn test_multi_layer_config_precedence_with_cli() {
        use std::fs;
        use tempfile::TempDir;

        // Create a temporary directory structure
        let temp_dir = TempDir::new().unwrap();
        let repo_dir = temp_dir.path().join("test-repo");
        fs::create_dir(&repo_dir).unwrap();

        // Create system-level config
        let system_config = r#"
ui = "tui"
log-level = "warn"
fs-snapshots = "zfs"
"#;
        let system_path = temp_dir.path().join("system-config.toml");
        fs::write(&system_path, system_config).unwrap();

        // Create user-level config (should override system)
        let user_config = r#"
ui = "webui"
log-level = "info"
terminal-multiplexer = "tmux"
"#;
        let user_path = temp_dir.path().join("user-config.toml");
        fs::write(&user_path, user_config).unwrap();

        // Create repo-level config (should override user)
        let repo_config = r#"
repo.supported-agents = "all"
log-level = "debug"
editor = "vim"
"#;
        let repo_config_path = repo_dir.join(".agents").join("config.toml");
        fs::create_dir_all(repo_config_path.parent().unwrap()).unwrap();
        fs::write(&repo_config_path, repo_config).unwrap();

        // Set environment variables (should override repo)
        std::env::set_var("AH_UI", "tui");
        std::env::set_var("AH_LOG_LEVEL", "error");
        std::env::set_var("AH_FS_SNAPSHOTS", "git");

        // Create mock paths
        let mock_paths = config_core::paths::Paths {
            system: system_path.clone(),
            user: user_path.clone(),
            repo: Some(repo_config_path.clone()),
            repo_user: None,
            cli_config: None,
        };

        // Test CLI overrides (highest precedence)
        let cli_overrides = serde_json::json!({
            "log-level": "trace",  // Should override env
            "fs-snapshots": "auto" // Should override env
        });

        // Load configuration with CLI overrides
        let resolved = load_all(&mock_paths, Some(&cli_overrides)).unwrap();
        let config: Config = serde_json::from_value(resolved.json).unwrap();

        // Verify precedence: system < user < repo < env < cli
        assert_eq!(config.startup.ui.as_deref(), Some("tui")); // CLI overrides env
        assert_eq!(config.logging.level, Some(CliLogLevel::Trace)); // CLI overrides env
        assert_eq!(config.fs_snapshots.provider.as_deref(), Some("auto")); // CLI overrides env
        assert_eq!(config.tui.terminal_multiplexer.as_deref(), Some("tmux")); // User overrides system
        assert_eq!(config.tui.editor.as_deref(), Some("vim")); // Repo overrides user

        // Clean up environment variables
        std::env::remove_var("AH_UI");
        std::env::remove_var("AH_LOG_LEVEL");
        std::env::remove_var("AH_FS_SNAPSHOTS");
    }

    #[test]
    #[serial]
    fn test_environment_variable_overrides_with_cli() {
        use std::fs;
        use tempfile::TempDir;

        // Create a temporary directory structure
        let temp_dir = TempDir::new().unwrap();

        // Create system-level config
        let system_config = r#"
log-level = "warn"
fs-snapshots = "zfs"
"#;
        let system_path = temp_dir.path().join("system-config.toml");
        fs::write(&system_path, system_config).unwrap();

        // Set environment variables
        std::env::set_var("AH_LOG_LEVEL", "info");
        std::env::set_var("AH_FS_SNAPSHOTS", "btrfs");

        // Create mock paths
        let mock_paths = config_core::paths::Paths {
            system: system_path.clone(),
            user: Default::default(),
            repo: None,
            repo_user: None,
            cli_config: None,
        };

        // Test CLI overrides environment variables
        let cli_overrides = serde_json::json!({
            "log-level": "debug",
            "experimental-features": ["codex"]
        });

        let resolved = load_all(&mock_paths, Some(&cli_overrides)).unwrap();
        let config: Config = serde_json::from_value(resolved.json).unwrap();

        // CLI should override environment
        assert_eq!(config.logging.level, Some(CliLogLevel::Debug));
        // Environment should override system
        assert_eq!(config.fs_snapshots.provider.as_deref(), Some("btrfs"));
        // CLI should add new values
        // Note: experimental_features not yet stored in config

        // Clean up
        std::env::remove_var("AH_LOG_LEVEL");
        std::env::remove_var("AH_FS_SNAPSHOTS");
    }

    #[test]
    #[serial]
    fn test_cli_partial_override_behavior() {
        use std::fs;
        use tempfile::TempDir;

        // Create a temporary directory structure
        let temp_dir = TempDir::new().unwrap();

        // Create user config with multiple settings
        let user_config = r#"
log-level = "info"
fs-snapshots = "zfs"
terminal-multiplexer = "screen"
editor = "nano"
"#;
        let user_path = temp_dir.path().join("user-config.toml");
        fs::write(&user_path, user_config).unwrap();

        // Create mock paths
        let mock_paths = config_core::paths::Paths {
            system: Default::default(),
            user: user_path.clone(),
            repo: None,
            repo_user: None,
            cli_config: None,
        };

        // Test partial CLI override - only override some settings
        let cli_overrides = serde_json::json!({
            "log-level": "debug"  // Only override log level
            // Don't override fs-snapshots, terminal-multiplexer, or editor
        });

        let resolved = load_all(&mock_paths, Some(&cli_overrides)).unwrap();
        let config: Config = serde_json::from_value(resolved.json).unwrap();

        // CLI override should take effect
        assert_eq!(config.logging.level, Some(CliLogLevel::Debug));
        // Other settings should remain from user config
        assert_eq!(config.fs_snapshots.provider.as_deref(), Some("zfs"));
        assert_eq!(config.tui.terminal_multiplexer.as_deref(), Some("screen"));
        assert_eq!(config.tui.editor.as_deref(), Some("nano"));
    }

    #[test]
    #[serial]
    fn test_cli_override_with_empty_values() {
        use std::fs;
        use tempfile::TempDir;

        // Create a temporary directory structure
        let temp_dir = TempDir::new().unwrap();

        // Create user config
        let user_config = r#"
experimental-features = ["codex", "claude"]
"#;
        let user_path = temp_dir.path().join("user-config.toml");
        fs::write(&user_path, user_config).unwrap();

        // Create mock paths
        let mock_paths = config_core::paths::Paths {
            system: Default::default(),
            user: user_path.clone(),
            repo: None,
            repo_user: None,
            cli_config: None,
        };

        // Test CLI override with empty experimental features
        let cli_overrides = serde_json::json!({
            "experimental-features": []  // Explicitly set to empty
        });

        let resolved = load_all(&mock_paths, Some(&cli_overrides)).unwrap();
        let _config: Config = serde_json::from_value(resolved.json).unwrap();

        // CLI should override with empty array
        // Note: experimental_features not yet stored in config
    }

    #[test]
    #[serial]
    fn test_full_integration_cli_to_config() {
        use crate::{Cli, Commands};
        use std::fs;
        use tempfile::TempDir;

        // Create a temporary directory structure
        let temp_dir = TempDir::new().unwrap();
        let repo_dir = temp_dir.path().join("test-repo");
        fs::create_dir(&repo_dir).unwrap();

        // Create system config
        let system_config = r#"
ui = "tui"
log-level = "warn"
"#;
        let system_path = temp_dir.path().join("system-config.toml");
        fs::write(&system_path, system_config).unwrap();

        // Create user config
        let user_config = r#"
ui = "webui"
terminal-multiplexer = "tmux"
"#;
        let user_path = temp_dir.path().join("user-config.toml");
        fs::write(&user_path, user_config).unwrap();

        // Create repo config
        let repo_config = r#"
repo.supported-agents = "all"
editor = "vim"
"#;
        let repo_config_path = repo_dir.join(".agents").join("config.toml");
        fs::create_dir_all(repo_config_path.parent().unwrap()).unwrap();
        fs::write(&repo_config_path, repo_config).unwrap();

        // Set environment variable
        std::env::set_var("AH_LOG_LEVEL", "info");

        // Create CLI with some overrides
        let cli = Cli {
            config: None,
            logging: logging_with_level(ah_logging::CliLogLevel::Debug), // Should override env
            repo: Some(repo_dir.to_string_lossy().to_string()),
            fs_snapshots: Some(crate::tui::FsSnapshotsType::Git), // Should add new setting
            experimental_features: None,
            command: Commands::Config {
                subcommand: crate::config_commands::ConfigCommands::Show {
                    key: None,
                    show_origin: false,
                },
            },
        };

        // Test the full pipeline: CLI → JSON → Config loading → Final config
        let cli_json = cli.to_json_overrides();
        let expected_cli_json = serde_json::json!({
            "log-level": "debug",
            "fs-snapshots": "git"
        });
        assert_eq!(cli_json, expected_cli_json);

        // Load config with CLI overrides
        let mock_paths = config_core::paths::Paths {
            system: system_path.clone(),
            user: user_path.clone(),
            repo: Some(repo_config_path.clone()),
            repo_user: None,
            cli_config: None,
        };

        let resolved = load_all(&mock_paths, Some(&cli_json)).unwrap();
        let config: Config = serde_json::from_value(resolved.json).unwrap();

        // Verify final precedence:
        // system < user < repo < env < cli
        assert_eq!(config.startup.ui.as_deref(), Some("webui")); // User overrides system
        assert_eq!(config.logging.level, Some(CliLogLevel::Debug)); // CLI overrides env
        assert_eq!(config.fs_snapshots.provider.as_deref(), Some("git")); // CLI adds new
        assert_eq!(config.tui.terminal_multiplexer.as_deref(), Some("tmux")); // User config
        assert_eq!(config.tui.editor.as_deref(), Some("vim")); // Repo config
        assert!(config.repo.is_some());
        assert_eq!(
            config.repo.as_ref().unwrap().supported_agents.as_deref(),
            Some("all")
        );

        // Clean up
        std::env::remove_var("AH_LOG_LEVEL");
    }

    #[test]
    #[serial]
    fn test_subcommand_override_structure() {
        use crate::{Cli, Commands};

        // Test that the framework is ready for subcommand overrides
        // When repo commands are implemented, this test can be expanded

        let cli = Cli {
            config: None,
            logging: logging_with_level(ah_logging::CliLogLevel::Info),
            repo: None,
            fs_snapshots: None,
            experimental_features: None,
            command: Commands::Config {
                subcommand: crate::config_commands::ConfigCommands::Show {
                    key: None,
                    show_origin: false,
                },
            },
        };

        let json = cli.to_json_overrides();

        // Should only contain global options that were explicitly provided
        let expected = serde_json::json!({
            "log-level": "info"
        });

        assert_eq!(json, expected);

        // When subcommands with config options are implemented, the JSON should include
        // nested structures like:
        // {
        //   "log-level": "info",
        //   "repo": {
        //     "init": {
        //       "vcs": "git",
        //       "devenv": "nix"
        //     }
        //   }
        // }
    }

    #[test]
    #[serial]
    fn test_tui_subcommand_overrides() {
        use crate::tui::{CliMultiplexerArg, TuiArgs};
        use crate::{Cli, Commands};

        // Test TUI subcommand with configuration options
        let tui_args = TuiArgs {
            remote_server: Some("https://ah.example.com".to_string()),
            api_key: Some("test-key".to_string()),
            bearer_token: None,
            multiplexer: Some(CliMultiplexerArg::Tmux),
            subcommand: None,
        };

        let cli = Cli {
            config: None,
            logging: logging_with_level(ah_logging::CliLogLevel::Debug),
            repo: None,
            fs_snapshots: None,
            experimental_features: None,
            command: Commands::Tui(tui_args),
        };

        let json = cli.to_json_overrides();

        // Should contain global options plus nested TUI config
        let expected = serde_json::json!({
            "log-level": "debug",
            "tui": {
                "remote-server": "https://ah.example.com",
                "api-key": "test-key",
                "multiplexer": "tmux"
            }
        });

        assert_eq!(json, expected);
    }

    #[test]
    #[serial]
    fn test_health_subcommand_overrides() {
        use crate::health::HealthArgs;
        use crate::{Cli, Commands};

        // Test Health subcommand with supported_agents option
        let health_args = HealthArgs {
            supported_agents: Some(vec!["codex".to_string(), "claude".to_string()]),
            json: false,
            quiet: false,
            with_credentials: false,
        };

        let cli = Cli {
            config: None,
            logging: logging_with_level(ah_logging::CliLogLevel::Info),
            repo: None,
            fs_snapshots: None,
            experimental_features: None,
            command: Commands::Health(health_args),
        };

        let json = cli.to_json_overrides();

        // Should contain global options plus nested health config
        let expected = serde_json::json!({
            "log-level": "info",
            "health": {
                "supported-agents": ["codex", "claude"]
            }
        });

        assert_eq!(json, expected);
    }

    #[test]
    #[serial]
    fn test_subcommand_with_no_config_options() {
        use crate::tui::TuiArgs;
        use crate::{Cli, Commands};

        // Test TUI subcommand with no configuration options provided
        let tui_args = TuiArgs {
            remote_server: None,
            api_key: None,
            bearer_token: None,
            multiplexer: None,
            subcommand: None,
        };

        let cli = Cli {
            config: None,
            logging: logging_with_level(ah_logging::CliLogLevel::Warn),
            repo: None,
            fs_snapshots: None,
            experimental_features: None,
            command: Commands::Tui(tui_args),
        };

        let json = cli.to_json_overrides();

        // Should only contain global options, no TUI section
        let expected = serde_json::json!({
            "log-level": "warn"
        });

        assert_eq!(json, expected);
    }
}
