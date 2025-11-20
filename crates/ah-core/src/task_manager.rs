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

    // Advanced launch options
    sandbox_profile: Option<String>,
    fs_snapshots: Option<String>,
    devcontainer_path: Option<String>,
    allow_egress: Option<bool>,
    allow_containers: Option<bool>,
    allow_vms: Option<bool>,
    allow_web_search: Option<bool>,
    interactive_mode: Option<bool>,
    output_format: Option<String>,
    record_output: Option<bool>,
    timeout: Option<String>,
    llm_provider: Option<String>,
    environment_variables: Option<Vec<(String, String)>>,
    delivery_method: Option<String>,
    target_branch: Option<String>,
    create_task_files: Option<bool>,
    create_metadata_commits: Option<bool>,
    notifications: Option<bool>,
    labels: Option<Vec<(String, String)>>,
    fleet: Option<String>,
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

    // Advanced launch options
    sandbox_profile: Option<String>,
    fs_snapshots: Option<String>,
    devcontainer_path: Option<String>,
    allow_egress: Option<bool>,
    allow_containers: Option<bool>,
    allow_vms: Option<bool>,
    allow_web_search: Option<bool>,
    interactive_mode: Option<bool>,
    output_format: Option<String>,
    record_output: Option<bool>,
    timeout: Option<String>,
    llm_provider: Option<String>,
    environment_variables: Option<Vec<(String, String)>>,
    delivery_method: Option<String>,
    target_branch: Option<String>,
    create_task_files: Option<bool>,
    create_metadata_commits: Option<bool>,
    notifications: Option<bool>,
    labels: Option<Vec<(String, String)>>,
    fleet: Option<String>,
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

    // Advanced launch options getters
    pub fn sandbox_profile(&self) -> Option<&str> {
        self.sandbox_profile.as_deref()
    }

    pub fn fs_snapshots(&self) -> Option<&str> {
        self.fs_snapshots.as_deref()
    }

    pub fn devcontainer_path(&self) -> Option<&str> {
        self.devcontainer_path.as_deref()
    }

    pub fn allow_egress(&self) -> Option<bool> {
        self.allow_egress
    }

    pub fn allow_containers(&self) -> Option<bool> {
        self.allow_containers
    }

    pub fn allow_vms(&self) -> Option<bool> {
        self.allow_vms
    }

    pub fn allow_web_search(&self) -> Option<bool> {
        self.allow_web_search
    }

    pub fn interactive_mode(&self) -> Option<bool> {
        self.interactive_mode
    }

    pub fn output_format(&self) -> Option<&str> {
        self.output_format.as_deref()
    }

    pub fn record_output(&self) -> Option<bool> {
        self.record_output
    }

    pub fn timeout(&self) -> Option<&str> {
        self.timeout.as_deref()
    }

    pub fn llm_provider(&self) -> Option<&str> {
        self.llm_provider.as_deref()
    }

    pub fn environment_variables(&self) -> Option<&[(String, String)]> {
        self.environment_variables.as_deref()
    }

    pub fn delivery_method(&self) -> Option<&str> {
        self.delivery_method.as_deref()
    }

    pub fn target_branch(&self) -> Option<&str> {
        self.target_branch.as_deref()
    }

    pub fn create_task_files(&self) -> Option<bool> {
        self.create_task_files
    }

    pub fn create_metadata_commits(&self) -> Option<bool> {
        self.create_metadata_commits
    }

    pub fn notifications(&self) -> Option<bool> {
        self.notifications
    }

    pub fn labels(&self) -> Option<&[(String, String)]> {
        self.labels.as_deref()
    }

    pub fn fleet(&self) -> Option<&str> {
        self.fleet.as_deref()
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

            // Advanced launch options defaults
            sandbox_profile: None,
            fs_snapshots: None,
            devcontainer_path: None,
            allow_egress: None,
            allow_containers: None,
            allow_vms: None,
            allow_web_search: None,
            interactive_mode: None,
            output_format: None,
            record_output: None,
            timeout: None,
            llm_provider: None,
            environment_variables: None,
            delivery_method: None,
            target_branch: None,
            create_task_files: None,
            create_metadata_commits: None,
            notifications: None,
            labels: None,
            fleet: None,
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

    /// Set the sandbox profile
    pub fn sandbox_profile(mut self, sandbox_profile: String) -> Self {
        self.sandbox_profile = Some(sandbox_profile);
        self
    }

    /// Set the filesystem snapshots
    /// TODO: Map to --fs-snapshots CLI flag when implemented
    pub fn fs_snapshots(mut self, fs_snapshots: String) -> Self {
        self.fs_snapshots = Some(fs_snapshots);
        self
    }

    /// Set the devcontainer path
    /// TODO: Map to --devcontainer-path CLI flag when implemented
    pub fn devcontainer_path(mut self, devcontainer_path: String) -> Self {
        self.devcontainer_path = Some(devcontainer_path);
        self
    }

    /// Set allow egress
    pub fn allow_egress(mut self, allow_egress: bool) -> Self {
        self.allow_egress = Some(allow_egress);
        self
    }

    /// Set allow containers
    pub fn allow_containers(mut self, allow_containers: bool) -> Self {
        self.allow_containers = Some(allow_containers);
        self
    }

    /// Set allow VMs
    pub fn allow_vms(mut self, allow_vms: bool) -> Self {
        self.allow_vms = Some(allow_vms);
        self
    }

    /// Set allow web search
    pub fn allow_web_search(mut self, allow_web_search: bool) -> Self {
        self.allow_web_search = Some(allow_web_search);
        self
    }

    /// Set interactive mode
    pub fn interactive_mode(mut self, interactive_mode: bool) -> Self {
        self.interactive_mode = Some(interactive_mode);
        self
    }

    /// Set output format
    pub fn output_format(mut self, output_format: String) -> Self {
        self.output_format = Some(output_format);
        self
    }

    /// Set record output
    /// TODO: Map to --no-record-output CLI flag when implemented
    pub fn record_output(mut self, record_output: bool) -> Self {
        self.record_output = Some(record_output);
        self
    }

    /// Set timeout
    /// TODO: Map to --timeout CLI flag when implemented
    pub fn timeout(mut self, timeout: String) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Set LLM provider
    pub fn llm_provider(mut self, llm_provider: String) -> Self {
        self.llm_provider = Some(llm_provider);
        self
    }

    /// Set environment variables
    pub fn environment_variables(mut self, environment_variables: Vec<(String, String)>) -> Self {
        self.environment_variables = Some(environment_variables);
        self
    }

    /// Set delivery method
    /// TODO: Map to --delivery-method CLI flag when implemented
    pub fn delivery_method(mut self, delivery_method: String) -> Self {
        self.delivery_method = Some(delivery_method);
        self
    }

    /// Set target branch
    /// TODO: Map to --target-branch CLI flag when implemented
    pub fn target_branch(mut self, target_branch: String) -> Self {
        self.target_branch = Some(target_branch);
        self
    }

    /// Set create task files
    /// TODO: Map to --no-create-task-files CLI flag when implemented
    pub fn create_task_files(mut self, create_task_files: bool) -> Self {
        self.create_task_files = Some(create_task_files);
        self
    }

    /// Set create metadata commits
    /// TODO: Map to --no-create-metadata-commits CLI flag when implemented
    pub fn create_metadata_commits(mut self, create_metadata_commits: bool) -> Self {
        self.create_metadata_commits = Some(create_metadata_commits);
        self
    }

    /// Set notifications
    /// TODO: Map to --notifications CLI flag when implemented
    pub fn notifications(mut self, notifications: bool) -> Self {
        self.notifications = Some(notifications);
        self
    }

    /// Set labels
    /// TODO: Map to --label KEY=VALUE CLI flag when implemented
    pub fn labels(mut self, labels: Vec<(String, String)>) -> Self {
        self.labels = Some(labels);
        self
    }

    /// Set fleet
    /// TODO: Map to --fleet CLI flag when implemented
    pub fn fleet(mut self, fleet: String) -> Self {
        self.fleet = Some(fleet);
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

            // Advanced launch options
            sandbox_profile: self.sandbox_profile,
            fs_snapshots: self.fs_snapshots,
            devcontainer_path: self.devcontainer_path,
            allow_egress: self.allow_egress,
            allow_containers: self.allow_containers,
            allow_vms: self.allow_vms,
            allow_web_search: self.allow_web_search,
            interactive_mode: self.interactive_mode,
            output_format: self.output_format,
            record_output: self.record_output,
            timeout: self.timeout,
            llm_provider: self.llm_provider,
            environment_variables: self.environment_variables,
            delivery_method: self.delivery_method,
            target_branch: self.target_branch,
            create_task_files: self.create_task_files,
            create_metadata_commits: self.create_metadata_commits,
            notifications: self.notifications,
            labels: self.labels,
            fleet: self.fleet,
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
