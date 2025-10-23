// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! AI Coding Agent Abstraction Layer
//!
//! This crate provides a unified interface for interacting with various AI coding agents
//! such as Claude Code, Codex CLI, Gemini CLI, and others.
//!
//! # Features
//!
//! The crate uses feature gates to enable specific agent backends:
//!
//! - `claude` - Claude Code agent
//! - `codex` - OpenAI Codex CLI agent
//! - `gemini` - Google Gemini CLI agent
//! - `opencode` - OpenCode agent
//! - `qwen` - Qwen Code agent
//! - `cursor-cli` - Cursor CLI agent
//! - `goose` - Goose agent
//! - `copilot-cli` - GitHub Copilot CLI agent
//! - `crush` - Crush agent
//! - `groq` - Groq Code CLI agent
//! - `amp` - Amp agent
//! - `windsurf` - Windsurf agent
//!
//! # Example
//!
//! ```no_run
//! use ah_agents::{AgentExecutor, AgentLaunchConfig};
//! use std::path::PathBuf;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Get an agent executor (e.g., Claude Code)
//!     #[cfg(feature = "claude")]
//!     {
//!         let agent = ah_agents::claude();
//!
//!         // Detect version
//!         let version = agent.detect_version().await?;
//!         println!("Claude version: {}", version.version);
//!
//!         // Launch agent
//!         let config = AgentLaunchConfig::new("Fix the bug in main.rs", "/tmp/agent-home")
//!             .api_server("http://localhost:18080");
//!
//!         let mut child = agent.launch(config).await?;
//!         let status = child.wait().await?;
//!         println!("Agent exited with status: {}", status);
//!     }
//!
//!     Ok(())
//! }
//! ```

// Public re-exports
pub mod credentials;
pub mod session;
pub mod traits;

// Agent implementations (feature-gated)
#[cfg(feature = "claude")]
pub mod claude;

#[cfg(feature = "codex")]
pub mod codex;

// Re-export core types
pub use traits::{
    AgentError, AgentEvent, AgentExecutor, AgentLaunchConfig, AgentResult, AgentVersion,
};

// Convenience constructors for each agent
#[cfg(feature = "claude")]
pub fn claude() -> claude::ClaudeAgent {
    claude::ClaudeAgent::new()
}

#[cfg(feature = "codex")]
pub fn codex() -> codex::CodexAgent {
    codex::CodexAgent::new()
}

/// Get an agent executor by name
///
/// This function returns a boxed trait object for the requested agent.
/// The agent must be enabled via feature flags.
///
/// # Example
///
/// ```
/// # #[cfg(feature = "claude")]
/// # {
/// let agent = ah_agents::agent_by_name("claude").unwrap();
/// # }
/// ```
pub fn agent_by_name(name: &str) -> Option<Box<dyn AgentExecutor>> {
    match name {
        #[cfg(feature = "claude")]
        "claude" => Some(Box::new(claude::ClaudeAgent::new())),

        #[cfg(feature = "codex")]
        "codex" => Some(Box::new(codex::CodexAgent::new())),

        _ => None,
    }
}

/// List all available agents (based on enabled features)
pub fn available_agents() -> Vec<&'static str> {
    let mut agents = Vec::new();

    #[cfg(feature = "claude")]
    agents.push("claude");

    #[cfg(feature = "codex")]
    agents.push("codex");

    #[cfg(feature = "gemini")]
    agents.push("gemini");

    #[cfg(feature = "opencode")]
    agents.push("opencode");

    #[cfg(feature = "qwen")]
    agents.push("qwen");

    #[cfg(feature = "cursor-cli")]
    agents.push("cursor-cli");

    #[cfg(feature = "goose")]
    agents.push("goose");

    #[cfg(feature = "copilot-cli")]
    agents.push("copilot-cli");

    #[cfg(feature = "crush")]
    agents.push("crush");

    #[cfg(feature = "groq")]
    agents.push("groq");

    #[cfg(feature = "amp")]
    agents.push("amp");

    #[cfg(feature = "windsurf")]
    agents.push("windsurf");

    agents
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_available_agents() {
        let agents = available_agents();
        assert!(!agents.is_empty());

        #[cfg(feature = "claude")]
        assert!(agents.contains(&"claude"));

        #[cfg(feature = "codex")]
        assert!(agents.contains(&"codex"));
    }

    #[cfg(feature = "claude")]
    #[test]
    fn test_claude_constructor() {
        let agent = claude();
        assert_eq!(agent.name(), "claude");
    }

    #[cfg(feature = "codex")]
    #[test]
    fn test_codex_constructor() {
        let agent = codex();
        assert_eq!(agent.name(), "codex");
    }

    #[cfg(feature = "claude")]
    #[test]
    fn test_agent_by_name_claude() {
        let agent = agent_by_name("claude");
        assert!(agent.is_some());
        assert_eq!(agent.unwrap().name(), "claude");
    }

    #[test]
    fn test_agent_by_name_unknown() {
        let agent = agent_by_name("unknown-agent");
        assert!(agent.is_none());
    }
}
