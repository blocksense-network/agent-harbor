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
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
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
        status: TaskStatus,
        #[serde(with = "time::serde::rfc3339")]
        ts: time::OffsetDateTime,
    },
    /// Log message from the task execution
    Log {
        level: LogLevel,
        message: String,
        tool_execution_id: Option<String>,
        #[serde(with = "time::serde::rfc3339")]
        ts: time::OffsetDateTime,
    },
    /// Agent thought/reasoning event
    Thought {
        thought: String,
        reasoning: Option<String>,
        #[serde(with = "time::serde::rfc3339")]
        ts: time::OffsetDateTime,
    },
    /// Tool usage started
    ToolUse {
        tool_name: String,
        tool_args: serde_json::Value,
        tool_execution_id: String,
        status: ToolStatus,
        #[serde(with = "time::serde::rfc3339")]
        ts: time::OffsetDateTime,
    },
    /// Tool execution completed
    ToolResult {
        tool_name: String,
        tool_output: String,
        tool_execution_id: String,
        status: ToolStatus,
        #[serde(with = "time::serde::rfc3339")]
        ts: time::OffsetDateTime,
    },
    /// File modification event
    FileEdit {
        file_path: String,
        lines_added: usize,
        lines_removed: usize,
        description: Option<String>,
        #[serde(with = "time::serde::rfc3339")]
        ts: time::OffsetDateTime,
    },
}

/// Task execution status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
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

/// Mock implementation for testing the MVVM architecture
///
/// This implementation simulates task launching without actually executing
/// any real processes or making network calls. It directly modifies the
/// model and view model to simulate the effects of task creation.
///
/// The mock manager:
/// - Generates deterministic task IDs based on input parameters
/// - Simulates successful task creation with configurable delay
/// - Updates the model state to reflect the launched task
/// - Returns appropriate NetworkMsg instances for UI feedback
/// - Provides sophisticated event streams that simulate real agent activity
#[derive(Debug)]
pub struct MockTaskManager {
    /// Whether to simulate failures for testing error handling
    simulate_failures: bool,
    /// Artificial delay in milliseconds for testing async behavior
    delay_ms: u64,
}

impl MockTaskManager {
    /// Create a new mock task manager with default settings
    pub fn new() -> Self {
        Self {
            simulate_failures: false,
            delay_ms: 100, // Small delay to simulate network/process overhead
        }
    }

    /// Create a mock task manager that sometimes fails
    pub fn with_failures(simulate_failures: bool) -> Self {
        Self {
            simulate_failures,
            delay_ms: 100,
        }
    }

    /// Create a mock task manager with custom delay
    pub fn with_delay(delay_ms: u64) -> Self {
        Self {
            simulate_failures: false,
            delay_ms,
        }
    }

    /// Generate a deterministic task ID based on task parameters
    ///
    /// This creates consistent IDs for testing while ensuring uniqueness
    /// based on the combination of repository, branch, and description.
    fn generate_task_id(&self, params: &TaskLaunchParams) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        params.repository.hash(&mut hasher);
        params.branch.hash(&mut hasher);
        params.description.hash(&mut hasher);
        format!("task_{:x}", hasher.finish())
    }

    /// Simulate the task creation process
    ///
    /// In a real implementation, this would:
    /// - Create a new task execution record in the database
    /// - Launch the agent process or send API request
    /// - Set up monitoring and logging
    ///
    /// For the mock, we simulate this with configurable behavior including async delays.
    async fn simulate_task_creation(&self, params: &TaskLaunchParams) -> TaskLaunchResult {
        // Simulate network/process delay
        if self.delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(self.delay_ms)).await;
        }

        // Simulate occasional failures for testing
        if self.simulate_failures && params.description.contains("fail") {
            return TaskLaunchResult::Failure {
                error: "Simulated task creation failure".to_string(),
            };
        }

        // Generate task ID and return success
        let task_id = self.generate_task_id(params);
        TaskLaunchResult::Success { task_id }
    }

    /// Create a sophisticated event stream that simulates real agent activity
    ///
    /// This creates a stream that mimics the behavior seen in the existing main.rs
    /// simulation, with realistic build commands, agent thinking, tool usage,
    /// and file modifications.
    fn create_event_stream(&self, task_id: &str) -> Pin<Box<dyn Stream<Item = TaskEvent> + Send>> {
        let task_id = task_id.to_string();
        Box::pin(futures::stream::unfold(
            MockEventStreamState::new(task_id),
            |mut state| async move {
                tokio::time::sleep(std::time::Duration::from_millis(state.next_delay_ms())).await;
                state.next_event()
            }
        ))
    }
}

/// Internal state for mock event stream generation
struct MockEventStreamState {
    task_id: String,
    event_index: usize,
    start_time: time::OffsetDateTime,
    tool_execution_counter: usize,
    active_tool_executions: std::collections::HashMap<usize, String>,
}

impl MockEventStreamState {
    fn new(task_id: String) -> Self {
        Self {
            task_id,
            event_index: 0,
            start_time: time::OffsetDateTime::now_utc(),
            tool_execution_counter: 0,
            active_tool_executions: HashMap::new(),
        }
    }

    fn next_delay_ms(&self) -> u64 {
        match self.event_index {
            // Status changes are quick
            0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 | 12 | 13 | 14 | 15 => 100,
            // Tool execution lines are fast (50-200ms)
            16..=50 => 50 + (rand::random::<u64>() % 150),
            // Thoughts and file edits are slower (2-5 seconds)
            51..=70 => 2000 + (rand::random::<u64>() % 3000),
            // More tool execution
            71..=100 => 50 + (rand::random::<u64>() % 150),
            // Final status
            _ => 500,
        }
    }

    fn get_or_create_tool_execution_id(&mut self, event_index: usize) -> String {
        if let Some(id) = self.active_tool_executions.get(&event_index) {
            return id.clone();
        }

        self.tool_execution_counter += 1;
        let id = format!("tool_exec_{:04x}", self.tool_execution_counter);
        self.active_tool_executions.insert(event_index, id.clone());
        id
    }

    fn next_event(&mut self) -> Option<(TaskEvent, Self)> {
        let ts = self.start_time + time::Duration::milliseconds((self.event_index as i64) * 500);

        let event = match self.event_index {
            0 => TaskEvent::Status { status: TaskStatus::Queued, ts },
            1 => TaskEvent::Log { level: LogLevel::Info, message: "Task queued for execution".to_string(), tool_execution_id: None, ts },
            2 => TaskEvent::Status { status: TaskStatus::Provisioning, ts },
            3 => TaskEvent::Log { level: LogLevel::Info, message: "Provisioning workspace...".to_string(), tool_execution_id: None, ts },
            4 => TaskEvent::Log { level: LogLevel::Info, message: "Cloning repository...".to_string(), tool_execution_id: None, ts },
            5 => TaskEvent::Log { level: LogLevel::Info, message: "Setting up development environment...".to_string(), tool_execution_id: None, ts },
            6 => TaskEvent::Status { status: TaskStatus::Running, ts },
            7 => TaskEvent::Log { level: LogLevel::Info, message: "Starting agent execution".to_string(), tool_execution_id: None, ts },

            // Initial thoughts
            8 => TaskEvent::Thought {
                thought: "Analyzing the user's request to understand requirements".to_string(),
                reasoning: Some("Need to understand what needs to be implemented".to_string()),
                ts,
            },
            9 => TaskEvent::Thought {
                thought: "Examining the codebase structure and existing patterns".to_string(),
                reasoning: Some("Looking for similar implementations to follow conventions".to_string()),
                ts,
            },

            // Tool usage: cargo check
            10 => {
                let tool_execution_id = self.get_or_create_tool_execution_id(10);
                TaskEvent::ToolUse {
                    tool_name: "cargo".to_string(),
                    tool_args: serde_json::json!(["check"]),
                    tool_execution_id,
                    status: ToolStatus::Started,
                    ts,
                }
            },
            11 => TaskEvent::Log {
                level: LogLevel::Info,
                message: "Running cargo check...".to_string(),
                tool_execution_id: Some(self.get_or_create_tool_execution_id(10)),
                ts,
            },
            12 => TaskEvent::Log {
                level: LogLevel::Info,
                message: "Checking agent-harbor v0.1.0 (/workspace)".to_string(),
                tool_execution_id: Some(self.get_or_create_tool_execution_id(10)),
                ts,
            },
            13 => TaskEvent::Log {
                level: LogLevel::Info,
                message: "Finished dev [unoptimized + debuginfo] target(s) in 8.45s".to_string(),
                tool_execution_id: Some(self.get_or_create_tool_execution_id(10)),
                ts,
            },
            14 => TaskEvent::ToolResult {
                tool_name: "cargo".to_string(),
                tool_output: "Finished dev [unoptimized + debuginfo] target(s) in 8.45s".to_string(),
                tool_execution_id: self.get_or_create_tool_execution_id(10),
                status: ToolStatus::Completed,
                ts,
            },

            // File edits
            15 => TaskEvent::Thought {
                thought: "Implementing the core functionality in main.rs".to_string(),
                reasoning: Some("Starting with the main entry point".to_string()),
                ts,
            },
            16 => TaskEvent::FileEdit {
                file_path: "src/main.rs".to_string(),
                lines_added: 25,
                lines_removed: 5,
                description: Some("Added new functionality and updated imports".to_string()),
                ts,
            },

            // More tool usage: cargo build
            17 => TaskEvent::Thought {
                thought: "Running tests to ensure changes work correctly".to_string(),
                reasoning: Some("Need to verify the implementation before proceeding".to_string()),
                ts,
            },
            18 => {
                let tool_execution_id = self.get_or_create_tool_execution_id(18);
                TaskEvent::ToolUse {
                    tool_name: "cargo".to_string(),
                    tool_args: serde_json::json!(["build"]),
                    tool_execution_id,
                    status: ToolStatus::Started,
                    ts,
                }
            },
            19 => TaskEvent::Log {
                level: LogLevel::Info,
                message: "Compiling agent-harbor v0.1.0 (/workspace)".to_string(),
                tool_execution_id: Some(self.get_or_create_tool_execution_id(18)),
                ts,
            },
            20 => TaskEvent::Log {
                level: LogLevel::Info,
                message: "Compiling serde v1.0.193".to_string(),
                tool_execution_id: Some(self.get_or_create_tool_execution_id(18)),
                ts,
            },
            21 => TaskEvent::Log {
                level: LogLevel::Info,
                message: "Compiling tokio v1.35.1".to_string(),
                tool_execution_id: Some(self.get_or_create_tool_execution_id(18)),
                ts,
            },
            22 => TaskEvent::Log {
                level: LogLevel::Info,
                message: "Compiling ratatui v0.26.0".to_string(),
                tool_execution_id: Some(self.get_or_create_tool_execution_id(18)),
                ts,
            },
            23 => TaskEvent::Log {
                level: LogLevel::Info,
                message: "Compiling crossterm v0.27.0".to_string(),
                tool_execution_id: Some(self.get_or_create_tool_execution_id(18)),
                ts,
            },
            24 => TaskEvent::Log {
                level: LogLevel::Info,
                message: "Compiling reqwest v0.11.22".to_string(),
                tool_execution_id: Some(self.get_or_create_tool_execution_id(18)),
                ts,
            },
            25 => TaskEvent::Log {
                level: LogLevel::Info,
                message: "Compiling sqlx v0.7.3".to_string(),
                tool_execution_id: Some(self.get_or_create_tool_execution_id(18)),
                ts,
            },
            26 => TaskEvent::Log {
                level: LogLevel::Info,
                message: "Compiling clap v4.4.18".to_string(),
                tool_execution_id: Some(self.get_or_create_tool_execution_id(18)),
                ts,
            },
            27 => TaskEvent::Log {
                level: LogLevel::Info,
                message: "Compiling tracing v0.1.40".to_string(),
                tool_execution_id: Some(self.get_or_create_tool_execution_id(18)),
                ts,
            },
            28 => TaskEvent::Log {
                level: LogLevel::Info,
                message: "Compiling thiserror v1.0.50".to_string(),
                tool_execution_id: Some(self.get_or_create_tool_execution_id(18)),
                ts,
            },
            29 => TaskEvent::Log {
                level: LogLevel::Info,
                message: "Compiling agent-harbor v0.1.0 (/workspace)".to_string(),
                tool_execution_id: Some(self.get_or_create_tool_execution_id(18)),
                ts,
            },
            30 => TaskEvent::Log {
                level: LogLevel::Info,
                message: "Finished dev [unoptimized + debuginfo] target(s) in 45.23s".to_string(),
                tool_execution_id: Some(self.get_or_create_tool_execution_id(18)),
                ts,
            },
            31 => TaskEvent::ToolResult {
                tool_name: "cargo".to_string(),
                tool_output: "Finished dev [unoptimized + debuginfo] target(s) in 45.23s".to_string(),
                tool_execution_id: self.get_or_create_tool_execution_id(18),
                status: ToolStatus::Completed,
                ts,
            },

            // More thoughts and file edits
            32 => TaskEvent::Thought {
                thought: "Optimizing database queries for better performance".to_string(),
                reasoning: Some("Analyzing current query patterns and identifying bottlenecks".to_string()),
                ts,
            },
            33 => TaskEvent::FileEdit {
                file_path: "src/database.rs".to_string(),
                lines_added: 15,
                lines_removed: 8,
                description: Some("Optimized query performance and added indexes".to_string()),
                ts,
            },

            // Tool usage: cargo test
            34 => TaskEvent::Thought {
                thought: "Running comprehensive test suite to ensure everything works".to_string(),
                reasoning: Some("Need to validate all changes before completion".to_string()),
                ts,
            },
            35 => {
                let tool_execution_id = self.get_or_create_tool_execution_id(35);
                TaskEvent::ToolUse {
                    tool_name: "cargo".to_string(),
                    tool_args: serde_json::json!(["test"]),
                    tool_execution_id,
                    status: ToolStatus::Started,
                    ts,
                }
            },
            36 => TaskEvent::Log {
                level: LogLevel::Info,
                message: "running 12 tests".to_string(),
                tool_execution_id: Some(self.get_or_create_tool_execution_id(35)),
                ts,
            },
            37 => TaskEvent::Log {
                level: LogLevel::Info,
                message: "test auth::login::test_valid_credentials ... ok".to_string(),
                tool_execution_id: Some(self.get_or_create_tool_execution_id(35)),
                ts,
            },
            38 => TaskEvent::Log {
                level: LogLevel::Info,
                message: "test auth::login::test_invalid_credentials ... ok".to_string(),
                tool_execution_id: Some(self.get_or_create_tool_execution_id(35)),
                ts,
            },
            39 => TaskEvent::Log {
                level: LogLevel::Info,
                message: "test api::users::test_create_user ... ok".to_string(),
                tool_execution_id: Some(self.get_or_create_tool_execution_id(35)),
                ts,
            },
            40 => TaskEvent::Log {
                level: LogLevel::Info,
                message: "test api::users::test_get_user ... ok".to_string(),
                tool_execution_id: Some(self.get_or_create_tool_execution_id(35)),
                ts,
            },
            41 => TaskEvent::Log {
                level: LogLevel::Info,
                message: "test api::projects::test_create_project ... ok".to_string(),
                tool_execution_id: Some(self.get_or_create_tool_execution_id(35)),
                ts,
            },
            42 => TaskEvent::Log {
                level: LogLevel::Info,
                message: "test api::projects::test_list_projects ... ok".to_string(),
                tool_execution_id: Some(self.get_or_create_tool_execution_id(35)),
                ts,
            },
            43 => TaskEvent::Log {
                level: LogLevel::Info,
                message: "test database::queries::test_optimization ... ok".to_string(),
                tool_execution_id: Some(self.get_or_create_tool_execution_id(35)),
                ts,
            },
            44 => TaskEvent::Log {
                level: LogLevel::Info,
                message: "test database::queries::test_performance ... ok".to_string(),
                tool_execution_id: Some(self.get_or_create_tool_execution_id(35)),
                ts,
            },
            45 => TaskEvent::Log {
                level: LogLevel::Info,
                message: "test api::middleware::test_auth_middleware ... ok".to_string(),
                tool_execution_id: Some(self.get_or_create_tool_execution_id(35)),
                ts,
            },
            46 => TaskEvent::Log {
                level: LogLevel::Info,
                message: "test utils::validation::test_input_sanitization ... ok".to_string(),
                tool_execution_id: Some(self.get_or_create_tool_execution_id(35)),
                ts,
            },
            47 => TaskEvent::Log {
                level: LogLevel::Info,
                message: "test config::loading::test_env_vars ... ok".to_string(),
                tool_execution_id: Some(self.get_or_create_tool_execution_id(35)),
                ts,
            },
            48 => TaskEvent::Log {
                level: LogLevel::Info,
                message: "test config::loading::test_file_config ... ok".to_string(),
                tool_execution_id: Some(self.get_or_create_tool_execution_id(35)),
                ts,
            },
            49 => TaskEvent::Log {
                level: LogLevel::Info,
                message: "test utils::logging::test_structured_logs ... ok".to_string(),
                tool_execution_id: Some(self.get_or_create_tool_execution_id(35)),
                ts,
            },
            50 => TaskEvent::ToolResult {
                tool_name: "cargo".to_string(),
                tool_output: "test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out".to_string(),
                tool_execution_id: self.get_or_create_tool_execution_id(35),
                status: ToolStatus::Completed,
                ts,
            },

            // Final thoughts and completion
            51 => TaskEvent::Thought {
                thought: "All tests passing, implementation is complete".to_string(),
                reasoning: Some("Successfully implemented all requested features".to_string()),
                ts,
            },
            52 => TaskEvent::FileEdit {
                file_path: "README.md".to_string(),
                lines_added: 10,
                lines_removed: 2,
                description: Some("Updated documentation with new features".to_string()),
                ts,
            },
            53 => TaskEvent::Status { status: TaskStatus::Completed, ts },
            54 => TaskEvent::Log { level: LogLevel::Info, message: "Task completed successfully".to_string(), tool_execution_id: None, ts },

            // End of stream
            _ => return None,
        };

        self.event_index += 1;
        Some((event, Self {
            task_id: self.task_id.clone(),
            event_index: self.event_index,
            start_time: self.start_time,
            tool_execution_counter: self.tool_execution_counter,
            active_tool_executions: self.active_tool_executions.clone(),
        }))
    }
}

#[async_trait]
impl TaskManager for MockTaskManager {
    async fn launch_task(&self, params: TaskLaunchParams) -> TaskLaunchResult {
        // Validate parameters (same validation as real implementations)
        if params.description.trim().is_empty() {
            return TaskLaunchResult::Failure {
                error: "Task description cannot be empty".to_string(),
            };
        }

        if params.models.is_empty() {
            return TaskLaunchResult::Failure {
                error: "At least one model must be selected".to_string(),
            };
        }

        if params.repository.trim().is_empty() {
            return TaskLaunchResult::Failure {
                error: "Repository cannot be empty".to_string(),
            };
        }

        if params.branch.trim().is_empty() {
            return TaskLaunchResult::Failure {
                error: "Branch cannot be empty".to_string(),
            };
        }

        // Simulate the actual task creation with async behavior
        self.simulate_task_creation(&params).await
    }

    fn task_events_stream(&self, task_id: &str) -> Pin<Box<dyn Stream<Item = TaskEvent> + Send>> {
        self.create_event_stream(task_id)
    }

    async fn get_initial_tasks(&self) -> (Vec<TaskInfo>, Vec<TaskInfo>) {
        // Simulate some initial tasks with configurable delay
        if self.delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(self.delay_ms)).await;
        }

        // Return (drafts, tasks)
        let drafts = vec![
            TaskInfo {
                id: "draft_001".to_string(),
                title: "Implement user authentication".to_string(),
                status: "draft".to_string(),
                repository: "myapp/backend".to_string(),
                branch: "main".to_string(),
                created_at: "2024-01-15T10:30:00Z".to_string(),
                models: vec!["Claude 3.5 Sonnet".to_string()],
            },
        ];

        let tasks = vec![
            TaskInfo {
                id: "task_001".to_string(),
                title: "Implement user authentication system".to_string(),
                status: "running".to_string(),
                repository: "myapp/backend".to_string(),
                branch: "feature/auth".to_string(),
                created_at: "2024-01-16T14:20:00Z".to_string(),
                models: vec!["Claude 3.5 Sonnet".to_string()],
            },
            TaskInfo {
                id: "task_002".to_string(),
                title: "Set up CI/CD pipeline".to_string(),
                status: "running".to_string(),
                repository: "myapp/backend".to_string(),
                branch: "main".to_string(),
                created_at: "2024-01-17T09:15:00Z".to_string(),
                models: vec!["Claude 3 Opus".to_string()],
            },
            TaskInfo {
                id: "task_003".to_string(),
                title: "Add database migrations".to_string(),
                status: "completed".to_string(),
                repository: "myapp/backend".to_string(),
                branch: "feature/db-migrations".to_string(),
                created_at: "2024-01-15T10:30:00Z".to_string(),
                models: vec!["GPT-4".to_string()],
            },
            TaskInfo {
                id: "task_004".to_string(),
                title: "Refactor API endpoints".to_string(),
                status: "merged".to_string(),
                repository: "myapp/frontend".to_string(),
                branch: "feature/api-refactor".to_string(),
                created_at: "2024-01-14T16:45:00Z".to_string(),
                models: vec!["Claude 3 Opus".to_string()],
            },
        ];

        (drafts, tasks)
    }

    async fn save_draft_task(&self, draft_id: &str, description: &str, repository: &str, branch: &str, models: &[SelectedModel]) -> SaveDraftResult {
        // Simulate auto-save with configurable delay
        if self.delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(self.delay_ms)).await;
        }

        // Simulate occasional save failures for testing
        if self.simulate_failures && description.contains("fail") {
            return SaveDraftResult::Failure {
                error: "Simulated save failure".to_string(),
            };
        }

        // In a real implementation, this would persist to a database
        SaveDraftResult::Success
    }

    async fn list_repositories(&self) -> Vec<Repository> {
        // Simulate repository listing with configurable delay
        if self.delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(self.delay_ms)).await;
        }

        vec![
            Repository {
                id: "repo_001".to_string(),
                name: "myapp/backend".to_string(),
                url: "https://github.com/user/myapp-backend".to_string(),
                default_branch: "main".to_string(),
            },
            Repository {
                id: "repo_002".to_string(),
                name: "myapp/frontend".to_string(),
                url: "https://github.com/user/myapp-frontend".to_string(),
                default_branch: "main".to_string(),
            },
            Repository {
                id: "repo_003".to_string(),
                name: "myapp/mobile".to_string(),
                url: "https://github.com/user/myapp-mobile".to_string(),
                default_branch: "develop".to_string(),
            },
        ]
    }

    async fn list_branches(&self, repository_id: &str) -> Vec<Branch> {
        // Simulate branch listing with configurable delay
        if self.delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(self.delay_ms)).await;
        }

        // Return different branches based on repository
        match repository_id {
            "repo_001" => vec![
                Branch {
                    name: "main".to_string(),
                    is_default: true,
                    last_commit: Some("abc123".to_string()),
                },
                Branch {
                    name: "develop".to_string(),
                    is_default: false,
                    last_commit: Some("def456".to_string()),
                },
                Branch {
                    name: "feature/auth".to_string(),
                    is_default: false,
                    last_commit: Some("ghi789".to_string()),
                },
            ],
            "repo_002" => vec![
                Branch {
                    name: "main".to_string(),
                    is_default: true,
                    last_commit: Some("jkl012".to_string()),
                },
                Branch {
                    name: "feature/ui".to_string(),
                    is_default: false,
                    last_commit: Some("mno345".to_string()),
                },
            ],
            "repo_003" => vec![
                Branch {
                    name: "develop".to_string(),
                    is_default: true,
                    last_commit: Some("pqr678".to_string()),
                },
                Branch {
                    name: "main".to_string(),
                    is_default: false,
                    last_commit: Some("stu901".to_string()),
                },
            ],
            _ => vec![],
        }
    }

    fn description(&self) -> &str {
        "Mock Task Manager (for testing MVVM architecture)"
    }
}

impl Default for MockTaskManager {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for TaskLaunchResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskLaunchResult::Success { task_id } => {
                write!(f, "Task launched successfully: {}", task_id)
            }
            TaskLaunchResult::Failure { error } => {
                write!(f, "Task launch failed: {}", error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_task_manager_launches_successful_task() {
        let manager = MockTaskManager::new();
        let params = TaskLaunchParams {
            repository: "test/repo".to_string(),
            branch: "main".to_string(),
            description: "Test task".to_string(),
            models: vec![SelectedModel {
                name: "Claude".to_string(),
                count: 1,
            }],
        };

        let result = manager.launch_task(params).await;

        assert!(result.is_success());
        assert!(result.task_id().unwrap().starts_with("task_"));
    }

    #[tokio::test]
    async fn mock_task_manager_validates_empty_description() {
        let manager = MockTaskManager::new();
        let params = TaskLaunchParams {
            repository: "test/repo".to_string(),
            branch: "main".to_string(),
            description: "".to_string(),
            models: vec![SelectedModel {
                name: "Claude".to_string(),
                count: 1,
            }],
        };

        let result = manager.launch_task(params).await;

        assert!(!result.is_success());
        assert_eq!(result.error().unwrap(), "Task description cannot be empty");
    }

    #[tokio::test]
    async fn mock_task_manager_validates_empty_models() {
        let manager = MockTaskManager::new();
        let params = TaskLaunchParams {
            repository: "test/repo".to_string(),
            branch: "main".to_string(),
            description: "Test task".to_string(),
            models: vec![],
        };

        let result = manager.launch_task(params).await;

        assert!(!result.is_success());
        assert_eq!(result.error().unwrap(), "At least one model must be selected");
    }

    #[tokio::test]
    async fn mock_task_manager_handles_simulated_failures() {
        let manager = MockTaskManager::with_failures(true);
        let params = TaskLaunchParams {
            repository: "test/repo".to_string(),
            branch: "main".to_string(),
            description: "This task will fail".to_string(),
            models: vec![SelectedModel {
                name: "Claude".to_string(),
                count: 1,
            }],
        };

        let result = manager.launch_task(params).await;

        assert!(!result.is_success());
        assert_eq!(result.error().unwrap(), "Simulated task creation failure");
    }

    #[tokio::test]
    async fn mock_task_manager_generates_deterministic_task_ids() {
        let manager = MockTaskManager::new();
        let params1 = TaskLaunchParams {
            repository: "test/repo".to_string(),
            branch: "main".to_string(),
            description: "Test task".to_string(),
            models: vec![SelectedModel {
                name: "Claude".to_string(),
                count: 1,
            }],
        };

        let params2 = params1.clone();

        let result1 = manager.launch_task(params1).await;
        let result2 = manager.launch_task(params2).await;

        // Same parameters should generate same task ID
        assert_eq!(result1.task_id(), result2.task_id());
    }

    #[test]
    fn task_launch_result_display_formats_correctly() {
        let success = TaskLaunchResult::Success {
            task_id: "task_123".to_string(),
        };
        let failure = TaskLaunchResult::Failure {
            error: "Something went wrong".to_string(),
        };

        assert_eq!(format!("{}", success), "Task launched successfully: task_123");
        assert_eq!(format!("{}", failure), "Task launch failed: Something went wrong");
    }

    #[tokio::test]
    async fn mock_task_manager_get_initial_tasks() {
        let manager = MockTaskManager::new();
        let (drafts, tasks) = manager.get_initial_tasks().await;

        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].id, "draft_001");
        assert_eq!(drafts[0].title, "Implement user authentication");

        assert_eq!(tasks.len(), 4);
        assert_eq!(tasks[0].id, "task_001");
        assert_eq!(tasks[0].status, "running");
        assert_eq!(tasks[1].id, "task_002");
        assert_eq!(tasks[1].status, "running");
        assert_eq!(tasks[2].id, "task_003");
        assert_eq!(tasks[2].status, "completed");
        assert_eq!(tasks[3].id, "task_004");
        assert_eq!(tasks[3].status, "merged");
    }

    #[tokio::test]
    async fn mock_task_manager_save_draft_task_success() {
        let manager = MockTaskManager::new();
        let models = vec![SelectedModel {
            name: "Claude".to_string(),
            count: 1,
        }];

        let result = manager.save_draft_task(
            "draft_001",
            "Test description",
            "test/repo",
            "main",
            &models,
        ).await;

        assert!(matches!(result, SaveDraftResult::Success));
    }

    #[tokio::test]
    async fn mock_task_manager_save_draft_task_failure() {
        let manager = MockTaskManager::with_failures(true);
        let models = vec![SelectedModel {
            name: "Claude".to_string(),
            count: 1,
        }];

        let result = manager.save_draft_task(
            "draft_001",
            "This will fail",
            "test/repo",
            "main",
            &models,
        ).await;

        assert!(matches!(result, SaveDraftResult::Failure { .. }));
    }

    #[tokio::test]
    async fn mock_task_manager_list_repositories() {
        let manager = MockTaskManager::new();
        let repos = manager.list_repositories().await;

        assert_eq!(repos.len(), 3);
        assert_eq!(repos[0].name, "myapp/backend");
        assert_eq!(repos[1].name, "myapp/frontend");
        assert_eq!(repos[2].name, "myapp/mobile");
    }

    #[tokio::test]
    async fn mock_task_manager_list_branches() {
        let manager = MockTaskManager::new();

        let branches = manager.list_branches("repo_001").await;
        assert_eq!(branches.len(), 3);
        assert_eq!(branches[0].name, "main");
        assert!(branches[0].is_default);

        let branches = manager.list_branches("repo_002").await;
        assert_eq!(branches.len(), 2);

        let branches = manager.list_branches("unknown_repo").await;
        assert_eq!(branches.len(), 0);
    }

    #[tokio::test]
    async fn mock_task_manager_time_simulation_with_accelerated_execution() {
        // This test demonstrates how to use Tokio's time utilities for accelerated testing
        // We can pause time, advance it manually, and control the execution order

        tokio::time::pause();

        let manager = MockTaskManager::with_delay(1000); // 1 second delay for operations

        // Start multiple async operations that would normally take time
        let initial_tasks_future = manager.get_initial_tasks();
        let repos_future = manager.list_repositories();

        // At this point, time is paused, so no operations have actually executed yet
        // We can advance time selectively to control execution order

        // Advance time by 500ms - initial_tasks_future should still be pending
        // repos_future should still be pending
        tokio::time::advance(std::time::Duration::from_millis(500)).await;

        // Both futures should still be pending since they require 1000ms delay
        // In a real test, we'd use tokio::select! or futures::poll_fn to check this

        // Advance time by another 600ms (total 1100ms)
        tokio::time::advance(std::time::Duration::from_millis(600)).await;

        // Now both operations should be able to complete
        let ((drafts, tasks), repos) = tokio::join!(initial_tasks_future, repos_future);

        assert_eq!(drafts.len() + tasks.len(), 5);
        assert_eq!(repos.len(), 3);

        // Resume normal time
        tokio::time::resume();
    }

    #[tokio::test]
    async fn mock_task_manager_concurrent_operations_with_time_control() {
        // Demonstrate controlling the timing of concurrent operations
        tokio::time::pause();

        let fast_manager = MockTaskManager::with_delay(100); // Fast operations
        let slow_manager = MockTaskManager::with_delay(1000); // Slow operations

        // Start a fast operation and a slow operation concurrently
        let fast_task = tokio::spawn(async move {
            fast_manager.get_initial_tasks().await
        });

        let slow_task = tokio::spawn(async move {
            slow_manager.list_repositories().await
        });

        // Advance time by 200ms - fast operation should complete, slow should still be pending
        tokio::time::advance(std::time::Duration::from_millis(200)).await;

        // Fast operation should be done
        let (drafts, tasks) = fast_task.await.unwrap();
        assert_eq!(drafts.len() + tasks.len(), 5);

        // Slow operation should still be running
        // (In a real test, we'd check that slow_task is still pending)

        // Advance time by another 900ms (total 1100ms) - slow operation should complete
        tokio::time::advance(std::time::Duration::from_millis(900)).await;

        let slow_result = slow_task.await.unwrap();
        assert_eq!(slow_result.len(), 3);

        tokio::time::resume();
    }
}
