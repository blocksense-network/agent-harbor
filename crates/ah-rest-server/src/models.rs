// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Data models and business logic

use ah_local_db::{
    Database, SessionRecord, SessionStore as DbSessionStore, TaskRecord, TaskStore as DbTaskStore,
};
use ah_rest_api_contract::*;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

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

/// Database-backed session store implementation
pub struct DatabaseSessionStore {
    db: Arc<Database>,
}

impl DatabaseSessionStore {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Convert API contract SessionStatus to database string
    fn session_status_to_string(status: SessionStatus) -> String {
        match status {
            SessionStatus::Queued => "queued".to_string(),
            SessionStatus::Provisioning => "provisioning".to_string(),
            SessionStatus::Running => "running".to_string(),
            SessionStatus::Pausing => "pausing".to_string(),
            SessionStatus::Paused => "paused".to_string(),
            SessionStatus::Resuming => "resuming".to_string(),
            SessionStatus::Stopping => "stopping".to_string(),
            SessionStatus::Stopped => "stopped".to_string(),
            SessionStatus::Completed => "completed".to_string(),
            SessionStatus::Failed => "failed".to_string(),
            SessionStatus::Cancelled => "cancelled".to_string(),
        }
    }

    /// Convert database string to API contract SessionStatus
    fn string_to_session_status(s: &str) -> SessionStatus {
        match s {
            "queued" => SessionStatus::Queued,
            "provisioning" => SessionStatus::Provisioning,
            "running" => SessionStatus::Running,
            "pausing" => SessionStatus::Pausing,
            "paused" => SessionStatus::Paused,
            "resuming" => SessionStatus::Resuming,
            "stopping" => SessionStatus::Stopping,
            "stopped" => SessionStatus::Stopped,
            "completed" => SessionStatus::Completed,
            "failed" => SessionStatus::Failed,
            "cancelled" => SessionStatus::Cancelled,
            _ => SessionStatus::Queued, // Default fallback
        }
    }

    /// Convert InternalSession to SessionRecord for database storage
    fn internal_session_to_record(session: &InternalSession) -> SessionRecord {
        SessionRecord {
            id: session.session.id.clone(),
            repo_id: None,                              // TODO: Implement repo lookup
            workspace_id: None,                         // TODO: Implement workspace lookup
            agent_id: None,                             // TODO: Implement agent lookup
            runtime_id: None,                           // TODO: Implement runtime lookup
            multiplexer_kind: Some("tmux".to_string()), // Default multiplexer
            mux_session: None,
            mux_window: None,
            pane_left: None,
            pane_right: None,
            pid_agent: None, // Will be set when process starts
            status: Self::session_status_to_string(session.session.status.clone()),
            log_path: None,       // Will be set when recording starts
            workspace_path: None, // Will be set when workspace is provisioned
            started_at: session.session.started_at.unwrap_or_else(|| Utc::now()).to_rfc3339(),
            ended_at: session.session.ended_at.map(|dt| dt.to_rfc3339()),
            agent_config: Some(
                serde_json::to_string(&session.session.agent).unwrap_or_else(|_| "{}".to_string()),
            ),
            runtime_config: Some(
                serde_json::to_string(&session.session.runtime)
                    .unwrap_or_else(|_| "{}".to_string()),
            ),
        }
    }

    /// Convert SessionRecord to InternalSession
    fn record_to_internal_session(
        record: SessionRecord,
        task: Option<TaskRecord>,
    ) -> InternalSession {
        let task_info = if let Some(ref task_record) = task {
            TaskInfo {
                prompt: task_record.prompt.clone(),
                attachments: HashMap::new(), // TODO: Implement attachments
                labels: if let Some(ref labels_str) = task_record.labels {
                    // Parse labels as JSON or simple key=value format
                    // For now, create a simple label
                    let mut labels = HashMap::new();
                    labels.insert("task".to_string(), labels_str.to_string());
                    labels
                } else {
                    HashMap::new()
                },
            }
        } else {
            TaskInfo {
                prompt: "Unknown task".to_string(),
                attachments: HashMap::new(),
                labels: HashMap::new(),
            }
        };

        let session_id = record.id.clone();
        InternalSession {
            session: Session {
                id: record.id,
                tenant_id: None,  // TODO: Implement tenant lookup
                project_id: None, // TODO: Implement project lookup
                task: task_info,
                agent: record
                    .agent_config
                    .as_ref()
                    .and_then(|config| serde_json::from_str(config).ok())
                    .unwrap_or_else(|| AgentConfig {
                        agent_type: "unknown".to_string(),
                        version: "latest".to_string(),
                        settings: HashMap::new(),
                    }),
                runtime: record
                    .runtime_config
                    .as_ref()
                    .and_then(|config| serde_json::from_str(config).ok())
                    .unwrap_or_else(|| RuntimeConfig {
                        runtime_type: RuntimeType::Local,
                        devcontainer_path: None,
                        resources: None,
                    }),
                workspace: WorkspaceInfo {
                    snapshot_provider: "git".to_string(),
                    mount_path: record
                        .workspace_path
                        .unwrap_or_else(|| "/tmp/workspace".to_string()),
                    host: None,
                    devcontainer_details: None,
                },
                vcs: task
                    .as_ref()
                    .map(|task_record| VcsInfo {
                        repo_url: task_record.repo_url.clone(),
                        branch: task_record.branch.clone(),
                        commit: task_record.commit.clone(),
                    })
                    .unwrap_or_else(|| VcsInfo {
                        repo_url: None,
                        branch: None,
                        commit: None,
                    }),
                status: Self::string_to_session_status(&record.status),
                started_at: Some(
                    DateTime::parse_from_rfc3339(&record.started_at)
                        .unwrap_or_else(|_| Utc::now().into())
                        .into(),
                ),
                ended_at: record.ended_at.map(|s| {
                    DateTime::parse_from_rfc3339(&s).unwrap_or_else(|_| Utc::now().into()).into()
                }),
                links: SessionLinks {
                    self_link: format!("/api/v1/sessions/{}", session_id),
                    events: format!("/api/v1/sessions/{}/events", session_id),
                    logs: format!("/api/v1/sessions/{}/logs", session_id),
                },
            },
            created_at: Utc::now(), // TODO: Track creation time
            updated_at: Utc::now(),
            logs: vec![],   // TODO: Implement log storage
            events: vec![], // TODO: Implement event storage
        }
    }
}

#[async_trait::async_trait]
impl SessionStore for DatabaseSessionStore {
    async fn create_session(&self, request: &CreateTaskRequest) -> anyhow::Result<String> {
        let session_id = uuid::Uuid::new_v4().to_string();

        // Create session record first (without foreign keys for now)
        let session_record = SessionRecord {
            id: session_id.clone(),
            repo_id: None,
            workspace_id: None,
            agent_id: None,
            runtime_id: None,
            multiplexer_kind: Some("tmux".to_string()),
            mux_session: None,
            mux_window: None,
            pane_left: None,
            pane_right: None,
            pid_agent: None,
            status: "queued".to_string(),
            log_path: None,
            workspace_path: None,
            started_at: Utc::now().to_rfc3339(),
            ended_at: None,
            agent_config: Some(
                serde_json::to_string(&request.agent).unwrap_or_else(|_| "{}".to_string()),
            ),
            runtime_config: Some(
                serde_json::to_string(&request.runtime).unwrap_or_else(|_| "{}".to_string()),
            ),
        };

        // Store session in database
        let conn = self
            .db
            .connection()
            .lock()
            .map_err(|e| anyhow::anyhow!("Failed to get database connection: {}", e))?;
        let session_store = DbSessionStore::new(&conn);
        session_store.insert(&session_record)?;

        // Create task record
        let task_record = TaskRecord {
            id: 0, // Will be set by database
            session_id: session_id.clone(),
            prompt: request.prompt.clone(),
            repo_url: request.repo.url.as_ref().map(|u| u.to_string()),
            branch: request.repo.branch.clone(),
            commit: request.repo.commit.clone(),
            delivery: None, // TODO: Map delivery config
            instances: None,
            labels: if request.labels.is_empty() {
                None
            } else {
                Some(
                    serde_json::to_string(&request.labels)
                        .unwrap_or_else(|_| "default".to_string()),
                )
            },
            browser_automation: 1, // Default enabled
            browser_profile: None,
            chatgpt_username: None,
            codex_workspace: None,
        };

        // Store task in database
        let task_store = DbTaskStore::new(&conn);
        task_store.insert(&task_record)?;

        Ok(session_id)
    }

    async fn get_session(&self, session_id: &str) -> anyhow::Result<Option<InternalSession>> {
        let conn = self
            .db
            .connection()
            .lock()
            .map_err(|e| anyhow::anyhow!("Failed to get database connection: {}", e))?;
        let session_store = DbSessionStore::new(&conn);
        let task_store = DbTaskStore::new(&conn);

        if let Some(session_record) = session_store.get(session_id)? {
            let task_record = task_store.get_by_session(session_id)?;
            let internal_session = Self::record_to_internal_session(session_record, task_record);
            Ok(Some(internal_session))
        } else {
            Ok(None)
        }
    }

    async fn update_session(
        &self,
        session_id: &str,
        session: &InternalSession,
    ) -> anyhow::Result<()> {
        let record = Self::internal_session_to_record(session);
        let conn = self
            .db
            .connection()
            .lock()
            .map_err(|e| anyhow::anyhow!("Failed to get database connection: {}", e))?;
        let session_store = DbSessionStore::new(&conn);
        session_store.update(&record)?;
        Ok(())
    }

    async fn delete_session(&self, session_id: &str) -> anyhow::Result<()> {
        let conn = self
            .db
            .connection()
            .lock()
            .map_err(|e| anyhow::anyhow!("Failed to get database connection: {}", e))?;
        let session_store = DbSessionStore::new(&conn);
        session_store.delete(session_id)?;
        Ok(())
    }

    async fn list_sessions(&self, filters: &FilterQuery) -> anyhow::Result<Vec<Session>> {
        let conn = self
            .db
            .connection()
            .lock()
            .map_err(|e| anyhow::anyhow!("Failed to get database connection: {}", e))?;
        let session_store = DbSessionStore::new(&conn);
        let task_store = DbTaskStore::new(&conn);

        let session_records = session_store.list()?;
        let mut sessions = Vec::new();

        for record in session_records {
            let task_record = task_store.get_by_session(&record.id)?;
            let internal_session = Self::record_to_internal_session(record, task_record);
            sessions.push(internal_session.session);
        }

        // Apply filters (simplified implementation)
        if let Some(status_filter) = &filters.status {
            sessions.retain(|s| s.status.to_string().to_lowercase() == *status_filter);
        }

        Ok(sessions)
    }

    async fn add_session_event(&self, session_id: &str, event: SessionEvent) -> anyhow::Result<()> {
        // TODO: Implement event storage in database
        Ok(())
    }

    async fn add_session_log(&self, session_id: &str, log: LogEntry) -> anyhow::Result<()> {
        // TODO: Implement log storage in database
        Ok(())
    }

    async fn get_session_logs(
        &self,
        session_id: &str,
        query: &LogQuery,
    ) -> anyhow::Result<Vec<LogEntry>> {
        // TODO: Implement log retrieval from database
        Ok(vec![])
    }

    async fn get_session_events(&self, session_id: &str) -> anyhow::Result<Vec<SessionEvent>> {
        // TODO: Implement event retrieval from database
        Ok(vec![])
    }
}
