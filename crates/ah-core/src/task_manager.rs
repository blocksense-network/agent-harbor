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

use ah_domain_types::AgentSoftware;
use ah_domain_types::{AgentChoice, LogLevel, TaskExecution, TaskInfo, TaskState, ToolStatus};
use ah_local_db::models::DraftRecord;
use ah_mux_core::SplitMode;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use tracing::debug;

/// Result of a draft task save operation
#[derive(Debug, Clone, PartialEq)]
pub enum SaveDraftResult {
    /// The draft was successfully saved
    Success,
    /// The save operation failed with an error message
    Failure { error: String },
}

/// Starting point for task execution - defines where to start the task from
#[derive(Debug, Clone, PartialEq, Eq)]
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
    starting_point: StartingPoint,
    working_copy_mode: crate::WorkingCopyMode,
    description: String,
    models: Vec<AgentChoice>,
    agent_type: AgentSoftware,
    split_mode: SplitMode,
    focus: bool,
    record: bool,
    task_id: String,
}

/// Builder for TaskLaunchParams
#[derive(Debug, Clone)]
pub struct TaskLaunchParamsBuilder {
    starting_point: Option<StartingPoint>,
    working_copy_mode: Option<crate::WorkingCopyMode>,
    description: Option<String>,
    models: Option<Vec<AgentChoice>>,
    agent_type: Option<AgentSoftware>,
    split_mode: Option<SplitMode>,
    focus: Option<bool>,
    record: Option<bool>,
    task_id: Option<String>,
}

impl TaskLaunchParams {
    /// Create a new builder for TaskLaunchParams
    pub fn builder() -> TaskLaunchParamsBuilder {
        TaskLaunchParamsBuilder::new()
    }

    /// Get the starting point
    pub fn starting_point(&self) -> &StartingPoint {
        &self.starting_point
    }

    /// Get the working copy mode
    pub fn working_copy_mode(&self) -> &crate::WorkingCopyMode {
        &self.working_copy_mode
    }

    /// Get the repository (for backward compatibility, extracts from starting_point)
    pub fn repository(&self) -> &str {
        match &self.starting_point {
            StartingPoint::RepositoryBranch { repository, .. } => repository,
            StartingPoint::RepositoryCommit { repository, .. } => repository,
            StartingPoint::FilesystemSnapshot { .. } => "",
        }
    }

    /// Get the branch (for backward compatibility, extracts from starting_point)
    pub fn branch(&self) -> &str {
        match &self.starting_point {
            StartingPoint::RepositoryBranch { branch, .. } => branch,
            StartingPoint::RepositoryCommit { .. } => "",
            StartingPoint::FilesystemSnapshot { .. } => "",
        }
    }

    /// Get the description
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Get the models
    pub fn models(&self) -> &[AgentChoice] {
        &self.models
    }

    /// Get the agent type
    pub fn agent_type(&self) -> &AgentSoftware {
        &self.agent_type
    }

    /// Get the split mode
    pub fn split_mode(&self) -> &SplitMode {
        &self.split_mode
    }

    /// Get the focus setting
    pub fn focus(&self) -> bool {
        self.focus
    }

    /// Get the record setting
    pub fn record(&self) -> bool {
        self.record
    }

    /// Get the task ID
    pub fn task_id(&self) -> &str {
        &self.task_id
    }

    /// Create TaskLaunchParams from a DraftRecord
    ///
    /// This is useful for resuming draft tasks.
    pub fn from_draft(draft: &DraftRecord) -> Result<Self, String> {
        let models: Vec<AgentChoice> = serde_json::from_str(&draft.models)
            .map_err(|e| format!("Invalid models JSON in draft: {}", e))?;

        // Use the first model as the primary model
        let _model = models.first().ok_or_else(|| "No models found in draft".to_string())?;

        let starting_point = StartingPoint::RepositoryBranch {
            repository: draft.repository.clone(),
            branch: draft.branch.clone().unwrap_or_else(|| "main".to_string()),
        };

        Self::builder()
            .starting_point(starting_point)
            .description(draft.description.clone())
            .agents(models)
            .agent_type(AgentSoftware::Codex) // Default agent type for drafts
            .task_id(draft.id.clone()) // Use the draft's existing ID
            .build()
    }
}

impl TaskLaunchParamsBuilder {
    /// Create a new builder with default values
    pub fn new() -> Self {
        Self {
            starting_point: None,
            working_copy_mode: Some(crate::WorkingCopyMode::InPlace), // Default working copy mode
            description: None,
            models: None,
            agent_type: Some(AgentSoftware::Codex), // Default agent type
            split_mode: Some(SplitMode::None),      // Default split mode
            focus: Some(false),                     // Default focus
            record: Some(true),                     // Default record
            task_id: None,
        }
    }

    /// Set the starting point
    pub fn starting_point(mut self, starting_point: StartingPoint) -> Self {
        self.starting_point = Some(starting_point);
        self
    }

    /// Set the working copy mode
    pub fn working_copy_mode(mut self, working_copy_mode: crate::WorkingCopyMode) -> Self {
        self.working_copy_mode = Some(working_copy_mode);
        self
    }

    /// Set repository and branch (backward compatibility)
    pub fn repository(mut self, repository: String) -> Self {
        // If we already have a starting_point, update it; otherwise create a new one
        if let Some(StartingPoint::RepositoryBranch { branch, .. }) = self.starting_point {
            self.starting_point = Some(StartingPoint::RepositoryBranch { repository, branch });
        } else {
            self.starting_point = Some(StartingPoint::RepositoryBranch {
                repository,
                branch: "main".to_string(), // Default branch
            });
        }
        self
    }

    /// Set the branch (backward compatibility)
    pub fn branch(mut self, branch: String) -> Self {
        // If we already have a starting_point, update it; otherwise create a new one
        if let Some(StartingPoint::RepositoryBranch { repository, .. }) = self.starting_point {
            self.starting_point = Some(StartingPoint::RepositoryBranch { repository, branch });
        } else {
            self.starting_point = Some(StartingPoint::RepositoryBranch {
                repository: "".to_string(), // Empty repository for now
                branch,
            });
        }
        self
    }

    /// Set the description
    pub fn description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }

    /// Set the models
    pub fn agents(mut self, models: Vec<AgentChoice>) -> Self {
        self.models = Some(models);
        self
    }

    /// Set the agent type
    pub fn agent_type(mut self, agent_type: AgentSoftware) -> Self {
        self.agent_type = Some(agent_type);
        self
    }

    /// Set the split mode
    pub fn split_mode(mut self, split_mode: SplitMode) -> Self {
        self.split_mode = Some(split_mode);
        self
    }

    /// Set the focus setting
    pub fn focus(mut self, focus: bool) -> Self {
        self.focus = Some(focus);
        self
    }

    /// Set the record setting
    pub fn record(mut self, record: bool) -> Self {
        self.record = Some(record);
        self
    }

    /// Set the task ID
    pub fn task_id(mut self, task_id: String) -> Self {
        self.task_id = Some(task_id);
        self
    }

    /// Build the TaskLaunchParams with validation
    ///
    /// # Errors
    ///
    /// Returns an error if any required fields are missing or invalid:
    /// - Starting point must be set and valid
    /// - Working copy mode must be set
    /// - Description must be set and non-empty
    /// - Models must be set and non-empty
    /// - Task ID must be set and non-empty
    /// - Repository in starting point must be a valid URL or file path
    pub fn build(self) -> Result<TaskLaunchParams, String> {
        // Extract required fields
        let starting_point =
            self.starting_point.ok_or_else(|| "Starting point is required".to_string())?;
        let working_copy_mode = self
            .working_copy_mode
            .ok_or_else(|| "Working copy mode is required".to_string())?;
        let description = self.description.ok_or_else(|| "Description is required".to_string())?;
        let models = self.models.ok_or_else(|| "Models are required".to_string())?;
        let task_id = self.task_id.ok_or_else(|| "Task ID is required".to_string())?;

        // Use defaults for optional fields
        let agent_type = self.agent_type.unwrap_or(AgentSoftware::Codex);
        let split_mode = self.split_mode.unwrap_or(SplitMode::None);
        let focus = self.focus.unwrap_or(false);
        let record = self.record.unwrap_or(true);

        // Validate description
        if description.trim().is_empty() {
            return Err("Task description cannot be empty".to_string());
        }

        // Validate models
        if models.is_empty() {
            return Err("At least one model must be selected".to_string());
        }

        // Validate task_id
        if task_id.trim().is_empty() {
            return Err("Task ID cannot be empty".to_string());
        }

        // Validate starting point
        match &starting_point {
            StartingPoint::RepositoryBranch { repository, branch } => {
                if repository.trim().is_empty() {
                    return Err("Repository cannot be empty".to_string());
                }
                if branch.trim().is_empty() {
                    return Err("Branch cannot be empty".to_string());
                }
                // Validate repository format (URL or file path)
                if let Err(url_err) = url::Url::parse(repository) {
                    // If URL parsing fails, check if it's a valid file path
                    if let Err(path_err) = std::path::Path::new(repository).canonicalize() {
                        return Err(format!(
                            "Invalid repository: not a valid URL ({}) or file path ({}): {}",
                            url_err, path_err, repository
                        ));
                    }
                }
            }
            StartingPoint::RepositoryCommit { repository, commit } => {
                if repository.trim().is_empty() {
                    return Err("Repository cannot be empty".to_string());
                }
                if commit.trim().is_empty() {
                    return Err("Commit cannot be empty".to_string());
                }
                // Validate repository format (URL or file path)
                if let Err(url_err) = url::Url::parse(repository) {
                    // If URL parsing fails, check if it's a valid file path
                    if let Err(path_err) = std::path::Path::new(repository).canonicalize() {
                        return Err(format!(
                            "Invalid repository: not a valid URL ({}) or file path ({}): {}",
                            url_err, path_err, repository
                        ));
                    }
                }
            }
            StartingPoint::FilesystemSnapshot { snapshot_id } => {
                if snapshot_id.trim().is_empty() {
                    return Err("Snapshot ID cannot be empty".to_string());
                }
            }
        }

        debug!("Starting point: {:?}", starting_point);
        debug!("Working copy mode: {:?}", working_copy_mode);
        debug!("Description: {}", description);
        debug!("Models: {:?}", models);
        debug!("Agent type: {:?}", agent_type);
        debug!("Split mode: {:?}", split_mode);
        debug!("Focus: {:?}", focus);
        debug!("Record: {:?}", record);
        debug!("Task ID: {}", task_id);

        Ok(TaskLaunchParams {
            starting_point,
            working_copy_mode,
            description,
            models,
            agent_type,
            split_mode,
            focus,
            record,
            task_id,
        })
    }
}

impl Default for TaskLaunchParamsBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of a task launch operation
#[derive(Debug, Clone, PartialEq)]
pub enum TaskLaunchResult {
    /// Task launched successfully with the assigned session ID(s)
    Success { session_ids: Vec<String> },
    /// Task launch failed with an error message
    Failure { error: String },
}

impl TaskLaunchResult {
    /// Check if the launch was successful
    pub fn is_success(&self) -> bool {
        matches!(self, TaskLaunchResult::Success { .. })
    }

    /// Get all session IDs if successful
    pub fn session_ids(&self) -> Option<&[String]> {
        match self {
            TaskLaunchResult::Success { session_ids } => Some(session_ids),
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
            TaskLaunchResult::Success { session_ids } => {
                if session_ids.len() == 1 {
                    write!(f, "Session launched successfully: {}", session_ids[0])
                } else {
                    write!(
                        f,
                        "Sessions launched successfully: {}",
                        session_ids.join(", ")
                    )
                }
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
        status: TaskState,
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
    ///
    /// The TaskLaunchParams includes a starting_point field that defines
    /// where the task should start from (repository branch, commit, or snapshot).
    async fn launch_task(&self, params: TaskLaunchParams) -> TaskLaunchResult;

    /// Get a receiver for task events for the given task ID
    ///
    /// Returns a receiver that yields task events as they occur during execution.
    /// The receiver should continue to receive events until the task completes or fails.
    /// Real implementations connect to SSE streams or monitoring systems.
    fn task_events_receiver(&self, task_id: &str) -> tokio::sync::broadcast::Receiver<TaskEvent>;

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
        models: &[AgentChoice],
    ) -> SaveDraftResult;

    /// Get a human-readable description of this task manager
    fn description(&self) -> &str;
}
