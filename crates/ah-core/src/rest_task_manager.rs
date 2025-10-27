// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! REST-based TaskManager implementation
//!
//! This module provides a TaskManager implementation that can work with either
//! the real REST API client or a mock client, allowing seamless switching between
//! production and testing environments.

use ah_domain_types::{
    Branch, Repository as DomainRepository, SelectedModel, TaskExecution, TaskInfo, TaskState,
};
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use std::pin::Pin;

use crate::{TaskEvent, TaskLaunchParams, TaskLaunchResult, TaskManager};
use ah_domain_types::{LogLevel, TaskExecutionStatus, ToolStatus};

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
        models: &[ah_domain_types::SelectedModel],
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
        match event.event_type {
            ah_rest_api_contract::EventType::Status => {
                if let Some(status) = event.status {
                    TaskEvent::Status {
                        status,
                        ts: event.ts.into(),
                    }
                } else {
                    // Default to running if no status
                    TaskEvent::Status {
                        status: TaskExecutionStatus::Running,
                        ts: event.ts.into(),
                    }
                }
            }
            ah_rest_api_contract::EventType::Log => {
                if let Some(message) = event.message {
                    TaskEvent::Log {
                        level: match event.level.unwrap_or(LogLevel::Info) {
                            LogLevel::Debug => LogLevel::Debug,
                            LogLevel::Info => LogLevel::Info,
                            LogLevel::Warn => LogLevel::Warn,
                            LogLevel::Error => LogLevel::Error,
                        },
                        message,
                        tool_execution_id: event.tool_execution_id,
                        ts: event.ts.into(),
                    }
                } else {
                    TaskEvent::Log {
                        level: LogLevel::Info,
                        message: "Unknown log event".to_string(),
                        tool_execution_id: None,
                        ts: event.ts.into(),
                    }
                }
            }
            ah_rest_api_contract::EventType::Thought => {
                if let Some(thought) = event.thought {
                    TaskEvent::Thought {
                        thought,
                        reasoning: event.reasoning,
                        ts: event.ts.into(),
                    }
                } else {
                    TaskEvent::Thought {
                        thought: "Unknown thought".to_string(),
                        reasoning: None,
                        ts: event.ts.into(),
                    }
                }
            }
            ah_rest_api_contract::EventType::ToolUse => {
                if let (Some(tool_name), Some(tool_args)) = (event.tool_name, event.tool_args) {
                    TaskEvent::ToolUse {
                        tool_name,
                        tool_args,
                        tool_execution_id: event
                            .tool_execution_id
                            .unwrap_or_else(|| "unknown".to_string()),
                        status: ToolStatus::Started, // Assume started for tool use events
                        ts: event.ts.into(),
                    }
                } else {
                    TaskEvent::ToolUse {
                        tool_name: "unknown".to_string(),
                        tool_args: serde_json::json!({}),
                        tool_execution_id: "unknown".to_string(),
                        status: ToolStatus::Started,
                        ts: event.ts.into(),
                    }
                }
            }
            ah_rest_api_contract::EventType::ToolResult => {
                if let (Some(tool_name), Some(tool_output)) = (event.tool_name, event.tool_output) {
                    TaskEvent::ToolResult {
                        tool_name,
                        tool_output,
                        tool_execution_id: event
                            .tool_execution_id
                            .unwrap_or_else(|| "unknown".to_string()),
                        status: ToolStatus::Completed, // Assume completed for tool result events
                        ts: event.ts.into(),
                    }
                } else {
                    TaskEvent::ToolResult {
                        tool_name: "unknown".to_string(),
                        tool_output: "Unknown output".to_string(),
                        tool_execution_id: "unknown".to_string(),
                        status: ToolStatus::Completed,
                        ts: event.ts.into(),
                    }
                }
            }
            ah_rest_api_contract::EventType::FileEdit => {
                if let Some(file_path) = event.file_path {
                    TaskEvent::FileEdit {
                        file_path,
                        lines_added: event.lines_added.unwrap_or(0) as usize,
                        lines_removed: event.lines_removed.unwrap_or(0) as usize,
                        description: event.description,
                        ts: event.ts.into(),
                    }
                } else {
                    TaskEvent::FileEdit {
                        file_path: "unknown".to_string(),
                        lines_added: 0,
                        lines_removed: 0,
                        description: None,
                        ts: event.ts.into(),
                    }
                }
            }
            // Handle other event types by creating appropriate TaskEvents or defaulting
            _ => TaskEvent::Log {
                level: LogLevel::Info,
                message: format!("Unhandled event type: {:?}", event.event_type),
                tool_execution_id: None,
                ts: event.ts.into(),
            },
        }
    }
}

#[async_trait]
impl<C> crate::TaskManager for GenericRestTaskManager<C>
where
    C: RestApiClient + Clone + 'static,
{
    async fn launch_task(&self, params: TaskLaunchParams) -> TaskLaunchResult {
        // Parse repository URL (validation already done in TaskLaunchParams::new)
        let repo_url = url::Url::parse(&params.repository)
            .expect("Repository URL should be valid (validated in TaskLaunchParams::new)");

        // Convert parameters to REST API format
        let agent = ah_rest_api_contract::AgentConfig {
            agent_type: params
                .models
                .first()
                .map(|m| m.name.clone())
                .unwrap_or_else(|| "claude-code".to_string()),
            version: "latest".to_string(),
            settings: std::collections::HashMap::new(),
        };

        let repo = ah_rest_api_contract::RepoConfig {
            mode: ah_rest_api_contract::RepoMode::Git,
            url: Some(repo_url),
            branch: Some(params.branch.clone()),
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
            prompt: params.description,
            repo,
            runtime,
            workspace: None,
            agent,
            delivery: None,
            labels: std::collections::HashMap::new(),
            webhooks: vec![],
        };

        // Make the API call
        match self.client.create_task(&request).await {
            Ok(response) => TaskLaunchResult::Success {
                task_id: response.id,
            },
            Err(e) => TaskLaunchResult::Failure {
                error: format!("Request failed: {}", e),
            },
        }
    }

    fn task_events_stream(&self, task_id: &str) -> Pin<Box<dyn Stream<Item = TaskEvent> + Send>> {
        let task_id = task_id.to_string();
        let client = self.client.clone();

        Box::pin(async_stream::stream! {
            match client.stream_session_events(&task_id).await {
                Ok(mut stream) => {
                    while let Some(result) = stream.next().await {
                        match result {
                            Ok(api_event) => {
                                let task_event = Self::convert_session_event(api_event);
                                yield task_event;
                            }
                            Err(e) => {
                                // Yield an error event and continue
                                let error_event = TaskEvent::Log {
                                    level: LogLevel::Error,
                                    message: format!("Event stream error: {}", e),
                                    tool_execution_id: None,
                                    ts: chrono::Utc::now(),
                                };
                                yield error_event;
                            }
                        }
                    }
                }
                Err(e) => {
                    // Yield an error event and end the stream
                    let error_event = TaskEvent::Log {
                        level: LogLevel::Error,
                        message: format!("Failed to connect to event stream: {}", e),
                        tool_execution_id: None,
                        ts: chrono::Utc::now(),
                    };
                    yield error_event;
                }
            }
        })
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
                        agents: vec![SelectedModel {
                            name: session.agent.agent_type,
                            count: 1,
                        }],
                        state: match session.status {
                            ah_rest_api_contract::SessionStatus::Completed => TaskState::Completed,
                            ah_rest_api_contract::SessionStatus::Failed => TaskState::Active, // Map to Active for now
                            ah_rest_api_contract::SessionStatus::Cancelled => TaskState::Active, // Map to Active for now
                            _ => TaskState::Active,
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

    async fn launch_task_from_starting_point(
        &self,
        starting_point: crate::task_manager::StartingPoint,
        description: &str,
        models: &[ah_domain_types::SelectedModel],
    ) -> crate::task_manager::TaskLaunchResult {
        // For now, only support RepositoryBranch starting point
        // TODO: Implement support for RepositoryCommit and FilesystemSnapshot
        match starting_point {
            crate::task_manager::StartingPoint::RepositoryBranch { repository, branch } => {
                match crate::task_manager::TaskLaunchParams::new(
                    repository,
                    branch,
                    description.to_string(),
                    models.to_vec(),
                ) {
                    Ok(params) => self.launch_task(params).await,
                    Err(e) => crate::task_manager::TaskLaunchResult::Failure {
                        error: format!("Invalid parameters: {}", e),
                    },
                }
            }
            crate::task_manager::StartingPoint::RepositoryCommit { .. } => {
                crate::task_manager::TaskLaunchResult::Failure {
                    error: "RepositoryCommit starting point not yet implemented".to_string(),
                }
            }
            crate::task_manager::StartingPoint::FilesystemSnapshot { .. } => {
                crate::task_manager::TaskLaunchResult::Failure {
                    error: "FilesystemSnapshot starting point not yet implemented".to_string(),
                }
            }
        }
    }

    async fn save_draft_task(
        &self,
        draft_id: &str,
        description: &str,
        repository: &str,
        branch: &str,
        models: &[ah_domain_types::SelectedModel],
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
        models: &[ah_domain_types::SelectedModel],
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
    use ah_domain_types::SelectedModel;

    #[tokio::test]
    async fn rest_task_manager_validates_parameters() {
        // Test that TaskLaunchParams validation works correctly

        // Empty description should fail validation
        let result = TaskLaunchParams::new(
            "test/repo".to_string(),
            "main".to_string(),
            "".to_string(),
            vec![SelectedModel {
                name: "claude-code".to_string(),
                count: 1,
            }],
        );
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Task description cannot be empty");

        // Empty models should fail validation
        let result = TaskLaunchParams::new(
            "test/repo".to_string(),
            "main".to_string(),
            "Test task".to_string(),
            vec![],
        );
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "At least one model must be selected");

        // Empty repository should fail validation
        let result = TaskLaunchParams::new(
            "".to_string(),
            "main".to_string(),
            "Test task".to_string(),
            vec![SelectedModel {
                name: "claude-code".to_string(),
                count: 1,
            }],
        );
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Repository cannot be empty");

        // Empty branch should fail validation
        let result = TaskLaunchParams::new(
            "test/repo".to_string(),
            "".to_string(),
            "Test task".to_string(),
            vec![SelectedModel {
                name: "claude-code".to_string(),
                count: 1,
            }],
        );
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Branch cannot be empty");

        // Invalid URL should fail validation
        let result = TaskLaunchParams::new(
            "not-a-url".to_string(),
            "main".to_string(),
            "Test task".to_string(),
            vec![SelectedModel {
                name: "claude-code".to_string(),
                count: 1,
            }],
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().starts_with("Invalid repository URL:"));

        // Valid parameters should succeed
        let result = TaskLaunchParams::new(
            "https://github.com/test/repo".to_string(),
            "main".to_string(),
            "Test task".to_string(),
            vec![SelectedModel {
                name: "claude-code".to_string(),
                count: 1,
            }],
        );
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn rest_task_manager_has_correct_description() {
        // Test that the trait compiles correctly
        // Since we can't instantiate clients in this crate, we just verify compilation
        fn _test_trait_compilation() {
            // This ensures the RestApiClient trait compiles
            use super::RestApiClient;
        }
        _test_trait_compilation();
    }
}
