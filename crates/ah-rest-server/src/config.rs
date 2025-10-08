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

    /// Rate limiting configuration
    pub rate_limit: RateLimitConfig,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:3001".parse().unwrap(),
            database_path: ":memory:".to_string(),
            enable_cors: false,
            jwt_secret: None,
            api_key: None,
            rate_limit: RateLimitConfig::default(),
        }
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
