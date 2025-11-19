// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Agent binary representation and utilities

use ah_domain_types::AgentSoftware;
use which::which;

/// Represents an agent binary that exists on the system PATH
#[derive(Clone, Debug)]
pub struct AgentBinary {
    /// Path to the agent binary
    pub path: std::path::PathBuf,
    /// Type of the agent
    pub agent_type: AgentSoftware,
    /// Version string of the agent
    pub version: String,
}

impl AgentBinary {
    /// Attempts to locate and create an AgentBinary for the given agent type
    /// Returns None if the binary is not found in PATH or version detection fails
    pub fn from_agent_type(agent_type: &AgentSoftware) -> Option<Self> {
        let binary_name = match agent_type {
            AgentSoftware::Codex => "codex",
            AgentSoftware::Claude => "claude",
            AgentSoftware::Copilot => "copilot",
            AgentSoftware::Gemini => "gemini",
            AgentSoftware::Opencode => "opencode",
            AgentSoftware::Qwen => "qwen",
            AgentSoftware::CursorCli => "cursor",
            AgentSoftware::Goose => "goose",
        };

        // Check if binary exists in PATH
        let path = which(binary_name).ok()?;

        // Get version (agent-specific logic)
        let version = match agent_type {
            AgentSoftware::Claude => {
                std::process::Command::new(&path)
                    .arg("--version")
                    .output()
                    .ok()
                    .and_then(|output| {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        let version_output = if !stdout.trim().is_empty() {
                            stdout.to_string()
                        } else {
                            stderr.to_string()
                        };
                        // Extract version using regex (similar to ClaudeAgent::parse_version)
                        regex::Regex::new(r"(\d+\.\d+\.\d+)")
                            .unwrap()
                            .captures(&version_output)
                            .and_then(|caps| caps.get(1))
                            .map(|m| m.as_str().to_string())
                    })
                    .unwrap_or_else(|| "unknown".to_string())
            }
            _ => "unknown".to_string(), // Other agents don't have version detection yet
        };

        Some(AgentBinary {
            path,
            agent_type: agent_type.clone(),
            version,
        })
    }

    /// Returns the tools profile name used by the mock server
    pub fn tools_profile(&self) -> &str {
        match self.agent_type {
            AgentSoftware::Claude => "claude",
            AgentSoftware::Codex => "codex",
            AgentSoftware::Gemini => "gemini",
            AgentSoftware::Opencode => "opencode",
            AgentSoftware::Qwen => "qwen",
            AgentSoftware::CursorCli => "cursor-cli",
            AgentSoftware::Copilot => "copilot",
            AgentSoftware::Goose => "goose",
        }
    }
}
