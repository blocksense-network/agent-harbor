// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Server configuration

use std::net::SocketAddr;

/// Server configuration
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Address to bind the server to
    pub bind_addr: SocketAddr,

    /// Path to SQLite database
    pub database_path: String,

    /// Enable CORS headers for development
    pub enable_cors: bool,

    /// JWT secret for token validation
    pub jwt_secret: Option<String>,

    /// API key for authentication
    pub api_key: Option<String>,

    /// Additional configuration file to load
    pub config_file: Option<String>,

    /// Rate limiting configuration
    pub rate_limit: RateLimitConfig,

    /// Agent Client Protocol gateway configuration
    pub acp: AcpConfig,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:3001".parse().unwrap(),
            database_path: ":memory:".to_string(),
            enable_cors: false,
            jwt_secret: None,
            api_key: None,
            config_file: None,
            rate_limit: RateLimitConfig::default(),
            acp: AcpConfig::default(),
        }
    }
}

/// ACP gateway configuration
///
/// The gateway exposes Agent Harbor through the Agent Client Protocol (ACP)
/// JSON-RPC surface described in `resources/acp-specs/docs/overview/architecture.mdx`
/// and `resources/acp-specs/docs/protocol/overview.mdx`. The gateway is
/// feature-gated so that we can roll out the ACP control plane alongside the
/// existing REST API without changing default behavior.
#[derive(Debug, Clone)]
pub struct AcpConfig {
    /// Enable or disable the ACP gateway entirely. When `false`, the server
    /// behaves exactly like today and does not bind the ACP transport.
    pub enabled: bool,

    /// TCP socket used for WebSocket transport (`/acp/v1/connect`). This is
    /// ignored when running in stdio mode.
    pub bind_addr: SocketAddr,

    /// How clients connect to the gateway. ACP supports both in-process stdio
    /// pipes (for `ah agent access-point --stdio-acp`) and WebSocket upgrade
    /// over HTTP; we negotiate capabilities during `initialize`.
    pub transport: AcpTransportMode,

    /// Authentication policy for `authenticate` frames. `InheritRestAuth`
    /// reuses the existing API key / JWT validation in `auth.rs`. `Anonymous`
    /// is reserved for local, air-gapped developer setups.
    pub auth_policy: AcpAuthPolicy,

    /// Maximum concurrent ACP connections (applied at handshake time).
    pub connection_limit: usize,

    /// Idle timeout for ACP connections (seconds). If no frames are received in
    /// this window the gateway will close the socket defensively.
    pub idle_timeout_secs: u64,
}

impl Default for AcpConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bind_addr: "127.0.0.1:3031".parse().expect("valid socket address"),
            transport: AcpTransportMode::WebSocket,
            auth_policy: AcpAuthPolicy::InheritRestAuth,
            connection_limit: 32,
            idle_timeout_secs: 30,
        }
    }
}

/// Supported ACP transport modes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcpTransportMode {
    /// Serve ACP over WebSocket (JSON-RPC over HTTP upgrade)
    WebSocket,
    /// Serve ACP over stdio pipes (launched as a sidecar process)
    Stdio,
}

impl AcpTransportMode {
    /// Returns true when the transport expects a TCP listener.
    pub fn uses_socket(self) -> bool {
        matches!(self, AcpTransportMode::WebSocket)
    }
}

/// Authentication policies for ACP connections
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcpAuthPolicy {
    /// Reuse the existing REST auth guardrails (API key and/or JWT)
    InheritRestAuth,
    /// Allow unauthenticated connections (intended only for local dev)
    Anonymous,
}

#[cfg(test)]
mod tests {
    use super::*;
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
                    tracing::info!(
                        "test log available at {} ({} bytes)",
                        self.path.display(),
                        meta.len()
                    );
                } else {
                    tracing::info!("test log available at {}", self.path.display());
                }
            }
        }
    }

    #[test]
    fn acp_config_defaults() {
        let mut log = TestLog::new("acp_config_defaults");
        let config = ServerConfig::default();
        log.record(&format!("defaults: {:?}", config.acp));

        assert!(
            !config.acp.enabled,
            "ACP must be opt-in so REST defaults stay unchanged"
        );
        assert_eq!(
            config.acp.bind_addr,
            "127.0.0.1:3031".parse().unwrap(),
            "default ACP bind address should be local-only"
        );
        assert_eq!(config.acp.transport, AcpTransportMode::WebSocket);
        assert_eq!(config.acp.auth_policy, AcpAuthPolicy::InheritRestAuth);
        assert_eq!(config.acp.connection_limit, 32);
        assert_eq!(config.acp.idle_timeout_secs, 30);
    }
}

/// Rate limiting configuration
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Requests per minute per IP
    pub requests_per_minute: u64,

    /// Burst size
    pub burst_size: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_minute: 60,
            burst_size: 10,
        }
    }
}
