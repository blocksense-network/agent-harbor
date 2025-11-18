// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! UI-related configuration types

use ah_domain_types::{AgentChoice, ExperimentalFeature};
use serde::{Deserialize, Serialize};

/// Root-level UI configuration that gets flattened into the main config.
/// This contains all top-level keys related to UI and general application settings.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct UiRoot {
    /// Default UI to launch with bare `ah`
    pub ui: Option<Ui>,
    /// Alternative way to specify default UI (for backward compatibility)
    pub ui_default: Option<String>,
    /// Enable/disable site automation
    pub browser_automation: Option<bool>,
    /// Preferred agent browser profile name
    pub browser_profile: Option<String>,
    /// Optional default ChatGPT username
    pub chatgpt_username: Option<String>,
    /// Default Codex workspace
    pub codex_workspace: Option<String>,
    /// Remote server reference or URL
    pub remote_server: Option<String>,
    /// TUI symbol style - auto-detected based on terminal capabilities
    #[serde(rename = "tui-font-style")]
    pub tui_font_style: Option<String>,
    /// TUI font name for advanced terminal font customization
    #[serde(rename = "tui-font")]
    pub tui_font: Option<String>,
    /// Log level
    pub log_level: Option<String>,
    /// Terminal multiplexer
    pub terminal_multiplexer: Option<String>,
    /// Editor
    pub editor: Option<String>,
    /// WebUI service base URL
    pub service_base_url: Option<String>,
    /// Default agent selections for task creation
    pub default_agents: Option<Vec<AgentChoice>>,
    /// Experimental features to enable (can be overridden by CLI)
    pub experimental_features: Option<Vec<ExperimentalFeature>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum Ui {
    Tui,
    Webui,
}
