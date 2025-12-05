// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Gateway entry point that will expose Agent Harbor over ACP.
//!
//! The gateway will eventually multiplex WebSocket and stdio transports, reuse
//! the REST dependency injector, and feed ACP `session/update` notifications
//! from the existing task manager. For Milestone 0 we only scaffold the
//! lifecycle wiring so the feature flag and bind address are honored without
//! changing default REST behavior.

use crate::{auth::AuthConfig, config::AcpConfig, state::AppState};
#[cfg(unix)]
use ah_acp_bridge::default_uds_path;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::task::JoinSet;

use super::{AcpResult, transport};
use crate::acp::errors::AcpError;

/// Handle to an ACP gateway instance.
pub struct AcpGateway {
    _config: AcpConfig,
    state: AppState,
    listener: Option<TcpListener>,
}

/// Lightweight handle exposed to the HTTP server so tests can introspect the
/// bound address without consuming the listener.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GatewayHandle {
    addr: SocketAddr,
}

impl GatewayHandle {
    /// Return the socket address the gateway is bound to.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }
}

impl AcpGateway {
    /// Build a gateway from configuration and shared app state.
    pub async fn bind(mut config: AcpConfig, state: AppState) -> AcpResult<Self> {
        if !config.enabled {
            return Ok(Self {
                _config: config,
                state,
                listener: None,
            });
        }

        #[cfg(unix)]
        if config.uds_path.is_none() {
            config.uds_path = Some(default_uds_path());
        }

        let listener = if config.transport.uses_socket() {
            Some(TcpListener::bind(config.bind_addr).await?)
        } else {
            None
        };

        Ok(Self {
            _config: config,
            state,
            listener,
        })
    }

    /// Returns the address of the bound socket when running in WebSocket mode.
    pub fn handle(&self) -> Option<GatewayHandle> {
        self.listener
            .as_ref()
            .and_then(|listener| listener.local_addr().ok())
            .map(|addr| GatewayHandle { addr })
    }

    /// Run the gateway. In Milestone 0 this serves a stub route that returns a
    /// 501 to make the binding observable without changing behavior.
    pub async fn run(self) -> AcpResult<()> {
        let auth = AuthConfig {
            api_key: self.state.config.api_key.clone(),
            jwt_secret: self.state.config.jwt_secret.clone(),
        };
        let permits =
            std::sync::Arc::new(tokio::sync::Semaphore::new(self._config.connection_limit));
        let idle = std::time::Duration::from_secs(self._config.idle_timeout_secs);
        let transport_state = transport::AcpTransportState {
            auth,
            permits,
            idle_timeout: idle,
            config: self._config.clone(),
            app_state: self.state.clone(),
        };

        let mut tasks: JoinSet<AcpResult<()>> = JoinSet::new();

        if matches!(
            self._config.transport,
            crate::config::AcpTransportMode::Stdio
        ) {
            let state = transport_state.clone();
            tasks.spawn(async move { transport::run_stdio(state).await });
        }

        if matches!(
            self._config.transport,
            crate::config::AcpTransportMode::WebSocket
        ) {
            if let Some(listener) = self.listener {
                let state = transport_state.clone();
                tasks.spawn(async move {
                    let app = transport::router(state);
                    axum::serve(listener, app).await.map_err(Into::into)
                });
            }
        }

        if let Some(path) = self._config.uds_path.clone() {
            let state = transport_state.clone();
            tasks.spawn(async move { transport::run_uds(state, path).await });
        }

        while let Some(res) = tasks.join_next().await {
            match res {
                Ok(inner) => inner?,
                Err(err) => return Err(AcpError::Internal(err.to_string())),
            }
        }

        Ok(())
    }
}
