// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Agent-related domain types
//!
//! Types related to AI agents, models, and their configurations.

use serde::{Deserialize, Serialize};

use crate::experimental_features::ExperimentalFeature;

/// Supported agent software types
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
pub enum AgentSoftware {
    /// OpenAI Codex CLI agent
    Codex,
    /// Anthropic Claude Code agent
    Claude,
    /// GitHub Copilot CLI agent
    Copilot,
    /// Google Gemini CLI agent
    Gemini,
    /// OpenCode agent
    Opencode,
    /// Qwen Code agent
    Qwen,
    /// Cursor CLI agent
    CursorCli,
    /// Goose agent
    Goose,
}

impl std::fmt::Display for AgentSoftware {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.cli_arg())
    }
}

impl AgentSoftware {
    /// Get the CLI argument string for this agent software (lowercase)
    pub fn cli_arg(&self) -> &'static str {
        match self {
            AgentSoftware::Codex => "codex",
            AgentSoftware::Claude => "claude",
            AgentSoftware::Copilot => "copilot",
            AgentSoftware::Gemini => "gemini",
            AgentSoftware::Opencode => "opencode",
            AgentSoftware::Qwen => "qwen",
            AgentSoftware::CursorCli => "cursor-cli",
            AgentSoftware::Goose => "goose",
        }
    }
}

/// Software and version combination for an agent
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct AgentSoftwareBuild {
    /// The software type
    pub software: AgentSoftware,
    /// Version string (e.g., "latest", "1.0.0", "sonnet", "gpt-5")
    #[serde(default = "default_version")]
    pub version: String,
}

/// Agent configuration for task execution
#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema, validator::Validate,
)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct AgentChoice {
    /// The agent software and version
    pub agent: AgentSoftwareBuild,
    /// Model identifier string (e.g., "sonnet", "gpt-5", "claude-3.5-sonnet")
    pub model: String,
    #[serde(default = "default_count")]
    #[validate(range(min = 1, message = "Count must be at least 1"))]
    pub count: usize,
    #[serde(skip_serializing_if = "std::collections::HashMap::is_empty", default)]
    pub settings: std::collections::HashMap<String, serde_json::Value>,
    /// Display name for UI purposes (optional, will be derived if not provided)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

fn default_version() -> String {
    "latest".to_string()
}

fn default_count() -> usize {
    1
}

impl AgentChoice {
    /// Get the display name for this agent, either from the display_name field or derived from software and model
    pub fn display_name(&self) -> String {
        self.display_name
            .clone()
            .unwrap_or_else(|| format!("{:?} {}", self.agent.software, self.model))
    }
}

/// Specific capabilities that agents can have
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentCapability {
    /// TODO
    /// These are placeholder example capabilities.
    /// We are not using them to make any decisions yet.
    /// Feel free to empty the list once we start making actual decisions based
    /// on real differences in the agent software that affect Agent Harbor.

    /// Code generation capabilities
    CodeGeneration,
    /// File editing capabilities
    FileEditing,
    /// Terminal/shell access
    TerminalAccess,
    /// Autonomous execution without user intervention
    AutonomousExecution,
    /// Search and replace operations
    SearchReplace,
    /// Code review and analysis
    CodeReview,
    /// Test generation and execution
    TestGeneration,
    /// Documentation generation
    DocumentationGeneration,
    /// Multi-file operations
    MultiFileOperations,
    /// Interactive debugging
    InteractiveDebugging,
}

/// Agent capabilities and metadata
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct AgentCapabilities {
    /// Supported model identifiers
    pub supported_models: Vec<String>,
    /// Whether this agent supports multi-instance execution
    #[serde(default)]
    pub supports_multi_instance: bool,
    /// Whether this agent supports custom settings
    #[serde(default)]
    pub supports_custom_settings: bool,
    /// Agent-specific capabilities
    #[serde(default)]
    pub capabilities: Vec<AgentCapability>,
}

/// Agent metadata including capabilities and configuration defaults
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct AgentMetadata {
    /// The agent software and version
    pub agent: AgentSoftwareBuild,
    /// Display name for UI purposes
    pub display_name: String,
    /// Description of what this agent does
    pub description: String,
    /// Whether this agent is experimental
    #[serde(default)]
    pub experimental: bool,
    /// Agent capabilities
    pub capabilities: AgentCapabilities,
    /// Default model to use if not specified
    pub default_model: String,
    /// Default instance count
    #[serde(default = "default_count")]
    pub default_count: usize,
    /// Default settings
    #[serde(skip_serializing_if = "std::collections::HashMap::is_empty", default)]
    pub default_settings: std::collections::HashMap<String, serde_json::Value>,
    /// Settings schema reference (JSON Schema URL)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings_schema_ref: Option<String>,
}

/// Agent catalog containing available agents with metadata
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct AgentCatalog {
    /// Available agents with their metadata
    pub agents: Vec<AgentMetadata>,
    /// Last updated timestamp (Unix timestamp)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<i64>,
    /// Source of the catalog (e.g., "local", "remote", "merged")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

impl AgentMetadata {
    /// Create an AgentChoice from this metadata with default settings
    pub fn to_agent_choice(&self) -> AgentChoice {
        AgentChoice {
            agent: self.agent.clone(),
            model: self.default_model.clone(),
            count: self.default_count,
            settings: self.default_settings.clone(),
            display_name: Some(self.display_name.clone()),
        }
    }

    /// Check if this agent matches the given software and version
    pub fn matches(&self, software: &AgentSoftware, version: &str) -> bool {
        self.agent.software == *software && self.agent.version == version
    }

    /// Get all supported model identifiers for this agent
    pub fn supported_models(&self) -> &[String] {
        &self.capabilities.supported_models
    }

    /// Check if this experimental agent is enabled by the given experimental features
    pub fn is_enabled_by_features(&self, enabled_features: &[ExperimentalFeature]) -> bool {
        if !self.experimental {
            return true; // Non-experimental agents are always enabled
        }

        // Map agent software to experimental feature
        let required_feature = match self.agent.software {
            AgentSoftware::Gemini => Some(ExperimentalFeature::Gemini),
            AgentSoftware::CursorCli => Some(ExperimentalFeature::CursorCli),
            AgentSoftware::Goose => Some(ExperimentalFeature::Goose),
            _ => None, // Non-experimental agents
        };

        required_feature.map_or(false, |feature| enabled_features.contains(&feature))
    }
}

impl AgentCatalog {
    /// Create an empty catalog
    pub fn empty() -> Self {
        Self {
            agents: Vec::new(),
            last_updated: None,
            source: None,
        }
    }

    /// Find an agent by software and version
    pub fn find_agent(&self, software: &AgentSoftware, version: &str) -> Option<&AgentMetadata> {
        self.agents.iter().find(|agent| agent.matches(software, version))
    }

    /// Get all non-experimental agents
    pub fn stable_agents(&self) -> Vec<&AgentMetadata> {
        self.agents.iter().filter(|agent| !agent.experimental).collect()
    }

    /// Get all experimental agents
    pub fn experimental_agents(&self) -> Vec<&AgentMetadata> {
        self.agents.iter().filter(|agent| agent.experimental).collect()
    }

    /// Merge two catalogs, with later catalogs taking precedence
    pub fn merge(mut self, other: Self) -> Self {
        // Create a map of existing agents by key (software + version)
        let mut agent_map: std::collections::HashMap<(AgentSoftware, String), AgentMetadata> = self
            .agents
            .into_iter()
            .map(|agent| {
                (
                    (agent.agent.software.clone(), agent.agent.version.clone()),
                    agent,
                )
            })
            .collect();

        // Add/override with agents from the other catalog
        for agent in other.agents {
            agent_map.insert(
                (agent.agent.software.clone(), agent.agent.version.clone()),
                agent,
            );
        }

        Self {
            agents: agent_map.into_values().collect(),
            last_updated: Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64,
            ),
            source: Some("merged".to_string()),
        }
    }

    /// Filter agents based on experimental flag
    pub fn filter_by_experimental(&self, include_experimental: bool) -> Self {
        Self {
            agents: if include_experimental {
                self.agents.clone()
            } else {
                self.agents.iter().filter(|agent| !agent.experimental).cloned().collect()
            },
            last_updated: self.last_updated,
            source: self.source.clone(),
        }
    }

    /// Filter agents based on enabled experimental features
    pub fn filter_by_experimental_features(
        &self,
        enabled_features: &[ExperimentalFeature],
    ) -> Self {
        Self {
            agents: self
                .agents
                .iter()
                .filter(|agent| {
                    !agent.experimental || agent.is_enabled_by_features(enabled_features)
                })
                .cloned()
                .collect(),
            last_updated: self.last_updated,
            source: self.source.clone(),
        }
    }
}
