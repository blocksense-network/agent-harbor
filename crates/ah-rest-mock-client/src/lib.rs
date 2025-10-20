// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Mock REST client implementing TaskManager trait for testing
//!
//! This crate provides a mock implementation of the TaskManager trait
//! that simulates task execution without making actual network calls.
//! It's designed for testing the TUI and other components with realistic
//! behavior and configurable delays.

use ah_core::{
    SaveDraftResult, TaskEvent, TaskExecutionStatus, TaskLaunchParams, TaskLaunchResult,
    TaskManager,
};
use ah_domain_types::{
    Branch, DeliveryStatus, Repository, SelectedModel, TaskExecution, TaskInfo, TaskState,
};
use ah_domain_types::{LogLevel, ToolStatus};
use ah_rest_api_contract::*;
use async_trait::async_trait;
use futures::stream;
use futures::{Stream, StreamExt};
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
    /// Whether to return mock data when no tasks are stored
    return_mock_data: bool,
    /// Next task ID counter
    next_task_id: Arc<RwLock<u64>>,
}

impl MockRestClient {
    /// Create a new mock client with default settings
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
            drafts: Arc::new(RwLock::new(HashMap::new())),
            delay_ms: 50,
            simulate_failures: false,
            return_mock_data: false,
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

    /// Create a mock client that returns mock data
    pub fn with_mock_data() -> Self {
        Self {
            return_mock_data: true,
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

    /// Convert TaskInfo to TaskExecution
    fn task_info_to_execution(&self, task_info: TaskInfo) -> TaskExecution {
        TaskExecution {
            id: task_info.id,
            repository: task_info.repository,
            branch: task_info.branch,
            agents: task_info
                .models
                .into_iter()
                .map(|name| SelectedModel { name, count: 1 })
                .collect(),
            state: match task_info.status.as_str() {
                "running" => TaskState::Active,
                "completed" => TaskState::Completed,
                "merged" => TaskState::Merged,
                _ => TaskState::Active,
            },
            timestamp: task_info.created_at,
            activity: vec![],        // Would be populated from events
            delivery_status: vec![], // Would be populated based on status
        }
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
            },
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

    async fn get_initial_tasks(&self) -> (Vec<TaskInfo>, Vec<TaskExecution>) {
        // Simulate network delay
        if self.delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(self.delay_ms)).await;
        }

        // Get any stored tasks/drafts
        let mut drafts: Vec<TaskInfo> = self.drafts.read().await.values().cloned().collect();
        let mut tasks: Vec<TaskExecution> = Vec::new();

        // Convert stored TaskInfo to TaskExecution and add any mock data
        for task_info in self.tasks.read().await.values() {
            tasks.push(self.task_info_to_execution(task_info.clone()));
        }

        // If no tasks are stored and mock data is enabled, add mock data
        if tasks.is_empty() && self.return_mock_data {
            // Add 2 active tasks
            tasks.push(TaskExecution {
                id: "Implement user authentication flow".to_string(),
                repository: "myapp/backend".to_string(),
                branch: "main".to_string(),
                agents: vec![SelectedModel {
                    name: "Claude 3.5 Sonnet".to_string(),
                    count: 1,
                }],
                state: TaskState::Active,
                timestamp: chrono::Utc::now()
                    .checked_sub_signed(chrono::Duration::hours(1))
                    .unwrap()
                    .to_rfc3339(),
                activity: vec![
                    "Analyzing user requirements".to_string(),
                    "Examining codebase structure".to_string(),
                ],
                delivery_status: vec![],
            });

            tasks.push(TaskExecution {
                id: "Optimize database queries for performance".to_string(),
                repository: "myapp/backend".to_string(),
                branch: "feature/db-optimization".to_string(),
                agents: vec![SelectedModel {
                    name: "GPT-4".to_string(),
                    count: 1,
                }],
                state: TaskState::Active,
                timestamp: chrono::Utc::now()
                    .checked_sub_signed(chrono::Duration::hours(2))
                    .unwrap()
                    .to_rfc3339(),
                activity: vec!["Reviewing current database schema".to_string()],
                delivery_status: vec![],
            });

            // Add 1 completed task
            tasks.push(TaskExecution {
                id: "Add error handling to API endpoints".to_string(),
                repository: "myapp/backend".to_string(),
                branch: "main".to_string(),
                agents: vec![SelectedModel {
                    name: "Claude 3 Opus".to_string(),
                    count: 1,
                }],
                state: TaskState::Completed,
                timestamp: chrono::Utc::now()
                    .checked_sub_signed(chrono::Duration::days(1))
                    .unwrap()
                    .to_rfc3339(),
                activity: vec![
                    "Added comprehensive error handling to all API endpoints".to_string(),
                    "Updated API response formats with proper error codes".to_string(),
                    "Added detailed logging for debugging failed requests".to_string(),
                    "Created unit tests for error scenarios".to_string(),
                ],
                delivery_status: vec![
                    DeliveryStatus::BranchCreated,
                    DeliveryStatus::PullRequestCreated {
                        pr_number: 123,
                        title: "Add error handling to API endpoints".to_string(),
                    },
                ],
            });

            // Add 1 merged task
            tasks.push(TaskExecution {
                id: "Implement dark mode toggle in UI".to_string(),
                repository: "myapp/frontend".to_string(),
                branch: "feature/dark-mode".to_string(),
                agents: vec![SelectedModel {
                    name: "GPT-3.5 Turbo".to_string(),
                    count: 1,
                }],
                state: TaskState::Merged,
                timestamp: chrono::Utc::now()
                    .checked_sub_signed(chrono::Duration::days(2))
                    .unwrap()
                    .to_rfc3339(),
                activity: vec![
                    "Implemented dark mode CSS variables for consistent theming".to_string(),
                    "Added theme toggle component to header navigation".to_string(),
                    "Updated all component styling for dark mode compatibility".to_string(),
                    "Added user preference persistence in localStorage".to_string(),
                    "Tested accessibility compliance with dark theme".to_string(),
                ],
                delivery_status: vec![
                    DeliveryStatus::BranchCreated,
                    DeliveryStatus::PullRequestCreated {
                        pr_number: 456,
                        title: "Implement dark mode toggle in UI".to_string(),
                    },
                    DeliveryStatus::PullRequestMerged { pr_number: 456 },
                ],
            });
        }

        (drafts, tasks)
    }

    async fn save_draft_task(
        &self,
        draft_id: &str,
        description: &str,
        repository: &str,
        branch: &str,
        models: &[SelectedModel],
    ) -> SaveDraftResult {
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

#[async_trait::async_trait]
impl ah_core::RestApiClient for MockRestClient {
    async fn create_task(
        &self,
        request: &ah_rest_api_contract::CreateTaskRequest,
    ) -> Result<ah_rest_api_contract::CreateTaskResponse, Box<dyn std::error::Error + Send + Sync>>
    {
        // Simulate creating a task by converting the request to a task launch
        let params = ah_core::TaskLaunchParams {
            repository: request
                .repo
                .url
                .as_ref()
                .map(|u| u.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            branch: request.repo.branch.clone().unwrap_or_else(|| "main".to_string()),
            description: request.prompt.clone(),
            models: vec![ah_domain_types::SelectedModel {
                name: request.agent.agent_type.clone(),
                count: 1,
            }],
        };

        match self.launch_task(params).await {
            ah_core::TaskLaunchResult::Success { task_id } => {
                Ok(ah_rest_api_contract::CreateTaskResponse {
                    id: task_id.clone(),
                    status: ah_rest_api_contract::SessionStatus::Queued,
                    links: ah_rest_api_contract::TaskLinks {
                        self_link: format!("/api/v1/tasks/{}", task_id),
                        events: format!("/api/v1/tasks/{}/events", task_id),
                        logs: format!("/api/v1/tasks/{}/logs", task_id),
                    },
                })
            }
            ah_core::TaskLaunchResult::Failure { error } => Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                error,
            ))),
        }
    }

    async fn stream_session_events(
        &self,
        session_id: &str,
    ) -> Result<
        std::pin::Pin<
            Box<
                dyn futures::Stream<
                        Item = Result<
                            ah_rest_api_contract::SessionEvent,
                            Box<dyn std::error::Error + Send + Sync>,
                        >,
                    > + Send,
            >,
        >,
        Box<dyn std::error::Error + Send + Sync>,
    > {
        // Convert our TaskEvent stream to SessionEvent stream
        let task_stream = self.task_events_stream(session_id);

        let converted_stream = task_stream.map(|task_event| {
            // Convert TaskEvent to SessionEvent - this is a simplified conversion
            // In a real implementation, this would need more comprehensive mapping
            let session_event = ah_rest_api_contract::SessionEvent {
                event_type: match task_event {
                    ah_core::TaskEvent::Status { .. } => ah_rest_api_contract::EventType::Status,
                    ah_core::TaskEvent::Log { .. } => ah_rest_api_contract::EventType::Log,
                    ah_core::TaskEvent::Thought { .. } => ah_rest_api_contract::EventType::Thought,
                    ah_core::TaskEvent::ToolUse { .. } => ah_rest_api_contract::EventType::ToolUse,
                    ah_core::TaskEvent::ToolResult { .. } => {
                        ah_rest_api_contract::EventType::ToolResult
                    }
                    ah_core::TaskEvent::FileEdit { .. } => {
                        ah_rest_api_contract::EventType::FileEdit
                    }
                },
                status: match &task_event {
                    ah_core::TaskEvent::Status { status, .. } => Some(match status {
                        ah_core::TaskExecutionStatus::Queued => {
                            ah_rest_api_contract::SessionStatus::Queued
                        }
                        ah_core::TaskExecutionStatus::Provisioning => {
                            ah_rest_api_contract::SessionStatus::Provisioning
                        }
                        ah_core::TaskExecutionStatus::Running => {
                            ah_rest_api_contract::SessionStatus::Running
                        }
                        ah_core::TaskExecutionStatus::Pausing => {
                            ah_rest_api_contract::SessionStatus::Pausing
                        }
                        ah_core::TaskExecutionStatus::Paused => {
                            ah_rest_api_contract::SessionStatus::Paused
                        }
                        ah_core::TaskExecutionStatus::Resuming => {
                            ah_rest_api_contract::SessionStatus::Resuming
                        }
                        ah_core::TaskExecutionStatus::Stopping => {
                            ah_rest_api_contract::SessionStatus::Stopping
                        }
                        ah_core::TaskExecutionStatus::Stopped => {
                            ah_rest_api_contract::SessionStatus::Stopped
                        }
                        ah_core::TaskExecutionStatus::Completed => {
                            ah_rest_api_contract::SessionStatus::Completed
                        }
                        ah_core::TaskExecutionStatus::Failed => {
                            ah_rest_api_contract::SessionStatus::Failed
                        }
                        ah_core::TaskExecutionStatus::Cancelled => {
                            ah_rest_api_contract::SessionStatus::Cancelled
                        }
                    }),
                    _ => None,
                },
                level: match &task_event {
                    ah_core::TaskEvent::Log { level, .. } => Some(level.clone()),
                    _ => None,
                },
                message: match &task_event {
                    ah_core::TaskEvent::Log { message, .. } => Some(message.clone()),
                    _ => None,
                },
                thought: match &task_event {
                    ah_core::TaskEvent::Thought { thought, .. } => Some(thought.clone()),
                    _ => None,
                },
                reasoning: match &task_event {
                    ah_core::TaskEvent::Thought { reasoning, .. } => reasoning.clone(),
                    _ => None,
                },
                tool_name: match &task_event {
                    ah_core::TaskEvent::ToolUse { tool_name, .. }
                    | ah_core::TaskEvent::ToolResult { tool_name, .. } => Some(tool_name.clone()),
                    _ => None,
                },
                tool_args: match &task_event {
                    ah_core::TaskEvent::ToolUse { tool_args, .. } => {
                        Some(serde_json::Value::String(tool_args.to_string()))
                    }
                    _ => None,
                },
                tool_output: match &task_event {
                    ah_core::TaskEvent::ToolResult { tool_output, .. } => Some(tool_output.clone()),
                    _ => None,
                },
                tool_execution_id: match &task_event {
                    ah_core::TaskEvent::ToolUse {
                        tool_execution_id, ..
                    }
                    | ah_core::TaskEvent::ToolResult {
                        tool_execution_id, ..
                    } => Some(tool_execution_id.clone()),
                    ah_core::TaskEvent::Log {
                        tool_execution_id, ..
                    } => tool_execution_id.clone(),
                    _ => None,
                },
                file_path: match &task_event {
                    ah_core::TaskEvent::FileEdit { file_path, .. } => Some(file_path.clone()),
                    _ => None,
                },
                lines_added: match &task_event {
                    ah_core::TaskEvent::FileEdit { lines_added, .. } => Some(*lines_added as u32),
                    _ => None,
                },
                lines_removed: match &task_event {
                    ah_core::TaskEvent::FileEdit { lines_removed, .. } => {
                        Some(*lines_removed as u32)
                    }
                    _ => None,
                },
                description: match &task_event {
                    ah_core::TaskEvent::FileEdit { description, .. } => description.clone(),
                    _ => None,
                },
                ts: match task_event {
                    ah_core::TaskEvent::Status { ts, .. } => ts,
                    ah_core::TaskEvent::Log { ts, .. } => ts,
                    ah_core::TaskEvent::Thought { ts, .. } => ts,
                    ah_core::TaskEvent::ToolUse { ts, .. } => ts,
                    ah_core::TaskEvent::ToolResult { ts, .. } => ts,
                    ah_core::TaskEvent::FileEdit { ts, .. } => ts,
                },
                snapshot_id: None,
                note: None,
                hosts: None,
                host: None,
                stream: None,
                passed: None,
                failed: None,
                delivery: None,
            };
            Ok(session_event)
        });

        Ok(Box::pin(converted_stream))
    }

    async fn list_sessions(
        &self,
        _filters: Option<&ah_rest_api_contract::FilterQuery>,
    ) -> Result<ah_rest_api_contract::SessionListResponse, Box<dyn std::error::Error + Send + Sync>>
    {
        // Get tasks from the mock client
        let (_drafts, tasks) = self.get_initial_tasks().await;

        let sessions = tasks
            .into_iter()
            .map(|task| ah_rest_api_contract::Session {
                id: task.id.clone(),
                tenant_id: None,
                project_id: None,
                task: ah_rest_api_contract::TaskInfo {
                    prompt: task.id.clone(), // Use id as prompt since we don't have separate title field
                    attachments: std::collections::HashMap::new(),
                    labels: std::collections::HashMap::new(),
                },
                agent: ah_rest_api_contract::AgentConfig {
                    agent_type: task
                        .agents
                        .first()
                        .map(|m| m.name.clone())
                        .unwrap_or_else(|| "claude-code".to_string()),
                    version: "latest".to_string(),
                    settings: std::collections::HashMap::new(),
                },
                runtime: ah_rest_api_contract::RuntimeConfig {
                    runtime_type: ah_rest_api_contract::RuntimeType::Local,
                    devcontainer_path: None,
                    resources: None,
                },
                workspace: ah_rest_api_contract::WorkspaceInfo {
                    snapshot_provider: "none".to_string(),
                    mount_path: "/tmp".to_string(),
                    host: None,
                    devcontainer_details: None,
                },
                vcs: ah_rest_api_contract::VcsInfo {
                    repo_url: Some(task.repository),
                    branch: Some(task.branch),
                    commit: None,
                },
                status: match task.state {
                    TaskState::Active => ah_rest_api_contract::SessionStatus::Running,
                    TaskState::Completed => ah_rest_api_contract::SessionStatus::Completed,
                    TaskState::Merged => ah_rest_api_contract::SessionStatus::Completed,
                    TaskState::Draft => ah_rest_api_contract::SessionStatus::Queued,
                },
                started_at: Some(
                    chrono::DateTime::parse_from_rfc3339(&task.timestamp)
                        .unwrap_or_else(|_| chrono::Utc::now().into())
                        .with_timezone(&chrono::Utc),
                ),
                ended_at: None,
                links: ah_rest_api_contract::SessionLinks {
                    self_link: format!("/api/v1/sessions/{}", task.id),
                    events: format!("/api/v1/sessions/{}/events", task.id),
                    logs: format!("/api/v1/sessions/{}/logs", task.id),
                },
            })
            .collect();

        Ok(ah_rest_api_contract::SessionListResponse {
            items: sessions,
            next_page: None,
            total: None,
        })
    }

    async fn list_repositories(
        &self,
        _tenant_id: Option<&str>,
        _project_id: Option<&str>,
    ) -> Result<Vec<ah_rest_api_contract::Repository>, Box<dyn std::error::Error + Send + Sync>>
    {
        // Get repositories from the mock client
        let repos = ah_core::TaskManager::list_repositories(self).await;

        Ok(repos
            .into_iter()
            .map(|repo| ah_rest_api_contract::Repository {
                id: repo.id,
                display_name: repo.name,
                scm_provider: "git".to_string(),
                remote_url: url::Url::parse(&repo.url)
                    .unwrap_or_else(|_| url::Url::parse("https://example.com").unwrap()),
                default_branch: repo.default_branch,
                last_used_at: Some(chrono::Utc::now()),
            })
            .collect())
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
            0 => TaskEvent::Status {
                status: TaskExecutionStatus::Queued,
                ts,
            },
            1 => TaskEvent::Log {
                level: LogLevel::Info,
                message: "Task queued for execution".to_string(),
                tool_execution_id: None,
                ts,
            },
            2 => TaskEvent::Status {
                status: TaskExecutionStatus::Provisioning,
                ts,
            },
            3 => TaskEvent::Log {
                level: LogLevel::Info,
                message: "Provisioning workspace...".to_string(),
                tool_execution_id: None,
                ts,
            },
            4 => TaskEvent::Log {
                level: LogLevel::Info,
                message: "Cloning repository...".to_string(),
                tool_execution_id: None,
                ts,
            },
            5 => TaskEvent::Log {
                level: LogLevel::Info,
                message: "Setting up development environment...".to_string(),
                tool_execution_id: None,
                ts,
            },
            6 => TaskEvent::Status {
                status: TaskExecutionStatus::Running,
                ts,
            },
            7 => TaskEvent::Log {
                level: LogLevel::Info,
                message: "Starting agent execution".to_string(),
                tool_execution_id: None,
                ts,
            },

            // Initial thoughts
            8 => TaskEvent::Thought {
                thought: "Analyzing the user's request to understand requirements".to_string(),
                reasoning: Some("Need to understand what needs to be implemented".to_string()),
                ts,
            },
            9 => TaskEvent::Thought {
                thought: "Examining the codebase structure and existing patterns".to_string(),
                reasoning: Some(
                    "Looking for similar implementations to follow conventions".to_string(),
                ),
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
            }
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
                tool_output: "Finished dev [unoptimized + debuginfo] target(s) in 8.45s"
                    .to_string(),
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
            }
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
                tool_output: "Finished dev [unoptimized + debuginfo] target(s) in 45.23s"
                    .to_string(),
                tool_execution_id: self.get_or_create_tool_execution_id(18),
                status: ToolStatus::Completed,
                ts,
            },

            // More thoughts and file edits
            32 => TaskEvent::Thought {
                thought: "Optimizing database queries for better performance".to_string(),
                reasoning: Some(
                    "Analyzing current query patterns and identifying bottlenecks".to_string(),
                ),
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
            }
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
                tool_output:
                    "test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out"
                        .to_string(),
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
            53 => TaskEvent::Status {
                status: TaskExecutionStatus::Completed,
                ts,
            },
            54 => TaskEvent::Log {
                level: LogLevel::Info,
                message: "Task completed successfully".to_string(),
                tool_execution_id: None,
                ts,
            },

            // End of stream
            _ => return None,
        };

        self.event_index += 1;
        Some((
            event,
            Self {
                task_id: self.task_id.clone(),
                event_index: self.event_index,
                start_time: self.start_time,
                tool_execution_counter: self.tool_execution_counter,
                active_tool_executions: self.active_tool_executions.clone(),
                delay_ms: self.delay_ms,
            },
        ))
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
        assert_eq!(
            result.error().unwrap(),
            "At least one model must be selected"
        );
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

        let result = client
            .save_draft_task(
                "draft_001",
                "Test description",
                "test/repo",
                "main",
                &models,
            )
            .await;

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
