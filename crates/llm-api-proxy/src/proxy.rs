// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Core proxy logic for routing and processing LLM API requests

use std::sync::Arc;
use tokio::sync::RwLock;

use crate::{
    config::{ProviderConfig, ProxyConfig, RoutingConfig},
    converters::ApiFormat,
    error::{Error, Result},
    metrics::MetricsCollector,
    routing::DynamicRouter,
    scenario::ScenarioPlayer,
};

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

/// Session-specific routing configuration
#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub routing_config: RoutingConfig,
    pub providers: HashMap<String, ProviderConfig>,
    pub created_at: Instant,
    pub last_used: Instant,
}

/// Session information stored per API key
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub session_id: String,
    pub api_key: String,
    pub config_hash: u64, // Hash of the routing config for deduplication
}

/// Session manager for dynamic routing configurations
#[derive(Debug)]
pub struct SessionManager {
    /// Map from API key to session info
    sessions: RwLock<HashMap<String, SessionInfo>>,
    /// Map from config hash to (config, reference count)
    config_cache: RwLock<HashMap<u64, (SessionConfig, AtomicU32)>>,
    /// Session expiration timeout (3 days)
    session_timeout: Duration,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            config_cache: RwLock::new(HashMap::new()),
            session_timeout: Duration::from_secs(3 * 24 * 60 * 60), // 3 days
        }
    }

    /// Prepare a session with custom routing configuration
    pub async fn prepare_session(
        &self,
        api_key: String,
        routing_config: RoutingConfig,
        providers: HashMap<String, ProviderConfig>,
    ) -> Result<String> {
        let session_id = format!("session-{}", uuid::Uuid::new_v4());

        // Create session config
        let session_config = SessionConfig {
            routing_config,
            providers,
            created_at: Instant::now(),
            last_used: Instant::now(),
        };

        // Calculate hash for deduplication
        let config_hash = self.calculate_config_hash(&session_config);

        // Store or update config in cache
        let mut config_cache = self.config_cache.write().await;
        config_cache
            .entry(config_hash)
            .or_insert_with(|| (session_config, AtomicU32::new(0)))
            .1
            .fetch_add(1, Ordering::SeqCst);

        // Create session info
        let session_info = SessionInfo {
            session_id: session_id.clone(),
            api_key: api_key.clone(),
            config_hash,
        };

        // Store session
        let mut sessions = self.sessions.write().await;
        sessions.insert(api_key, session_info);

        Ok(session_id)
    }

    /// Get session configuration for an API key
    pub async fn get_session_config(&self, api_key: &str) -> Option<SessionConfig> {
        let sessions = self.sessions.read().await;
        if let Some(session_info) = sessions.get(api_key) {
            // Update last used time
            let mut config_cache = self.config_cache.write().await;
            if let Some((config, _)) = config_cache.get_mut(&session_info.config_hash) {
                config.last_used = Instant::now();
                return Some(config.clone());
            }
        }
        None
    }

    /// End a session explicitly
    pub async fn end_session(&self, api_key: &str) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        if let Some(session_info) = sessions.remove(api_key) {
            // Decrease reference count
            let mut config_cache = self.config_cache.write().await;
            if let Some((_, ref_count)) = config_cache.get_mut(&session_info.config_hash) {
                let new_count = ref_count.fetch_sub(1, Ordering::SeqCst);
                if new_count == 0 {
                    // Remove config if no more references
                    config_cache.remove(&session_info.config_hash);
                }
            }
        }
        Ok(())
    }

    /// Clean up expired sessions
    pub async fn cleanup_expired_sessions(&self) -> usize {
        let mut sessions = self.sessions.write().await;
        let mut config_cache = self.config_cache.write().await;
        let mut expired_sessions = Vec::new();

        // Find expired sessions
        for (api_key, session_info) in sessions.iter() {
            if let Some((config, _)) = config_cache.get(&session_info.config_hash) {
                if config.last_used.elapsed() > self.session_timeout {
                    expired_sessions.push(api_key.clone());
                }
            }
        }

        // Remove expired sessions
        let expired_count = expired_sessions.len();
        for api_key in expired_sessions {
            if let Some(session_info) = sessions.remove(&api_key) {
                // Decrease reference count
                if let Some((_, ref_count)) = config_cache.get_mut(&session_info.config_hash) {
                    let new_count = ref_count.fetch_sub(1, Ordering::SeqCst);
                    if new_count == 0 {
                        config_cache.remove(&session_info.config_hash);
                    }
                }
            }
        }

        expired_count
    }

    /// Calculate hash for config deduplication
    fn calculate_config_hash(&self, config: &SessionConfig) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        config.routing_config.hash(&mut hasher);
        for (key, provider) in &config.providers {
            key.hash(&mut hasher);
            provider.name.hash(&mut hasher);
            provider.base_url.hash(&mut hasher);
            // Don't hash API keys for security, but include other provider fields
        }
        hasher.finish()
    }
}

/// Main LLM API proxy struct
pub struct LlmApiProxy {
    config: Arc<RwLock<ProxyConfig>>,
    router: Arc<DynamicRouter>,
    metrics: Arc<MetricsCollector>,
    scenario_player: Option<Arc<RwLock<ScenarioPlayer>>>,
    session_manager: Arc<SessionManager>,
    http_client: reqwest::Client,
}

impl LlmApiProxy {
    /// Create a new proxy instance
    pub async fn new(config: ProxyConfig) -> Result<Self> {
        // Validate configuration
        config.validate()?;

        let config = Arc::new(RwLock::new(config));
        let router = Arc::new(DynamicRouter::new(config.clone()).await?);
        let metrics = Arc::new(MetricsCollector::new().await?);
        let session_manager = Arc::new(SessionManager::new());

        let scenario_player = if config.read().await.scenario.enabled {
            Some(Arc::new(RwLock::new(
                ScenarioPlayer::new(config.clone()).await?,
            )))
        } else {
            None
        };

        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()?;

        Ok(Self {
            config,
            router,
            metrics,
            scenario_player,
            session_manager,
            http_client,
        })
    }

    /// Check if scenario mode is enabled
    pub fn is_scenario_mode(&self) -> bool {
        self.scenario_player.is_some()
    }

    /// Prepare a session with custom routing configuration
    pub async fn prepare_session(
        &self,
        api_key: String,
        routing_config: RoutingConfig,
        providers: HashMap<String, ProviderConfig>,
    ) -> Result<String> {
        self.session_manager.prepare_session(api_key, routing_config, providers).await
    }

    /// End a session explicitly
    pub async fn end_session(&self, api_key: &str) -> Result<()> {
        self.session_manager.end_session(api_key).await
    }

    /// Get session configuration for an API key
    pub async fn get_session_config(&self, api_key: &str) -> Option<SessionConfig> {
        self.session_manager.get_session_config(api_key).await
    }

    /// Clean up expired sessions
    pub async fn cleanup_expired_sessions(&self) -> usize {
        self.session_manager.cleanup_expired_sessions().await
    }

    /// Extract API key from request headers
    fn extract_api_key_from_request(&self, request: &ProxyRequest) -> Result<Option<String>> {
        // Check Authorization header
        if let Some(auth_header) = request.headers.get("authorization") {
            if let Some(bearer_token) = auth_header.strip_prefix("Bearer ") {
                return Ok(Some(bearer_token.to_string()));
            }
        }

        // Check alternative headers
        if let Some(api_key) = request.headers.get("api-key") {
            return Ok(Some(api_key.to_string()));
        }

        if let Some(api_key) = request.headers.get("x-api-key") {
            return Ok(Some(api_key.to_string()));
        }

        Ok(None)
    }

    /// Process an API request through the proxy
    pub async fn proxy_request(&self, request: ProxyRequest) -> Result<ProxyResponse> {
        let start_time = std::time::Instant::now();

        // Record request metrics
        self.metrics.record_request_start(&request).await;

        let result = match request.mode {
            ProxyMode::Live => self.handle_live_request(&request).await,
            ProxyMode::Scenario => self.handle_scenario_request(&request).await,
        };

        // Record response metrics
        let duration = start_time.elapsed();
        match &result {
            Ok(response) => {
                self.metrics.record_request_success(&request, response, duration).await;
            }
            Err(error) => {
                self.metrics.record_request_error(&request, error, duration).await;
            }
        }

        result
    }

    /// Handle a live API request (route to real providers)
    async fn handle_live_request(&self, request: &ProxyRequest) -> Result<ProxyResponse> {
        // Check if this request has a session-based configuration
        let api_key = self.extract_api_key_from_request(request)?;
        let session_config = if let Some(api_key) = &api_key {
            self.get_session_config(api_key).await
        } else {
            None
        };

        let provider = if let Some(session_config) = &session_config {
            // Use session-specific routing
            let session_router = DynamicRouter::new_from_session(session_config.clone()).await?;
            session_router.select_provider(&request).await?
        } else {
            // Use default routing
            self.router.select_provider(&request).await?
        };

        // Convert request format if needed
        let converted_request = self.convert_request_for_provider(&request, &provider).await?;

        // Send request to provider
        let response = self.send_provider_request(&provider, converted_request).await?;

        // Convert response back to client's expected format
        let final_response = self.convert_response_for_client(response, &request).await?;

        Ok(final_response)
    }

    /// Handle a scenario playback request
    async fn handle_scenario_request(&self, request: &ProxyRequest) -> Result<ProxyResponse> {
        let player = self.scenario_player.as_ref().ok_or_else(|| Error::Scenario {
            message: "Scenario playback not enabled".to_string(),
        })?;

        // Validate tool definitions in client requests (strict tools validation)
        if let Some(tools) = request.payload.get("tools").and_then(|t| t.as_array()) {
            let player_guard = player.read().await;
            player_guard.validate_tool_definitions(tools, &request.payload).await?;
        }

        let mut player = player.write().await;
        player.play_request(request).await
    }

    /// Convert request to the format expected by the target provider
    async fn convert_request_for_provider(
        &self,
        request: &ProxyRequest,
        provider: &ProviderInfo,
    ) -> Result<serde_json::Value> {
        // Determine the source format from the request
        let source_format = request.client_format;

        // For now, use pass-through with basic model mapping
        // TODO: Implement full API format conversions when API compatibility is resolved
        match (source_format, provider.api_format) {
            // Anthropic request to OpenAI provider (e.g., OpenRouter)
            (ApiFormat::Anthropic, ApiFormat::OpenAI) => {
                // For OpenRouter, we can pass Anthropic requests through as-is
                // since OpenRouter supports both formats
                Ok(request.payload.clone())
            }
            (ApiFormat::OpenAIResponses, ApiFormat::OpenAI) => Ok(request.payload.clone()),
            // OpenAI request to OpenAI provider
            (ApiFormat::OpenAI, ApiFormat::OpenAI) => Ok(request.payload.clone()),
            // OpenAI request to Anthropic provider
            (ApiFormat::OpenAI, ApiFormat::Anthropic) => {
                // Pass through for now
                Ok(request.payload.clone())
            }
            (ApiFormat::OpenAIResponses, ApiFormat::Anthropic) => Ok(request.payload.clone()),
            // Anthropic request to Anthropic provider
            (ApiFormat::Anthropic, ApiFormat::Anthropic) => Ok(request.payload.clone()),
            // Default passthrough for other combinations
            _ => Ok(request.payload.clone()),
        }
    }

    /// Send request to the provider
    async fn send_provider_request(
        &self,
        provider: &ProviderInfo,
        request: serde_json::Value,
    ) -> Result<ProviderResponse> {
        // Special handling for OpenRouter provider
        if provider.name == "openrouter" {
            return self.send_openrouter_request(provider, request).await;
        }

        // Default HTTP implementation for other providers
        let url = format!("{}/chat/completions", provider.base_url);

        let mut request_builder =
            self.http_client.post(&url).header("Content-Type", "application/json");

        // Add authentication
        if let Some(api_key) = &provider.api_key {
            match provider.api_format {
                ApiFormat::OpenAI | ApiFormat::OpenAIResponses => {
                    request_builder = request_builder.bearer_auth(api_key);
                }
                ApiFormat::Anthropic => {
                    request_builder = request_builder.header("x-api-key", api_key);
                }
            }
        }

        // Add any additional headers
        for (key, value) in &provider.headers {
            request_builder = request_builder.header(key, value);
        }

        let response = request_builder.json(&request).send().await?;

        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            return Err(Error::Provider {
                provider: provider.name.clone(),
                status: status.as_u16(),
                message: body,
            });
        }

        Ok(ProviderResponse {
            status: status.as_u16(),
            body: serde_json::from_str(&body)?,
            headers: Default::default(), // TODO: capture headers
        })
    }

    /// Send request to OpenRouter using HTTP API
    async fn send_openrouter_request(
        &self,
        provider: &ProviderInfo,
        request: serde_json::Value,
    ) -> Result<ProviderResponse> {
        let url = format!("{}/chat/completions", provider.base_url);

        let mut request_builder =
            self.http_client.post(&url).header("Content-Type", "application/json").header(
                "Authorization",
                format!(
                    "Bearer {}",
                    provider.api_key.as_ref().unwrap_or(&"".to_string())
                ),
            );

        // Add OpenRouter-specific headers
        for (key, value) in &provider.headers {
            request_builder = request_builder.header(key, value);
        }

        let response = request_builder.json(&request).send().await?;

        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            return Err(Error::Provider {
                provider: provider.name.clone(),
                status: status.as_u16(),
                message: body,
            });
        }

        Ok(ProviderResponse {
            status: status.as_u16(),
            body: serde_json::from_str(&body)?,
            headers: Default::default(),
        })
    }

    /// Convert provider response back to client's expected format
    async fn convert_response_for_client(
        &self,
        response: ProviderResponse,
        original_request: &ProxyRequest,
    ) -> Result<ProxyResponse> {
        // For now, return provider response as-is
        // TODO: Implement proper response format conversions when API compatibility is resolved
        match original_request.client_format {
            ApiFormat::Anthropic => {
                // For demonstration, return OpenRouter response as-is
                // In practice, this would need conversion back to Anthropic format
                Ok(ProxyResponse {
                    status: response.status,
                    payload: response.body,
                    headers: response.headers,
                })
            }
            ApiFormat::OpenAI | ApiFormat::OpenAIResponses => {
                // Client expects OpenAI format, provider should return OpenAI format
                Ok(ProxyResponse {
                    status: response.status,
                    payload: response.body,
                    headers: response.headers,
                })
            }
        }
    }

    /// Get current configuration
    pub async fn config(&self) -> ProxyConfig {
        self.config.read().await.clone()
    }

    /// Update configuration
    pub async fn update_config(&self, config: ProxyConfig) -> Result<()> {
        config.validate()?;
        *self.config.write().await = config;
        Ok(())
    }

    /// Get metrics collector
    pub fn metrics(&self) -> &Arc<MetricsCollector> {
        &self.metrics
    }

    /// Check if scenario playback is enabled
    pub fn scenario_enabled(&self) -> bool {
        self.scenario_player.is_some()
    }
}

/// Request to be processed by the proxy
#[derive(Debug, Clone)]
pub struct ProxyRequest {
    /// Client's expected API format
    pub client_format: ApiFormat,
    /// Proxy mode (live or scenario)
    pub mode: ProxyMode,
    /// Request payload
    pub payload: serde_json::Value,
    /// Request headers
    pub headers: std::collections::HashMap<String, String>,
    /// Request ID for tracking
    pub request_id: String,
}

/// Proxy mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyMode {
    /// Route to live providers
    Live,
    /// Use scenario playback
    Scenario,
}

/// Response from the proxy
#[derive(Debug, Clone)]
pub struct ProxyResponse {
    /// HTTP status code
    pub status: u16,
    /// Response payload
    pub payload: serde_json::Value,
    /// Response headers
    pub headers: std::collections::HashMap<String, String>,
}

/// Provider information
#[derive(Debug, Clone)]
pub struct ProviderInfo {
    /// Provider name
    pub name: String,
    /// Base URL
    pub base_url: String,
    /// API format used by this provider
    pub api_format: ApiFormat,
    /// API key
    pub api_key: Option<String>,
    /// Additional headers
    pub headers: std::collections::HashMap<String, String>,
}

/// Response from a provider
#[derive(Debug, Clone)]
pub struct ProviderResponse {
    /// HTTP status code
    pub status: u16,
    /// Response body
    pub body: serde_json::Value,
    /// Response headers
    pub headers: std::collections::HashMap<String, String>,
}
