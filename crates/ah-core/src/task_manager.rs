//! Task Manager - Abstract Task Launching Interface
//!
//! This module defines the `TaskManager` trait that abstracts task launching
//! functionality across different execution modes (local, remote, mock).
//!
//! ## Architecture Overview
//!
//! The TaskManager trait provides a clean abstraction for launching tasks,
//! allowing the ViewModel to be decoupled from the specifics of task execution.
//! Different implementations handle local execution, remote REST API calls,
//! and mock testing scenarios.
//!
//! ## Usage in MVVM Architecture
//!
//! The ViewModel holds a reference to a TaskManager and calls `launch_task()`
//! when the user initiates task creation. The TaskManager handles the actual
//! execution details and returns a result that the ViewModel can translate
//! into domain messages for the Model.

use ah_domain_types::{Repository, Branch, TaskInfo, SelectedModel};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::pin::Pin;

/// Parameters for launching a task
#[derive(Debug, Clone, PartialEq)]
pub struct TaskLaunchParams {
    pub repository: String,
    pub branch: String,
    pub description: String,
    pub models: Vec<SelectedModel>,
}

/// Result of a task launch operation
#[derive(Debug, Clone, PartialEq)]
pub enum TaskLaunchResult {
    /// Task launched successfully with the assigned task ID
    Success { task_id: String },
    /// Task launch failed with an error message
    Failure { error: String },
}

impl TaskLaunchResult {
    /// Check if the launch was successful
    pub fn is_success(&self) -> bool {
        matches!(self, TaskLaunchResult::Success { .. })
    }

    /// Get the task ID if successful
    pub fn task_id(&self) -> Option<&str> {
        match self {
            TaskLaunchResult::Success { task_id } => Some(task_id),
            TaskLaunchResult::Failure { .. } => None,
        }
    }

    /// Get the error message if failed
    pub fn error(&self) -> Option<&str> {
        match self {
            TaskLaunchResult::Success { .. } => None,
            TaskLaunchResult::Failure { error } => Some(error),
        }
    }
}

/// Task event types corresponding to SSE events from API.md
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TaskEvent {
    /// Status change event (queued, provisioning, running, etc.)
    Status {
        status: TaskExecutionStatus,
        ts: DateTime<Utc>,
    },
    /// Log message from the task execution
    Log {
        level: LogLevel,
        message: String,
        tool_execution_id: Option<String>,
        ts: DateTime<Utc>,
    },
    /// Agent thought/reasoning event
    Thought {
        thought: String,
        reasoning: Option<String>,
        ts: DateTime<Utc>,
    },
    /// Tool usage started
    ToolUse {
        tool_name: String,
        tool_args: serde_json::Value,
        tool_execution_id: String,
        status: ToolStatus,
        ts: DateTime<Utc>,
    },
    /// Tool execution completed
    ToolResult {
        tool_name: String,
        tool_output: String,
        tool_execution_id: String,
        status: ToolStatus,
        ts: DateTime<Utc>,
    },
    /// File modification event
    FileEdit {
        file_path: String,
        lines_added: usize,
        lines_removed: usize,
        description: Option<String>,
        ts: DateTime<Utc>,
    },
}

/// Task execution status for events
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskExecutionStatus {
    Queued,
    Provisioning,
    Running,
    Pausing,
    Paused,
    Resuming,
    Stopping,
    Stopped,
    Completed,
    Failed,
    Cancelled,
}

/// Log levels for task events
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

/// Tool execution status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ToolStatus {
    Started,
    Completed,
    Failed,
}

/// Result of saving a draft task
#[derive(Debug, Clone, PartialEq)]
pub enum SaveDraftResult {
    Success,
    Failure { error: String },
}

/// Abstract trait for task launching functionality
///
/// This trait defines the interface that all task managers must implement.
/// Different implementations handle different execution modes:
/// - Local: Execute tasks using local ah-core crate
/// - Remote: Execute tasks via REST API calls
/// - Mock: Simulate task execution for testing
#[async_trait]
pub trait TaskManager: Send + Sync {
    /// Launch a task with the given parameters
    ///
    /// Returns a TaskLaunchResult indicating success or failure.
    /// Real implementations may involve network calls, process spawning,
    /// or other async operations.
    async fn launch_task(&self, params: TaskLaunchParams) -> TaskLaunchResult;

    /// Get a stream of task events for the given task ID
    ///
    /// Returns a stream that yields task events as they occur during execution.
    /// The stream should continue until the task completes or fails.
    /// Real implementations connect to SSE streams or monitoring systems.
    fn task_events_stream(&self, task_id: &str) -> Pin<Box<dyn Stream<Item = TaskEvent> + Send>>;

    /// Get initial set of tasks to display in the UI
    ///
    /// Returns separate collections for draft tasks and completed tasks that should be shown
    /// when the application starts. Drafts are editable, tasks are read-only.
    async fn get_initial_tasks(&self) -> (Vec<TaskInfo>, Vec<TaskInfo>);

    /// Auto-save modifications to a draft task
    ///
    /// Persists changes to a draft task to prevent data loss.
    async fn save_draft_task(&self, draft_id: &str, description: &str, repository: &str, branch: &str, models: &[SelectedModel]) -> SaveDraftResult;

    /// List available repositories/projects
    ///
    /// Returns repositories that can be used for task creation.
    async fn list_repositories(&self) -> Vec<Repository>;

    /// List branches for a specific repository
    ///
    /// Returns available branches for the given repository.
    async fn list_branches(&self, repository_id: &str) -> Vec<Branch>;

    /// Get a human-readable description of this task manager
    fn description(&self) -> &str;
}
