//! Mock REST client implementing TaskManager trait for testing
//!
//! This crate provides a mock implementation of the TaskManager trait
//! that simulates task execution without making actual network calls.
//! It's designed for testing the TUI and other components with realistic
//! behavior and configurable delays.

use ah_core::{
    TaskManager, TaskLaunchParams, TaskLaunchResult, TaskEvent, TaskExecutionStatus,
    LogLevel, ToolStatus, SaveDraftResult
};
use ah_domain_types::{Repository, Branch, TaskInfo, SelectedModel};
use async_trait::async_trait;
use futures::stream::{self, Stream};
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Mock REST client implementing the TaskManager trait
#[derive(Debug)]
pub struct MockRestClient {
    /// In-memory storage for tasks
    tasks: Arc<RwLock<HashMap<String, TaskInfo>>>,
    /// In-memory storage for drafts
    drafts: Arc<RwLock<HashMap<String, TaskInfo>>>,
    /// Configurable delay for operations (in milliseconds)
    delay_ms: u64,
    /// Whether to simulate failures
    simulate_failures: bool,
    /// Next task ID counter
    next_task_id: Arc<RwLock<u64>>,
}

impl MockRestClient {
    /// Create a new mock client with default settings
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
            drafts: Arc::new(RwLock::new(HashMap::new())),
            delay_ms: 100,
            simulate_failures: false,
            next_task_id: Arc::new(RwLock::new(1)),
        }
    }

    /// Create a mock client with custom delay
    pub fn with_delay(delay_ms: u64) -> Self {
        Self {
            delay_ms,
            ..Self::new()
        }
    }

    /// Create a mock client that simulates failures
    pub fn with_failures(simulate_failures: bool) -> Self {
        Self {
            simulate_failures,
            ..Self::new()
        }
    }

    /// Generate a deterministic task ID
    fn generate_task_id(&self, params: &TaskLaunchParams) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        params.repository.hash(&mut hasher);
        params.branch.hash(&mut hasher);
        params.description.hash(&mut hasher);
        format!("task_{:x}", hasher.finish())
    }

    /// Generate a unique draft ID
    async fn generate_draft_id(&self) -> String {
        let mut counter = self.next_task_id.write().await;
        let id = *counter;
        *counter += 1;
        format!("draft_{}", id)
    }

    /// Create realistic event stream for a task
    fn create_event_stream(&self, task_id: &str) -> Pin<Box<dyn Stream<Item = TaskEvent> + Send>> {
        let task_id = task_id.to_string();
        let delay_ms = self.delay_ms;

        Box::pin(stream::unfold(
            MockEventState::new(task_id, delay_ms),
            |mut state| async move {
                tokio::time::sleep(std::time::Duration::from_millis(state.next_delay_ms())).await;
                state.next_event()
            }
        ))
    }

    /// Validate task launch parameters
    fn validate_params(&self, params: &TaskLaunchParams) -> Result<(), String> {
        if params.description.trim().is_empty() {
            return Err("Task description cannot be empty".to_string());
        }
        if params.models.is_empty() {
            return Err("At least one model must be selected".to_string());
        }
        if params.repository.trim().is_empty() {
            return Err("Repository cannot be empty".to_string());
        }
        if params.branch.trim().is_empty() {
            return Err("Branch cannot be empty".to_string());
        }
        Ok(())
    }
}

#[async_trait]
impl TaskManager for MockRestClient {
    async fn launch_task(&self, params: TaskLaunchParams) -> TaskLaunchResult {
        // Validate parameters
        if let Err(error) = self.validate_params(&params) {
            return TaskLaunchResult::Failure { error };
        }

        // Simulate network delay
        if self.delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(self.delay_ms)).await;
        }

        // Simulate failures if enabled
        if self.simulate_failures && params.description.contains("fail") {
            return TaskLaunchResult::Failure {
                error: "Simulated task launch failure".to_string(),
            };
        }

        // Generate task ID and store task
        let task_id = self.generate_task_id(&params);
        let task_info = TaskInfo {
            id: task_id.clone(),
            title: params.description.clone(),
            status: "running".to_string(),
            repository: params.repository.clone(),
            branch: params.branch.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
            models: params.models.iter().map(|m| m.name.clone()).collect(),
        };

        self.tasks.write().await.insert(task_id.clone(), task_info);

        TaskLaunchResult::Success { task_id }
    }

    fn task_events_stream(&self, task_id: &str) -> Pin<Box<dyn Stream<Item = TaskEvent> + Send>> {
        self.create_event_stream(task_id)
    }

    async fn get_initial_tasks(&self) -> (Vec<TaskInfo>, Vec<TaskInfo>) {
        // Simulate network delay
        if self.delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(self.delay_ms)).await;
        }

        let drafts = self.drafts.read().await.values().cloned().collect();
        let tasks = self.tasks.read().await.values().cloned().collect();

        (drafts, tasks)
    }

    async fn save_draft_task(&self, draft_id: &str, description: &str, repository: &str, branch: &str, models: &[SelectedModel]) -> SaveDraftResult {
        // Simulate persistence delay
        if self.delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(self.delay_ms)).await;
        }

        // Simulate failures if enabled
        if self.simulate_failures && description.contains("fail") {
            return SaveDraftResult::Failure {
                error: "Simulated save failure".to_string(),
            };
        }

        let draft_info = TaskInfo {
            id: draft_id.to_string(),
            title: description.to_string(),
            status: "draft".to_string(),
            repository: repository.to_string(),
            branch: branch.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            models: models.iter().map(|m| m.name.clone()).collect(),
        };

        self.drafts.write().await.insert(draft_id.to_string(), draft_info);
        SaveDraftResult::Success
    }

    async fn list_repositories(&self) -> Vec<Repository> {
        // Simulate network delay
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
        // Simulate network delay
        if self.delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(self.delay_ms)).await;
        }

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
        "Mock REST Client (for testing TaskManager interface)"
    }
}

impl Default for MockRestClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Internal state for mock event stream generation
struct MockEventState {
    task_id: String,
    event_index: usize,
    start_time: chrono::DateTime<chrono::Utc>,
    tool_execution_counter: usize,
    active_tool_executions: HashMap<usize, String>,
    delay_ms: u64,
}

impl MockEventState {
    fn new(task_id: String, delay_ms: u64) -> Self {
        Self {
            task_id,
            event_index: 0,
            start_time: chrono::Utc::now(),
            tool_execution_counter: 0,
            active_tool_executions: HashMap::new(),
            delay_ms,
        }
    }

    fn next_delay_ms(&self) -> u64 {
        match self.event_index {
            // Status changes are quick
            0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 | 12 | 13 | 14 | 15 => self.delay_ms,
            // Tool execution lines are fast (50-200ms)
            16..=50 => 50 + (rand::random::<u64>() % 150).max(10),
            // Thoughts and file edits are slower (2-5 seconds)
            51..=70 => 2000 + (rand::random::<u64>() % 3000),
            // More tool execution
            71..=100 => 50 + (rand::random::<u64>() % 150).max(10),
            // Final status
            _ => self.delay_ms * 2,
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
        let ts = self.start_time + chrono::Duration::milliseconds((self.event_index as i64) * 500);

        let event = match self.event_index {
            0 => TaskEvent::Status { status: TaskExecutionStatus::Queued, ts },
            1 => TaskEvent::Log { level: LogLevel::Info, message: "Task queued for execution".to_string(), tool_execution_id: None, ts },
            2 => TaskEvent::Status { status: TaskExecutionStatus::Provisioning, ts },
            3 => TaskEvent::Log { level: LogLevel::Info, message: "Provisioning workspace...".to_string(), tool_execution_id: None, ts },
            4 => TaskEvent::Log { level: LogLevel::Info, message: "Cloning repository...".to_string(), tool_execution_id: None, ts },
            5 => TaskEvent::Log { level: LogLevel::Info, message: "Setting up development environment...".to_string(), tool_execution_id: None, ts },
            6 => TaskEvent::Status { status: TaskExecutionStatus::Running, ts },
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
            53 => TaskEvent::Status { status: TaskExecutionStatus::Completed, ts },
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
            delay_ms: self.delay_ms,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_client_launches_successful_task() {
        let client = MockRestClient::new();
        let params = TaskLaunchParams {
            repository: "test/repo".to_string(),
            branch: "main".to_string(),
            description: "Test task".to_string(),
            models: vec![SelectedModel {
                name: "Claude".to_string(),
                count: 1,
            }],
        };

        let result = client.launch_task(params).await;

        assert!(result.is_success());
        assert!(result.task_id().unwrap().starts_with("task_"));
    }

    #[tokio::test]
    async fn mock_client_validates_empty_description() {
        let client = MockRestClient::new();
        let params = TaskLaunchParams {
            repository: "test/repo".to_string(),
            branch: "main".to_string(),
            description: "".to_string(),
            models: vec![SelectedModel {
                name: "Claude".to_string(),
                count: 1,
            }],
        };

        let result = client.launch_task(params).await;

        assert!(!result.is_success());
        assert_eq!(result.error().unwrap(), "Task description cannot be empty");
    }

    #[tokio::test]
    async fn mock_client_validates_empty_models() {
        let client = MockRestClient::new();
        let params = TaskLaunchParams {
            repository: "test/repo".to_string(),
            branch: "main".to_string(),
            description: "Test task".to_string(),
            models: vec![],
        };

        let result = client.launch_task(params).await;

        assert!(!result.is_success());
        assert_eq!(result.error().unwrap(), "At least one model must be selected");
    }

    #[tokio::test]
    async fn mock_client_handles_simulated_failures() {
        let client = MockRestClient::with_failures(true);
        let params = TaskLaunchParams {
            repository: "test/repo".to_string(),
            branch: "main".to_string(),
            description: "This task will fail".to_string(),
            models: vec![SelectedModel {
                name: "Claude".to_string(),
                count: 1,
            }],
        };

        let result = client.launch_task(params).await;

        assert!(!result.is_success());
        assert_eq!(result.error().unwrap(), "Simulated task launch failure");
    }

    #[tokio::test]
    async fn mock_client_get_initial_tasks() {
        let client = MockRestClient::new();
        let (drafts, tasks) = client.get_initial_tasks().await;

        // Initially empty
        assert_eq!(drafts.len(), 0);
        assert_eq!(tasks.len(), 0);
    }

    #[tokio::test]
    async fn mock_client_save_draft_task_success() {
        let client = MockRestClient::new();
        let models = vec![SelectedModel {
            name: "Claude".to_string(),
            count: 1,
        }];

        let result = client.save_draft_task(
            "draft_001",
            "Test description",
            "test/repo",
            "main",
            &models,
        ).await;

        assert!(matches!(result, SaveDraftResult::Success));
    }

    #[tokio::test]
    async fn mock_client_list_repositories() {
        let client = MockRestClient::new();
        let repos = client.list_repositories().await;

        assert_eq!(repos.len(), 3);
        assert_eq!(repos[0].name, "myapp/backend");
        assert_eq!(repos[1].name, "myapp/frontend");
        assert_eq!(repos[2].name, "myapp/mobile");
    }

    #[tokio::test]
    async fn mock_client_list_branches() {
        let client = MockRestClient::new();

        let branches = client.list_branches("repo_001").await;
        assert_eq!(branches.len(), 3);
        assert_eq!(branches[0].name, "main");
        assert!(branches[0].is_default);

        let branches = client.list_branches("unknown_repo").await;
        assert_eq!(branches.len(), 0);
    }
}
