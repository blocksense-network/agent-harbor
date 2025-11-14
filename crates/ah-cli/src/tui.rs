// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! TUI command handling for the CLI

#[allow(unused_imports)]
use ah_core::{
    CliMultiplexerType, LocalBranchesEnumerator, LocalRepositoriesEnumerator, MultiplexerChoice,
    RemoteBranchesEnumerator, RemoteRepositoriesEnumerator, RemoteWorkspaceFilesEnumerator,
    WorkspaceFilesEnumerator, determine_multiplexer_choice,
};
use ah_mux::detection;
use ah_mux_core::{Multiplexer, WindowOptions};
use ah_repo::VcsRepo;
use ah_rest_client::AuthConfig;
use ah_tui::dashboard_loop::run_dashboard;
use ah_tui::settings::Settings;
use ah_tui::view::TuiDependencies;
use ah_workflows::{WorkflowConfig, WorkflowProcessor, WorkspaceWorkflowsEnumerator};
use anyhow::Result;
use clap::{Args, Subcommand, ValueEnum};
use std::sync::Arc;

/// Multiplexer types supported by the CLI (wrapper around core type with Auto variant)
#[derive(Clone, Debug, PartialEq, ValueEnum)]
pub enum CliMultiplexerArg {
    /// Auto-detect the best available multiplexer
    Auto,
    /// tmux terminal multiplexer
    Tmux,
    /// kitty terminal emulator
    Kitty,
    /// iTerm2 terminal emulator (macOS)
    ITerm2,
    /// WezTerm terminal emulator
    Wezterm,
    /// zellij terminal multiplexer
    Zellij,
    /// GNU screen
    Screen,
    /// Tilix terminal emulator (Linux only)
    Tilix,
    /// Windows Terminal (Windows only)
    WindowsTerminal,
    /// Ghostty terminal emulator
    Ghostty,
    /// Neovim
    Neovim,
    /// Vim
    Vim,
    /// Emacs
    Emacs,
}

/// Filesystem snapshot provider type
#[derive(Clone, Debug, clap::ValueEnum)]
pub enum FsSnapshotsType {
    /// Auto-detect the best available snapshot provider
    Auto,
    /// ZFS filesystem snapshots
    Zfs,
    /// Btrfs filesystem snapshots
    Btrfs,
    /// AgentFS overlay filesystem
    Agentfs,
    /// Git shadow commits
    Git,
    /// Disable filesystem snapshots
    Disable,
}

impl Default for FsSnapshotsType {
    fn default() -> Self {
        Self::Auto
    }
}

impl CliMultiplexerArg {
    /// Get a human-readable display name for the multiplexer type
    pub fn display_name(&self) -> &'static str {
        match self {
            CliMultiplexerArg::Auto => "auto-detected",
            CliMultiplexerArg::Tmux => "Tmux",
            CliMultiplexerArg::Kitty => "Kitty",
            CliMultiplexerArg::ITerm2 => "iTerm2",
            CliMultiplexerArg::Wezterm => "WezTerm",
            CliMultiplexerArg::Zellij => "Zellij",
            CliMultiplexerArg::Screen => "GNU Screen",
            CliMultiplexerArg::Tilix => "Tilix",
            CliMultiplexerArg::WindowsTerminal => "Windows Terminal",
            CliMultiplexerArg::Ghostty => "Ghostty",
            CliMultiplexerArg::Neovim => "Neovim",
            CliMultiplexerArg::Vim => "Vim",
            CliMultiplexerArg::Emacs => "Emacs",
        }
    }

    /// Convert CLI argument to core multiplexer type
    /// Returns None for Auto variant as it needs runtime detection
    pub fn to_core_type(&self) -> Option<CliMultiplexerType> {
        match self {
            CliMultiplexerArg::Auto => None,
            CliMultiplexerArg::Tmux => Some(CliMultiplexerType::Tmux),
            CliMultiplexerArg::Kitty => Some(CliMultiplexerType::Kitty),
            CliMultiplexerArg::ITerm2 => Some(CliMultiplexerType::ITerm2),
            CliMultiplexerArg::Wezterm => Some(CliMultiplexerType::WezTerm),
            CliMultiplexerArg::Zellij => Some(CliMultiplexerType::Zellij),
            CliMultiplexerArg::Screen => Some(CliMultiplexerType::Screen),
            CliMultiplexerArg::Tilix => Some(CliMultiplexerType::Tilix),
            CliMultiplexerArg::WindowsTerminal => Some(CliMultiplexerType::WindowsTerminal),
            CliMultiplexerArg::Ghostty => Some(CliMultiplexerType::Ghostty),
            CliMultiplexerArg::Neovim => Some(CliMultiplexerType::Neovim),
            CliMultiplexerArg::Vim => Some(CliMultiplexerType::Vim),
            CliMultiplexerArg::Emacs => Some(CliMultiplexerType::Emacs),
        }
    }
}

/// Helper to get display name for core CliMultiplexerType
fn multiplexer_display_name(mux_type: &CliMultiplexerType) -> &'static str {
    mux_type.display_name()
}

/// Arguments for the TUI command
#[derive(Args)]
pub struct TuiArgs {
    /// Remote server URL for REST API connectivity
    #[arg(long, help = "URL of the remote agent-harbor REST service")]
    remote_server: Option<String>,

    /// API key for authentication with remote server
    #[arg(long, help = "API key for authenticating with the remote server")]
    api_key: Option<String>,

    /// Bearer token for authentication with remote server
    #[arg(
        long,
        help = "JWT bearer token for authenticating with the remote server"
    )]
    bearer_token: Option<String>,

    /// Which multiplexer to use
    #[arg(long, help = "Multiplexer to use for session management")]
    multiplexer: Option<CliMultiplexerArg>,

    #[command(subcommand)]
    pub subcommand: Option<TuiSubcommands>,
}

/// TUI subcommands
#[derive(Subcommand)]
pub enum TuiSubcommands {
    /// Launch the TUI dashboard directly (for use within multiplexer windows)
    Dashboard {
        /// Remote server URL for REST API connectivity
        #[arg(long, help = "URL of the remote agent-harbor REST service")]
        remote_server: Option<String>,

        /// API key for authentication with remote server
        #[arg(long, help = "API key for authenticating with the remote server")]
        api_key: Option<String>,

        /// Bearer token for authentication with remote server
        #[arg(
            long,
            help = "JWT bearer token for authenticating with the remote server"
        )]
        bearer_token: Option<String>,
    },
}

impl TuiArgs {
    /// Run the TUI command
    pub async fn run(self, fs_snapshots: FsSnapshotsType) -> Result<()> {
        match self.subcommand {
            Some(TuiSubcommands::Dashboard {
                ref remote_server,
                ref api_key,
                ref bearer_token,
            }) => {
                // Run dashboard directly
                let deps = Self::create_dashboard_dependencies(
                    remote_server.clone(),
                    api_key.clone(),
                    bearer_token.clone(),
                    None,
                    fs_snapshots,
                )?;
                run_dashboard(deps).await.map_err(|e| anyhow::anyhow!("TUI error: {}", e))
            }
            None => {
                // Main TUI command - handle multiplexer session management
                self.run_with_multiplexer(fs_snapshots).await
            }
        }
    }

    /// Run the main TUI command with multiplexer session management
    async fn run_with_multiplexer(&self, fs_snapshots: FsSnapshotsType) -> Result<()> {
        // Detect the current terminal environment stack
        let terminal_envs = detection::detect_terminal_environments();

        // Determine which multiplexer to use based on terminal environment detection
        let multiplexer_choice = determine_multiplexer_choice(&terminal_envs);

        match (&self.multiplexer, multiplexer_choice) {
            // Auto mode: detect terminal environment and choose appropriate action
            (
                Some(CliMultiplexerArg::Auto),
                MultiplexerChoice::InSupportedMultiplexer(multiplexer_type),
            )
            | (None, MultiplexerChoice::InSupportedMultiplexer(multiplexer_type)) => {
                tracing::info!(
                    multiplexer_type = ?multiplexer_type,
                    "Detected multiplexer environment, launching dashboard directly"
                );
                // Use the detected multiplexer for task management
                let multiplexer_type = Some(multiplexer_type.clone());
                let deps = Self::create_dashboard_dependencies(
                    self.remote_server.clone(),
                    self.api_key.clone(),
                    self.bearer_token.clone(),
                    multiplexer_type,
                    fs_snapshots,
                )?;
                run_dashboard(deps).await.map_err(|e| anyhow::anyhow!("TUI error: {}", e))
            }
            (Some(CliMultiplexerArg::Auto), MultiplexerChoice::InSupportedTerminal)
            | (None, MultiplexerChoice::InSupportedTerminal) => {
                tracing::info!(
                    "Detected supported terminal environment, launching dashboard directly"
                );
                let deps = Self::create_dashboard_dependencies(
                    self.remote_server.clone(),
                    self.api_key.clone(),
                    self.bearer_token.clone(),
                    self.multiplexer.as_ref().and_then(|m| m.to_core_type()),
                    fs_snapshots,
                )?;
                run_dashboard(deps).await.map_err(|e| anyhow::anyhow!("TUI error: {}", e))
            }
            (Some(CliMultiplexerArg::Auto), MultiplexerChoice::UnsupportedEnvironment)
            | (None, MultiplexerChoice::UnsupportedEnvironment) => {
                // Not in a supported environment, create a multiplexer session
                let multiplexer = self.create_multiplexer()?;
                self.create_and_enter_multiplexer_session(&*multiplexer, fs_snapshots).await
            }
            // Explicit multiplexer type specified - always create/manage session
            (Some(multiplexer_type), _) => {
                let multiplexer = self.create_multiplexer()?;
                self.create_and_enter_multiplexer_session(&*multiplexer, fs_snapshots).await
            }
        }
    }

    /// Create the appropriate multiplexer instance based on configuration
    fn create_multiplexer(&self) -> Result<Box<dyn Multiplexer>> {
        match self.multiplexer {
            Some(CliMultiplexerArg::Auto) | None => {
                // For auto/none, use the default multiplexer (which prioritizes tmux)
                Ok(Box::new(ah_mux::TmuxMultiplexer::default()))
            }
            Some(CliMultiplexerArg::Tmux) => Ok(Box::new(ah_mux::TmuxMultiplexer::default())),
            Some(CliMultiplexerArg::Kitty) => {
                // TODO: Implement KittyMultiplexer
                anyhow::bail!("Kitty multiplexer is not yet supported");
            }
            Some(CliMultiplexerArg::ITerm2) => Ok(Box::new(ah_mux::ITerm2Multiplexer::new()?)),
            Some(CliMultiplexerArg::Wezterm) => {
                // TODO: Implement WezTermMultiplexer
                anyhow::bail!("WezTerm multiplexer is not yet supported");
            }
            Some(CliMultiplexerArg::Zellij) => {
                // TODO: Implement ZellijMultiplexer
                anyhow::bail!("Zellij multiplexer is not yet supported");
            }
            Some(CliMultiplexerArg::Screen) => {
                // TODO: Implement ScreenMultiplexer
                anyhow::bail!("Screen multiplexer is not yet supported");
            }
            Some(CliMultiplexerArg::Tilix) => {
                // TODO: Implement TilixMultiplexer
                anyhow::bail!("Tilix multiplexer is not yet supported");
            }
            Some(CliMultiplexerArg::WindowsTerminal) => {
                // TODO: Implement WindowsTerminalMultiplexer
                anyhow::bail!("Windows Terminal multiplexer is not yet supported");
            }
            Some(CliMultiplexerArg::Ghostty) => {
                // TODO: Implement GhosttyMultiplexer
                anyhow::bail!("Ghostty multiplexer is not yet supported");
            }
            Some(CliMultiplexerArg::Neovim) => {
                // TODO: Implement NeovimMultiplexer
                anyhow::bail!("Neovim multiplexer is not yet supported");
            }
            Some(CliMultiplexerArg::Vim) => {
                // TODO: Implement VimMultiplexer
                anyhow::bail!("Vim multiplexer is not yet supported");
            }
            Some(CliMultiplexerArg::Emacs) => {
                // TODO: Implement EmacsMultiplexer
                anyhow::bail!("Emacs multiplexer is not yet supported");
            }
        }
    }

    /// Create a new multiplexer session and enter it
    async fn create_and_enter_multiplexer_session(
        &self,
        multiplexer: &dyn Multiplexer,
        fs_snapshots: FsSnapshotsType,
    ) -> Result<()> {
        if !multiplexer.is_available() {
            anyhow::bail!(
                "Multiplexer '{}' is not available on this system",
                multiplexer.id()
            );
        }

        tracing::info!(
            multiplexer_id = %multiplexer.id(),
            "Creating new multiplexer session for agent-harbor"
        );

        // Create a new window with the dashboard command
        let window_opts = WindowOptions {
            title: Some("agent-harbor"),
            cwd: Some(&std::env::current_dir()?),
            profile: None,
            focus: true,
        };

        let window_id = multiplexer.open_window(&window_opts)?;

        // Run the dashboard command in the new window
        let dashboard_cmd = self.build_dashboard_command();
        multiplexer.run_command(
            &format!("{}-1", window_id),
            &dashboard_cmd,
            &Default::default(),
        )?;

        // For now, we'll run the dashboard locally instead of trying to exec into multiplexer
        // TODO: Implement proper session attachment
        tracing::info!(
            multiplexer_id = %multiplexer.id(),
            window_id = %window_id,
            "Multiplexer session created, run attach command for full integration"
        );

        // For development, just run the dashboard directly
        let deps = Self::create_dashboard_dependencies(
            self.remote_server.clone(),
            self.api_key.clone(),
            self.bearer_token.clone(),
            self.multiplexer.as_ref().and_then(|m| m.to_core_type()),
            fs_snapshots,
        )?;
        run_dashboard(deps).await.map_err(|e| anyhow::anyhow!("TUI error: {}", e))
    }

    /// Build the command to run the dashboard
    fn build_dashboard_command(&self) -> String {
        let mut cmd = format!(
            "{} tui dashboard",
            std::env::current_exe().unwrap().display()
        );

        if let Some(ref remote_server) = self.remote_server {
            cmd.push_str(&format!(" --remote-server {}", remote_server));
        }

        if let Some(ref api_key) = self.api_key {
            cmd.push_str(&format!(" --api-key {}", api_key));
        }

        if let Some(ref bearer_token) = self.bearer_token {
            cmd.push_str(&format!(" --bearer-token {}", bearer_token));
        }

        if let Some(ref multiplexer) = self.multiplexer {
            let multiplexer_str = match multiplexer {
                CliMultiplexerArg::Auto => "auto",
                CliMultiplexerArg::Tmux => "tmux",
                CliMultiplexerArg::Zellij => "zellij",
                CliMultiplexerArg::Screen => "screen",
                CliMultiplexerArg::ITerm2 => "iterm2",
                CliMultiplexerArg::Kitty => "kitty",
                CliMultiplexerArg::Wezterm => "wezterm",
                CliMultiplexerArg::Tilix => "tilix",
                CliMultiplexerArg::WindowsTerminal => "windows-terminal",
                CliMultiplexerArg::Ghostty => "ghostty",
                CliMultiplexerArg::Neovim => "neovim",
                CliMultiplexerArg::Vim => "vim",
                CliMultiplexerArg::Emacs => "emacs",
            };
            cmd.push_str(&format!(" --multiplexer {}", multiplexer_str));
        }

        cmd
    }

    pub fn get_tui_dependencies(
        repo: Option<String>,
        remote_server: Option<String>,
        api_key: Option<String>,
        bearer_token: Option<String>,
        multiplexer: Option<CliMultiplexerType>,
        fs_snapshots: FsSnapshotsType,
    ) -> Result<TuiDependencies> {
        // Validate arguments
        if api_key.is_some() && bearer_token.is_some() {
            anyhow::bail!("Cannot specify both --api-key and --bearer-token");
        }

        if (api_key.is_some() || bearer_token.is_some()) && remote_server.is_none() {
            anyhow::bail!("--remote-server is required when using authentication");
        }

        // Get workspace directory for repository detection
        let workspace_dir = if let Some(repo_path) = repo {
            std::path::PathBuf::from(repo_path)
        } else {
            std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
        };

        // Create VcsRepo instance for repository detection
        let vcs_repo = VcsRepo::new(&workspace_dir).unwrap();

        // Detect current repository - use full path for proper validation
        let current_repository = Some(vcs_repo.root().to_string_lossy().to_string());

        // Create database manager for enumerators
        let db_manager =
            ah_core::DatabaseManager::new().expect("Failed to create database manager");

        // Register the detected repository in the database
        // Ignore errors - repository might already be registered
        let _ = db_manager.get_or_create_repo(&vcs_repo);

        // Create service dependencies based on remote server configuration
        let deps = if let Some(server_url) = remote_server {
            // Remote server mode
            tracing::info!(server_url = %server_url, "Connecting to remote server");

            // Create authentication config
            let auth = if let Some(api_key) = api_key {
                AuthConfig::with_api_key(api_key)
            } else if let Some(bearer_token) = bearer_token {
                AuthConfig::with_bearer(bearer_token)
            } else {
                AuthConfig::default()
            };

            // Create REST client
            let rest_client = ah_rest_client::RestClient::from_url(&server_url, auth)?;

            // Use RestTaskManager for remote mode
            let task_manager: Arc<dyn ah_core::TaskManager> = Arc::new(
                ah_core::rest_task_manager::GenericRestTaskManager::new(rest_client.clone()),
            );

            // Use RemoteWorkspaceFilesEnumerator for remote mode
            let workspace_files: Arc<dyn WorkspaceFilesEnumerator> =
                Arc::new(RemoteWorkspaceFilesEnumerator::new(
                    rest_client.clone(),
                    current_repository.clone().unwrap_or_else(|| "unknown".to_string()),
                ));
            let workspace_workflows: Arc<dyn WorkspaceWorkflowsEnumerator> =
                Arc::new(WorkflowProcessor::new(WorkflowConfig::default()));

            // Create remote enumerators for remote mode
            let repositories_enumerator: Arc<dyn ah_core::RepositoriesEnumerator> = Arc::new(
                ah_core::RemoteRepositoriesEnumerator::new(rest_client.clone(), server_url.clone()),
            );
            let branches_enumerator: Arc<dyn ah_core::BranchesEnumerator> = Arc::new(
                ah_core::RemoteBranchesEnumerator::new(rest_client.clone(), server_url.clone()),
            );
            Ok(TuiDependencies {
                workspace_files,
                workspace_workflows,
                task_manager,
                repositories_enumerator,
                branches_enumerator,
                settings: Settings::default(),
                current_repository,
            })
        } else {
            // Local mode
            tracing::info!("Running in local mode");

            // Create local service dependencies
            let workspace_dir =
                std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

            // Detect current repository and ensure it's in the database
            let detected_repo_result = VcsRepo::new(&workspace_dir);

            let workspace_files: Arc<dyn WorkspaceFilesEnumerator> =
                Arc::new(match detected_repo_result {
                    Ok(vcs_repo) => vcs_repo,
                    Err(_) => {
                        // If not a git repository, we could return an error or use a mock
                        // For now, let's just panic since the dashboard expects a valid repo
                        panic!("Current directory is not a git repository");
                    }
                });
            let config = WorkflowConfig::default();
            let workspace_workflows: Arc<dyn WorkspaceWorkflowsEnumerator> = Arc::new(
                WorkflowProcessor::for_repo(config, &workspace_dir)
                    .unwrap_or_else(|_| WorkflowProcessor::new(WorkflowConfig::default())),
            );

            // Use shared task manager initialization
            let recording_disabled = matches!(fs_snapshots, FsSnapshotsType::Disable);
            let multiplexer_preference = match multiplexer {
                Some(CliMultiplexerType::ITerm2) => ah_core::MultiplexerPreference::ITerm2,
                Some(CliMultiplexerType::Tmux) => ah_core::MultiplexerPreference::Tmux,
                None => ah_core::MultiplexerPreference::Auto,
                // For unsupported multiplexers, fall back to auto-detection
                Some(unsupported) => {
                    tracing::warn!(
                        multiplexer_type = ?unsupported,
                        "Multiplexer not yet supported for task manager, using auto-detection"
                    );
                    ah_core::MultiplexerPreference::Auto
                }
            };

            let task_manager = ah_core::create_local_task_manager_with_multiplexer(
                ah_core::TaskManagerConfig {
                    recording_disabled,
                    config_file: None, // Use default configuration
                },
                multiplexer_preference,
            )
            .expect("Failed to create local task manager");

            // Create local enumerators for local mode
            let repositories_enumerator: Arc<dyn ah_core::RepositoriesEnumerator> = Arc::new(
                ah_core::LocalRepositoriesEnumerator::new(db_manager.clone()),
            );
            let branches_enumerator: Arc<dyn ah_core::BranchesEnumerator> =
                Arc::new(ah_core::LocalBranchesEnumerator::new(db_manager.clone()));

            Ok(TuiDependencies {
                workspace_files,
                workspace_workflows,
                task_manager,
                repositories_enumerator,
                branches_enumerator,
                settings: Settings::default(),
                current_repository,
            })
        };
        deps
    }

    /// Create TUI dependencies for the dashboard
    fn create_dashboard_dependencies(
        remote_server: Option<String>,
        api_key: Option<String>,
        bearer_token: Option<String>,
        multiplexer: Option<CliMultiplexerType>,
        fs_snapshots: FsSnapshotsType,
    ) -> Result<TuiDependencies> {
        Self::get_tui_dependencies(
            None,
            remote_server,
            api_key,
            bearer_token,
            multiplexer,
            fs_snapshots,
        )
    }
}
