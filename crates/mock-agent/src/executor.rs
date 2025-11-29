// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Scenario execution engine for the mock ACP client

use agent_client_protocol::{Agent, AgentSideConnection, CLIENT_METHOD_NAMES, Client};
use agent_client_protocol_schema::{
    CancelNotification, CreateTerminalRequest, EnvVariable, InitializeRequest, InitializeResponse,
    LoadSessionRequest, LoadSessionResponse, NewSessionRequest, NewSessionResponse,
    PermissionOption, PermissionOptionId, PermissionOptionKind, PromptRequest, PromptResponse,
    ReadTextFileRequest, ReadTextFileResponse, RequestPermissionRequest, StopReason, ToolCallId,
    ToolCallUpdate, ToolCallUpdateFields, WriteTextFileRequest,
};
use agent_client_protocol_schema::{
    ContentBlock, RequestPermissionOutcome, SessionId, SessionNotification, SessionUpdate,
    TerminalId, TerminalOutputRequest, TextContent, ToolCall, ToolCallContent, ToolCallLocation,
    ToolCallStatus, ToolKind, WaitForTerminalExitRequest,
};
use ah_scenario_format::{
    AgentFileReadsData, AgentPermissionRequestData, AgentPlanData, AssistantStep, FileEditData,
    InitializeData, ResponseElement, Scenario, SessionStartData, TimelineEvent, ToolResultData,
    ToolUseData, UserInputEntry,
};
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use tokio::time::{Duration, sleep};

/// Executor for running ACP scenarios
#[derive(Clone)]
pub struct ScenarioExecutor {
    scenario: Arc<Scenario>,
    elapsed_ms: u64,
}

/// ACP-oriented action derived from a scenario timeline.
#[derive(Debug, Clone)]
pub enum AcpAction {
    Initialize(InitializeData),
    SessionNew {
        session_id: Option<String>,
    },
    SessionLoad {
        session_id: Option<String>,
        historical: Vec<ScheduledAcpAction>,
    },
    Prompt {
        input: UserInputEntry,
    },
    UpdateAssistant {
        steps: Vec<AssistantStep>,
    },
    UpdateToolUse {
        tool: ToolUseData,
    },
    UpdateToolResult {
        result: ToolResultData,
    },
    UpdatePlan {
        plan: AgentPlanData,
    },
    UpdateFileEdit {
        edit: FileEditData,
    },
    Status {
        status: String,
    },
    Log {
        message: String,
    },
    Thought {
        steps: Vec<ah_scenario_format::ThinkingStep>,
    },
    UpdateError {
        error: ah_scenario_format::ErrorData,
    },
    PermissionRequest(AgentPermissionRequestData),
    FileReads(AgentFileReadsData),
    ModeChange(ah_scenario_format::SetModeData),
    ModelChange(ah_scenario_format::SetModelData),
    Cancel,
}

/// Scheduled ACP action with timeline context.
#[derive(Debug, Clone)]
pub struct ScheduledAcpAction {
    pub at_ms: u64,
    pub action: AcpAction,
    pub meta: Option<serde_yaml::Value>,
}

/// Playbook partitions historical vs live actions around sessionStart.
#[derive(Debug)]
pub struct AcpPlaybook {
    pub session_start: Option<SessionStartData>,
    pub historical: Vec<ScheduledAcpAction>,
    pub live: Vec<ScheduledAcpAction>,
}

#[derive(Debug, Default)]
struct ActionsResult {
    notifications: Vec<SessionNotification>,
    cancel: bool,
    permission_requests: Vec<AgentPermissionRequestData>,
    file_reads: Vec<AgentFileReadsData>,
    tool_runs: Vec<ToolReplay>,
    file_writes: Vec<FileEditData>,
}
/// ACP-ready transcript produced from a playbook.
#[derive(Debug)]
pub struct AcpTranscript {
    pub session_id: SessionId,
    pub historical: Vec<SessionNotification>,
    pub live: Vec<SessionNotification>,
    pub cancel_requested: bool,
    pub permission_requests: Vec<AgentPermissionRequestData>,
    pub file_reads: Vec<AgentFileReadsData>,
    pub tool_runs: Vec<ToolReplay>,
    pub file_writes: Vec<FileEditData>,
    pub initialize_response: agent_client_protocol_schema::InitializeResponse,
    pub new_session_response: agent_client_protocol_schema::NewSessionResponse,
    pub cwd_override: Option<std::path::PathBuf>,
    pub mcp_servers_override: Option<serde_json::Value>,
    pub initial_follower_updates: Vec<SessionNotification>,
}

/// Captured tool use for outbound client interactions (e.g., terminals).
#[derive(Debug, Clone)]
pub struct ToolReplay {
    pub id: String,
    pub tool: ToolUseData,
}

/// Scenario-backed ACP Agent implementation (lightweight, in-process).
pub struct ScenarioAgent {
    transcript: AcpTranscript,
    notifier: Mutex<Option<mpsc::UnboundedSender<SessionNotification>>>,
    cancel_flag: Mutex<bool>,
    client_conn: Mutex<Option<Arc<AgentSideConnection>>>,
    load_replayed: Mutex<bool>,
}

impl ScenarioAgent {
    pub fn new(transcript: AcpTranscript) -> Arc<Self> {
        Arc::new(Self {
            transcript,
            notifier: Mutex::new(None),
            cancel_flag: Mutex::new(false),
            client_conn: Mutex::new(None),
            load_replayed: Mutex::new(false),
        })
    }

    /// Attach a notification sink to receive session/update messages.
    pub async fn set_notifier(
        self: &Arc<Self>,
        sender: mpsc::UnboundedSender<SessionNotification>,
    ) {
        let mut guard = self.notifier.lock().await;
        *guard = Some(sender);
    }

    /// Attach a client connection for issuing requests (fs/permission/etc.).
    pub async fn attach_client_connection(self: &Arc<Self>, conn: Arc<AgentSideConnection>) {
        let mut guard = self.client_conn.lock().await;
        *guard = Some(conn);
    }

    /// Helper to stream a vector of notifications to the notifier (if attached).
    async fn stream_notifications(&self, updates: Vec<SessionNotification>) {
        if let Some(tx) = &*self.notifier.lock().await {
            for note in updates {
                let _ = tx.send(note);
            }
            return;
        }

        if let Some(conn) = &*self.client_conn.lock().await {
            for note in updates {
                let _ = conn.notify(
                    CLIENT_METHOD_NAMES.session_update,
                    Some(
                        agent_client_protocol_schema::AgentNotification::SessionNotification(note),
                    ),
                );
            }
        }
    }

    /// Convenience: send a single update.
    #[allow(dead_code)]
    async fn send_update(&self, update: SessionUpdate, meta: Option<serde_json::Value>) {
        self.stream_notifications(vec![SessionNotification {
            session_id: self.transcript.session_id.clone(),
            update,
            meta,
        }])
        .await;
    }
}

#[async_trait::async_trait(?Send)]
impl Agent for ScenarioAgent {
    async fn initialize(
        &self,
        _args: InitializeRequest,
    ) -> Result<InitializeResponse, agent_client_protocol_schema::Error> {
        Ok(self.transcript.initialize_response.clone())
    }

    async fn authenticate(
        &self,
        _args: agent_client_protocol_schema::AuthenticateRequest,
    ) -> Result<
        agent_client_protocol_schema::AuthenticateResponse,
        agent_client_protocol_schema::Error,
    > {
        Ok(agent_client_protocol_schema::AuthenticateResponse::default())
    }

    async fn new_session(
        &self,
        _args: NewSessionRequest,
    ) -> Result<NewSessionResponse, agent_client_protocol_schema::Error> {
        let load_cap = self.transcript.initialize_response.agent_capabilities.load_session;
        // Only replay historical on session/new when loadSession is not advertised.
        if !load_cap {
            self.replay_side_effects().await;
            self.stream_notifications(self.transcript.historical.clone()).await;
        }
        Ok(self.transcript.new_session_response.clone())
    }

    async fn prompt(
        &self,
        _args: PromptRequest,
    ) -> Result<PromptResponse, agent_client_protocol_schema::Error> {
        let cancel = { *self.cancel_flag.lock().await };
        if !cancel {
            self.replay_side_effects().await;
            self.stream_notifications(self.transcript.live.clone()).await;
        }
        Ok(PromptResponse {
            stop_reason: if cancel {
                StopReason::Cancelled
            } else {
                StopReason::EndTurn
            },
            meta: None,
        })
    }

    async fn cancel(
        &self,
        _args: CancelNotification,
    ) -> Result<(), agent_client_protocol_schema::Error> {
        let mut flag = self.cancel_flag.lock().await;
        *flag = true;
        Ok(())
    }

    async fn load_session(
        &self,
        _args: LoadSessionRequest,
    ) -> Result<LoadSessionResponse, agent_client_protocol_schema::Error> {
        let mut replayed = self.load_replayed.lock().await;
        if !*replayed {
            self.replay_side_effects().await;
            self.stream_notifications(self.transcript.historical.clone()).await;
            *replayed = true;
        }
        Ok(LoadSessionResponse::default())
    }

    async fn set_session_mode(
        &self,
        _args: agent_client_protocol_schema::SetSessionModeRequest,
    ) -> Result<
        agent_client_protocol_schema::SetSessionModeResponse,
        agent_client_protocol_schema::Error,
    > {
        Ok(agent_client_protocol_schema::SetSessionModeResponse { meta: None })
    }
}

impl ScenarioExecutor {
    /// Create a new scenario executor
    pub fn new(scenario: Arc<Scenario>) -> Self {
        Self {
            scenario,
            elapsed_ms: 0,
        }
    }

    /// Build an ACP playbook partitioned by sessionStart for loadSession flows.
    pub fn build_playbook(&self) -> AcpPlaybook {
        let partition = self.scenario.partition_by_session_start();
        let historical = schedule_events(&partition.historical);
        let live = schedule_events(&partition.live);

        AcpPlaybook {
            session_start: partition.session_start.cloned(),
            historical,
            live,
        }
    }

    /// Convert the scenario into ACP session notifications (agent-side stream).
    pub fn to_acp_transcript(
        &self,
        provided_session: Option<&str>,
        capability_override: Option<ah_scenario_format::AcpCapabilities>,
        cwd_override: Option<std::path::PathBuf>,
        mcp_servers_override: Option<serde_json::Value>,
    ) -> AcpTranscript {
        let playbook = self.build_playbook();
        let init_event = self.scenario.timeline.iter().find_map(|ev| match ev {
            TimelineEvent::Initialize { initialize } => Some(initialize),
            _ => None,
        });
        let expected_init = init_event.and_then(|i| i.expected_response.as_ref());

        let session_id = SessionId(Arc::from(
            provided_session
                .or_else(|| playbook.session_start.as_ref().and_then(|s| s.session_id.as_deref()))
                .unwrap_or("mock-session"),
        ));

        let historical_res = actions_to_notifications(&session_id, &playbook.historical);
        let live_res = actions_to_notifications(&session_id, &playbook.live);
        let cap_override =
            expected_init.map(|e| e.agent_capabilities.clone()).or(capability_override);
        let agent_capabilities = map_agent_capabilities(self.scenario.as_ref(), cap_override);

        AcpTranscript {
            session_id: session_id.clone(),
            historical: historical_res.notifications,
            live: live_res.notifications,
            cancel_requested: historical_res.cancel || live_res.cancel,
            permission_requests: [
                historical_res.permission_requests,
                live_res.permission_requests,
            ]
            .concat(),
            file_reads: [historical_res.file_reads, live_res.file_reads].concat(),
            tool_runs: [historical_res.tool_runs, live_res.tool_runs].concat(),
            file_writes: [historical_res.file_writes, live_res.file_writes].concat(),
            initialize_response: agent_client_protocol_schema::InitializeResponse {
                protocol_version: expected_init
                    .map(|e| (e.protocol_version as u16).into())
                    .unwrap_or(agent_client_protocol_schema::VERSION),
                agent_capabilities,
                auth_methods: vec![],
                meta: expected_init.and_then(|e| e.meta.as_ref()).map(yaml_to_json),
            },
            new_session_response: agent_client_protocol_schema::NewSessionResponse {
                session_id: session_id.clone(),
                modes: None,
                meta: None,
            },
            cwd_override,
            mcp_servers_override,
            initial_follower_updates: Vec::new(),
        }
    }

    /// Simulate scenario execution (placeholder until ACP SDK is available)
    pub async fn simulate_scenario(&mut self) -> Result<()> {
        tracing::info!("Simulating scenario execution: {}", self.scenario.name);

        // Execute timeline events in simulation mode
        let timeline = self.scenario.timeline.clone();
        for event in &timeline {
            self.simulate_event(event).await?;
        }

        tracing::info!("Scenario simulation completed");
        Ok(())
    }

    /// Simulate a single timeline event
    async fn simulate_event(&mut self, event: &TimelineEvent) -> Result<()> {
        match event {
            TimelineEvent::UserInputs { user_inputs } => {
                // Simulate sending prompts to the agent
                for input in user_inputs {
                    if input.relative_time < self.elapsed_ms {
                        tracing::warn!(
                            "userInputs relativeTime {} earlier than current {}, clamping to current",
                            input.relative_time,
                            self.elapsed_ms
                        );
                    }
                    let target = input.relative_time.max(self.elapsed_ms);
                    let delta = target.saturating_sub(self.elapsed_ms);
                    if delta > 0 {
                        sleep(Duration::from_millis(delta)).await;
                    }
                    self.elapsed_ms = target;

                    let block_count = match &input.input {
                        ah_scenario_format::InputContent::Text(_) => 1,
                        ah_scenario_format::InputContent::Rich(blocks) => blocks.len(),
                    };
                    tracing::info!("Simulating prompt with {} content blocks", block_count);

                    // Simulate prompt response
                    tracing::info!("Prompt simulation completed");
                }
            }

            TimelineEvent::AgentToolUse { agent_tool_use, .. } => {
                self.simulate_tool_use(agent_tool_use).await?;
            }

            TimelineEvent::AgentFileReads {
                agent_file_reads, ..
            } => {
                self.simulate_file_reads(agent_file_reads).await?;
            }

            TimelineEvent::AgentPermissionRequest {
                agent_permission_request,
                ..
            } => {
                self.simulate_permission_request(agent_permission_request).await?;
            }

            TimelineEvent::AdvanceMs { base_time_delta } => {
                self.elapsed_ms = self.elapsed_ms.saturating_add(*base_time_delta);
                sleep(Duration::from_millis(*base_time_delta)).await;
            }

            // Other events are handled at the scenario level or ignored for ACP testing
            _ => {
                tracing::debug!(
                    "Ignoring unsupported event: {:?}",
                    std::mem::discriminant(event)
                );
            }
        }

        Ok(())
    }

    /// Simulate a tool use event (placeholder for ACP terminal calls)
    async fn simulate_tool_use(&self, tool_use: &ToolUseData) -> Result<()> {
        if tool_use.tool_name == "runCmd" {
            // Extract command from args
            if let Some(serde_yaml::Value::String(cmd)) = tool_use.args.get("cmd") {
                tracing::info!("Simulating terminal command: {}", cmd);

                // Simulate terminal creation
                let terminal_id = format!("simulated-terminal-{}", uuid::Uuid::new_v4());
                tracing::info!("Simulated terminal created: {}", terminal_id);

                // Simulate command execution
                tracing::info!("Simulated command execution completed");

                // Simulate terminal cleanup
                tracing::info!("Simulated terminal released: {}", terminal_id);
            }
        } else {
            tracing::info!("Simulating tool use: {}", tool_use.tool_name);
        }

        Ok(())
    }

    /// Simulate file reads (placeholder for ACP filesystem calls)
    async fn simulate_file_reads(&self, file_reads: &AgentFileReadsData) -> Result<()> {
        for file_spec in &file_reads.files {
            tracing::info!("Simulating read of file: {}", file_spec.path);

            // Simulate file read response
            tracing::info!("Simulated file read completed: {} bytes", 42);

            // TODO: Validate against expected_content if provided
        }

        Ok(())
    }

    /// Simulate permission requests
    async fn simulate_permission_request(
        &self,
        permission_request: &AgentPermissionRequestData,
    ) -> Result<()> {
        tracing::info!(
            "Simulating permission request for tool call: {:?}",
            permission_request.tool_call
        );

        // Simulate permission approval
        let approved_option = permission_request
            .options
            .as_ref()
            .and_then(|opts| opts.first())
            .map(|opt| opt.id.clone())
            .unwrap_or_else(|| "allow".to_string());

        tracing::info!(
            "Permission simulation completed - approved: {}",
            approved_option
        );

        Ok(())
    }
}

fn push_actions_for_event(
    event: &TimelineEvent,
    timeline_ms: u64,
    bucket: &mut Vec<ScheduledAcpAction>,
) {
    match event {
        TimelineEvent::Initialize { initialize } => {
            bucket.push(ScheduledAcpAction {
                at_ms: timeline_ms,
                action: AcpAction::Initialize(initialize.clone()),
                meta: initialize.meta.clone(),
            });
        }
        TimelineEvent::SessionStart { .. } => {
            // handled by partitioner; no direct action
        }
        TimelineEvent::UserInputs { user_inputs } => {
            for input in user_inputs {
                bucket.push(ScheduledAcpAction {
                    at_ms: timeline_ms.saturating_add(input.relative_time),
                    action: AcpAction::Prompt {
                        input: input.clone(),
                    },
                    meta: input.meta.clone(),
                });
            }
        }
        TimelineEvent::LlmResponse { llm_response, meta } => {
            for element in llm_response {
                match element {
                    ResponseElement::Think { think } => {
                        bucket.push(ScheduledAcpAction {
                            at_ms: timeline_ms,
                            action: AcpAction::Thought {
                                steps: think.clone(),
                            },
                            meta: meta.clone(),
                        });
                    }
                    ResponseElement::Assistant { assistant } => {
                        bucket.push(ScheduledAcpAction {
                            at_ms: timeline_ms,
                            action: AcpAction::UpdateAssistant {
                                steps: assistant.clone(),
                            },
                            meta: meta.clone(),
                        });
                    }
                    ResponseElement::AgentToolUse { agent_tool_use } => {
                        bucket.push(ScheduledAcpAction {
                            at_ms: timeline_ms,
                            action: AcpAction::UpdateToolUse {
                                tool: agent_tool_use.clone(),
                            },
                            meta: meta.clone(),
                        })
                    }
                    ResponseElement::AgentEdits { agent_edits } => {
                        bucket.push(ScheduledAcpAction {
                            at_ms: timeline_ms,
                            action: AcpAction::UpdateFileEdit {
                                edit: agent_edits.clone(),
                            },
                            meta: meta.clone(),
                        })
                    }
                    ResponseElement::ToolResult { tool_result } => {
                        bucket.push(ScheduledAcpAction {
                            at_ms: timeline_ms,
                            action: AcpAction::UpdateToolResult {
                                result: tool_result.clone(),
                            },
                            meta: meta.clone(),
                        })
                    }
                    ResponseElement::AgentPlan { agent_plan } => bucket.push(ScheduledAcpAction {
                        at_ms: timeline_ms,
                        action: AcpAction::UpdatePlan {
                            plan: agent_plan.clone(),
                        },
                        meta: meta.clone(),
                    }),
                    ResponseElement::Error { error } => bucket.push(ScheduledAcpAction {
                        at_ms: timeline_ms,
                        action: AcpAction::UpdateError {
                            error: error.clone(),
                        },
                        meta: meta.clone(),
                    }),
                }
            }
        }
        TimelineEvent::AgentToolUse {
            agent_tool_use,
            meta,
        } => {
            bucket.push(ScheduledAcpAction {
                at_ms: timeline_ms,
                action: AcpAction::UpdateToolUse {
                    tool: agent_tool_use.clone(),
                },
                meta: meta.clone(),
            });
        }
        TimelineEvent::AgentPlan { agent_plan, meta } => {
            bucket.push(ScheduledAcpAction {
                at_ms: timeline_ms,
                action: AcpAction::UpdatePlan {
                    plan: agent_plan.clone(),
                },
                meta: meta.clone(),
            });
        }
        TimelineEvent::AgentFileReads {
            agent_file_reads,
            meta,
        } => {
            bucket.push(ScheduledAcpAction {
                at_ms: timeline_ms,
                action: AcpAction::FileReads(agent_file_reads.clone()),
                meta: meta.clone(),
            });
        }
        TimelineEvent::AgentPermissionRequest {
            agent_permission_request,
            meta,
        } => {
            bucket.push(ScheduledAcpAction {
                at_ms: timeline_ms,
                action: AcpAction::PermissionRequest(agent_permission_request.clone()),
                meta: meta.clone(),
            });
        }
        TimelineEvent::SetMode { set_mode, meta } => {
            bucket.push(ScheduledAcpAction {
                at_ms: timeline_ms,
                action: AcpAction::ModeChange(set_mode.clone()),
                meta: meta.clone(),
            });
        }
        TimelineEvent::SetModel { set_model, meta } => {
            bucket.push(ScheduledAcpAction {
                at_ms: timeline_ms,
                action: AcpAction::ModelChange(set_model.clone()),
                meta: meta.clone(),
            });
        }
        TimelineEvent::UserCancelSession { .. } => {
            bucket.push(ScheduledAcpAction {
                at_ms: timeline_ms,
                action: AcpAction::Cancel,
                meta: None,
            });
        }
        TimelineEvent::AgentEdits { agent_edits, meta } => {
            bucket.push(ScheduledAcpAction {
                at_ms: timeline_ms,
                action: AcpAction::UpdateFileEdit {
                    edit: agent_edits.clone(),
                },
                meta: meta.clone(),
            });
        }
        TimelineEvent::Status { status } => {
            bucket.push(ScheduledAcpAction {
                at_ms: timeline_ms,
                action: AcpAction::Status {
                    status: status.clone(),
                },
                meta: None,
            });
        }
        TimelineEvent::Log { log, meta } => {
            bucket.push(ScheduledAcpAction {
                at_ms: timeline_ms,
                action: AcpAction::Log {
                    message: log.clone(),
                },
                meta: meta.clone(),
            });
        }
        TimelineEvent::AdvanceMs { .. }
        | TimelineEvent::Complete { .. }
        | TimelineEvent::Merge { .. }
        | TimelineEvent::Assert { .. }
        | TimelineEvent::Screenshot { .. } => {}
    }
}

fn schedule_events(events: &[&TimelineEvent]) -> Vec<ScheduledAcpAction> {
    let mut out = Vec::new();
    let mut timeline_ms = 0u64;
    for event in events {
        match event {
            TimelineEvent::AdvanceMs { base_time_delta } => {
                timeline_ms = timeline_ms.saturating_add(*base_time_delta);
                continue;
            }
            TimelineEvent::SessionStart { .. } => {
                timeline_ms = 0;
                continue;
            }
            _ => {}
        }
        push_actions_for_event(event, timeline_ms, &mut out);
    }
    out
}

fn actions_to_notifications(
    session_id: &SessionId,
    actions: &[ScheduledAcpAction],
) -> ActionsResult {
    let mut notifications = Vec::new();
    let mut cancel = false;
    let mut permission_requests = Vec::new();
    let mut file_reads = Vec::new();
    let mut tool_runs = Vec::new();
    let mut file_writes = Vec::new();
    let mut tool_ids: HashMap<String, agent_client_protocol_schema::ToolCallId> = HashMap::new();
    let mut tool_seq = 0usize;

    for action in actions {
        let update_opt = match &action.action {
            AcpAction::Prompt { input } => match &input.input {
                ah_scenario_format::InputContent::Text(_) => {
                    let content = to_content_block(&input.input, input.meta.as_ref());
                    Some(SessionUpdate::UserMessageChunk { content })
                }
                ah_scenario_format::InputContent::Rich(blocks) => {
                    if blocks.is_empty() {
                        None
                    } else {
                        for block in blocks {
                            notifications.push(SessionNotification {
                                session_id: session_id.clone(),
                                update: SessionUpdate::UserMessageChunk {
                                    content: map_rich_block(block),
                                },
                                meta: input.meta.as_ref().map(yaml_to_json),
                            });
                        }
                        None
                    }
                }
            },
            AcpAction::Thought { steps } => {
                for step in steps {
                    notifications.push(SessionNotification {
                        session_id: session_id.clone(),
                        update: SessionUpdate::AgentThoughtChunk {
                            content: ContentBlock::Text(TextContent {
                                annotations: None,
                                text: step.content.clone(),
                                meta: None,
                            }),
                        },
                        meta: action.meta.as_ref().map(yaml_to_json),
                    });
                }
                None
            }
            AcpAction::UpdateAssistant { steps } => {
                // Stream each assistant step as its own chunk to preserve timing semantics.
                for step in steps {
                    notifications.push(SessionNotification {
                        session_id: session_id.clone(),
                        update: SessionUpdate::AgentMessageChunk {
                            content: to_content_block_from_response(&step.content),
                        },
                        meta: action.meta.as_ref().map(yaml_to_json),
                    });
                }
                None
            }
            AcpAction::UpdateToolUse { tool } => {
                let key = extract_tool_call_key(tool, tool_seq);
                let id = tool_ids
                    .entry(key.clone())
                    .or_insert_with(|| agent_client_protocol_schema::ToolCallId(Arc::from(key)))
                    .clone();
                tool_seq += 1;
                tool_runs.push(ToolReplay {
                    id: id.0.to_string(),
                    tool: tool.clone(),
                });
                let raw_input = serde_json::to_value(&tool.args).ok();
                let status = map_tool_status(
                    tool.status.as_deref(),
                    agent_client_protocol_schema::ToolCallStatus::InProgress,
                );
                notifications.push(SessionNotification {
                    session_id: session_id.clone(),
                    update: SessionUpdate::ToolCall(ToolCall {
                        id: id.clone(),
                        title: tool.tool_name.clone(),
                        kind: ToolKind::Execute,
                        status,
                        content: vec![ToolCallContent::Terminal {
                            terminal_id: id.0.to_string().into(),
                        }],
                        locations: vec![],
                        raw_input,
                        raw_output: None,
                        meta: action.meta.as_ref().map(yaml_to_json),
                    }),
                    meta: action.meta.as_ref().map(yaml_to_json),
                });

                // Emit an initial ToolCallUpdate so clients that only listen for updates still receive the event.
                notifications.push(SessionNotification {
                    session_id: session_id.clone(),
                    update: SessionUpdate::ToolCallUpdate(
                        agent_client_protocol_schema::ToolCallUpdate {
                            id: id.clone(),
                            fields: agent_client_protocol_schema::ToolCallUpdateFields {
                                status: Some(status),
                                ..Default::default()
                            },
                            meta: action.meta.as_ref().map(yaml_to_json),
                        },
                    ),
                    meta: action.meta.as_ref().map(yaml_to_json),
                });

                if let Some(progress) = &tool.progress {
                    for step in progress {
                        notifications.push(SessionNotification {
                            session_id: session_id.clone(),
                            update: SessionUpdate::ToolCallUpdate(agent_client_protocol_schema::ToolCallUpdate {
                                id: id.clone(),
                                fields: agent_client_protocol_schema::ToolCallUpdateFields {
                                    status: Some(agent_client_protocol_schema::ToolCallStatus::InProgress),
                                    content: Some(vec![ToolCallContent::Content {
                                        content: ContentBlock::Text(TextContent {
                                            annotations: None,
                                            text: step.message.clone(),
                                            meta: None,
                                        }),
                                    }]),
                                    ..Default::default()
                                },
                                meta: action.meta.as_ref().map(yaml_to_json),
                            }),
                            meta: action.meta.as_ref().map(yaml_to_json),
                        });

                        // Optional validation hook: if expect_output is provided, emit a warning if mismatch occurs.
                        if let Some(expected) = &step.expect_output {
                            if step.message.trim() != expected.trim() {
                                tracing::warn!(
                                    tool_call_id = %id.0,
                                    expected,
                                    got = %step.message,
                                    "progress output mismatch"
                                );
                            }
                        }
                    }
                }

                if let Some(exec) = &tool.tool_execution {
                    for event in &exec.events {
                        let status = match event.kind.as_str() {
                            "completion" => Some(map_exit_status(event.exit_code)),
                            _ => None,
                        };
                        let content = match &event.content {
                            Some(text) if !text.is_empty() => {
                                Some(vec![ToolCallContent::Content {
                                    content: ContentBlock::Text(TextContent {
                                        annotations: None,
                                        text: text.clone(),
                                        meta: None,
                                    }),
                                }])
                            }
                            _ => None,
                        };
                        notifications.push(SessionNotification {
                            session_id: session_id.clone(),
                            update: SessionUpdate::ToolCallUpdate(
                                agent_client_protocol_schema::ToolCallUpdate {
                                    id: id.clone(),
                                    fields: agent_client_protocol_schema::ToolCallUpdateFields {
                                        status,
                                        content,
                                        ..Default::default()
                                    },
                                    meta: action.meta.as_ref().map(yaml_to_json),
                                },
                            ),
                            meta: action.meta.as_ref().map(yaml_to_json),
                        });
                    }
                }

                None
            }
            AcpAction::UpdateToolResult { result } => {
                let id = tool_ids
                    .entry(result.tool_call_id.clone())
                    .or_insert_with(|| {
                        agent_client_protocol_schema::ToolCallId(Arc::from(
                            result.tool_call_id.as_str(),
                        ))
                    })
                    .clone();
                let fields = agent_client_protocol_schema::ToolCallUpdateFields {
                    status: Some(if result.is_error {
                        agent_client_protocol_schema::ToolCallStatus::Failed
                    } else {
                        agent_client_protocol_schema::ToolCallStatus::Completed
                    }),
                    raw_output: serde_json::to_value(&result.content).ok(),
                    ..Default::default()
                };
                Some(SessionUpdate::ToolCallUpdate(
                    agent_client_protocol_schema::ToolCallUpdate {
                        id,
                        fields,
                        meta: action.meta.as_ref().map(yaml_to_json),
                    },
                ))
            }
            AcpAction::UpdatePlan { plan } => {
                let entries = plan
                    .entries
                    .iter()
                    .map(|entry| agent_client_protocol_schema::PlanEntry {
                        content: entry.content.clone(),
                        priority: match entry.priority.as_str() {
                            "high" => agent_client_protocol_schema::PlanEntryPriority::High,
                            "medium" => agent_client_protocol_schema::PlanEntryPriority::Medium,
                            "low" => agent_client_protocol_schema::PlanEntryPriority::Low,
                            _ => agent_client_protocol_schema::PlanEntryPriority::Low,
                        },
                        status: match entry.status.as_str() {
                            "in_progress" => {
                                agent_client_protocol_schema::PlanEntryStatus::InProgress
                            }
                            "completed" => agent_client_protocol_schema::PlanEntryStatus::Completed,
                            _ => agent_client_protocol_schema::PlanEntryStatus::Pending,
                        },
                        meta: None,
                    })
                    .collect();
                Some(SessionUpdate::Plan(agent_client_protocol_schema::Plan {
                    entries,
                    meta: action.meta.as_ref().map(yaml_to_json),
                }))
            }
            AcpAction::UpdateError { error } => Some(SessionUpdate::AgentMessageChunk {
                content: ContentBlock::Text(TextContent {
                    annotations: None,
                    text: format!("Error: {} ({})", error.message, error.error_type),
                    meta: None,
                }),
            }),
            AcpAction::Status { status } => Some(SessionUpdate::AgentMessageChunk {
                content: ContentBlock::Text(TextContent {
                    annotations: None,
                    text: format!("[status] {}", status),
                    meta: None,
                }),
            }),
            AcpAction::Log { message } => Some(SessionUpdate::AgentMessageChunk {
                content: ContentBlock::Text(TextContent {
                    annotations: None,
                    text: format!("[log] {}", message),
                    meta: None,
                }),
            }),
            AcpAction::ModeChange(mode) => Some(SessionUpdate::CurrentModeUpdate {
                current_mode_id: agent_client_protocol_schema::SessionModeId(Arc::from(
                    mode.mode_id.clone(),
                )),
            }),
            AcpAction::PermissionRequest(req) => {
                permission_requests.push(req.clone());
                None
            }
            AcpAction::FileReads(reads) => {
                file_reads.push(reads.clone());
                None
            }
            AcpAction::UpdateFileEdit { edit } => {
                file_writes.push(edit.clone());
                None
            }
            AcpAction::Cancel => {
                cancel = true;
                None // session/cancel is handled as a notification from the client, not emitted by agent
            }
            _ => None, // Tool calls, plans, etc. will be added when mock agent wiring is implemented
        };

        if let Some(update) = update_opt {
            notifications.push(SessionNotification {
                session_id: session_id.clone(),
                update,
                meta: action.meta.as_ref().map(yaml_to_json),
            });
        }
    }
    ActionsResult {
        notifications,
        cancel,
        permission_requests,
        file_reads,
        tool_runs,
        file_writes,
    }
}

fn to_content_block(
    input: &ah_scenario_format::InputContent,
    meta: Option<&serde_yaml::Value>,
) -> ContentBlock {
    match input {
        ah_scenario_format::InputContent::Text(text) => ContentBlock::Text(TextContent {
            annotations: None,
            text: text.clone(),
            meta: meta.map(yaml_to_json),
        }),
        ah_scenario_format::InputContent::Rich(blocks) => {
            if let Some(block) = blocks.first() {
                map_rich_block(block)
            } else {
                ContentBlock::Text(TextContent {
                    annotations: None,
                    text: String::new(),
                    meta: meta.map(yaml_to_json),
                })
            }
        }
    }
}

fn yaml_to_json(value: &serde_yaml::Value) -> serde_json::Value {
    serde_json::to_value(value).unwrap_or(serde_json::Value::Null)
}

fn map_agent_capabilities(
    scenario: &ah_scenario_format::Scenario,
    overrides: Option<ah_scenario_format::AcpCapabilities>,
) -> agent_client_protocol_schema::AgentCapabilities {
    let mut caps = agent_client_protocol_schema::AgentCapabilities::default();
    let merged = merge_capabilities(
        scenario.acp.as_ref().and_then(|a| a.capabilities.clone()),
        overrides,
    );
    if let Some(c) = merged {
        caps.load_session = c.load_session.unwrap_or(false);
        if let Some(prompt) = c.prompt_capabilities {
            caps.prompt_capabilities.image = prompt.image.unwrap_or(false);
            caps.prompt_capabilities.audio = prompt.audio.unwrap_or(false);
            caps.prompt_capabilities.embedded_context = prompt.embedded_context.unwrap_or(false);
        }
        if let Some(mcp) = c.mcp_capabilities {
            caps.mcp_capabilities.http = mcp.http.unwrap_or(false);
            caps.mcp_capabilities.sse = mcp.sse.unwrap_or(false);
        }
    }
    caps
}

fn map_permission_option_kind(kind: &str) -> PermissionOptionKind {
    match kind {
        "allow_always" => PermissionOptionKind::AllowAlways,
        "reject_once" => PermissionOptionKind::RejectOnce,
        "reject_always" => PermissionOptionKind::RejectAlways,
        _ => PermissionOptionKind::AllowOnce,
    }
}

fn to_content_block_from_response(content: &ah_scenario_format::ContentBlock) -> ContentBlock {
    match content {
        ah_scenario_format::ContentBlock::Text(text) => ContentBlock::Text(TextContent {
            annotations: None,
            text: text.clone(),
            meta: None,
        }),
        ah_scenario_format::ContentBlock::Rich(block) => map_rich_block(block),
    }
}

fn map_rich_block(block: &ah_scenario_format::RichContentBlock) -> ContentBlock {
    match block {
        ah_scenario_format::RichContentBlock::Image {
            data,
            mime_type,
            path,
            ..
        } => ContentBlock::Image(agent_client_protocol_schema::ImageContent {
            annotations: None,
            data: data.clone().unwrap_or_default(),
            mime_type: mime_type.clone(),
            uri: path.clone(),
            meta: None,
        }),
        ah_scenario_format::RichContentBlock::Audio {
            data, mime_type, ..
        } => ContentBlock::Audio(agent_client_protocol_schema::AudioContent {
            annotations: None,
            data: data.clone().unwrap_or_default(),
            mime_type: mime_type.clone(),
            meta: None,
        }),
        ah_scenario_format::RichContentBlock::Resource { resource, .. } => {
            let ah_scenario_format::EmbeddedResource {
                uri,
                mime_type,
                text,
            } = resource;
            ContentBlock::Resource(agent_client_protocol_schema::EmbeddedResource {
                annotations: None,
                resource:
                    agent_client_protocol_schema::EmbeddedResourceResource::TextResourceContents(
                        agent_client_protocol_schema::TextResourceContents {
                            mime_type: Some(mime_type.clone()),
                            text: text.clone().unwrap_or_default(),
                            uri: uri.clone(),
                            meta: None,
                        },
                    ),
                meta: None,
            })
        }
        _ => {
            let text = ah_scenario_format::extract_text_from_rich_content_block(block)
                .unwrap_or_else(|| serde_yaml::to_string(block).unwrap_or_default());
            ContentBlock::Text(TextContent {
                annotations: None,
                text,
                meta: None,
            })
        }
    }
}

fn extract_tool_call_key(tool: &ToolUseData, seq: usize) -> String {
    if let Some(id) = tool.tool_call_id.as_ref() {
        return id.to_string();
    }
    if let Some(id) = tool.args.get("toolCallId").and_then(|v| v.as_str()) {
        return id.to_string();
    }
    format!("{}-{}", tool.tool_name, seq)
}

fn map_tool_status(status: Option<&str>, default: ToolCallStatus) -> ToolCallStatus {
    match status {
        Some("pending") => ToolCallStatus::Pending,
        Some("in_progress") => ToolCallStatus::InProgress,
        Some("ok") | Some("completed") => ToolCallStatus::Completed,
        Some("error") | Some("failed") => ToolCallStatus::Failed,
        _ => default,
    }
}

fn map_exit_status(exit_code: Option<i32>) -> ToolCallStatus {
    match exit_code {
        Some(code) if code != 0 => ToolCallStatus::Failed,
        _ => ToolCallStatus::Completed,
    }
}

fn tool_call_update_from_yaml(
    raw: Option<&serde_yaml::Value>,
    idx: usize,
) -> (ToolCallId, ToolCallUpdateFields) {
    let mut fields = ToolCallUpdateFields::default();
    let mut id_str: Option<String> = None;

    if let Some(raw) = raw {
        match raw {
            serde_yaml::Value::String(s) => id_str = Some(s.clone()),
            serde_yaml::Value::Mapping(map) => {
                for (k, v) in map {
                    let key = k.as_str().unwrap_or_default();
                    match key {
                        "toolCallId" | "id" => {
                            if let Some(s) = v.as_str() {
                                id_str = Some(s.to_string());
                            }
                        }
                        "title" => {
                            if let Some(s) = v.as_str() {
                                fields.title = Some(s.to_string());
                            }
                        }
                        "kind" => {
                            if let Some(s) = v.as_str() {
                                fields.kind = Some(map_tool_kind(s));
                            }
                        }
                        "status" => {
                            if let Some(s) = v.as_str() {
                                fields.status =
                                    Some(map_tool_status(Some(s), ToolCallStatus::Pending));
                            }
                        }
                        "locations" => {
                            if let Some(locations) = parse_locations(v) {
                                fields.locations = Some(locations);
                            }
                        }
                        "rawInput" => fields.raw_input = serde_json::to_value(v).ok(),
                        "rawOutput" => fields.raw_output = serde_json::to_value(v).ok(),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    let id = ToolCallId(Arc::from(
        id_str.unwrap_or_else(|| format!("scenario-permission-{}", idx)),
    ));
    (id, fields)
}

fn parse_locations(raw: &serde_yaml::Value) -> Option<Vec<ToolCallLocation>> {
    let mut out = Vec::new();
    match raw {
        serde_yaml::Value::Sequence(seq) => {
            for item in seq {
                if let Some(loc) = location_from_value(item) {
                    out.push(loc);
                }
            }
        }
        other => {
            if let Some(loc) = location_from_value(other) {
                out.push(loc);
            }
        }
    }
    if out.is_empty() { None } else { Some(out) }
}

fn location_from_value(value: &serde_yaml::Value) -> Option<ToolCallLocation> {
    match value {
        serde_yaml::Value::String(path) => Some(ToolCallLocation {
            path: std::path::PathBuf::from(path),
            line: None,
            meta: None,
        }),
        serde_yaml::Value::Mapping(map) => {
            let mut path: Option<String> = None;
            let mut line: Option<i64> = None;
            for (k, v) in map {
                let key = k.as_str().unwrap_or_default();
                match key {
                    "path" => path = v.as_str().map(|s| s.to_string()),
                    "line" => line = v.as_i64(),
                    _ => {}
                }
            }
            path.map(|p| ToolCallLocation {
                path: std::path::PathBuf::from(p),
                line: line.map(|l| l as u32),
                meta: None,
            })
        }
        _ => None,
    }
}

fn map_tool_kind(kind: &str) -> ToolKind {
    match kind {
        "read" => ToolKind::Read,
        "edit" => ToolKind::Edit,
        "delete" => ToolKind::Delete,
        "move" => ToolKind::Move,
        "search" => ToolKind::Search,
        "terminal" | "execute" | "run" => ToolKind::Execute,
        "think" => ToolKind::Think,
        "fetch" => ToolKind::Fetch,
        "switch_mode" => ToolKind::SwitchMode,
        _ => ToolKind::Other,
    }
}

fn map_env_vars(raw: Option<&serde_yaml::Value>) -> Vec<EnvVariable> {
    match raw {
        Some(serde_yaml::Value::Mapping(map)) => map
            .iter()
            .filter_map(|(k, v)| {
                let key = k.as_str()?;
                let val = v.as_str().unwrap_or_default().to_string();
                Some(EnvVariable {
                    name: key.to_string(),
                    value: val,
                    meta: None,
                })
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn merge_capabilities(
    base: Option<ah_scenario_format::AcpCapabilities>,
    overrides: Option<ah_scenario_format::AcpCapabilities>,
) -> Option<ah_scenario_format::AcpCapabilities> {
    match (base, overrides) {
        (None, None) => None,
        (Some(mut b), Some(o)) => {
            if o.load_session.is_some() {
                b.load_session = o.load_session;
            }
            if let Some(mut prompt) = b.prompt_capabilities.clone() {
                if let Some(op) = o.prompt_capabilities {
                    if op.image.is_some() {
                        prompt.image = op.image;
                    }
                    if op.audio.is_some() {
                        prompt.audio = op.audio;
                    }
                    if op.embedded_context.is_some() {
                        prompt.embedded_context = op.embedded_context;
                    }
                    b.prompt_capabilities = Some(prompt);
                }
            } else if o.prompt_capabilities.is_some() {
                b.prompt_capabilities = o.prompt_capabilities;
            }

            if let Some(mut mcp) = b.mcp_capabilities.clone() {
                if let Some(om) = o.mcp_capabilities {
                    if om.http.is_some() {
                        mcp.http = om.http;
                    }
                    if om.sse.is_some() {
                        mcp.sse = om.sse;
                    }
                    b.mcp_capabilities = Some(mcp);
                }
            } else if o.mcp_capabilities.is_some() {
                b.mcp_capabilities = o.mcp_capabilities;
            }
            Some(b)
        }
        (Some(b), None) => Some(b),
        (None, Some(o)) => Some(o),
    }
}

fn validate_permission_response(
    req: &AgentPermissionRequestData,
    resp: &agent_client_protocol_schema::RequestPermissionResponse,
) -> Result<(), String> {
    if let Some(decision) = &req.decision {
        match (decision.outcome.as_str(), &resp.outcome) {
            ("cancelled", RequestPermissionOutcome::Cancelled) => Ok(()),
            ("selected", RequestPermissionOutcome::Selected { option_id }) => {
                if let Some(expected) = &decision.option_id {
                    if option_id.0.as_ref() == expected {
                        Ok(())
                    } else {
                        Err(format!(
                            "expected option {} but client selected {}",
                            expected,
                            option_id.0.as_ref()
                        ))
                    }
                } else {
                    Ok(())
                }
            }
            (other, outcome) => Err(format!(
                "expected permission outcome {}, got {:?}",
                other, outcome
            )),
        }
    } else if let Some(granted) = req.granted {
        match (&resp.outcome, granted) {
            (RequestPermissionOutcome::Selected { .. }, true) => Ok(()),
            (RequestPermissionOutcome::Cancelled, false) => Ok(()),
            (outcome, expected) => Err(format!(
                "expected granted={} but outcome was {:?}",
                expected, outcome
            )),
        }
    } else {
        Ok(())
    }
}

fn normalize_yaml_string(value: &serde_yaml::Value) -> Option<String> {
    if let Some(s) = value.as_str() {
        return Some(s.to_string());
    }
    serde_yaml::to_string(value).ok().map(|mut s| {
        if s.ends_with('\n') {
            s.pop();
            if s.ends_with('\r') {
                s.pop();
            }
        }
        s
    })
}

fn validate_file_content(
    expected: &serde_yaml::Value,
    resp: &ReadTextFileResponse,
) -> Result<(), String> {
    let Some(expected_str) = normalize_yaml_string(expected) else {
        return Ok(());
    };
    if resp.content.trim_end_matches('\n') == expected_str.trim_end_matches('\n') {
        Ok(())
    } else {
        Err(format!(
            "expected file content {:?} but received {:?}",
            expected_str, resp.content
        ))
    }
}

impl ScenarioAgent {
    /// Replay side-effectful requests like permissions and file reads against the client connection.
    async fn replay_side_effects(&self) {
        let conn = { self.client_conn.lock().await.clone() };
        let Some(conn) = conn else {
            return;
        };

        // Permissions
        for (idx, req) in self.transcript.permission_requests.iter().enumerate() {
            let options = req
                .options
                .clone()
                .unwrap_or_default()
                .into_iter()
                .map(|opt| PermissionOption {
                    id: PermissionOptionId(Arc::from(opt.id)),
                    name: opt.label,
                    kind: map_permission_option_kind(&opt.kind),
                    meta: None,
                })
                .collect();

            let (tool_call_id, tool_call_fields) =
                tool_call_update_from_yaml(req.tool_call.as_ref(), idx);

            match conn
                .request_permission(RequestPermissionRequest {
                    session_id: self.transcript.session_id.clone(),
                    tool_call: ToolCallUpdate {
                        id: tool_call_id,
                        fields: tool_call_fields,
                        meta: req.tool_call.as_ref().map(yaml_to_json),
                    },
                    options,
                    meta: None,
                })
                .await
            {
                Ok(resp) => {
                    if let Err(err) = validate_permission_response(req, &resp) {
                        tracing::warn!(?err, "permission response validation failed");
                    }
                }
                Err(err) => tracing::warn!(?err, "permission request failed"),
            }
        }

        // File reads
        for reads in &self.transcript.file_reads {
            for file in &reads.files {
                match conn
                    .read_text_file(ReadTextFileRequest {
                        session_id: self.transcript.session_id.clone(),
                        path: std::path::PathBuf::from(file.path.clone()),
                        line: None,
                        limit: None,
                        meta: file.expected_content.as_ref().map(yaml_to_json),
                    })
                    .await
                {
                    Ok(resp) => {
                        if let Some(expected) = &file.expected_content {
                            if let Err(err) = validate_file_content(expected, &resp) {
                                tracing::warn!(path = %file.path, ?err, "file content mismatch");
                            }
                        }
                    }
                    Err(err) => tracing::warn!(path = %file.path, ?err, "read_text_file failed"),
                }
            }
        }

        // File writes (simulate write_text_file for agent edits)
        for edit in &self.transcript.file_writes {
            let content = format!(
                "// mock-agent wrote {} lines (+{}) (-{})\n",
                edit.path, edit.lines_added, edit.lines_removed
            );
            if let Err(err) = conn
                .write_text_file(WriteTextFileRequest {
                    session_id: self.transcript.session_id.clone(),
                    path: std::path::PathBuf::from(edit.path.clone()),
                    content: content.clone(),
                    meta: None,
                })
                .await
            {
                tracing::warn!(path = %edit.path, ?err, "write_text_file failed");
            }
        }

        // Tool runs (terminal follower command creation + streaming output)
        for (idx, run) in self.transcript.tool_runs.iter().enumerate() {
            if run.tool.tool_name != "runCmd" {
                continue;
            }
            let cmd = run.tool.args.get("cmd").and_then(|v| v.as_str()).unwrap_or_default();
            let follower = format!("ah show-sandbox-execution \"{}\" --id {}", cmd, run.id);
            let terminal_id = conn
                .create_terminal(CreateTerminalRequest {
                    command: follower.clone(),
                    args: Vec::new(),
                    cwd: run
                        .tool
                        .args
                        .get("cwd")
                        .and_then(|v| v.as_str())
                        .map(std::path::PathBuf::from)
                        .or_else(|| self.transcript.cwd_override.clone()),
                    env: map_env_vars(run.tool.args.get("env")),
                    output_byte_limit: None,
                    session_id: self.transcript.session_id.clone(),
                    meta: None,
                })
                .await
                .map(|resp| resp.terminal_id)
                .map_err(|err| {
                    tracing::warn!(tool_call_id = %run.id, idx, ?err, "create_terminal failed")
                })
                .ok();

            // Spawn a background poller to stream terminal output into tool updates.
            if let Some(term_id) = terminal_id {
                // Emit initial in-progress update with parsed command for observability.
                let _ = conn.notify(
                    CLIENT_METHOD_NAMES.session_update,
                    Some(agent_client_protocol_schema::AgentNotification::SessionNotification(
                        SessionNotification {
                            session_id: self.transcript.session_id.clone(),
                            update: SessionUpdate::ToolCallUpdate(
                                agent_client_protocol_schema::ToolCallUpdate {
                                    id: agent_client_protocol_schema::ToolCallId(Arc::from(run.id.clone())),
                                    fields: agent_client_protocol_schema::ToolCallUpdateFields {
                                        status: Some(agent_client_protocol_schema::ToolCallStatus::InProgress),
                                        raw_output: Some(serde_json::json!({ "follower": follower })),
                                        ..Default::default()
                                    },
                                    meta: None,
                                },
                            ),
                            meta: None,
                        },
                    )),
                );

                let conn = conn.clone();
                let session_id = self.transcript.session_id.clone();
                let tool_id = run.id.clone();
                tokio::task::spawn_local(async move {
                    stream_terminal_output(conn, session_id, tool_id, term_id).await;
                });
            }
        }
    }
}

/// Poll terminal/output and forward content as ToolCallUpdate content/status.
async fn stream_terminal_output(
    conn: Arc<AgentSideConnection>,
    session_id: SessionId,
    tool_id: String,
    terminal_id: TerminalId,
) {
    let mut attempts = 0usize;
    loop {
        attempts += 1;
        match conn
            .terminal_output(TerminalOutputRequest {
                session_id: session_id.clone(),
                terminal_id: terminal_id.clone(),
                meta: None,
            })
            .await
        {
            Ok(resp) => {
                if !resp.output.is_empty() {
                    let _ = conn.notify(
                        CLIENT_METHOD_NAMES.session_update,
                        Some(
                            agent_client_protocol_schema::AgentNotification::SessionNotification(
                                SessionNotification {
                                    session_id: session_id.clone(),
                                    update: SessionUpdate::ToolCallUpdate(
                                        agent_client_protocol_schema::ToolCallUpdate {
                                            id: agent_client_protocol_schema::ToolCallId(
                                                Arc::from(tool_id.clone()),
                                            ),
                                            fields:
                                                agent_client_protocol_schema::ToolCallUpdateFields {
                                                    content: Some(vec![ToolCallContent::Content {
                                                        content: ContentBlock::Text(TextContent {
                                                            annotations: None,
                                                            text: resp.output.clone(),
                                                            meta: None,
                                                        }),
                                                    }]),
                                                    ..Default::default()
                                                },
                                            meta: None,
                                        },
                                    ),
                                    meta: None,
                                },
                            ),
                        ),
                    );
                }

                if let Some(status) = resp.exit_status {
                    let stop_status = if status.exit_code.unwrap_or(0) == 0 {
                        ToolCallStatus::Completed
                    } else {
                        ToolCallStatus::Failed
                    };
                    let _ = conn.notify(
                        CLIENT_METHOD_NAMES.session_update,
                        Some(
                            agent_client_protocol_schema::AgentNotification::SessionNotification(
                                SessionNotification {
                                    session_id: session_id.clone(),
                                    update: SessionUpdate::ToolCallUpdate(
                                        agent_client_protocol_schema::ToolCallUpdate {
                                            id: agent_client_protocol_schema::ToolCallId(
                                                Arc::from(tool_id.clone()),
                                            ),
                                            fields:
                                                agent_client_protocol_schema::ToolCallUpdateFields {
                                                    status: Some(stop_status),
                                                    ..Default::default()
                                                },
                                            meta: None,
                                        },
                                    ),
                                    meta: None,
                                },
                            ),
                        ),
                    );
                    break;
                }
            }
            Err(err) => {
                tracing::warn!(?err, %tool_id, "terminal_output failed");
                break;
            }
        }
        if attempts > 20 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let _ = conn
        .wait_for_terminal_exit(WaitForTerminalExitRequest {
            session_id,
            terminal_id,
            meta: None,
        })
        .await;
}
