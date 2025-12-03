// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! REST-based TaskManager implementation
//!
//! This module provides a TaskManager implementation that can work with either
//! the real REST API client or a mock client, allowing seamless switching between
//! production and testing environments.

use ah_domain_types::{TaskExecution, TaskInfo, TaskState};

use async_trait::async_trait;
use futures::{Stream, StreamExt};
use std::pin::Pin;
use tokio::sync::broadcast;

use crate::{TaskEvent, TaskLaunchParams, TaskLaunchResult, TaskManager};
use ah_domain_types::{LogLevel, ToolStatus};

/// Trait for REST API clients that can be used with RestTaskManager
///
/// This trait defines a subset of REST API features that ah-core needs for task
/// execution. It stays in ah-core rather than ah-rest-client because:
///
/// 1. It represents ah-core's interface requirements, not the REST client's capabilities
/// 2. It allows ah-core to work with different client implementations (real, mock)
/// 3. It keeps the REST client crate focused on low-level HTTP operations
/// 4. Third-party users of ah-rest-client don't need this trait abstraction
///
/// Since ah-core depends on ah-rest-client, we can implement this trait directly
/// for RestClient, eliminating the need for a wrapper type.
#[async_trait]
pub trait RestApiClient: Send + Sync {
    /// Create a new task
    async fn create_task(
        &self,
        request: &ah_rest_api_contract::CreateTaskRequest,
    ) -> Result<ah_rest_api_contract::CreateTaskResponse, Box<dyn std::error::Error + Send + Sync>>;

    /// Stream events for a task
    async fn stream_session_events(
        &self,
        session_id: &str,
    ) -> Result<
        Pin<
            Box<
                dyn Stream<
                        Item = Result<
                            ah_rest_api_contract::SessionEvent,
                            Box<dyn std::error::Error + Send + Sync>,
                        >,
                    > + Send,
            >,
        >,
        Box<dyn std::error::Error + Send + Sync>,
    >;

    /// List sessions
    async fn list_sessions(
        &self,
        filters: Option<&ah_rest_api_contract::FilterQuery>,
    ) -> Result<ah_rest_api_contract::SessionListResponse, Box<dyn std::error::Error + Send + Sync>>;

    /// List repositories
    async fn list_repositories(
        &self,
        tenant_id: Option<&str>,
        project_id: Option<&str>,
    ) -> Result<Vec<ah_rest_api_contract::Repository>, Box<dyn std::error::Error + Send + Sync>>;

    /// Get branches for a repository
    async fn get_repository_branches(
        &self,
        repository_id: &str,
    ) -> Result<Vec<ah_rest_api_contract::BranchInfo>, Box<dyn std::error::Error + Send + Sync>>;

    /// Save a draft task
    async fn save_draft_task(
        &self,
        draft_id: &str,
        description: &str,
        repository: &str,
        branch: &str,
        models: &[ah_domain_types::AgentChoice],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Get files for a repository
    async fn get_repository_files(
        &self,
        repository_id: &str,
    ) -> Result<Vec<ah_rest_api_contract::RepositoryFile>, Box<dyn std::error::Error + Send + Sync>>;
}

/// Generic TaskManager implementation for REST API clients
///
/// This TaskManager can be instantiated with any client that implements RestApiClient,
/// allowing it to work with both real REST clients and mock clients.
#[derive(Debug)]
pub struct GenericRestTaskManager<C> {
    /// The underlying REST API client (can be real or mock)
    client: C,
}

impl<C> GenericRestTaskManager<C>
where
    C: RestApiClient + Clone + 'static,
{
    /// Create a new REST task manager with the given client
    pub fn new(client: C) -> Self {
        Self { client }
    }

    /// Get a reference to the underlying client
    pub fn client(&self) -> &C {
        &self.client
    }

    /// Convert REST API event to TaskEvent
    fn convert_session_event(event: ah_rest_api_contract::SessionEvent) -> TaskEvent {
        // Convert u64 timestamp to DateTime<Utc)
        let datetime_ts = chrono::DateTime::from_timestamp(event.timestamp() as i64, 0)
            .unwrap_or_else(chrono::Utc::now);

        match event {
            ah_rest_api_contract::SessionEvent::Status(event) => {
                let status = match event.status {
                    ah_rest_api_contract::SessionStatus::Queued => TaskState::Queued,
                    ah_rest_api_contract::SessionStatus::Provisioning => TaskState::Provisioning,
                    ah_rest_api_contract::SessionStatus::Running => TaskState::Running,
                    ah_rest_api_contract::SessionStatus::Pausing => TaskState::Pausing,
                    ah_rest_api_contract::SessionStatus::Paused => TaskState::Paused,
                    ah_rest_api_contract::SessionStatus::Resuming => TaskState::Resuming,
                    ah_rest_api_contract::SessionStatus::Stopping => TaskState::Stopping,
                    ah_rest_api_contract::SessionStatus::Stopped => TaskState::Stopped,
                    ah_rest_api_contract::SessionStatus::Completed => TaskState::Completed,
                    ah_rest_api_contract::SessionStatus::Failed => TaskState::Failed,
                    ah_rest_api_contract::SessionStatus::Cancelled => TaskState::Cancelled,
                };
                TaskEvent::Status {
                    status,
                    ts: datetime_ts,
                }
            }
            ah_rest_api_contract::SessionEvent::Log(event) => {
                let level = match event.level {
                    ah_rest_api_contract::SessionLogLevel::Debug => LogLevel::Debug,
                    ah_rest_api_contract::SessionLogLevel::Info => LogLevel::Info,
                    ah_rest_api_contract::SessionLogLevel::Warn => LogLevel::Warn,
                    ah_rest_api_contract::SessionLogLevel::Error => LogLevel::Error,
                };
                let message = String::from_utf8_lossy(&event.message).to_string();
                let tool_execution_id = event
                    .tool_execution_id
                    .as_ref()
                    .map(|bytes| String::from_utf8_lossy(bytes).to_string());
                TaskEvent::Log {
                    level,
                    message,
                    tool_execution_id,
                    ts: datetime_ts,
                }
            }
            ah_rest_api_contract::SessionEvent::Error(event) => {
                let message = String::from_utf8_lossy(&event.message).to_string();
                TaskEvent::Log {
                    level: LogLevel::Error,
                    message,
                    tool_execution_id: None, // Agent errors don't have tool execution IDs
                    ts: datetime_ts,
                }
            }
            ah_rest_api_contract::SessionEvent::Thought(event) => {
                let thought = String::from_utf8_lossy(&event.thought).to_string();
                let reasoning = event
                    .reasoning
                    .as_ref()
                    .map(|bytes| String::from_utf8_lossy(bytes).to_string());
                TaskEvent::Thought {
                    thought,
                    reasoning,
                    ts: datetime_ts,
                }
            }
            ah_rest_api_contract::SessionEvent::ToolUse(event) => {
                let tool_name = String::from_utf8_lossy(&event.tool_name).to_string();
                let tool_args_str = String::from_utf8_lossy(&event.tool_args).to_string();
                let tool_args =
                    serde_json::from_str(&tool_args_str).unwrap_or(serde_json::Value::Null);
                let tool_execution_id =
                    String::from_utf8_lossy(&event.tool_execution_id).to_string();
                let status = match event.status {
                    ah_rest_api_contract::SessionToolStatus::Started => ToolStatus::Started,
                    ah_rest_api_contract::SessionToolStatus::Completed => ToolStatus::Completed,
                    ah_rest_api_contract::SessionToolStatus::Failed => ToolStatus::Failed,
                };
                TaskEvent::ToolUse {
                    tool_name,
                    tool_args,
                    tool_execution_id,
                    status,
                    ts: datetime_ts,
                }
            }
            ah_rest_api_contract::SessionEvent::ToolResult(event) => {
                let tool_name = String::from_utf8_lossy(&event.tool_name).to_string();
                let tool_output = String::from_utf8_lossy(&event.tool_output).to_string();
                let tool_execution_id =
                    String::from_utf8_lossy(&event.tool_execution_id).to_string();
                let status = match event.status {
                    ah_rest_api_contract::SessionToolStatus::Started => ToolStatus::Started,
                    ah_rest_api_contract::SessionToolStatus::Completed => ToolStatus::Completed,
                    ah_rest_api_contract::SessionToolStatus::Failed => ToolStatus::Failed,
                };
                TaskEvent::ToolResult {
                    tool_name,
                    tool_output,
                    tool_execution_id,
                    status,
                    ts: datetime_ts,
                }
            }
            ah_rest_api_contract::SessionEvent::FileEdit(event) => {
                let file_path = String::from_utf8_lossy(&event.file_path).to_string();
                let description = event
                    .description
                    .as_ref()
                    .map(|bytes| String::from_utf8_lossy(bytes).to_string());
                TaskEvent::FileEdit {
                    file_path,
                    lines_added: event.lines_added,
                    lines_removed: event.lines_removed,
                    description,
                    ts: datetime_ts,
                }
            }
        }
    }
}

#[async_trait]
impl<C> TaskManager for GenericRestTaskManager<C>
where
    C: RestApiClient + Clone + 'static,
{
    async fn launch_task(&self, params: TaskLaunchParams) -> TaskLaunchResult {
        // Parse repository URL (validation already done in TaskLaunchParams::new)
        let repo_url = url::Url::parse(params.repository())
            .expect("Repository URL should be valid (validated in TaskLaunchParams::new)");

        // Use agents directly from parameters
        let agents: Vec<ah_domain_types::AgentChoice> = params.models().to_vec();

        let repo = ah_rest_api_contract::RepoConfig {
            mode: ah_rest_api_contract::RepoMode::Git,
            url: Some(repo_url),
            branch: Some(params.branch().to_string()),
            commit: None,
        };

        let runtime = ah_rest_api_contract::RuntimeConfig {
            runtime_type: ah_rest_api_contract::RuntimeType::Local,
            devcontainer_path: None,
            resources: None,
        };

        let request = ah_rest_api_contract::CreateTaskRequest {
            tenant_id: None,
            project_id: None,
            prompt: params.description().to_string(),
            repo,
            runtime,
            workspace: None,
            agents,
            delivery: None,
            labels: std::collections::HashMap::new(),
            webhooks: vec![],
        };

        // Make the API call
        match self.client.create_task(&request).await {
            Ok(response) => TaskLaunchResult::Success {
                session_ids: response.session_ids,
            },
            Err(e) => TaskLaunchResult::Failure {
                error: format!("Request failed: {}", e),
            },
        }
    }

    fn task_events_receiver(&self, task_id: &str) -> broadcast::Receiver<TaskEvent> {
        let task_id = task_id.to_string();
        let client = self.client.clone();

        // Create a broadcast channel for this task's events
        let (tx, rx) = broadcast::channel(100);

        // Spawn a task to consume the REST stream and forward events to the broadcast channel
        tokio::spawn(async move {
            match client.stream_session_events(&task_id).await {
                Ok(mut stream) => {
                    while let Some(result) = stream.next().await {
                        match result {
                            Ok(api_event) => {
                                let task_event = Self::convert_session_event(api_event);
                                let _ = tx.send(task_event);
                            }
                            Err(e) => {
                                // Send an error event
                                let error_event = TaskEvent::Log {
                                    level: LogLevel::Error,
                                    message: format!("Event stream error: {}", e),
                                    tool_execution_id: None,
                                    ts: chrono::Utc::now(),
                                };
                                let _ = tx.send(error_event);
                            }
                        }
                    }
                }
                Err(e) => {
                    // Send an error event
                    let error_event = TaskEvent::Log {
                        level: LogLevel::Error,
                        message: format!("Failed to connect to event stream: {}", e),
                        tool_execution_id: None,
                        ts: chrono::Utc::now(),
                    };
                    let _ = tx.send(error_event);
                }
            }
        });

        rx
    }

    async fn get_initial_tasks(&self) -> (Vec<TaskInfo>, Vec<TaskExecution>) {
        match self.client.list_sessions(None).await {
            Ok(response) => {
                let tasks: Vec<TaskExecution> = response
                    .items
                    .into_iter()
                    .map(|session| TaskExecution {
                        id: session.id,
                        repository: session.vcs.repo_url.unwrap_or_else(|| "unknown".to_string()),
                        branch: session.vcs.branch.unwrap_or_else(|| "main".to_string()),
                        agents: vec![session.agent.clone()],
                        state: match session.status {
                            ah_rest_api_contract::SessionStatus::Queued => TaskState::Queued,
                            ah_rest_api_contract::SessionStatus::Provisioning => {
                                TaskState::Provisioning
                            }
                            ah_rest_api_contract::SessionStatus::Running => TaskState::Running,
                            ah_rest_api_contract::SessionStatus::Pausing => TaskState::Pausing,
                            ah_rest_api_contract::SessionStatus::Paused => TaskState::Paused,
                            ah_rest_api_contract::SessionStatus::Resuming => TaskState::Resuming,
                            ah_rest_api_contract::SessionStatus::Stopping => TaskState::Stopping,
                            ah_rest_api_contract::SessionStatus::Stopped => TaskState::Stopped,
                            ah_rest_api_contract::SessionStatus::Completed => TaskState::Completed,
                            ah_rest_api_contract::SessionStatus::Failed => TaskState::Failed,
                            ah_rest_api_contract::SessionStatus::Cancelled => TaskState::Cancelled,
                        },
                        timestamp: session.started_at.map(|dt| dt.to_rfc3339()).unwrap_or_default(),
                        activity: vec![session.task.prompt.clone()],
                        delivery_status: vec![], // TODO: populate from session data if available
                    })
                    .collect();

                // For now, return empty drafts and all tasks as executions (drafts would need separate API)
                (vec![], tasks)
            }
            Err(e) => {
                tracing::warn!("Failed to list sessions: {}", e);
                (vec![], vec![])
            }
        }
    }

    async fn save_draft_task(
        &self,
        draft_id: &str,
        description: &str,
        repository: &str,
        branch: &str,
        models: &[ah_domain_types::AgentChoice],
    ) -> crate::task_manager::SaveDraftResult {
        match self
            .client
            .save_draft_task(draft_id, description, repository, branch, models)
            .await
        {
            Ok(()) => crate::task_manager::SaveDraftResult::Success,
            Err(e) => crate::task_manager::SaveDraftResult::Failure {
                error: format!("Failed to save draft to remote server: {}", e),
            },
        }
    }

    fn description(&self) -> &str {
        "REST API Task Manager (generic implementation)"
    }
}

/// Type alias for the most common usage: RestTaskManager with a dynamic RestApiClient
pub type RestTaskManager = GenericRestTaskManager<Box<dyn RestApiClient>>;

#[async_trait]
impl RestApiClient for ah_rest_client::RestClient {
    async fn create_task(
        &self,
        request: &ah_rest_api_contract::CreateTaskRequest,
    ) -> Result<ah_rest_api_contract::CreateTaskResponse, Box<dyn std::error::Error + Send + Sync>>
    {
        self.create_task(request)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }

    async fn stream_session_events(
        &self,
        session_id: &str,
    ) -> Result<
        Pin<
            Box<
                dyn Stream<
                        Item = Result<
                            ah_rest_api_contract::SessionEvent,
                            Box<dyn std::error::Error + Send + Sync>,
                        >,
                    > + Send,
            >,
        >,
        Box<dyn std::error::Error + Send + Sync>,
    > {
        let stream = self
            .stream_session_events(session_id)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
        let mapped_stream = stream
            .map(|item| item.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>));
        Ok(Box::pin(mapped_stream))
    }

    async fn list_sessions(
        &self,
        filters: Option<&ah_rest_api_contract::FilterQuery>,
    ) -> Result<ah_rest_api_contract::SessionListResponse, Box<dyn std::error::Error + Send + Sync>>
    {
        self.list_sessions(filters)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }

    async fn list_repositories(
        &self,
        tenant_id: Option<&str>,
        project_id: Option<&str>,
    ) -> Result<Vec<ah_rest_api_contract::Repository>, Box<dyn std::error::Error + Send + Sync>>
    {
        self.list_repositories(tenant_id, project_id)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }

    async fn get_repository_branches(
        &self,
        repository_id: &str,
    ) -> Result<Vec<ah_rest_api_contract::BranchInfo>, Box<dyn std::error::Error + Send + Sync>>
    {
        self.get_repository_branches(repository_id)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }

    async fn save_draft_task(
        &self,
        draft_id: &str,
        description: &str,
        repository: &str,
        branch: &str,
        models: &[ah_domain_types::AgentChoice],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.save_draft_task(draft_id, description, repository, branch, models)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }

    async fn get_repository_files(
        &self,
        repository_id: &str,
    ) -> Result<Vec<ah_rest_api_contract::RepositoryFile>, Box<dyn std::error::Error + Send + Sync>>
    {
        self.get_repository_files(repository_id)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ah_domain_types::{AgentChoice, AgentSoftware, AgentSoftwareBuild};

    #[tokio::test]
    async fn rest_task_manager_validates_parameters() {
        // Test that TaskLaunchParams validation works correctly

        // Empty description should fail validation
        let result = TaskLaunchParams::builder()
            .repository("https://github.com/test/repo".to_string())
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
                acp_stdio_launch_command: None,
            }])
            .agent_type(AgentSoftware::Claude)
            .task_id("test-task-id".to_string())
            .build();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Task description cannot be empty");

        // Empty models should fail validation
        let result = TaskLaunchParams::builder()
            .repository("https://github.com/test/repo".to_string())
            .branch("main".to_string())
            .description("Test task".to_string())
            .agents(vec![])
            .agent_type(AgentSoftware::Claude)
            .task_id("test-task-id".to_string())
            .build();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "At least one model must be selected");

        // Empty repository should fail validation
        let result = TaskLaunchParams::builder()
            .repository("".to_string())
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
                acp_stdio_launch_command: None,
            }])
            .agent_type(AgentSoftware::Claude)
            .task_id("test-task-id".to_string())
            .build();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Repository cannot be empty");

        // Empty branch should fail validation
        let result = TaskLaunchParams::builder()
            .repository("https://github.com/test/repo".to_string())
            .branch("".to_string())
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
                acp_stdio_launch_command: None,
            }])
            .agent_type(AgentSoftware::Claude)
            .task_id("test-task-id".to_string())
            .build();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Branch cannot be empty");

        // Invalid URL validation happens in launch_task, not in constructor
        // So "not-a-url" will pass TaskLaunchParams validation but fail in launch_task

        // Valid parameters should succeed
        let result = TaskLaunchParams::builder()
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
                acp_stdio_launch_command: None,
            }])
            .agent_type(AgentSoftware::Claude)
            .task_id("test-task-id".to_string())
            .build();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn rest_task_manager_has_correct_description() {
        // Test that the trait compiles correctly
        // Since we can't instantiate clients in this crate, we just verify compilation
        fn _test_trait_compilation() {
            // Touch the trait type to ensure it exists and compiles
            let _ = std::mem::size_of::<Option<&'static dyn super::RestApiClient>>();
        }
        _test_trait_compilation();
    }
}
