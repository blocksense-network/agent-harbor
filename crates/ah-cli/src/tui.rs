// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! TUI command handling for the CLI

use ah_core::{
    AgentExecutionConfig, WorkspaceFilesEnumerator, local_task_manager::GenericLocalTaskManager,
};
use ah_mux::{
    TmuxMultiplexer,
    detection::{self, TerminalEnvironment},
};
use ah_mux_core::{Multiplexer, WindowOptions};
use ah_repo::VcsRepo;
use ah_rest_client::AuthConfig;
use ah_tui::dashboard_loop::{DashboardDependencies, run_dashboard};
use ah_tui::settings::Settings;
use ah_workflows::{WorkflowConfig, WorkflowProcessor, WorkspaceWorkflowsEnumerator};
use anyhow::Result;
use clap::{Args, Subcommand, ValueEnum};
use std::{fs::OpenOptions, sync::Arc};

/// Multiplexer types supported by the CLI
#[derive(Clone, Debug, PartialEq, ValueEnum)]
pub enum CliMultiplexerType {
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

impl CliMultiplexerType {
    /// Get a human-readable display name for the multiplexer type
    pub fn display_name(&self) -> &'static str {
        match self {
            CliMultiplexerType::Auto => "auto-detected",
            CliMultiplexerType::Tmux => "Tmux",
            CliMultiplexerType::Kitty => "Kitty",
            CliMultiplexerType::ITerm2 => "iTerm2",
            CliMultiplexerType::Wezterm => "WezTerm",
            CliMultiplexerType::Zellij => "Zellij",
            CliMultiplexerType::Screen => "GNU Screen",
            CliMultiplexerType::Tilix => "Tilix",
            CliMultiplexerType::WindowsTerminal => "Windows Terminal",
            CliMultiplexerType::Ghostty => "Ghostty",
            CliMultiplexerType::Neovim => "Neovim",
            CliMultiplexerType::Vim => "Vim",
            CliMultiplexerType::Emacs => "Emacs",
        }
    }
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
    multiplexer: Option<CliMultiplexerType>,

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

/// Result of terminal environment analysis for multiplexer choice
#[derive(Debug, Clone)]
enum MultiplexerChoice {
    /// Currently running inside a supported multiplexer (use inner-most one)
    InSupportedMultiplexer(CliMultiplexerType),
    /// Currently running in a supported terminal but no multiplexer
    InSupportedTerminal,
    /// Not in any supported terminal/multiplexer environment
    UnsupportedEnvironment,
}

impl TuiArgs {
    /// Run the TUI command
    pub async fn run(self) -> Result<()> {
        match self.subcommand {
            Some(TuiSubcommands::Dashboard {
                remote_server,
                api_key,
                bearer_token,
            }) => {
                // Run dashboard directly
                Self::run_dashboard(remote_server, api_key, bearer_token, None).await
            }
            None => {
                // Main TUI command - handle multiplexer session management
                self.run_with_multiplexer().await
            }
        }
    }

    /// Determine the multiplexer choice based on detected terminal environments
    fn determine_multiplexer_choice(
        &self,
        terminal_envs: &[TerminalEnvironment],
    ) -> Result<MultiplexerChoice> {
        // Check for supported multiplexers (inner-most first, as per spec)
        // terminal_envs is in wrapping order (outermost to innermost), so rev() gives us innermost first
        for env in terminal_envs.iter().rev() {
            match env {
                TerminalEnvironment::Tmux => {
                    // Tmux is supported
                    return Ok(MultiplexerChoice::InSupportedMultiplexer(
                        CliMultiplexerType::Tmux,
                    ));
                }
                #[cfg(target_os = "macos")]
                TerminalEnvironment::ITerm2 => {
                    // iTerm2 is a terminal emulator, not a multiplexer, but we can run dashboard directly
                    return Ok(MultiplexerChoice::InSupportedTerminal);
                }
                // Other supported terminal environments - we can run dashboard directly
                TerminalEnvironment::Kitty
                | TerminalEnvironment::WezTerm
                | TerminalEnvironment::Tilix
                | TerminalEnvironment::WindowsTerminal
                | TerminalEnvironment::Ghostty => {
                    return Ok(MultiplexerChoice::InSupportedTerminal);
                }
                // Other multiplexers/editors - not supported yet, continue checking
                TerminalEnvironment::Zellij
                | TerminalEnvironment::Screen
                | TerminalEnvironment::Neovim
                | TerminalEnvironment::Vim
                | TerminalEnvironment::Emacs => {} // Continue checking
                // Catch any other variants (e.g., platform-specific ones)
                #[allow(unreachable_patterns)]
                _ => {} // Continue checking
            }
        }

        // No supported environments detected
        Ok(MultiplexerChoice::UnsupportedEnvironment)
    }

    /// Run the main TUI command with multiplexer session management
    async fn run_with_multiplexer(self) -> Result<()> {
        // Detect the current terminal environment stack
        let terminal_envs = detection::detect_terminal_environments();

        // Determine which multiplexer to use based on terminal environment detection
        let multiplexer_choice = self.determine_multiplexer_choice(&terminal_envs)?;

        match (&self.multiplexer, multiplexer_choice) {
            // Auto mode: detect terminal environment and choose appropriate action
            (
                Some(CliMultiplexerType::Auto),
                MultiplexerChoice::InSupportedMultiplexer(multiplexer_type),
            )
            | (None, MultiplexerChoice::InSupportedMultiplexer(multiplexer_type)) => {
                println!(
                    "Detected {} multiplexer environment, launching dashboard directly...",
                    multiplexer_type.display_name()
                );
                // Use the detected multiplexer for task management
                let multiplexer_type = Some(multiplexer_type.clone());
                Self::run_dashboard(
                    self.remote_server,
                    self.api_key,
                    self.bearer_token,
                    multiplexer_type,
                )
                .await
            }
            (Some(CliMultiplexerType::Auto), MultiplexerChoice::InSupportedTerminal)
            | (None, MultiplexerChoice::InSupportedTerminal) => {
                println!(
                    "Detected supported terminal environment, launching dashboard directly..."
                );
                Self::run_dashboard(
                    self.remote_server,
                    self.api_key,
                    self.bearer_token,
                    self.multiplexer.clone(),
                )
                .await
            }
            (Some(CliMultiplexerType::Auto), MultiplexerChoice::UnsupportedEnvironment)
            | (None, MultiplexerChoice::UnsupportedEnvironment) => {
                // Not in a supported environment, create a multiplexer session
                let multiplexer = self.create_multiplexer()?;
                self.create_and_enter_multiplexer_session(&*multiplexer).await
            }
            // Specific multiplexer requested - always create/manage session
            (Some(_), _) => {
                let multiplexer = self.create_multiplexer()?;
                self.create_and_enter_multiplexer_session(&*multiplexer).await
            }
        }
    }

    /// Create the appropriate multiplexer instance based on configuration
    fn create_multiplexer(&self) -> Result<Box<dyn Multiplexer>> {
        match self.multiplexer {
            Some(CliMultiplexerType::Auto) | None => {
                // For auto/none, use the default multiplexer (which prioritizes tmux)
                Ok(Box::new(TmuxMultiplexer::default()))
            }
            Some(CliMultiplexerType::Tmux) => Ok(Box::new(TmuxMultiplexer::default())),
            Some(CliMultiplexerType::Kitty) => {
                // TODO: Implement KittyMultiplexer
                anyhow::bail!("Kitty multiplexer is not yet supported");
            }
            Some(CliMultiplexerType::ITerm2) => Ok(Box::new(ah_mux::ITerm2Multiplexer::new()?)),
            Some(CliMultiplexerType::Wezterm) => {
                // TODO: Implement WezTermMultiplexer
                anyhow::bail!("WezTerm multiplexer is not yet supported");
            }
            Some(CliMultiplexerType::Zellij) => {
                // TODO: Implement ZellijMultiplexer
                anyhow::bail!("Zellij multiplexer is not yet supported");
            }
            Some(CliMultiplexerType::Screen) => {
                // TODO: Implement ScreenMultiplexer
                anyhow::bail!("Screen multiplexer is not yet supported");
            }
            Some(CliMultiplexerType::Tilix) => {
                // TODO: Implement TilixMultiplexer
                anyhow::bail!("Tilix multiplexer is not yet supported");
            }
            Some(CliMultiplexerType::WindowsTerminal) => {
                // TODO: Implement WindowsTerminalMultiplexer
                anyhow::bail!("Windows Terminal multiplexer is not yet supported");
            }
            Some(CliMultiplexerType::Ghostty) => {
                // TODO: Implement GhosttyMultiplexer
                anyhow::bail!("Ghostty multiplexer is not yet supported");
            }
            Some(CliMultiplexerType::Neovim) => {
                // TODO: Implement NeovimMultiplexer
                anyhow::bail!("Neovim multiplexer is not yet supported");
            }
            Some(CliMultiplexerType::Vim) => {
                // TODO: Implement VimMultiplexer
                anyhow::bail!("Vim multiplexer is not yet supported");
            }
            Some(CliMultiplexerType::Emacs) => {
                // TODO: Implement EmacsMultiplexer
                anyhow::bail!("Emacs multiplexer is not yet supported");
            }
        }
    }

    /// Detect if we're currently running inside a multiplexer
    fn detect_if_in_multiplexer(&self) -> bool {
        detection::is_in_multiplexer()
    }

    /// Create a new multiplexer session and enter it
    async fn create_and_enter_multiplexer_session(
        &self,
        multiplexer: &dyn Multiplexer,
    ) -> Result<()> {
        if !multiplexer.is_available() {
            anyhow::bail!(
                "Multiplexer '{}' is not available on this system",
                multiplexer.id()
            );
        }

        println!(
            "Creating new {} session for agent-harbor...",
            multiplexer.id()
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
        println!("Note: Multiplexer session created. For full multiplexer integration, run:");
        println!("  {} attach -t {}", multiplexer.id(), window_id);

        // For development, just run the dashboard directly
        Self::run_dashboard(
            self.remote_server.clone(),
            self.api_key.clone(),
            self.bearer_token.clone(),
            self.multiplexer.clone(),
        )
        .await
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

        cmd
    }

    /// Run the TUI dashboard
    async fn run_dashboard(
        remote_server: Option<String>,
        api_key: Option<String>,
        bearer_token: Option<String>,
        multiplexer: Option<CliMultiplexerType>,
    ) -> Result<()> {
        // Validate arguments
        if api_key.is_some() && bearer_token.is_some() {
            anyhow::bail!("Cannot specify both --api-key and --bearer-token");
        }

        if (api_key.is_some() || bearer_token.is_some()) && remote_server.is_none() {
            anyhow::bail!("--remote-server is required when using authentication");
        }

        // Create service dependencies based on remote server configuration
        let deps = if let Some(server_url) = remote_server {
            // Remote server mode
            println!("Connecting to remote server: {}", server_url);

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
                ah_core::rest_task_manager::GenericRestTaskManager::new(rest_client),
            );

            // For remote mode, we need remote workspace files and workflows
            // TODO: Implement remote WorkspaceFiles and WorkspaceWorkflowsEnumerator
            // For now, we'll use local implementations
            let workspace_files: Arc<dyn WorkspaceFilesEnumerator> =
                Arc::new(VcsRepo::new(&std::path::Path::new(".").to_path_buf()).unwrap());
            let workspace_workflows: Arc<dyn WorkspaceWorkflowsEnumerator> =
                Arc::new(WorkflowProcessor::new(WorkflowConfig::default()));

            DashboardDependencies {
                workspace_files,
                workspace_workflows,
                task_manager,
                settings: Settings::default(),
            }
        } else {
            // Local mode
            println!("Running in local mode");

            // Create local service dependencies
            let workspace_dir =
                std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let workspace_files: Arc<dyn WorkspaceFilesEnumerator> =
                match VcsRepo::new(&workspace_dir) {
                    Ok(vcs_repo) => Arc::new(vcs_repo),
                    Err(_) => {
                        // If not a git repository, we could return an error or use a mock
                        // For now, let's just panic since the dashboard expects a valid repo
                        panic!("Current directory is not a git repository");
                    }
                };
            let config = WorkflowConfig::default();
            let workspace_workflows: Arc<dyn WorkspaceWorkflowsEnumerator> = Arc::new(
                WorkflowProcessor::for_repo(config, &workspace_dir)
                    .unwrap_or_else(|_| WorkflowProcessor::new(WorkflowConfig::default())),
            );

            // Use LocalTaskManager for local mode
            let agent_config = AgentExecutionConfig {
                config_file: None, // Use default configuration
            };

            // For now, always use TmuxMultiplexer for the task manager
            // TODO: Support other multiplexers in task manager when they're implemented
            if let Some(multiplexer_type) = multiplexer {
                if !matches!(
                    multiplexer_type,
                    CliMultiplexerType::Tmux
                        | CliMultiplexerType::Auto
                        | CliMultiplexerType::ITerm2
                ) {
                    eprintln!(
                        "Warning: Multiplexer {:?} not yet supported for task manager, using tmux",
                        multiplexer_type
                    );
                }
            }

            let task_manager: Arc<dyn ah_core::TaskManager> = Arc::new(
                GenericLocalTaskManager::new(agent_config, TmuxMultiplexer::default())
                    .expect("Failed to create local task manager"),
            );

            DashboardDependencies {
                workspace_files,
                workspace_workflows,
                task_manager,
                settings: Settings::default(),
            }
        };

        // Run the dashboard (handles its own signal/panic handling)
        run_dashboard(deps).await.map_err(|e| anyhow::anyhow!("TUI error: {}", e))
    }
}
