// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Task-related domain types
//!
//! These types represent tasks, their states, and related business entities
//! that are shared across the Agent Harbor system.

use crate::AgentChoice;
#[cfg(feature = "clap")]
use clap::ValueEnum;
use serde::{Deserialize, Serialize};

/// Task state - shared between REST API and local task management
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum TaskState {
    /// Draft task being edited
    Draft,
    /// Task is queued for execution
    Queued,
    /// Task is being provisioned
    Provisioning,
    /// Task is actively running
    Running,
    /// Task is being paused
    Pausing,
    /// Task execution is paused
    Paused,
    /// Task is resuming from pause
    Resuming,
    /// Task is being stopped
    Stopping,
    /// Task execution is stopped
    Stopped,
    /// Task completed successfully
    Completed,
    /// Task failed during execution
    Failed,
    /// Task was cancelled
    Cancelled,
    /// Task results were merged
    Merged,
}

impl std::fmt::Display for TaskState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status_str = match self {
            TaskState::Draft => "draft",
            TaskState::Queued => "queued",
            TaskState::Provisioning => "provisioning",
            TaskState::Running => "running",
            TaskState::Pausing => "pausing",
            TaskState::Paused => "paused",
            TaskState::Resuming => "resuming",
            TaskState::Stopping => "stopping",
            TaskState::Stopped => "stopped",
            TaskState::Completed => "completed",
            TaskState::Failed => "failed",
            TaskState::Cancelled => "cancelled",
            TaskState::Merged => "merged",
        };
        write!(f, "{}", status_str)
    }
}

/// Log levels for task events and CLI configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "clap", derive(ValueEnum))]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    /// Show only errors
    Error,
    /// Show warnings and errors
    Warn,
    /// Show info, warnings, and errors
    Info,
    /// Show debug info, info, warnings, and errors
    Debug,
    /// Show all log levels including trace
    Trace,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Error => write!(f, "error"),
            LogLevel::Warn => write!(f, "warn"),
            LogLevel::Info => write!(f, "info"),
            LogLevel::Debug => write!(f, "debug"),
            LogLevel::Trace => write!(f, "trace"),
        }
    }
}

impl From<LogLevel> for tracing::Level {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Error => tracing::Level::ERROR,
            LogLevel::Warn => tracing::Level::WARN,
            LogLevel::Info => tracing::Level::INFO,
            LogLevel::Debug => tracing::Level::DEBUG,
            LogLevel::Trace => tracing::Level::TRACE,
        }
    }
}

/// Tool execution status
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolStatus {
    Started,
    Completed,
    Failed,
}

/// Draft task - represents a task being created/edited
/// Different from TaskCard as it has different lifecycle and structure
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DraftTask {
    pub id: String,
    pub description: String,
    pub repository: String,
    pub branch: String,
    pub selected_agents: Vec<AgentChoice>,
    pub created_at: String,
}

/// Task execution record - represents executed/running tasks in the domain
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskExecution {
    pub id: String,
    pub repository: String,
    pub branch: String,
    pub agents: Vec<AgentChoice>,
    pub state: TaskState,
    pub timestamp: String,
    pub activity: Vec<String>,                // For active tasks
    pub delivery_status: Vec<DeliveryStatus>, // For completed/merged tasks
}

/// Delivery status for completed tasks
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DeliveryStatus {
    BranchCreated,
    PullRequestCreated { pr_number: u32, title: String },
    PullRequestMerged { pr_number: u32 },
}
/// Task status - consolidated enum for task lifecycle states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "clap", derive(ValueEnum))]
#[cfg_attr(feature = "rusqlite", derive(rusqlite::ToSql, rusqlite::FromSql))]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    /// Task has been created but not yet started
    Pending,
    /// Task is currently running
    Running,
    /// Task completed successfully
    Completed,
    /// Task failed with an error
    Failed,
    /// Task was cancelled
    Cancelled,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status_str = match self {
            TaskStatus::Pending => "pending",
            TaskStatus::Running => "running",
            TaskStatus::Completed => "completed",
            TaskStatus::Failed => "failed",
            TaskStatus::Cancelled => "cancelled",
        };
        write!(f, "{}", status_str)
    }
}

impl std::str::FromStr for TaskStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(TaskStatus::Pending),
            "running" => Ok(TaskStatus::Running),
            "completed" => Ok(TaskStatus::Completed),
            "failed" => Ok(TaskStatus::Failed),
            "cancelled" => Ok(TaskStatus::Cancelled),
            _ => Err(format!("Unknown task status: {}", s)),
        }
    }
}

/// Multiplexer type - consolidated enum for terminal multiplexers
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "clap", derive(ValueEnum))]
#[cfg_attr(feature = "rusqlite", derive(rusqlite::ToSql, rusqlite::FromSql))]
#[serde(rename_all = "kebab-case")]
pub enum MultiplexerType {
    /// tmux multiplexer
    Tmux,
    /// iTerm2 multiplexer
    ITerm2,
    /// Kitty multiplexer
    Kitty,
    /// WezTerm multiplexer
    WezTerm,
    /// Zellij multiplexer
    Zellij,
    /// Screen multiplexer
    Screen,
    /// Tilix multiplexer
    Tilix,
    /// Windows Terminal multiplexer
    WindowsTerminal,
    /// Ghostty multiplexer
    Ghostty,
    /// Vim multiplexer
    Vim,
    /// Neovim multiplexer
    Neovim,
    /// Emacs multiplexer
    Emacs,
}

impl std::fmt::Display for MultiplexerType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let type_str = match self {
            MultiplexerType::Tmux => "tmux",
            MultiplexerType::ITerm2 => "iterm2",
            MultiplexerType::Kitty => "kitty",
            MultiplexerType::WezTerm => "wezterm",
            MultiplexerType::Zellij => "zellij",
            MultiplexerType::Screen => "screen",
            MultiplexerType::Tilix => "tilix",
            MultiplexerType::WindowsTerminal => "windows-terminal",
            MultiplexerType::Ghostty => "ghostty",
            MultiplexerType::Vim => "vim",
            MultiplexerType::Neovim => "neovim",
            MultiplexerType::Emacs => "emacs",
        };
        write!(f, "{}", type_str)
    }
}

impl std::str::FromStr for MultiplexerType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "tmux" => Ok(MultiplexerType::Tmux),
            "iterm2" => Ok(MultiplexerType::ITerm2),
            "kitty" => Ok(MultiplexerType::Kitty),
            "wezterm" => Ok(MultiplexerType::WezTerm),
            "zellij" => Ok(MultiplexerType::Zellij),
            "screen" => Ok(MultiplexerType::Screen),
            "tilix" => Ok(MultiplexerType::Tilix),
            "windows-terminal" => Ok(MultiplexerType::WindowsTerminal),
            "ghostty" => Ok(MultiplexerType::Ghostty),
            "vim" => Ok(MultiplexerType::Vim),
            "neovim" => Ok(MultiplexerType::Neovim),
            "emacs" => Ok(MultiplexerType::Emacs),
            _ => Err(format!("Unknown multiplexer type: {}", s)),
        }
    }
}

impl MultiplexerType {
    /// Get a human-readable display name for the multiplexer type
    pub fn display_name(&self) -> &'static str {
        match self {
            MultiplexerType::Tmux => "Tmux",
            MultiplexerType::ITerm2 => "iTerm2",
            MultiplexerType::Kitty => "Kitty",
            MultiplexerType::WezTerm => "WezTerm",
            MultiplexerType::Zellij => "Zellij",
            MultiplexerType::Screen => "GNU Screen",
            MultiplexerType::Tilix => "Tilix",
            MultiplexerType::WindowsTerminal => "Windows Terminal",
            MultiplexerType::Ghostty => "Ghostty",
            MultiplexerType::Vim => "Vim",
            MultiplexerType::Neovim => "Neovim",
            MultiplexerType::Emacs => "Emacs",
        }
    }
}

/// Output format for agent execution
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "clap", derive(ValueEnum))]
#[serde(rename_all = "kebab-case")]
pub enum OutputFormat {
    /// Display agent output unmodified (default)
    Text,
    /// Display textual output with consistent structure regardless of agent type
    #[cfg_attr(feature = "clap", clap(name = "text-normalized"))]
    TextNormalized,
    /// Display JSON output if available (e.g., codex --json)
    Json,
    /// Map JSON to agent-harbor defined schema consistent across agent types
    #[cfg_attr(feature = "clap", clap(name = "json-normalized"))]
    JsonNormalized,
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let format_str = match self {
            OutputFormat::Text => "text",
            OutputFormat::TextNormalized => "text-normalized",
            OutputFormat::Json => "json",
            OutputFormat::JsonNormalized => "json-normalized",
        };
        write!(f, "{}", format_str)
    }
}

impl std::str::FromStr for OutputFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "text" => Ok(OutputFormat::Text),
            "text-normalized" => Ok(OutputFormat::TextNormalized),
            "json" => Ok(OutputFormat::Json),
            "json-normalized" => Ok(OutputFormat::JsonNormalized),
            _ => Err(format!("Unknown output format: {}", s)),
        }
    }
}

/// Task information from external systems (simplified for now)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskInfo {
    pub id: String,
    pub title: String,
    pub status: String,
    pub repository: String,
    pub branch: String,
    pub created_at: String,
    pub models: Vec<AgentChoice>,
}

impl TaskExecution {
    /// Add activity to an active task
    pub fn add_activity(&mut self, activity: String) {
        // Check if task is in an active/running state
        match self.state {
            TaskState::Queued
            | TaskState::Provisioning
            | TaskState::Running
            | TaskState::Pausing
            | TaskState::Paused
            | TaskState::Resuming
            | TaskState::Stopping
            | TaskState::Stopped => {
                self.activity.push(activity);
                // Keep only last 10 activities for memory efficiency
                if self.activity.len() > 10 {
                    self.activity.remove(0);
                }
            }
            _ => {} // Don't add activity for non-active tasks
        }
    }

    /// Get recent activity for display
    pub fn get_recent_activity(&self, count: usize) -> Vec<String> {
        // Check if task is in an active/running state
        match self.state {
            TaskState::Queued
            | TaskState::Provisioning
            | TaskState::Running
            | TaskState::Pausing
            | TaskState::Paused
            | TaskState::Resuming
            | TaskState::Stopping
            | TaskState::Stopped => {
                let recent: Vec<String> = self.activity.iter().rev().take(count).cloned().collect();
                let mut result: Vec<String> = recent.into_iter().rev().collect();

                // Always return exactly count lines, padding with empty strings at the beginning
                while result.len() < count {
                    result.insert(0, String::new());
                }
                result
            }
            _ => {
                vec![String::new(); count]
            }
        }
    }
}
