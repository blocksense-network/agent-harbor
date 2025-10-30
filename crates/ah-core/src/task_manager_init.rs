// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Task manager initialization utilities
//!
//! This module provides shared functionality for initializing task managers
//! across different parts of the application (dashboard, session viewer, CLI, etc.)

use crate::TaskManager;
use crate::agent_executor::AgentExecutionConfig;
use crate::local_task_manager::GenericLocalTaskManager;
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
enum MultiplexerChoice {
    /// Currently running inside a supported multiplexer (use inner-most one)
    InSupportedMultiplexer(CliMultiplexerType),
    /// Currently running in a supported terminal but no multiplexer
    InSupportedTerminal,
    /// Not in any supported terminal/multiplexer environment
    UnsupportedEnvironment,
}

/// Multiplexer types supported
#[derive(Debug, Clone)]
enum CliMultiplexerType {
    /// Use Tmux multiplexer
    Tmux,
}

/// Determine multiplexer choice based on terminal environments
fn determine_multiplexer_choice(terminal_envs: &[TerminalEnvironment]) -> MultiplexerChoice {
    // Check for supported multiplexers (inner-most first, as per spec)
    // terminal_envs is in wrapping order (outermost to innermost), so rev() gives us innermost first
    for env in terminal_envs.iter().rev() {
        match env {
            TerminalEnvironment::Tmux => {
                // Tmux is supported
                return MultiplexerChoice::InSupportedMultiplexer(CliMultiplexerType::Tmux);
            }
            #[cfg(target_os = "macos")]
            TerminalEnvironment::ITerm2 => {
                // iTerm2 is a terminal emulator, not a multiplexer, but we can run dashboard directly
                return MultiplexerChoice::InSupportedTerminal;
            }
            // Other supported terminal environments - we can run dashboard directly
            TerminalEnvironment::Kitty
            | TerminalEnvironment::WezTerm
            | TerminalEnvironment::Tilix
            | TerminalEnvironment::WindowsTerminal
            | TerminalEnvironment::Ghostty => {
                return MultiplexerChoice::InSupportedTerminal;
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
    /// Prefer iTerm2 multiplexer if available, fall back to tmux
    ITerm2,
    /// Use tmux multiplexer
    Tmux,
}

/// Detect the appropriate multiplexer for the current environment
fn detect_multiplexer() -> Result<Box<dyn Multiplexer + Send + Sync>, String> {
    // Detect the current terminal environment stack
    let terminal_envs = ah_mux::detection::detect_terminal_environments();

    // Determine which multiplexer to use based on terminal environment detection
    let choice = determine_multiplexer_choice(&terminal_envs);

    match choice {
        MultiplexerChoice::InSupportedMultiplexer(CliMultiplexerType::Tmux) => {
            Ok(Box::new(TmuxMultiplexer::default()))
        }
        MultiplexerChoice::InSupportedTerminal => {
            // For supported terminals like iTerm2, try to use iTerm2 multiplexer
            // Fall back to tmux if iTerm2 is not available
            match ITerm2Multiplexer::new() {
                Ok(iterm2_mux) => Ok(Box::new(iterm2_mux)),
                Err(_) => Ok(Box::new(TmuxMultiplexer::default())),
            }
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
                MultiplexerChoice::InSupportedMultiplexer(CliMultiplexerType::Tmux) => {
                    GenericLocalTaskManager::new(agent_config, TmuxMultiplexer::default())
                        .map(|tm| Arc::new(tm) as Arc<dyn TaskManager>)
                        .map_err(|e| format!("Failed to create local task manager: {}", e))
                }
                MultiplexerChoice::InSupportedTerminal => {
                    // For supported terminals like iTerm2, try to use iTerm2 multiplexer
                    // Fall back to tmux if iTerm2 is not available
                    match ITerm2Multiplexer::new() {
                        Ok(iterm2_mux) => GenericLocalTaskManager::new(agent_config, iterm2_mux)
                            .map(|tm| Arc::new(tm) as Arc<dyn TaskManager>)
                            .map_err(|e| format!("Failed to create local task manager: {}", e)),
                        Err(_) => {
                            GenericLocalTaskManager::new(agent_config, TmuxMultiplexer::default())
                                .map(|tm| Arc::new(tm) as Arc<dyn TaskManager>)
                                .map_err(|e| format!("Failed to create local task manager: {}", e))
                        }
                    }
                }
                MultiplexerChoice::UnsupportedEnvironment => {
                    // Unsupported environment, default to tmux
                    GenericLocalTaskManager::new(agent_config, TmuxMultiplexer::default())
                        .map(|tm| Arc::new(tm) as Arc<dyn TaskManager>)
                        .map_err(|e| format!("Failed to create local task manager: {}", e))
                }
            }
        }
        MultiplexerPreference::ITerm2 => {
            // Prefer iTerm2, fall back to tmux
            match ITerm2Multiplexer::new() {
                Ok(iterm2_mux) => GenericLocalTaskManager::new(agent_config, iterm2_mux)
                    .map(|tm| Arc::new(tm) as Arc<dyn TaskManager>)
                    .map_err(|e| format!("Failed to create local task manager: {}", e)),
                Err(_) => GenericLocalTaskManager::new(agent_config, TmuxMultiplexer::default())
                    .map(|tm| Arc::new(tm) as Arc<dyn TaskManager>)
                    .map_err(|e| format!("Failed to create local task manager: {}", e)),
            }
        }
        MultiplexerPreference::Tmux => {
            // Use tmux
            GenericLocalTaskManager::new(agent_config, TmuxMultiplexer::default())
                .map(|tm| Arc::new(tm) as Arc<dyn TaskManager>)
                .map_err(|e| format!("Failed to create local task manager: {}", e))
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
