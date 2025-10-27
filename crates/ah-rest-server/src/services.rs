// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Business logic services

use crate::models::{InternalSession, SessionStore};
use ah_core::{
    BranchesEnumerator, WorkspaceFilesEnumerator,
    local_branches_enumerator::LocalBranchesEnumerator,
};
use ah_local_db::Database;
use ah_rest_api_contract::*;
use futures::StreamExt;
use std::sync::Arc;

/// Session service for managing session lifecycle
pub struct SessionService<S: SessionStore> {
    store: Arc<S>,
}

impl<S: SessionStore> SessionService<S> {
    pub fn new(store: Arc<S>) -> Self {
        Self { store }
    }

    /// Create a new session from task request
    pub async fn create_session(
        &self,
        request: &CreateTaskRequest,
    ) -> anyhow::Result<CreateTaskResponse> {
        let session_id = self.store.create_session(request).await?;

        Ok(CreateTaskResponse {
            id: session_id.clone(),
            status: SessionStatus::Queued,
            links: TaskLinks {
                self_link: format!("/api/v1/sessions/{}", session_id),
                events: format!("/api/v1/sessions/{}/events", session_id),
                logs: format!("/api/v1/sessions/{}/logs", session_id),
            },
        })
    }

    /// Get session by ID
    pub async fn get_session(&self, session_id: &str) -> anyhow::Result<Option<Session>> {
        if let Some(internal_session) = self.store.get_session(session_id).await? {
            Ok(Some(internal_session.session))
        } else {
            Ok(None)
        }
    }

    /// List sessions with filtering
    pub async fn list_sessions(
        &self,
        filters: &FilterQuery,
    ) -> anyhow::Result<SessionListResponse> {
        let sessions = self.store.list_sessions(filters).await?;
        let total = sessions.len() as u32;

        Ok(SessionListResponse {
            items: sessions,
            next_page: None, // TODO: Implement pagination
            total: Some(total),
        })
    }

    /// Update session status
    pub async fn update_session_status(
        &self,
        session_id: &str,
        status: SessionStatus,
    ) -> anyhow::Result<()> {
        if let Some(mut internal_session) = self.store.get_session(session_id).await? {
            internal_session.session.status = status;
            self.store.update_session(session_id, &internal_session).await?;
        }
        Ok(())
    }

    /// Delete session
    pub async fn delete_session(&self, session_id: &str) -> anyhow::Result<()> {
        self.store.delete_session(session_id).await
    }

    /// Add event to session
    pub async fn add_session_event(
        &self,
        session_id: &str,
        event: SessionEvent,
    ) -> anyhow::Result<()> {
        self.store.add_session_event(session_id, event).await
    }

    /// Add log to session
    pub async fn add_session_log(&self, session_id: &str, log: LogEntry) -> anyhow::Result<()> {
        self.store.add_session_log(session_id, log).await
    }

    /// Get session logs
    pub async fn get_session_logs(
        &self,
        session_id: &str,
        query: &LogQuery,
    ) -> anyhow::Result<SessionLogsResponse> {
        let logs = self.store.get_session_logs(session_id, query).await?;

        Ok(SessionLogsResponse {
            items: logs,
            next_page: None, // TODO: Implement pagination
        })
    }

    /// Get session events
    pub async fn get_session_events(&self, session_id: &str) -> anyhow::Result<Vec<SessionEvent>> {
        self.store.get_session_events(session_id).await
    }
}

/// Task service for managing draft tasks
pub struct TaskService {
    // TODO: Implement draft task storage
}

impl TaskService {
    pub fn new() -> Self {
        Self {}
    }

    // TODO: Implement draft task methods
}

/// Repository service for repository-related operations
pub struct RepositoryService {
    database: Arc<Database>,
}

impl RepositoryService {
    pub fn new(database: Arc<Database>) -> Self {
        Self { database }
    }

    /// Get branches for a repository
    pub async fn get_repository_branches(
        &self,
        repository_id: &str,
    ) -> anyhow::Result<Vec<BranchInfo>> {
        use ah_core::DatabaseManager;
        let db_manager = DatabaseManager::with_database((*self.database).clone());
        let branches_enumerator = LocalBranchesEnumerator::new(db_manager);
        let branches = branches_enumerator.list_branches(repository_id).await;
        Ok(branches
            .into_iter()
            .map(|branch| BranchInfo {
                name: branch.name,
                is_default: branch.is_default,
                last_commit: branch.last_commit,
            })
            .collect())
    }

    /// Get files for a repository
    pub async fn get_repository_files(
        &self,
        repository_id: &str,
    ) -> anyhow::Result<Vec<ah_core::workspace_files_enumerator::RepositoryFile>> {
        use ah_core::DatabaseManager;
        let db_manager = DatabaseManager::with_database((*self.database).clone());

        // Get repository info from database
        let repo_record = db_manager
            .get_repository_by_id(repository_id.parse::<i64>()?)
            .map_err(|e| anyhow::anyhow!("Database error: {}", e))?
            .ok_or_else(|| anyhow::anyhow!("Repository not found: {}", repository_id))?;

        let root_path = repo_record
            .root_path
            .ok_or_else(|| anyhow::anyhow!("Repository has no root path: {}", repository_id))?;

        // Use VcsRepo to get files
        let vcs_repo = ah_repo::VcsRepo::new(&root_path)
            .map_err(|e| anyhow::anyhow!("Failed to open repository at {}: {}", root_path, e))?;

        let mut stream = vcs_repo
            .stream_repository_files()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to stream repository files: {}", e))?;

        let mut files = Vec::new();
        while let Some(result) = stream.next().await {
            match result {
                Ok(file) => files.push(ah_core::workspace_files_enumerator::RepositoryFile {
                    path: file.path,
                    detail: file.detail,
                }),
                Err(e) => return Err(anyhow::anyhow!("Error reading file: {}", e)),
            }
        }

        Ok(files)
    }
}
