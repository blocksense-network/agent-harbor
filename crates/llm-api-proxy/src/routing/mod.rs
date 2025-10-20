// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Provider routing and load balancing logic
//!
//! This module integrates with the Helicone ai-gateway routing crates
//! to provide intelligent provider selection and load balancing.
//!
//! TODO: Integrate with actual Helicone crates when available

use std::collections::HashMap;
use std::sync::Mutex;

use crate::{
    config::{ProviderConfig, ProxyConfig},
    error::{Error, Result},
    proxy::{ProviderInfo, ProxyRequest},
};

// TODO: Replace with actual Helicone imports when available
// use dynamic_router::DynamicRouter as HeliconeDynamicRouter;
// use weighted_balance::WeightedBalancer;

// Placeholder type until Helicone crates are integrated
#[derive(Debug)]
#[allow(dead_code)]
struct HeliconeDynamicRouter;

/// Dynamic router that selects providers based on request characteristics
pub struct DynamicRouter {
    // TODO: Replace with HeliconeDynamicRouter when available
    // helicone_router: HeliconeDynamicRouter,
    config: std::sync::Arc<tokio::sync::RwLock<ProxyConfig>>,
    selector: ProviderSelector,
}

impl DynamicRouter {
    /// Create a new dynamic router
    pub async fn new(config: std::sync::Arc<tokio::sync::RwLock<ProxyConfig>>) -> Result<Self> {
        // TODO: Initialize Helicone dynamic router with our configuration
        // let helicone_router = Self::create_helicone_router(&config).await?;

        Ok(Self {
            // helicone_router,
            config,
            selector: ProviderSelector::new(),
        })
    }

    /// Select the best provider for a request
    pub async fn select_provider(&self, request: &ProxyRequest) -> Result<ProviderInfo> {
        let config = self.config.read().await;

        // Extract model from request to determine routing
        let model = Self::extract_model_from_request(request)?;

        // Use routing rules to select provider
        let provider_name = self.select_provider_name(&model, &config).await;

        let mut candidates: Vec<(ProviderInfo, u32)> = Vec::new();

        // Prefer exact key match first
        if let Some(provider) = config.providers.get(&provider_name) {
            candidates.push((
                Self::provider_config_to_info(provider),
                provider.weight.max(1),
            ));
        }

        // Include providers with matching logical name (for regional replicas)
        for (key, provider) in &config.providers {
            if key == &provider_name {
                continue;
            }
            if provider.name == provider_name {
                candidates.push((
                    Self::provider_config_to_info(provider),
                    provider.weight.max(1),
                ));
            }
        }

        if candidates.is_empty() {
            return Err(Error::Routing {
                message: format!("Provider '{}' not found in configuration", provider_name),
            });
        }

        if candidates.len() == 1 || !config.routing.enable_fallback {
            return Ok(candidates.remove(0).0);
        }

        self.selector.select(&provider_name, &candidates).ok_or_else(|| Error::Routing {
            message: format!("Unable to select provider for '{}'", provider_name),
        })
    }

    /// Select provider name based on model and routing rules
    async fn select_provider_name(&self, model: &str, config: &ProxyConfig) -> String {
        // Check model-specific routing rules first
        if let Some(provider) = config.routing.model_routing.get(model) {
            return provider.clone();
        }

        // Check for model patterns (e.g., "gpt-*" -> "openai")
        for (pattern, provider) in &config.routing.model_routing {
            if pattern.contains('*') {
                let regex_pattern = pattern.replace('*', ".*");
                if regex::Regex::new(&regex_pattern).map_or(false, |re| re.is_match(model)) {
                    return provider.clone();
                }
            }
        }

        // Fall back to default provider
        config.routing.default_provider.clone()
    }

    /// Extract model name from request payload
    fn extract_model_from_request(request: &ProxyRequest) -> Result<String> {
        // Try to extract model from the JSON payload based on format
        match request.client_format {
            crate::converters::ApiFormat::OpenAI
            | crate::converters::ApiFormat::OpenAIResponses => {
                // OpenAI format: {"model": "..."}
                if let Some(model) = request.payload.get("model").and_then(|m| m.as_str()) {
                    Ok(model.to_string())
                } else {
                    Err(Error::Routing {
                        message: "Could not extract model from OpenAI request".to_string(),
                    })
                }
            }
            crate::converters::ApiFormat::Anthropic => {
                // Anthropic format: {"model": "..."}
                if let Some(model) = request.payload.get("model").and_then(|m| m.as_str()) {
                    Ok(model.to_string())
                } else {
                    Err(Error::Routing {
                        message: "Could not extract model from Anthropic request".to_string(),
                    })
                }
            }
        }
    }

    /// Convert provider config to provider info
    fn provider_config_to_info(config: &ProviderConfig) -> ProviderInfo {
        use crate::converters::ApiFormat;

        // Determine API format based on provider name
        let api_format = match config.name.to_lowercase().as_str() {
            "anthropic" => ApiFormat::Anthropic,
            "openai" | "openrouter" => ApiFormat::OpenAI,
            _ => ApiFormat::OpenAI, // Default to OpenAI format
        };

        ProviderInfo {
            name: config.name.clone(),
            base_url: config.base_url.clone(),
            api_format,
            api_key: config.api_key.clone(),
            headers: config.headers.clone(),
        }
    }

    /// Create the underlying Helicone router
    #[allow(dead_code)]
    async fn create_helicone_router(
        _config: &std::sync::Arc<tokio::sync::RwLock<ProxyConfig>>,
    ) -> Result<HeliconeDynamicRouter> {
        // TODO: Initialize Helicone router with our provider configurations
        // For now, return a placeholder
        Err(Error::Routing {
            message: "Helicone router integration not yet implemented".to_string(),
        })
    }
}

/// Provider selector for load balancing
#[derive(Default)]
pub struct ProviderSelector {
    counters: Mutex<HashMap<String, usize>>,
}

impl ProviderSelector {
    /// Create a new provider selector
    pub fn new() -> Self {
        Self::default()
    }

    /// Select a provider using weighted round-robin
    pub fn select(&self, key: &str, providers: &[(ProviderInfo, u32)]) -> Option<ProviderInfo> {
        if providers.is_empty() {
            return None;
        }

        let total_weight: u32 = providers.iter().map(|(_, weight)| (*weight).max(1)).sum();
        if total_weight == 0 {
            return Some(providers[0].0.clone());
        }

        let index = {
            let mut guard = self.counters.lock().unwrap();
            let counter = guard.entry(key.to_string()).or_insert(0);
            let current = *counter % total_weight as usize;
            *counter = (*counter + 1) % total_weight as usize;
            current
        };

        let mut cumulative: usize = 0;
        for (info, weight) in providers {
            cumulative += (*weight as usize).max(1);
            if index < cumulative {
                return Some(info.clone());
            }
        }

        providers.last().map(|(info, _)| info.clone())
    }
}

// ProviderConfig is already imported at the top of the file
