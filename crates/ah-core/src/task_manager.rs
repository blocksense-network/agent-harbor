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
use ah_local_db::models::DraftRecord;
use crate::db::DatabaseManager;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use serde_json;
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

/// Local task manager implementation that spawns agent processes directly
///
/// This implementation runs agents directly from the current filesystem state,
/// without the snapshot caching complexity used by the server. It's suitable for
/// local development and testing where users want to run agents immediately.
pub struct LocalTaskManager {
    agent_executor: std::sync::Arc<crate::AgentExecutor>,
    db_manager: DatabaseManager,
}

impl LocalTaskManager {
    /// Create a new local task manager
    pub fn new(config: crate::AgentExecutionConfig) -> anyhow::Result<Self> {
        let agent_executor = std::sync::Arc::new(crate::AgentExecutor::new(config));
        let db_manager = DatabaseManager::new()?;
        Ok(Self { agent_executor, db_manager })
    }
}

#[async_trait]
impl TaskManager for LocalTaskManager {
    async fn launch_task(&self, params: TaskLaunchParams) -> TaskLaunchResult {
        // For local mode, we run agents directly from the current filesystem state
        // without snapshot caching. This is simpler and more direct for local development.

        let session_id = uuid::Uuid::new_v4().to_string();
        let current_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

        // Spawn the agent process directly (no snapshot caching for local mode)
        match self.agent_executor.spawn_agent_process(
            &session_id,
            "codex", // Default agent type for local mode
            &params.description,
            crate::WorkingCopyMode::InPlace, // Local mode uses in-place working copy
            Some(&current_dir),
            None, // No snapshot for local mode
        ).await {
            Ok(_handle) => {
                // In local mode, we don't track the task lifecycle as tightly
                // The agent runs and completes, but we don't wait for it here
                TaskLaunchResult::Success {
                    task_id: session_id,
                }
            }
            Err(e) => {
                TaskLaunchResult::Failure {
                    error: format!("Failed to spawn agent process: {}", e),
                }
            }
        }
    }

    fn task_events_stream(&self, _task_id: &str) -> Pin<Box<dyn Stream<Item = TaskEvent> + Send>> {
        // TODO: Implement event streaming from local agent processes
        Box::pin(futures::stream::empty())
    }

    async fn get_initial_tasks(&self) -> (Vec<TaskInfo>, Vec<TaskInfo>) {
        // Get draft tasks from the database
        match self.db_manager.list_drafts() {
            Ok(draft_records) => {
                let draft_tasks: Vec<TaskInfo> = draft_records.into_iter().map(|draft| {
                    // Parse models JSON
                    let models = match serde_json::from_str::<Vec<ah_domain_types::SelectedModel>>(&draft.models) {
                        Ok(models) => models.iter().map(|m| m.name.clone()).collect(),
                        Err(_) => Vec::new(),
                    };

                    TaskInfo {
                        id: draft.id,
                        title: draft.description,
                        status: "draft".to_string(),
                        repository: draft.repository,
                        branch: draft.branch.unwrap_or_default(),
                        created_at: draft.created_at,
                        models,
                    }
                }).collect();

                // For completed tasks, return empty for now (could be extended to get from database)
                (draft_tasks, Vec::new())
            }
            Err(e) => {
                tracing::warn!("Failed to list drafts: {}", e);
                (Vec::new(), Vec::new())
            }
        }
    }

    async fn save_draft_task(
        &self,
        draft_id: &str,
        description: &str,
        repository: &str,
        branch: &str,
        models: &[SelectedModel],
    ) -> SaveDraftResult {
        // Convert models to JSON string
        let models_json = match serde_json::to_string(models) {
            Ok(json) => json,
            Err(e) => {
                return SaveDraftResult::Failure {
                    error: format!("Failed to serialize models: {}", e),
                };
            }
        };

        let now = chrono::Utc::now().to_rfc3339();
        let draft_record = DraftRecord {
            id: draft_id.to_string(),
            description: description.to_string(),
            repository: repository.to_string(),
            branch: Some(branch.to_string()),
            models: models_json,
            created_at: now.clone(),
            updated_at: now,
        };

        match self.db_manager.save_draft(&draft_record) {
            Ok(()) => SaveDraftResult::Success,
            Err(e) => SaveDraftResult::Failure {
                error: format!("Failed to save draft: {}", e),
            },
        }
    }

    async fn list_repositories(&self) -> Vec<Repository> {
        // Get repositories from the local database
        match self.db_manager.list_repositories() {
            Ok(repos) => repos.into_iter().map(|repo_record| {
                let remote_url = repo_record.remote_url.as_ref();
                let root_path = repo_record.root_path.as_ref();
                Repository {
                    id: repo_record.id.to_string(),
                    name: remote_url.unwrap_or(&root_path.unwrap_or(&"Unknown".to_string())).clone(),
                    url: remote_url.unwrap_or(&"".to_string()).clone(),
                    default_branch: repo_record.default_branch.unwrap_or_else(|| "main".to_string()),
                }
            }).collect(),
            Err(e) => {
                tracing::warn!("Failed to list repositories: {}", e);
                Vec::new()
            }
        }
    }

    async fn list_branches(&self, repository_id: &str) -> Vec<Branch> {
        // Parse repository ID as integer to get repo info from database
        match repository_id.parse::<i64>() {
            Ok(repo_id) => {
                match self.db_manager.get_repository_by_id(repo_id) {
                    Ok(Some(repo_record)) => {
                        if let Some(root_path) = repo_record.root_path {
                            // Use ah-repo to get branches from the actual repository
                            match ah_repo::VcsRepo::new(&root_path) {
                                Ok(repo) => {
                                    match repo.branches() {
                                        Ok(branch_names) => {
                                            let default_branch = repo_record.default_branch.unwrap_or_else(|| "main".to_string());
                                            branch_names.into_iter().map(|name| Branch {
                                                name: name.clone(),
                                                is_default: name == default_branch,
                                                last_commit: None, // Could be populated if needed
                                            }).collect()
                                        }
                                        Err(e) => {
                                            tracing::warn!("Failed to get branches for repository {}: {}", repository_id, e);
                                            Vec::new()
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!("Failed to open repository at {}: {}", root_path, e);
                                    Vec::new()
                                }
                            }
                        } else {
                            tracing::warn!("Repository {} has no root path", repository_id);
                            Vec::new()
                        }
                    }
                    Ok(None) => {
                        tracing::warn!("Repository {} not found", repository_id);
                        Vec::new()
                    }
                    Err(e) => {
                        tracing::warn!("Failed to get repository {}: {}", repository_id, e);
                        Vec::new()
                    }
                }
            }
            Err(_) => {
                tracing::warn!("Invalid repository ID: {}", repository_id);
                Vec::new()
            }
        }
    }

    fn description(&self) -> &str {
        "Local Task Manager - executes tasks directly on this machine"
    }
}
