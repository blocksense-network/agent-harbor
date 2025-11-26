// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Transport layer for ACP (SDK-backed).
//!
//! WebSocket transport authenticates using the REST auth config (API key or JWT)
//! passed via `?api_key=` query param or `Authorization` header, applies a
//! connection-limit guard/idle timeout, and delegates JSON-RPC handling to the
//! ACP runtime via the vendored SDK dispatcher. Stdio plumbing reuses the same
//! dispatcher.

use crate::{
    acp::translator::JsonRpcTranslator,
    auth::{AuthConfig, Claims},
    config::{AcpAuthPolicy, AcpConfig},
    error::{ServerError, ServerResult},
    services::SessionService,
    state::AppState,
};
use agent_client_protocol::{
    AgentCapabilities, Error, IncomingMessage, OutgoingMessage, ResponseResult, Side,
    ValueDispatcher,
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
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
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
use serde::Deserialize;

/// Conservative context window guardrail for inbound ACP prompts.
/// Matches the per-message cap to keep total user-provided text bounded
/// until the recorder/LLM bridge is wired (Milestone 4 follow-up).
const MAX_CONTEXT_CHARS: usize = 16_000;

/// Minimal RPC payload that preserves the method name and params as raw JSON
/// for routing through the existing translator/TaskManager bridge without
/// depending on the SDK's typed request structs (which do not yet mirror
/// Harbor's prompt/session fields).
#[derive(Clone, Debug, Serialize, Deserialize)]
struct RawRpcPayload {
    method: Arc<str>,
    #[serde(default)]
    params: Value,
}

#[derive(Clone)]
struct IncomingSide;

impl Side for IncomingSide {
    type InRequest = RawRpcPayload;
    type OutResponse = Value;
    type InNotification = RawRpcPayload;

    fn decode_request(
        method: &str,
        params: Option<&serde_json::value::RawValue>,
    ) -> Result<Self::InRequest, Error> {
        let params = params
            .and_then(|raw| serde_json::from_str(raw.get()).ok())
            .unwrap_or(Value::Null);
        Ok(RawRpcPayload {
            method: Arc::from(method),
            params,
        })
    }

    fn decode_notification(
        method: &str,
        params: Option<&serde_json::value::RawValue>,
    ) -> Result<Self::InNotification, Error> {
        let params = params
            .and_then(|raw| serde_json::from_str(raw.get()).ok())
            .unwrap_or(Value::Null);
        Ok(RawRpcPayload {
            method: Arc::from(method),
            params,
        })
    }
}

/// Outgoing side uses raw JSON params; decode methods are unreachable because
/// this side is only used for serialization.
#[derive(Clone)]
struct OutgoingSide;

impl Side for OutgoingSide {
    type InRequest = Value;
    type OutResponse = Value;
    type InNotification = Value;

    fn decode_request(
        _method: &str,
        _params: Option<&serde_json::value::RawValue>,
    ) -> Result<Self::InRequest, Error> {
        Err(Error::method_not_found())
    }

    fn decode_notification(
        _method: &str,
        _params: Option<&serde_json::value::RawValue>,
    ) -> Result<Self::InNotification, Error> {
        Err(Error::method_not_found())
    }
}

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

/// Run ACP over stdio using the same dispatcher that powers the WebSocket
/// transport. This is intended for `ah agent access-point --stdio-acp`.
pub async fn run_stdio(state: AcpTransportState) -> crate::acp::AcpResult<()> {
    let reader = BufReader::new(io::stdin());
    let mut lines = reader.lines();
    let writer = Arc::new(Mutex::new(io::stdout()));

    let context = Arc::new(Mutex::new(AcpSessionContext {
        auth_claims: None,
        ..Default::default()
    }));
    let (dispatcher, _streams) = ValueDispatcher::<IncomingSide, OutgoingSide>::new();
    let driver = Arc::new(Mutex::new(dispatcher));
    let mut incoming_rx = driver.lock().await.take_incoming().fuse();

    let mut tick = interval(Duration::from_millis(100));
    let idle_timeout = state.idle_timeout;
    let idle_timer = sleep(idle_timeout);
    tokio::pin!(idle_timer);

    loop {
        tokio::select! {
            _ = &mut idle_timer => {
                break;
            }
            _ = tick.tick() => {
                let mut ctx = context.lock().await;
                let flushed_events = flush_session_events_stdio(&mut ctx, &driver, &writer).await;
                let flushed_pty = flush_pty_events_stdio(&mut ctx, &driver, &writer).await;
                if flushed_events || flushed_pty {
                    idle_timer.as_mut().reset(Instant::now() + idle_timeout);
                }
            }
            maybe_incoming = incoming_rx.next() => {
                if let Some(message) = maybe_incoming {
                    match message {
                        IncomingMessage::Request { id, request } => {
                            let result = route_request_stdio(&state, &context, &driver, &writer, request, &HeaderMap::new()).await;
                            let outgoing = OutgoingMessage::Response {
                                id,
                                result: ResponseResult::from(result),
                            };
                            if send_outgoing_stdout(&driver, &writer, &outgoing).await.is_err() {
                                break;
                            }
                            idle_timer.as_mut().reset(Instant::now() + idle_timeout);
                        }
                        IncomingMessage::Notification { notification } => {
                            let _ = route_notification_stdio(&state, &context, &driver, &writer, notification, &HeaderMap::new()).await;
                            idle_timer.as_mut().reset(Instant::now() + idle_timeout);
                        }
                    }
                } else {
                    break;
                }
            }
            maybe_line = lines.next_line() => {
                match maybe_line {
                    Ok(Some(text)) => {
                        match serde_json::from_str::<Value>(&text) {
                            Ok(value) => {
                                let immediate = {
                                    let mut guard = driver.lock().await;
                                    guard.handle_json(value).await
                                };
                                if let Some(payload) = immediate {
                                    if send_json_stdout(&writer, payload).await.is_err() {
                                        break;
                                    }
                                }
                                idle_timer.as_mut().reset(Instant::now() + idle_timeout);
                            }
                            Err(_) => {
                                let _ = send_json_stdout(&writer, json_error(Value::Null, -32700, "invalid_json")).await;
                            }
                        }
                    }
                    Ok(None) => break, // EOF
                    Err(_) => break,
                }
            }
        }
    }

    Ok(())
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
    let claims = match authenticate(&state.auth, state.config.auth_policy, &query, &headers) {
        Ok(claims) => claims,
        Err(err) => {
            drop(permit);
            let status = StatusCode::UNAUTHORIZED;
            return (status, Json(err.to_problem())).into_response();
        }
    };

    let header_clone = headers.clone();
    ws.on_upgrade(move |socket| handle_socket(socket, state, permit, claims, header_clone))
}

fn authenticate(
    auth: &AuthConfig,
    policy: AcpAuthPolicy,
    query: &AcpQuery,
    headers: &HeaderMap,
) -> Result<Option<Claims>, ServerError> {
    if matches!(policy, AcpAuthPolicy::Anonymous) {
        return Ok(None);
    }
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
    headers: HeaderMap,
) {
    let (sender, mut receiver) = socket.split();
    let sender = Arc::new(Mutex::new(sender));
    let context = Arc::new(Mutex::new(AcpSessionContext {
        auth_claims: claims,
        ..Default::default()
    }));
    let (dispatcher, _streams) = ValueDispatcher::<IncomingSide, OutgoingSide>::new();
    let driver = Arc::new(Mutex::new(dispatcher));
    let mut incoming_rx = driver.lock().await.take_incoming().fuse();

    let idle_timer = sleep(state.idle_timeout);
    tokio::pin!(idle_timer);
    let mut tick = interval(Duration::from_millis(100));

    loop {
        tokio::select! {
            _ = &mut idle_timer => {
                break;
            }
            _ = tick.tick() => {
                let mut ctx_guard = context.lock().await;
                let flushed_events = flush_session_events(&mut ctx_guard, &driver, &sender).await;
                let flushed_pty = flush_pty_events(&mut ctx_guard, &driver, &sender).await;
                drop(ctx_guard);
                if flushed_events || flushed_pty {
                    idle_timer.as_mut().reset(Instant::now() + state.idle_timeout);
                }
            }
            maybe_incoming = incoming_rx.next() => {
                if let Some(message) = maybe_incoming {
                    match message {
                        IncomingMessage::Request { id, request } => {
                            let result = route_request(&state, &context, &sender, &driver, request, &headers).await;
                            let outgoing = OutgoingMessage::Response {
                                id,
                                result: ResponseResult::from(result),
                            };
                            if send_outgoing(&driver, &sender, &outgoing).await.is_err() {
                                break;
                            }
                            idle_timer.as_mut().reset(Instant::now() + state.idle_timeout);
                        }
                        IncomingMessage::Notification { notification } => {
                            let _ = route_notification(&state, &context, &sender, &driver, notification, &headers).await;
                            idle_timer.as_mut().reset(Instant::now() + state.idle_timeout);
                        }
                    }
                } else {
                    break;
                }
            }
            maybe_msg = receiver.next() => {
                match maybe_msg {
                    Some(Ok(msg)) => {
                        match msg {
                            WsMessage::Text(text) => {
                                match serde_json::from_str::<Value>(&text) {
                                    Ok(value) => {
                                        let immediate = {
                                            let mut guard = driver.lock().await;
                                            guard.handle_json(value).await
                                        };
                                        if let Some(payload) = immediate {
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

async fn route_request(
    state: &AcpTransportState,
    ctx: &Arc<Mutex<AcpSessionContext>>,
    sender: &Arc<Mutex<SplitSink<WebSocket, WsMessage>>>,
    driver: &Arc<Mutex<ValueDispatcher<IncomingSide, OutgoingSide>>>,
    request: RawRpcPayload,
    headers: &HeaderMap,
) -> Result<Value, Error> {
    let mut guard = ctx.lock().await;
    let method = request.method.as_ref();
    match method {
        "initialize" => {
            let caps = JsonRpcTranslator::negotiate_caps(&state.config);
            guard.negotiated_caps = Some(caps.clone());
            Ok(JsonRpcTranslator::initialize_response(&caps))
        }
        "authenticate" => handle_authenticate(state, &mut guard, request.params, headers)
            .await
            .map_err(server_error_to_rpc),
        "session/new" | "session/list" | "session/load" | "session/prompt" | "session/cancel"
        | "session/pause" | "session/resume" => {
            require_initialized(&guard)?;
            drop(guard);
            // Re-acquire as needed inside handlers
            match method {
                "session/new" => {
                    let mut guard = ctx.lock().await;
                    handle_session_new(state, &mut guard, driver, sender, request.params)
                        .await
                        .map_err(server_error_to_rpc)
                }
                "session/list" => {
                    let guard = ctx.lock().await;
                    handle_session_list(state, request.params, &guard)
                        .await
                        .map_err(server_error_to_rpc)
                }
                "session/load" => {
                    let mut guard = ctx.lock().await;
                    handle_session_load(state, &mut guard, driver, sender, request.params)
                        .await
                        .map_err(server_error_to_rpc)
                }
                "session/prompt" => {
                    let mut guard = ctx.lock().await;
                    handle_session_prompt(state, &mut guard, driver, sender, request.params)
                        .await
                        .map_err(server_error_to_rpc)
                }
                "session/cancel" => {
                    let mut guard = ctx.lock().await;
                    handle_session_cancel(state, &mut guard, driver, sender, request.params)
                        .await
                        .map_err(server_error_to_rpc)
                }
                "session/pause" => {
                    let mut guard = ctx.lock().await;
                    handle_session_pause(state, &mut guard, driver, sender, request.params)
                        .await
                        .map_err(server_error_to_rpc)
                }
                "session/resume" => {
                    let mut guard = ctx.lock().await;
                    handle_session_resume(state, &mut guard, driver, sender, request.params)
                        .await
                        .map_err(server_error_to_rpc)
                }
                _ => Err(Error::method_not_found()),
            }
        }
        "_ah/terminal/write" => {
            handle_terminal_write(state, &mut guard, driver, sender, request.params)
                .await
                .map_err(server_error_to_rpc)
        }
        "_ah/terminal/follow" => {
            drop(guard);
            handle_terminal_follow(state, driver, sender, request.params)
                .await
                .map_err(server_error_to_rpc)
        }
        "_ah/terminal/detach" => {
            drop(guard);
            handle_terminal_detach(driver, sender, request.params)
                .await
                .map_err(server_error_to_rpc)
        }
        _ => Ok(request.params),
    }
}

async fn route_notification(
    state: &AcpTransportState,
    ctx: &Arc<Mutex<AcpSessionContext>>,
    sender: &Arc<Mutex<SplitSink<WebSocket, WsMessage>>>,
    driver: &Arc<Mutex<ValueDispatcher<IncomingSide, OutgoingSide>>>,
    notification: RawRpcPayload,
    headers: &HeaderMap,
) -> Result<(), Error> {
    // Treat notifications the same as requests but drop the response
    let _ = route_request(state, ctx, sender, driver, notification, headers).await?;
    Ok(())
}

fn require_initialized(ctx: &AcpSessionContext) -> Result<(), Error> {
    if ctx.negotiated_caps.is_none() {
        Err(Error::invalid_request()
            .with_data("initialize must be called before session operations"))
    } else {
        Ok(())
    }
}

fn server_error_to_rpc(err: ServerError) -> Error {
    match err {
        ServerError::BadRequest(msg) => Error::invalid_params().with_data(msg),
        ServerError::Validation(_) => Error::invalid_params(),
        ServerError::Auth(msg) | ServerError::Authorization(msg) => {
            Error::auth_required().with_data(msg)
        }
        ServerError::SessionNotFound(id) | ServerError::TaskNotFound(id) => {
            Error::resource_not_found(Some(id))
        }
        _ => Error::internal_error().with_data(err.to_string()),
    }
}

async fn route_request_stdio(
    state: &AcpTransportState,
    ctx: &Arc<Mutex<AcpSessionContext>>,
    driver: &Arc<Mutex<ValueDispatcher<IncomingSide, OutgoingSide>>>,
    writer: &Arc<Mutex<tokio::io::Stdout>>,
    request: RawRpcPayload,
    headers: &HeaderMap,
) -> Result<Value, Error> {
    let mut guard = ctx.lock().await;
    let method = request.method.as_ref();
    match method {
        "initialize" => {
            let caps = JsonRpcTranslator::negotiate_caps(&state.config);
            guard.negotiated_caps = Some(caps.clone());
            Ok(JsonRpcTranslator::initialize_response(&caps))
        }
        "authenticate" => handle_authenticate(state, &mut guard, request.params, headers)
            .await
            .map_err(server_error_to_rpc),
        "session/new" | "session/list" | "session/load" | "session/prompt" | "session/cancel"
        | "session/pause" | "session/resume" => {
            require_initialized(&guard)?;
            drop(guard);
            match method {
                "session/new" => {
                    let mut guard = ctx.lock().await;
                    handle_session_new_stdio(state, &mut guard, driver, writer, request.params)
                        .await
                        .map_err(server_error_to_rpc)
                }
                "session/list" => {
                    let guard = ctx.lock().await;
                    handle_session_list(state, request.params, &guard)
                        .await
                        .map_err(server_error_to_rpc)
                }
                "session/load" => {
                    let mut guard = ctx.lock().await;
                    handle_session_load_stdio(state, &mut guard, driver, writer, request.params)
                        .await
                        .map_err(server_error_to_rpc)
                }
                "session/prompt" => {
                    let mut guard = ctx.lock().await;
                    handle_session_prompt_stdio(state, &mut guard, driver, writer, request.params)
                        .await
                        .map_err(server_error_to_rpc)
                }
                "session/cancel" => {
                    let mut guard = ctx.lock().await;
                    handle_session_cancel_stdio(state, &mut guard, driver, writer, request.params)
                        .await
                        .map_err(server_error_to_rpc)
                }
                "session/pause" => {
                    let mut guard = ctx.lock().await;
                    handle_session_pause_stdio(state, &mut guard, driver, writer, request.params)
                        .await
                        .map_err(server_error_to_rpc)
                }
                "session/resume" => {
                    let mut guard = ctx.lock().await;
                    handle_session_resume_stdio(state, &mut guard, driver, writer, request.params)
                        .await
                        .map_err(server_error_to_rpc)
                }
                _ => Err(Error::method_not_found()),
            }
        }
        "_ah/terminal/write" => {
            handle_terminal_write_stdio(state, &mut guard, driver, writer, request.params)
                .await
                .map_err(server_error_to_rpc)
        }
        "_ah/terminal/follow" => {
            drop(guard);
            handle_terminal_follow_stdio(state, driver, writer, request.params)
                .await
                .map_err(server_error_to_rpc)
        }
        "_ah/terminal/detach" => {
            drop(guard);
            handle_terminal_detach_stdio(driver, writer, request.params)
                .await
                .map_err(server_error_to_rpc)
        }
        _ => Ok(request.params),
    }
}

async fn route_notification_stdio(
    state: &AcpTransportState,
    ctx: &Arc<Mutex<AcpSessionContext>>,
    driver: &Arc<Mutex<ValueDispatcher<IncomingSide, OutgoingSide>>>,
    writer: &Arc<Mutex<tokio::io::Stdout>>,
    notification: RawRpcPayload,
    headers: &HeaderMap,
) -> Result<(), Error> {
    let _ = route_request_stdio(state, ctx, driver, writer, notification, headers).await?;
    Ok(())
}

#[derive(Default)]
struct AcpSessionContext {
    negotiated_caps: Option<AgentCapabilities>,
    sessions: HashSet<String>,
    receivers: HashMap<String, Receiver<SessionEvent>>,
    pty_receivers: HashMap<String, tokio::sync::broadcast::Receiver<TaskManagerMessage>>,
    auth_claims: Option<Claims>,
}

fn json_error(id: Value, code: i64, message: &str) -> Value {
    serde_json::json!({
        "id": id,
        "error": { "code": code, "message": message }
    })
}

async fn handle_session_new(
    state: &AcpTransportState,
    ctx: &mut AcpSessionContext,
    driver: &Arc<Mutex<ValueDispatcher<IncomingSide, OutgoingSide>>>,
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
        subscribe_session(ctx, &state.app_state, &session_id, driver, sender).await;
        let _ = seed_history(&state.app_state, &session_id, driver, sender).await;
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

async fn handle_session_new_stdio(
    state: &AcpTransportState,
    ctx: &mut AcpSessionContext,
    driver: &Arc<Mutex<ValueDispatcher<IncomingSide, OutgoingSide>>>,
    writer: &Arc<Mutex<tokio::io::Stdout>>,
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
        subscribe_session_stdio(ctx, &state.app_state, &session_id, driver, writer).await;
        let _ = seed_history_stdio(&state.app_state, &session_id, driver, writer).await;
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

async fn handle_authenticate(
    state: &AcpTransportState,
    ctx: &mut AcpSessionContext,
    params: Value,
    headers: &HeaderMap,
) -> ServerResult<Value> {
    let query = AcpQuery {
        api_key: params.get("apiKey").and_then(|v| v.as_str()).map(|s| s.to_string()),
        bearer_token: params.get("token").and_then(|v| v.as_str()).map(|s| s.to_string()),
    };

    let claims = authenticate(&state.auth, state.config.auth_policy, &query, headers)?;
    ctx.auth_claims = claims.clone();

    Ok(json!({
        "authenticated": true,
        "tenantId": claims.as_ref().and_then(|c| c.tenant_id.clone()),
        "projectId": claims.as_ref().and_then(|c| c.project_id.clone())
    }))
}

async fn handle_terminal_write(
    state: &AcpTransportState,
    ctx: &mut AcpSessionContext,
    driver: &Arc<Mutex<ValueDispatcher<IncomingSide, OutgoingSide>>>,
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

    subscribe_session(ctx, &state.app_state, session_id, driver, sender).await;
    Ok(json!({"sessionId": session_id, "accepted": true}))
}

async fn handle_terminal_write_stdio(
    state: &AcpTransportState,
    ctx: &mut AcpSessionContext,
    driver: &Arc<Mutex<ValueDispatcher<IncomingSide, OutgoingSide>>>,
    writer: &Arc<Mutex<tokio::io::Stdout>>,
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

    subscribe_session_stdio(ctx, &state.app_state, session_id, driver, writer).await;
    Ok(json!({"sessionId": session_id, "accepted": true}))
}

async fn handle_terminal_follow(
    state: &AcpTransportState,
    driver: &Arc<Mutex<ValueDispatcher<IncomingSide, OutgoingSide>>>,
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

    let update = terminal_follow_params(session_id, execution_id, &cmd);
    let _ = send_session_notification(driver, sender, update).await;

    // Attempt to stream PTY backlog + live updates when available.
    if let Some(controller) = &state.app_state.task_controller {
        match controller.subscribe_pty(session_id).await {
            Ok((backlog, mut rx)) => {
                for msg in backlog {
                    if matches!(
                        msg,
                        TaskManagerMessage::PtyData(_) | TaskManagerMessage::PtyResize(_)
                    ) {
                        let _ = send_session_notification(
                            driver,
                            sender,
                            pty_to_params(session_id, &msg),
                        )
                        .await;
                    }
                }
                let sender_clone = Arc::clone(sender);
                let driver_clone = driver.clone();
                let session = session_id.to_string();
                tokio::spawn(async move {
                    while let Ok(msg) = rx.recv().await {
                        if matches!(
                            msg,
                            TaskManagerMessage::PtyData(_) | TaskManagerMessage::PtyResize(_)
                        ) {
                            let _ = send_session_notification(
                                &driver_clone,
                                &sender_clone,
                                pty_to_params(&session, &msg),
                            )
                            .await;
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

async fn handle_terminal_follow_stdio(
    state: &AcpTransportState,
    driver: &Arc<Mutex<ValueDispatcher<IncomingSide, OutgoingSide>>>,
    writer: &Arc<Mutex<tokio::io::Stdout>>,
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

    let update = terminal_follow_params(session_id, execution_id, &cmd);
    let _ = send_session_notification_stdout(driver, writer, update).await;

    if let Some(controller) = &state.app_state.task_controller {
        match controller.subscribe_pty(session_id).await {
            Ok((backlog, mut rx)) => {
                for msg in backlog {
                    if matches!(
                        msg,
                        TaskManagerMessage::PtyData(_) | TaskManagerMessage::PtyResize(_)
                    ) {
                        let _ = send_session_notification_stdout(
                            driver,
                            writer,
                            pty_to_params(session_id, &msg),
                        )
                        .await;
                    }
                }
                let driver_clone = driver.clone();
                let writer_clone = writer.clone();
                let session = session_id.to_string();
                tokio::spawn(async move {
                    while let Ok(msg) = rx.recv().await {
                        if matches!(
                            msg,
                            TaskManagerMessage::PtyData(_) | TaskManagerMessage::PtyResize(_)
                        ) {
                            let _ = send_session_notification_stdout(
                                &driver_clone,
                                &writer_clone,
                                pty_to_params(&session, &msg),
                            )
                            .await;
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
    driver: &Arc<Mutex<ValueDispatcher<IncomingSide, OutgoingSide>>>,
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

    let update = terminal_detach_params(session_id, execution_id);
    let _ = send_session_notification(driver, sender, update).await;

    Ok(json!({ "sessionId": session_id, "executionId": execution_id, "detached": true }))
}

async fn handle_terminal_detach_stdio(
    driver: &Arc<Mutex<ValueDispatcher<IncomingSide, OutgoingSide>>>,
    writer: &Arc<Mutex<tokio::io::Stdout>>,
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

    let update = terminal_detach_params(session_id, execution_id);
    let _ = send_session_notification_stdout(driver, writer, update).await;

    Ok(json!({ "sessionId": session_id, "executionId": execution_id, "detached": true }))
}

async fn handle_session_list(
    state: &AcpTransportState,
    params: Value,
    ctx: &AcpSessionContext,
) -> ServerResult<Value> {
    let offset = params.get("offset").and_then(|v| v.as_u64()).unwrap_or(0).min(u64::MAX);
    let limit = params.get("limit").and_then(|v| v.as_u64());

    let filters = FilterQuery {
        status: params.get("status").and_then(|v| v.as_str()).map(|s| s.to_string()),
        agent: params.get("agent").and_then(|v| v.as_str()).map(|s| s.to_string()),
        project_id: params
            .get("projectId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| ctx.auth_claims.as_ref().and_then(|c| c.project_id.clone())),
        tenant_id: params
            .get("tenantId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| ctx.auth_claims.as_ref().and_then(|c| c.tenant_id.clone())),
    };

    let sessions = state.app_state.session_store.list_sessions(&filters).await?;
    let start = offset.min(sessions.len() as u64) as usize;
    let end = limit
        .map(|l| start.saturating_add(l as usize).min(sessions.len()))
        .unwrap_or_else(|| sessions.len());

    let items: Vec<Value> =
        sessions.iter().skip(start).take(end - start).map(session_to_json).collect();
    Ok(json!({ "items": items, "total": sessions.len(), "offset": offset, "limit": limit }))
}

async fn handle_session_load(
    state: &AcpTransportState,
    ctx: &mut AcpSessionContext,
    driver: &Arc<Mutex<ValueDispatcher<IncomingSide, OutgoingSide>>>,
    sender: &Arc<Mutex<SplitSink<WebSocket, WsMessage>>>,
    params: Value,
) -> ServerResult<Value> {
    let session_id = params
        .get("sessionId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ServerError::BadRequest("sessionId is required".into()))?;

    if let Some(session) = state.app_state.session_store.get_session(session_id).await? {
        subscribe_session(ctx, &state.app_state, session_id, driver, sender).await;
        let _ = seed_history(&state.app_state, session_id, driver, sender).await;
        ctx.sessions.insert(session_id.to_string());
        Ok(json!({
            "session": session_to_json(&session.session),
        }))
    } else {
        Err(ServerError::SessionNotFound(session_id.to_string()))
    }
}

async fn handle_session_load_stdio(
    state: &AcpTransportState,
    ctx: &mut AcpSessionContext,
    driver: &Arc<Mutex<ValueDispatcher<IncomingSide, OutgoingSide>>>,
    writer: &Arc<Mutex<tokio::io::Stdout>>,
    params: Value,
) -> ServerResult<Value> {
    let session_id = params
        .get("sessionId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ServerError::BadRequest("sessionId is required".into()))?;

    if let Some(session) = state.app_state.session_store.get_session(session_id).await? {
        subscribe_session_stdio(ctx, &state.app_state, session_id, driver, writer).await;
        let _ = seed_history_stdio(&state.app_state, session_id, driver, writer).await;
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
    driver: &Arc<Mutex<ValueDispatcher<IncomingSide, OutgoingSide>>>,
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

    subscribe_session(ctx, &state.app_state, session_id, driver, sender).await;
    let _ = seed_history(&state.app_state, session_id, driver, sender).await;
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

async fn handle_session_prompt_stdio(
    state: &AcpTransportState,
    ctx: &mut AcpSessionContext,
    driver: &Arc<Mutex<ValueDispatcher<IncomingSide, OutgoingSide>>>,
    writer: &Arc<Mutex<tokio::io::Stdout>>,
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

    subscribe_session_stdio(ctx, &state.app_state, session_id, driver, writer).await;
    let _ = seed_history_stdio(&state.app_state, session_id, driver, writer).await;
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
    driver: &Arc<Mutex<ValueDispatcher<IncomingSide, OutgoingSide>>>,
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

    subscribe_session(ctx, &state.app_state, session_id, driver, sender).await;
    let _ = seed_history(&state.app_state, session_id, driver, sender).await;
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

async fn handle_session_cancel_stdio(
    state: &AcpTransportState,
    ctx: &mut AcpSessionContext,
    driver: &Arc<Mutex<ValueDispatcher<IncomingSide, OutgoingSide>>>,
    writer: &Arc<Mutex<tokio::io::Stdout>>,
    params: Value,
) -> ServerResult<Value> {
    let session_id = params
        .get("sessionId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ServerError::BadRequest("sessionId is required".into()))?;

    let Some(mut session) = state.app_state.session_store.get_session(session_id).await? else {
        return Err(ServerError::SessionNotFound(session_id.to_string()));
    };

    subscribe_session_stdio(ctx, &state.app_state, session_id, driver, writer).await;
    let _ = seed_history_stdio(&state.app_state, session_id, driver, writer).await;
    ctx.sessions.insert(session_id.to_string());

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
    driver: &Arc<Mutex<ValueDispatcher<IncomingSide, OutgoingSide>>>,
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

    subscribe_session(ctx, &state.app_state, session_id, driver, sender).await;
    let _ = seed_history(&state.app_state, session_id, driver, sender).await;
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

async fn handle_session_pause_stdio(
    state: &AcpTransportState,
    ctx: &mut AcpSessionContext,
    driver: &Arc<Mutex<ValueDispatcher<IncomingSide, OutgoingSide>>>,
    writer: &Arc<Mutex<tokio::io::Stdout>>,
    params: Value,
) -> ServerResult<Value> {
    let session_id = params
        .get("sessionId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ServerError::BadRequest("sessionId is required".into()))?;

    let Some(mut session) = state.app_state.session_store.get_session(session_id).await? else {
        return Err(ServerError::SessionNotFound(session_id.to_string()));
    };

    subscribe_session_stdio(ctx, &state.app_state, session_id, driver, writer).await;
    let _ = seed_history_stdio(&state.app_state, session_id, driver, writer).await;
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
    driver: &Arc<Mutex<ValueDispatcher<IncomingSide, OutgoingSide>>>,
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

    subscribe_session(ctx, &state.app_state, session_id, driver, sender).await;
    let _ = seed_history(&state.app_state, session_id, driver, sender).await;
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

async fn handle_session_resume_stdio(
    state: &AcpTransportState,
    ctx: &mut AcpSessionContext,
    driver: &Arc<Mutex<ValueDispatcher<IncomingSide, OutgoingSide>>>,
    writer: &Arc<Mutex<tokio::io::Stdout>>,
    params: Value,
) -> ServerResult<Value> {
    let session_id = params
        .get("sessionId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ServerError::BadRequest("sessionId is required".into()))?;

    let Some(mut session) = state.app_state.session_store.get_session(session_id).await? else {
        return Err(ServerError::SessionNotFound(session_id.to_string()));
    };

    subscribe_session_stdio(ctx, &state.app_state, session_id, driver, writer).await;
    let _ = seed_history_stdio(&state.app_state, session_id, driver, writer).await;
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
    driver: &Arc<Mutex<ValueDispatcher<IncomingSide, OutgoingSide>>>,
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
                    let _ =
                        send_session_notification(driver, sender, pty_to_params(session_id, &msg))
                            .await;
                }
                ctx.pty_receivers.insert(session_id.to_string(), rx);
            }
        }
    }
}

async fn subscribe_session_stdio(
    ctx: &mut AcpSessionContext,
    app_state: &AppState,
    session_id: &str,
    driver: &Arc<Mutex<ValueDispatcher<IncomingSide, OutgoingSide>>>,
    writer: &Arc<Mutex<tokio::io::Stdout>>,
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
                    let _ = send_session_notification_stdout(
                        driver,
                        writer,
                        pty_to_params(session_id, &msg),
                    )
                    .await;
                }
                ctx.pty_receivers.insert(session_id.to_string(), rx);
            }
        }
    }
}

async fn seed_history(
    app_state: &AppState,
    session_id: &str,
    driver: &Arc<Mutex<ValueDispatcher<IncomingSide, OutgoingSide>>>,
    sender: &Arc<Mutex<SplitSink<WebSocket, WsMessage>>>,
) -> Result<(), ()> {
    if let Ok(events) = app_state.session_store.get_session_events(session_id).await {
        for event in events {
            let params = json!({
                "sessionId": session_id,
                "event": event_to_json(session_id, &event),
            });
            send_session_notification(driver, sender, params).await?;
        }
    }
    Ok(())
}

async fn seed_history_stdio(
    app_state: &AppState,
    session_id: &str,
    driver: &Arc<Mutex<ValueDispatcher<IncomingSide, OutgoingSide>>>,
    writer: &Arc<Mutex<tokio::io::Stdout>>,
) -> Result<(), ()> {
    if let Ok(events) = app_state.session_store.get_session_events(session_id).await {
        for event in events {
            let params = json!({
                "sessionId": session_id,
                "event": event_to_json(session_id, &event),
            });
            send_session_notification_stdout(driver, writer, params).await?;
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
    driver: &Arc<Mutex<ValueDispatcher<IncomingSide, OutgoingSide>>>,
    sender: &Arc<Mutex<SplitSink<WebSocket, WsMessage>>>,
) -> bool {
    let mut sent = false;
    let mut dead_sessions = Vec::new();

    for (session_id, receiver) in ctx.receivers.iter_mut() {
        loop {
            match receiver.try_recv() {
                Ok(event) => {
                    let update = json!({
                        "sessionId": session_id,
                        "event": event_to_json(session_id, &event),
                    });
                    if send_session_notification(driver, sender, update).await.is_err() {
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

async fn flush_session_events_stdio(
    ctx: &mut AcpSessionContext,
    driver: &Arc<Mutex<ValueDispatcher<IncomingSide, OutgoingSide>>>,
    writer: &Arc<Mutex<tokio::io::Stdout>>,
) -> bool {
    let mut sent = false;
    let mut dead_sessions = Vec::new();

    for (session_id, receiver) in ctx.receivers.iter_mut() {
        loop {
            match receiver.try_recv() {
                Ok(event) => {
                    let update = json!({
                        "sessionId": session_id,
                        "event": event_to_json(session_id, &event),
                    });
                    if send_session_notification_stdout(driver, writer, update).await.is_err() {
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
    driver: &Arc<Mutex<ValueDispatcher<IncomingSide, OutgoingSide>>>,
    sender: &Arc<Mutex<SplitSink<WebSocket, WsMessage>>>,
) -> bool {
    let mut sent = false;
    let mut dead = Vec::new();

    for (session_id, rx) in ctx.pty_receivers.iter_mut() {
        loop {
            match rx.try_recv() {
                Ok(msg) => {
                    let update = pty_to_params(session_id, &msg);
                    if send_session_notification(driver, sender, update).await.is_err() {
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

async fn flush_pty_events_stdio(
    ctx: &mut AcpSessionContext,
    driver: &Arc<Mutex<ValueDispatcher<IncomingSide, OutgoingSide>>>,
    writer: &Arc<Mutex<tokio::io::Stdout>>,
) -> bool {
    let mut sent = false;
    let mut dead = Vec::new();

    for (session_id, rx) in ctx.pty_receivers.iter_mut() {
        loop {
            match rx.try_recv() {
                Ok(msg) => {
                    let update = pty_to_params(session_id, &msg);
                    if send_session_notification_stdout(driver, writer, update).await.is_err() {
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

fn pty_to_params(session_id: &str, msg: &TaskManagerMessage) -> Value {
    match msg {
        TaskManagerMessage::PtyData(bytes) => json!({
            "sessionId": session_id,
            "event": {
                "type": "terminal",
                "encoding": "base64",
                "data": B64.encode(bytes),
                "timestamp": chrono::Utc::now().timestamp_millis(),
            }
        }),
        TaskManagerMessage::PtyResize((cols, rows)) => json!({
            "sessionId": session_id,
            "event": {
                "type": "terminal_resize",
                "cols": cols,
                "rows": rows,
                "timestamp": chrono::Utc::now().timestamp_millis(),
            }
        }),
        _ => json!({
            "sessionId": session_id,
            "event": {
                "type": "terminal",
                "encoding": "base64",
                "data": "",
                "timestamp": chrono::Utc::now().timestamp_millis(),
            }
        }),
    }
}

fn terminal_follow_params(session_id: &str, execution_id: &str, command: &str) -> Value {
    json!({
        "sessionId": session_id,
        "event": {
            "type": "terminal_follow",
            "executionId": execution_id,
            "command": command,
            "timestamp": chrono::Utc::now().timestamp_millis(),
        }
    })
}

fn terminal_detach_params(session_id: &str, execution_id: &str) -> Value {
    json!({
        "sessionId": session_id,
        "event": {
            "type": "terminal_detach",
            "executionId": execution_id,
            "timestamp": chrono::Utc::now().timestamp_millis(),
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
        let json = pty_to_params("sess", &data);
        assert_eq!(
            json.pointer("/event/type").and_then(|v| v.as_str()),
            Some("terminal")
        );
        assert_eq!(
            json.pointer("/event/data").and_then(|v| v.as_str()),
            Some(B64.encode("hi").as_str())
        );

        let resize = TaskManagerMessage::PtyResize((120, 33));
        let json = pty_to_params("sess", &resize);
        assert_eq!(
            json.pointer("/event/type").and_then(|v| v.as_str()),
            Some("terminal_resize")
        );
        assert_eq!(
            json.pointer("/event/cols").and_then(|v| v.as_u64()),
            Some(120)
        );
        assert_eq!(
            json.pointer("/event/rows").and_then(|v| v.as_u64()),
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

async fn send_outgoing(
    driver: &Arc<Mutex<ValueDispatcher<IncomingSide, OutgoingSide>>>,
    sender: &Arc<Mutex<SplitSink<WebSocket, WsMessage>>>,
    message: &OutgoingMessage<IncomingSide, OutgoingSide>,
) -> Result<(), ()> {
    let payload = {
        let dispatcher = driver.lock().await;
        dispatcher.outgoing_to_value(message).map_err(|_| ())?
    };
    send_json(sender, payload).await
}

async fn send_session_notification(
    driver: &Arc<Mutex<ValueDispatcher<IncomingSide, OutgoingSide>>>,
    sender: &Arc<Mutex<SplitSink<WebSocket, WsMessage>>>,
    params: Value,
) -> Result<(), ()> {
    let message = OutgoingMessage::Notification {
        method: Arc::from("session/update"),
        params: Some(params),
    };
    send_outgoing(driver, sender, &message).await
}

async fn send_json_stdout(
    writer: &Arc<Mutex<tokio::io::Stdout>>,
    payload: Value,
) -> Result<(), ()> {
    let mut guard = writer.lock().await;
    let mut buf = serde_json::to_vec(&payload).map_err(|_| ())?;
    buf.push(b'\n');
    guard.write_all(&buf).await.map_err(|_| ())?;
    guard.flush().await.map_err(|_| ())
}

async fn send_outgoing_stdout(
    driver: &Arc<Mutex<ValueDispatcher<IncomingSide, OutgoingSide>>>,
    writer: &Arc<Mutex<tokio::io::Stdout>>,
    message: &OutgoingMessage<IncomingSide, OutgoingSide>,
) -> Result<(), ()> {
    let payload = {
        let dispatcher = driver.lock().await;
        dispatcher.outgoing_to_value(message).map_err(|_| ())?
    };
    send_json_stdout(writer, payload).await
}

async fn send_session_notification_stdout(
    driver: &Arc<Mutex<ValueDispatcher<IncomingSide, OutgoingSide>>>,
    writer: &Arc<Mutex<tokio::io::Stdout>>,
    params: Value,
) -> Result<(), ()> {
    let message = OutgoingMessage::Notification {
        method: Arc::from("session/update"),
        params: Some(params),
    };
    send_outgoing_stdout(driver, writer, &message).await
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
