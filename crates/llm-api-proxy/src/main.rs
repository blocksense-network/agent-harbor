// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! LLM API Proxy - Library usage and test server

use axum::extract::Path;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::{
    Router,
    routing::{get, post},
};
use clap::{Parser, Subcommand};
use futures::stream;
use llm_api_proxy::proxy::ProxyResponse;
use llm_api_proxy::proxy::{ModelMapping, ProviderDefinition};
use llm_api_proxy::{LlmApiProxy, ProxyConfig};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;
use tower_http::cors::CorsLayer;
use uuid::Uuid;

#[derive(Parser)]
#[command(name = "llm-api-proxy")]
#[command(about = "LLM API Proxy for testing and development")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a proxy server that forwards requests to real LLM APIs
    Proxy {
        /// Port to bind the proxy server to
        #[arg(long, default_value = "18081")]
        port: u16,

        /// Target provider to proxy to (auto-detected from agent-type if not specified)
        #[arg(long)]
        provider: Option<String>,

        /// API key for the target provider
        #[arg(long)]
        api_key: Option<String>,

        /// Path to log request details
        #[arg(long)]
        request_log: Option<PathBuf>,

        /// Include request headers in logs
        #[arg(long)]
        log_headers: bool,

        /// Include request body in logs
        #[arg(long)]
        log_body: bool,

        /// Include responses in logs
        #[arg(long)]
        log_responses: bool,

        /// Minimize JSON logs (default: false, pretty-print by default)
        #[arg(long)]
        minimize_logs: bool,
    },
    /// Run a test server for integration testing
    TestServer {
        /// Port to bind the test server to
        #[arg(long, default_value = "18081")]
        port: u16,

        /// Scenario file to use for mock responses
        #[arg(long)]
        scenario_file: PathBuf,

        /// Agent type (affects API format expectations)
        #[arg(long, default_value = "codex")]
        agent_type: String,

        /// Agent version string
        #[arg(long, default_value = "unknown")]
        agent_version: String,

        /// Enable strict tools validation
        #[arg(long)]
        strict_tools_validation: bool,

        /// Path to log request details
        #[arg(long)]
        request_log: Option<PathBuf>,

        /// Include request headers in logs
        #[arg(long)]
        log_headers: bool,

        /// Include request body in logs
        #[arg(long)]
        log_body: bool,

        /// Include responses in logs
        #[arg(long)]
        log_responses: bool,

        /// Minimize JSON logs (default: false, pretty-print by default)
        #[arg(long)]
        minimize_logs: bool,

        /// Symbols for scenario rule evaluation (KEY=VAL). Repeatable.
        #[arg(long = "scenario-define", value_name = "KEY=VAL", action = clap::ArgAction::Append)]
        scenario_defines: Vec<String>,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Proxy {
            port,
            provider,
            api_key,
            request_log,
            log_headers,
            log_body,
            log_responses,
            minimize_logs,
        } => {
            run_proxy_server(
                port,
                provider,
                api_key,
                request_log,
                log_headers,
                log_body,
                log_responses,
                minimize_logs,
            )
            .await?;
        }
        Commands::TestServer {
            port,
            scenario_file,
            agent_type,
            agent_version,
            strict_tools_validation,
            request_log,
            log_headers,
            log_body,
            log_responses,
            minimize_logs,
            scenario_defines,
        } => {
            run_test_server(
                port,
                scenario_file,
                agent_type,
                agent_version,
                strict_tools_validation,
                request_log,
                log_headers,
                log_body,
                log_responses,
                minimize_logs,
                scenario_defines,
            )
            .await?;
        }
    }

    Ok(())
}

/// Run as a proxy server for forwarding to real LLM APIs
#[allow(clippy::too_many_arguments)] // Function mirrors CLI shape for clarity
async fn run_proxy_server(
    port: u16,
    provider: Option<String>,
    api_key: Option<String>,
    request_log_path: Option<PathBuf>,
    log_headers: bool,
    log_body: bool,
    log_responses: bool,
    minimize_logs: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    tracing::info!("Starting LLM API Proxy server on port {}", port);
    tracing::info!("Mode: Live proxy to real LLM APIs");
    tracing::info!("Target provider: {:?}", provider);

    // Check if API key is configured (either from command line or environment)
    let provider_name = provider.as_deref().unwrap_or("openai");
    let api_key_configured = api_key.is_some()
        || match provider_name {
            "anthropic" => std::env::var("ANTHROPIC_API_KEY").is_ok(),
            "openai" => std::env::var("OPENAI_API_KEY").is_ok(),
            "openrouter" => std::env::var("OPENROUTER_API_KEY").is_ok(),
            _ => false,
        };
    tracing::info!("API key configured: {}", api_key_configured);

    tracing::info!("Request log path: {:?}", request_log_path);
    tracing::info!("Log headers: {}", log_headers);
    tracing::info!("Log body: {}", log_body);
    tracing::info!("Log responses: {}", log_responses);

    // Set logging environment variables
    if let Some(log_path) = &request_log_path {
        std::env::set_var("REQUEST_LOG_TEMPLATE", log_path);
        tracing::info!("Set REQUEST_LOG_TEMPLATE={}", log_path.display());
    }

    // Set logging policy flags
    std::env::set_var(
        "LLM_API_PROXY_LOG_HEADERS",
        if log_headers { "true" } else { "false" },
    );
    std::env::set_var(
        "LLM_API_PROXY_LOG_BODY",
        if log_body { "true" } else { "false" },
    );
    std::env::set_var(
        "LLM_API_PROXY_LOG_RESPONSES",
        if log_responses { "true" } else { "false" },
    );

    // Create proxy configuration for live proxying
    let mut config = ProxyConfig::default();
    config.server.port = port;
    config.scenario.enabled = false; // Disable scenario mode
    config.routing.default_provider = provider.unwrap_or_else(|| "openai".to_string());
    config.scenario.minimize_logs = minimize_logs;

    // Override API key if provided via command line
    if let Some(api_key) = api_key {
        if let Some(provider_config) = config.providers.get_mut(&config.routing.default_provider) {
            provider_config.api_key = Some(api_key);
        }
    }

    // Create the proxy
    tracing::info!("Creating proxy in live mode");
    let proxy = match LlmApiProxy::new(config).await {
        Ok(p) => {
            tracing::info!("Proxy created successfully");
            p
        }
        Err(e) => {
            tracing::error!("Failed to create proxy: {}", e);
            return Err(e.into());
        }
    };
    let proxy = std::sync::Arc::new(tokio::sync::RwLock::new(proxy));

    // Create Axum router for HTTP endpoints (same as test server)
    let proxy_for_chat = proxy.clone();
    let proxy_for_responses = proxy.clone();
    let proxy_for_responses_stream = proxy.clone();
    let proxy_for_messages = proxy.clone();
    let proxy_for_prepare_session = proxy.clone();
    let proxy_for_end_session = proxy.clone();
    let app = Router::new()
        .route(
            "/v1/chat/completions",
            post(move |body| handle_chat_completion(proxy_for_chat.clone(), body)),
        )
        .route(
            "/v1/responses",
            post(move |body| handle_openai_responses(proxy_for_responses.clone(), body)),
        )
        .route(
            "/v1/responses/{id}/events",
            get(move |path| {
                handle_openai_responses_stream(proxy_for_responses_stream.clone(), path)
            }),
        )
        .route(
            "/v1/messages",
            post(move |body| handle_anthropic_messages(proxy_for_messages.clone(), body)),
        )
        .route(
            "/prepare-session",
            post(move |body| handle_prepare_session(proxy_for_prepare_session.clone(), body)),
        )
        .route(
            "/end-session",
            post(move |body| handle_end_session(proxy_for_end_session.clone(), body)),
        )
        .route("/health", axum::routing::get(|| async { "OK" }))
        .fallback(|req: axum::http::Request<axum::body::Body>| async move {
            tracing::warn!("Unhandled request path: {}", req.uri());
            (axum::http::StatusCode::NOT_FOUND, "Not Found")
        })
        .layer(CorsLayer::permissive());

    // Bind to address
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    tracing::info!("Server listening on {}", addr);

    // Run the server
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Run as a test server for integration testing
#[allow(clippy::too_many_arguments)] // Function mirrors CLI shape for clarity
async fn run_test_server(
    port: u16,
    scenario_file: PathBuf,
    agent_type: String,
    agent_version: String,
    strict_tools_validation: bool,
    request_log_path: Option<PathBuf>,
    log_headers: bool,
    log_body: bool,
    log_responses: bool,
    minimize_logs: bool,
    scenario_defines: Vec<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    tracing::info!("Starting LLM API Proxy test server on port {}", port);
    tracing::info!("Scenario file: {}", scenario_file.display());
    tracing::info!("Scenario file exists: {}", scenario_file.exists());
    tracing::info!("Agent type: {}", agent_type);
    tracing::info!("Agent version: {}", agent_version);
    tracing::info!("Strict tools validation: {}", strict_tools_validation);
    tracing::info!("Request log path: {:?}", request_log_path);
    tracing::info!("Log headers: {}", log_headers);
    tracing::info!("Log body: {}", log_body);
    tracing::info!("Log responses: {}", log_responses);

    // Set logging environment variables
    if let Some(log_path) = &request_log_path {
        std::env::set_var("REQUEST_LOG_TEMPLATE", log_path);
        tracing::info!("Set REQUEST_LOG_TEMPLATE={}", log_path.display());
    }

    // Set logging policy flags
    std::env::set_var(
        "LLM_API_PROXY_LOG_HEADERS",
        if log_headers { "true" } else { "false" },
    );
    std::env::set_var(
        "LLM_API_PROXY_LOG_BODY",
        if log_body { "true" } else { "false" },
    );
    std::env::set_var(
        "LLM_API_PROXY_LOG_RESPONSES",
        if log_responses { "true" } else { "false" },
    );

    // Create proxy configuration for scenario playback
    let mut config = ProxyConfig::default();
    config.server.port = port;
    config.scenario.enabled = true;
    config.scenario.scenario_file = Some(scenario_file.to_string_lossy().to_string());
    config.scenario.agent_type = Some(agent_type.to_string());
    config.scenario.agent_version = Some(agent_version.to_string());
    config.scenario.strict_tools_validation = strict_tools_validation;
    config.scenario.minimize_logs = minimize_logs;
    if !scenario_defines.is_empty() {
        config.scenario.scenario_defines = Some(scenario_defines);
    }

    // Create the proxy
    tracing::info!(
        "Creating proxy with config: agent_type={:?}, agent_version={:?}, scenario_file={:?}",
        config.scenario.agent_type,
        config.scenario.agent_version,
        config.scenario.scenario_file
    );
    let proxy = match LlmApiProxy::new(config).await {
        Ok(p) => {
            tracing::info!("Proxy created successfully");
            p
        }
        Err(e) => {
            tracing::error!("Failed to create proxy: {}", e);
            return Err(e.into());
        }
    };
    let proxy = std::sync::Arc::new(tokio::sync::RwLock::new(proxy));

    // Create Axum router for HTTP endpoints
    let proxy_for_chat = proxy.clone();
    let proxy_for_responses = proxy.clone();
    let proxy_for_responses_stream = proxy.clone();
    let proxy_for_messages = proxy.clone();
    let app = Router::new()
        .route(
            "/v1/chat/completions",
            post(move |body| handle_chat_completion(proxy_for_chat.clone(), body)),
        )
        .route(
            "/v1/responses",
            post(move |body| handle_openai_responses(proxy_for_responses.clone(), body)),
        )
        .route(
            "/v1/responses/{id}/events",
            get(move |path| {
                handle_openai_responses_stream(proxy_for_responses_stream.clone(), path)
            }),
        )
        .route(
            "/v1/messages",
            post(move |body| handle_anthropic_messages(proxy_for_messages.clone(), body)),
        )
        .route("/health", axum::routing::get(|| async { "OK" }))
        .fallback(|req: axum::http::Request<axum::body::Body>| async move {
            tracing::warn!("Unhandled request path: {}", req.uri());
            (axum::http::StatusCode::NOT_FOUND, "Not Found")
        })
        .layer(CorsLayer::permissive());

    // Bind to address
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    tracing::info!("Server listening on {}", addr);

    // Run the server
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Convert ProxyResponse to appropriate Axum response type
fn proxy_response_to_axum_response(response: ProxyResponse) -> Response {
    use axum::http::{HeaderMap, StatusCode};
    use axum::response::IntoResponse;

    // Check if this is an SSE response
    if let Some(sse_data) = response.sse_data {
        use axum::body::Body;
        use axum::http::header;

        let mut headers = HeaderMap::new();
        for (key, value) in &response.headers {
            if let Ok(header_name) = header::HeaderName::from_bytes(key.as_bytes()) {
                if let Ok(header_value) = header::HeaderValue::from_str(value) {
                    headers.insert(header_name, header_value);
                }
            }
        }

        return Response::builder()
            .status(StatusCode::from_u16(response.status).unwrap_or(StatusCode::OK))
            .header(header::CONTENT_TYPE, "text/event-stream")
            .header(header::CACHE_CONTROL, "no-cache")
            .body(Body::from(sse_data))
            .unwrap();
    }

    // Regular JSON response
    axum::response::Json(response.payload).into_response()
}

/// Handle OpenAI chat completion requests
async fn handle_chat_completion(
    proxy: std::sync::Arc<tokio::sync::RwLock<LlmApiProxy>>,
    axum::extract::Json(body): axum::extract::Json<serde_json::Value>,
) -> Response {
    tracing::info!("Received OpenAI chat completion request");
    let proxy_guard = proxy.read().await;

    // Determine mode based on proxy configuration
    let mode = if proxy_guard.is_scenario_mode() {
        llm_api_proxy::proxy::ProxyMode::Scenario
    } else {
        llm_api_proxy::proxy::ProxyMode::Live
    };

    // Create a proxy request from the incoming body
    let streaming = body.get("stream").and_then(|s| s.as_bool()).unwrap_or(false);
    let proxy_request = llm_api_proxy::proxy::ProxyRequest {
        client_format: llm_api_proxy::converters::ApiFormat::OpenAI,
        mode,
        payload: body,
        headers: std::collections::HashMap::new(), // TODO: Extract headers from request
        request_id: format!("req-{}", uuid::Uuid::new_v4()),
        streaming,
    };

    // Process the request through the proxy
    match proxy_guard.proxy_request(proxy_request).await {
        Ok(response) => proxy_response_to_axum_response(response),
        Err(e) => {
            tracing::error!("Proxy error: {}", e);
            axum::response::Json(serde_json::json!({
                "error": {
                    "message": format!("Proxy error: {}", e),
                    "type": "internal_error"
                }
            }))
            .into_response()
        }
    }
}

/// Handle session preparation requests
async fn handle_prepare_session(
    proxy: std::sync::Arc<tokio::sync::RwLock<LlmApiProxy>>,
    axum::extract::Json(body): axum::extract::Json<serde_json::Value>,
) -> axum::response::Json<serde_json::Value> {
    tracing::info!("Received prepare-session request");

    // Extract required fields
    let api_key = match body.get("api_key").and_then(|v| v.as_str()) {
        Some(key) => key.to_string(),
        None => {
            return axum::response::Json(serde_json::json!({
                "error": {
                    "message": "Missing required field: api_key",
                    "type": "validation_error"
                }
            }));
        }
    };

    // Parse providers
    let providers: Vec<ProviderDefinition> = match body.get("providers") {
        Some(providers_value) => match serde_json::from_value(providers_value.clone()) {
            Ok(providers) => providers,
            Err(e) => {
                return axum::response::Json(serde_json::json!({
                    "error": {
                        "message": format!("Invalid providers: {}", e),
                        "type": "validation_error"
                    }
                }));
            }
        },
        None => {
            return axum::response::Json(serde_json::json!({
                "error": {
                    "message": "Missing required field: providers",
                    "type": "validation_error"
                }
            }));
        }
    };

    // Parse model mappings
    let model_mappings: Vec<ModelMapping> = match body.get("model_mappings") {
        Some(mappings_value) => match serde_json::from_value(mappings_value.clone()) {
            Ok(mappings) => mappings,
            Err(e) => {
                return axum::response::Json(serde_json::json!({
                    "error": {
                        "message": format!("Invalid model_mappings: {}", e),
                        "type": "validation_error"
                    }
                }));
            }
        },
        None => Vec::new(), // Optional field, defaults to empty
    };

    // Parse default provider
    let default_provider = match body.get("default_provider").and_then(|v| v.as_str()) {
        Some(provider) => provider.to_string(),
        None => {
            return axum::response::Json(serde_json::json!({
                "error": {
                    "message": "Missing required field: default_provider",
                    "type": "validation_error"
                }
            }));
        }
    };

    // Validate that all model mappings reference valid providers
    let provider_names: std::collections::HashSet<String> =
        providers.iter().map(|p| p.name.clone()).collect();
    for mapping in &model_mappings {
        if !provider_names.contains(&mapping.provider) {
            return axum::response::Json(serde_json::json!({
                "error": {
                    "message": format!("Model mapping references unknown provider '{}'", mapping.provider),
                    "type": "validation_error"
                }
            }));
        }
    }

    // Validate that default provider exists
    if !provider_names.contains(&default_provider) {
        return axum::response::Json(serde_json::json!({
            "error": {
                "message": format!("Default provider '{}' is not in the providers list", default_provider),
                "type": "validation_error"
            }
        }));
    }

    let proxy_guard = proxy.read().await;

    // Prepare the session
    match proxy_guard
        .prepare_session(api_key, providers, model_mappings, default_provider)
        .await
    {
        Ok(session_id) => {
            let expires_at = chrono::Utc::now() + chrono::Duration::days(3);
            axum::response::Json(serde_json::json!({
                "status": "success",
                "session_id": session_id,
                "expires_at": expires_at.to_rfc3339()
            }))
        }
        Err(e) => {
            tracing::error!("Session preparation error: {}", e);
            axum::response::Json(serde_json::json!({
                "error": {
                    "message": format!("Session preparation failed: {}", e),
                    "type": "internal_error"
                }
            }))
        }
    }
}

/// Handle session end requests
async fn handle_end_session(
    proxy: std::sync::Arc<tokio::sync::RwLock<LlmApiProxy>>,
    axum::extract::Json(body): axum::extract::Json<serde_json::Value>,
) -> axum::response::Json<serde_json::Value> {
    tracing::info!("Received end-session request");

    // Extract required fields
    let api_key = match body.get("api_key").and_then(|v| v.as_str()) {
        Some(key) => key.to_string(),
        None => {
            return axum::response::Json(serde_json::json!({
                "error": {
                    "message": "Missing required field: api_key",
                    "type": "validation_error"
                }
            }));
        }
    };

    let proxy_guard = proxy.read().await;

    // End the session
    match proxy_guard.end_session(&api_key).await {
        Ok(()) => axum::response::Json(serde_json::json!({
            "status": "success",
            "message": "Session ended"
        })),
        Err(e) => {
            tracing::error!("Session end error: {}", e);
            axum::response::Json(serde_json::json!({
                "error": {
                    "message": format!("Session end failed: {}", e),
                    "type": "internal_error"
                }
            }))
        }
    }
}

/// Handle OpenAI responses API requests
async fn handle_openai_responses(
    proxy: std::sync::Arc<tokio::sync::RwLock<LlmApiProxy>>,
    axum::extract::Json(body): axum::extract::Json<serde_json::Value>,
) -> Sse<impl futures::Stream<Item = Result<Event, Infallible>>> {
    tracing::info!("Received OpenAI responses request");
    let proxy_guard = proxy.read().await;

    // Determine mode based on proxy configuration
    let mode = if proxy_guard.is_scenario_mode() {
        llm_api_proxy::proxy::ProxyMode::Scenario
    } else {
        llm_api_proxy::proxy::ProxyMode::Live
    };

    let streaming = body.get("stream").and_then(|s| s.as_bool()).unwrap_or(false);
    let proxy_request = llm_api_proxy::proxy::ProxyRequest {
        client_format: llm_api_proxy::converters::ApiFormat::OpenAIResponses,
        mode,
        payload: body,
        headers: std::collections::HashMap::new(),
        request_id: format!("req-{}", uuid::Uuid::new_v4()),
        streaming,
    };

    let (events, keep_alive) = match proxy_guard.proxy_request(proxy_request).await {
        Ok(response) => {
            let response_payload = response.payload;
            let response_id = response_payload
                .get("id")
                .and_then(|id| id.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("resp-{}", Uuid::new_v4()));

            let created_event = serde_json::json!({
                "response": {
                    "id": response_id,
                    "status": "in_progress"
                }
            });

            let completed_event = serde_json::json!({
                "response": response_payload
            });

            (
                stream::iter(vec![
                    Ok(Event::default().event("response.created").data(created_event.to_string())),
                    Ok(Event::default()
                        .event("response.completed")
                        .data(completed_event.to_string())),
                ]),
                KeepAlive::new().interval(Duration::from_secs(5)),
            )
        }
        Err(e) => {
            tracing::error!("Proxy error: {}", e);
            let error_event = serde_json::json!({
                "error": {
                    "message": format!("Proxy error: {}", e),
                    "type": "internal_error"
                }
            });
            (
                stream::iter(vec![Ok(Event::default()
                    .event("response.error")
                    .data(error_event.to_string()))]),
                KeepAlive::new().interval(Duration::from_secs(5)),
            )
        }
    };

    Sse::new(events).keep_alive(keep_alive)
}

/// Handle OpenAI responses streaming endpoint with deterministic events
async fn handle_openai_responses_stream(
    _proxy: std::sync::Arc<tokio::sync::RwLock<LlmApiProxy>>,
    Path(response_id): Path<String>,
) -> Sse<impl futures::Stream<Item = Result<Event, Infallible>>> {
    tracing::info!("Streaming responses requested for id={}", response_id);
    let created_event = serde_json::json!({
        "response": {
            "id": response_id,
            "status": "in_progress"
        }
    });

    let completed_event = serde_json::json!({
        "response": {
            "id": response_id,
            "status": "completed"
        }
    });

    let events = stream::iter(vec![
        Ok(Event::default().event("response.created").data(created_event.to_string())),
        Ok(Event::default().event("response.completed").data(completed_event.to_string())),
    ]);

    Sse::new(events).keep_alive(KeepAlive::new().interval(Duration::from_secs(5)))
}

/// Handle Anthropic messages requests
async fn handle_anthropic_messages(
    proxy: std::sync::Arc<tokio::sync::RwLock<LlmApiProxy>>,
    axum::extract::Json(body): axum::extract::Json<serde_json::Value>,
) -> Response {
    tracing::info!("Received Anthropic messages request");
    let proxy_guard = proxy.read().await;

    // Determine mode based on proxy configuration
    let mode = if proxy_guard.is_scenario_mode() {
        llm_api_proxy::proxy::ProxyMode::Scenario
    } else {
        llm_api_proxy::proxy::ProxyMode::Live
    };

    // Create a proxy request from the incoming body
    let streaming = body.get("stream").and_then(|s| s.as_bool()).unwrap_or(false);
    let proxy_request = llm_api_proxy::proxy::ProxyRequest {
        client_format: llm_api_proxy::converters::ApiFormat::Anthropic,
        mode,
        payload: body,
        headers: std::collections::HashMap::new(), // TODO: Extract headers from request
        request_id: format!("req-{}", uuid::Uuid::new_v4()),
        streaming,
    };

    // Process the request through the proxy
    match proxy_guard.proxy_request(proxy_request).await {
        Ok(response) => proxy_response_to_axum_response(response),
        Err(e) => {
            tracing::error!("Proxy error: {}", e);
            axum::response::Json(serde_json::json!({
                "error": {
                    "message": format!("Proxy error: {}", e),
                    "type": "internal_error"
                }
            }))
            .into_response()
        }
    }
}
