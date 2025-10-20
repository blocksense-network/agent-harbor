//! Agent binary representation and utilities

use crate::agent_types::AgentType;

/// Represents an agent binary that exists on the system PATH
#[derive(Clone, Debug)]
pub struct AgentBinary {
    /// Path to the agent binary
    pub path: std::path::PathBuf,
    /// Type of the agent
    pub agent_type: AgentType,
    /// Version string of the agent
    pub version: String,
}

impl AgentBinary {
    /// Attempts to locate and create an AgentBinary for the given agent type
    /// Returns None if the binary is not found in PATH or version detection fails
    pub fn from_agent_type(agent_type: AgentType) -> Option<Self> {
        let binary_name = match agent_type {
            AgentType::Mock => "mock-agent",
            AgentType::Codex => "codex",
            AgentType::Claude => "claude",
            AgentType::Gemini => "gemini",
            AgentType::Opencode => "opencode",
            AgentType::Qwen => "qwen",
            AgentType::CursorCli => "cursor",
            AgentType::Goose => "goose",
        };

        // Check if binary exists in PATH
        let path = std::process::Command::new("which").arg(binary_name).output().ok().and_then(
            |output| {
                if output.status.success() {
                    let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    Some(std::path::PathBuf::from(path_str))
                } else {
                    None
                }
            },
        )?;

        // Get version (agent-specific logic)
        let version = match agent_type {
            AgentType::Claude => {
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
            agent_type,
            version,
        })
    }

    /// Returns the tools profile name used by the mock server
    pub fn tools_profile(&self) -> &str {
        match self.agent_type {
            AgentType::Claude => "claude",
            AgentType::Codex => "codex",
            AgentType::Gemini => "gemini",
            AgentType::Opencode => "opencode",
            AgentType::Qwen => "qwen",
            AgentType::CursorCli => "cursor-cli",
            AgentType::Goose => "goose",
            AgentType::Mock => "mock",
        }
    }
}
