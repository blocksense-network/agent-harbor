//! Codex CLI Agent Facade
//!
//! This crate provides a lightweight facade for using only the Codex CLI agent
//! from the ah-agents monolith crate.
//!
//! # Example
//!
//! ```no_run
//! use ah_agent_codex::{CodexAgent, AgentExecutor, AgentLaunchConfig, AgentResult};
//!
//! #[tokio::main]
//! async fn main() -> AgentResult<()> {
//!     let agent = CodexAgent::new();
//!
//!     // Detect version
//!     let version = agent.detect_version().await?;
//!     println!("Codex version: {}", version.version);
//!
//!     // Launch agent
//!     let config = AgentLaunchConfig::new("Implement feature X", "/tmp/agent-home");
//!     let mut child = agent.launch(config).await?;
//!     let status = child.wait().await?;
//!
//!     Ok(())
//! }
//! ```

// Re-export all Codex-specific types and traits
pub use ah_agents::codex::CodexAgent;
pub use ah_agents::{
    AgentError, AgentEvent, AgentExecutor, AgentLaunchConfig, AgentResult, AgentVersion,
};

// Convenience constructor
pub fn codex() -> CodexAgent {
    CodexAgent::new()
}
