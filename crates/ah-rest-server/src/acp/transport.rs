// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Transport layer for ACP (Milestone 1).
//!
//! For now we expose a minimal WebSocket echo loop that:
//! - Authenticates using the existing REST auth config (API key or JWT) passed
//!   via `?api_key=` query param or `Authorization` header.
//! - Applies a simple connection-limit guard and idle timeout.
//! - Echoes JSON-RPC requests by returning `{"id": <id>, "result": <params>}`.

use crate::{
    acp::translator::{AcpCapabilities, JsonRpcTranslator},
    auth::AuthConfig,
    config::AcpConfig,
    error::{ServerError, ServerResult},
    services::SessionService,
    state::AppState,
};
use axum::{
    Json, Router,
    extract::{
        Query, State,
        ws::{Message as WsMessage, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::get,
};
use futures::{SinkExt, StreamExt};
use futures::stream::SplitSink;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::{HashMap, HashSet}, sync::Arc, time::Duration};
use tokio::{sync::{Mutex, Semaphore}, time::{Instant, interval, sleep}};
use tokio::sync::broadcast::Receiver;
use tokio_tungstenite;
use tokio_tungstenite::tungstenite::Message as ClientMessage;
use url::Url;

use ah_domain_types::{AgentChoice, AgentSoftware, AgentSoftwareBuild};
use ah_rest_api_contract::{
    CreateTaskRequest, FilterQuery, RepoConfig, RepoMode, RuntimeConfig, RuntimeType, Session,
    SessionEvent, SessionLogLevel, SessionStatus, SessionToolStatus,
};

#[derive(Clone)]
pub struct AcpTransportState {
    pub auth: AuthConfig,
    pub permits: Arc<Semaphore>,
    pub idle_timeout: Duration,
    pub config: AcpConfig,
    pub app_state: AppState,
}

/// Query params accepted by the WebSocket endpoint.
#[derive(Debug, Deserialize)]
pub struct AcpQuery {
    #[serde(rename = "api_key")]
    pub api_key: Option<String>,
}

pub fn router(state: AcpTransportState) -> Router {
    Router::new()
        .route("/acp/v1/connect", get(acp_connect))
        .with_state(Arc::new(state))
}

async fn acp_connect(
    State(state): State<Arc<AcpTransportState>>,
    Query(query): Query<AcpQuery>,
    ws: WebSocketUpgrade,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Connection limit
    let permit = match state.permits.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(crate::error::ServerError::RateLimited.to_problem()),
            )
                .into_response();
        }
    };

    // Authenticate
    if let Err(err) = authenticate(&state.auth, &query, &headers) {
        drop(permit);
        let status = StatusCode::UNAUTHORIZED;
        return (status, Json(err.to_problem())).into_response();
    }

    ws.on_upgrade(move |socket| handle_socket(socket, state, permit))
}

fn authenticate(
    auth: &AuthConfig,
    query: &AcpQuery,
    headers: &HeaderMap,
) -> Result<(), ServerError> {
    // Prefer Authorization header
    if let Some(value) = headers.get(axum::http::header::AUTHORIZATION) {
        if let Ok(v) = value.to_str() {
            if let Some(stripped) = v.strip_prefix("ApiKey ") {
                return auth.validate_api_key(stripped);
            }
            if let Some(stripped) = v.strip_prefix("Bearer ") {
                return auth.validate_jwt(stripped).map(|_| ());
            }
        }
    }

    // Fallback to query param for convenience in tests
    if let Some(key) = &query.api_key {
        return auth.validate_api_key(key);
    }

    if auth.requires_auth() {
        Err(ServerError::Auth(
            "Missing or invalid authorization header".to_string(),
        ))
    } else {
        Ok(())
    }
}

async fn handle_socket(
    socket: WebSocket,
    state: Arc<AcpTransportState>,
    _permit: tokio::sync::OwnedSemaphorePermit,
) {
    let (sender, mut receiver) = socket.split();
    let sender = Arc::new(Mutex::new(sender));
    let mut context = AcpSessionContext::default();
    let idle_timer = sleep(state.idle_timeout);
    tokio::pin!(idle_timer);
    let mut tick = interval(Duration::from_millis(100));

    loop {
        tokio::select! {
            _ = &mut idle_timer => {
                break;
            }
            _ = tick.tick() => {
                let flushed = flush_session_events(&mut context, &sender).await;
                if flushed {
                    idle_timer.as_mut().reset(Instant::now() + state.idle_timeout);
                }
            }
            maybe_msg = receiver.next() => {
                match maybe_msg {
                    Some(Ok(msg)) => {
                        match msg {
                            WsMessage::Text(text) => {
                                match serde_json::from_str::<Value>(&text) {
                                    Ok(value) => {
                                        let response = handle_rpc(&state, &mut context, value).await;
                                        if let Some(payload) = response {
                                            if send_json(&sender, payload).await.is_err() {
                                                break;
                                            }
                                        }
                                        idle_timer.as_mut().reset(Instant::now() + state.idle_timeout);
                                    }
                                    Err(_) => {
                                        let _ = send_json(&sender, json_error(Value::Null, -32700, "invalid_json")).await;
                                    }
                                }
                            }
                            WsMessage::Binary(bin) => {
                                let mut guard = sender.lock().await;
                                if guard.send(WsMessage::Binary(bin)).await.is_err() {
                                    break;
                                }
                            }
                            WsMessage::Close(_) => break,
                            _ => {}
                        }
                    }
                    Some(Err(_)) | None => break,
                }
            }
        }
    }
}

#[derive(Default)]
struct AcpSessionContext {
    negotiated_caps: Option<AcpCapabilities>,
    sessions: HashSet<String>,
    receivers: HashMap<String, Receiver<SessionEvent>>,
}

fn rpc_response(id: Value, result: Value) -> Value {
    serde_json::json!({
        "id": id,
        "result": result
    })
}

fn json_error(id: Value, code: i64, message: &str) -> Value {
    serde_json::json!({
        "id": id,
        "error": { "code": code, "message": message }
    })
}

async fn handle_rpc(
    state: &AcpTransportState,
    ctx: &mut AcpSessionContext,
    request: Value,
) -> Option<Value> {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let method = request.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let params = request.get("params").cloned().unwrap_or(Value::Null);

    match method {
        "initialize" => {
            let caps = JsonRpcTranslator::negotiate_caps(&state.config);
            ctx.negotiated_caps = Some(caps.clone());
            let result = JsonRpcTranslator::initialize_response(&caps);
            Some(rpc_response(id, result))
        }
        "session/new" | "session/list" | "session/load" | "session/prompt" | "session/cancel"
            if ctx.negotiated_caps.is_none() =>
        {
            Some(json_error(
                id,
                -32001,
                "initialize must be called before session operations",
            ))
        }
        "session/new" => match handle_session_new(state, ctx, params).await {
            Ok(result) => Some(rpc_response(id, result)),
            Err(err) => Some(json_error(id, -32000, &err.to_string())),
        },
        "session/list" => match handle_session_list(state, params).await {
            Ok(result) => Some(rpc_response(id, result)),
            Err(err) => Some(json_error(id, -32000, &err.to_string())),
        },
        "session/load" => match handle_session_load(state, ctx, params).await {
            Ok(result) => Some(rpc_response(id, result)),
            Err(err) => Some(json_error(id, -32000, &err.to_string())),
        },
        "session/prompt" => match handle_session_prompt(state, ctx, params).await {
            Ok(result) => Some(rpc_response(id, result)),
            Err(err) => Some(json_error(id, -32000, &err.to_string())),
        },
        "session/cancel" => match handle_session_cancel(state, ctx, params).await {
            Ok(result) => Some(rpc_response(id, result)),
            Err(err) => Some(json_error(id, -32000, &err.to_string())),
        },
        _ => {
            let result = request.get("params").cloned().unwrap_or(Value::Null);
            Some(rpc_response(id, result))
        }
    }
}

async fn handle_session_new(
    state: &AcpTransportState,
    ctx: &mut AcpSessionContext,
    params: Value,
) -> ServerResult<Value> {
    let request = translate_create_request(params)?;
    let service = SessionService::new(Arc::clone(&state.app_state.session_store));
    let response = service.create_session(&request).await?;
    let session_id = response
        .session_ids
        .first()
        .cloned()
        .ok_or_else(|| ServerError::Internal("session creation returned no ids".into()))?;

    if let Some(session) = state.app_state.session_store.get_session(&session_id).await? {
        subscribe_session(ctx, &state.app_state, &session_id);
        ctx.sessions.insert(session_id.clone());
        let _ = state
            .app_state
            .session_store
            .add_session_event(
                &session_id,
                SessionEvent::status(
                    session.session.status.clone(),
                    chrono::Utc::now().timestamp_millis() as u64,
                ),
            )
            .await;
        Ok(json!({
            "sessionId": session_id,
            "status": session.session.status.to_string(),
            "workspace": session.session.workspace.mount_path,
            "agent": session.session.agent.display_name.clone().unwrap_or_else(|| session.session.agent.model.clone()),
        }))
    } else {
        Err(ServerError::SessionNotFound(session_id))
    }
}

async fn handle_session_list(
    state: &AcpTransportState,
    params: Value,
) -> ServerResult<Value> {
    let filters = FilterQuery {
        status: params
            .get("status")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        agent: params
            .get("agent")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        project_id: params
            .get("projectId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        tenant_id: params
            .get("tenantId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    };

    let sessions = state.app_state.session_store.list_sessions(&filters).await?;
    let items: Vec<Value> = sessions.iter().map(session_to_json).collect();
    Ok(json!({ "items": items, "total": items.len() }))
}

async fn handle_session_load(
    state: &AcpTransportState,
    ctx: &mut AcpSessionContext,
    params: Value,
) -> ServerResult<Value> {
    let session_id = params
        .get("sessionId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ServerError::BadRequest("sessionId is required".into()))?;

    if let Some(session) = state
        .app_state
        .session_store
        .get_session(session_id)
        .await?
    {
        subscribe_session(ctx, &state.app_state, session_id);
        ctx.sessions.insert(session_id.to_string());
        Ok(json!({
            "session": session_to_json(&session.session),
        }))
    } else {
        Err(ServerError::SessionNotFound(session_id.to_string()))
    }
}

async fn handle_session_prompt(
    state: &AcpTransportState,
    ctx: &mut AcpSessionContext,
    params: Value,
) -> ServerResult<Value> {
    let session_id = params
        .get("sessionId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ServerError::BadRequest("sessionId is required".into()))?;
    let message = params
        .get("message")
        .or_else(|| params.get("prompt"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| ServerError::BadRequest("message is required".into()))?;

    if let Some(mut session) = state
        .app_state
        .session_store
        .get_session(session_id)
        .await?
    {
        subscribe_session(ctx, &state.app_state, session_id);
        ctx.sessions.insert(session_id.to_string());
        if matches!(session.session.status, SessionStatus::Queued | SessionStatus::Provisioning) {
            session.session.status = SessionStatus::Running;
            state
                .app_state
                .session_store
                .update_session(session_id, &session)
                .await?;
            state
                .app_state
                .session_store
                .add_session_event(
                    session_id,
                    SessionEvent::status(
                        SessionStatus::Running,
                        chrono::Utc::now().timestamp_millis() as u64,
                    ),
                )
                .await?;
        }
    } else {
        return Err(ServerError::SessionNotFound(session_id.to_string()));
    }

    state
        .app_state
        .session_store
        .add_session_event(
            session_id,
            SessionEvent::log(
                SessionLogLevel::Info,
                format!("user: {}", message),
                None,
                chrono::Utc::now().timestamp_millis() as u64,
            ),
        )
        .await?;

    Ok(json!({
        "sessionId": session_id,
        "accepted": true
    }))
}

async fn handle_session_cancel(
    state: &AcpTransportState,
    ctx: &mut AcpSessionContext,
    params: Value,
) -> ServerResult<Value> {
    let session_id = params
        .get("sessionId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ServerError::BadRequest("sessionId is required".into()))?;

    let Some(mut session) = state
        .app_state
        .session_store
        .get_session(session_id)
        .await?
    else {
        return Err(ServerError::SessionNotFound(session_id.to_string()));
    };

    subscribe_session(ctx, &state.app_state, session_id);
    ctx.sessions.insert(session_id.to_string());

    session.session.status = SessionStatus::Cancelled;
    state
        .app_state
        .session_store
        .update_session(session_id, &session)
        .await?;
    state
        .app_state
        .session_store
        .add_session_event(
            session_id,
            SessionEvent::status(
                SessionStatus::Cancelled,
                chrono::Utc::now().timestamp_millis() as u64,
            ),
        )
        .await?;

    Ok(json!({
        "sessionId": session_id,
        "cancelled": true
    }))
}

fn subscribe_session(ctx: &mut AcpSessionContext, app_state: &AppState, session_id: &str) {
    if ctx.receivers.contains_key(session_id) {
        return;
    }
    if let Some(receiver) = app_state.session_store.subscribe_session_events(session_id) {
        ctx.receivers.insert(session_id.to_string(), receiver);
    }
}

fn session_to_json(session: &Session) -> Value {
    json!({
        "id": session.id,
        "status": session.status.to_string(),
        "prompt": session.task.prompt,
        "workspace": session.workspace.mount_path,
        "agent": session.agent.display_name.clone().unwrap_or_else(|| session.agent.model.clone()),
        "links": {
            "events": session.links.events,
            "logs": session.links.logs
        }
    })
}

async fn flush_session_events(
    ctx: &mut AcpSessionContext,
    sender: &Arc<Mutex<SplitSink<WebSocket, WsMessage>>>,
) -> bool {
    let mut sent = false;
    let mut dead_sessions = Vec::new();

    for (session_id, receiver) in ctx.receivers.iter_mut() {
        loop {
            match receiver.try_recv() {
                Ok(event) => {
                    let update = json!({
                        "method": "session/update",
                        "params": {
                            "sessionId": session_id,
                            "event": event_to_json(session_id, &event),
                        }
                    });
                    if send_json(sender, update).await.is_err() {
                        dead_sessions.push(session_id.clone());
                        break;
                    }
                    sent = true;
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
                Err(tokio::sync::broadcast::error::TryRecvError::Closed) => {
                    dead_sessions.push(session_id.clone());
                    break;
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => break,
            }
        }
    }

    for session_id in dead_sessions {
        ctx.receivers.remove(&session_id);
    }

    sent
}

fn event_to_json(session_id: &str, event: &SessionEvent) -> Value {
    match event {
        SessionEvent::Status(status) => json!({
            "type": "status",
            "sessionId": session_id,
            "status": status.status.to_string(),
            "timestamp": status.timestamp,
        }),
        SessionEvent::Log(log) => json!({
            "type": "log",
            "sessionId": session_id,
            "level": format!("{:?}", log.level).to_lowercase(),
            "message": String::from_utf8_lossy(&log.message),
            "timestamp": log.timestamp,
        }),
        SessionEvent::Error(err) => json!({
            "type": "error",
            "sessionId": session_id,
            "message": String::from_utf8_lossy(&err.message),
            "timestamp": err.timestamp,
        }),
        SessionEvent::Thought(thought) => json!({
            "type": "thought",
            "sessionId": session_id,
            "text": String::from_utf8_lossy(&thought.thought),
            "timestamp": thought.timestamp,
        }),
        SessionEvent::ToolUse(tool) => json!({
            "type": "tool_use",
            "sessionId": session_id,
            "toolName": String::from_utf8_lossy(&tool.tool_name),
            "args": String::from_utf8_lossy(&tool.tool_args),
            "executionId": String::from_utf8_lossy(&tool.tool_execution_id),
            "status": tool_status_str(&tool.status),
            "timestamp": tool.timestamp,
        }),
        SessionEvent::ToolResult(result) => json!({
            "type": "tool_result",
            "sessionId": session_id,
            "toolName": String::from_utf8_lossy(&result.tool_name),
            "output": String::from_utf8_lossy(&result.tool_output),
            "executionId": String::from_utf8_lossy(&result.tool_execution_id),
            "status": tool_status_str(&result.status),
            "timestamp": result.timestamp,
        }),
        SessionEvent::FileEdit(edit) => json!({
            "type": "file_edit",
            "sessionId": session_id,
            "path": String::from_utf8_lossy(&edit.file_path),
            "added": edit.lines_added,
            "removed": edit.lines_removed,
            "timestamp": edit.timestamp,
        }),
    }
}

fn tool_status_str(status: &SessionToolStatus) -> &'static str {
    match status {
        SessionToolStatus::Started => "started",
        SessionToolStatus::Completed => "completed",
        SessionToolStatus::Failed => "failed",
    }
}

async fn send_json(
    sender: &Arc<Mutex<SplitSink<WebSocket, WsMessage>>>,
    payload: Value,
) -> Result<(), ()> {
    let text = serde_json::to_string(&payload).map_err(|_| ())?;
    let mut guard = sender.lock().await;
    guard
        .send(WsMessage::Text(text))
        .await
        .map_err(|_| ())
}

fn translate_create_request(params: Value) -> ServerResult<CreateTaskRequest> {
    let prompt = params
        .get("prompt")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    if prompt.trim().is_empty() {
        return Err(ServerError::BadRequest(
            "prompt is required for session/new".into(),
        ));
    }

    let repo_url = params
        .get("repoUrl")
        .and_then(|v| v.as_str())
        .and_then(|s| Url::parse(s).ok());
    let repo = RepoConfig {
        mode: if repo_url.is_some() { RepoMode::Git } else { RepoMode::None },
        url: repo_url,
        branch: params.get("branch").and_then(|v| v.as_str()).map(|s| s.to_string()),
        commit: None,
    };

    let agent_model = params
        .get("agent")
        .and_then(|v| v.as_str())
        .unwrap_or("sonnet")
        .to_string();

    let agent = AgentChoice {
        agent: AgentSoftwareBuild {
            software: AgentSoftware::Claude,
            version: "latest".into(),
        },
        model: agent_model.clone(),
        count: 1,
        settings: std::collections::HashMap::new(),
        display_name: Some(agent_model),
    };

    let runtime = RuntimeConfig {
        runtime_type: RuntimeType::Local,
        devcontainer_path: None,
        resources: None,
    };

    let labels = params
        .get("labels")
        .and_then(|v| v.as_object())
        .map(|map| {
            map.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();

    Ok(CreateTaskRequest {
        tenant_id: params
            .get("tenantId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        project_id: params
            .get("projectId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        prompt,
        repo,
        runtime,
        workspace: None,
        agents: vec![agent],
        delivery: None,
        labels,
        webhooks: Vec::new(),
    })
}

/// Client helper used by tests
pub async fn ws_echo(url: &str, message: Value) -> ServerResult<Value> {
    let (mut socket, _response) =
        tokio_tungstenite::connect_async(url).await.map_err(ServerError::from)?;
    socket
        .send(ClientMessage::Text(message.to_string()))
        .await
        .map_err(|e| ServerError::Internal(format!("send failed: {e}")))?;
    if let Some(msg) = socket.next().await {
        match msg {
            Ok(ClientMessage::Text(text)) => {
                let value: Value = serde_json::from_str(&text)
                    .map_err(|e| ServerError::Internal(format!("invalid json: {e}")))?;
                Ok(value)
            }
            other => Err(ServerError::Internal(format!(
                "unexpected frame: {other:?}"
            ))),
        }
    } else {
        Err(ServerError::Internal("no response".into()))
    }
}
