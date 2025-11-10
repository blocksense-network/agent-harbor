// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Cursor CLI Agent Facade
//!
//! This crate re-exports the Cursor agent implementation from the ah-agents
//! monolith crate behind the `cursor-cli` feature, providing a lightweight
//! dependency surface for consumers that only need Cursor.

// Re-export Cursor-specific types and traits
pub use ah_agents::cursor::CursorAgent;
pub use ah_agents::{
    AgentError, AgentEvent, AgentExecutor, AgentLaunchConfig, AgentResult, AgentVersion,
};

/// Convenience constructor
pub fn cursor() -> CursorAgent {
    CursorAgent::new()
}
