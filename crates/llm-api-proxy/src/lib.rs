//! # LLM API Proxy
//!
//! A high-performance LLM API proxy library for Agent Harbor WebUI that provides:
//! - Bidirectional API translation between OpenAI and Anthropic formats
//! - Intelligent routing to multiple LLM providers (OpenRouter, Anthropic, OpenAI, etc.)
//! - Comprehensive metrics collection (Helicone-style telemetry)
//! - Deterministic scenario playback for testing
//!
//! ## Architecture
//!
//! The proxy leverages existing Helicone ai-gateway crates where possible:
//! - `telemetry` crate for metrics collection
//! - `dynamic-router` crate for intelligent provider routing
//! - `weighted-balance` crate for load balancing
//!
//! ## Usage
//!
//! ```rust,no_run
//! use llm_api_proxy::{LlmApiProxy, ProxyConfig};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create a basic configuration
//!     let config = ProxyConfig::default();
//!
//!     // Create the proxy instance
//!     let proxy = LlmApiProxy::new(config).await?;
//!
//!     println!("Proxy initialized successfully!");
//!     println!("Scenario playback enabled: {}", proxy.scenario_enabled());
//!
//!     Ok(())
//! }
//! ```

pub mod config;
pub mod converters;
pub mod error;
pub mod metrics;
pub mod proxy;
pub mod routing;
pub mod scenario;

pub use config::ProxyConfig;
pub use error::{Error, Result};
pub use proxy::LlmApiProxy;

/// Version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Re-export commonly used types
pub use converters::{ApiFormat, ConversionRequest, ConversionResponse};
pub use config::ProviderConfig;
pub use scenario::{Scenario, ScenarioPlayer};
