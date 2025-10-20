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
