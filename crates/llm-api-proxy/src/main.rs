// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! LLM API Proxy - Library usage and test server

use axum::extract::Path;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::{
    Router,
    routing::{get, post},
};
use clap::{Parser, Subcommand};
use futures::stream;
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
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
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
            )
            .await?;
        }
    }

    Ok(())
}

/// Run as a test server for integration testing
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
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting LLM API Proxy test server on port {}", port);
    println!("Scenario file: {}", scenario_file.display());
    println!("Scenario file exists: {}", scenario_file.exists());
    println!("Agent type: {}", agent_type);
    println!("Agent version: {}", agent_version);
    println!("Strict tools validation: {}", strict_tools_validation);
    println!("Request log path: {:?}", request_log_path);
    println!("Log headers: {}", log_headers);
    println!("Log body: {}", log_body);
    println!("Log responses: {}", log_responses);

    // Set logging environment variables
    if let Some(log_path) = &request_log_path {
        std::env::set_var("REQUEST_LOG_TEMPLATE", log_path);
        println!("Set REQUEST_LOG_TEMPLATE={}", log_path.display());
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

    // Create the proxy
    println!(
        "Creating proxy with config: agent_type={:?}, agent_version={:?}, scenario_file={:?}",
        config.scenario.agent_type, config.scenario.agent_version, config.scenario.scenario_file
    );
    let proxy = match LlmApiProxy::new(config).await {
        Ok(p) => {
            println!("Proxy created successfully");
            p
        }
        Err(e) => {
            eprintln!("Failed to create proxy: {}", e);
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
            eprintln!("Unhandled request path: {}", req.uri());
            (axum::http::StatusCode::NOT_FOUND, "Not Found")
        })
        .layer(CorsLayer::permissive());

    // Bind to address
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    println!("Server listening on {}", addr);

    // Run the server
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Handle OpenAI chat completion requests
async fn handle_chat_completion(
    proxy: std::sync::Arc<tokio::sync::RwLock<LlmApiProxy>>,
    axum::extract::Json(body): axum::extract::Json<serde_json::Value>,
) -> axum::response::Json<serde_json::Value> {
    println!("Received OpenAI chat completion request");
    let proxy_guard = proxy.read().await;

    // Create a proxy request from the incoming body
    let proxy_request = llm_api_proxy::proxy::ProxyRequest {
        client_format: llm_api_proxy::converters::ApiFormat::OpenAI,
        mode: llm_api_proxy::proxy::ProxyMode::Scenario,
        payload: body,
        headers: std::collections::HashMap::new(), // TODO: Extract headers from request
        request_id: format!("req-{}", uuid::Uuid::new_v4()),
    };

    // Process the request through the proxy
    match proxy_guard.proxy_request(proxy_request).await {
        Ok(response) => axum::response::Json(response.payload),
        Err(e) => {
            eprintln!("Proxy error: {}", e);
            axum::response::Json(serde_json::json!({
                "error": {
                    "message": format!("Proxy error: {}", e),
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
    println!("Received OpenAI responses request");
    let proxy_guard = proxy.read().await;

    let proxy_request = llm_api_proxy::proxy::ProxyRequest {
        client_format: llm_api_proxy::converters::ApiFormat::OpenAIResponses,
        mode: llm_api_proxy::proxy::ProxyMode::Scenario,
        payload: body,
        headers: std::collections::HashMap::new(),
        request_id: format!("req-{}", uuid::Uuid::new_v4()),
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
            eprintln!("Proxy error: {}", e);
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
    eprintln!("Streaming responses requested for id={}", response_id);
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
) -> axum::response::Json<serde_json::Value> {
    println!("Received Anthropic messages request");
    let proxy_guard = proxy.read().await;

    // Create a proxy request from the incoming body
    let proxy_request = llm_api_proxy::proxy::ProxyRequest {
        client_format: llm_api_proxy::converters::ApiFormat::Anthropic,
        mode: llm_api_proxy::proxy::ProxyMode::Scenario,
        payload: body,
        headers: std::collections::HashMap::new(), // TODO: Extract headers from request
        request_id: format!("req-{}", uuid::Uuid::new_v4()),
    };

    // Process the request through the proxy
    match proxy_guard.proxy_request(proxy_request).await {
        Ok(response) => axum::response::Json(response.payload),
        Err(e) => {
            eprintln!("Proxy error: {}", e);
            axum::response::Json(serde_json::json!({
                "error": {
                    "message": format!("Proxy error: {}", e),
                    "type": "internal_error"
                }
            }))
        }
    }
}
