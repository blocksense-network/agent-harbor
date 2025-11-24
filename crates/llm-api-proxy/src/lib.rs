// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

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

pub use ah_scenario_format::Scenario;
pub use config::ProviderConfig;
/// Re-export commonly used types
pub use converters::{ApiFormat, ConversionRequest, ConversionResponse};
pub use scenario::ScenarioPlayer;
