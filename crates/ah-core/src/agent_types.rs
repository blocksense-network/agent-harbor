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
