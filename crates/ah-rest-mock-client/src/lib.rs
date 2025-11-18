// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Mock REST client implementing TaskManager trait for testing
//!
//! This crate provides a mock implementation of the TaskManager trait
//! that simulates task execution without making actual network calls.
//! It's designed for testing the TUI and other components with realistic
//! behavior and configurable delays.

use ah_core::{SplitMode, TaskEvent, TaskLaunchParams, TaskLaunchResult, TaskManager, TaskState};
use ah_domain_types::{
    AgentChoice, AgentSoftware, AgentSoftwareBuild, DeliveryStatus, TaskExecution, TaskInfo,
};
use ah_domain_types::{LogLevel, ToolStatus};
use async_trait::async_trait;
use futures::stream;
use futures::{Stream, StreamExt};
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::sync::broadcast;

/// Helper function to create AgentChoice from display name
fn create_agent_choice(display_name: &str, count: usize) -> AgentChoice {
    let (software, model) = match display_name {
        "Claude 3.5 Sonnet" => (AgentSoftware::Claude, "sonnet"),
        "Claude Opus" | "Claude 3 Opus" => (AgentSoftware::Claude, "opus"),
        "GPT-4" => (AgentSoftware::Codex, "gpt-4"),
        "GPT-5" => (AgentSoftware::Codex, "gpt-5"),
        "GPT-3.5 Turbo" => (AgentSoftware::Codex, "gpt-3.5-turbo"),
        _ => (AgentSoftware::Claude, "sonnet"), // default fallback
    };
    AgentChoice {
        agent: AgentSoftwareBuild {
            software,
            version: "latest".to_string(),
        },
        model: model.to_string(),
        count,
        settings: std::collections::HashMap::new(),
        display_name: Some(display_name.to_string()),
    }
}

/// Mock REST client implementing the TaskManager trait
#[derive(Debug, Clone)]
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
    next_task_id: Arc<RwLock<u64>>,
}

impl MockRestClient {
    /// Get the currently launched tasks (for testing purposes)
    pub async fn get_launched_tasks(&self) -> Vec<TaskInfo> {
        self.tasks.read().await.values().cloned().collect()
    }

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
        params.repository().hash(&mut hasher);
        params.branch().hash(&mut hasher);
        params.description().hash(&mut hasher);
        format!("task_{:x}", hasher.finish())
    }

    /// Generate a unique draft ID
    #[allow(dead_code)] // Will be used when draft feature endpoints integrate
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
            agents: task_info.models.clone(),
            state: match task_info.status.as_str() {
                "running" => TaskState::Running,
                "completed" => TaskState::Completed,
                "merged" => TaskState::Merged,
                _ => TaskState::Running,
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
        if params.description().trim().is_empty() {
            return Err("Task description cannot be empty".to_string());
        }
        if params.models().is_empty() {
            return Err("At least one model must be selected".to_string());
        }
        if params.repository().trim().is_empty() {
            return Err("Repository cannot be empty".to_string());
        }
        if params.branch().trim().is_empty() {
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
        if self.simulate_failures && params.description().contains("fail") {
            return TaskLaunchResult::Failure {
                error: "Simulated task launch failure".to_string(),
            };
        }

        // Generate task ID and store task
        let task_id = self.generate_task_id(&params);
        let task_info = TaskInfo {
            id: task_id.clone(),
            title: params.description().to_string(),
            status: "running".to_string(),
            repository: params.repository().to_string(),
            branch: params.branch().to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            models: params.models().to_vec(),
        };

        self.tasks.write().await.insert(task_id.clone(), task_info);

        TaskLaunchResult::Success {
            session_ids: vec![task_id],
        }
    }

    fn task_events_receiver(&self, task_id: &str) -> broadcast::Receiver<TaskEvent> {
        // For the mock client, we'll create a broadcast channel and spawn a task to forward events
        let (tx, rx) = broadcast::channel(100);
        let stream = self.create_event_stream(task_id);

        tokio::spawn(async move {
            let mut stream = stream;
            while let Some(event) = stream.next().await {
                let _ = tx.send(event);
            }
        });

        rx
    }

    async fn get_initial_tasks(&self) -> (Vec<TaskInfo>, Vec<TaskExecution>) {
        // Simulate network delay
        if self.delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(self.delay_ms)).await;
        }

        // Get any stored tasks/drafts
        let drafts: Vec<TaskInfo> = self.drafts.read().await.values().cloned().collect();
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
                agents: vec![create_agent_choice("Claude 3.5 Sonnet", 1)],
                state: TaskState::Running,
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
                agents: vec![create_agent_choice("GPT-4", 1)],
                state: TaskState::Running,
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
                agents: vec![create_agent_choice("Claude 3 Opus", 1)],
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
                agents: vec![create_agent_choice("GPT-3.5 Turbo", 1)],
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
        models: &[ah_domain_types::AgentChoice],
    ) -> ah_core::task_manager::SaveDraftResult {
        use ah_core::task_manager::SaveDraftResult;

        // Simulate network delay
        if self.delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(self.delay_ms)).await;
        }

        // Simulate failures if enabled
        if self.simulate_failures && description.contains("fail") {
            return SaveDraftResult::Failure {
                error: "Simulated save failure".to_string(),
            };
        }

        let draft_info = ah_domain_types::TaskInfo {
            id: draft_id.to_string(),
            title: description.to_string(),
            status: "draft".to_string(),
            repository: repository.to_string(),
            branch: branch.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            models: models.to_vec(),
        };

        self.drafts.write().await.insert(draft_id.to_string(), draft_info);
        SaveDraftResult::Success
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
        let task_id = uuid::Uuid::new_v4().to_string();
        let params = ah_core::TaskLaunchParams::builder()
            .repository(
                request
                    .repo
                    .url
                    .as_ref()
                    .map(|u| u.to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
            )
            .branch(request.repo.branch.clone().unwrap_or_else(|| "main".to_string()))
            .description(request.prompt.clone())
            .agents(request.agents.clone())
            .agent_type(ah_core::agent_types::AgentType::Codex)
            .split_mode(SplitMode::None) // Mock client doesn't support split view
            .focus(false) // Mock client doesn't support focus
            .record(true) // Mock client enables recording by default
            .task_id(task_id)
            .build()
            .map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::InvalidInput, e)))?;

        match self.launch_task(params).await {
            ah_core::TaskLaunchResult::Success { session_ids } => {
                Ok(ah_rest_api_contract::CreateTaskResponse {
                    session_ids: session_ids.clone(),
                    status: ah_rest_api_contract::SessionStatus::Queued,
                    links: ah_rest_api_contract::TaskLinks {
                        self_link: format!(
                            "/api/v1/tasks/{}",
                            session_ids.first().unwrap_or(&"unknown".to_string())
                        ),
                        events: format!(
                            "/api/v1/tasks/{}/events",
                            session_ids.first().unwrap_or(&"unknown".to_string())
                        ),
                        logs: format!(
                            "/api/v1/tasks/{}/logs",
                            session_ids.first().unwrap_or(&"unknown".to_string())
                        ),
                    },
                })
            }
            ah_core::TaskLaunchResult::Failure { error } => {
                Err(Box::new(std::io::Error::other(error)))
            }
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
        // Get the TaskEvent receiver and convert to SessionEvent stream
        let task_receiver = self.task_events_receiver(session_id);

        let converted_stream = futures::stream::unfold(task_receiver, |mut receiver| async move {
            match receiver.recv().await {
                Ok(task_event) => {
                    // Convert TaskEvent to SessionEvent
                    let timestamp = match &task_event {
                        ah_core::TaskEvent::Status { ts, .. } => ts.timestamp() as u64,
                        ah_core::TaskEvent::Log { ts, .. } => ts.timestamp() as u64,
                        ah_core::TaskEvent::Thought { ts, .. } => ts.timestamp() as u64,
                        ah_core::TaskEvent::ToolUse { ts, .. } => ts.timestamp() as u64,
                        ah_core::TaskEvent::ToolResult { ts, .. } => ts.timestamp() as u64,
                        ah_core::TaskEvent::FileEdit { ts, .. } => ts.timestamp() as u64,
                    };

                    let session_event = match task_event {
                        ah_core::TaskEvent::Status { status, .. } => {
                            let session_status = match status {
                                TaskState::Queued => ah_rest_api_contract::SessionStatus::Queued,
                                TaskState::Provisioning => {
                                    ah_rest_api_contract::SessionStatus::Provisioning
                                }
                                TaskState::Running => ah_rest_api_contract::SessionStatus::Running,
                                TaskState::Pausing => ah_rest_api_contract::SessionStatus::Pausing,
                                TaskState::Paused => ah_rest_api_contract::SessionStatus::Paused,
                                TaskState::Resuming => {
                                    ah_rest_api_contract::SessionStatus::Resuming
                                }
                                TaskState::Stopping => {
                                    ah_rest_api_contract::SessionStatus::Stopping
                                }
                                TaskState::Stopped => ah_rest_api_contract::SessionStatus::Stopped,
                                TaskState::Completed => {
                                    ah_rest_api_contract::SessionStatus::Completed
                                }
                                TaskState::Failed => ah_rest_api_contract::SessionStatus::Failed,
                                TaskState::Cancelled => {
                                    ah_rest_api_contract::SessionStatus::Cancelled
                                }
                                TaskState::Draft => ah_rest_api_contract::SessionStatus::Queued,
                                TaskState::Merged => ah_rest_api_contract::SessionStatus::Completed,
                            };
                            ah_rest_api_contract::SessionEvent::Status(
                                ah_rest_api_contract::SessionStatusEvent {
                                    status: session_status,
                                    timestamp,
                                },
                            )
                        }
                        ah_core::TaskEvent::Log {
                            message,
                            level,
                            tool_execution_id,
                            ..
                        } => {
                            let log_level = match level {
                                LogLevel::Debug => ah_rest_api_contract::SessionLogLevel::Debug,
                                LogLevel::Info => ah_rest_api_contract::SessionLogLevel::Info,
                                LogLevel::Warn => ah_rest_api_contract::SessionLogLevel::Warn,
                                LogLevel::Error => ah_rest_api_contract::SessionLogLevel::Error,
                            };
                            ah_rest_api_contract::SessionEvent::Log(
                                ah_rest_api_contract::SessionLogEvent {
                                    message: message.into_bytes(),
                                    level: log_level,
                                    tool_execution_id: tool_execution_id.map(|s| s.into_bytes()),
                                    timestamp,
                                },
                            )
                        }
                        ah_core::TaskEvent::Thought { thought, .. } => {
                            ah_rest_api_contract::SessionEvent::Log(
                                ah_rest_api_contract::SessionLogEvent {
                                    message: thought.into_bytes(),
                                    level: ah_rest_api_contract::SessionLogLevel::Info,
                                    tool_execution_id: None,
                                    timestamp,
                                },
                            )
                        }
                        ah_core::TaskEvent::ToolUse {
                            tool_name,
                            tool_args,
                            tool_execution_id,
                            status,
                            ..
                        } => {
                            let session_status = match status {
                                ah_domain_types::ToolStatus::Started => {
                                    ah_rest_api_contract::SessionToolStatus::Started
                                }
                                ah_domain_types::ToolStatus::Completed => {
                                    ah_rest_api_contract::SessionToolStatus::Completed
                                }
                                ah_domain_types::ToolStatus::Failed => {
                                    ah_rest_api_contract::SessionToolStatus::Failed
                                }
                            };
                            ah_rest_api_contract::SessionEvent::ToolUse(
                                ah_rest_api_contract::SessionToolUseEvent {
                                    tool_name: tool_name.into_bytes(),
                                    tool_args: serde_json::to_string(&tool_args)
                                        .unwrap_or_default()
                                        .into_bytes(),
                                    tool_execution_id: tool_execution_id.into_bytes(),
                                    status: session_status,
                                    timestamp,
                                },
                            )
                        }
                        ah_core::TaskEvent::ToolResult {
                            tool_name,
                            tool_output,
                            tool_execution_id,
                            status,
                            ..
                        } => {
                            let session_status = match status {
                                ah_domain_types::ToolStatus::Started => {
                                    ah_rest_api_contract::SessionToolStatus::Started
                                }
                                ah_domain_types::ToolStatus::Completed => {
                                    ah_rest_api_contract::SessionToolStatus::Completed
                                }
                                ah_domain_types::ToolStatus::Failed => {
                                    ah_rest_api_contract::SessionToolStatus::Failed
                                }
                            };
                            ah_rest_api_contract::SessionEvent::ToolResult(
                                ah_rest_api_contract::SessionToolResultEvent {
                                    tool_name: tool_name.into_bytes(),
                                    tool_output: tool_output.into_bytes(),
                                    tool_execution_id: tool_execution_id.into_bytes(),
                                    status: session_status,
                                    timestamp,
                                },
                            )
                        }
                        ah_core::TaskEvent::FileEdit {
                            file_path,
                            lines_added,
                            lines_removed,
                            description,
                            ..
                        } => ah_rest_api_contract::SessionEvent::FileEdit(
                            ah_rest_api_contract::SessionFileEditEvent {
                                file_path: file_path.into_bytes(),
                                lines_added,
                                lines_removed,
                                description: description.map(|s| s.into_bytes()),
                                timestamp,
                            },
                        ),
                    };

                    Some((Ok(session_event), receiver))
                }
                Err(_) => None, // Stream ends when receiver is closed
            }
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
                agent: task.agents.first().cloned().unwrap_or_else(|| AgentChoice {
                    agent: AgentSoftwareBuild {
                        software: AgentSoftware::Claude,
                        version: "latest".to_string(),
                    },
                    model: "sonnet".to_string(),
                    count: 1,
                    settings: std::collections::HashMap::new(),
                    display_name: Some("Claude Code".to_string()),
                }),
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
                    TaskState::Queued => ah_rest_api_contract::SessionStatus::Queued,
                    TaskState::Provisioning => ah_rest_api_contract::SessionStatus::Provisioning,
                    TaskState::Running => ah_rest_api_contract::SessionStatus::Running,
                    TaskState::Pausing => ah_rest_api_contract::SessionStatus::Pausing,
                    TaskState::Paused => ah_rest_api_contract::SessionStatus::Paused,
                    TaskState::Resuming => ah_rest_api_contract::SessionStatus::Resuming,
                    TaskState::Stopping => ah_rest_api_contract::SessionStatus::Stopping,
                    TaskState::Stopped => ah_rest_api_contract::SessionStatus::Stopped,
                    TaskState::Completed => ah_rest_api_contract::SessionStatus::Completed,
                    TaskState::Failed => ah_rest_api_contract::SessionStatus::Failed,
                    TaskState::Cancelled => ah_rest_api_contract::SessionStatus::Cancelled,
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
        // Simulate network delay
        if self.delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(self.delay_ms)).await;
        }

        // Return mock repositories
        Ok(vec![
            ah_rest_api_contract::Repository {
                id: "repo_001".to_string(),
                display_name: "myapp/backend".to_string(),
                scm_provider: "git".to_string(),
                remote_url: url::Url::parse("https://github.com/user/myapp-backend")
                    .unwrap_or_else(|_| url::Url::parse("https://example.com").unwrap()),
                default_branch: "main".to_string(),
                last_used_at: Some(chrono::Utc::now()),
            },
            ah_rest_api_contract::Repository {
                id: "repo_002".to_string(),
                display_name: "myapp/frontend".to_string(),
                scm_provider: "git".to_string(),
                remote_url: url::Url::parse("https://github.com/user/myapp-frontend")
                    .unwrap_or_else(|_| url::Url::parse("https://example.com").unwrap()),
                default_branch: "main".to_string(),
                last_used_at: Some(chrono::Utc::now()),
            },
            ah_rest_api_contract::Repository {
                id: "repo_003".to_string(),
                display_name: "myapp/mobile".to_string(),
                scm_provider: "git".to_string(),
                remote_url: url::Url::parse("https://github.com/user/myapp-mobile")
                    .unwrap_or_else(|_| url::Url::parse("https://example.com").unwrap()),
                default_branch: "develop".to_string(),
                last_used_at: Some(chrono::Utc::now()),
            },
        ])
    }

    async fn get_repository_branches(
        &self,
        repository_id: &str,
    ) -> Result<Vec<ah_rest_api_contract::BranchInfo>, Box<dyn std::error::Error + Send + Sync>>
    {
        // Simulate network delay
        if self.delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(self.delay_ms)).await;
        }

        // Return mock branches based on repository
        match repository_id {
            "repo_001" => Ok(vec![
                ah_rest_api_contract::BranchInfo {
                    name: "main".to_string(),
                    is_default: true,
                    last_commit: Some("a1b2c3d4e5f6".to_string()),
                },
                ah_rest_api_contract::BranchInfo {
                    name: "develop".to_string(),
                    is_default: false,
                    last_commit: Some("f6e5d4c3b2a1".to_string()),
                },
                ah_rest_api_contract::BranchInfo {
                    name: "feature/auth".to_string(),
                    is_default: false,
                    last_commit: Some("123456789abc".to_string()),
                },
            ]),
            "repo_002" => Ok(vec![ah_rest_api_contract::BranchInfo {
                name: "main".to_string(),
                is_default: true,
                last_commit: Some("abcdef123456".to_string()),
            }]),
            "repo_003" => Ok(vec![
                ah_rest_api_contract::BranchInfo {
                    name: "develop".to_string(),
                    is_default: true,
                    last_commit: Some("654321fedcba".to_string()),
                },
                ah_rest_api_contract::BranchInfo {
                    name: "feature/ui".to_string(),
                    is_default: false,
                    last_commit: Some("fedcba654321".to_string()),
                },
            ]),
            _ => Ok(vec![]), // Empty for unknown repositories
        }
    }

    async fn save_draft_task(
        &self,
        _draft_id: &str,
        _description: &str,
        _repository: &str,
        _branch: &str,
        _models: &[ah_domain_types::AgentChoice],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Simulate network delay
        if self.delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(self.delay_ms)).await;
        }

        // Simulate failures if enabled
        if self.simulate_failures {
            return Err(Box::new(std::io::Error::other("Mock REST client failure")));
        }

        // Mock successful save - in a real implementation, this would persist to the server
        Ok(())
    }

    async fn get_repository_files(
        &self,
        repository_id: &str,
    ) -> Result<Vec<ah_rest_api_contract::RepositoryFile>, Box<dyn std::error::Error + Send + Sync>>
    {
        // Simulate network delay
        if self.delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(self.delay_ms)).await;
        }

        // Return mock files based on repository
        match repository_id {
            "repo_001" => Ok(vec![
                ah_rest_api_contract::RepositoryFile {
                    path: "src/main.rs".to_string(),
                    detail: Some("Tracked file".to_string()),
                },
                ah_rest_api_contract::RepositoryFile {
                    path: "Cargo.toml".to_string(),
                    detail: Some("Tracked file".to_string()),
                },
                ah_rest_api_contract::RepositoryFile {
                    path: "README.md".to_string(),
                    detail: Some("Tracked file".to_string()),
                },
            ]),
            "repo_002" => Ok(vec![
                ah_rest_api_contract::RepositoryFile {
                    path: "main.py".to_string(),
                    detail: Some("Tracked file".to_string()),
                },
                ah_rest_api_contract::RepositoryFile {
                    path: "requirements.txt".to_string(),
                    detail: Some("Tracked file".to_string()),
                },
            ]),
            _ => Ok(vec![]), // Empty for unknown repositories
        }
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
            0..=15 => self.delay_ms,
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
                status: TaskState::Queued,
                ts,
            },
            1 => TaskEvent::Log {
                level: LogLevel::Info,
                message: "Task queued for execution".to_string(),
                tool_execution_id: None,
                ts,
            },
            2 => TaskEvent::Status {
                status: TaskState::Provisioning,
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
                status: TaskState::Running,
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
                status: TaskState::Completed,
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
    use ah_core::{RestApiClient, agent_types::AgentType};

    #[tokio::test]
    async fn mock_client_launches_successful_task() {
        let client = MockRestClient::new();
        let params = TaskLaunchParams::builder()
            .repository("https://github.com/test/repo".to_string())
            .branch("main".to_string())
            .description("Test task".to_string())
            .agents(vec![AgentChoice {
                agent: AgentSoftwareBuild {
                    software: AgentSoftware::Claude,
                    version: "latest".to_string(),
                },
                model: "sonnet".to_string(),
                count: 1,
                settings: std::collections::HashMap::new(),
                display_name: None,
            }])
            .split_mode(SplitMode::None)
            .focus(false)
            .agent_type(AgentType::Claude)
            .record(true)
            .task_id("test-task-id".to_string())
            .build()
            .unwrap();

        let result = client.launch_task(params).await;

        assert!(result.is_success());
        assert!(result.session_ids().unwrap()[0].starts_with("task_"));
    }

    #[tokio::test]
    async fn mock_client_validates_empty_description() {
        let result = TaskLaunchParams::builder()
            .repository("test/repo".to_string())
            .branch("main".to_string())
            .description("".to_string())
            .agents(vec![AgentChoice {
                agent: AgentSoftwareBuild {
                    software: AgentSoftware::Claude,
                    version: "latest".to_string(),
                },
                model: "sonnet".to_string(),
                count: 1,
                settings: std::collections::HashMap::new(),
                display_name: None,
            }])
            .split_mode(SplitMode::None)
            .focus(false)
            .agent_type(AgentType::Claude)
            .record(true)
            .task_id("test-task-id".to_string())
            .build();

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Task description cannot be empty");
    }

    #[tokio::test]
    async fn mock_client_validates_empty_models() {
        let result = TaskLaunchParams::builder()
            .repository("test/repo".to_string())
            .branch("main".to_string())
            .description("Test task".to_string())
            .agents(vec![])
            .split_mode(SplitMode::None)
            .focus(false)
            .agent_type(AgentType::Claude)
            .record(true)
            .task_id("test-task-id".to_string())
            .build();

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "At least one model must be selected");
    }

    #[tokio::test]
    async fn mock_client_handles_simulated_failures() {
        let client = MockRestClient::with_failures(true);
        let params = TaskLaunchParams::builder()
            .repository("https://github.com/test/repo".to_string())
            .branch("main".to_string())
            .description("This task will fail".to_string())
            .agents(vec![AgentChoice {
                agent: AgentSoftwareBuild {
                    software: AgentSoftware::Claude,
                    version: "latest".to_string(),
                },
                model: "sonnet".to_string(),
                count: 1,
                settings: std::collections::HashMap::new(),
                display_name: None,
            }])
            .split_mode(SplitMode::None)
            .focus(false)
            .agent_type(AgentType::Claude)
            .record(true)
            .task_id("test-task-id".to_string())
            .build()
            .unwrap();

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
    async fn mock_client_list_repositories() {
        let client = MockRestClient::new();
        let repos = client.list_repositories(None, None).await.unwrap();

        assert_eq!(repos.len(), 3);
        assert_eq!(repos[0].display_name, "myapp/backend");
        assert_eq!(repos[1].display_name, "myapp/frontend");
        assert_eq!(repos[2].display_name, "myapp/mobile");
    }

    #[tokio::test]
    async fn mock_client_get_repository_branches() {
        let client = MockRestClient::new();

        let branches = client.get_repository_branches("repo_001").await.unwrap();
        assert_eq!(branches.len(), 3);
        assert_eq!(branches[0].name, "main");
        assert!(branches[0].is_default);

        let branches = client.get_repository_branches("unknown_repo").await.unwrap();
        assert_eq!(branches.len(), 0);
    }
}
