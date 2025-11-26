// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use crate::{
    config::ServerConfig,
    models::{InMemorySessionStore, InternalSession, SessionStore},
    state::AppState,
};
use ah_domain_types::{AgentSoftware, AgentSoftwareBuild};
use ah_local_db::Database;
use ah_rest_api_contract::*;
use ah_scenario_format::{
    LegacyAssertion, LegacyAssertEvent, LegacyScenarioEvent, PlaybackEventKind, PlaybackIterator,
    PlaybackOptions, Scenario, ScenarioLoader, ScenarioMatcher, ScenarioRecord, ScenarioSource,
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
}

impl Default for ScenarioPlaybackOptions {
    fn default() -> Self {
        Self {
            scenario_files: Vec::new(),
            speed_multiplier: 1.0,
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
            let loader = ScenarioLoader::from_sources(sources)?;
            Arc::new(ScenarioSessionStore::new(loader, options.speed_multiplier)?)
        };

        let state = AppState {
            db,
            config,
            session_store,
            task_controller: None,
        };

        Ok(Self { state })
    }

    pub fn into_state(self) -> AppState {
        self.state
    }
}

#[derive(Clone)]
struct ScenarioSessionStore {
    inner: Arc<ScenarioSessionStoreInner>,
}

struct ScenarioSessionStoreInner {
    sessions: RwLock<HashMap<String, ScenarioSession>>,
    scenarios: Vec<ScenarioRecord>,
    speed_multiplier: f64,
}

struct ScenarioSession {
    internal: InternalSession,
    events: Vec<SessionEvent>,
    logs: Vec<LogEntry>,
    broadcaster: broadcast::Sender<SessionEvent>,
    tool_counter: u64,
    active_tool: Option<ActiveTool>,
}

struct ActiveTool {
    execution_id: String,
}

impl ScenarioSessionStore {
    fn new(loader: ScenarioLoader, speed_multiplier: f64) -> Result<Self> {
        let scenarios = loader.into_records();
        if scenarios.is_empty() {
            return Err(anyhow::anyhow!(
                "No scenarios were loaded for the mock REST server"
            ));
        }

        Ok(Self {
            inner: Arc::new(ScenarioSessionStoreInner {
                sessions: RwLock::new(HashMap::new()),
                scenarios,
                speed_multiplier,
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
        if !scenario.timeline.is_empty() {
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
                self.apply_playback_event(&session_id, scheduled.kind, scheduled.at_ms)
                    .await?;
            }
        } else if !scenario.legacy_events.is_empty() {
            let mut events = scenario.legacy_events.clone();
            events.sort_by_key(|e| e.at_ms);
            let mut previous = 0_u64;
            for event in events {
                let delay = event.at_ms.saturating_sub(previous);
                previous = event.at_ms;
                if delay > 0 {
                    sleep(Duration::from_millis(delay)).await;
                }
                self.apply_legacy_event(&session_id, &event).await?;
            }
        }

        if !scenario.legacy_assertions.is_empty() {
            self.evaluate_legacy_assertions(&session_id, &scenario.legacy_assertions)
                .await?;
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
                self.push_event(session_id, SessionEvent::thought(text, None, timestamp_ms))
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

    async fn apply_legacy_event(
        &self,
        session_id: &str,
        event: &LegacyScenarioEvent,
    ) -> Result<()> {
        match event.kind.as_str() {
            "status" => {
                let status = match event
                    .value
                    .as_deref()
                    .unwrap_or_default()
                    .to_lowercase()
                    .as_str()
                {
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
                    other => {
                        tracing::warn!("unknown legacy status '{other}', defaulting to running");
                        SessionStatus::Running
                    }
                };
                self.update_status(session_id, status, event.at_ms).await?;
            }
            "log" => {
                let message = event
                    .message
                    .clone()
                    .or_else(|| event.value.clone())
                    .unwrap_or_default();
                self.push_event(
                    session_id,
                    SessionEvent::log(SessionLogLevel::Info, message, None, event.at_ms),
                )
                .await?;
            }
            "thought" => {
                let text = event
                    .text
                    .clone()
                    .or_else(|| event.value.clone())
                    .unwrap_or_default();
                self.push_event(
                    session_id,
                    SessionEvent::thought(text, None, event.at_ms),
                )
                .await?;
            }
            _ => tracing::warn!("Unhandled legacy event kind: {}", event.kind),
        }
        Ok(())
    }

    async fn evaluate_legacy_assertions(
        &self,
        session_id: &str,
        assertions: &[LegacyAssertion],
    ) -> Result<()> {
        for assertion in assertions {
            if assertion.kind == "has_event" {
                if let Some(event) = &assertion.event {
                    if !self.assert_event_present(session_id, event).await? {
                        let msg = format!(
                            "Assertion failed: missing event type={} status={:?}",
                            event.event_type, event.status
                        );
                        self.push_event(
                            session_id,
                            SessionEvent::error(msg.clone(), current_timestamp()),
                        )
                        .await?;
                        self.update_status(session_id, SessionStatus::Failed, current_timestamp())
                            .await?;
                    }
                }
            }
        }
        Ok(())
    }

    async fn assert_event_present(
        &self,
        session_id: &str,
        expected: &LegacyAssertEvent,
    ) -> Result<bool> {
        let sessions = self.inner.sessions.read().await;
        if let Some(record) = sessions.get(session_id) {
            for ev in &record.events {
                match ev {
                    SessionEvent::Status(status) => {
                        if expected.event_type == "status"
                            && expected.status.as_deref()
                                == Some(status.status.to_string().to_lowercase().as_str())
                        {
                            return Ok(true);
                        }
                    }
                    SessionEvent::Log(log) => {
                        if expected.event_type == "log" {
                            if let Some(substr) = &expected.message_contains {
                                let msg = String::from_utf8_lossy(&log.message);
                                if msg.contains(substr) {
                                    return Ok(true);
                                }
                            } else {
                                return Ok(true);
                            }
                        }
                    }
                    SessionEvent::Thought(thought) => {
                        if expected.event_type == "thought" {
                            let text = String::from_utf8_lossy(&thought.thought);
                            if expected
                                .message_contains
                                .as_deref()
                                .map(|needle| text.contains(needle))
                                .unwrap_or(true)
                            {
                                return Ok(true);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(false)
    }

    async fn push_event(&self, session_id: &str, event: SessionEvent) -> Result<()> {
        let mut sessions = self.inner.sessions.write().await;
        if let Some(record) = sessions.get_mut(session_id) {
            record.internal.updated_at = Utc::now();
            record.events.push(event.clone());
            let _ = record.broadcaster.send(event);
        }
        Ok(())
    }

    async fn begin_tool(&self, session_id: &str, _tool_name: &str) -> Result<String> {
        let mut sessions = self.inner.sessions.write().await;
        if let Some(record) = sessions.get_mut(session_id) {
            record.tool_counter += 1;
            let execution_id = format!("tool_exec_{:04}", record.tool_counter);
            record.active_tool = Some(ActiveTool {
                execution_id: execution_id.clone(),
            });
            return Ok(execution_id);
        }
        Ok(format!("tool_exec_{}", Uuid::new_v4().simple()))
    }

    async fn current_tool_id(&self, session_id: &str) -> Option<String> {
        let sessions = self.inner.sessions.read().await;
        sessions
            .get(session_id)
            .and_then(|record| record.active_tool.as_ref().map(|tool| tool.execution_id.clone()))
    }

    async fn end_tool(&self, session_id: &str) {
        let mut sessions = self.inner.sessions.write().await;
        if let Some(record) = sessions.get_mut(session_id) {
            record.active_tool = None;
        }
    }

    fn select_scenario(&self, prompt: &str) -> Option<Scenario> {
        let matcher = ScenarioMatcher::new(&self.inner.scenarios);
        matcher.best_match(prompt).map(|matched| matched.scenario.clone())
    }
}

#[async_trait]
impl SessionStore for ScenarioSessionStore {
    async fn create_session(&self, request: &CreateTaskRequest) -> anyhow::Result<Vec<String>> {
        let scenario = self
            .select_scenario(&request.prompt)
            .ok_or_else(|| anyhow::anyhow!("No scenarios available"))?;

        let now = Utc::now();
        let session_id = Uuid::new_v4().to_string();

        let task = TaskInfo {
            prompt: request.prompt.clone(),
            attachments: HashMap::new(),
            labels: request.labels.clone(),
        };
        let agent = request.agents.first().cloned().unwrap_or_else(|| AgentChoice {
            agent: AgentSoftwareBuild {
                software: AgentSoftware::Claude,
                version: "latest".into(),
            },
            model: "mock".into(),
            count: 1,
            settings: HashMap::new(),
            display_name: Some("mock-agent".into()),
        });
        let runtime = request.runtime.clone();
        let workspace = WorkspaceInfo {
            snapshot_provider: "mock".into(),
            mount_path: format!("/tmp/mock/{session_id}"),
            host: Some("localhost".into()),
            devcontainer_details: None,
        };
        let vcs = VcsInfo {
            repo_url: request.repo.url.as_ref().map(|url| url.to_string()),
            branch: request.repo.branch.clone(),
            commit: request.repo.commit.clone(),
        };

        let session = Session {
            id: session_id.clone(),
            tenant_id: request.tenant_id.clone(),
            project_id: request.project_id.clone(),
            task,
            agent,
            runtime,
            workspace,
            vcs,
            status: SessionStatus::Queued,
            started_at: None,
            ended_at: None,
            links: SessionLinks {
                self_link: format!("/api/v1/sessions/{session_id}"),
                events: format!("/api/v1/sessions/{session_id}/events"),
                logs: format!("/api/v1/sessions/{session_id}/logs"),
            },
        };

        let internal = InternalSession {
            session,
            created_at: now,
            updated_at: now,
            logs: Vec::new(),
            events: Vec::new(),
        };

        let (tx, _) = broadcast::channel(100);
        self.inner.sessions.write().await.insert(
            session_id.clone(),
            ScenarioSession {
                internal,
                events: Vec::new(),
                logs: Vec::new(),
                broadcaster: tx,
                tool_counter: 0,
                active_tool: None,
            },
        );

        self.spawn_playback(session_id.clone(), scenario);

        Ok(vec![session_id])
    }

    async fn get_session(&self, session_id: &str) -> anyhow::Result<Option<InternalSession>> {
        let sessions = self.inner.sessions.read().await;
        Ok(sessions.get(session_id).map(|record| record.internal.clone()))
    }

    async fn update_session(
        &self,
        _session_id: &str,
        _session: &InternalSession,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    async fn delete_session(&self, session_id: &str) -> anyhow::Result<()> {
        self.inner.sessions.write().await.remove(session_id);
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
        let mut sessions = self.inner.sessions.write().await;
        if let Some(record) = sessions.get_mut(session_id) {
            record.logs.push(log.clone());
        }
        drop(sessions);
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
        self.inner.sessions.try_read().ok().and_then(|sessions| {
            sessions.get(session_id).map(|record| record.broadcaster.subscribe())
        })
    }
}

fn current_timestamp() -> u64 {
    Utc::now().timestamp_millis() as u64
}
