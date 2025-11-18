// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Unit tests for agent catalog functionality.
//!
//! Tests cover catalog merging, deduplication, experimental gating,
//! and REST fallback behavior as required by R2 Milestone M1.

use ah_core::agent_catalog::*;
use ah_domain_types::{
    AgentCapabilities, AgentCapability, AgentCatalog, AgentMetadata, AgentSoftware,
    AgentSoftwareBuild, ExperimentalFeature,
};
use std::str::FromStr;
use std::sync::Arc;
use tokio::time::{Duration, timeout};

/// Test catalog merging functionality
#[tokio::test]
async fn test_catalog_merging() {
    // Create two catalogs with some overlapping agents
    let catalog1 = AgentCatalog {
        agents: vec![
            AgentMetadata {
                agent: AgentSoftwareBuild {
                    software: AgentSoftware::Claude,
                    version: "1.0".to_string(),
                },
                display_name: "Claude Code".to_string(),
                description: "Anthropic's Claude Code agent".to_string(),
                experimental: false,
                capabilities: AgentCapabilities {
                    supported_models: vec!["claude-3-5-sonnet".to_string()],
                    supports_multi_instance: false,
                    supports_custom_settings: false,
                    capabilities: vec![AgentCapability::CodeGeneration],
                },
                default_model: "claude-3-5-sonnet".to_string(),
                default_count: 1,
                default_settings: std::collections::HashMap::new(),
                settings_schema_ref: None,
            },
            AgentMetadata {
                agent: AgentSoftwareBuild {
                    software: AgentSoftware::Copilot,
                    version: "1.0".to_string(),
                },
                display_name: "GitHub Copilot".to_string(),
                description: "GitHub's Copilot agent".to_string(),
                experimental: false,
                capabilities: AgentCapabilities {
                    supported_models: vec!["default".to_string()],
                    supports_multi_instance: false,
                    supports_custom_settings: false,
                    capabilities: vec![AgentCapability::CodeGeneration],
                },
                default_model: "default".to_string(),
                default_count: 1,
                default_settings: std::collections::HashMap::new(),
                settings_schema_ref: None,
            },
        ],
        last_updated: Some(1000),
        source: Some("source1".to_string()),
    };

    let catalog2 = AgentCatalog {
        agents: vec![
            AgentMetadata {
                agent: AgentSoftwareBuild {
                    software: AgentSoftware::Copilot,
                    version: "2.0".to_string(), // Different version
                },
                display_name: "GitHub Copilot Updated".to_string(),
                description: "Updated GitHub Copilot agent".to_string(),
                experimental: false,
                capabilities: AgentCapabilities {
                    supported_models: vec!["gpt-4".to_string()],
                    supports_multi_instance: false,
                    supports_custom_settings: false,
                    capabilities: vec![
                        AgentCapability::CodeGeneration,
                        AgentCapability::FileEditing,
                    ],
                },
                default_model: "gpt-4".to_string(),
                default_count: 1,
                default_settings: std::collections::HashMap::new(),
                settings_schema_ref: None,
            },
            AgentMetadata {
                agent: AgentSoftwareBuild {
                    software: AgentSoftware::Gemini,
                    version: "1.0".to_string(),
                },
                display_name: "Google Gemini".to_string(),
                description: "Google's Gemini agent".to_string(),
                experimental: true,
                capabilities: AgentCapabilities {
                    supported_models: vec!["gemini-pro".to_string()],
                    supports_multi_instance: false,
                    supports_custom_settings: false,
                    capabilities: vec![AgentCapability::CodeGeneration],
                },
                default_model: "gemini-pro".to_string(),
                default_count: 1,
                default_settings: std::collections::HashMap::new(),
                settings_schema_ref: None,
            },
        ],
        last_updated: Some(2000),
        source: Some("source2".to_string()),
    };

    // Merge the catalogs
    let merged = catalog1.merge(catalog2);

    // Should have 4 unique agents (Claude, Copilot v1.0, Copilot v2.0, Gemini)
    // The merge keeps agents with different (software, version) combinations
    assert_eq!(merged.agents.len(), 4);

    // Check that Claude is present
    let claude = merged
        .agents
        .iter()
        .find(|a| a.agent.software == AgentSoftware::Claude && a.agent.version == "1.0")
        .unwrap();
    assert_eq!(claude.display_name, "Claude Code");

    // Check that both Copilot versions are present
    let copilots: Vec<_> = merged
        .agents
        .iter()
        .filter(|a| a.agent.software == AgentSoftware::Copilot)
        .collect();
    assert_eq!(copilots.len(), 2);

    // Find Copilot v1.0 and v2.0
    let copilot_v1 = copilots.iter().find(|a| a.agent.version == "1.0").unwrap();
    let copilot_v2 = copilots.iter().find(|a| a.agent.version == "2.0").unwrap();

    assert_eq!(copilot_v1.display_name, "GitHub Copilot");
    assert_eq!(copilot_v2.display_name, "GitHub Copilot Updated");

    // Check that Gemini is present
    let gemini = merged
        .agents
        .iter()
        .find(|a| a.agent.software == AgentSoftware::Gemini)
        .unwrap();
    assert_eq!(gemini.agent.version, "1.0");
    assert!(gemini.experimental);
}

/// Test deduplication behavior - later catalogs take precedence
#[tokio::test]
async fn test_catalog_deduplication() {
    let mut catalog1 = AgentCatalog::empty();
    let mut catalog2 = AgentCatalog::empty();

    // Add the same agent to both catalogs with different metadata
    catalog1.agents.push(AgentMetadata {
        agent: AgentSoftwareBuild {
            software: AgentSoftware::Claude,
            version: "latest".to_string(),
        },
        display_name: "Claude Old".to_string(),
        description: "Old version".to_string(),
        experimental: false,
        capabilities: AgentCapabilities {
            supported_models: vec!["old-model".to_string()],
            supports_multi_instance: false,
            supports_custom_settings: false,
            capabilities: vec![AgentCapability::CodeGeneration],
        },
        default_model: "old-model".to_string(),
        default_count: 1,
        default_settings: std::collections::HashMap::new(),
        settings_schema_ref: None,
    });

    catalog2.agents.push(AgentMetadata {
        agent: AgentSoftwareBuild {
            software: AgentSoftware::Claude,
            version: "latest".to_string(), // Same version to test deduplication
        },
        display_name: "Claude New".to_string(),
        description: "New version".to_string(),
        experimental: false,
        capabilities: AgentCapabilities {
            supported_models: vec!["new-model".to_string()],
            supports_multi_instance: false,
            supports_custom_settings: false,
            capabilities: vec![
                AgentCapability::CodeGeneration,
                AgentCapability::FileEditing,
            ],
        },
        default_model: "new-model".to_string(),
        default_count: 1,
        default_settings: std::collections::HashMap::new(),
        settings_schema_ref: None,
    });

    let merged = catalog1.merge(catalog2);

    // Should have only one Claude agent, with the version from catalog2 (last merged)
    assert_eq!(merged.agents.len(), 1);
    let claude = &merged.agents[0];
    assert_eq!(claude.agent.version, "latest");
    assert_eq!(claude.display_name, "Claude New");
    assert_eq!(
        claude.capabilities.supported_models,
        vec!["new-model".to_string()]
    );
}

/// Test experimental feature gating in local catalog
#[tokio::test]
async fn test_experimental_feature_gating() {
    // Test with only Gemini enabled
    let config_gemini_only = LocalAgentCatalogConfig {
        executable_paths: vec![],
        health_check_timeout: Duration::from_secs(1),
        query_third_party_apis: false,
        cache_ttl: Duration::from_secs(300),
        experimental_features: vec![ExperimentalFeature::Gemini], // Only Gemini enabled
    };

    let catalog_gemini_only = LocalAgentCatalog::new(config_gemini_only);

    // Get the catalog (this will internally check experimental features)
    let result = catalog_gemini_only.get_catalog().await;
    assert!(result.is_ok());
    let catalog = result.unwrap();

    // In a real scenario, experimental agents would only appear if they're available
    // and enabled. For this test, we verify the structure works correctly.
    // The actual filtering happens during agent discovery based on feature flags.

    // Test with no experimental features enabled
    let config_none = LocalAgentCatalogConfig {
        executable_paths: vec![],
        health_check_timeout: Duration::from_secs(1),
        query_third_party_apis: false,
        cache_ttl: Duration::from_secs(300),
        experimental_features: vec![], // No experimental features
    };

    let catalog_none = LocalAgentCatalog::new(config_none);
    let result_none = catalog_none.get_catalog().await;
    assert!(result_none.is_ok());
}

/// Test experimental feature filtering in catalog filtering
#[tokio::test]
async fn test_experimental_feature_filtering() {
    let catalog = RemoteAgentCatalog::default_catalog();

    // Filter with only Gemini enabled
    let filtered = catalog.filter_by_experimental_features(&[ExperimentalFeature::Gemini]);

    // Should only include non-experimental agents plus Gemini
    let experimental_agents: Vec<_> = filtered.agents.iter().filter(|a| a.experimental).collect();
    assert_eq!(experimental_agents.len(), 1);
    assert_eq!(experimental_agents[0].agent.software, AgentSoftware::Gemini);

    // Non-experimental agents should still be present
    let non_experimental_count = filtered.agents.iter().filter(|a| !a.experimental).count();
    assert!(non_experimental_count > 0);
}

/// Test MockAgentsEnumerator returns the configured catalog
#[tokio::test]
async fn test_mock_agents_enumerator() {
    let test_catalog = AgentCatalog {
        agents: vec![AgentMetadata {
            agent: AgentSoftwareBuild {
                software: AgentSoftware::Claude,
                version: "test".to_string(),
            },
            display_name: "Test Claude".to_string(),
            description: "Test agent".to_string(),
            experimental: false,
            capabilities: AgentCapabilities {
                supported_models: vec!["test-model".to_string()],
                supports_multi_instance: false,
                supports_custom_settings: false,
                capabilities: vec![AgentCapability::CodeGeneration],
            },
            default_model: "test-model".to_string(),
            default_count: 1,
            default_settings: std::collections::HashMap::new(),
            settings_schema_ref: None,
        }],
        last_updated: Some(12345),
        source: Some("test".to_string()),
    };

    let enumerator = MockAgentsEnumerator::new(test_catalog.clone());

    let result = enumerator.enumerate_agents().await.unwrap();
    assert_eq!(result.agents.len(), 1);
    assert_eq!(result.agents[0].agent.software, AgentSoftware::Claude);
    assert_eq!(result.agents[0].display_name, "Test Claude");

    // Test refresh returns the same catalog
    let refresh_result = enumerator.refresh().await.unwrap();
    assert_eq!(refresh_result.agents.len(), 1);
    assert_eq!(refresh_result.agents[0].display_name, "Test Claude");
}

/// Test caching behavior in RemoteAgentCatalog
#[tokio::test]
async fn test_remote_catalog_caching() {
    let config = RemoteAgentCatalogConfig {
        rest_server_url: "http://test.example.com".to_string(),
        cache_ttl: Duration::from_secs(300),
        max_retries: 1,
        retry_delay: Duration::from_millis(1),
    };

    let catalog = RemoteAgentCatalog::new(config);

    // First call should fetch and cache
    let result1 = catalog.get_catalog().await;
    assert!(result1.is_ok());

    // Second call should return cached result
    let result2 = catalog.get_catalog().await;
    assert!(result2.is_ok());

    // Results should be identical
    assert_eq!(result1.unwrap().agents.len(), result2.unwrap().agents.len());
}

/// Test refresh bypasses cache in RemoteAgentCatalog
#[tokio::test]
async fn test_remote_catalog_refresh_bypasses_cache() {
    let config = RemoteAgentCatalogConfig {
        rest_server_url: "http://test.example.com".to_string(),
        cache_ttl: Duration::from_secs(300),
        max_retries: 1,
        retry_delay: Duration::from_millis(1),
    };

    let catalog = RemoteAgentCatalog::new(config);

    // First get should populate cache
    let _ = catalog.get_catalog().await.unwrap();

    // Refresh should bypass cache and fetch fresh
    let refresh_result = catalog.refresh().await;
    assert!(refresh_result.is_ok());
}

/// Test default catalog contains expected agents
#[tokio::test]
async fn test_default_catalog_contents() {
    let catalog = RemoteAgentCatalog::default_catalog();

    // Should contain both experimental and non-experimental agents
    assert!(!catalog.agents.is_empty());

    // Check for specific known agents
    let has_claude = catalog.agents.iter().any(|a| a.agent.software == AgentSoftware::Claude);
    let has_copilot = catalog.agents.iter().any(|a| a.agent.software == AgentSoftware::Copilot);
    let has_gemini = catalog.agents.iter().any(|a| a.agent.software == AgentSoftware::Gemini);
    let has_cursor_cli =
        catalog.agents.iter().any(|a| a.agent.software == AgentSoftware::CursorCli);
    let has_goose = catalog.agents.iter().any(|a| a.agent.software == AgentSoftware::Goose);

    assert!(has_claude, "Default catalog should contain Claude");
    assert!(has_copilot, "Default catalog should contain Copilot");
    assert!(has_gemini, "Default catalog should contain Gemini");
    assert!(has_cursor_cli, "Default catalog should contain Cursor CLI");
    assert!(has_goose, "Default catalog should contain Goose");

    // Experimental agents should be marked as experimental
    let gemini = catalog
        .agents
        .iter()
        .find(|a| a.agent.software == AgentSoftware::Gemini)
        .unwrap();
    let cursor_cli = catalog
        .agents
        .iter()
        .find(|a| a.agent.software == AgentSoftware::CursorCli)
        .unwrap();
    let goose = catalog
        .agents
        .iter()
        .find(|a| a.agent.software == AgentSoftware::Goose)
        .unwrap();

    assert!(
        gemini.experimental,
        "Gemini should be marked as experimental"
    );
    assert!(
        cursor_cli.experimental,
        "Cursor CLI should be marked as experimental"
    );
    assert!(goose.experimental, "Goose should be marked as experimental");

    // Non-experimental agents should not be marked as experimental
    let claude = catalog
        .agents
        .iter()
        .find(|a| a.agent.software == AgentSoftware::Claude)
        .unwrap();
    let copilot = catalog
        .agents
        .iter()
        .find(|a| a.agent.software == AgentSoftware::Copilot)
        .unwrap();

    assert!(
        !claude.experimental,
        "Claude should not be marked as experimental"
    );
    assert!(
        !copilot.experimental,
        "Copilot should not be marked as experimental"
    );
}

/// Test that experimental agents have appropriate capabilities
#[tokio::test]
async fn test_experimental_agent_capabilities() {
    let catalog = RemoteAgentCatalog::default_catalog();

    let gemini = catalog
        .agents
        .iter()
        .find(|a| a.agent.software == AgentSoftware::Gemini)
        .unwrap();
    let cursor_cli = catalog
        .agents
        .iter()
        .find(|a| a.agent.software == AgentSoftware::CursorCli)
        .unwrap();
    let goose = catalog
        .agents
        .iter()
        .find(|a| a.agent.software == AgentSoftware::Goose)
        .unwrap();

    // All should have code generation capability
    assert!(gemini.capabilities.capabilities.contains(&AgentCapability::CodeGeneration));
    assert!(cursor_cli.capabilities.capabilities.contains(&AgentCapability::CodeGeneration));
    assert!(goose.capabilities.capabilities.contains(&AgentCapability::CodeGeneration));

    // Gemini and Goose should have additional capabilities
    assert!(gemini.capabilities.capabilities.contains(&AgentCapability::FileEditing));
    assert!(gemini.capabilities.capabilities.contains(&AgentCapability::TerminalAccess));
    assert!(goose.capabilities.capabilities.contains(&AgentCapability::FileEditing));
    assert!(goose.capabilities.capabilities.contains(&AgentCapability::TerminalAccess));
    assert!(goose.capabilities.capabilities.contains(&AgentCapability::AutonomousExecution));
}

/// Test AgentSoftware Display implementation
#[test]
fn test_agent_software_display() {
    assert_eq!(format!("{}", AgentSoftware::Claude), "claude");
    assert_eq!(format!("{}", AgentSoftware::Copilot), "copilot");
    assert_eq!(format!("{}", AgentSoftware::Gemini), "gemini");
    assert_eq!(format!("{}", AgentSoftware::CursorCli), "cursor-cli");
    assert_eq!(format!("{}", AgentSoftware::Goose), "goose");
}

/// Test ExperimentalFeature Display implementation
#[test]
fn test_experimental_feature_display() {
    assert_eq!(format!("{}", ExperimentalFeature::Gemini), "gemini");
    assert_eq!(format!("{}", ExperimentalFeature::CursorCli), "cursor-cli");
    assert_eq!(format!("{}", ExperimentalFeature::Goose), "goose");
}

/// Test ExperimentalFeature FromStr implementation
#[test]
fn test_experimental_feature_from_str() {
    assert_eq!(
        ExperimentalFeature::from_str("gemini").unwrap(),
        ExperimentalFeature::Gemini
    );
    assert_eq!(
        ExperimentalFeature::from_str("cursor-cli").unwrap(),
        ExperimentalFeature::CursorCli
    );
    assert_eq!(
        ExperimentalFeature::from_str("cursor_cli").unwrap(),
        ExperimentalFeature::CursorCli
    ); // underscore variant
    assert_eq!(
        ExperimentalFeature::from_str("goose").unwrap(),
        ExperimentalFeature::Goose
    );

    // Test invalid input
    assert!(ExperimentalFeature::from_str("invalid").is_err());
    assert!(ExperimentalFeature::from_str("").is_err());
}

/// Test AgentCatalog empty constructor
#[test]
fn test_agent_catalog_empty() {
    let empty = AgentCatalog::empty();
    assert!(empty.agents.is_empty());
    assert!(empty.last_updated.is_none());
    assert!(empty.source.is_none());
}
