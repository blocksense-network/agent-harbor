//! Data models and business logic

use ah_rest_api_contract::*;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Internal session model with additional fields
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternalSession {
    pub session: Session,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub logs: Vec<LogEntry>,
    pub events: Vec<SessionEvent>,
}

/// Session store interface
#[async_trait::async_trait]
pub trait SessionStore: Send + Sync {
    async fn create_session(&self, request: &CreateTaskRequest) -> anyhow::Result<String>;
    async fn get_session(&self, session_id: &str) -> anyhow::Result<Option<InternalSession>>;
    async fn update_session(
        &self,
        session_id: &str,
        session: &InternalSession,
    ) -> anyhow::Result<()>;
    async fn delete_session(&self, session_id: &str) -> anyhow::Result<()>;
    async fn list_sessions(&self, filters: &FilterQuery) -> anyhow::Result<Vec<Session>>;
    async fn add_session_event(&self, session_id: &str, event: SessionEvent) -> anyhow::Result<()>;
    async fn add_session_log(&self, session_id: &str, log: LogEntry) -> anyhow::Result<()>;
    async fn get_session_logs(
        &self,
        session_id: &str,
        query: &LogQuery,
    ) -> anyhow::Result<Vec<LogEntry>>;
    async fn get_session_events(&self, session_id: &str) -> anyhow::Result<Vec<SessionEvent>>;
}

/// In-memory session store implementation (for development/testing)
pub struct InMemorySessionStore {
    sessions: tokio::sync::RwLock<HashMap<String, InternalSession>>,
}

impl InMemorySessionStore {
    pub fn new() -> Self {
        Self {
            sessions: tokio::sync::RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait::async_trait]
impl SessionStore for InMemorySessionStore {
    async fn create_session(&self, request: &CreateTaskRequest) -> anyhow::Result<String> {
        let session_id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now();

        let session = Session {
            id: session_id.clone(),
            tenant_id: request.tenant_id.clone(),
            project_id: request.project_id.clone(),
            task: TaskInfo {
                prompt: request.prompt.clone(),
                attachments: HashMap::new(),
                labels: request.labels.clone(),
            },
            agent: request.agent.clone(),
            runtime: request.runtime.clone(),
            workspace: WorkspaceInfo {
                snapshot_provider: "git".to_string(),     // placeholder
                mount_path: "/tmp/workspace".to_string(), // placeholder
                host: None,
                devcontainer_details: None,
            },
            vcs: VcsInfo {
                repo_url: request.repo.url.as_ref().map(|u| u.to_string()),
                branch: request.repo.branch.clone(),
                commit: request.repo.commit.clone(),
            },
            status: SessionStatus::Queued,
            started_at: None,
            ended_at: None,
            links: SessionLinks {
                self_link: format!("/api/v1/sessions/{}", session_id),
                events: format!("/api/v1/sessions/{}/events", session_id),
                logs: format!("/api/v1/sessions/{}/logs", session_id),
            },
        };

        let internal_session = InternalSession {
            session,
            created_at: now,
            updated_at: now,
            logs: vec![],
            events: vec![],
        };

        let mut sessions = self.sessions.write().await;
        sessions.insert(session_id.clone(), internal_session);

        Ok(session_id)
    }

    async fn get_session(&self, session_id: &str) -> anyhow::Result<Option<InternalSession>> {
        let sessions = self.sessions.read().await;
        Ok(sessions.get(session_id).cloned())
    }

    async fn update_session(
        &self,
        session_id: &str,
        session: &InternalSession,
    ) -> anyhow::Result<()> {
        let mut sessions = self.sessions.write().await;
        sessions.insert(session_id.to_string(), session.clone());
        Ok(())
    }

    async fn delete_session(&self, session_id: &str) -> anyhow::Result<()> {
        let mut sessions = self.sessions.write().await;
        sessions.remove(session_id);
        Ok(())
    }

    async fn list_sessions(&self, filters: &FilterQuery) -> anyhow::Result<Vec<Session>> {
        let sessions = self.sessions.read().await;

        let mut filtered: Vec<Session> = sessions
            .values()
            .filter(|session| {
                if let Some(status_filter) = &filters.status {
                    if &session.session.status.to_string().to_lowercase() != status_filter {
                        return false;
                    }
                }
                if let Some(project_id) = &filters.project_id {
                    if session.session.project_id.as_ref() != Some(project_id) {
                        return false;
                    }
                }
                if let Some(tenant_id) = &filters.tenant_id {
                    if session.session.tenant_id.as_ref() != Some(tenant_id) {
                        return false;
                    }
                }
                true
            })
            .map(|s| s.session.clone())
            .collect();

        // Sort by creation time (newest first)
        filtered.sort_by(|a, b| b.id.cmp(&a.id));

        Ok(filtered)
    }

    async fn add_session_event(&self, session_id: &str, event: SessionEvent) -> anyhow::Result<()> {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id) {
            session.events.push(event);
            session.updated_at = Utc::now();
        }
        Ok(())
    }

    async fn add_session_log(&self, session_id: &str, log: LogEntry) -> anyhow::Result<()> {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id) {
            session.logs.push(log);
            session.updated_at = Utc::now();
        }
        Ok(())
    }

    async fn get_session_logs(
        &self,
        session_id: &str,
        _query: &LogQuery,
    ) -> anyhow::Result<Vec<LogEntry>> {
        let sessions = self.sessions.read().await;
        if let Some(session) = sessions.get(session_id) {
            Ok(session.logs.clone())
        } else {
            Ok(vec![])
        }
    }

    async fn get_session_events(&self, session_id: &str) -> anyhow::Result<Vec<SessionEvent>> {
        let sessions = self.sessions.read().await;
        if let Some(session) = sessions.get(session_id) {
            Ok(session.events.clone())
        } else {
            Ok(vec![])
        }
    }
}
