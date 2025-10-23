// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Example usage of the LLM API proxy library

use llm_api_proxy::{LlmApiProxy, ProxyConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("LLM API Proxy - Example usage");

    // Create a basic configuration
    let config = ProxyConfig::default();

    // Create the proxy instance
    let proxy = LlmApiProxy::new(config).await?;

    println!("Proxy initialized successfully!");
    println!("Scenario playback enabled: {}", proxy.scenario_enabled());

    Ok(())
}
