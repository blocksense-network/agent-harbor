//! Core proxy logic for routing and processing LLM API requests

use std::sync::Arc;
use tokio::sync::RwLock;

use crate::{
    config::ProxyConfig,
    converters::{self, ApiFormat},
    error::{Error, Result},
    metrics::MetricsCollector,
    routing::{DynamicRouter, ProviderSelector},
    scenario::ScenarioPlayer,
};

/// Main LLM API proxy struct
pub struct LlmApiProxy {
    config: Arc<RwLock<ProxyConfig>>,
    router: Arc<DynamicRouter>,
    metrics: Arc<MetricsCollector>,
    scenario_player: Option<Arc<ScenarioPlayer>>,
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

        let scenario_player = if config.read().await.scenario.enabled {
            Some(Arc::new(ScenarioPlayer::new(config.clone()).await?))
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
            http_client,
        })
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
        // Determine target provider
        let provider = self.router.select_provider(&request).await?;

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
            // OpenAI request to OpenAI provider
            (ApiFormat::OpenAI, ApiFormat::OpenAI) => Ok(request.payload.clone()),
            // OpenAI request to Anthropic provider
            (ApiFormat::OpenAI, ApiFormat::Anthropic) => {
                // Pass through for now
                Ok(request.payload.clone())
            }
            // Anthropic request to Anthropic provider
            (ApiFormat::Anthropic, ApiFormat::Anthropic) => Ok(request.payload.clone()),
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
                ApiFormat::OpenAI => {
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
            ApiFormat::OpenAI => {
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
