// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

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
//! ## Design Principles
//!
//! ### Location and Dependencies
//!
//! The `TaskManager` trait is intentionally located in `ah-core` because:
//!
//! 1. **ah-core contains everything necessary for task execution**: Whether
//!    executing tasks locally through multiplexers or remotely via REST APIs,
//!    ah-core provides the full execution environment and coordination logic.
//!
//! 2. **ah-core implements TaskManager for all execution modes**: It contains
//!    implementations that use local multiplexers (`LocalTaskManager`) and
//!    REST API clients (`GenericRestTaskManager`).
//!
//! 3. **The RestApiClient trait stays in ah-core**: It defines a subset of REST
//!    API features that ah-core needs for task execution. This trait abstracts
//!    the HTTP client implementation, allowing ah-core to work with both real
//!    REST clients and mock clients for testing.
//!
//! ### Separation from REST Client Crate
//!
//! The `ah-rest-client` crate intentionally does NOT implement the `TaskManager`
//! trait to maintain minimal dependencies. This design allows:
//!
//! 1. **Third-party usage**: External software can use `ah-rest-client` to
//!    interact with Agent Harbor APIs without pulling in the heavy dependencies
//!    of ah-core (multiplexers, local execution, database, etc.).
//!
//! 2. **Clean dependency boundaries**: ah-core depends on ah-rest-client for
//!    HTTP communication, but ah-rest-client does not depend on ah-core.
//!
//! 3. **Flexible composition**: ah-core can compose TaskManager implementations
//!    that use the REST client, while the REST client remains a lightweight
//!    HTTP library.
//!
//! ## Usage in MVVM Architecture
//!
//! The ViewModel holds a reference to a TaskManager and calls `launch_task()`
//! when the user initiates task creation. The TaskManager handles the actual
//! execution details and returns a result that the ViewModel can translate
//! into domain messages for the Model.

use ah_domain_types::{
    Branch, LogLevel, Repository, SelectedModel, TaskExecution, TaskExecutionStatus, TaskInfo,
    ToolStatus,
};
use ah_local_db::models::DraftRecord;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::{fmt, pin::Pin};

/// Starting point for task execution - defines where to start the task from
#[derive(Debug, Clone, PartialEq)]
pub enum StartingPoint {
    /// Start from a repository branch (traditional approach)
    RepositoryBranch { repository: String, branch: String },
    /// Start from a specific repository commit
    RepositoryCommit { repository: String, commit: String },
    /// Start from a filesystem snapshot
    FilesystemSnapshot { snapshot_id: String },
}

/// Parameters for launching a task
#[derive(Debug, Clone, PartialEq)]
pub struct TaskLaunchParams {
    pub repository: String,
    pub branch: String,
    pub description: String,
    pub models: Vec<SelectedModel>,
}

impl TaskLaunchParams {
    /// Create new TaskLaunchParams with validation
    ///
    /// # Errors
    ///
    /// Returns an error if any validation fails:
    /// - Empty or whitespace-only description
    /// - No models selected
    /// - Empty or whitespace-only repository
    /// - Empty or whitespace-only branch
    /// - Invalid repository URL format
    pub fn new(
        repository: String,
        branch: String,
        description: String,
        models: Vec<SelectedModel>,
    ) -> Result<Self, String> {
        // Validate description
        if description.trim().is_empty() {
            return Err("Task description cannot be empty".to_string());
        }

        // Validate models
        if models.is_empty() {
            return Err("At least one model must be selected".to_string());
        }

        // Validate repository
        if repository.trim().is_empty() {
            return Err("Repository cannot be empty".to_string());
        }

        // Validate branch
        if branch.trim().is_empty() {
            return Err("Branch cannot be empty".to_string());
        }

        // Validate repository URL format
        if let Err(e) = url::Url::parse(&repository) {
            return Err(format!("Invalid repository URL: {}", e));
        }

        Ok(Self {
            repository,
            branch,
            description,
            models,
        })
    }

    /// Create TaskLaunchParams from a DraftRecord
    ///
    /// This is useful for resuming draft tasks.
    pub fn from_draft(draft: &DraftRecord) -> Result<Self, String> {
        let models = serde_json::from_str(&draft.models)
            .map_err(|e| format!("Invalid models JSON in draft: {}", e))?;

        Self::new(
            draft.repository.clone(),
            draft.branch.clone().unwrap_or_else(|| "main".to_string()),
            draft.description.clone(),
            models,
        )
    }
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

impl fmt::Display for TaskLaunchResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskLaunchResult::Success { task_id } => {
                write!(f, "Task launched successfully: {task_id}")
            }
            TaskLaunchResult::Failure { error } => {
                write!(f, "Task launch failed: {error}")
            }
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

    /// Launch a task from a specific starting point
    ///
    /// This allows launching tasks from filesystem snapshots or specific commits,
    /// which is useful for replay scenarios.
    async fn launch_task_from_starting_point(
        &self,
        starting_point: StartingPoint,
        description: &str,
        models: &[SelectedModel],
    ) -> TaskLaunchResult;

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
    async fn get_initial_tasks(&self) -> (Vec<TaskInfo>, Vec<TaskExecution>);

    /// Auto-save modifications to a draft task
    ///
    /// Persists changes to a draft task to prevent data loss.
    async fn save_draft_task(
        &self,
        draft_id: &str,
        description: &str,
        repository: &str,
        branch: &str,
        models: &[SelectedModel],
    ) -> SaveDraftResult;

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
