// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Agent catalog and enumeration functionality
//!
//! This module provides the core functionality for discovering, caching, and
//! enumerating available agents from various sources including local config,
//! remote REST APIs, and built-in defaults.
//!
//! ## Architecture Overview
//!
//! The agent enumeration system supports multiple data sources:
//!
//! 1. **LocalAgentCatalog**: Discovers agents by querying locally installed agent software
//!    - Interacts with installed CLI tools to determine available models and capabilities
//!    - May query third-party servers associated with agent software for model lists
//!    - Checks actual software availability similar to `ah health` command
//!
//! 2. **RemoteAgentCatalog**: Fetches agent metadata from REST API endpoints
//!    - Retrieves agent capabilities and configurations from remote servers
//!    - Supports caching with configurable TTL and retry behavior
//!    - Handles authentication and error scenarios
//!
//! ## Agent Availability and Health Checking
//!
//! Similar to the `ah health` command, the AgentEnumerator checks actual software
//! availability by:
//! - Verifying agent executables are installed and accessible
//! - Testing basic functionality (e.g., version commands)
//! - Querying supported models from local installations or remote endpoints
//! - Filtering out unavailable agents from the catalog
//!
//! ## Model Discovery
//!
//! Model lists for agents can be obtained through multiple mechanisms:
//! - **Local Querying**: Direct interaction with installed agent software
//! - **Third-party APIs**: Querying servers associated with agent vendors
//! - **Configuration**: Static model lists from configuration files
//! - **REST API**: Dynamic model lists from remote Agent Harbor servers
//!
//! ## Configuration
//!
//! Different catalog implementations accept their own configuration objects:
//! - `LocalAgentCatalogConfig`: Paths, timeouts, health check settings
//! - `RemoteAgentCatalogConfig`: REST URLs, authentication, cache settings, retries
//!
//! ## Local vs Remote Mode Behavior
//!
//! The agent catalog system behaves differently in local vs remote mode:
//!
//! ### Local Mode
//! - Uses `LocalAgentCatalog` to discover actually installed agent software
//! - Creates `AgentBinary` objects for found executables, caching paths and versions
//! - Applies experimental feature filtering based on client configuration
//! - Only shows agents that are both available locally AND enabled by experimental flags
//! - `AgentBinary` objects guarantee paths have been verified and avoid repeated PATH searches
//!
//! ### Remote Mode
//! - Uses `RemoteAgentCatalog` to fetch agent metadata from REST API
//! - The server provides the complete catalog without client-side filtering
//! - Client experimental flags have no effect - server decides what's available
//! - Falls back to local catalog if remote server is unavailable
//!
//! ### REST Server Behavior
//! - The REST server uses its own `LocalAgentCatalog` instance to generate catalog responses
//! - Server applies its own experimental feature configuration to determine available agents
//! - Clients receive the server's filtered view, not their own experimental preferences
//!
//! ## Experimental Features
//!
//! Experimental agents are gated behind feature flags with different behavior by mode:
//!
//! - **Local Mode**: Client experimental flags control which experimental agents are shown
//!   from the locally discovered set. Non-experimental agents are always available if installed.
//!
//! - **Remote Mode**: Client experimental flags are ignored. The server determines which
//!   experimental agents are available based on its own configuration.
//!
//! - **Configuration Precedence**: CLI flags > user config > repository config > defaults
//!
//! Individual experimental agents are controlled by `ExperimentalFeature` enum values:
//! - `Gemini`: Google Gemini CLI agent
//! - `CursorCli`: Cursor CLI agent
//! - `Goose`: Block's Goose agent

use ah_domain_types::{
    AgentCapabilities, AgentCapability, AgentCatalog, AgentMetadata, AgentSoftware,
    AgentSoftwareBuild, ExperimentalFeature,
};
use async_trait::async_trait;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Result type for agent catalog operations
pub type AgentCatalogResult<T> = Result<T, AgentCatalogError>;

/// Errors that can occur during agent catalog operations
#[derive(Debug, thiserror::Error)]
pub enum AgentCatalogError {
    #[error("REST API error: {0}")]
    RestApi(#[from] ah_rest_client::RestClientError),
    #[error("Configuration error: {0}")]
    Config(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON parsing error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Cache TTL expired")]
    CacheExpired,
}

/// Cached catalog entry with metadata
#[derive(Debug, Clone)]
struct CachedCatalog {
    catalog: ah_domain_types::AgentCatalog,
    fetched_at: Instant,
}

/// Trait for agent catalog implementations
#[async_trait]
pub trait AgentCatalogProvider {
    /// Get the agent catalog
    async fn get_catalog(&self) -> AgentCatalogResult<ah_domain_types::AgentCatalog>;

    /// Refresh the catalog (force fetch from source)
    async fn refresh(&self) -> AgentCatalogResult<ah_domain_types::AgentCatalog>;

    /// Check if the catalog source is available
    async fn is_available(&self) -> bool;
}

/// Configuration for local agent catalog behavior
#[derive(Debug, Clone)]
pub struct LocalAgentCatalogConfig {
    /// Paths to search for agent executables
    pub executable_paths: Vec<std::path::PathBuf>,
    /// Timeout for health checks
    pub health_check_timeout: Duration,
    /// Whether to query third-party APIs for model lists
    pub query_third_party_apis: bool,
    /// Cache TTL for local catalog data
    pub cache_ttl: Duration,
    /// Enabled experimental features
    pub experimental_features: Vec<ExperimentalFeature>,
}

/// Configuration for remote agent catalog behavior
#[derive(Debug, Clone)]
pub struct RemoteAgentCatalogConfig {
    /// REST server URL for remote agent discovery
    pub rest_server_url: String,
    /// Cache TTL for remote catalog data
    pub cache_ttl: Duration,
    /// Maximum number of retry attempts for REST calls
    pub max_retries: usize,
    /// Base delay between retries (exponential backoff)
    pub retry_delay: Duration,
}

/// Remote agent catalog with caching and REST API integration
#[derive(Debug)]
pub struct RemoteAgentCatalog {
    config: RemoteAgentCatalogConfig,
    _rest_client: ah_rest_client::RestClient,
    cache: RwLock<Option<CachedCatalog>>,
}

#[async_trait]
impl AgentsEnumerator for RemoteAgentCatalog {
    async fn enumerate_agents(&self) -> AgentCatalogResult<ah_domain_types::AgentCatalog> {
        self.get_catalog().await
    }

    async fn refresh(&self) -> AgentCatalogResult<ah_domain_types::AgentCatalog> {
        AgentCatalogProvider::refresh(self).await
    }
}

impl RemoteAgentCatalog {
    /// Create a new remote agent catalog
    pub fn new(config: RemoteAgentCatalogConfig) -> Self {
        let url = url::Url::parse(&config.rest_server_url).expect("Invalid REST server URL");
        let rest_client = ah_rest_client::RestClient::new(
            url,
            ah_rest_client::AuthConfig {
                method: ah_rest_client::AuthMethod::None,
                tenant_id: None,
            },
        );

        Self {
            config,
            _rest_client: rest_client,
            cache: RwLock::new(None),
        }
    }

    /// Check if the remote server is available
    pub async fn is_available(&self) -> bool {
        // Simple connectivity check - try to reach the server
        // For now, return false to force local catalog usage
        // TODO: Implement actual connectivity check
        false
    }

    /// Refresh the catalog (force fetch from source)
    pub async fn refresh(&self) -> AgentCatalogResult<ah_domain_types::AgentCatalog> {
        self.fetch_from_rest().await
    }

    /// Fetch catalog from all available sources and merge them
    async fn fetch_catalog(&self) -> AgentCatalogResult<ah_domain_types::AgentCatalog> {
        let mut catalogs = Vec::new();

        // Add local default catalog
        catalogs.push(Self::default_catalog());

        // Add remote catalog if configured
        if let Some(catalog) = self.fetch_remote_catalog().await? {
            catalogs.push(catalog);
        }

        // Merge all catalogs
        let mut merged = ah_domain_types::AgentCatalog::empty();
        for catalog in catalogs {
            merged = merged.merge(catalog);
        }

        // In remote mode, the server provides the catalog as-is
        // Client experimental flags do not filter the server's catalog
        Ok(merged)
    }

    /// Fetch catalog from remote REST API with retry logic
    async fn fetch_from_rest(&self) -> AgentCatalogResult<ah_domain_types::AgentCatalog> {
        let mut last_error = None;

        for attempt in 0..self.config.max_retries {
            match self.fetch_remote_catalog().await {
                Ok(Some(catalog)) => return Ok(catalog),
                Ok(None) => return Ok(ah_domain_types::AgentCatalog::empty()),
                Err(e) => {
                    last_error = Some(e);
                    if attempt < self.config.max_retries - 1 {
                        tokio::time::sleep(self.config.retry_delay * (attempt as u32 + 1)).await;
                    }
                }
            }
        }

        Err(last_error.unwrap())
    }

    /// Fetch catalog from remote REST API
    async fn fetch_remote_catalog(
        &self,
    ) -> AgentCatalogResult<Option<ah_domain_types::AgentCatalog>> {
        // This is a placeholder - in practice, this would call the REST API
        // For now, return None to indicate no remote catalog available
        Ok(None)
    }

    /// Convert REST API capabilities to catalog
    #[allow(dead_code)]
    fn capabilities_to_catalog(
        &self,
        _capabilities: Vec<ah_rest_api_contract::AgentCapability>,
    ) -> ah_domain_types::AgentCatalog {
        // Placeholder implementation - in practice, this would convert REST API capabilities
        // to domain types. For now, return empty catalog.
        ah_domain_types::AgentCatalog::empty()
    }

    /// Convert REST API capability to agent metadata
    #[allow(dead_code)]
    fn capability_to_metadata(
        capability: ah_rest_api_contract::AgentCapability,
    ) -> Option<ah_domain_types::AgentMetadata> {
        // Map agent_type to AgentSoftware
        let software = match capability.agent_type.as_str() {
            "claude-code" => AgentSoftware::Claude,
            "copilot" => AgentSoftware::Copilot,
            "gemini" => AgentSoftware::Gemini,
            "cursor-cli" => AgentSoftware::CursorCli,
            "goose" => AgentSoftware::Goose,
            _ => return None,
        };

        let version = capability.versions.first().cloned().unwrap_or_else(|| "latest".to_string());

        Some(ah_domain_types::AgentMetadata {
            agent: ah_domain_types::AgentSoftwareBuild { software, version },
            display_name: capability.agent_type.clone(),
            description: format!("{} agent", capability.agent_type),
            experimental: capability.settings_schema_ref.is_some(),
            capabilities: ah_domain_types::AgentCapabilities {
                supported_models: vec!["default".to_string()],
                supports_multi_instance: false,
                supports_custom_settings: capability.settings_schema_ref.is_some(),
                capabilities: vec![
                    ah_domain_types::AgentCapability::CodeGeneration,
                    ah_domain_types::AgentCapability::FileEditing,
                ],
            },
            default_model: "default".to_string(),
            default_count: 1,
            default_settings: std::collections::HashMap::new(),
            settings_schema_ref: capability.settings_schema_ref,
        })
    }

    /// Create default catalog with built-in agents
    pub fn default_catalog() -> ah_domain_types::AgentCatalog {
        let agents = vec![
            ah_domain_types::AgentMetadata {
                agent: ah_domain_types::AgentSoftwareBuild {
                    software: AgentSoftware::Claude,
                    version: "latest".to_string(),
                },
                display_name: "Claude Code".to_string(),
                description: "Anthropic's Claude Code agent".to_string(),
                experimental: false,
                capabilities: ah_domain_types::AgentCapabilities {
                    supported_models: vec![
                        "claude-3-5-sonnet".to_string(),
                        "claude-3-haiku".to_string(),
                    ],
                    supports_multi_instance: false,
                    supports_custom_settings: false,
                    capabilities: vec![
                        ah_domain_types::AgentCapability::CodeGeneration,
                        ah_domain_types::AgentCapability::FileEditing,
                    ],
                },
                default_model: "claude-3-5-sonnet".to_string(),
                default_count: 1,
                default_settings: std::collections::HashMap::new(),
                settings_schema_ref: None,
            },
            ah_domain_types::AgentMetadata {
                agent: ah_domain_types::AgentSoftwareBuild {
                    software: AgentSoftware::Codex,
                    version: "latest".to_string(),
                },
                display_name: "GitHub Copilot".to_string(),
                description: "GitHub's Copilot CLI agent".to_string(),
                experimental: false,
                capabilities: ah_domain_types::AgentCapabilities {
                    supported_models: vec!["default".to_string()],
                    supports_multi_instance: false,
                    supports_custom_settings: false,
                    capabilities: vec![
                        ah_domain_types::AgentCapability::CodeGeneration,
                        ah_domain_types::AgentCapability::FileEditing,
                    ],
                },
                default_model: "default".to_string(),
                default_count: 1,
                default_settings: std::collections::HashMap::new(),
                settings_schema_ref: None,
            },
            ah_domain_types::AgentMetadata {
                agent: ah_domain_types::AgentSoftwareBuild {
                    software: AgentSoftware::Copilot,
                    version: "latest".to_string(),
                },
                display_name: "GitHub Copilot".to_string(),
                description: "GitHub's Copilot CLI agent".to_string(),
                experimental: false,
                capabilities: ah_domain_types::AgentCapabilities {
                    supported_models: vec!["default".to_string()],
                    supports_multi_instance: false,
                    supports_custom_settings: false,
                    capabilities: vec![
                        ah_domain_types::AgentCapability::CodeGeneration,
                        ah_domain_types::AgentCapability::FileEditing,
                    ],
                },
                default_model: "default".to_string(),
                default_count: 1,
                default_settings: std::collections::HashMap::new(),
                settings_schema_ref: None,
            },
            ah_domain_types::AgentMetadata {
                agent: ah_domain_types::AgentSoftwareBuild {
                    software: AgentSoftware::Gemini,
                    version: "latest".to_string(),
                },
                display_name: "Google Gemini".to_string(),
                description: "Google's Gemini CLI agent for code assistance".to_string(),
                experimental: true,
                capabilities: ah_domain_types::AgentCapabilities {
                    supported_models: vec!["gemini-pro".to_string(), "gemini-flash".to_string()],
                    supports_multi_instance: true,
                    supports_custom_settings: true,
                    capabilities: vec![
                        ah_domain_types::AgentCapability::CodeGeneration,
                        ah_domain_types::AgentCapability::FileEditing,
                        ah_domain_types::AgentCapability::TerminalAccess,
                    ],
                },
                default_model: "gemini-pro".to_string(),
                default_count: 1,
                default_settings: std::collections::HashMap::new(),
                settings_schema_ref: Some("/api/v1/schemas/agents/gemini.json".to_string()),
            },
            ah_domain_types::AgentMetadata {
                agent: ah_domain_types::AgentSoftwareBuild {
                    software: AgentSoftware::CursorCli,
                    version: "latest".to_string(),
                },
                display_name: "Cursor CLI".to_string(),
                description: "Cursor's command-line agent for AI-assisted development".to_string(),
                experimental: true,
                capabilities: ah_domain_types::AgentCapabilities {
                    supported_models: vec!["default".to_string()],
                    supports_multi_instance: false,
                    supports_custom_settings: false,
                    capabilities: vec![
                        ah_domain_types::AgentCapability::CodeGeneration,
                        ah_domain_types::AgentCapability::FileEditing,
                    ],
                },
                default_model: "default".to_string(),
                default_count: 1,
                default_settings: std::collections::HashMap::new(),
                settings_schema_ref: None,
            },
            ah_domain_types::AgentMetadata {
                agent: ah_domain_types::AgentSoftwareBuild {
                    software: AgentSoftware::Goose,
                    version: "latest".to_string(),
                },
                display_name: "Goose".to_string(),
                description: "Block's Goose agent for autonomous software development".to_string(),
                experimental: true,
                capabilities: ah_domain_types::AgentCapabilities {
                    supported_models: vec!["default".to_string()],
                    supports_multi_instance: false,
                    supports_custom_settings: true,
                    capabilities: vec![
                        ah_domain_types::AgentCapability::CodeGeneration,
                        ah_domain_types::AgentCapability::FileEditing,
                        ah_domain_types::AgentCapability::TerminalAccess,
                        ah_domain_types::AgentCapability::AutonomousExecution,
                    ],
                },
                default_model: "default".to_string(),
                default_count: 1,
                default_settings: std::collections::HashMap::new(),
                settings_schema_ref: Some("/api/v1/schemas/agents/goose.json".to_string()),
            },
        ];

        ah_domain_types::AgentCatalog {
            agents,
            last_updated: Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64,
            ),
            source: Some("built-in".to_string()),
        }
    }
}

#[async_trait]
impl AgentCatalogProvider for RemoteAgentCatalog {
    async fn get_catalog(&self) -> AgentCatalogResult<ah_domain_types::AgentCatalog> {
        // Check cache first
        if let Some(cached) = self.cache.read().await.as_ref() {
            if cached.fetched_at.elapsed() < self.config.cache_ttl {
                return Ok(cached.catalog.clone());
            }
        }

        // Fetch fresh catalog
        let catalog = self.fetch_catalog().await?;

        // Update cache
        let cached = CachedCatalog {
            catalog: catalog.clone(),
            fetched_at: Instant::now(),
        };
        *self.cache.write().await = Some(cached);

        Ok(catalog)
    }

    async fn refresh(&self) -> AgentCatalogResult<ah_domain_types::AgentCatalog> {
        // Clear cache
        *self.cache.write().await = None;

        // Fetch fresh catalog
        self.get_catalog().await
    }

    async fn is_available(&self) -> bool {
        RemoteAgentCatalog::is_available(self).await
    }
}

/// Convert REST API capabilities to domain catalog
fn _capabilities_to_catalog(
    _capabilities: Vec<ah_rest_api_contract::AgentCapability>,
) -> ah_domain_types::AgentCatalog {
    // Placeholder implementation - in practice, this would convert REST API capabilities
    // to domain types. For now, return empty catalog.
    ah_domain_types::AgentCatalog::empty()
}

/// Get the default built-in catalog
fn _default_catalog() -> AgentCatalog {
    let mut agents = Vec::new();

    // Claude models
    for model in &["sonnet", "haiku", "opus"] {
        let display_name = match *model {
            "sonnet" => "Claude Sonnet".to_string(),
            "haiku" => "Claude Haiku".to_string(),
            "opus" => "Claude Opus".to_string(),
            _ => format!("Claude {}", model),
        };

        agents.push(AgentMetadata {
            agent: AgentSoftwareBuild {
                software: AgentSoftware::Claude,
                version: "latest".to_string(),
            },
            display_name,
            description: "Anthropic's Claude Code agent for software development".to_string(),
            experimental: false,
            capabilities: AgentCapabilities {
                supported_models: vec![model.to_string()],
                supports_multi_instance: true,
                supports_custom_settings: true,
                capabilities: vec![
                    ah_domain_types::AgentCapability::CodeGeneration,
                    ah_domain_types::AgentCapability::FileEditing,
                    ah_domain_types::AgentCapability::TerminalAccess,
                    ah_domain_types::AgentCapability::SearchReplace,
                ],
            },
            default_model: model.to_string(),
            default_count: 1,
            default_settings: std::collections::HashMap::new(),
            settings_schema_ref: Some("/api/v1/schemas/agents/claude-code.json".to_string()),
        });
    }

    // Codex models
    for model in &["gpt-5.1-codex", "gpt-5.1-codex-mini", "gpt-5.1"] {
        let display_name = match *model {
            "gpt-5.1" => "GPT-5.1".to_string(),
            "gpt-5.1-codex" => "GPT-5.1 Codex".to_string(),
            "gpt-5.1-codex-mini" => "GPT-5.1Codex (Mini)".to_string(),
            "gpt-5.1-codex-low" => "GPT-5.1 Codex Low".to_string(),
            "gpt-5.1-codex-medium" => "GPT-5.1 Codex Medium".to_string(),
            "gpt-5.1-codex-high" => "GPT-5.1 Codex High".to_string(),
            _ => format!("Codex {}", model),
        };

        agents.push(AgentMetadata {
            agent: AgentSoftwareBuild {
                software: AgentSoftware::Codex,
                version: "latest".to_string(),
            },
            display_name,
            description: "OpenAI's Codex CLI agent for code assistance".to_string(),
            experimental: false,
            capabilities: AgentCapabilities {
                supported_models: vec![model.to_string()],
                supports_multi_instance: true,
                supports_custom_settings: true,
                capabilities: vec![
                    AgentCapability::CodeGeneration,
                    AgentCapability::FileEditing,
                    AgentCapability::TerminalAccess,
                ],
            },
            default_model: model.to_string(),
            default_count: 1,
            default_settings: std::collections::HashMap::new(),
            settings_schema_ref: Some("/api/v1/schemas/agents/codex.json".to_string()),
        });
    }

    // Copilot
    agents.push(AgentMetadata {
        agent: AgentSoftwareBuild {
            software: AgentSoftware::Copilot,
            version: "latest".to_string(),
        },
        display_name: "GitHub Copilot".to_string(),
        description: "GitHub Copilot CLI agent for AI-powered code assistance".to_string(),
        experimental: true,
        capabilities: AgentCapabilities {
            supported_models: vec!["default".to_string()],
            supports_multi_instance: false,
            supports_custom_settings: false,
            capabilities: vec![
                AgentCapability::CodeGeneration,
                AgentCapability::FileEditing,
            ],
        },
        default_model: "default".to_string(),
        default_count: 1,
        default_settings: std::collections::HashMap::new(),
        settings_schema_ref: None,
    });

    // Experimental agents
    for model in &["gemini-pro", "gemini-flash"] {
        agents.push(AgentMetadata {
            agent: AgentSoftwareBuild {
                software: AgentSoftware::Gemini,
                version: "latest".to_string(),
            },
            display_name: format!("Google Gemini ({})", model),
            description: "Google's Gemini CLI agent for code assistance".to_string(),
            experimental: true,
            capabilities: AgentCapabilities {
                supported_models: vec![model.to_string()],
                supports_multi_instance: true,
                supports_custom_settings: true,
                capabilities: vec![
                    AgentCapability::CodeGeneration,
                    AgentCapability::FileEditing,
                    AgentCapability::TerminalAccess,
                ],
            },
            default_model: model.to_string(),
            default_count: 1,
            default_settings: std::collections::HashMap::new(),
            settings_schema_ref: Some("/api/v1/schemas/agents/gemini.json".to_string()),
        });
    }

    agents.push(AgentMetadata {
        agent: AgentSoftwareBuild {
            software: AgentSoftware::CursorCli,
            version: "latest".to_string(),
        },
        display_name: "Cursor CLI".to_string(),
        description: "Cursor's command-line agent for AI-assisted development".to_string(),
        experimental: true,
        capabilities: AgentCapabilities {
            supported_models: vec!["default".to_string()],
            supports_multi_instance: false,
            supports_custom_settings: false,
            capabilities: vec![
                AgentCapability::CodeGeneration,
                AgentCapability::FileEditing,
            ],
        },
        default_model: "default".to_string(),
        default_count: 1,
        default_settings: std::collections::HashMap::new(),
        settings_schema_ref: None,
    });

    agents.push(AgentMetadata {
        agent: AgentSoftwareBuild {
            software: AgentSoftware::Goose,
            version: "latest".to_string(),
        },
        display_name: "Goose".to_string(),
        description: "Block's Goose agent for autonomous software development".to_string(),
        experimental: true,
        capabilities: AgentCapabilities {
            supported_models: vec!["default".to_string()],
            supports_multi_instance: false,
            supports_custom_settings: true,
            capabilities: vec![
                AgentCapability::CodeGeneration,
                AgentCapability::FileEditing,
                AgentCapability::TerminalAccess,
                AgentCapability::AutonomousExecution,
            ],
        },
        default_model: "default".to_string(),
        default_count: 1,
        default_settings: std::collections::HashMap::new(),
        settings_schema_ref: Some("/api/v1/schemas/agents/goose.json".to_string()),
    });

    AgentCatalog {
        agents,
        last_updated: Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
        ),
        source: Some("built-in".to_string()),
    }
}

/// Local agent catalog that discovers agents by checking installed software
///
/// Uses AgentBinary objects to represent discovered agents, ensuring that:
/// - Binary paths are only resolved once per agent
/// - The presence of an AgentBinary guarantees the executable was found in PATH
/// - Version information is cached along with the path
#[derive(Debug)]
pub struct LocalAgentCatalog {
    config: LocalAgentCatalogConfig,
    cache: RwLock<Option<CachedCatalog>>,
    /// Discovered agent binaries (only constructed for agents that are actually available)
    /// Key: AgentSoftware, Value: AgentBinary (guarantees path has been verified)
    discovered_binaries:
        RwLock<std::collections::HashMap<AgentSoftware, crate::agent_binary::AgentBinary>>,
}

impl LocalAgentCatalog {
    /// Create a new local agent catalog
    pub fn new(config: LocalAgentCatalogConfig) -> Self {
        Self {
            config,
            cache: RwLock::new(None),
            discovered_binaries: RwLock::new(std::collections::HashMap::new()),
        }
    }

    /// Check if an agent software is available locally and return AgentBinary if found
    async fn check_agent_availability(
        &self,
        software: &AgentSoftware,
    ) -> Option<crate::agent_binary::AgentBinary> {
        // First check if we already have a discovered binary for this software
        {
            let binaries = self.discovered_binaries.read().await;
            if let Some(binary) = binaries.get(software) {
                return Some(binary.clone());
            }
        }

        // Check if experimental features are enabled for experimental agents
        match software {
            AgentSoftware::Copilot
            | AgentSoftware::Gemini
            | AgentSoftware::CursorCli
            | AgentSoftware::Goose => {
                // For experimental agents, check both availability and feature flag
                if !self.is_experimental_feature_enabled(software) {
                    return None;
                }
            }
            _ => {} // Non-experimental agents don't need feature flag check
        }

        // Try to create an AgentBinary for this software
        if let Some(binary) = crate::agent_binary::AgentBinary::from_agent_type(software) {
            // Store the discovered binary to avoid future path searches
            let mut binaries = self.discovered_binaries.write().await;
            binaries.insert(software.clone(), binary.clone());
            Some(binary)
        } else {
            None
        }
    }

    /// Check if an experimental feature is enabled for the given agent software
    fn is_experimental_feature_enabled(&self, software: &AgentSoftware) -> bool {
        let feature = match software {
            AgentSoftware::Copilot => ExperimentalFeature::Copilot,
            AgentSoftware::Gemini => ExperimentalFeature::Gemini,
            AgentSoftware::CursorCli => ExperimentalFeature::CursorCli,
            AgentSoftware::Goose => ExperimentalFeature::Goose,
            // Non-experimental agents don't have feature flags
            _ => return false,
        };
        self.config.experimental_features.contains(&feature)
    }

    /// Get a discovered agent binary by software type
    pub async fn get_agent_binary(
        &self,
        software: &AgentSoftware,
    ) -> Option<crate::agent_binary::AgentBinary> {
        let binaries = self.discovered_binaries.read().await;
        binaries.get(software).cloned()
    }

    /// Get all discovered agent binaries
    pub async fn get_all_agent_binaries(&self) -> Vec<crate::agent_binary::AgentBinary> {
        let binaries = self.discovered_binaries.read().await;
        binaries.values().cloned().collect()
    }

    /// Discover models for an agent (local discovery or third-party API)
    async fn discover_models(&self, software: &AgentSoftware) -> Vec<String> {
        // This would implement the model discovery logic:
        // - Query local agent software for supported models
        // - Call third-party APIs if configured
        // - Fall back to built-in defaults

        match software {
            AgentSoftware::Claude => vec![
                "sonnet".to_string(), // Latest Sonnet model
                "haiku".to_string(),  // Latest Haiku model
                "opus".to_string(),   // Latest Opus model
            ],
            AgentSoftware::Codex => vec![
                "gpt-5.1".to_string(),
                "gpt-5.1-codex".to_string(),
                "gpt-5.1-codex-mini".to_string(),
                "gpt-5.1-codex-low".to_string(),
                "gpt-5.1-codex-medium".to_string(),
                "gpt-5.1-codex-high".to_string(),
            ],
            AgentSoftware::Gemini => vec!["gemini-pro".to_string(), "gemini-flash".to_string()],
            _ => vec!["default".to_string()],
        }
    }

    /// Build catalog from available local agents
    async fn build_local_catalog(&self) -> AgentCatalogResult<AgentCatalog> {
        let mut agents = Vec::new();

        // Check all known agent software
        for software in [
            AgentSoftware::Claude,
            AgentSoftware::Codex,
            AgentSoftware::Copilot,
            AgentSoftware::Gemini,
            AgentSoftware::CursorCli,
            AgentSoftware::Goose,
        ]
        .iter()
        {
            if let Some(_agent_binary) = self.check_agent_availability(software).await {
                // AgentBinary is now cached and available - we can use it if needed in the future
                let models = self.discover_models(software).await;

                // Create separate AgentMetadata for each model
                for model in models {
                    let metadata = self.create_agent_metadata_for_model(software.clone(), model);
                    agents.push(metadata);
                }
            }
        }

        Ok(AgentCatalog {
            agents,
            last_updated: Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64,
            ),
            source: Some("local-discovery".to_string()),
        })
    }

    /// Create agent metadata for a discovered agent and specific model
    fn create_agent_metadata_for_model(
        &self,
        software: AgentSoftware,
        model: String,
    ) -> AgentMetadata {
        // This would build proper metadata based on discovered capabilities
        // For now, return basic metadata
        let (base_display_name, description, experimental, capabilities) = match software {
            AgentSoftware::Claude => (
                "Claude".to_string(),
                "Anthropic's Claude Code agent".to_string(),
                false,
                vec![
                    AgentCapability::CodeGeneration,
                    AgentCapability::FileEditing,
                ],
            ),
            AgentSoftware::Codex => (
                "Codex".to_string(),
                "OpenAI's Codex CLI agent".to_string(),
                false,
                vec![
                    AgentCapability::CodeGeneration,
                    AgentCapability::FileEditing,
                ],
            ),
            AgentSoftware::Copilot => (
                "Copilot".to_string(),
                "GitHub Copilot CLI agent".to_string(),
                true,
                vec![
                    AgentCapability::CodeGeneration,
                    AgentCapability::FileEditing,
                ],
            ),
            AgentSoftware::Gemini => (
                "Google Gemini".to_string(),
                "Google's Gemini CLI agent".to_string(),
                true,
                vec![
                    AgentCapability::CodeGeneration,
                    AgentCapability::FileEditing,
                    AgentCapability::TerminalAccess,
                ],
            ),
            AgentSoftware::CursorCli => (
                "Cursor CLI".to_string(),
                "Cursor's command-line agent".to_string(),
                true,
                vec![
                    AgentCapability::CodeGeneration,
                    AgentCapability::FileEditing,
                ],
            ),
            AgentSoftware::Goose => (
                "Goose".to_string(),
                "Block's Goose agent".to_string(),
                true,
                vec![
                    AgentCapability::CodeGeneration,
                    AgentCapability::FileEditing,
                    AgentCapability::TerminalAccess,
                    AgentCapability::AutonomousExecution,
                ],
            ),
            _ => (
                format!("{:?}", software),
                format!("{} agent", software),
                false,
                vec![
                    AgentCapability::CodeGeneration,
                    AgentCapability::FileEditing,
                ],
            ),
        };

        // Create display name based on model
        let display_name = match software {
            AgentSoftware::Claude => match model.as_str() {
                "sonnet" => "Claude Sonnet".to_string(),
                "haiku" => "Claude Haiku".to_string(),
                "opus" => "Claude Opus".to_string(),
                _ => format!("{} {}", base_display_name, model),
            },
            AgentSoftware::Codex => match model.as_str() {
                "gpt-5.1-codex" => "GPT-5.1 Codex (Optimized)".to_string(),
                "gpt-5.1-codex-mini" => "GPT-5.1 Codex (Mini)".to_string(),
                "gpt-5.1" => "GPT-5.1".to_string(),
                "gpt-5.1-codex-high" => "GPT-5.1 Codex (High)".to_string(),
                "gpt-5.1-codex-medium" => "GPT-5.1 Codex (Medium)".to_string(),
                "gpt-5.1-codex-low" => "GPT-5.1 Codex (Low)".to_string(),
                _ => format!("{} {}", base_display_name, model),
            },
            _ => format!("{} {}", base_display_name, model),
        };

        AgentMetadata {
            agent: AgentSoftwareBuild {
                software,
                version: "latest".to_string(),
            },
            display_name,
            description,
            experimental,
            capabilities: AgentCapabilities {
                supported_models: vec![model.clone()],
                supports_multi_instance: false,
                supports_custom_settings: false,
                capabilities,
            },
            default_model: model,
            default_count: 1,
            default_settings: std::collections::HashMap::new(),
            settings_schema_ref: None,
        }
    }
}

#[async_trait]
impl AgentCatalogProvider for LocalAgentCatalog {
    async fn get_catalog(&self) -> AgentCatalogResult<ah_domain_types::AgentCatalog> {
        // Check cache first
        if let Some(cached) = self.cache.read().await.as_ref() {
            if cached.fetched_at.elapsed() < self.config.cache_ttl {
                return Ok(cached.catalog.clone());
            }
        }

        // Build fresh catalog
        let catalog = self.build_local_catalog().await?;

        // Update cache
        let cached = CachedCatalog {
            catalog: catalog.clone(),
            fetched_at: Instant::now(),
        };
        *self.cache.write().await = Some(cached);

        Ok(catalog)
    }

    async fn refresh(&self) -> AgentCatalogResult<ah_domain_types::AgentCatalog> {
        // Clear cache
        *self.cache.write().await = None;

        // Build fresh catalog
        self.get_catalog().await
    }

    async fn is_available(&self) -> bool {
        // Local catalog is always available (may discover no agents though)
        true
    }
}

#[async_trait]
impl AgentsEnumerator for LocalAgentCatalog {
    async fn enumerate_agents(&self) -> AgentCatalogResult<ah_domain_types::AgentCatalog> {
        self.get_catalog().await
    }

    async fn refresh(&self) -> AgentCatalogResult<ah_domain_types::AgentCatalog> {
        AgentCatalogProvider::refresh(self).await
    }
}

/// Trait for enumerating available agents
#[async_trait]
pub trait AgentsEnumerator: Send + Sync {
    /// Get all available agents
    async fn enumerate_agents(&self) -> AgentCatalogResult<ah_domain_types::AgentCatalog>;

    /// Refresh the agent catalog
    async fn refresh(&self) -> AgentCatalogResult<ah_domain_types::AgentCatalog>;
}

/// Mock agent enumerator for testing
#[derive(Debug)]
pub struct MockAgentsEnumerator {
    catalog: ah_domain_types::AgentCatalog,
}

impl MockAgentsEnumerator {
    pub fn new(catalog: ah_domain_types::AgentCatalog) -> Self {
        Self { catalog }
    }
}

#[async_trait]
impl AgentsEnumerator for MockAgentsEnumerator {
    async fn enumerate_agents(&self) -> AgentCatalogResult<ah_domain_types::AgentCatalog> {
        Ok(self.catalog.clone())
    }

    async fn refresh(&self) -> AgentCatalogResult<ah_domain_types::AgentCatalog> {
        Ok(self.catalog.clone())
    }
}
