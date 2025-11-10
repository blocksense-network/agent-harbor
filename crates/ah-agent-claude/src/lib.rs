// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Claude Code Agent Facade
//!
//! This crate provides a lightweight facade for using only the Claude Code agent
//! from the ah-agents monolith crate.
//!
//! # Example
//!
//! ```no_run
//! use ah_agent_claude::{ClaudeAgent, AgentExecutor, AgentLaunchConfig, AgentResult};
//!
//! #[tokio::main]
//! async fn main() -> AgentResult<()> {
//!     let agent = ClaudeAgent::new();
//!
//!     // Detect version
//!     let version = agent.detect_version().await?;
//!     println!("Claude version: {}", version.version);
//!
//!     // Launch agent
//!     let config = AgentLaunchConfig::new("/tmp/agent-home").prompt("Fix bug in main.rs");
//!     let mut child = agent.launch(config).await?;
//!     let status = child.wait().await?;
//!
//!     Ok(())
//! }
//! ```

// Re-export all Claude-specific types and traits
pub use ah_agents::claude::ClaudeAgent;
pub use ah_agents::{
    AgentError, AgentEvent, AgentExecutor, AgentLaunchConfig, AgentResult, AgentVersion,
};

// Convenience constructor
pub fn claude() -> ClaudeAgent {
    ClaudeAgent::new()
}
