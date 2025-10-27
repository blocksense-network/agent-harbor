// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Main server implementation

use crate::config::ServerConfig;
use crate::error::ServerResult;
use crate::handlers;
use crate::middleware::{RateLimitState, rate_limit_middleware};
use crate::state::AppState;
use axum::{
    Router,
    http::HeaderValue,
    middleware::from_fn,
    routing::{delete, get, post, put},
};
use std::net::SocketAddr;
use tower::ServiceBuilder;
use tower_http::{
    compression::CompressionLayer,
    cors::{Any, CorsLayer},
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};
use tracing::info;

/// REST API server
pub struct Server {
    config: ServerConfig,
    app: Router,
}

impl Server {
    /// Create a new server instance
    pub async fn new(config: ServerConfig) -> ServerResult<Self> {
        let state = AppState::new(config.clone()).await?;

        // Start the task executor
        state.task_executor.start();

        // Build the application with all routes and middleware
        let app = Self::build_app(state, &config);

        Ok(Self { config, app })
    }

    /// Build the Axum application with routes and middleware
    fn build_app(state: AppState, config: &ServerConfig) -> Router {
        // Build middleware stack
        let middleware_stack = ServiceBuilder::new()
            .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
            .layer(PropagateRequestIdLayer::x_request_id())
            .layer(TraceLayer::new_for_http())
            .layer(CompressionLayer::new())
            .layer(from_fn({
                let rate_limit_state =
                    std::sync::Arc::new(RateLimitState::new(config.rate_limit.clone()));
                move |req, next| {
                    let state = std::sync::Arc::clone(&rate_limit_state);
                    rate_limit_middleware(state, req, next)
                }
            }))
            .layer({
                if config.enable_cors {
                    CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any)
                } else {
                    CorsLayer::new()
                        .allow_origin(vec![
                            HeaderValue::from_static("http://localhost:3000"),
                            HeaderValue::from_static("http://127.0.0.1:3000"),
                        ])
                        .allow_methods([
                            axum::http::Method::GET,
                            axum::http::Method::POST,
                            axum::http::Method::PUT,
                            axum::http::Method::DELETE,
                        ])
                        .allow_headers([
                            axum::http::header::AUTHORIZATION,
                            axum::http::header::CONTENT_TYPE,
                        ])
                }
            });

        // API routes
        let api_routes = Router::new()
            // Health and status endpoints
            .route("/healthz", get(handlers::health::health_check))
            .route("/readyz", get(handlers::health::readiness_check))
            .route("/version", get(handlers::health::version))
            // Task management
            .route("/tasks", post(handlers::tasks::create_task))
            // Session management
            .route("/sessions", get(handlers::sessions::list_sessions))
            .route("/sessions/:id", get(handlers::sessions::get_session))
            .route("/sessions/:id", put(handlers::sessions::update_session))
            .route("/sessions/:id", delete(handlers::sessions::delete_session))
            // Session control
            .route("/sessions/:id/stop", post(handlers::sessions::stop_session))
            .route(
                "/sessions/:id/pause",
                post(handlers::sessions::pause_session),
            )
            .route(
                "/sessions/:id/resume",
                post(handlers::sessions::resume_session),
            )
            // Logs and events
            .route(
                "/sessions/:id/logs",
                get(handlers::sessions::get_session_logs),
            )
            .route(
                "/sessions/:id/events",
                get(handlers::sessions::stream_session_events),
            )
            .route(
                "/sessions/:id/info",
                get(handlers::sessions::get_session_info),
            )
            // Capability discovery
            .route("/agents", get(handlers::capabilities::list_agents))
            .route("/runtimes", get(handlers::capabilities::list_runtimes))
            .route("/executors", get(handlers::capabilities::list_executors))
            // Projects and repositories
            .route("/projects", get(handlers::projects::list_projects))
            .route("/repos", get(handlers::projects::list_repositories))
            .route(
                "/repositories/:id/branches",
                get(handlers::repositories::get_repository_branches),
            )
            .route(
                "/repositories/:id/files",
                get(handlers::repositories::get_repository_files),
            )
            // Workspaces
            .route("/workspaces", get(handlers::workspaces::list_workspaces))
            .route("/workspaces/:id", get(handlers::workspaces::get_workspace))
            // Drafts
            .route("/drafts", get(handlers::drafts::list_drafts))
            .route("/drafts", post(handlers::drafts::create_draft))
            .route("/drafts/:id", get(handlers::drafts::get_draft))
            .route("/drafts/:id", put(handlers::drafts::update_draft))
            .route("/drafts/:id", delete(handlers::drafts::delete_draft));

        // OpenAPI routes
        let openapi_routes = Router::new()
            .route("/openapi.json", get(handlers::openapi::openapi_spec))
            .route("/docs/", get(handlers::openapi::swagger_ui));

        // Combine all routes
        let app = Router::new()
            .nest("/api/v1", api_routes)
            .nest("/", openapi_routes)
            .with_state(state)
            .layer(middleware_stack);

        app
    }

    /// Run the server
    pub async fn run(self) -> ServerResult<()> {
        let addr = self.config.bind_addr;
        info!("Starting server on {}", addr);

        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, self.app).await?;

        Ok(())
    }

    /// Get the bind address
    pub fn addr(&self) -> SocketAddr {
        self.config.bind_addr
    }
}
