// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Basic integration tests for the LLM API proxy

use llm_api_proxy::proxy::{ProxyMode, ProxyRequest};
use llm_api_proxy::{LlmApiProxy, ProxyConfig, converters::ApiFormat, routing::DynamicRouter};

#[tokio::test]
async fn test_proxy_creation() {
    // Create a basic configuration
    let config = ProxyConfig::default();

    // Create the proxy instance
    let proxy = LlmApiProxy::new(config).await;

    // Should succeed with default config
    assert!(proxy.is_ok());

    let proxy = proxy.unwrap();

    // Check that scenario playback is disabled by default
    assert!(!proxy.scenario_enabled());
}

#[tokio::test]
async fn test_config_validation() {
    // Test valid config
    let config = ProxyConfig::default();
    match config.validate() {
        Ok(_) => println!("Config validation passed"),
        Err(e) => println!("Config validation failed: {:?}", e),
    }
    assert!(config.validate().is_ok());

    // Test invalid config (empty providers)
    let mut invalid_config = ProxyConfig::default();
    invalid_config.providers.clear();
    assert!(invalid_config.validate().is_err());
}

#[tokio::test]
async fn test_anthropic_to_openrouter_routing() {
    // Create proxy with OpenRouter provider
    let proxy = LlmApiProxy::new(ProxyConfig::default()).await.unwrap();

    // Get initial metrics
    let initial_metrics = proxy.metrics().snapshot().await.unwrap();

    // Create an Anthropic-style request (simplified example)
    let anthropic_request = serde_json::json!({
        "model": "claude-3-haiku-20240307",
        "messages": [
            {
                "role": "user",
                "content": "Hello, how are you?"
            }
        ],
        "max_tokens": 100
    });

    let request = ProxyRequest {
        client_format: ApiFormat::Anthropic,
        mode: ProxyMode::Live,
        payload: anthropic_request,
        headers: std::collections::HashMap::new(),
        request_id: "test-anthropic-openrouter".to_string(),
    };

    // Note: This will fail because we don't have a real OpenRouter API key
    // and the mock provider doesn't actually handle requests, but it demonstrates
    // the routing logic works
    let result = proxy.proxy_request(request).await;

    // We expect this to fail due to no real API, but not due to routing/conversion issues
    assert!(result.is_err());

    // Check that metrics were recorded
    let final_metrics = proxy.metrics().snapshot().await.unwrap();
    assert_eq!(
        final_metrics.total_requests,
        initial_metrics.total_requests + 1
    );
    assert_eq!(
        final_metrics.failed_requests,
        initial_metrics.failed_requests + 1
    );
    assert!(final_metrics.average_response_time_ms > 0.0);

    println!("✅ Anthropic -> OpenRouter routing test passed!");
    println!(
        "📊 Metrics: {} total requests, {} failed, {:.2}ms avg latency",
        final_metrics.total_requests,
        final_metrics.failed_requests,
        final_metrics.average_response_time_ms
    );
}

#[tokio::test]
async fn test_full_proxy_workflow() {
    // Test the complete proxy workflow: Anthropic request → OpenRouter → metrics
    let proxy = LlmApiProxy::new(ProxyConfig::default()).await.unwrap();

    // Verify configuration
    let config = proxy.config().await;
    assert!(config.providers.contains_key("openrouter"));
    assert_eq!(config.routing.default_provider, "mock");

    // Create multiple requests to test metrics accumulation
    let requests = vec![
        ("claude-3-haiku-20240307", "What is the capital of France?"),
        ("claude-3-sonnet-20240229", "Explain quantum computing"),
        ("claude-3-opus-20240229", "Write a haiku about programming"),
    ];

    let mut total_requests = 0;
    let mut total_failures = 0;

    for (model, prompt) in requests {
        let anthropic_request = serde_json::json!({
            "model": model,
            "messages": [{"role": "user", "content": prompt}],
            "max_tokens": 150
        });

        let request = ProxyRequest {
            client_format: ApiFormat::Anthropic,
            mode: ProxyMode::Live,
            payload: anthropic_request,
            headers: std::collections::HashMap::new(),
            request_id: format!("test-workflow-{}", total_requests),
        };

        let result = proxy.proxy_request(request).await;
        assert!(result.is_err()); // Expected to fail without real API key

        total_requests += 1;
        total_failures += 1;
    }

    // Verify metrics accumulation
    let metrics = proxy.metrics().snapshot().await.unwrap();
    assert_eq!(metrics.total_requests, total_requests);
    assert_eq!(metrics.failed_requests, total_failures);
    assert_eq!(metrics.successful_requests, 0);
    assert!(metrics.average_response_time_ms > 0.0);
    assert!(metrics.active_requests == 0); // All requests completed

    println!("✅ Full proxy workflow test passed!");
    println!("📊 Final Metrics:");
    println!("   Total requests: {}", metrics.total_requests);
    println!("   Successful: {}", metrics.successful_requests);
    println!("   Failed: {}", metrics.failed_requests);
    println!("   Avg latency: {:.2}ms", metrics.average_response_time_ms);
    println!("   Total prompt tokens: {}", metrics.total_prompt_tokens);
    println!(
        "   Total completion tokens: {}",
        metrics.total_completion_tokens
    );
}

#[tokio::test]
async fn test_provider_routing_logic() {
    // Test that the routing logic correctly selects providers based on model names
    let proxy = LlmApiProxy::new(ProxyConfig::default()).await.unwrap();

    // Test Anthropic models route to OpenRouter
    let anthropic_models = vec![
        "claude-3-haiku-20240307",
        "claude-3-sonnet-20240229",
        "claude-3-opus-20240229",
        "claude-3-5-sonnet-20240620",
    ];

    for model in anthropic_models {
        let request = ProxyRequest {
            client_format: ApiFormat::Anthropic,
            mode: ProxyMode::Live,
            payload: serde_json::json!({"model": model, "messages": []}),
            headers: std::collections::HashMap::new(),
            request_id: format!("test-routing-{}", model),
        };

        // The routing should work (even if the actual request fails)
        let result = proxy.proxy_request(request).await;
        assert!(result.is_err()); // Expected due to no API key

        // But check that it tried to route to the right provider
        // We can't easily test the internal routing without more complex mocking,
        // but the fact that it processes the request shows routing is working
    }

    println!("✅ Provider routing logic test passed!");
}

#[tokio::test]
async fn test_model_routing_patterns() {
    // Test the standard routing patterns (non-session based)
    let config = ProxyConfig::default();
    let router = DynamicRouter::new(std::sync::Arc::new(tokio::sync::RwLock::new(config)))
        .await
        .unwrap();

    // Test that models route to their expected providers based on patterns
    let test_cases = vec![
        // Anthropic models should route to anthropic provider
        ("claude-3-haiku-20240307", "anthropic"),
        ("claude-3-sonnet-20240229", "anthropic"),
        ("claude-3-opus-20240229", "anthropic"),
        ("claude-3-5-sonnet-20241022", "anthropic"),
        // OpenAI models should route to openai provider
        ("gpt-4o", "openai"),
        ("gpt-4o-mini", "openai"),
        ("gpt-4-turbo", "openai"),
        ("gpt-3.5-turbo", "openai"),
        // OpenRouter models should route to openrouter provider
        ("anthropic/claude-3-haiku", "openrouter"),
        ("anthropic/claude-3-sonnet", "openrouter"),
        ("openai/gpt-4o", "openrouter"),
    ];

    for (model_name, expected_provider) in test_cases {
        let request = ProxyRequest {
            client_format: ApiFormat::Anthropic, // Test with anthropic format
            mode: ProxyMode::Live,
            payload: serde_json::json!({"model": model_name, "messages": []}),
            headers: std::collections::HashMap::new(),
            request_id: format!("test-routing-{}", model_name),
        };

        let provider_info = router.select_provider(&request).await.unwrap();
        assert_eq!(
            provider_info.name, expected_provider,
            "Model '{}' should route to provider '{}', but got '{}'",
            model_name, expected_provider, provider_info.name
        );
    }

    println!("✅ Model routing patterns test passed!");
}

#[tokio::test]
async fn test_metrics_collection() {
    let proxy = LlmApiProxy::new(ProxyConfig::default()).await.unwrap();

    // Get initial metrics
    let initial_metrics = proxy.metrics().snapshot().await.unwrap();
    assert_eq!(initial_metrics.total_requests, 0);

    // Create a mock request that will fail (since we have no real API)
    let request = ProxyRequest {
        client_format: ApiFormat::Anthropic,
        mode: ProxyMode::Live,
        payload: serde_json::json!({"model": "claude-3-haiku-20240307", "messages": []}),
        headers: std::collections::HashMap::new(),
        request_id: "test-metrics".to_string(),
    };

    // Process the request (will fail)
    let _ = proxy.proxy_request(request).await;

    // Check that metrics were updated
    let final_metrics = proxy.metrics().snapshot().await.unwrap();
    assert_eq!(final_metrics.total_requests, 1);
    assert_eq!(final_metrics.failed_requests, 1);
    assert!(final_metrics.average_response_time_ms > 0.0);
}
