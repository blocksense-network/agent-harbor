// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use crate::{
    config::ServerConfig,
    dependencies::TaskController,
    models::{InMemorySessionStore, InternalSession, SessionStore},
    state::AppState,
};
use ah_local_db::Database;
use ah_rest_api_contract::*;
use ah_scenario_format::{
    PlaybackEventKind, PlaybackIterator, PlaybackOptions, Scenario, ScenarioLoader, ScenarioRecord,
    ScenarioSource,
};
use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::{
    sync::{RwLock, broadcast},
    time::{Duration, sleep},
};
use uuid::Uuid;

/// Options configuring scenario playback for the mock REST server.
#[derive(Debug, Clone)]
pub struct ScenarioPlaybackOptions {
    pub scenario_files: Vec<PathBuf>,
    pub speed_multiplier: f64,
    /// Optional linger (seconds) to keep connections open after timeline finishes.
    pub linger_after_timeline_secs: Option<f64>,
}

impl ScenarioPlaybackOptions {
    pub fn with_files(files: Vec<PathBuf>) -> Self {
        Self {
            scenario_files: files,
            ..Default::default()
        }
    }
}

impl Default for ScenarioPlaybackOptions {
    fn default() -> Self {
        Self {
            scenario_files: Vec::new(),
            speed_multiplier: 1.0,
            linger_after_timeline_secs: None,
        }
    }
}

/// Dependency wiring for the mock REST server.
pub struct MockServerDependencies {
    state: AppState,
}

impl MockServerDependencies {
    pub async fn new(config: ServerConfig) -> Result<Self> {
        Self::with_options(config, ScenarioPlaybackOptions::default()).await
    }

    pub async fn with_options(
        config: ServerConfig,
        options: ScenarioPlaybackOptions,
    ) -> Result<Self> {
        let db = Arc::new(Database::open_in_memory()?);

        let session_store: Arc<dyn SessionStore> = if options.scenario_files.is_empty() {
            Arc::new(InMemorySessionStore::new())
        } else {
            let sources = options
                .scenario_files
                .iter()
                .map(|path| {
                    if path.is_dir() {
                        ScenarioSource::Directory(path.clone())
                    } else {
                        ScenarioSource::File(path.clone())
                    }
                })
                .collect::<Vec<_>>();
            match ScenarioLoader::from_sources(sources) {
                Ok(loader) => Arc::new(ScenarioSessionStore::new(
                    loader,
                    options.speed_multiplier,
                    options.linger_after_timeline_secs,
                )?),
                Err(err) => {
                    tracing::warn!(
                        "Falling back to in-memory session store; failed to load scenarios: {err}"
                    );
                    Arc::new(InMemorySessionStore::new())
                }
            }
        };

        let state = AppState {
            db,
            config,
            session_store,
            task_controller: Some(Arc::new(MockTaskController::default())),
        };

        Ok(Self { state })
    }

    pub fn into_state(self) -> AppState {
        self.state
    }
}

#[derive(Default)]
struct MockTaskController {
    // Track injected prompts/bytes per session for test diagnostics
    injected_messages: tokio::sync::Mutex<std::collections::HashMap<String, Vec<Vec<u8>>>>,
}

#[async_trait]
impl TaskController for MockTaskController {
    async fn stop_task(&self, _session_id: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn pause_task(&self, _session_id: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn resume_task(&self, _session_id: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn inject_message(&self, session_id: &str, message: &str) -> anyhow::Result<()> {
        let mut guard = self.injected_messages.lock().await;
        guard
            .entry(session_id.to_string())
            .or_default()
            .push(message.as_bytes().to_vec());
        Ok(())
    }

    async fn inject_bytes(&self, session_id: &str, bytes: &[u8]) -> anyhow::Result<()> {
        let mut guard = self.injected_messages.lock().await;
        guard.entry(session_id.to_string()).or_default().push(bytes.to_vec());
        Ok(())
    }
}

#[derive(Clone)]
struct ScenarioSessionStore {
    inner: Arc<ScenarioSessionStoreInner>,
}

struct ScenarioSessionStoreInner {
    sessions: RwLock<HashMap<String, ScenarioSession>>,
    broadcasters: RwLock<HashMap<String, broadcast::Sender<SessionEvent>>>,
    scenarios: Vec<ScenarioRecord>,
    speed_multiplier: f64,
    linger_after_timeline_secs: Option<f64>,
}

struct ScenarioSession {
    internal: InternalSession,
    events: Vec<SessionEvent>,
    logs: Vec<LogEntry>,
    tool_counter: u64,
    active_tool: Option<ActiveTool>,
}

struct ActiveTool {
    execution_id: String,
}

#[async_trait]
impl SessionStore for ScenarioSessionStore {
    async fn create_session(&self, request: &CreateTaskRequest) -> anyhow::Result<Vec<String>> {
        let mut session_ids = Vec::new();
        let now = Utc::now();
        let now_ms = now.timestamp_millis() as u64;

        for agent_config in &request.agents {
            for instance in 0..agent_config.count {
                let session_id = if request.agents.len() == 1 && agent_config.count == 1 {
                    uuid::Uuid::new_v4().to_string()
                } else if agent_config.count == 1 {
                    let agent_index =
                        request.agents.iter().position(|a| a == agent_config).unwrap();
                    format!("{}-{}", uuid::Uuid::new_v4(), agent_index)
                } else {
                    let agent_index =
                        request.agents.iter().position(|a| a == agent_config).unwrap();
                    format!("{}-{}-{}", uuid::Uuid::new_v4(), agent_index, instance)
                };

                let session = Session {
                    id: session_id.clone(),
                    tenant_id: request.tenant_id.clone(),
                    project_id: request.project_id.clone(),
                    task: TaskInfo {
                        prompt: request.prompt.clone(),
                        attachments: HashMap::new(),
                        labels: request.labels.clone(),
                    },
                    agent: agent_config.clone(),
                    runtime: request.runtime.clone(),
                    workspace: WorkspaceInfo {
                        snapshot_provider: "git".to_string(),
                        mount_path: "/tmp/workspace".to_string(),
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
                    events: vec![SessionEvent::status(SessionStatus::Queued, now_ms)],
                };

                let (tx, _) = broadcast::channel(128);
                tx.send(SessionEvent::status(SessionStatus::Queued, now_ms)).ok();

                {
                    let mut sessions = self.inner.sessions.write().await;
                    sessions.insert(
                        session_id.clone(),
                        ScenarioSession {
                            internal: internal_session,
                            events: vec![],
                            logs: vec![],
                            tool_counter: 0,
                            active_tool: None,
                        },
                    );
                }
                self.inner.broadcasters.write().await.insert(session_id.clone(), tx);

                // Kick off scenario playback for this session using a simple round-robin
                // selection across the loaded scenarios.
                if !self.inner.scenarios.is_empty() {
                    let scenario_idx = session_ids.len() % self.inner.scenarios.len();
                    let scenario = self.inner.scenarios[scenario_idx].scenario.clone();
                    self.spawn_playback(session_id.clone(), scenario);
                }

                session_ids.push(session_id);
            }
        }

        Ok(session_ids)
    }

    async fn get_session(&self, session_id: &str) -> anyhow::Result<Option<InternalSession>> {
        let sessions = self.inner.sessions.read().await;
        Ok(sessions.get(session_id).map(|s| s.internal.clone()))
    }

    async fn update_session(
        &self,
        session_id: &str,
        session: &InternalSession,
    ) -> anyhow::Result<()> {
        {
            let mut sessions = self.inner.sessions.write().await;
            sessions.insert(
                session_id.to_string(),
                ScenarioSession {
                    internal: session.clone(),
                    events: Vec::new(),
                    logs: Vec::new(),
                    tool_counter: 0,
                    active_tool: None,
                },
            );
        }
        self.push_event(
            session_id,
            SessionEvent::status(
                session.session.status.clone(),
                Utc::now().timestamp_millis() as u64,
            ),
        )
        .await
    }

    async fn delete_session(&self, session_id: &str) -> anyhow::Result<()> {
        self.inner.sessions.write().await.remove(session_id);
        self.inner.broadcasters.write().await.remove(session_id);
        Ok(())
    }

    async fn list_sessions(&self, filters: &FilterQuery) -> anyhow::Result<Vec<Session>> {
        let sessions = self.inner.sessions.read().await;
        let mut items: Vec<Session> =
            sessions.values().map(|record| record.internal.session.clone()).collect();

        if let Some(status) = &filters.status {
            items.retain(|session| {
                session.status.to_string().to_lowercase() == status.to_lowercase()
            });
        }

        Ok(items)
    }

    async fn add_session_event(&self, session_id: &str, event: SessionEvent) -> anyhow::Result<()> {
        self.push_event(session_id, event).await
    }

    async fn add_session_log(&self, session_id: &str, log: LogEntry) -> anyhow::Result<()> {
        {
            let mut sessions = self.inner.sessions.write().await;
            if let Some(record) = sessions.get_mut(session_id) {
                record.logs.push(log.clone());
            }
        }
        self.push_event(
            session_id,
            SessionEvent::log(
                SessionLogLevel::Info,
                log.message.clone(),
                None,
                log.ts.timestamp_millis() as u64,
            ),
        )
        .await
    }

    async fn get_session_logs(
        &self,
        session_id: &str,
        _query: &LogQuery,
    ) -> anyhow::Result<Vec<LogEntry>> {
        let sessions = self.inner.sessions.read().await;
        Ok(sessions.get(session_id).map(|record| record.logs.clone()).unwrap_or_default())
    }

    async fn get_session_events(&self, session_id: &str) -> anyhow::Result<Vec<SessionEvent>> {
        let sessions = self.inner.sessions.read().await;
        Ok(sessions.get(session_id).map(|record| record.events.clone()).unwrap_or_default())
    }

    fn subscribe_session_events(
        &self,
        session_id: &str,
    ) -> Option<broadcast::Receiver<SessionEvent>> {
        self.inner
            .broadcasters
            .try_read()
            .ok()
            .and_then(|map| map.get(session_id).map(|tx| tx.subscribe()))
    }
}
impl ScenarioSessionStore {
    fn new(
        loader: ScenarioLoader,
        speed_multiplier: f64,
        linger_after_timeline_secs: Option<f64>,
    ) -> Result<Self> {
        let scenarios = loader.into_records();
        if scenarios.is_empty() {
            return Err(anyhow::anyhow!(
                "No scenarios were loaded for the mock REST server"
            ));
        }

        Ok(Self {
            inner: Arc::new(ScenarioSessionStoreInner {
                sessions: RwLock::new(HashMap::new()),
                broadcasters: RwLock::new(HashMap::new()),
                scenarios,
                speed_multiplier,
                linger_after_timeline_secs,
            }),
        })
    }

    fn spawn_playback(&self, session_id: String, scenario: Scenario) {
        let store = self.clone();
        tokio::spawn(async move {
            if let Err(err) = store.playback_loop(session_id.clone(), scenario).await {
                tracing::error!(
                    "Scenario playback failed for session {}: {err:?}",
                    session_id
                );
                let _ = store
                    .push_event(
                        &session_id,
                        SessionEvent::error("Scenario playback failed".into(), current_timestamp()),
                    )
                    .await;
            }
        });
    }

    async fn playback_loop(&self, session_id: String, scenario: Scenario) -> Result<()> {
        let iterator = PlaybackIterator::new(
            &scenario,
            PlaybackOptions {
                speed_multiplier: self.inner.speed_multiplier,
            },
        )?;
        let mut previous = 0_u64;
        for scheduled in iterator {
            let delay = scheduled.at_ms.saturating_sub(previous);
            previous = scheduled.at_ms;
            if delay > 0 {
                sleep(Duration::from_millis(delay)).await;
            }
            self.apply_playback_event(&session_id, scheduled.kind, scheduled.at_ms).await?;
        }

        if let Some(linger) = self.inner.linger_after_timeline_secs {
            sleep(Duration::from_secs_f64(linger)).await;
        }

        Ok(())
    }

    async fn update_status(
        &self,
        session_id: &str,
        status: SessionStatus,
        timestamp_ms: u64,
    ) -> Result<()> {
        let mut sessions = self.inner.sessions.write().await;
        if let Some(record) = sessions.get_mut(session_id) {
            record.internal.session.status = status.clone();
            if status == SessionStatus::Running && record.internal.session.started_at.is_none() {
                record.internal.session.started_at = Some(Utc::now());
            }
            if matches!(status, SessionStatus::Completed | SessionStatus::Failed) {
                record.internal.session.ended_at = Some(Utc::now());
            }
        }
        drop(sessions);

        self.push_event(session_id, SessionEvent::status(status, timestamp_ms)).await
    }

    async fn apply_playback_event(
        &self,
        session_id: &str,
        kind: PlaybackEventKind,
        timestamp_ms: u64,
    ) -> Result<()> {
        match kind {
            PlaybackEventKind::Status { value } => {
                let status = match value.to_lowercase().as_str() {
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
                    _ => SessionStatus::Running,
                };
                self.update_status(session_id, status, timestamp_ms).await?;
            }
            PlaybackEventKind::Thinking { text } => {
                self.push_event(
                    session_id,
                    SessionEvent::thought(text.clone(), None, timestamp_ms),
                )
                .await?;
                // Also emit a log so early SSE consumers observe activity promptly.
                self.push_event(
                    session_id,
                    SessionEvent::log(
                        SessionLogLevel::Info,
                        format!("thinking: {}", text),
                        None,
                        timestamp_ms,
                    ),
                )
                .await?;
            }
            PlaybackEventKind::Assistant { text } => {
                self.push_event(
                    session_id,
                    SessionEvent::log(
                        SessionLogLevel::Info,
                        format!("assistant: {}", text),
                        None,
                        timestamp_ms,
                    ),
                )
                .await?;
            }
            PlaybackEventKind::Log { message } => {
                self.push_event(
                    session_id,
                    SessionEvent::log(SessionLogLevel::Info, message, None, timestamp_ms),
                )
                .await?;
            }
            PlaybackEventKind::ToolStart { tool_name, args } => {
                let execution_id = self.begin_tool(session_id, &tool_name).await?;
                self.push_event(
                    session_id,
                    SessionEvent::tool_use(
                        tool_name,
                        serde_json::to_string(&args).unwrap_or_else(|_| "{}".into()),
                        execution_id,
                        SessionToolStatus::Started,
                        timestamp_ms,
                    ),
                )
                .await?;
            }
            PlaybackEventKind::ToolProgress {
                tool_name: _,
                message,
            } => {
                self.push_event(
                    session_id,
                    SessionEvent::log(
                        SessionLogLevel::Info,
                        message,
                        self.current_tool_id(session_id).await,
                        timestamp_ms,
                    ),
                )
                .await?;
            }
            PlaybackEventKind::ToolResult {
                tool_name,
                status,
                output,
            } => {
                let execution_id = self
                    .current_tool_id(session_id)
                    .await
                    .unwrap_or_else(|| format!("tool_exec_{}", Uuid::new_v4().simple()));
                let tool_status = if status.eq_ignore_ascii_case("error") {
                    SessionToolStatus::Failed
                } else {
                    SessionToolStatus::Completed
                };
                let output_string = output.map(|v| format!("{:?}", v)).unwrap_or_default();
                self.push_event(
                    session_id,
                    SessionEvent::tool_result(
                        tool_name,
                        output_string,
                        execution_id,
                        tool_status,
                        timestamp_ms,
                    ),
                )
                .await?;
                self.end_tool(session_id).await;
            }
            PlaybackEventKind::FileEdit(data) => {
                self.push_event(
                    session_id,
                    SessionEvent::file_edit(
                        data.path.clone(),
                        data.lines_added as usize,
                        data.lines_removed as usize,
                        None,
                        timestamp_ms,
                    ),
                )
                .await?;
            }
            PlaybackEventKind::UserInput { target, value } => {
                self.push_event(
                    session_id,
                    SessionEvent::log(
                        SessionLogLevel::Info,
                        format!(
                            "user input{}: {}",
                            target.map(|t| format!(" ({})", t)).unwrap_or_else(|| "".into()),
                            value
                        ),
                        None,
                        timestamp_ms,
                    ),
                )
                .await?;
            }
            PlaybackEventKind::UserCommand { cmd, cwd } => {
                self.push_event(
                    session_id,
                    SessionEvent::log(
                        SessionLogLevel::Info,
                        format!(
                            "user command{}: {}",
                            cwd.map(|c| format!(" [cwd: {}]", c)).unwrap_or_else(|| "".into()),
                            cmd
                        ),
                        None,
                        timestamp_ms,
                    ),
                )
                .await?;
            }
            PlaybackEventKind::Screenshot { label } => {
                self.push_event(
                    session_id,
                    SessionEvent::log(
                        SessionLogLevel::Info,
                        format!("screenshot captured: {}", label),
                        None,
                        timestamp_ms,
                    ),
                )
                .await?;
            }
            PlaybackEventKind::Assert(assertion) => {
                self.push_event(
                    session_id,
                    SessionEvent::log(
                        SessionLogLevel::Info,
                        format!("assertion executed: {:?}", assertion),
                        None,
                        timestamp_ms,
                    ),
                )
                .await?;
            }
            PlaybackEventKind::Merge => {
                self.push_event(
                    session_id,
                    SessionEvent::log(
                        SessionLogLevel::Info,
                        "scenario marked for merge".into(),
                        None,
                        timestamp_ms,
                    ),
                )
                .await?;
            }
            PlaybackEventKind::Complete => {
                self.update_status(session_id, SessionStatus::Completed, timestamp_ms).await?;
            }
            PlaybackEventKind::Error(error) => {
                self.push_event(
                    session_id,
                    SessionEvent::error(
                        format!("scenario error: {} ({})", error.message, error.error_type),
                        timestamp_ms,
                    ),
                )
                .await?;
                self.update_status(session_id, SessionStatus::Failed, timestamp_ms).await?;
            }
        }
        Ok(())
    }
    async fn push_event(&self, session_id: &str, event: SessionEvent) -> Result<()> {
        {
            let mut sessions = self.inner.sessions.write().await;
            if let Some(record) = sessions.get_mut(session_id) {
                record.events.push(event.clone());
                if let SessionEvent::Status(status) = &event {
                    record.internal.session.status = status.status.clone();
                    if status.status == SessionStatus::Running
                        && record.internal.session.started_at.is_none()
                    {
                        record.internal.session.started_at = Some(Utc::now());
                    }
                    if matches!(
                        status.status,
                        SessionStatus::Completed | SessionStatus::Failed
                    ) {
                        record.internal.session.ended_at = Some(Utc::now());
                    }
                }
            }
        }

        if let Some(sender) = self.inner.broadcasters.write().await.get(session_id).cloned() {
            let _ = sender.send(event);
        }
        Ok(())
    }

    async fn begin_tool(&self, session_id: &str, tool_name: &str) -> Result<String> {
        let mut sessions = self.inner.sessions.write().await;
        if let Some(record) = sessions.get_mut(session_id) {
            record.tool_counter += 1;
            let execution_id = format!("tool-{}", record.tool_counter);
            record.active_tool = Some(ActiveTool {
                execution_id: execution_id.clone(),
            });
            drop(sessions);
            self.push_event(
                session_id,
                SessionEvent::tool_use(
                    tool_name.to_string(),
                    "".to_string(),
                    execution_id.clone(),
                    SessionToolStatus::Started,
                    current_timestamp(),
                ),
            )
            .await?;
            return Ok(execution_id);
        }
        Err(anyhow::anyhow!("session not found"))
    }

    async fn current_tool_id(&self, session_id: &str) -> Option<String> {
        let sessions = self.inner.sessions.read().await;
        sessions
            .get(session_id)
            .and_then(|record| record.active_tool.as_ref().map(|t| t.execution_id.clone()))
    }

    async fn end_tool(&self, session_id: &str) {
        let mut sessions = self.inner.sessions.write().await;
        if let Some(record) = sessions.get_mut(session_id) {
            record.active_tool = None;
        }
    }
}

fn current_timestamp() -> u64 {
    Utc::now().timestamp_millis() as u64
}
