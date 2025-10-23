// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Session lifecycle management and orchestration.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Unique identifier for a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub u64);

/// Status of a session in its lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionStatus {
    /// Session has been created but not yet started.
    Created,
    /// Session is currently active and running.
    Active,
    /// Session is paused.
    Paused,
    /// Session completed successfully.
    Completed,
    /// Session failed with an error.
    Failed,
    /// Session was cancelled.
    Cancelled,
}

/// Represents a session in the AH system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub task_id: super::task::TaskId,
    pub name: String,
    pub status: SessionStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Session {
    /// Create a new session with the given parameters.
    pub fn new(id: SessionId, task_id: super::task::TaskId, name: String) -> Self {
        let now = chrono::Utc::now();
        Self {
            id,
            task_id,
            name,
            status: SessionStatus::Created,
            created_at: now,
            updated_at: now,
            started_at: None,
            finished_at: None,
            metadata: HashMap::new(),
        }
    }

    /// Update the session status and modification time.
    pub fn update_status(&mut self, status: SessionStatus) {
        self.status = status;
        self.updated_at = chrono::Utc::now();

        match status {
            SessionStatus::Active if self.started_at.is_none() => {
                self.started_at = Some(chrono::Utc::now());
            }
            SessionStatus::Completed | SessionStatus::Failed | SessionStatus::Cancelled => {
                if self.finished_at.is_none() {
                    self.finished_at = Some(chrono::Utc::now());
                }
            }
            _ => {}
        }
    }
}

/// Manages the lifecycle of sessions in the system.
#[derive(Debug, Clone)]
pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<SessionId, Session>>>,
    next_id: Arc<RwLock<SessionId>>,
}

impl SessionManager {
    /// Create a new session manager.
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            next_id: Arc::new(RwLock::new(SessionId(1))),
        }
    }

    /// Create a new session and add it to the manager.
    pub async fn create_session(
        &self,
        task_id: super::task::TaskId,
        name: String,
    ) -> crate::Result<SessionId> {
        let mut next_id = self.next_id.write().await;
        let session_id = *next_id;
        next_id.0 += 1;

        let session = Session::new(session_id, task_id, name);
        self.sessions.write().await.insert(session_id, session);

        Ok(session_id)
    }

    /// Get a session by its ID.
    pub async fn get_session(&self, id: SessionId) -> crate::Result<Option<Session>> {
        Ok(self.sessions.read().await.get(&id).cloned())
    }

    /// Update the status of a session.
    pub async fn update_session_status(
        &self,
        id: SessionId,
        status: SessionStatus,
    ) -> crate::Result<()> {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(&id) {
            session.update_status(status);
            Ok(())
        } else {
            Err(crate::Error::session(format!("Session {} not found", id.0)))
        }
    }

    /// List all sessions for a given task.
    pub async fn list_sessions_for_task(
        &self,
        task_id: super::task::TaskId,
    ) -> crate::Result<Vec<Session>> {
        let sessions = self
            .sessions
            .read()
            .await
            .values()
            .filter(|s| s.task_id == task_id)
            .cloned()
            .collect();
        Ok(sessions)
    }

    /// List all sessions.
    pub async fn list_sessions(&self) -> crate::Result<Vec<Session>> {
        Ok(self.sessions.read().await.values().cloned().collect())
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}
