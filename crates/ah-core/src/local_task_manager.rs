// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Local Task Manager - Direct Task Execution
//!
//! This module provides the LocalTaskManager implementation that executes tasks
//! directly on the local machine without snapshot caching. It's designed for
//! local development and testing scenarios where users want immediate agent execution.
//!
//! Tasks are executed through the configured multiplexer (tmux, kitty, etc.) to provide
//! proper terminal window management and session isolation.

use crate::db::DatabaseManager;
use crate::task_manager::{
    SaveDraftResult, TaskEvent, TaskLaunchParams, TaskLaunchResult, TaskManager,
};
use ah_domain_types::{
    Branch, LogLevel, Repository, SelectedModel, TaskExecution, TaskExecutionStatus, TaskInfo,
    ToolStatus,
};
use ah_local_db::models::DraftRecord;
use ah_mux_core::Multiplexer;
use ah_tui_multiplexer::{AwMultiplexer, LayoutConfig, PaneRole};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::stream::Stream;
use serde_json;
use std::path::PathBuf;
use std::pin::Pin;

/// Generic Local task manager implementation that executes tasks through a multiplexer
///
/// This implementation runs agents directly from the current filesystem state,
/// without the snapshot caching complexity used by the server. Tasks are executed
/// through the configured multiplexer (tmux, kitty, etc.) to provide proper terminal
/// window management and session isolation.
pub struct GenericLocalTaskManager<M: Multiplexer + Send + Sync + 'static> {
    agent_executor: std::sync::Arc<crate::AgentExecutor>,
    db_manager: DatabaseManager,
    multiplexer: AwMultiplexer<M>,
}

impl<M> GenericLocalTaskManager<M>
where
    M: Multiplexer + Send + Sync + 'static,
{
    /// Create a new generic local task manager with the specified multiplexer
    pub fn new(config: crate::AgentExecutionConfig, multiplexer: M) -> anyhow::Result<Self> {
        let agent_executor = std::sync::Arc::new(crate::AgentExecutor::new(config));
        let db_manager = DatabaseManager::new()?;
        let multiplexer = AwMultiplexer::new(multiplexer);
        Ok(Self {
            agent_executor,
            db_manager,
            multiplexer,
        })
    }
}

#[async_trait]
impl<M> TaskManager for GenericLocalTaskManager<M>
where
    M: Multiplexer + Send + Sync + 'static,
{
    async fn launch_task(&self, params: TaskLaunchParams) -> TaskLaunchResult {
        // For local mode, we run agents directly from the current filesystem state
        // without snapshot caching. Tasks are executed through the configured multiplexer
        // to provide proper terminal window management and session isolation.

        let session_id = uuid::Uuid::new_v4().to_string();
        let current_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

        // Get the agent command line from the executor
        let agent_cmd = self.agent_executor.get_agent_command_string(
            &session_id,
            "codex", // Default agent type for local mode
            &params.description,
            crate::WorkingCopyMode::InPlace, // Local mode uses in-place working copy
            Some(&current_dir),
            None, // No snapshot for local mode
        );

        // Create a multiplexer layout for the task
        let layout_config = LayoutConfig {
            task_id: &session_id,
            working_dir: &current_dir,
            editor_cmd: Some("bash"), // Default to bash for editor pane
            agent_cmd: &agent_cmd,
            log_cmd: None, // No separate log command for now
        };

        match self.multiplexer.create_task_layout(&layout_config) {
            Ok(_layout_handle) => {
                // Task layout created successfully in multiplexer
                // The agent command is now running in the multiplexer pane
                TaskLaunchResult::Success {
                    task_id: session_id,
                }
            }
            Err(e) => TaskLaunchResult::Failure {
                error: format!("Failed to create task layout in multiplexer: {}", e),
            },
        }
    }

    fn task_events_stream(&self, _task_id: &str) -> Pin<Box<dyn Stream<Item = TaskEvent> + Send>> {
        // TODO: Implement event streaming from local agent processes
        Box::pin(futures::stream::empty())
    }

    async fn get_initial_tasks(&self) -> (Vec<TaskInfo>, Vec<TaskExecution>) {
        // Get draft tasks from the database
        match self.db_manager.list_drafts() {
            Ok(draft_records) => {
                let draft_tasks: Vec<TaskInfo> =
                    draft_records
                        .into_iter()
                        .map(|draft| {
                            // Parse models JSON
                            let models = match serde_json::from_str::<
                                Vec<ah_domain_types::SelectedModel>,
                            >(&draft.models)
                            {
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
                        })
                        .collect();

                // For completed tasks, return empty for now (could be extended to get from database)
                (draft_tasks, Vec::<TaskExecution>::new())
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
            Ok(repos) => repos
                .into_iter()
                .map(|repo_record| {
                    let remote_url = repo_record.remote_url.as_ref();
                    let root_path = repo_record.root_path.as_ref();
                    Repository {
                        id: repo_record.id.to_string(),
                        name: remote_url
                            .unwrap_or(&root_path.unwrap_or(&"Unknown".to_string()))
                            .clone(),
                        url: remote_url.unwrap_or(&"".to_string()).clone(),
                        default_branch: repo_record
                            .default_branch
                            .unwrap_or_else(|| "main".to_string()),
                    }
                })
                .collect(),
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
                                            let default_branch = repo_record
                                                .default_branch
                                                .unwrap_or_else(|| "main".to_string());
                                            branch_names
                                                .into_iter()
                                                .map(|name| Branch {
                                                    name: name.clone(),
                                                    is_default: name == default_branch,
                                                    last_commit: None, // Could be populated if needed
                                                })
                                                .collect()
                                        }
                                        Err(e) => {
                                            tracing::warn!(
                                                "Failed to get branches for repository {}: {}",
                                                repository_id,
                                                e
                                            );
                                            Vec::new()
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        "Failed to open repository at {}: {}",
                                        root_path,
                                        e
                                    );
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

/// Type alias for the most common usage: GenericLocalTaskManager with a dynamic multiplexer
pub type LocalTaskManager = GenericLocalTaskManager<Box<dyn Multiplexer + Send + Sync>>;
