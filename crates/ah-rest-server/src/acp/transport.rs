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
    auth::{AuthConfig, Claims},
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
use futures::stream::SplitSink;
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};
use tokio::sync::broadcast::Receiver;
use tokio::{
    sync::{Mutex, Semaphore},
    time::{Instant, interval, sleep},
};
use tokio_tungstenite;
use tokio_tungstenite::tungstenite::Message as ClientMessage;
use url::Url;

use crate::acp::recorder::follower_command;
use ah_core::task_manager_wire::TaskManagerMessage;
use ah_domain_types::{AgentChoice, AgentSoftware, AgentSoftwareBuild};
use ah_rest_api_contract::{
    CreateTaskRequest, FilterQuery, RepoConfig, RepoMode, RuntimeConfig, RuntimeType, Session,
    SessionEvent, SessionLogLevel, SessionStatus, SessionToolStatus,
};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;

/// Conservative context window guardrail for inbound ACP prompts.
/// Matches the per-message cap to keep total user-provided text bounded
/// until the recorder/LLM bridge is wired (Milestone 4 follow-up).
const MAX_CONTEXT_CHARS: usize = 16_000;

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
    #[serde(rename = "token")]
    pub bearer_token: Option<String>,
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
    let claims = match authenticate(&state.auth, &query, &headers) {
        Ok(claims) => claims,
        Err(err) => {
            drop(permit);
            let status = StatusCode::UNAUTHORIZED;
            return (status, Json(err.to_problem())).into_response();
        }
    };

    ws.on_upgrade(move |socket| handle_socket(socket, state, permit, claims))
}

fn authenticate(
    auth: &AuthConfig,
    query: &AcpQuery,
    headers: &HeaderMap,
) -> Result<Option<Claims>, ServerError> {
    // Query bearer token takes precedence for test harness convenience
    if let Some(token) = &query.bearer_token {
        return auth.validate_jwt(token).map(Some);
    }

    // Prefer Authorization header
    if let Some(value) = headers.get(axum::http::header::AUTHORIZATION) {
        if let Ok(v) = value.to_str() {
            if let Some(stripped) = v.strip_prefix("ApiKey ") {
                return auth.validate_api_key(stripped).map(|_| None);
            }
            if let Some(stripped) = v.strip_prefix("Bearer ") {
                return auth.validate_jwt(stripped).map(Some);
            }
        }
    }

    // Fallback to query param for convenience in tests
    if let Some(key) = &query.api_key {
        return auth.validate_api_key(key).map(|_| None);
    }

    if auth.requires_auth() {
        Err(ServerError::Auth(
            "Missing or invalid authorization header".to_string(),
        ))
    } else {
        Ok(None)
    }
}

async fn handle_socket(
    socket: WebSocket,
    state: Arc<AcpTransportState>,
    _permit: tokio::sync::OwnedSemaphorePermit,
    claims: Option<Claims>,
) {
    let (sender, mut receiver) = socket.split();
    let sender = Arc::new(Mutex::new(sender));
    let mut context = AcpSessionContext {
        auth_claims: claims,
        ..Default::default()
    };
    let idle_timer = sleep(state.idle_timeout);
    tokio::pin!(idle_timer);
    let mut tick = interval(Duration::from_millis(100));

    loop {
        tokio::select! {
            _ = &mut idle_timer => {
                break;
            }
            _ = tick.tick() => {
                let flushed_events = flush_session_events(&mut context, &sender).await;
                let flushed_pty = flush_pty_events(&mut context, &sender).await;
                if flushed_events || flushed_pty {
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
                                        let response = handle_rpc(&state, &mut context, &sender, value).await;
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
    pty_receivers: HashMap<String, tokio::sync::broadcast::Receiver<TaskManagerMessage>>,
    auth_claims: Option<Claims>,
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
    sender: &Arc<Mutex<SplitSink<WebSocket, WsMessage>>>,
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
        | "session/pause" | "session/resume"
            if ctx.negotiated_caps.is_none() =>
        {
            Some(json_error(
                id,
                -32001,
                "initialize must be called before session operations",
            ))
        }
        "session/new" => match handle_session_new(state, ctx, sender, params).await {
            Ok(result) => Some(rpc_response(id, result)),
            Err(err) => Some(json_error(id, -32000, &err.to_string())),
        },
        "session/list" => match handle_session_list(state, params, ctx).await {
            Ok(result) => Some(rpc_response(id, result)),
            Err(err) => Some(json_error(id, -32000, &err.to_string())),
        },
        "session/load" => match handle_session_load(state, ctx, sender, params).await {
            Ok(result) => Some(rpc_response(id, result)),
            Err(err) => Some(json_error(id, -32000, &err.to_string())),
        },
        "session/prompt" => match handle_session_prompt(state, ctx, sender, params).await {
            Ok(result) => Some(rpc_response(id, result)),
            Err(err) => Some(json_error(id, -32000, &err.to_string())),
        },
        "session/cancel" => match handle_session_cancel(state, ctx, sender, params).await {
            Ok(result) => Some(rpc_response(id, result)),
            Err(err) => Some(json_error(id, -32000, &err.to_string())),
        },
        "session/pause" => match handle_session_pause(state, ctx, sender, params).await {
            Ok(result) => Some(rpc_response(id, result)),
            Err(err) => Some(json_error(id, -32000, &err.to_string())),
        },
        "session/resume" => match handle_session_resume(state, ctx, sender, params).await {
            Ok(result) => Some(rpc_response(id, result)),
            Err(err) => Some(json_error(id, -32000, &err.to_string())),
        },
        "_ah/terminal/write" => match handle_terminal_write(state, ctx, sender, params).await {
            Ok(result) => Some(rpc_response(id, result)),
            Err(err) => Some(json_error(id, -32000, &err.to_string())),
        },
        "_ah/terminal/follow" => match handle_terminal_follow(state, sender, params).await {
            Ok(result) => Some(rpc_response(id, result)),
            Err(err) => Some(json_error(id, -32000, &err.to_string())),
        },
        "_ah/terminal/detach" => match handle_terminal_detach(sender, params).await {
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
    sender: &Arc<Mutex<SplitSink<WebSocket, WsMessage>>>,
    params: Value,
) -> ServerResult<Value> {
    let prompt = params.get("prompt").and_then(|v| v.as_str()).unwrap_or_default().to_string();
    let prompt_len = prompt.chars().count();
    if prompt_len > MAX_CONTEXT_CHARS {
        return Ok(json!({
            "accepted": false,
            "stopReason": "context_limit",
            "limitChars": MAX_CONTEXT_CHARS,
            "usedChars": prompt_len,
            "overLimitBy": prompt_len.saturating_sub(MAX_CONTEXT_CHARS),
            "remainingChars": 0
        }));
    }

    let mut request = translate_create_request(params)?;
    if request.tenant_id.is_none() {
        request.tenant_id = ctx.auth_claims.as_ref().and_then(|c| c.tenant_id.clone());
    }
    if request.project_id.is_none() {
        request.project_id = ctx.auth_claims.as_ref().and_then(|c| c.project_id.clone());
    }
    let service = SessionService::new(Arc::clone(&state.app_state.session_store));
    let response = service.create_session(&request).await?;
    let session_id = response
        .session_ids
        .first()
        .cloned()
        .ok_or_else(|| ServerError::Internal("session creation returned no ids".into()))?;

    if let Some(session) = state.app_state.session_store.get_session(&session_id).await? {
        subscribe_session(ctx, &state.app_state, &session_id, sender).await;
        let _ = seed_history(&state.app_state, &session_id, sender).await;
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
            "tenantId": session.session.tenant_id,
            "projectId": session.session.project_id,
            "workspace": session.session.workspace.mount_path,
            "agent": session.session.agent.display_name.clone().unwrap_or_else(|| session.session.agent.model.clone()),
        }))
    } else {
        Err(ServerError::SessionNotFound(session_id))
    }
}

async fn handle_terminal_write(
    state: &AcpTransportState,
    ctx: &mut AcpSessionContext,
    sender: &Arc<Mutex<SplitSink<WebSocket, WsMessage>>>,
    params: Value,
) -> ServerResult<Value> {
    let session_id = params
        .get("sessionId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ServerError::BadRequest("sessionId is required".into()))?;
    let data_b64 = params
        .get("data")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ServerError::BadRequest("data is required".into()))?;

    let bytes = B64
        .decode(data_b64)
        .map_err(|_| ServerError::BadRequest("data must be base64".into()))?;

    if let Some(controller) = &state.app_state.task_controller {
        let _ = controller.inject_bytes(session_id, &bytes).await;
    }

    subscribe_session(ctx, &state.app_state, session_id, sender).await;
    Ok(json!({"sessionId": session_id, "accepted": true}))
}

async fn handle_terminal_follow(
    state: &AcpTransportState,
    sender: &Arc<Mutex<SplitSink<WebSocket, WsMessage>>>,
    params: Value,
) -> ServerResult<Value> {
    let session_id = params
        .get("sessionId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ServerError::BadRequest("sessionId is required".into()))?;
    let execution_id = params
        .get("executionId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ServerError::BadRequest("executionId is required".into()))?;
    let command = params
        .get("command")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ServerError::BadRequest("command is required".into()))?;

    let cmd = follower_command(execution_id, session_id, command);

    let update = terminal_follow_update(session_id, execution_id, &cmd);
    let _ = send_json(sender, update).await;

    // Attempt to stream PTY backlog + live updates when available.
    if let Some(controller) = &state.app_state.task_controller {
        match controller.subscribe_pty(session_id).await {
            Ok((backlog, mut rx)) => {
                for msg in backlog {
                    if matches!(
                        msg,
                        TaskManagerMessage::PtyData(_) | TaskManagerMessage::PtyResize(_)
                    ) {
                        let _ = send_json(sender, pty_to_update(session_id, &msg)).await;
                    }
                }
                let sender_clone = Arc::clone(sender);
                let session = session_id.to_string();
                tokio::spawn(async move {
                    while let Ok(msg) = rx.recv().await {
                        if matches!(
                            msg,
                            TaskManagerMessage::PtyData(_) | TaskManagerMessage::PtyResize(_)
                        ) {
                            let _ = send_json(&sender_clone, pty_to_update(&session, &msg)).await;
                        }
                    }
                });
            }
            Err(err) => {
                tracing::debug!("PTY subscribe unavailable for {}: {}", session_id, err);
            }
        }
    }

    Ok(json!({ "sessionId": session_id, "executionId": execution_id, "command": cmd }))
}

async fn handle_terminal_detach(
    sender: &Arc<Mutex<SplitSink<WebSocket, WsMessage>>>,
    params: Value,
) -> ServerResult<Value> {
    let session_id = params
        .get("sessionId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ServerError::BadRequest("sessionId is required".into()))?;
    let execution_id = params
        .get("executionId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ServerError::BadRequest("executionId is required".into()))?;

    let update = terminal_detach_update(session_id, execution_id);
    let _ = send_json(sender, update).await;

    Ok(json!({ "sessionId": session_id, "executionId": execution_id, "detached": true }))
}

async fn handle_session_list(
    state: &AcpTransportState,
    params: Value,
    ctx: &AcpSessionContext,
) -> ServerResult<Value> {
    let filters = FilterQuery {
        status: params.get("status").and_then(|v| v.as_str()).map(|s| s.to_string()),
        agent: params.get("agent").and_then(|v| v.as_str()).map(|s| s.to_string()),
        project_id: params.get("projectId").and_then(|v| v.as_str()).map(|s| s.to_string()),
        tenant_id: params
            .get("tenantId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| ctx.auth_claims.as_ref().and_then(|c| c.tenant_id.clone())),
    };

    let sessions = state.app_state.session_store.list_sessions(&filters).await?;
    let items: Vec<Value> = sessions.iter().map(session_to_json).collect();
    Ok(json!({ "items": items, "total": items.len() }))
}

async fn handle_session_load(
    state: &AcpTransportState,
    ctx: &mut AcpSessionContext,
    sender: &Arc<Mutex<SplitSink<WebSocket, WsMessage>>>,
    params: Value,
) -> ServerResult<Value> {
    let session_id = params
        .get("sessionId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ServerError::BadRequest("sessionId is required".into()))?;

    if let Some(session) = state.app_state.session_store.get_session(session_id).await? {
        subscribe_session(ctx, &state.app_state, session_id, sender).await;
        let _ = seed_history(&state.app_state, session_id, sender).await;
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
    sender: &Arc<Mutex<SplitSink<WebSocket, WsMessage>>>,
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
    let message_chars = message.chars().count();
    if message_chars > MAX_CONTEXT_CHARS {
        return Ok(json!({
            "sessionId": session_id,
            "accepted": false,
            "stopReason": "context_limit",
            "limitChars": MAX_CONTEXT_CHARS,
            "usedChars": message_chars,
            "currentChars": 0,
            "overLimitBy": message_chars.saturating_sub(MAX_CONTEXT_CHARS),
            "remainingChars": 0
        }));
    }

    let Some(mut session) = state.app_state.session_store.get_session(session_id).await? else {
        return Err(ServerError::SessionNotFound(session_id.to_string()));
    };

    let current_context =
        current_context_chars(&state.app_state, session_id, &session.session).await?;
    if current_context + message_chars > MAX_CONTEXT_CHARS {
        let over_by = current_context + message_chars - MAX_CONTEXT_CHARS;
        let remaining = MAX_CONTEXT_CHARS.saturating_sub(current_context);
        return Ok(json!({
            "sessionId": session_id,
            "accepted": false,
            "stopReason": "context_limit",
            "limitChars": MAX_CONTEXT_CHARS,
            "usedChars": current_context + message_chars,
            "currentChars": current_context,
            "rejectedChars": message_chars,
            "overLimitBy": over_by,
            "remainingChars": remaining
        }));
    }

    subscribe_session(ctx, &state.app_state, session_id, sender).await;
    let _ = seed_history(&state.app_state, session_id, sender).await;
    ctx.sessions.insert(session_id.to_string());
    if matches!(
        session.session.status,
        SessionStatus::Queued | SessionStatus::Provisioning
    ) {
        session.session.status = SessionStatus::Running;
        state.app_state.session_store.update_session(session_id, &session).await?;
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

    // Best-effort agent delivery through task controller when available
    if let Some(controller) = &state.app_state.task_controller {
        let _ = controller.inject_message(session_id, message).await;
    }

    Ok(json!({
        "sessionId": session_id,
        "accepted": true
    }))
}

async fn handle_session_cancel(
    state: &AcpTransportState,
    ctx: &mut AcpSessionContext,
    sender: &Arc<Mutex<SplitSink<WebSocket, WsMessage>>>,
    params: Value,
) -> ServerResult<Value> {
    let session_id = params
        .get("sessionId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ServerError::BadRequest("sessionId is required".into()))?;

    let Some(mut session) = state.app_state.session_store.get_session(session_id).await? else {
        return Err(ServerError::SessionNotFound(session_id.to_string()));
    };

    subscribe_session(ctx, &state.app_state, session_id, sender).await;
    let _ = seed_history(&state.app_state, session_id, sender).await;
    ctx.sessions.insert(session_id.to_string());

    // best-effort task stop via TaskController if available
    if let Some(controller) = &state.app_state.task_controller {
        let _ = controller.stop_task(session_id).await;
    }

    session.session.status = SessionStatus::Cancelled;
    state.app_state.session_store.update_session(session_id, &session).await?;
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

async fn handle_session_pause(
    state: &AcpTransportState,
    ctx: &mut AcpSessionContext,
    sender: &Arc<Mutex<SplitSink<WebSocket, WsMessage>>>,
    params: Value,
) -> ServerResult<Value> {
    let session_id = params
        .get("sessionId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ServerError::BadRequest("sessionId is required".into()))?;

    let Some(mut session) = state.app_state.session_store.get_session(session_id).await? else {
        return Err(ServerError::SessionNotFound(session_id.to_string()));
    };

    subscribe_session(ctx, &state.app_state, session_id, sender).await;
    let _ = seed_history(&state.app_state, session_id, sender).await;
    ctx.sessions.insert(session_id.to_string());

    session.session.status = SessionStatus::Paused;
    state.app_state.session_store.update_session(session_id, &session).await?;
    state
        .app_state
        .session_store
        .add_session_event(
            session_id,
            SessionEvent::status(
                SessionStatus::Paused,
                chrono::Utc::now().timestamp_millis() as u64,
            ),
        )
        .await?;

    if let Some(controller) = &state.app_state.task_controller {
        let _ = controller.pause_task(session_id).await;
    }

    Ok(json!({
        "sessionId": session_id,
        "status": "paused"
    }))
}

async fn handle_session_resume(
    state: &AcpTransportState,
    ctx: &mut AcpSessionContext,
    sender: &Arc<Mutex<SplitSink<WebSocket, WsMessage>>>,
    params: Value,
) -> ServerResult<Value> {
    let session_id = params
        .get("sessionId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ServerError::BadRequest("sessionId is required".into()))?;

    let Some(mut session) = state.app_state.session_store.get_session(session_id).await? else {
        return Err(ServerError::SessionNotFound(session_id.to_string()));
    };

    subscribe_session(ctx, &state.app_state, session_id, sender).await;
    let _ = seed_history(&state.app_state, session_id, sender).await;
    ctx.sessions.insert(session_id.to_string());

    session.session.status = SessionStatus::Running;
    state.app_state.session_store.update_session(session_id, &session).await?;
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

    if let Some(controller) = &state.app_state.task_controller {
        let _ = controller.resume_task(session_id).await;
    }

    Ok(json!({
        "sessionId": session_id,
        "status": "running"
    }))
}

async fn subscribe_session(
    ctx: &mut AcpSessionContext,
    app_state: &AppState,
    session_id: &str,
    sender: &Arc<Mutex<SplitSink<WebSocket, WsMessage>>>,
) {
    if !ctx.receivers.contains_key(session_id) {
        if let Some(receiver) = app_state.session_store.subscribe_session_events(session_id) {
            ctx.receivers.insert(session_id.to_string(), receiver);
        }
    }
    if !ctx.pty_receivers.contains_key(session_id) {
        if let Some(controller) = &app_state.task_controller {
            if let Ok((backlog, rx)) = controller.subscribe_pty(session_id).await {
                for msg in backlog {
                    let _ = send_json(sender, pty_to_update(session_id, &msg)).await;
                }
                ctx.pty_receivers.insert(session_id.to_string(), rx);
            }
        }
    }
}

async fn seed_history(
    app_state: &AppState,
    session_id: &str,
    sender: &Arc<Mutex<SplitSink<WebSocket, WsMessage>>>,
) -> Result<(), ()> {
    if let Ok(events) = app_state.session_store.get_session_events(session_id).await {
        for event in events {
            let update = json!({
                "method": "session/update",
                "params": {
                    "sessionId": session_id,
                    "event": event_to_json(session_id, &event),
                }
            });
            send_json(sender, update).await?;
        }
    }
    Ok(())
}

fn session_to_json(session: &Session) -> Value {
    json!({
        "id": session.id,
        "tenantId": session.tenant_id,
        "projectId": session.project_id,
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

async fn flush_pty_events(
    ctx: &mut AcpSessionContext,
    sender: &Arc<Mutex<SplitSink<WebSocket, WsMessage>>>,
) -> bool {
    let mut sent = false;
    let mut dead = Vec::new();

    for (session_id, rx) in ctx.pty_receivers.iter_mut() {
        loop {
            match rx.try_recv() {
                Ok(msg) => {
                    let update = pty_to_update(session_id, &msg);
                    if send_json(sender, update).await.is_err() {
                        dead.push(session_id.clone());
                        break;
                    }
                    sent = true;
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
                Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => break,
                Err(tokio::sync::broadcast::error::TryRecvError::Closed) => {
                    dead.push(session_id.clone());
                    break;
                }
            }
        }
    }

    for id in dead {
        ctx.pty_receivers.remove(&id);
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
            "followerCommand": follower_command(
                &String::from_utf8_lossy(&tool.tool_execution_id),
                session_id,
                &String::from_utf8_lossy(&tool.tool_name)
            ),
        }),
        SessionEvent::ToolResult(result) => json!({
            "type": "tool_result",
            "sessionId": session_id,
            "toolName": String::from_utf8_lossy(&result.tool_name),
            "output": String::from_utf8_lossy(&result.tool_output),
            "executionId": String::from_utf8_lossy(&result.tool_execution_id),
            "status": tool_status_str(&result.status),
            "timestamp": result.timestamp,
            "followerCommand": follower_command(
                &String::from_utf8_lossy(&result.tool_execution_id),
                session_id,
                &String::from_utf8_lossy(&result.tool_name)
            ),
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

fn pty_to_update(session_id: &str, msg: &TaskManagerMessage) -> Value {
    match msg {
        TaskManagerMessage::PtyData(bytes) => json!({
            "method": "session/update",
            "params": {
                "sessionId": session_id,
                "event": {
                    "type": "terminal",
                    "encoding": "base64",
                    "data": B64.encode(bytes),
                    "timestamp": chrono::Utc::now().timestamp_millis(),
                }
            }
        }),
        TaskManagerMessage::PtyResize((cols, rows)) => json!({
            "method": "session/update",
            "params": {
                "sessionId": session_id,
                "event": {
                    "type": "terminal_resize",
                    "cols": cols,
                    "rows": rows,
                    "timestamp": chrono::Utc::now().timestamp_millis(),
                }
            }
        }),
        _ => json!({
            "method": "session/update",
            "params": {
                "sessionId": session_id,
                "event": {
                    "type": "terminal",
                    "encoding": "base64",
                    "data": "",
                    "timestamp": chrono::Utc::now().timestamp_millis(),
                }
            }
        }),
    }
}

fn terminal_follow_update(session_id: &str, execution_id: &str, command: &str) -> Value {
    json!({
        "method": "session/update",
        "params": {
            "sessionId": session_id,
            "event": {
                "type": "terminal_follow",
                "executionId": execution_id,
                "command": command,
                "timestamp": chrono::Utc::now().timestamp_millis(),
            }
        }
    })
}

fn terminal_detach_update(session_id: &str, execution_id: &str) -> Value {
    json!({
        "method": "session/update",
        "params": {
            "sessionId": session_id,
            "event": {
                "type": "terminal_detach",
                "executionId": execution_id,
                "timestamp": chrono::Utc::now().timestamp_millis(),
            }
        }
    })
}

fn tool_status_str(status: &SessionToolStatus) -> &'static str {
    match status {
        SessionToolStatus::Started => "started",
        SessionToolStatus::Completed => "completed",
        SessionToolStatus::Failed => "failed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pty_update_encodes_and_resizes() {
        let data = TaskManagerMessage::PtyData(b"hi".to_vec());
        let json = pty_to_update("sess", &data);
        assert_eq!(
            json.pointer("/params/event/type").and_then(|v| v.as_str()),
            Some("terminal")
        );
        assert_eq!(
            json.pointer("/params/event/data").and_then(|v| v.as_str()),
            Some(B64.encode("hi").as_str())
        );

        let resize = TaskManagerMessage::PtyResize((120, 33));
        let json = pty_to_update("sess", &resize);
        assert_eq!(
            json.pointer("/params/event/type").and_then(|v| v.as_str()),
            Some("terminal_resize")
        );
        assert_eq!(
            json.pointer("/params/event/cols").and_then(|v| v.as_u64()),
            Some(120)
        );
        assert_eq!(
            json.pointer("/params/event/rows").and_then(|v| v.as_u64()),
            Some(33)
        );
    }
}

async fn send_json(
    sender: &Arc<Mutex<SplitSink<WebSocket, WsMessage>>>,
    payload: Value,
) -> Result<(), ()> {
    let text = serde_json::to_string(&payload).map_err(|_| ())?;
    let mut guard = sender.lock().await;
    guard.send(WsMessage::Text(text)).await.map_err(|_| ())
}

async fn current_context_chars(
    app_state: &AppState,
    session_id: &str,
    session: &Session,
) -> ServerResult<usize> {
    let mut total = session.task.prompt.chars().count();
    if let Ok(events) = app_state.session_store.get_session_events(session_id).await {
        for event in events {
            if let SessionEvent::Log(log) = event {
                let msg = String::from_utf8_lossy(&log.message);
                if let Some(stripped) = msg.strip_prefix("user: ") {
                    total += stripped.chars().count();
                }
            }
        }
    }
    Ok(total)
}

fn translate_create_request(params: Value) -> ServerResult<CreateTaskRequest> {
    let prompt = params.get("prompt").and_then(|v| v.as_str()).unwrap_or_default().to_string();
    if prompt.trim().is_empty() {
        return Err(ServerError::BadRequest(
            "prompt is required for session/new".into(),
        ));
    }
    if prompt.chars().count() > MAX_CONTEXT_CHARS {
        return Err(ServerError::BadRequest(format!(
            "prompt exceeds max length of {} characters",
            MAX_CONTEXT_CHARS
        )));
    }
    if prompt.trim().is_empty() {
        return Err(ServerError::BadRequest(
            "prompt is required for session/new".into(),
        ));
    }

    let repo_url = params.get("repoUrl").and_then(|v| v.as_str()).and_then(|s| Url::parse(s).ok());
    let repo = RepoConfig {
        mode: if repo_url.is_some() {
            RepoMode::Git
        } else {
            RepoMode::None
        },
        url: repo_url,
        branch: params.get("branch").and_then(|v| v.as_str()).map(|s| s.to_string()),
        commit: None,
    };

    let agent_model = params.get("agent").and_then(|v| v.as_str()).unwrap_or("sonnet").to_string();

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
        tenant_id: params.get("tenantId").and_then(|v| v.as_str()).map(|s| s.to_string()),
        project_id: params.get("projectId").and_then(|v| v.as_str()).map(|s| s.to_string()),
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
