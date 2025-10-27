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

    /// Get a clone of the database manager
    pub fn db_manager(&self) -> DatabaseManager {
        self.db_manager.clone()
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
            split_mode: params.split_mode,
            focus: params.focus,
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

    fn description(&self) -> &str {
        "Local Task Manager - executes tasks directly on this machine"
    }
}

/// Type alias for the most common usage: GenericLocalTaskManager with a dynamic multiplexer
pub type LocalTaskManager = GenericLocalTaskManager<Box<dyn Multiplexer + Send + Sync>>;
