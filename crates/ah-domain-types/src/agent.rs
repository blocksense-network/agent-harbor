// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Agent-related domain types
//!
//! Types related to AI agents, models, and their configurations.

use serde::{Deserialize, Serialize};

/// Supported agent software types
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
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
