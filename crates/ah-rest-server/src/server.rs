// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Main server implementation

use crate::dependencies::DefaultServerDependencies;
use crate::error::ServerResult;
use crate::handlers;
use crate::middleware::{RateLimitState, rate_limit_middleware};
use crate::state::AppState;
use crate::{acp::AcpGateway, config::ServerConfig, error::ServerError};
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
    acp_gateway: Option<AcpGateway>,
}

impl Server {
    /// Create a new server instance
    pub async fn new(config: ServerConfig) -> ServerResult<Self> {
        let state = DefaultServerDependencies::new(config.clone()).await?.into_state();
        Self::with_state(config, state).await
    }

    /// Construct a server from an already-built app state (used for custom dependencies)
    pub async fn with_state(config: ServerConfig, state: AppState) -> ServerResult<Self> {
        let acp_gateway = AcpGateway::bind(config.acp.clone(), state.clone()).await?;
        let app = Self::build_app(state, &config);
        Ok(Self {
            config: config.clone(),
            app,
            acp_gateway: config.acp.enabled.then_some(acp_gateway),
        })
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
        Router::new()
            .nest("/api/v1", api_routes)
            .nest("/", openapi_routes)
            .with_state(state)
            .layer(middleware_stack)
    }

    /// Run the server
    pub async fn run(self) -> ServerResult<()> {
        let addr = self.config.bind_addr;
        info!("Starting server on {}", addr);

        let listener = tokio::net::TcpListener::bind(addr).await?;
        let rest_server = axum::serve(listener, self.app);

        if let Some(acp_gateway) = self.acp_gateway {
            tokio::try_join!(
                async move {
                    rest_server
                        .await
                        .map_err(|err| ServerError::Internal(format!("REST server error: {err}")))
                },
                async move { acp_gateway.run().await.map_err(ServerError::from) },
            )?;
        } else {
            rest_server
                .await
                .map_err(|err| ServerError::Internal(format!("REST server error: {err}")))?;
        }

        Ok(())
    }

    /// Address of the ACP gateway when it is enabled and bound.
    pub fn acp_bind_addr(&self) -> Option<std::net::SocketAddr> {
        self.acp_gateway.as_ref().and_then(|gateway| gateway.handle().map(|h| h.addr()))
    }

    /// Get the bind address
    pub fn addr(&self) -> SocketAddr {
        self.config.bind_addr
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock_dependencies::MockServerDependencies;
    use std::{
        fs::{File, metadata},
        io::Write,
        path::PathBuf,
    };
    use uuid::Uuid;

    struct TestLog {
        path: PathBuf,
        file: File,
    }

    impl TestLog {
        fn new(name: &str) -> Self {
            let mut path = std::env::temp_dir();
            path.push(format!("ah-rest-server-{}-{}.log", name, Uuid::new_v4()));
            let file = File::create(&path).expect("create log file");
            Self { path, file }
        }

        fn record(&mut self, msg: &str) {
            writeln!(self.file, "{}", msg).expect("write log line");
        }
    }

    impl Drop for TestLog {
        fn drop(&mut self) {
            if std::thread::panicking() {
                if let Ok(meta) = metadata(&self.path) {
                    eprintln!(
                        "test log available at {} ({} bytes)",
                        self.path.display(),
                        meta.len()
                    );
                } else {
                    eprintln!("test log available at {}", self.path.display());
                }
            }
        }
    }

    #[tokio::test]
    async fn acp_flag_enables_gateway() {
        let mut log = TestLog::new("acp_flag_enables_gateway");

        let mut config = ServerConfig::default();
        config.bind_addr = "127.0.0.1:0".parse().unwrap();
        config.enable_cors = true;
        config.acp.enabled = true;
        config.acp.bind_addr = "127.0.0.1:0".parse().unwrap();

        let deps = MockServerDependencies::new(config.clone()).await.expect("mock deps");
        let server = Server::with_state(config.clone(), deps.into_state()).await.expect("server");

        let acp_addr = server.acp_bind_addr();
        log.record(&format!("acp_addr_enabled={:?}", acp_addr));
        let bound = acp_addr.expect("ACP gateway should bind when enabled");
        assert_ne!(bound.port(), 0, "gateway should bind to an ephemeral port");

        let mut disabled_config = config;
        disabled_config.acp.enabled = false;
        let deps_disabled =
            MockServerDependencies::new(disabled_config.clone()).await.expect("mock deps");
        let server_disabled = Server::with_state(disabled_config, deps_disabled.into_state())
            .await
            .expect("server");
        log.record(&format!(
            "acp_addr_disabled={:?}",
            server_disabled.acp_bind_addr()
        ));
        assert!(
            server_disabled.acp_bind_addr().is_none(),
            "gateway should remain disabled when flag is off"
        );
    }
}
