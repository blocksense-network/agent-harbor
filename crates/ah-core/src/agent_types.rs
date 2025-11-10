// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Agent type definitions shared across the codebase

/// Supported agent types
#[derive(Clone, Debug, PartialEq)]
pub enum AgentType {
    /// Mock agent for testing
    Mock,
    /// OpenAI Codex CLI agent
    Codex,
    /// Anthropic Claude Code agent
    Claude,
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

impl std::fmt::Display for AgentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentType::Mock => write!(f, "mock"),
            AgentType::Codex => write!(f, "codex"),
            AgentType::Claude => write!(f, "claude"),
            AgentType::Gemini => write!(f, "gemini"),
            AgentType::Opencode => write!(f, "opencode"),
            AgentType::Qwen => write!(f, "qwen"),
            AgentType::CursorCli => write!(f, "cursor-cli"),
            AgentType::Goose => write!(f, "goose"),
        }
    }
}
