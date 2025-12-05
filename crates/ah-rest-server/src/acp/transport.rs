// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Transport layer for ACP (SDK-backed).
//!
//! WebSocket transport authenticates using the REST auth config (API key or JWT)
//! passed via `?api_key=` query param or `Authorization` header, applies a
//! connection-limit guard/idle timeout, and delegates JSON-RPC handling to the
//! ACP runtime via the vendored SDK dispatcher. UDS and stdio transports reuse
//! the same dispatcher with line-delimited JSON framing.

use crate::{
    acp::{
        notify::Notifier,
        translator::{InitializeLite, JsonRpcTranslator},
    },
    auth::{AuthConfig, Claims},
    config::{AcpAuthPolicy, AcpConfig},
    error::{ServerError, ServerResult},
    services::SessionService,
    state::AppState,
};
use agent_client_protocol::{
    AgentCapabilities, AgentNotification, AuthMethodId, AuthenticateRequest, ClientNotification,
    ClientRequest, ContentBlock, Error, ExtRequest, IncomingMessage, OutgoingMessage,
    ResponseResult, SessionId, SessionNotification, SessionUpdate, Side, TextContent,
    ValueDispatcher, WrappedRequest,
};
use ah_acp_bridge::{
    ensure_uds_parent, notification_envelope, session_event_to_notification,
    value_to_session_notification,
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
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use tokio::io::{self, AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::broadcast::Receiver;
use tokio::{
    sync::{Mutex, Semaphore},
    time::{Instant, interval, sleep},
};
use tokio_tungstenite;
use tokio_tungstenite::tungstenite::Message as ClientMessage;
use url::Url;

type AnyWriter = Arc<Mutex<Box<dyn AsyncWrite + Send + Unpin>>>;

static STDIO_BOUND: AtomicBool = AtomicBool::new(false);

#[derive(Debug)]
struct StdioGuard;

impl Drop for StdioGuard {
    fn drop(&mut self) {
        STDIO_BOUND.store(false, Ordering::SeqCst);
    }
}

fn assert_stdio_guard() -> StdioGuard {
    let already = STDIO_BOUND.swap(true, Ordering::SeqCst);
    assert!(
        !already,
        "ACP stdio transport bound twice in the same process; this is a bug"
    );
    StdioGuard
}

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

#[derive(Default, Clone, Debug, Deserialize)]
struct AuthenticateLite {
    #[serde(rename = "methodId")]
    _method_id: Option<String>,
    #[serde(rename = "_meta", default)]
    meta: Option<Value>,
}

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

/// SDK side that keeps both typed and raw params to avoid losing Harbor-specific fields.
#[derive(Clone)]
struct HarborAgentSide;

impl Side for HarborAgentSide {
    type InRequest = WrappedRequest<agent_client_protocol::ClientRequest>;
    type OutResponse = Value;
    type InNotification = WrappedRequest<agent_client_protocol::ClientNotification>;

    fn decode_request(
        method: &str,
        params: Option<&serde_json::value::RawValue>,
    ) -> Result<Self::InRequest, Error> {
        let mut params_raw = params
            .and_then(|p| serde_json::from_str::<Value>(p.get()).ok())
            .unwrap_or(Value::Null);

        // Backwards compatibility: schema requires fields we historically omitted.
        if method == "initialize" {
            if let Value::Object(obj) = &mut params_raw {
                obj.entry("protocolVersion").or_insert_with(|| Value::String("1.0".to_string()));
            }
        }
        if method == "session/new" {
            if let Value::Object(obj) = &mut params_raw {
                obj.entry("cwd").or_insert_with(|| Value::String("/workspace".to_string()));
                obj.entry("mcpServers").or_insert_with(|| Value::Array(vec![]));
            }
        }
        if method == "session/load" {
            if let Value::Object(obj) = &mut params_raw {
                obj.entry("cwd").or_insert_with(|| Value::String("/workspace".to_string()));
                obj.entry("mcpServers").or_insert_with(|| Value::Array(vec![]));
            }
        }

        let raw_clone = params_raw.clone();

        // For prompt, synthesize the typed schema fields (`prompt` blocks) when only a
        // string message is supplied, keeping `raw` intact for Harbor handlers.
        let mut decode_value = params_raw.clone();
        if method == "session/prompt" {
            if let Some(message) = decode_value
                .get("message")
                .or_else(|| decode_value.get("prompt"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
            {
                let prompt_block = json!([{
                    "type": "text",
                    "text": message
                }]);
                if let Some(obj) = decode_value.as_object_mut() {
                    obj.entry("prompt").or_insert(prompt_block);
                }
            }
        }

        let raw_value =
            serde_json::to_string(&decode_value).map_err(|_| Error::invalid_params())?;
        let raw_val = serde_json::value::RawValue::from_string(raw_value)
            .map_err(|_| Error::invalid_params())?;

        // Extension methods that the SDK schema doesn't currently model (plus
        // legacy request forms such as session/cancel).
        let is_ext = matches!(
            method,
            "session/list"
                | "session/pause"
                | "session/resume"
                | "session/cancel"
                | "ping"
                | "_ah/terminal/follow"
                | "_ah/terminal/write"
                | "_ah/terminal/detach"
        );
        if is_ext {
            return Ok(WrappedRequest {
                typed: ClientRequest::ExtMethodRequest(ExtRequest {
                    method: method.trim_start_matches('_').into(),
                    params: raw_val.into(),
                }),
                raw: raw_clone,
            });
        }

        // Authenticate payloads coming from legacy clients may omit methodId/_meta;
        // fall back to a synthesized request so we can still route while preserving
        // the original params in `raw`.
        let req = if method == "authenticate" {
            match agent_client_protocol::AgentSide::decode_request(method, Some(&raw_val)) {
                Ok(r) => r,
                Err(_) => ClientRequest::AuthenticateRequest(AuthenticateRequest {
                    method_id: AuthMethodId("harbor-api-key".into()),
                    meta: params_raw.get("_meta").cloned(),
                }),
            }
        } else {
            agent_client_protocol::AgentSide::decode_request(method, Some(&raw_val))?
        };
        Ok(WrappedRequest {
            typed: req,
            raw: raw_clone,
        })
    }

    fn decode_notification(
        method: &str,
        params: Option<&serde_json::value::RawValue>,
    ) -> Result<Self::InNotification, Error> {
        let params_raw = params
            .and_then(|p| serde_json::from_str::<Value>(p.get()).ok())
            .unwrap_or(Value::Null);
        let notif = agent_client_protocol::AgentSide::decode_notification(method, params)?;
        Ok(WrappedRequest {
            typed: notif,
            raw: params_raw,
        })
    }
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

/// Run ACP over stdio using the same dispatcher that powers WebSocket/UDS.
/// Intended for inline launches (e.g. `ah acp --daemonize=never`).
pub async fn run_stdio(state: AcpTransportState) -> crate::acp::AcpResult<()> {
    let _guard = assert_stdio_guard();

    let reader = BufReader::new(io::stdin());
    let mut lines = reader.lines();
    let writer: AnyWriter = Arc::new(Mutex::new(Box::new(io::stdout())));

    let notifier = Notifier::new_threaded();

    let context = Arc::new(Mutex::new(AcpSessionContext {
        auth_claims: None,
        notifier: notifier.clone(),
        ..Default::default()
    }));
    let (dispatcher, _streams) = ValueDispatcher::<HarborAgentSide, OutgoingSide>::new();
    let driver = Arc::new(Mutex::new(dispatcher));
    let mut incoming_rx = driver.lock().await.take_incoming().fuse();

    if let Some(mut rx) = notifier.subscribe() {
        let writer_clone = writer.clone();
        tokio::spawn(async move {
            while let Ok(payload) = rx.recv().await {
                if send_json_lines(&writer_clone, payload).await.is_err() {
                    break;
                }
            }
        });
    }

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
                let notifier = ctx.notifier.clone();
                let flushed_events = flush_session_events_stdio(&mut ctx, &notifier).await;
                let flushed_pty = flush_pty_events_stdio(&mut ctx, &notifier).await;
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
                            if send_outgoing_lines(&driver, &writer, &outgoing).await.is_err() {
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
                                    if send_json_lines(&writer, payload).await.is_err() {
                                        break;
                                    }
                                }
                                idle_timer.as_mut().reset(Instant::now() + idle_timeout);
                            }
                            Err(_) => {
                                let _ = send_json_lines(&writer, json_error(Value::Null, -32700, "invalid_json")).await;
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

/// Run ACP over a Unix-domain socket using the same line-based framing as
/// stdio. This allows local clients (and ssh port-forwards) to reuse the ACP
/// gateway without HTTP upgrade when the access point runs as a daemon. The
/// same dispatcher is used for stdio when launched inline (e.g. by `ah acp`).
#[cfg(unix)]
pub async fn run_uds(
    state: AcpTransportState,
    path: std::path::PathBuf,
) -> crate::acp::AcpResult<()> {
    use std::os::unix::fs::PermissionsExt;
    use tokio::fs;
    use tokio::net::UnixListener;

    ensure_uds_parent(&path).map_err(|e| crate::acp::errors::AcpError::Internal(e.to_string()))?;
    if path.exists() {
        let _ = fs::remove_file(&path).await;
    }

    let listener = UnixListener::bind(&path)?;
    let _cleanup = SocketCleanup(path.clone());
    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    let shared_state = Arc::new(state);

    loop {
        let (stream, _) = listener.accept().await?;
        let state_clone = shared_state.clone();
        tokio::spawn(async move {
            if let Err(err) = handle_uds_stream(stream, state_clone).await {
                tracing::debug!(?err, "UDS ACP connection terminated");
            }
        });
    }
}

#[cfg(not(unix))]
pub async fn run_uds(
    _state: AcpTransportState,
    _path: std::path::PathBuf,
) -> crate::acp::AcpResult<()> {
    Err(crate::acp::errors::AcpError::Internal(
        "UDS ACP transport not supported on this platform".to_string(),
    ))
}

/// Removes the socket file when the listener task exits so repeated invocations
/// do not fail on stale paths.
#[cfg(unix)]
struct SocketCleanup(std::path::PathBuf);

#[cfg(unix)]
impl Drop for SocketCleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[cfg(unix)]
async fn handle_uds_stream(
    stream: tokio::net::UnixStream,
    state: Arc<AcpTransportState>,
) -> crate::acp::AcpResult<()> {
    let (reader_half, writer_half) = stream.into_split();
    let reader = BufReader::new(reader_half);
    let mut lines = reader.lines();
    let writer: AnyWriter = Arc::new(Mutex::new(Box::new(writer_half)));

    let notifier = Notifier::new_threaded();
    let context = Arc::new(Mutex::new(AcpSessionContext {
        auth_claims: None,
        notifier: notifier.clone(),
        ..Default::default()
    }));
    let (dispatcher, _streams) = ValueDispatcher::<HarborAgentSide, OutgoingSide>::new();
    let driver = Arc::new(Mutex::new(dispatcher));
    let mut incoming_rx = driver.lock().await.take_incoming().fuse();

    if let Some(mut rx) = notifier.subscribe() {
        let writer_clone = writer.clone();
        tokio::spawn(async move {
            while let Ok(payload) = rx.recv().await {
                if send_json_lines(&writer_clone, payload).await.is_err() {
                    break;
                }
            }
        });
    }

    let mut tick = interval(Duration::from_millis(100));
    let idle_timeout = state.idle_timeout;
    let idle_timer = sleep(idle_timeout);
    tokio::pin!(idle_timer);

    loop {
        tokio::select! {
            _ = &mut idle_timer => break,
            _ = tick.tick() => {
                let mut ctx = context.lock().await;
                let notifier = ctx.notifier.clone();
                let flushed_events = flush_session_events_stdio(&mut ctx, &notifier).await;
                let flushed_pty = flush_pty_events_stdio(&mut ctx, &notifier).await;
                if flushed_events || flushed_pty {
                    idle_timer.as_mut().reset(Instant::now() + idle_timeout);
                }
            }
            maybe_incoming = incoming_rx.next() => {
                if let Some(message) = maybe_incoming {
                    match message {
                        IncomingMessage::Request { id, request } => {
                            let result = route_request_stdio(state.as_ref(), &context, &driver, &writer, request, &HeaderMap::new()).await;
                            let outgoing = OutgoingMessage::Response { id, result: ResponseResult::from(result) };
                            if send_outgoing_lines(&driver, &writer, &outgoing).await.is_err() {
                                break;
                            }
                            idle_timer.as_mut().reset(Instant::now() + idle_timeout);
                        }
                        IncomingMessage::Notification { notification } => {
                            let _ = route_notification_stdio(state.as_ref(), &context, &driver, &writer, notification, &HeaderMap::new()).await;
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
                                    if send_json_lines(&writer, payload).await.is_err() {
                                        break;
                                    }
                                }
                                idle_timer.as_mut().reset(Instant::now() + idle_timeout);
                            }
                            Err(_) => {
                                let _ = send_json_lines(&writer, json_error(Value::Null, -32700, "invalid_json")).await;
                            }
                        }
                    }
                    Ok(None) | Err(_) => break,
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
    let notifier = Notifier::new_threaded();
    let context = Arc::new(Mutex::new(AcpSessionContext {
        auth_claims: claims,
        notifier: notifier.clone(),
        ..Default::default()
    }));
    let (dispatcher, _streams) = ValueDispatcher::<HarborAgentSide, OutgoingSide>::new();
    let driver = Arc::new(Mutex::new(dispatcher));
    let mut incoming_rx = driver.lock().await.take_incoming().fuse();

    // Forward SDK-produced notifications to the WebSocket transport.
    if let Some(mut rx) = notifier.subscribe() {
        let sender_clone = sender.clone();
        tokio::spawn(async move {
            while let Ok(payload) = rx.recv().await {
                if send_json(&sender_clone, payload).await.is_err() {
                    break;
                }
            }
        });
    }

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
                    let notifier = ctx_guard.notifier.clone();
                    let flushed_events =
                        flush_session_events(&mut ctx_guard, &notifier).await;
                    let flushed_pty =
                        flush_pty_events(&mut ctx_guard, &notifier).await;
            drop(ctx_guard);
            if flushed_events || flushed_pty {
                idle_timer.as_mut().reset(Instant::now() + state.idle_timeout);
            }
        }
                maybe_incoming = incoming_rx.next() => {
                    if let Some(message) = maybe_incoming {
                        match message {
                            IncomingMessage::Request { id, request } => {
                                let result = route_wrapped_request(&state, &context, &sender, &driver, request, &headers).await;
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

async fn route_wrapped_request(
    state: &AcpTransportState,
    ctx: &Arc<Mutex<AcpSessionContext>>,
    sender: &Arc<Mutex<SplitSink<WebSocket, WsMessage>>>,
    driver: &Arc<Mutex<ValueDispatcher<HarborAgentSide, OutgoingSide>>>,
    request: WrappedRequest<agent_client_protocol::ClientRequest>,
    headers: &HeaderMap,
) -> Result<Value, Error> {
    let mut guard = ctx.lock().await;
    #[allow(unreachable_patterns)]
    match request.typed {
        ClientRequest::InitializeRequest(_) => {
            let caps = JsonRpcTranslator::negotiate_caps(&state.config);
            guard.negotiated_caps = Some(caps.clone());
            drop(guard);
            let req: InitializeLite =
                serde_json::from_value(request.raw.clone()).unwrap_or_default();
            Ok(JsonRpcTranslator::initialize_response_typed(&caps, &req))
        }
        ClientRequest::AuthenticateRequest(_) => {
            handle_authenticate_with_raw(state, &mut guard, request.raw, headers)
                .await
                .map_err(server_error_to_rpc)
        }
        ClientRequest::NewSessionRequest(_) => {
            require_initialized(&guard)?;
            drop(guard);
            let mut guard = ctx.lock().await;
            handle_session_new(state, &mut guard, driver, sender, request.raw)
                .await
                .map_err(server_error_to_rpc)
        }
        ClientRequest::LoadSessionRequest(_) => {
            require_initialized(&guard)?;
            drop(guard);
            let mut guard = ctx.lock().await;
            handle_session_load(state, &mut guard, request.raw)
                .await
                .map_err(server_error_to_rpc)
        }
        ClientRequest::PromptRequest(_) => {
            require_initialized(&guard)?;
            drop(guard);
            let mut guard = ctx.lock().await;
            handle_session_prompt(state, &mut guard, driver, sender, request.raw)
                .await
                .map_err(server_error_to_rpc)
        }
        ClientRequest::SetSessionModeRequest(_) => Err(Error::method_not_found()),
        ClientRequest::ExtMethodRequest(ext) => match ext.method.as_ref() {
            "session/list" => handle_session_list(state, request.raw, &guard)
                .await
                .map_err(server_error_to_rpc),
            "session/cancel" => {
                drop(guard);
                let mut guard = ctx.lock().await;
                handle_session_cancel(state, &mut guard, driver, sender, request.raw)
                    .await
                    .map_err(server_error_to_rpc)
            }
            "session/pause" => {
                drop(guard);
                let mut guard = ctx.lock().await;
                handle_session_pause(state, &mut guard, driver, sender, request.raw)
                    .await
                    .map_err(server_error_to_rpc)
            }
            "session/resume" => {
                drop(guard);
                let mut guard = ctx.lock().await;
                handle_session_resume(state, &mut guard, driver, sender, request.raw)
                    .await
                    .map_err(server_error_to_rpc)
            }
            "ah/terminal/follow" => {
                drop(guard);
                let mut guard = ctx.lock().await;
                handle_terminal_follow(state, &mut guard, driver, sender, request.raw)
                    .await
                    .map_err(server_error_to_rpc)
            }
            "ah/terminal/write" => {
                drop(guard);
                let mut guard = ctx.lock().await;
                handle_terminal_write(state, &mut guard, driver, sender, request.raw)
                    .await
                    .map_err(server_error_to_rpc)
            }
            "ah/terminal/detach" => {
                drop(guard);
                let notifier = {
                    let g = ctx.lock().await;
                    g.notifier.clone()
                };
                handle_terminal_detach(driver, sender, request.raw, &notifier)
                    .await
                    .map_err(server_error_to_rpc)
            }
            "ping" => Ok(request.raw),
            _ => Err(Error::method_not_found()),
        },
        _ => Err(Error::method_not_found()),
    }
}

async fn route_notification(
    state: &AcpTransportState,
    ctx: &Arc<Mutex<AcpSessionContext>>,
    sender: &Arc<Mutex<SplitSink<WebSocket, WsMessage>>>,
    driver: &Arc<Mutex<ValueDispatcher<HarborAgentSide, OutgoingSide>>>,
    notification: WrappedRequest<agent_client_protocol::ClientNotification>,
    _headers: &HeaderMap,
) -> Result<(), Error> {
    match notification.typed {
        ClientNotification::CancelNotification(_) => {
            let mut guard = ctx.lock().await;
            handle_session_cancel(state, &mut guard, driver, sender, notification.raw)
                .await
                .map_err(server_error_to_rpc)?;
        }
        ClientNotification::ExtNotification(_) => {}
    }
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
    driver: &Arc<Mutex<ValueDispatcher<HarborAgentSide, OutgoingSide>>>,
    writer: &AnyWriter,
    request: WrappedRequest<agent_client_protocol::ClientRequest>,
    headers: &HeaderMap,
) -> Result<Value, Error> {
    let mut guard = ctx.lock().await;
    #[allow(unreachable_patterns)]
    match request.typed {
        ClientRequest::InitializeRequest(_) => {
            let caps = JsonRpcTranslator::negotiate_caps(&state.config);
            guard.negotiated_caps = Some(caps.clone());
            drop(guard);
            let req: InitializeLite =
                serde_json::from_value(request.raw.clone()).unwrap_or_default();
            Ok(JsonRpcTranslator::initialize_response_typed(&caps, &req))
        }
        ClientRequest::AuthenticateRequest(_) => {
            handle_authenticate_with_raw(state, &mut guard, request.raw, headers)
                .await
                .map_err(server_error_to_rpc)
        }
        ClientRequest::NewSessionRequest(_) => {
            require_initialized(&guard)?;
            drop(guard);
            let mut guard = ctx.lock().await;
            handle_session_new_stdio(state, &mut guard, driver, writer, request.raw)
                .await
                .map_err(server_error_to_rpc)
        }
        ClientRequest::LoadSessionRequest(_) => {
            require_initialized(&guard)?;
            drop(guard);
            let mut guard = ctx.lock().await;
            handle_session_load_stdio(state, &mut guard, driver, writer, request.raw)
                .await
                .map_err(server_error_to_rpc)
        }
        ClientRequest::PromptRequest(_) => {
            require_initialized(&guard)?;
            drop(guard);
            let mut guard = ctx.lock().await;
            handle_session_prompt_stdio(state, &mut guard, driver, writer, request.raw)
                .await
                .map_err(server_error_to_rpc)
        }
        ClientRequest::SetSessionModeRequest(_) => Err(Error::method_not_found()),
        ClientRequest::ExtMethodRequest(ext) => match ext.method.as_ref() {
            "session/list" => handle_session_list(state, request.raw, &guard)
                .await
                .map_err(server_error_to_rpc),
            "session/cancel" => {
                drop(guard);
                let mut guard = ctx.lock().await;
                handle_session_cancel_stdio(state, &mut guard, driver, writer, request.raw)
                    .await
                    .map_err(server_error_to_rpc)
            }
            "session/pause" => {
                drop(guard);
                let mut guard = ctx.lock().await;
                handle_session_pause_stdio(state, &mut guard, driver, writer, request.raw)
                    .await
                    .map_err(server_error_to_rpc)
            }
            "session/resume" => {
                drop(guard);
                let mut guard = ctx.lock().await;
                handle_session_resume_stdio(state, &mut guard, driver, writer, request.raw)
                    .await
                    .map_err(server_error_to_rpc)
            }
            "ah/terminal/follow" => {
                drop(guard);
                let mut guard = ctx.lock().await;
                handle_terminal_follow_stdio(state, &mut guard, driver, writer, request.raw)
                    .await
                    .map_err(server_error_to_rpc)
            }
            "ah/terminal/write" => {
                drop(guard);
                let mut guard = ctx.lock().await;
                handle_terminal_write_stdio(state, &mut guard, driver, writer, request.raw)
                    .await
                    .map_err(server_error_to_rpc)
            }
            "ah/terminal/detach" => {
                drop(guard);
                let notifier = {
                    let g = ctx.lock().await;
                    g.notifier.clone()
                };
                handle_terminal_detach_stdio(driver, writer, request.raw, &notifier)
                    .await
                    .map_err(server_error_to_rpc)
            }
            _ => Err(Error::method_not_found()),
        },
        _ => Err(Error::method_not_found()),
    }
}

async fn route_notification_stdio(
    state: &AcpTransportState,
    ctx: &Arc<Mutex<AcpSessionContext>>,
    driver: &Arc<Mutex<ValueDispatcher<HarborAgentSide, OutgoingSide>>>,
    writer: &AnyWriter,
    notification: WrappedRequest<agent_client_protocol::ClientNotification>,
    _headers: &HeaderMap,
) -> Result<(), Error> {
    match notification.typed {
        ClientNotification::CancelNotification(_) => {
            let mut guard = ctx.lock().await;
            handle_session_cancel_stdio(state, &mut guard, driver, writer, notification.raw)
                .await
                .map_err(server_error_to_rpc)?;
        }
        ClientNotification::ExtNotification(_) => {}
    }
    Ok(())
}

#[derive(Default)]
struct AcpSessionContext {
    negotiated_caps: Option<AgentCapabilities>,
    sessions: HashSet<String>,
    receivers: HashMap<String, Receiver<SessionEvent>>,
    pty_receivers: HashMap<String, tokio::sync::broadcast::Receiver<TaskManagerMessage>>,
    auth_claims: Option<Claims>,
    /// Cached execution commands derived from recorded tool events to avoid trusting
    /// client-supplied follow commands.
    execution_commands: HashMap<String, HashMap<String, String>>,
    notifier: Notifier,
}

fn json_error(id: Value, code: i64, message: &str) -> Value {
    serde_json::json!({
        "id": id,
        "error": { "code": code, "message": message }
    })
}

fn sanitize_command(cmd: &str) -> Option<String> {
    let trimmed = cmd.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.len() > 1024 {
        return None;
    }
    if trimmed.chars().any(|c| c == '\n' || c == '\r') {
        return None;
    }
    Some(trimmed.to_string())
}

/// Derive a human-friendly command string for a tool execution from recorded
/// tool metadata, preferring the tool arguments over the name.
fn tool_event_command(tool_name: &[u8], tool_args: &[u8]) -> Option<String> {
    let args = String::from_utf8_lossy(tool_args).trim().to_string();
    if !args.is_empty() {
        return Some(args);
    }
    let name = String::from_utf8_lossy(tool_name).trim().to_string();
    if !name.is_empty() {
        return Some(name);
    }
    None
}

/// Walk recorded session events (latest first) to find the command associated
/// with an execution id. Only recorder-derived tool metadata is trusted.
async fn resolve_execution_command(
    ctx: &mut AcpSessionContext,
    app_state: &AppState,
    session_id: &str,
    execution_id: &str,
) -> Option<String> {
    if let Some(cmd) = ctx.execution_commands.get(session_id).and_then(|m| m.get(execution_id)) {
        return Some(cmd.clone());
    }
    if let Ok(events) = app_state.session_store.get_session_events(session_id).await {
        if let Some(cmd) = command_from_events(&events, execution_id) {
            ctx.execution_commands
                .entry(session_id.to_string())
                .or_default()
                .insert(execution_id.to_string(), cmd.clone());
            return Some(cmd);
        }
    }
    None
}

fn command_from_events(events: &[SessionEvent], execution_id: &str) -> Option<String> {
    for event in events.iter().rev() {
        match event {
            SessionEvent::ToolUse(ev) if ev.tool_execution_id == execution_id.as_bytes() => {
                if let Some(cmd) = tool_event_command(&ev.tool_name, &ev.tool_args) {
                    return Some(cmd);
                }
            }
            SessionEvent::ToolResult(ev) if ev.tool_execution_id == execution_id.as_bytes() => {
                if let Some(cmd) = tool_event_command(&ev.tool_name, &[]) {
                    return Some(cmd);
                }
            }
            _ => {}
        }
    }
    None
}

/// When recorder metadata is absent (e.g., synthetic/mock sessions), seed a minimal
/// tool_use event so follower commands can still be derived without trusting the
/// raw client-supplied string forever.
async fn maybe_cache_synthetic_tool_event(
    app_state: &AppState,
    ctx: &mut AcpSessionContext,
    session_id: &str,
    execution_id: &str,
    provided_command: &str,
) {
    if ctx
        .execution_commands
        .get(session_id)
        .and_then(|m| m.get(execution_id))
        .is_some()
    {
        return;
    }
    // Avoid storing obviously invalid commands.
    let Some(cmd) = sanitize_command(provided_command) else {
        return;
    };

    let event = SessionEvent::tool_use(
        "client-supplied".to_string(),
        cmd.clone(),
        execution_id.to_string(),
        SessionToolStatus::Started,
        chrono::Utc::now().timestamp_millis() as u64,
    );
    let _ = app_state.session_store.add_session_event(session_id, event.clone()).await;
    cache_execution_command(&mut ctx.execution_commands, session_id, &event);
}

async fn handle_authenticate_with_raw(
    state: &AcpTransportState,
    ctx: &mut AcpSessionContext,
    raw: Value,
    headers: &HeaderMap,
) -> ServerResult<Value> {
    let req: AuthenticateLite = serde_json::from_value(raw.clone()).unwrap_or_default();
    let meta = req.meta.unwrap_or_default();
    let query = AcpQuery {
        api_key: meta
            .get("apiKey")
            .or_else(|| raw.get("apiKey"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        bearer_token: meta
            .get("token")
            .or_else(|| raw.get("token"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    };

    let claims = authenticate(&state.auth, state.config.auth_policy, &query, headers)?;
    ctx.auth_claims = claims.clone();

    Ok(json!({
        "authenticated": true,
        "tenantId": claims.as_ref().and_then(|c| c.tenant_id.clone()),
        "projectId": claims.as_ref().and_then(|c| c.project_id.clone())
    }))
}

async fn handle_session_new(
    state: &AcpTransportState,
    ctx: &mut AcpSessionContext,
    _driver: &Arc<Mutex<ValueDispatcher<HarborAgentSide, OutgoingSide>>>,
    _sender: &Arc<Mutex<SplitSink<WebSocket, WsMessage>>>,
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
        subscribe_session(ctx, &state.app_state, &session_id).await;
        let _ = seed_history(&state.app_state, &session_id, &ctx.notifier).await;
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
    _driver: &Arc<Mutex<ValueDispatcher<HarborAgentSide, OutgoingSide>>>,
    _writer: &AnyWriter,
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
        subscribe_session_stdio(ctx, &state.app_state, &session_id).await;
        let _ = seed_history_stdio(&state.app_state, &session_id, &ctx.notifier).await;
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
    _driver: &Arc<Mutex<ValueDispatcher<HarborAgentSide, OutgoingSide>>>,
    _sender: &Arc<Mutex<SplitSink<WebSocket, WsMessage>>>,
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

    let controller = state
        .app_state
        .task_controller
        .as_ref()
        .ok_or_else(|| ServerError::NotImplemented("terminal write unavailable".into()))?;
    controller
        .inject_bytes(session_id, &bytes)
        .await
        .map_err(|e| ServerError::Internal(format!("failed to write to terminal: {e}")))?;

    subscribe_session(ctx, &state.app_state, session_id).await;
    Ok(json!({"sessionId": session_id, "accepted": true}))
}

async fn handle_terminal_write_stdio(
    state: &AcpTransportState,
    ctx: &mut AcpSessionContext,
    _driver: &Arc<Mutex<ValueDispatcher<HarborAgentSide, OutgoingSide>>>,
    _writer: &AnyWriter,
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

    let controller = state
        .app_state
        .task_controller
        .as_ref()
        .ok_or_else(|| ServerError::NotImplemented("terminal write unavailable".into()))?;
    controller
        .inject_bytes(session_id, &bytes)
        .await
        .map_err(|e| ServerError::Internal(format!("failed to write to terminal: {e}")))?;

    subscribe_session_stdio(ctx, &state.app_state, session_id).await;
    Ok(json!({"sessionId": session_id, "accepted": true}))
}

async fn handle_terminal_follow(
    state: &AcpTransportState,
    ctx: &mut AcpSessionContext,
    _driver: &Arc<Mutex<ValueDispatcher<HarborAgentSide, OutgoingSide>>>,
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
    if let Some(provided) = params.get("command").and_then(|v| v.as_str()) {
        maybe_cache_synthetic_tool_event(&state.app_state, ctx, session_id, execution_id, provided)
            .await;
    }

    let cmd_string = resolve_execution_command(ctx, &state.app_state, session_id, execution_id)
        .await
        .ok_or_else(|| {
            ServerError::BadRequest(
                "executionId has no recorded command (recorder metadata missing)".into(),
            )
        })?;

    let cmd = follower_command(execution_id, session_id, &cmd_string);

    let notifier = ctx.notifier.clone();
    let update = terminal_follow_params(session_id, execution_id, &cmd);
    let _ = send_raw_session_update_ws(sender, update.clone()).await;
    let _ = notifier
        .notify(
            "sessionUpdate",
            Some(AgentNotification::SessionNotification(
                session_update_from_json(&update),
            )),
        )
        .await;

    // Attempt to stream PTY backlog + live updates when available.
    if let Some(controller) = &state.app_state.task_controller {
        match controller.subscribe_pty(session_id).await {
            Ok((backlog, mut rx)) => {
                for msg in backlog {
                    if matches!(
                        msg,
                        TaskManagerMessage::PtyData(_)
                            | TaskManagerMessage::PtyResize(_)
                            | TaskManagerMessage::CommandChunk(_)
                    ) {
                        let notif = session_update_from_json(&terminal_to_params(session_id, &msg));
                        let _ = send_session_notification(notif, &notifier).await;
                    }
                }
                let notifier_clone = notifier.clone();
                let session = session_id.to_string();
                tokio::spawn(async move {
                    while let Ok(msg) = rx.recv().await {
                        if matches!(
                            msg,
                            TaskManagerMessage::PtyData(_)
                                | TaskManagerMessage::PtyResize(_)
                                | TaskManagerMessage::CommandChunk(_)
                        ) {
                            let notif =
                                session_update_from_json(&terminal_to_params(&session, &msg));
                            let _ = send_session_notification(notif, &notifier_clone).await;
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
    ctx: &mut AcpSessionContext,
    _driver: &Arc<Mutex<ValueDispatcher<HarborAgentSide, OutgoingSide>>>,
    writer: &AnyWriter,
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
    if let Some(provided) = params.get("command").and_then(|v| v.as_str()) {
        maybe_cache_synthetic_tool_event(&state.app_state, ctx, session_id, execution_id, provided)
            .await;
    }
    let cmd_string = resolve_execution_command(ctx, &state.app_state, session_id, execution_id)
        .await
        .ok_or_else(|| {
            ServerError::BadRequest(
                "executionId has no recorded command (recorder metadata missing)".into(),
            )
        })?;

    let cmd = follower_command(execution_id, session_id, &cmd_string);

    let notifier = ctx.notifier.clone();
    let update = terminal_follow_params(session_id, execution_id, &cmd);
    let follow_notif = session_update_from_json(&update);
    let _ = send_raw_session_update(writer, update.clone()).await;
    let _ = send_session_notification(follow_notif, &notifier).await;

    if let Some(controller) = &state.app_state.task_controller {
        match controller.subscribe_pty(session_id).await {
            Ok((backlog, mut rx)) => {
                for msg in backlog {
                    if matches!(
                        msg,
                        TaskManagerMessage::PtyData(_)
                            | TaskManagerMessage::PtyResize(_)
                            | TaskManagerMessage::CommandChunk(_)
                    ) {
                        let notif = session_update_from_json(&terminal_to_params(session_id, &msg));
                        let _ = send_session_notification(notif, &notifier).await;
                    }
                }
                let notifier_clone = notifier.clone();
                let session = session_id.to_string();
                tokio::spawn(async move {
                    while let Ok(msg) = rx.recv().await {
                        if matches!(
                            msg,
                            TaskManagerMessage::PtyData(_)
                                | TaskManagerMessage::PtyResize(_)
                                | TaskManagerMessage::CommandChunk(_)
                        ) {
                            let notif =
                                session_update_from_json(&terminal_to_params(&session, &msg));
                            let _ = send_session_notification(notif, &notifier_clone).await;
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
    _driver: &Arc<Mutex<ValueDispatcher<HarborAgentSide, OutgoingSide>>>,
    sender: &Arc<Mutex<SplitSink<WebSocket, WsMessage>>>,
    params: Value,
    notifier: &Notifier,
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
    let _ = send_raw_session_update_ws(sender, update.clone()).await;
    let _ = send_session_notification(session_update_from_json(&update), notifier).await;

    Ok(json!({ "sessionId": session_id, "executionId": execution_id, "detached": true }))
}

async fn handle_terminal_detach_stdio(
    _driver: &Arc<Mutex<ValueDispatcher<HarborAgentSide, OutgoingSide>>>,
    writer: &AnyWriter,
    params: Value,
    notifier: &Notifier,
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
    let _ = send_raw_session_update(writer, update.clone()).await;
    let _ = send_session_notification(session_update_from_json(&update), notifier).await;

    Ok(json!({ "sessionId": session_id, "executionId": execution_id, "detached": true }))
}

async fn handle_session_list(
    state: &AcpTransportState,
    params: Value,
    ctx: &AcpSessionContext,
) -> ServerResult<Value> {
    let offset = params.get("offset").and_then(|v| v.as_u64()).unwrap_or(0);
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
    params: Value,
) -> ServerResult<Value> {
    let session_id = params
        .get("sessionId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ServerError::BadRequest("sessionId is required".into()))?;

    if let Some(session) = state.app_state.session_store.get_session(session_id).await? {
        subscribe_session(ctx, &state.app_state, session_id).await;
        let _ = seed_history(&state.app_state, session_id, &ctx.notifier).await;
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
    _driver: &Arc<Mutex<ValueDispatcher<HarborAgentSide, OutgoingSide>>>,
    _writer: &AnyWriter,
    params: Value,
) -> ServerResult<Value> {
    let session_id = params
        .get("sessionId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ServerError::BadRequest("sessionId is required".into()))?;

    if let Some(session) = state.app_state.session_store.get_session(session_id).await? {
        subscribe_session_stdio(ctx, &state.app_state, session_id).await;
        let _ = seed_history_stdio(&state.app_state, session_id, &ctx.notifier).await;
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
    _driver: &Arc<Mutex<ValueDispatcher<HarborAgentSide, OutgoingSide>>>,
    _sender: &Arc<Mutex<SplitSink<WebSocket, WsMessage>>>,
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

    subscribe_session(ctx, &state.app_state, session_id).await;
    let notifier = ctx.notifier.clone();
    let _ = seed_history(&state.app_state, session_id, &notifier).await;
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

    let controller =
        state.app_state.task_controller.as_ref().ok_or_else(|| {
            ServerError::NotImplemented("live prompt delivery unavailable".into())
        })?;
    controller
        .inject_message(session_id, message)
        .await
        .map_err(|e| ServerError::Internal(format!("failed to deliver prompt: {e}")))?;

    Ok(json!({
        "sessionId": session_id,
        "accepted": true
    }))
}

async fn handle_session_prompt_stdio(
    state: &AcpTransportState,
    ctx: &mut AcpSessionContext,
    _driver: &Arc<Mutex<ValueDispatcher<HarborAgentSide, OutgoingSide>>>,
    _writer: &AnyWriter,
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

    subscribe_session_stdio(ctx, &state.app_state, session_id).await;
    let notifier = ctx.notifier.clone();
    let _ = seed_history_stdio(&state.app_state, session_id, &notifier).await;
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

    let controller =
        state.app_state.task_controller.as_ref().ok_or_else(|| {
            ServerError::NotImplemented("live prompt delivery unavailable".into())
        })?;
    controller
        .inject_message(session_id, message)
        .await
        .map_err(|e| ServerError::Internal(format!("failed to deliver prompt: {e}")))?;

    Ok(json!({
        "sessionId": session_id,
        "accepted": true
    }))
}

async fn handle_session_cancel(
    state: &AcpTransportState,
    ctx: &mut AcpSessionContext,
    _driver: &Arc<Mutex<ValueDispatcher<HarborAgentSide, OutgoingSide>>>,
    _sender: &Arc<Mutex<SplitSink<WebSocket, WsMessage>>>,
    params: Value,
) -> ServerResult<Value> {
    let session_id = params
        .get("sessionId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ServerError::BadRequest("sessionId is required".into()))?;

    let Some(mut session) = state.app_state.session_store.get_session(session_id).await? else {
        return Err(ServerError::SessionNotFound(session_id.to_string()));
    };

    subscribe_session(ctx, &state.app_state, session_id).await;
    let notifier = ctx.notifier.clone();
    let _ = seed_history(&state.app_state, session_id, &notifier).await;
    ctx.sessions.insert(session_id.to_string());

    let controller = state
        .app_state
        .task_controller
        .as_ref()
        .ok_or_else(|| ServerError::NotImplemented("task controller unavailable".into()))?;
    controller
        .stop_task(session_id)
        .await
        .map_err(|e| ServerError::Internal(format!("failed to stop task: {e}")))?;

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
    _driver: &Arc<Mutex<ValueDispatcher<HarborAgentSide, OutgoingSide>>>,
    _writer: &AnyWriter,
    params: Value,
) -> ServerResult<Value> {
    let session_id = params
        .get("sessionId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ServerError::BadRequest("sessionId is required".into()))?;

    let Some(mut session) = state.app_state.session_store.get_session(session_id).await? else {
        return Err(ServerError::SessionNotFound(session_id.to_string()));
    };

    subscribe_session_stdio(ctx, &state.app_state, session_id).await;
    let notifier = ctx.notifier.clone();
    let _ = seed_history_stdio(&state.app_state, session_id, &notifier).await;
    ctx.sessions.insert(session_id.to_string());

    let controller = state
        .app_state
        .task_controller
        .as_ref()
        .ok_or_else(|| ServerError::NotImplemented("task controller unavailable".into()))?;
    controller
        .stop_task(session_id)
        .await
        .map_err(|e| ServerError::Internal(format!("failed to stop task: {e}")))?;

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
    _driver: &Arc<Mutex<ValueDispatcher<HarborAgentSide, OutgoingSide>>>,
    _sender: &Arc<Mutex<SplitSink<WebSocket, WsMessage>>>,
    params: Value,
) -> ServerResult<Value> {
    let session_id = params
        .get("sessionId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ServerError::BadRequest("sessionId is required".into()))?;

    let Some(mut session) = state.app_state.session_store.get_session(session_id).await? else {
        return Err(ServerError::SessionNotFound(session_id.to_string()));
    };

    subscribe_session(ctx, &state.app_state, session_id).await;
    let notifier = ctx.notifier.clone();
    let _ = seed_history(&state.app_state, session_id, &notifier).await;
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

    let controller = state
        .app_state
        .task_controller
        .as_ref()
        .ok_or_else(|| ServerError::NotImplemented("task controller unavailable".into()))?;
    controller
        .pause_task(session_id)
        .await
        .map_err(|e| ServerError::Internal(format!("failed to pause task: {e}")))?;

    Ok(json!({
        "sessionId": session_id,
        "status": "paused"
    }))
}

async fn handle_session_pause_stdio(
    state: &AcpTransportState,
    ctx: &mut AcpSessionContext,
    _driver: &Arc<Mutex<ValueDispatcher<HarborAgentSide, OutgoingSide>>>,
    _writer: &AnyWriter,
    params: Value,
) -> ServerResult<Value> {
    let session_id = params
        .get("sessionId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ServerError::BadRequest("sessionId is required".into()))?;

    let Some(mut session) = state.app_state.session_store.get_session(session_id).await? else {
        return Err(ServerError::SessionNotFound(session_id.to_string()));
    };

    subscribe_session_stdio(ctx, &state.app_state, session_id).await;
    let notifier = ctx.notifier.clone();
    let _ = seed_history_stdio(&state.app_state, session_id, &notifier).await;
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

    let controller = state
        .app_state
        .task_controller
        .as_ref()
        .ok_or_else(|| ServerError::NotImplemented("task controller unavailable".into()))?;
    controller
        .pause_task(session_id)
        .await
        .map_err(|e| ServerError::Internal(format!("failed to pause task: {e}")))?;

    Ok(json!({
        "sessionId": session_id,
        "status": "paused"
    }))
}

async fn handle_session_resume(
    state: &AcpTransportState,
    ctx: &mut AcpSessionContext,
    _driver: &Arc<Mutex<ValueDispatcher<HarborAgentSide, OutgoingSide>>>,
    _sender: &Arc<Mutex<SplitSink<WebSocket, WsMessage>>>,
    params: Value,
) -> ServerResult<Value> {
    let session_id = params
        .get("sessionId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ServerError::BadRequest("sessionId is required".into()))?;

    let Some(mut session) = state.app_state.session_store.get_session(session_id).await? else {
        return Err(ServerError::SessionNotFound(session_id.to_string()));
    };

    subscribe_session(ctx, &state.app_state, session_id).await;
    let notifier = ctx.notifier.clone();
    let _ = seed_history(&state.app_state, session_id, &notifier).await;
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

    let controller = state
        .app_state
        .task_controller
        .as_ref()
        .ok_or_else(|| ServerError::NotImplemented("task controller unavailable".into()))?;
    controller
        .resume_task(session_id)
        .await
        .map_err(|e| ServerError::Internal(format!("failed to resume task: {e}")))?;

    Ok(json!({
        "sessionId": session_id,
        "status": "running"
    }))
}

async fn handle_session_resume_stdio(
    state: &AcpTransportState,
    ctx: &mut AcpSessionContext,
    _driver: &Arc<Mutex<ValueDispatcher<HarborAgentSide, OutgoingSide>>>,
    _writer: &AnyWriter,
    params: Value,
) -> ServerResult<Value> {
    let session_id = params
        .get("sessionId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ServerError::BadRequest("sessionId is required".into()))?;

    let Some(mut session) = state.app_state.session_store.get_session(session_id).await? else {
        return Err(ServerError::SessionNotFound(session_id.to_string()));
    };

    subscribe_session_stdio(ctx, &state.app_state, session_id).await;
    let notifier = ctx.notifier.clone();
    let _ = seed_history_stdio(&state.app_state, session_id, &notifier).await;
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

    let controller = state
        .app_state
        .task_controller
        .as_ref()
        .ok_or_else(|| ServerError::NotImplemented("task controller unavailable".into()))?;
    controller
        .resume_task(session_id)
        .await
        .map_err(|e| ServerError::Internal(format!("failed to resume task: {e}")))?;

    Ok(json!({
        "sessionId": session_id,
        "status": "running"
    }))
}

async fn subscribe_session(ctx: &mut AcpSessionContext, app_state: &AppState, session_id: &str) {
    if !ctx.receivers.contains_key(session_id) {
        if let Some(receiver) = app_state.session_store.subscribe_session_events(session_id) {
            ctx.receivers.insert(session_id.to_string(), receiver);
        }
    }
    if !ctx.pty_receivers.contains_key(session_id) {
        if let Some(controller) = &app_state.task_controller {
            if let Ok((backlog, rx)) = controller.subscribe_pty(session_id).await {
                for msg in backlog {
                    let notif = session_update_from_json(&terminal_to_params(session_id, &msg));
                    let _ = send_session_notification(notif, &ctx.notifier).await;
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
                    let notif = session_update_from_json(&terminal_to_params(session_id, &msg));
                    let _ = send_session_notification(notif, &ctx.notifier).await;
                }
                ctx.pty_receivers.insert(session_id.to_string(), rx);
            }
        }
    }
}

async fn seed_history(
    app_state: &AppState,
    session_id: &str,
    notifier: &Notifier,
) -> Result<(), ()> {
    if let Ok(events) = app_state.session_store.get_session_events(session_id).await {
        for event in events {
            let notif = session_event_to_notification(session_id, &event);
            send_session_notification(notif, notifier).await?;
        }
    }
    Ok(())
}

async fn seed_history_stdio(
    app_state: &AppState,
    session_id: &str,
    notifier: &Notifier,
) -> Result<(), ()> {
    if let Ok(events) = app_state.session_store.get_session_events(session_id).await {
        for event in events {
            let notif = session_event_to_notification(session_id, &event);
            send_session_notification(notif, notifier).await?;
        }
    }
    Ok(())
}

fn session_to_json(session: &Session) -> Value {
    let read_only = matches!(session.status, SessionStatus::Paused);
    json!({
        "id": session.id,
        "tenantId": session.tenant_id,
        "projectId": session.project_id,
        "status": session.status.to_string(),
        "prompt": session.task.prompt,
        "workspace": session.workspace.mount_path,
        "workspaceReadOnly": read_only,
        "snapshotProvider": session.workspace.snapshot_provider,
        "agent": session.agent.display_name.clone().unwrap_or_else(|| session.agent.model.clone()),
        "links": {
            "events": session.links.events,
            "logs": session.links.logs
        }
    })
}

async fn flush_session_events(ctx: &mut AcpSessionContext, notifier: &Notifier) -> bool {
    let mut sent = false;
    let mut dead_sessions = Vec::new();
    let ids: Vec<String> = ctx.receivers.keys().cloned().collect();
    let mut buffered: Vec<(String, SessionEvent)> = Vec::new();

    for session_id in &ids {
        if let Some(receiver) = ctx.receivers.get_mut(session_id) {
            loop {
                match receiver.try_recv() {
                    Ok(event) => buffered.push((session_id.clone(), event)),
                    Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
                    Err(tokio::sync::broadcast::error::TryRecvError::Closed) => {
                        dead_sessions.push(session_id.clone());
                        break;
                    }
                    Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => break,
                }
            }
        }
    }

    for (session_id, event) in buffered {
        cache_execution_command(&mut ctx.execution_commands, &session_id, &event);
        let update = session_event_to_notification(&session_id, &event);
        if send_session_notification(update, notifier).await.is_err() {
            dead_sessions.push(session_id.clone());
        } else {
            sent = true;
        }

        // Proactively tell clients to launch follower terminals when a tool starts.
        if let SessionEvent::ToolUse(ev) = &event {
            if ev.status == SessionToolStatus::Started {
                if let Some(cmd) = tool_event_command(&ev.tool_name, &ev.tool_args) {
                    let exec_id = String::from_utf8_lossy(&ev.tool_execution_id).to_string();
                    let follow_cmd = follower_command(&exec_id, &session_id, &cmd);
                    let follow_update = terminal_follow_params(&session_id, &exec_id, &follow_cmd);
                    let _ = send_session_notification(
                        session_update_from_json(&follow_update),
                        notifier,
                    )
                    .await;
                }
            }
        }
    }

    for session_id in dead_sessions {
        ctx.receivers.remove(&session_id);
    }

    sent
}

async fn flush_session_events_stdio(ctx: &mut AcpSessionContext, notifier: &Notifier) -> bool {
    let mut sent = false;
    let mut dead_sessions = Vec::new();
    let ids: Vec<String> = ctx.receivers.keys().cloned().collect();
    let mut buffered: Vec<(String, SessionEvent)> = Vec::new();

    for session_id in &ids {
        if let Some(receiver) = ctx.receivers.get_mut(session_id) {
            loop {
                match receiver.try_recv() {
                    Ok(event) => buffered.push((session_id.clone(), event)),
                    Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
                    Err(tokio::sync::broadcast::error::TryRecvError::Closed) => {
                        dead_sessions.push(session_id.clone());
                        break;
                    }
                    Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => break,
                }
            }
        }
    }

    for (session_id, event) in buffered {
        cache_execution_command(&mut ctx.execution_commands, &session_id, &event);
        let update = session_event_to_notification(&session_id, &event);
        if send_session_notification(update, notifier).await.is_err() {
            dead_sessions.push(session_id.clone());
        } else {
            sent = true;
        }

        if let SessionEvent::ToolUse(ev) = &event {
            if ev.status == SessionToolStatus::Started {
                if let Some(cmd) = tool_event_command(&ev.tool_name, &ev.tool_args) {
                    let exec_id = String::from_utf8_lossy(&ev.tool_execution_id).to_string();
                    let follow_cmd = follower_command(&exec_id, &session_id, &cmd);
                    let follow_update = terminal_follow_params(&session_id, &exec_id, &follow_cmd);
                    let _ = send_session_notification(
                        session_update_from_json(&follow_update),
                        notifier,
                    )
                    .await;
                }
            }
        }
    }

    for session_id in dead_sessions {
        ctx.receivers.remove(&session_id);
    }

    sent
}

async fn flush_pty_events(ctx: &mut AcpSessionContext, notifier: &Notifier) -> bool {
    let mut sent = false;
    let mut dead = Vec::new();

    for (session_id, rx) in ctx.pty_receivers.iter_mut() {
        loop {
            match rx.try_recv() {
                Ok(msg) => {
                    let update = session_update_from_json(&terminal_to_params(session_id, &msg));
                    if send_session_notification(update, notifier).await.is_err() {
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

async fn flush_pty_events_stdio(ctx: &mut AcpSessionContext, notifier: &Notifier) -> bool {
    let mut sent = false;
    let mut dead = Vec::new();

    for (session_id, rx) in ctx.pty_receivers.iter_mut() {
        loop {
            match rx.try_recv() {
                Ok(msg) => {
                    let update = session_update_from_json(&terminal_to_params(session_id, &msg));
                    if send_session_notification(update, notifier).await.is_err() {
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

fn cache_execution_command(
    commands: &mut HashMap<String, HashMap<String, String>>,
    session_id: &str,
    event: &SessionEvent,
) {
    match event {
        SessionEvent::ToolUse(ev) => {
            if let Some(cmd) = tool_event_command(&ev.tool_name, &ev.tool_args) {
                commands.entry(session_id.to_string()).or_default().insert(
                    String::from_utf8_lossy(&ev.tool_execution_id).to_string(),
                    cmd,
                );
            }
        }
        SessionEvent::ToolResult(ev) => {
            if let Some(cmd) = tool_event_command(&ev.tool_name, &[]) {
                commands.entry(session_id.to_string()).or_default().insert(
                    String::from_utf8_lossy(&ev.tool_execution_id).to_string(),
                    cmd,
                );
            }
        }
        _ => {}
    }
}

fn terminal_to_params(session_id: &str, msg: &TaskManagerMessage) -> Value {
    match msg {
        TaskManagerMessage::PtyData(bytes) => json!({
            "sessionId": session_id,
            "event": {
                "type": "terminal",
                "encoding": "base64",
                "data": B64.encode(bytes),
                "stream": "stdout",
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
        TaskManagerMessage::CommandChunk(chunk) => json!({
            "sessionId": session_id,
            "event": {
                "type": "terminal",
                "encoding": "base64",
                "executionId": String::from_utf8_lossy(&chunk.execution_id),
                "stream": if chunk.stream == 1 { "stderr" } else { "stdout" },
                "data": B64.encode(&chunk.data),
                "timestamp": chrono::Utc::now().timestamp_millis(),
            }
        }),
        _ => json!({}),
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

fn session_update_from_json(params: &Value) -> SessionNotification {
    value_to_session_notification(params).unwrap_or_else(|| SessionNotification {
        session_id: SessionId(
            params.get("sessionId").and_then(|v| v.as_str()).unwrap_or_default().into(),
        ),
        update: SessionUpdate::AgentMessageChunk {
            content: ContentBlock::Text(TextContent {
                annotations: None,
                text: params.to_string(),
                meta: None,
            }),
        },
        meta: params.get("event").cloned(),
    })
}

async fn send_json(
    sender: &Arc<Mutex<SplitSink<WebSocket, WsMessage>>>,
    payload: Value,
) -> Result<(), ()> {
    let text = serde_json::to_string(&payload).map_err(|_| ())?;
    let mut guard = sender.lock().await;
    guard.send(WsMessage::Text(text)).await.map_err(|_| ())
}

async fn send_raw_session_update_ws(
    sender: &Arc<Mutex<SplitSink<WebSocket, WsMessage>>>,
    params: Value,
) -> Result<(), ()> {
    send_json(
        sender,
        json!({
            "jsonrpc": "2.0",
            "method": "session/update",
            "params": params,
        }),
    )
    .await
}

async fn send_outgoing(
    driver: &Arc<Mutex<ValueDispatcher<HarborAgentSide, OutgoingSide>>>,
    sender: &Arc<Mutex<SplitSink<WebSocket, WsMessage>>>,
    message: &OutgoingMessage<HarborAgentSide, OutgoingSide>,
) -> Result<(), ()> {
    let payload = {
        let dispatcher = driver.lock().await;
        dispatcher.outgoing_to_value(message).map_err(|_| ())?
    };
    send_json(sender, payload).await
}

async fn send_session_notification(
    notification: SessionNotification,
    notifier: &Notifier,
) -> Result<(), ()> {
    notifier.push_raw(notification_envelope(&notification));
    notifier
        .notify(
            "sessionUpdate",
            Some(AgentNotification::SessionNotification(notification)),
        )
        .await
}

async fn send_json_lines<W>(writer: &Arc<Mutex<W>>, payload: Value) -> Result<(), ()>
where
    W: AsyncWrite + Unpin + Send,
{
    let mut guard = writer.lock().await;
    let mut buf = serde_json::to_vec(&payload).map_err(|_| ())?;
    buf.push(b'\n');
    guard.write_all(&buf).await.map_err(|_| ())?;
    guard.flush().await.map_err(|_| ())
}

async fn send_raw_session_update<W>(writer: &Arc<Mutex<W>>, params: Value) -> Result<(), ()>
where
    W: AsyncWrite + Unpin + Send,
{
    send_json_lines(
        writer,
        json!({
            "jsonrpc": "2.0",
            "method": "session/update",
            "params": params,
        }),
    )
    .await
}

async fn send_outgoing_lines<W>(
    driver: &Arc<Mutex<ValueDispatcher<HarborAgentSide, OutgoingSide>>>,
    writer: &Arc<Mutex<W>>,
    message: &OutgoingMessage<HarborAgentSide, OutgoingSide>,
) -> Result<(), ()>
where
    W: AsyncWrite + Unpin + Send,
{
    let payload = {
        let dispatcher = driver.lock().await;
        dispatcher.outgoing_to_value(message).map_err(|_| ())?
    };
    send_json_lines(writer, payload).await
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
    let meta = params.get("_meta").cloned().unwrap_or(Value::Null);

    // Prompt/agent live at the root; accept _meta as a backwards-compatible fallback
    // but prefer the top-level fields to avoid duplication.
    let prompt = params
        .get("prompt")
        .or_else(|| meta.get("prompt"))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
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
    let agent_model = params
        .get("agent")
        .or_else(|| meta.get("agent"))
        .and_then(|v| v.as_str())
        .unwrap_or("sonnet")
        .to_string();

    // Harbor-specific session options live under _meta; keep root fallback for
    // backwards compatibility during the migration.
    let repo_url = meta
        .get("repoUrl")
        .or_else(|| params.get("repoUrl"))
        .and_then(|v| v.as_str())
        .and_then(|s| Url::parse(s).ok());
    let repo = RepoConfig {
        mode: if repo_url.is_some() {
            RepoMode::Git
        } else {
            RepoMode::None
        },
        url: repo_url,
        branch: meta
            .get("branch")
            .or_else(|| params.get("branch"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        commit: None,
    };

    let agent = AgentChoice {
        agent: AgentSoftwareBuild {
            software: AgentSoftware::Claude,
            version: "latest".into(),
        },
        model: agent_model.clone(),
        count: 1,
        settings: std::collections::HashMap::new(),
        display_name: Some(agent_model),
        acp_stdio_launch_command: None,
    };

    let runtime = RuntimeConfig {
        runtime_type: RuntimeType::Local,
        devcontainer_path: None,
        resources: None,
    };

    let labels = meta
        .get("labels")
        .or_else(|| params.get("labels"))
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

#[cfg(test)]
mod tests {
    use super::*;
    use ah_core::task_manager_wire::CommandChunkPayload;

    #[test]
    fn terminal_update_encodes_and_resizes() {
        let data = TaskManagerMessage::PtyData(b"hi".to_vec());
        let json = terminal_to_params("sess", &data);
        assert_eq!(
            json.pointer("/event/type").and_then(|v| v.as_str()),
            Some("terminal")
        );
        assert_eq!(
            json.pointer("/event/data").and_then(|v| v.as_str()),
            Some(B64.encode("hi").as_str())
        );

        let resize = TaskManagerMessage::PtyResize((120, 33));
        let json = terminal_to_params("sess", &resize);
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

    #[test]
    fn command_from_tool_use_prefers_args() {
        let events = vec![SessionEvent::tool_use(
            "bash".into(),
            "npm test".into(),
            "exec-42".into(),
            SessionToolStatus::Started,
            1,
        )];
        let cmd = command_from_events(&events, "exec-42");
        assert_eq!(cmd.as_deref(), Some("npm test"));
    }

    #[test]
    fn command_from_tool_use_falls_back_to_name() {
        let events = vec![SessionEvent::tool_use(
            "cargo fmt".into(),
            "".into(),
            "exec-99".into(),
            SessionToolStatus::Started,
            1,
        )];
        let cmd = command_from_events(&events, "exec-99");
        assert_eq!(cmd.as_deref(), Some("cargo fmt"));
    }

    #[test]
    fn sanitize_command_rejects_newlines_and_empty() {
        assert!(sanitize_command("echo hi").is_some());
        assert!(sanitize_command("   ").is_none());
        assert!(sanitize_command("evil\ncmd").is_none());
        assert!(sanitize_command("evil\rcmd").is_none());
    }

    #[test]
    fn session_update_maps_status() {
        let params = json!({
            "sessionId": "sess",
            "event": { "type": "status", "status": "running" }
        });
        let notif = session_update_from_json(&params);
        assert_eq!(notif.session_id.0.as_ref(), "sess");
        match notif.update {
            SessionUpdate::AgentMessageChunk {
                content: ContentBlock::Text(t),
            } => {
                assert_eq!(t.text, "running")
            }
            _ => panic!("expected AgentMessageChunk for status"),
        }
        assert!(notif.meta.is_some());
    }

    #[test]
    fn session_update_maps_log_and_thought() {
        let log = session_update_from_json(&json!({
            "sessionId": "s1",
            "event": { "type": "log", "message": "hello" }
        }));
        match log.update {
            SessionUpdate::AgentMessageChunk {
                content: ContentBlock::Text(t),
            } => {
                assert_eq!(t.text, "hello")
            }
            _ => panic!("expected log to map to AgentMessageChunk"),
        }

        let thought = session_update_from_json(&json!({
            "sessionId": "s1",
            "event": { "type": "thought", "text": "reason" }
        }));
        match thought.update {
            SessionUpdate::AgentThoughtChunk {
                content: ContentBlock::Text(t),
            } => {
                assert_eq!(t.text, "reason")
            }
            _ => panic!("expected thought to map to AgentThoughtChunk"),
        }
    }

    #[test]
    fn session_update_maps_tool_calls() {
        let call = session_update_from_json(&json!({
            "sessionId": "s1",
            "event": {
                "type": "tool_use",
                "executionId": "exec-1",
                "toolName": "echo",
                "status": "running",
                "args": "\"hi\""
            }
        }));
        match call.update {
            SessionUpdate::ToolCall(call) => {
                assert_eq!(call.id.0.as_ref(), "exec-1");
                assert_eq!(call.title, "echo");
                assert_eq!(
                    call.status,
                    agent_client_protocol::ToolCallStatus::InProgress
                );
                assert_eq!(call.raw_input, Some(json!("\"hi\"")));
            }
            _ => panic!("expected ToolCall"),
        }

        let result = session_update_from_json(&json!({
            "sessionId": "s1",
            "event": {
                "type": "tool_result",
                "executionId": "exec-1",
                "status": "completed",
                "output": "{\"ok\":true}"
            }
        }));
        match result.update {
            SessionUpdate::ToolCallUpdate(update) => {
                assert_eq!(update.id.0.as_ref(), "exec-1");
                assert_eq!(
                    update.fields.status,
                    Some(agent_client_protocol::ToolCallStatus::Completed)
                );
                assert_eq!(update.fields.raw_output, Some(json!("{\"ok\":true}")));
            }
            _ => panic!("expected ToolCallUpdate"),
        }
    }

    #[test]
    fn session_update_maps_terminal() {
        let follow = session_update_from_json(&json!({
            "sessionId": "s1",
            "event": { "type": "terminal_follow", "executionId": "exec" }
        }));
        match follow.update {
            SessionUpdate::AgentMessageChunk {
                content: ContentBlock::Text(t),
            } => {
                assert!(t.text.contains("terminal"))
            }
            _ => panic!("expected terminal follow to map to AgentMessageChunk"),
        }
    }

    #[tokio::test]
    async fn notifier_emits_raw_session_update() {
        let notifier = Notifier::new_threaded();
        let mut rx = notifier.subscribe().expect("subscription");

        let params = json!({
            "sessionId": "s1",
            "event": { "type": "terminal_follow", "executionId": "exec", "command": "cmd" }
        });

        let notif = session_update_from_json(&params);
        send_session_notification(notif, &notifier).await.unwrap();

        let payload = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
            .await
            .expect("payload")
            .expect("value");

        assert_eq!(
            payload.get("method").and_then(|v| v.as_str()),
            Some("session/update")
        );
        assert_eq!(
            payload.pointer("/params/_meta/type").and_then(|v| v.as_str()),
            Some("terminal_follow")
        );
    }

    #[test]
    fn command_chunk_serializes_stream_and_execution() {
        let chunk = TaskManagerMessage::CommandChunk(CommandChunkPayload {
            execution_id: b"exec-1".to_vec(),
            stream: 1,
            data: b"err".to_vec(),
        });
        let json = terminal_to_params("sess", &chunk);
        assert_eq!(
            json.pointer("/event/stream").and_then(|v| v.as_str()),
            Some("stderr")
        );
        assert_eq!(
            json.pointer("/event/executionId").and_then(|v| v.as_str()),
            Some("exec-1")
        );
        assert!(json.pointer("/event/data").and_then(|v| v.as_str()).is_some());
    }
}
