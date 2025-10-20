// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Metrics collection and telemetry
//!
//! This module provides basic metrics collection for API requests
//! including token counts, latency, and request statistics.
//!
//! TODO: Integrate with actual Helicone telemetry when available

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;
// TODO: Uncomment when telemetry crate is available
// use telemetry::MetricsCollector as HeliconeMetrics;

/// Metrics collector for the LLM API proxy
#[derive(Debug)]
pub struct MetricsCollector {
    // Basic in-memory metrics (will be replaced with Helicone telemetry)
    total_requests: AtomicU64,
    successful_requests: AtomicU64,
    failed_requests: AtomicU64,
    total_response_time_ms: AtomicU64,
    total_prompt_tokens: AtomicU64,
    total_completion_tokens: AtomicU64,
    active_requests: AtomicUsize,
}

impl MetricsCollector {
    /// Create a new metrics collector
    pub async fn new() -> Result<Self, crate::Error> {
        // TODO: Initialize Helicone metrics collector
        // let helicone_metrics = HeliconeMetrics::new().await?;

        Ok(Self {
            total_requests: AtomicU64::new(0),
            successful_requests: AtomicU64::new(0),
            failed_requests: AtomicU64::new(0),
            total_response_time_ms: AtomicU64::new(0),
            total_prompt_tokens: AtomicU64::new(0),
            total_completion_tokens: AtomicU64::new(0),
            active_requests: AtomicUsize::new(0),
        })
    }

    /// Record the start of a request
    pub async fn record_request_start(&self, request: &crate::proxy::ProxyRequest) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.active_requests.fetch_add(1, Ordering::Relaxed);
        tracing::debug!("Request started: {}", request.request_id);
    }

    /// Record a successful request completion
    pub async fn record_request_success(
        &self,
        request: &crate::proxy::ProxyRequest,
        response: &crate::proxy::ProxyResponse,
        duration: Duration,
    ) {
        self.successful_requests.fetch_add(1, Ordering::Relaxed);
        self.active_requests.fetch_sub(1, Ordering::Relaxed);
        self.total_response_time_ms
            .fetch_add(duration.as_millis() as u64, Ordering::Relaxed);

        // Try to extract token counts from response
        self.extract_and_record_tokens(response);

        tracing::debug!(
            "Request completed successfully: {} ({}ms)",
            request.request_id,
            duration.as_millis()
        );
    }

    /// Record a request error
    pub async fn record_request_error(
        &self,
        request: &crate::proxy::ProxyRequest,
        error: &crate::Error,
        duration: Duration,
    ) {
        self.failed_requests.fetch_add(1, Ordering::Relaxed);
        self.active_requests.fetch_sub(1, Ordering::Relaxed);
        self.total_response_time_ms
            .fetch_add(duration.as_millis() as u64, Ordering::Relaxed);

        tracing::debug!(
            "Request failed: {} ({}ms) - {:?}",
            request.request_id,
            duration.as_millis(),
            error
        );
    }

    /// Extract and record token counts from response
    fn extract_and_record_tokens(&self, response: &crate::proxy::ProxyResponse) {
        // Try to extract token usage from OpenAI-style responses (including OpenRouter)
        if let Ok(openai_resp) = serde_json::from_value::<
            async_openai::types::CreateChatCompletionResponse,
        >(response.payload.clone())
        {
            if let Some(usage) = openai_resp.usage {
                self.total_prompt_tokens
                    .fetch_add(usage.prompt_tokens as u64, Ordering::Relaxed);
                self.total_completion_tokens
                    .fetch_add(usage.completion_tokens as u64, Ordering::Relaxed);
            }
        }

        // TODO: Handle Anthropic response format token extraction when implemented
        // TODO: Add specific OpenRouter response format handling if different from OpenAI
    }

    /// Get current metrics snapshot
    pub async fn snapshot(&self) -> Result<MetricsSnapshot, crate::Error> {
        let total_requests = self.total_requests.load(Ordering::Relaxed);
        let successful_requests = self.successful_requests.load(Ordering::Relaxed);
        let total_response_time_ms = self.total_response_time_ms.load(Ordering::Relaxed);

        let average_response_time_ms = if total_requests > 0 {
            total_response_time_ms as f64 / total_requests as f64
        } else {
            0.0
        };

        Ok(MetricsSnapshot {
            total_requests,
            successful_requests,
            failed_requests: self.failed_requests.load(Ordering::Relaxed),
            average_response_time_ms,
            total_prompt_tokens: self.total_prompt_tokens.load(Ordering::Relaxed),
            total_completion_tokens: self.total_completion_tokens.load(Ordering::Relaxed),
            active_requests: self.active_requests.load(Ordering::Relaxed),
        })
    }

    /// Export metrics to external system
    pub async fn export(&self) -> Result<(), crate::Error> {
        // TODO: Export metrics using Helicone telemetry
        Ok(())
    }
}

/// Snapshot of current metrics
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MetricsSnapshot {
    /// Total number of requests processed
    pub total_requests: u64,
    /// Number of successful requests
    pub successful_requests: u64,
    /// Number of failed requests
    pub failed_requests: u64,
    /// Average response time in milliseconds
    pub average_response_time_ms: f64,
    /// Total prompt tokens used across all requests
    pub total_prompt_tokens: u64,
    /// Total completion tokens generated across all requests
    pub total_completion_tokens: u64,
    /// Number of currently active requests
    pub active_requests: usize,
}

/// Request metrics
#[derive(Debug, Clone)]
pub struct RequestMetrics {
    /// Request ID
    pub request_id: String,
    /// Provider used
    pub provider: String,
    /// Model used
    pub model: String,
    /// Request start time
    pub start_time: std::time::Instant,
    /// Request tokens (prompt)
    pub prompt_tokens: Option<u32>,
    /// Response tokens (completion)
    pub completion_tokens: Option<u32>,
    /// Total tokens
    pub total_tokens: Option<u32>,
    /// Response time
    pub response_time: Option<Duration>,
    /// Success flag
    pub success: bool,
    /// Error message if failed
    pub error_message: Option<String>,
}
