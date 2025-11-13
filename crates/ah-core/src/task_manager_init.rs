// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Task manager initialization utilities
//!
//! This module provides shared functionality for initializing task managers
//! across different parts of the application (dashboard, session viewer, CLI, etc.)

use crate::TaskManager;
use crate::agent_executor::AgentExecutionConfig;
use crate::local_task_manager::GenericLocalTaskManager;
#[cfg(feature = "emacs")]
use ah_mux::EmacsMultiplexer;
#[cfg(feature = "ghostty")]
use ah_mux::GhosttyMultiplexer;
#[cfg(feature = "kitty")]
use ah_mux::KittyMultiplexer;
#[cfg(feature = "neovim")]
use ah_mux::NeovimMultiplexer;
#[cfg(feature = "screen")]
use ah_mux::ScreenMultiplexer;
#[cfg(all(feature = "tilix", target_os = "linux"))]
use ah_mux::TilixMultiplexer;
#[cfg(feature = "vim")]
use ah_mux::VimMultiplexer;
#[cfg(feature = "wezterm")]
use ah_mux::WezTermMultiplexer;
#[cfg(feature = "windows-terminal")]
use ah_mux::WindowsTerminalMultiplexer;
#[cfg(feature = "zellij")]
use ah_mux::ZellijMultiplexer;
use ah_mux::{ITerm2Multiplexer, TmuxMultiplexer, detection::TerminalEnvironment};
use ah_mux_core::Multiplexer;
use std::sync::Arc;

/// Configuration for task manager initialization
#[derive(Debug, Clone)]
pub struct TaskManagerConfig {
    /// Whether recording is disabled
    pub recording_disabled: bool,
    /// Optional path to agent configuration file
    pub config_file: Option<String>,
}

/// Result of terminal environment analysis for multiplexer choice
#[derive(Debug, Clone)]
pub enum MultiplexerChoice {
    /// Currently running inside a supported multiplexer (use inner-most one)
    InSupportedMultiplexer(CliMultiplexerType),
    /// Currently running in a supported terminal but no multiplexer
    InSupportedTerminal,
    /// Not in any supported terminal/multiplexer environment
    UnsupportedEnvironment,
}

/// Multiplexer types supported
#[derive(Debug, Clone)]
pub enum CliMultiplexerType {
    /// Use Tmux multiplexer
    Tmux,
    /// Use Kitty multiplexer
    Kitty,
    /// Use WezTerm multiplexer
    WezTerm,
    /// Use Zellij multiplexer
    Zellij,
    /// Use Screen multiplexer
    Screen,
    /// Use Tilix multiplexer
    Tilix,
    /// Use Windows Terminal multiplexer
    WindowsTerminal,
    /// Use Ghostty multiplexer
    Ghostty,
    /// Use Vim multiplexer
    Vim,
    /// Use Neovim multiplexer
    Neovim,
    /// Use Emacs multiplexer
    Emacs,
    /// Use iTerm2 multiplexer
    ITerm2,
}

impl CliMultiplexerType {
    /// Get a human-readable display name for the multiplexer type
    pub fn display_name(&self) -> &'static str {
        match self {
            CliMultiplexerType::Tmux => "Tmux",
            CliMultiplexerType::Kitty => "Kitty",
            CliMultiplexerType::ITerm2 => "iTerm2",
            CliMultiplexerType::WezTerm => "WezTerm",
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

/// Determine multiplexer choice based on terminal environments
pub fn determine_multiplexer_choice(terminal_envs: &[TerminalEnvironment]) -> MultiplexerChoice {
    // Check for supported multiplexers (inner-most first, as per spec)
    // terminal_envs is in wrapping order (outermost to innermost), so rev() gives us innermost first
    for env in terminal_envs.iter().rev() {
        match env {
            // True multiplexers - these manage multiple panes/windows
            TerminalEnvironment::Tmux => {
                return MultiplexerChoice::InSupportedMultiplexer(CliMultiplexerType::Tmux);
            }
            #[cfg(feature = "zellij")]
            TerminalEnvironment::Zellij => {
                return MultiplexerChoice::InSupportedMultiplexer(CliMultiplexerType::Zellij);
            }
            #[cfg(feature = "screen")]
            TerminalEnvironment::Screen => {
                return MultiplexerChoice::InSupportedMultiplexer(CliMultiplexerType::Screen);
            }

            // Terminal emulators that can act as multiplexers
            #[cfg(feature = "kitty")]
            TerminalEnvironment::Kitty => {
                return MultiplexerChoice::InSupportedMultiplexer(CliMultiplexerType::Kitty);
            }
            #[cfg(feature = "wezterm")]
            TerminalEnvironment::WezTerm => {
                return MultiplexerChoice::InSupportedMultiplexer(CliMultiplexerType::WezTerm);
            }
            #[cfg(target_os = "macos")]
            TerminalEnvironment::ITerm2 => {
                return MultiplexerChoice::InSupportedMultiplexer(CliMultiplexerType::ITerm2);
            }
            #[cfg(all(feature = "tilix", target_os = "linux"))]
            TerminalEnvironment::Tilix => {
                return MultiplexerChoice::InSupportedMultiplexer(CliMultiplexerType::Tilix);
            }
            #[cfg(feature = "windows-terminal")]
            TerminalEnvironment::WindowsTerminal => {
                return MultiplexerChoice::InSupportedMultiplexer(
                    CliMultiplexerType::WindowsTerminal,
                );
            }
            #[cfg(feature = "ghostty")]
            TerminalEnvironment::Ghostty => {
                return MultiplexerChoice::InSupportedMultiplexer(CliMultiplexerType::Ghostty);
            }

            // Editors with terminal support
            #[cfg(feature = "vim")]
            TerminalEnvironment::Vim => {
                return MultiplexerChoice::InSupportedMultiplexer(CliMultiplexerType::Vim);
            }
            #[cfg(feature = "neovim")]
            TerminalEnvironment::Neovim => {
                return MultiplexerChoice::InSupportedMultiplexer(CliMultiplexerType::Neovim);
            }
            #[cfg(feature = "emacs")]
            TerminalEnvironment::Emacs => {
                return MultiplexerChoice::InSupportedMultiplexer(CliMultiplexerType::Emacs);
            }

            // Unsupported environments - continue checking for nested supported ones
            #[cfg(not(feature = "zellij"))]
            TerminalEnvironment::Zellij => {} // Continue checking
            #[cfg(not(feature = "screen"))]
            TerminalEnvironment::Screen => {} // Continue checking
            #[cfg(not(feature = "kitty"))]
            TerminalEnvironment::Kitty => {} // Continue checking
            #[cfg(not(feature = "wezterm"))]
            TerminalEnvironment::WezTerm => {} // Continue checking
            #[cfg(not(target_os = "macos"))]
            TerminalEnvironment::ITerm2 => {} // Continue checking
            #[cfg(not(all(feature = "tilix", target_os = "linux")))]
            TerminalEnvironment::Tilix => {} // Continue checking
            #[cfg(not(feature = "windows-terminal"))]
            TerminalEnvironment::WindowsTerminal => {} // Continue checking
            #[cfg(not(feature = "ghostty"))]
            TerminalEnvironment::Ghostty => {} // Continue checking
            #[cfg(not(feature = "vim"))]
            TerminalEnvironment::Vim => {} // Continue checking
            #[cfg(not(feature = "neovim"))]
            TerminalEnvironment::Neovim => {} // Continue checking
            #[cfg(not(feature = "emacs"))]
            TerminalEnvironment::Emacs => {} // Continue checking
        }
    }

    // No supported environments detected
    MultiplexerChoice::UnsupportedEnvironment
}

/// Create a local task manager with the given configuration using auto-detected multiplexer
pub fn create_local_task_manager(
    config: TaskManagerConfig,
) -> Result<Arc<dyn TaskManager>, String> {
    create_local_task_manager_with_multiplexer(config, MultiplexerPreference::Auto)
}

/// Create a task manager for dashboard use (recording enabled by default)
pub fn create_dashboard_task_manager() -> Result<Arc<dyn TaskManager>, String> {
    create_local_task_manager(TaskManagerConfig {
        recording_disabled: false,
        config_file: None,
    })
}

/// Create a task manager for session viewer use (recording enabled by default)
pub fn create_session_viewer_task_manager() -> Result<Arc<dyn TaskManager>, String> {
    create_local_task_manager(TaskManagerConfig {
        recording_disabled: false,
        config_file: None,
    })
}

/// Explicit multiplexer preference for task manager creation
#[derive(Debug, Clone)]
pub enum MultiplexerPreference {
    /// Auto-detect the best multiplexer based on environment
    Auto,
    /// Use tmux multiplexer
    Tmux,
    /// Use iTerm2 multiplexer (macOS only)
    ITerm2,
    /// Use kitty multiplexer
    Kitty,
    /// Use WezTerm multiplexer
    WezTerm,
    /// Use zellij multiplexer
    Zellij,
    /// Use screen multiplexer
    Screen,
    /// Use tilix multiplexer (Linux only)
    Tilix,
    /// Use Windows Terminal multiplexer (Windows only)
    WindowsTerminal,
    /// Use ghostty multiplexer
    Ghostty,
    /// Use vim multiplexer
    Vim,
    /// Use neovim multiplexer
    Neovim,
    /// Use emacs multiplexer
    Emacs,
}

/// Detect the appropriate multiplexer for the current environment
#[allow(dead_code)]
fn detect_multiplexer() -> Result<Box<dyn Multiplexer + Send + Sync>, String> {
    // Detect the current terminal environment stack
    let terminal_envs = ah_mux::detection::detect_terminal_environments();

    // Determine which multiplexer to use based on terminal environment detection
    let choice = determine_multiplexer_choice(&terminal_envs);

    match choice {
        MultiplexerChoice::InSupportedMultiplexer(mux_type) => {
            match mux_type {
                CliMultiplexerType::Tmux => Ok(Box::new(TmuxMultiplexer::default())),
                #[cfg(target_os = "macos")]
                CliMultiplexerType::ITerm2 => ITerm2Multiplexer::new()
                    .map(|m| Box::new(m) as Box<dyn Multiplexer + Send + Sync>)
                    .map_err(|e| format!("Failed to create iTerm2 multiplexer: {}", e)),
                #[cfg(feature = "kitty")]
                CliMultiplexerType::Kitty => KittyMultiplexer::new()
                    .map(|m| Box::new(m) as Box<dyn Multiplexer + Send + Sync>)
                    .map_err(|e| format!("Failed to create Kitty multiplexer: {}", e)),
                #[cfg(feature = "wezterm")]
                CliMultiplexerType::WezTerm => WezTermMultiplexer::new()
                    .map(|m| Box::new(m) as Box<dyn Multiplexer + Send + Sync>)
                    .map_err(|e| format!("Failed to create WezTerm multiplexer: {}", e)),
                #[cfg(feature = "zellij")]
                CliMultiplexerType::Zellij => ZellijMultiplexer::new()
                    .map(|m| Box::new(m) as Box<dyn Multiplexer + Send + Sync>)
                    .map_err(|e| format!("Failed to create Zellij multiplexer: {}", e)),
                #[cfg(feature = "screen")]
                CliMultiplexerType::Screen => ScreenMultiplexer::new()
                    .map(|m| Box::new(m) as Box<dyn Multiplexer + Send + Sync>)
                    .map_err(|e| format!("Failed to create Screen multiplexer: {}", e)),
                #[cfg(all(feature = "tilix", target_os = "linux"))]
                CliMultiplexerType::Tilix => TilixMultiplexer::new()
                    .map(|m| Box::new(m) as Box<dyn Multiplexer + Send + Sync>)
                    .map_err(|e| format!("Failed to create Tilix multiplexer: {}", e)),
                #[cfg(feature = "windows-terminal")]
                CliMultiplexerType::WindowsTerminal => WindowsTerminalMultiplexer::new()
                    .map(|m| Box::new(m) as Box<dyn Multiplexer + Send + Sync>)
                    .map_err(|e| format!("Failed to create Windows Terminal multiplexer: {}", e)),
                #[cfg(feature = "ghostty")]
                CliMultiplexerType::Ghostty => GhosttyMultiplexer::new()
                    .map(|m| Box::new(m) as Box<dyn Multiplexer + Send + Sync>)
                    .map_err(|e| format!("Failed to create Ghostty multiplexer: {}", e)),
                #[cfg(feature = "vim")]
                CliMultiplexerType::Vim => VimMultiplexer::new()
                    .map(|m| Box::new(m) as Box<dyn Multiplexer + Send + Sync>)
                    .map_err(|e| format!("Failed to create Vim multiplexer: {}", e)),
                #[cfg(feature = "neovim")]
                CliMultiplexerType::Neovim => NeovimMultiplexer::new()
                    .map(|m| Box::new(m) as Box<dyn Multiplexer + Send + Sync>)
                    .map_err(|e| format!("Failed to create Neovim multiplexer: {}", e)),
                #[cfg(feature = "emacs")]
                CliMultiplexerType::Emacs => EmacsMultiplexer::new()
                    .map(|m| Box::new(m) as Box<dyn Multiplexer + Send + Sync>)
                    .map_err(|e| format!("Failed to create Emacs multiplexer: {}", e)),
                // Fallback cases when features are disabled
                #[cfg(not(target_os = "macos"))]
                CliMultiplexerType::ITerm2 => Err("iTerm2 is only available on macOS".to_string()),
                #[cfg(not(feature = "kitty"))]
                CliMultiplexerType::Kitty => Err("Kitty feature is not enabled".to_string()),
                #[cfg(not(feature = "wezterm"))]
                CliMultiplexerType::WezTerm => Err("WezTerm feature is not enabled".to_string()),
                #[cfg(not(feature = "zellij"))]
                CliMultiplexerType::Zellij => Err("Zellij feature is not enabled".to_string()),
                #[cfg(not(feature = "screen"))]
                CliMultiplexerType::Screen => Err("Screen feature is not enabled".to_string()),
                #[cfg(not(all(feature = "tilix", target_os = "linux")))]
                CliMultiplexerType::Tilix => {
                    Err("Tilix is only available on Linux with tilix feature".to_string())
                }
                #[cfg(not(feature = "windows-terminal"))]
                CliMultiplexerType::WindowsTerminal => {
                    Err("Windows Terminal feature is not enabled".to_string())
                }
                #[cfg(not(feature = "ghostty"))]
                CliMultiplexerType::Ghostty => Err("Ghostty feature is not enabled".to_string()),
                #[cfg(not(feature = "vim"))]
                CliMultiplexerType::Vim => Err("Vim feature is not enabled".to_string()),
                #[cfg(not(feature = "neovim"))]
                CliMultiplexerType::Neovim => Err("Neovim feature is not enabled".to_string()),
                #[cfg(not(feature = "emacs"))]
                CliMultiplexerType::Emacs => Err("Emacs feature is not enabled".to_string()),
            }
        }
        MultiplexerChoice::InSupportedTerminal => {
            // This should not happen anymore, but keep for compatibility
            // Fall back to tmux
            Ok(Box::new(TmuxMultiplexer::default()))
        }
        MultiplexerChoice::UnsupportedEnvironment => {
            // Unsupported environment, default to tmux
            Ok(Box::new(TmuxMultiplexer::default()))
        }
    }
}

/// Create a local task manager with explicit multiplexer preference
pub fn create_local_task_manager_with_multiplexer(
    config: TaskManagerConfig,
    multiplexer_preference: MultiplexerPreference,
) -> Result<Arc<dyn TaskManager>, String> {
    let agent_config = AgentExecutionConfig {
        config_file: config.config_file,
        recording_disabled: config.recording_disabled,
    };

    match multiplexer_preference {
        MultiplexerPreference::Auto => {
            // Use automatic detection - create concrete instances
            let terminal_envs = ah_mux::detection::detect_terminal_environments();
            let choice = determine_multiplexer_choice(&terminal_envs);

            match choice {
                MultiplexerChoice::InSupportedMultiplexer(mux_type) => {
                    match mux_type {
                        CliMultiplexerType::Tmux => {
                            GenericLocalTaskManager::new(agent_config, TmuxMultiplexer::default())
                                .map(|tm| Arc::new(tm) as Arc<dyn TaskManager>)
                                .map_err(|e| format!("Failed to create local task manager with Tmux: {}", e))
                        }
                        #[cfg(target_os = "macos")]
                        CliMultiplexerType::ITerm2 => {
                            ITerm2Multiplexer::new()
                                .map_err(|e| format!("iTerm2 multiplexer is not available: {}", e))
                                .and_then(|mux| {
                                    GenericLocalTaskManager::new(agent_config, mux)
                                        .map(|tm| Arc::new(tm) as Arc<dyn TaskManager>)
                                        .map_err(|e| format!("Failed to create local task manager with iTerm2: {}", e))
                                })
                        }
                        #[cfg(feature = "kitty")]
                        CliMultiplexerType::Kitty => {
                            KittyMultiplexer::new()
                                .map_err(|e| format!("Kitty multiplexer is not available: {}", e))
                                .and_then(|mux| {
                                    GenericLocalTaskManager::new(agent_config, mux)
                                        .map(|tm| Arc::new(tm) as Arc<dyn TaskManager>)
                                        .map_err(|e| format!("Failed to create local task manager with Kitty: {}", e))
                                })
                        }
                        #[cfg(feature = "wezterm")]
                        CliMultiplexerType::WezTerm => {
                            WezTermMultiplexer::new()
                                .map_err(|e| format!("WezTerm multiplexer is not available: {}", e))
                                .and_then(|mux| {
                                    GenericLocalTaskManager::new(agent_config, mux)
                                        .map(|tm| Arc::new(tm) as Arc<dyn TaskManager>)
                                        .map_err(|e| format!("Failed to create local task manager with WezTerm: {}", e))
                                })
                        }
                        #[cfg(feature = "zellij")]
                        CliMultiplexerType::Zellij => {
                            ZellijMultiplexer::new()
                                .map_err(|e| format!("Zellij multiplexer is not available: {}", e))
                                .and_then(|mux| {
                                    GenericLocalTaskManager::new(agent_config, mux)
                                        .map(|tm| Arc::new(tm) as Arc<dyn TaskManager>)
                                        .map_err(|e| format!("Failed to create local task manager with Zellij: {}", e))
                                })
                        }
                        #[cfg(feature = "screen")]
                        CliMultiplexerType::Screen => {
                            ScreenMultiplexer::new()
                                .map_err(|e| format!("Screen multiplexer is not available: {}", e))
                                .and_then(|mux| {
                                    GenericLocalTaskManager::new(agent_config, mux)
                                        .map(|tm| Arc::new(tm) as Arc<dyn TaskManager>)
                                        .map_err(|e| format!("Failed to create local task manager with Screen: {}", e))
                                })
                        }
                        #[cfg(all(feature = "tilix", target_os = "linux"))]
                        CliMultiplexerType::Tilix => {
                            TilixMultiplexer::new()
                                .map_err(|e| format!("Tilix multiplexer is not available: {}", e))
                                .and_then(|mux| {
                                    GenericLocalTaskManager::new(agent_config, mux)
                                        .map(|tm| Arc::new(tm) as Arc<dyn TaskManager>)
                                        .map_err(|e| format!("Failed to create local task manager with Tilix: {}", e))
                                })
                        }
                        #[cfg(feature = "windows-terminal")]
                        CliMultiplexerType::WindowsTerminal => {
                            WindowsTerminalMultiplexer::new()
                                .map_err(|e| format!("Windows Terminal multiplexer is not available: {}", e))
                                .and_then(|mux| {
                                    GenericLocalTaskManager::new(agent_config, mux)
                                        .map(|tm| Arc::new(tm) as Arc<dyn TaskManager>)
                                        .map_err(|e| format!("Failed to create local task manager with Windows Terminal: {}", e))
                                })
                        }
                        #[cfg(feature = "ghostty")]
                        CliMultiplexerType::Ghostty => {
                            GhosttyMultiplexer::new()
                                .map_err(|e| format!("Ghostty multiplexer is not available: {}", e))
                                .and_then(|mux| {
                                    GenericLocalTaskManager::new(agent_config, mux)
                                        .map(|tm| Arc::new(tm) as Arc<dyn TaskManager>)
                                        .map_err(|e| format!("Failed to create local task manager with Ghostty: {}", e))
                                })
                        }
                        #[cfg(feature = "vim")]
                        CliMultiplexerType::Vim => {
                            VimMultiplexer::new()
                                .map_err(|e| format!("Vim multiplexer is not available: {}", e))
                                .and_then(|mux| {
                                    GenericLocalTaskManager::new(agent_config, mux)
                                        .map(|tm| Arc::new(tm) as Arc<dyn TaskManager>)
                                        .map_err(|e| format!("Failed to create local task manager with Vim: {}", e))
                                })
                        }
                        #[cfg(feature = "neovim")]
                        CliMultiplexerType::Neovim => {
                            NeovimMultiplexer::new()
                                .map_err(|e| format!("Neovim multiplexer is not available: {}", e))
                                .and_then(|mux| {
                                    GenericLocalTaskManager::new(agent_config, mux)
                                        .map(|tm| Arc::new(tm) as Arc<dyn TaskManager>)
                                        .map_err(|e| format!("Failed to create local task manager with Neovim: {}", e))
                                })
                        }
                        #[cfg(feature = "emacs")]
                        CliMultiplexerType::Emacs => {
                            EmacsMultiplexer::new()
                                .map_err(|e| format!("Emacs multiplexer is not available: {}", e))
                                .and_then(|mux| {
                                    GenericLocalTaskManager::new(agent_config, mux)
                                        .map(|tm| Arc::new(tm) as Arc<dyn TaskManager>)
                                        .map_err(|e| format!("Failed to create local task manager with Emacs: {}", e))
                                })
                        }
                        // Fallback cases for when features are disabled
                        #[cfg(not(target_os = "macos"))]
                        CliMultiplexerType::ITerm2 => {
                            Err("iTerm2 is only available on macOS".to_string())
                        }
                        #[cfg(not(feature = "kitty"))]
                        CliMultiplexerType::Kitty => {
                            Err("Kitty multiplexer feature is not enabled".to_string())
                        }
                        #[cfg(not(feature = "wezterm"))]
                        CliMultiplexerType::WezTerm => {
                            Err("WezTerm multiplexer feature is not enabled".to_string())
                        }
                        #[cfg(not(feature = "zellij"))]
                        CliMultiplexerType::Zellij => {
                            Err("Zellij multiplexer feature is not enabled".to_string())
                        }
                        #[cfg(not(feature = "screen"))]
                        CliMultiplexerType::Screen => {
                            Err("Screen multiplexer feature is not enabled".to_string())
                        }
                        #[cfg(not(all(feature = "tilix", target_os = "linux")))]
                        CliMultiplexerType::Tilix => {
                            Err("Tilix multiplexer is only available on Linux with the tilix feature enabled".to_string())
                        }
                        #[cfg(not(feature = "windows-terminal"))]
                        CliMultiplexerType::WindowsTerminal => {
                            Err("Windows Terminal multiplexer feature is not enabled".to_string())
                        }
                        #[cfg(not(feature = "ghostty"))]
                        CliMultiplexerType::Ghostty => {
                            Err("Ghostty multiplexer feature is not enabled".to_string())
                        }
                        #[cfg(not(feature = "vim"))]
                        CliMultiplexerType::Vim => {
                            Err("Vim multiplexer feature is not enabled".to_string())
                        }
                        #[cfg(not(feature = "neovim"))]
                        CliMultiplexerType::Neovim => {
                            Err("Neovim multiplexer feature is not enabled".to_string())
                        }
                        #[cfg(not(feature = "emacs"))]
                        CliMultiplexerType::Emacs => {
                            Err("Emacs multiplexer feature is not enabled".to_string())
                        }
                    }
                }
                MultiplexerChoice::InSupportedTerminal => {
                    // This should not happen anymore since we map all terminals to multiplexer types
                    // Fall back to tmux as a safe default
                    GenericLocalTaskManager::new(agent_config, TmuxMultiplexer::default())
                        .map(|tm| Arc::new(tm) as Arc<dyn TaskManager>)
                        .map_err(|e| {
                            format!(
                                "Failed to create local task manager with Tmux (fallback): {}",
                                e
                            )
                        })
                }
                MultiplexerChoice::UnsupportedEnvironment => {
                    // Unsupported environment, default to tmux
                    GenericLocalTaskManager::new(agent_config, TmuxMultiplexer::default())
                        .map(|tm| Arc::new(tm) as Arc<dyn TaskManager>)
                        .map_err(|e| {
                            format!(
                                "Failed to create local task manager with Tmux (default): {}",
                                e
                            )
                        })
                }
            }
        }
        MultiplexerPreference::Tmux => {
            // Use tmux
            GenericLocalTaskManager::new(agent_config, TmuxMultiplexer::default())
                .map(|tm| Arc::new(tm) as Arc<dyn TaskManager>)
                .map_err(|e| format!("Failed to create local task manager with tmux: {}", e))
        }
        MultiplexerPreference::ITerm2 => {
            // Use iTerm2 - fail if not available
            #[cfg(target_os = "macos")]
            {
                let iterm2_mux = ITerm2Multiplexer::new()
                    .map_err(|e| format!("iTerm2 multiplexer is not available: {}", e))?;
                GenericLocalTaskManager::new(agent_config, iterm2_mux)
                    .map(|tm| Arc::new(tm) as Arc<dyn TaskManager>)
                    .map_err(|e| format!("Failed to create local task manager with iTerm2: {}", e))
            }
            #[cfg(not(target_os = "macos"))]
            {
                Err("iTerm2 multiplexer is only available on macOS".to_string())
            }
        }
        MultiplexerPreference::Kitty => {
            // Use kitty - fail if not available
            #[cfg(feature = "kitty")]
            {
                let kitty_mux = KittyMultiplexer::new()
                    .map_err(|e| format!("Kitty multiplexer is not available: {}", e))?;
                GenericLocalTaskManager::new(agent_config, kitty_mux)
                    .map(|tm| Arc::new(tm) as Arc<dyn TaskManager>)
                    .map_err(|e| format!("Failed to create local task manager with Kitty: {}", e))
            }
            #[cfg(not(feature = "kitty"))]
            {
                Err("Kitty multiplexer feature is not enabled".to_string())
            }
        }
        MultiplexerPreference::WezTerm => {
            // Use WezTerm - fail if not available
            #[cfg(feature = "wezterm")]
            {
                let wezterm_mux = WezTermMultiplexer::new()
                    .map_err(|e| format!("WezTerm multiplexer is not available: {}", e))?;
                GenericLocalTaskManager::new(agent_config, wezterm_mux)
                    .map(|tm| Arc::new(tm) as Arc<dyn TaskManager>)
                    .map_err(|e| format!("Failed to create local task manager with WezTerm: {}", e))
            }
            #[cfg(not(feature = "wezterm"))]
            {
                Err("WezTerm multiplexer feature is not enabled".to_string())
            }
        }
        MultiplexerPreference::Zellij => {
            // Use zellij - fail if not available
            #[cfg(feature = "zellij")]
            {
                let zellij_mux = ZellijMultiplexer::new()
                    .map_err(|e| format!("Zellij multiplexer is not available: {}", e))?;
                GenericLocalTaskManager::new(agent_config, zellij_mux)
                    .map(|tm| Arc::new(tm) as Arc<dyn TaskManager>)
                    .map_err(|e| format!("Failed to create local task manager with Zellij: {}", e))
            }
            #[cfg(not(feature = "zellij"))]
            {
                Err("Zellij multiplexer feature is not enabled".to_string())
            }
        }
        MultiplexerPreference::Screen => {
            // Use screen - fail if not available
            #[cfg(feature = "screen")]
            {
                let screen_mux = ScreenMultiplexer::new()
                    .map_err(|e| format!("Screen multiplexer is not available: {}", e))?;
                GenericLocalTaskManager::new(agent_config, screen_mux)
                    .map(|tm| Arc::new(tm) as Arc<dyn TaskManager>)
                    .map_err(|e| format!("Failed to create local task manager with Screen: {}", e))
            }
            #[cfg(not(feature = "screen"))]
            {
                Err("Screen multiplexer feature is not enabled".to_string())
            }
        }
        MultiplexerPreference::Tilix => {
            // Use tilix - fail if not available
            #[cfg(all(feature = "tilix", target_os = "linux"))]
            {
                let tilix_mux = TilixMultiplexer::new()
                    .map_err(|e| format!("Tilix multiplexer is not available: {}", e))?;
                GenericLocalTaskManager::new(agent_config, tilix_mux)
                    .map(|tm| Arc::new(tm) as Arc<dyn TaskManager>)
                    .map_err(|e| format!("Failed to create local task manager with Tilix: {}", e))
            }
            #[cfg(not(all(feature = "tilix", target_os = "linux")))]
            {
                Err(
                    "Tilix multiplexer is only available on Linux with the tilix feature enabled"
                        .to_string(),
                )
            }
        }
        MultiplexerPreference::WindowsTerminal => {
            // Use Windows Terminal - fail if not available
            #[cfg(feature = "windows-terminal")]
            {
                let wt_mux = WindowsTerminalMultiplexer::new()
                    .map_err(|e| format!("Windows Terminal multiplexer is not available: {}", e))?;
                GenericLocalTaskManager::new(agent_config, wt_mux)
                    .map(|tm| Arc::new(tm) as Arc<dyn TaskManager>)
                    .map_err(|e| {
                        format!(
                            "Failed to create local task manager with Windows Terminal: {}",
                            e
                        )
                    })
            }
            #[cfg(not(feature = "windows-terminal"))]
            {
                Err("Windows Terminal multiplexer feature is not enabled".to_string())
            }
        }
        MultiplexerPreference::Ghostty => {
            // Use ghostty - fail if not available
            #[cfg(feature = "ghostty")]
            {
                let ghostty_mux = GhosttyMultiplexer::new()
                    .map_err(|e| format!("Ghostty multiplexer is not available: {}", e))?;
                GenericLocalTaskManager::new(agent_config, ghostty_mux)
                    .map(|tm| Arc::new(tm) as Arc<dyn TaskManager>)
                    .map_err(|e| format!("Failed to create local task manager with Ghostty: {}", e))
            }
            #[cfg(not(feature = "ghostty"))]
            {
                Err("Ghostty multiplexer feature is not enabled".to_string())
            }
        }
        MultiplexerPreference::Vim => {
            // Use vim - fail if not available
            #[cfg(feature = "vim")]
            {
                let vim_mux = VimMultiplexer::new()
                    .map_err(|e| format!("Vim multiplexer is not available: {}", e))?;
                GenericLocalTaskManager::new(agent_config, vim_mux)
                    .map(|tm| Arc::new(tm) as Arc<dyn TaskManager>)
                    .map_err(|e| format!("Failed to create local task manager with Vim: {}", e))
            }
            #[cfg(not(feature = "vim"))]
            {
                Err("Vim multiplexer feature is not enabled".to_string())
            }
        }
        MultiplexerPreference::Neovim => {
            // Use neovim - fail if not available
            #[cfg(feature = "neovim")]
            {
                let neovim_mux = NeovimMultiplexer::new()
                    .map_err(|e| format!("Neovim multiplexer is not available: {}", e))?;
                GenericLocalTaskManager::new(agent_config, neovim_mux)
                    .map(|tm| Arc::new(tm) as Arc<dyn TaskManager>)
                    .map_err(|e| format!("Failed to create local task manager with Neovim: {}", e))
            }
            #[cfg(not(feature = "neovim"))]
            {
                Err("Neovim multiplexer feature is not enabled".to_string())
            }
        }
        MultiplexerPreference::Emacs => {
            // Use emacs - fail if not available
            #[cfg(feature = "emacs")]
            {
                let emacs_mux = EmacsMultiplexer::new()
                    .map_err(|e| format!("Emacs multiplexer is not available: {}", e))?;
                GenericLocalTaskManager::new(agent_config, emacs_mux)
                    .map(|tm| Arc::new(tm) as Arc<dyn TaskManager>)
                    .map_err(|e| format!("Failed to create local task manager with Emacs: {}", e))
            }
            #[cfg(not(feature = "emacs"))]
            {
                Err("Emacs multiplexer feature is not enabled".to_string())
            }
        }
    }
}

/// Create a task manager with recording disabled
pub fn create_task_manager_no_recording() -> Result<Arc<dyn TaskManager>, String> {
    create_local_task_manager(TaskManagerConfig {
        recording_disabled: true,
        config_file: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that Tmux multiplexer can be explicitly selected
    #[test]
    fn test_explicit_tmux_selection() {
        let config = TaskManagerConfig {
            recording_disabled: true,
            config_file: None,
        };

        // Tmux should always work (it's the default fallback)
        let result =
            create_local_task_manager_with_multiplexer(config.clone(), MultiplexerPreference::Tmux);

        // We expect this to succeed if tmux is available, or fail with a clear error
        match result {
            Ok(_) => {
                // Success - tmux is available
                println!("Tmux multiplexer successfully created");
            }
            Err(e) => {
                // Should fail with a descriptive error if tmux is not available
                assert!(
                    e.contains("tmux") || e.contains("not available"),
                    "Expected error about tmux availability, got: {}",
                    e
                );
            }
        }
    }

    /// Test that unavailable multiplexers are rejected with proper errors
    #[test]
    fn test_unavailable_multiplexer_rejection() {
        let config = TaskManagerConfig {
            recording_disabled: true,
            config_file: None,
        };

        // Test with a multiplexer that's likely not available
        #[cfg(not(feature = "zellij"))]
        {
            let result = create_local_task_manager_with_multiplexer(
                config.clone(),
                MultiplexerPreference::Zellij,
            );

            assert!(
                result.is_err(),
                "Should reject Zellij when feature is disabled"
            );
            if let Err(err) = result {
                assert!(
                    err.contains("Zellij") && err.contains("not enabled"),
                    "Error should mention Zellij and that it's not enabled: {}",
                    err
                );
            }
        }

        // Test iTerm2 on non-macOS platforms
        #[cfg(not(target_os = "macos"))]
        {
            let result = create_local_task_manager_with_multiplexer(
                config.clone(),
                MultiplexerPreference::ITerm2,
            );

            assert!(result.is_err(), "Should reject iTerm2 on non-macOS");
            if let Err(err) = result {
                assert!(
                    err.contains("iTerm2") && err.contains("macOS"),
                    "Error should mention iTerm2 is macOS-only: {}",
                    err
                );
            }
        }

        // Test Tilix on non-Linux platforms
        #[cfg(not(target_os = "linux"))]
        {
            let result = create_local_task_manager_with_multiplexer(
                config.clone(),
                MultiplexerPreference::Tilix,
            );

            assert!(result.is_err(), "Should reject Tilix on non-Linux");
            if let Err(err) = result {
                assert!(
                    err.contains("Tilix") && err.contains("Linux"),
                    "Error should mention Tilix is Linux-only: {}",
                    err
                );
            }
        }
    }

    /// Test that Auto mode works and doesn't panic
    #[test]
    fn test_auto_detection_works() {
        let config = TaskManagerConfig {
            recording_disabled: true,
            config_file: None,
        };

        // Auto should always succeed by falling back to tmux
        let result =
            create_local_task_manager_with_multiplexer(config, MultiplexerPreference::Auto);

        // This should succeed if any supported multiplexer is available
        // (which should at least be tmux in most environments)
        match result {
            Ok(_) => {
                println!("Auto-detection successfully created a task manager");
            }
            Err(e) => {
                // If it fails, it should be due to no multiplexer being available
                println!(
                    "Auto-detection failed (expected in minimal environments): {}",
                    e
                );
            }
        }
    }

    /// Test that all enum variants are handled
    #[test]
    fn test_all_multiplexer_variants_handled() {
        let config = TaskManagerConfig {
            recording_disabled: true,
            config_file: None,
        };

        // Test each variant to ensure they don't panic
        let variants = vec![
            MultiplexerPreference::Auto,
            MultiplexerPreference::Tmux,
            MultiplexerPreference::ITerm2,
            MultiplexerPreference::Kitty,
            MultiplexerPreference::WezTerm,
            MultiplexerPreference::Zellij,
            MultiplexerPreference::Screen,
            MultiplexerPreference::Tilix,
            MultiplexerPreference::WindowsTerminal,
            MultiplexerPreference::Ghostty,
            MultiplexerPreference::Vim,
            MultiplexerPreference::Neovim,
            MultiplexerPreference::Emacs,
        ];

        for variant in variants {
            // Each variant should either succeed or return a proper error
            // (not panic or hang)
            let result =
                create_local_task_manager_with_multiplexer(config.clone(), variant.clone());

            match result {
                Ok(_) => {
                    println!("Variant {:?} succeeded", variant);
                }
                Err(e) => {
                    // Verify the error message is meaningful
                    assert!(!e.is_empty(), "Variant {:?} returned empty error", variant);
                    println!("Variant {:?} failed with: {}", variant, e);
                }
            }
        }
    }

    /// Test that environment detection correctly maps to multiplexer types
    #[test]
    fn test_environment_detection_mapping() {
        use ah_mux::detection::TerminalEnvironment;

        // Test that each terminal environment maps to the correct multiplexer type
        let test_cases = vec![
            (vec![TerminalEnvironment::Tmux], "Tmux"),
            #[cfg(feature = "kitty")]
            (vec![TerminalEnvironment::Kitty], "Kitty"),
            #[cfg(feature = "wezterm")]
            (vec![TerminalEnvironment::WezTerm], "WezTerm"),
            #[cfg(feature = "zellij")]
            (vec![TerminalEnvironment::Zellij], "Zellij"),
            #[cfg(feature = "screen")]
            (vec![TerminalEnvironment::Screen], "Screen"),
        ];

        for (envs, expected_name) in test_cases {
            let choice = determine_multiplexer_choice(&envs);
            match choice {
                MultiplexerChoice::InSupportedMultiplexer(_) => {
                    println!(
                        "Environment {:?} correctly mapped to {}",
                        envs, expected_name
                    );
                }
                MultiplexerChoice::InSupportedTerminal => {
                    panic!(
                        "Environment {:?} incorrectly mapped to InSupportedTerminal",
                        envs
                    );
                }
                MultiplexerChoice::UnsupportedEnvironment => {
                    panic!(
                        "Environment {:?} incorrectly mapped to UnsupportedEnvironment",
                        envs
                    );
                }
            }
        }
    }

    /// Test that nested environments preserve innermost multiplexer
    #[test]
    fn test_nested_environment_detection() {
        use ah_mux::detection::TerminalEnvironment;

        // Test nested environments: outermost to innermost
        // Should select the innermost supported multiplexer
        #[cfg(feature = "kitty")]
        {
            let envs = vec![
                TerminalEnvironment::Kitty, // outer
                TerminalEnvironment::Tmux,  // inner (should be selected)
            ];
            let choice = determine_multiplexer_choice(&envs);
            match choice {
                MultiplexerChoice::InSupportedMultiplexer(CliMultiplexerType::Tmux) => {
                    println!("Correctly selected innermost Tmux from nested environment");
                }
                _ => {
                    panic!("Failed to select innermost Tmux from {:?}", envs);
                }
            }
        }

        // Test that unsupported outer layers are skipped
        let envs = vec![
            TerminalEnvironment::Tmux, // inner (should be selected)
        ];
        let choice = determine_multiplexer_choice(&envs);
        match choice {
            MultiplexerChoice::InSupportedMultiplexer(CliMultiplexerType::Tmux) => {
                println!("Correctly selected Tmux");
            }
            _ => {
                panic!("Failed to select Tmux from {:?}", envs);
            }
        }
    }
}
