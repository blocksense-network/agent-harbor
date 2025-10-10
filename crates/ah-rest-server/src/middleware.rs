//! Custom middleware

use crate::config::RateLimitConfig;
use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_governor::GovernorLayer;

/// Rate limiting state
#[derive(Clone)]
pub struct RateLimitState {
    requests: Arc<Mutex<HashMap<String, Vec<std::time::Instant>>>>,
    config: RateLimitConfig,
}

impl RateLimitState {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            requests: Arc::new(Mutex::new(HashMap::new())),
            config,
        }
    }

    /// Check if request should be rate limited
    pub async fn check_rate_limit(&self, key: &str) -> bool {
        let mut requests = self.requests.lock().await;
        let now = std::time::Instant::now();

        let client_requests = requests.entry(key.to_string()).or_insert_with(Vec::new);

        // Remove old requests outside the time window
        let window_start = now - std::time::Duration::from_secs(60);
        client_requests.retain(|&time| time > window_start);

        // Check if under limit
        if client_requests.len() < self.config.requests_per_minute as usize {
            client_requests.push(now);
            true
        } else {
            false
        }
    }
}

/// Simple rate limiting middleware (alternative to tower_governor)
pub async fn rate_limit_middleware(
    state: Arc<RateLimitState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Get client identifier (IP address for now)
    let client_ip = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("unknown");

    if state.check_rate_limit(client_ip).await {
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::TOO_MANY_REQUESTS)
    }
}

// TODO: Implement proper governor rate limiting
// pub fn create_governor_layer(config: &RateLimitConfig) -> GovernorLayer {
//     // Implementation here
// }
