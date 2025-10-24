// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Configuration management for the LLM API proxy

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Main configuration structure for the LLM API proxy
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProxyConfig {
    /// Server configuration
    pub server: ServerConfig,

    /// Provider configurations
    pub providers: HashMap<String, ProviderConfig>,

    /// Routing configuration
    pub routing: RoutingConfig,

    /// Metrics configuration
    pub metrics: MetricsConfig,

    /// Security configuration
    pub security: SecurityConfig,

    /// Scenario playback configuration
    pub scenario: ScenarioConfig,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        let mut providers = HashMap::new();
        // Add Anthropic provider
        providers.insert(
            "anthropic".to_string(),
            ProviderConfig {
                name: "anthropic".to_string(),
                base_url: "https://api.anthropic.com".to_string(),
                api_key: std::env::var("ANTHROPIC_API_KEY").ok(),
                headers: HashMap::new(),
                models: vec![
                    "claude-3-haiku-20240307".to_string(),
                    "claude-3-sonnet-20240229".to_string(),
                    "claude-3-opus-20240229".to_string(),
                    "claude-3-5-sonnet-20241022".to_string(),
                ],
                weight: 1,
                rate_limit_rpm: Some(50), // Anthropic rate limit
                timeout_seconds: Some(300),
            },
        );

        // Add OpenAI provider
        providers.insert(
            "openai".to_string(),
            ProviderConfig {
                name: "openai".to_string(),
                base_url: "https://api.openai.com/v1".to_string(),
                api_key: std::env::var("OPENAI_API_KEY").ok(),
                headers: HashMap::new(),
                models: vec![
                    "gpt-4o".to_string(),
                    "gpt-4o-mini".to_string(),
                    "gpt-4-turbo".to_string(),
                    "gpt-3.5-turbo".to_string(),
                ],
                weight: 1,
                rate_limit_rpm: Some(10000), // OpenAI rate limit
                timeout_seconds: Some(60),
            },
        );

        // Add OpenRouter provider for Anthropic -> OpenRouter routing
        providers.insert(
            "openrouter".to_string(),
            ProviderConfig {
                name: "openrouter".to_string(),
                base_url: "https://openrouter.ai/api/v1".to_string(),
                api_key: std::env::var("OPENROUTER_API_KEY").ok(),
                headers: HashMap::new(),
                models: vec![
                    "anthropic/claude-3-haiku".to_string(),
                    "anthropic/claude-3-sonnet".to_string(),
                    "anthropic/claude-3-opus".to_string(),
                    "anthropic/claude-3.5-sonnet".to_string(),
                ],
                weight: 1,
                rate_limit_rpm: Some(1000), // OpenRouter rate limit
                timeout_seconds: Some(60),
            },
        );

        // Add a default mock provider for testing
        providers.insert(
            "mock".to_string(),
            ProviderConfig {
                name: "mock".to_string(),
                base_url: "http://mock-provider".to_string(),
                api_key: None,
                headers: HashMap::new(),
                models: vec!["gpt-3.5-turbo".to_string()],
                weight: 1,
                rate_limit_rpm: None,
                timeout_seconds: None,
            },
        );

        Self {
            server: ServerConfig::default(),
            providers,
            routing: RoutingConfig::default(),
            metrics: MetricsConfig::default(),
            security: SecurityConfig::default(),
            scenario: ScenarioConfig::default(),
        }
    }
}

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    /// Server host
    pub host: String,

    /// Server port
    pub port: u16,

    /// Request timeout in seconds
    pub timeout_seconds: u64,

    /// Maximum request body size in bytes
    pub max_body_size: usize,

    /// Enable CORS
    pub cors_enabled: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8080,
            timeout_seconds: 300,            // 5 minutes
            max_body_size: 10 * 1024 * 1024, // 10MB
            cors_enabled: true,
        }
    }
}

/// Provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Provider name (anthropic, openai, openrouter, etc.)
    pub name: String,

    /// Base URL for the provider API
    pub base_url: String,

    /// API key for authentication
    pub api_key: Option<String>,

    /// Additional headers to send with requests
    pub headers: HashMap<String, String>,

    /// Models supported by this provider
    pub models: Vec<String>,

    /// Weight for load balancing (higher = more requests)
    pub weight: u32,

    /// Maximum requests per minute
    pub rate_limit_rpm: Option<u32>,

    /// Timeout in seconds for this provider
    pub timeout_seconds: Option<u64>,
}

/// Routing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RoutingConfig {
    /// Default provider to use
    pub default_provider: String,

    /// Routing rules based on model patterns
    pub model_routing: HashMap<String, String>,

    /// Enable fallback to other providers on failure
    pub enable_fallback: bool,

    /// Maximum number of retries
    pub max_retries: u32,

    /// Retry delay in milliseconds
    pub retry_delay_ms: u64,
}

impl Default for RoutingConfig {
    fn default() -> Self {
        Self {
            default_provider: "mock".to_string(),
            model_routing: HashMap::new(),
            enable_fallback: true,
            max_retries: 3,
            retry_delay_ms: 1000,
        }
    }
}

impl std::hash::Hash for ProviderConfig {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
        self.base_url.hash(state);
        // Don't hash api_key for security
        // Don't hash headers as they may contain sensitive data
        // Don't hash models as they might be large
        self.weight.hash(state);
        self.rate_limit_rpm.hash(state);
        self.timeout_seconds.hash(state);
    }
}

impl std::hash::Hash for RoutingConfig {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.default_provider.hash(state);
        // Don't hash model_routing as it's a HashMap
        self.enable_fallback.hash(state);
        self.max_retries.hash(state);
        self.retry_delay_ms.hash(state);
    }
}

/// Metrics configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MetricsConfig {
    /// Enable metrics collection
    pub enabled: bool,

    /// Metrics endpoint
    pub endpoint: Option<String>,

    /// Include request/response bodies in metrics (be careful with PII)
    pub include_bodies: bool,

    /// Sampling rate for metrics (0.0 = none, 1.0 = all)
    pub sampling_rate: f64,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            endpoint: None,
            include_bodies: false,
            sampling_rate: 1.0,
        }
    }
}

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SecurityConfig {
    /// API keys for client authentication
    pub api_keys: Vec<String>,

    /// Rate limiting configuration
    pub rate_limiting: RateLimitConfig,

    /// Request validation settings
    pub validation: ValidationConfig,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            api_keys: Vec::new(),
            rate_limiting: RateLimitConfig::default(),
            validation: ValidationConfig::default(),
        }
    }
}

/// Rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RateLimitConfig {
    /// Requests per minute limit
    pub requests_per_minute: u32,

    /// Burst size
    pub burst_size: u32,

    /// Enable rate limiting
    pub enabled: bool,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_minute: 1000,
            burst_size: 100,
            enabled: true,
        }
    }
}

/// Request validation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ValidationConfig {
    /// Maximum prompt tokens
    pub max_prompt_tokens: Option<usize>,

    /// Maximum completion tokens
    pub max_completion_tokens: Option<usize>,

    /// Allowed models (empty = all allowed)
    pub allowed_models: Vec<String>,

    /// Block potentially harmful content
    pub content_filtering: bool,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            max_prompt_tokens: Some(128_000),
            max_completion_tokens: Some(4096),
            allowed_models: Vec::new(),
            content_filtering: true,
        }
    }
}

/// Scenario playback configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ScenarioConfig {
    /// Enable scenario playback mode
    pub enabled: bool,

    /// Directory containing scenario files (for loading multiple scenarios)
    pub scenario_dir: Option<String>,

    /// Single scenario file to load (for testing)
    pub scenario_file: Option<String>,

    /// Agent type for tool validation (claude, codex, etc.)
    pub agent_type: Option<String>,

    /// Agent version for tool changes tracking
    pub agent_version: Option<String>,

    /// Workspace directory for scenario execution
    pub workspace_dir: Option<String>,

    /// Enable strict tools validation mode
    pub strict_tools_validation: bool,

    /// Minimize JSON logs (default: false, pretty-print by default)
    pub minimize_logs: bool,
}

impl Default for ScenarioConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            scenario_dir: None,
            scenario_file: None,
            agent_type: None,
            agent_version: None,
            workspace_dir: None,
            strict_tools_validation: false,
            minimize_logs: false,
        }
    }
}

impl ProxyConfig {
    /// Load configuration from a file
    pub fn from_file(path: &std::path::Path) -> std::result::Result<Self, Error> {
        let contents = std::fs::read_to_string(path)?;
        let config: ProxyConfig = serde_yaml::from_str(&contents)?;
        Ok(config)
    }

    /// Save configuration to a file
    pub fn save_to_file(&self, path: &std::path::Path) -> std::result::Result<(), Error> {
        let contents = serde_yaml::to_string(self)?;
        std::fs::write(path, contents)?;
        Ok(())
    }

    /// Validate the configuration
    pub fn validate(&self) -> std::result::Result<(), Error> {
        // Validate server config
        if self.server.port == 0 {
            return Err(Error::Config {
                message: "Server port cannot be 0".to_string(),
            });
        }

        // Validate providers
        if self.providers.is_empty() {
            return Err(Error::Config {
                message: "At least one provider must be configured".to_string(),
            });
        }

        // Validate default provider exists
        if !self.providers.contains_key(&self.routing.default_provider) {
            return Err(Error::Config {
                message: format!(
                    "Default provider '{}' not found in providers",
                    self.routing.default_provider
                ),
            });
        }

        // Validate provider configurations
        for (name, provider) in &self.providers {
            if provider.models.is_empty() {
                return Err(Error::Config {
                    message: format!("Provider '{}' has no models configured", name),
                });
            }

            // For the default provider, require API key when not in scenario mode (except for mock provider)
            if name == &self.routing.default_provider
                && name != "mock"
                && !self.scenario.enabled
                && provider.api_key.is_none()
            {
                return Err(Error::Config {
                    message: format!(
                        "Default provider '{}' requires an API key. Set the {} environment variable or provide --api-key",
                        name,
                        match name.as_str() {
                            "anthropic" => "ANTHROPIC_API_KEY",
                            "openai" => "OPENAI_API_KEY",
                            "openrouter" => "OPENROUTER_API_KEY",
                            _ => "API_KEY",
                        }
                    ),
                });
            }
        }

        Ok(())
    }
}

use crate::error::Error;
