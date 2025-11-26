// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Root Scenario-Format structure (see specs/Public/Scenario-Format.md).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scenario {
    pub name: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub terminal_ref: Option<String>,
    pub compat: Option<CompatibilityFlags>,
    /// Optional natural-language description used for fuzzy matching.
    pub initial_prompt: Option<String>,
    pub repo: Option<RepoConfig>,
    pub ah: Option<AhConfig>,
    pub server: Option<ServerConfig>,
    #[serde(default)]
    pub timeline: Vec<TimelineEvent>,
    /// Legacy timeline syntax (used by existing ACP fixtures).
    #[serde(default, rename = "events")]
    pub legacy_events: Vec<LegacyScenarioEvent>,
    /// Legacy assertions section used by ACP fixtures.
    #[serde(default, rename = "assertions")]
    pub legacy_assertions: Vec<LegacyAssertion>,
    pub expect: Option<ExpectConfig>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatibilityFlags {
    pub allow_inline_terminal: Option<bool>,
    pub allow_type_steps: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoConfig {
    pub init: Option<bool>,
    pub branch: Option<String>,
    pub dir: Option<String>,
    pub files: Option<Vec<FileSeed>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FileSeed {
    pub path: String,
    pub contents: serde_yaml::Value,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AhConfig {
    pub cmd: String,
    #[serde(default)]
    pub flags: Vec<String>,
    pub env: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerConfig {
    pub mode: Option<String>,
    pub llm_api_style: Option<String>,
    pub coalesce_thinking_with_tool_use: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum TimelineEvent {
    /// Unified llmResponse section (preferred)
    LlmResponse {
        #[serde(rename = "llmResponse")]
        llm_response: Vec<ResponseElement>,
    },
    /// Legacy individual event objects (e.g. `{ "think": [...] }`)
    Legacy(HashMap<String, serde_yaml::Value>),
    /// Assertion
    Assert { assert: AssertionData },
    /// Control events defined with `{ "type": "...", ... }`
    Control {
        #[serde(rename = "type")]
        event_type: String,
        #[serde(flatten)]
        data: HashMap<String, serde_yaml::Value>,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ResponseElement {
    #[serde(rename = "think")]
    Think { think: Vec<ThinkingStep> },
    #[serde(rename = "assistant")]
    Assistant { assistant: Vec<AssistantStep> },
    #[serde(rename = "agentToolUse")]
    AgentToolUse { agent_tool_use: ToolUseData },
    #[serde(rename = "agentEdits")]
    AgentEdits { agent_edits: FileEditData },
    #[serde(rename = "toolResult")]
    ToolResult { tool_result: ToolResultData },
    #[serde(rename = "error")]
    Error { error: ErrorData },
}

/// `(milliseconds, text)`
#[derive(Debug, Clone, Deserialize)]
pub struct ThinkingStep(pub u64, pub String);

/// `(milliseconds, text)`
#[derive(Debug, Clone, Deserialize)]
pub struct AssistantStep(pub u64, pub String);

/// `(milliseconds, text)`
#[derive(Debug, Clone, Deserialize)]
pub struct ProgressStep(pub u64, pub String);

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolUseData {
    #[serde(rename = "toolName")]
    pub tool_name: String,
    #[serde(default)]
    pub args: HashMap<String, serde_yaml::Value>,
    pub progress: Option<Vec<ProgressStep>>,
    pub result: Option<serde_yaml::Value>,
    pub status: Option<String>,
    pub tool_execution: Option<ToolExecution>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolExecution {
    pub start_time_ms: Option<u64>,
    pub events: Vec<ToolExecutionEvent>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolExecutionEvent {
    #[serde(rename = "type")]
    pub kind: String,
    pub time_ms: Option<u64>,
    pub content: Option<String>,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResultData {
    #[serde(rename = "toolCallId")]
    pub tool_call_id: String,
    pub content: serde_yaml::Value,
    #[serde(default)]
    pub is_error: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEditData {
    pub path: String,
    pub lines_added: u32,
    pub lines_removed: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpectConfig {
    pub exit_code: Option<i32>,
    pub artifacts: Option<Vec<ArtifactExpectation>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactExpectation {
    #[serde(rename = "type")]
    pub artifact_type: String,
    pub pattern: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssertionData {
    pub fs: Option<FilesystemAssertions>,
    pub text: Option<TextAssertions>,
    pub json: Option<JsonAssertions>,
    pub git: Option<GitAssertions>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilesystemAssertions {
    pub exists: Option<Vec<String>>,
    pub not_exists: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextAssertions {
    pub contains: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonAssertions {
    pub file: Option<Vec<JsonFileAssertion>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LegacyScenarioEvent {
    #[serde(rename = "at_ms")]
    pub at_ms: u64,
    pub kind: String,
    pub value: Option<String>,
    pub message: Option<String>,
    pub text: Option<String>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LegacyAssertion {
    pub kind: String,
    pub event: Option<LegacyAssertEvent>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LegacyAssertEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub status: Option<String>,
    pub message_contains: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonFileAssertion {
    pub path: String,
    pub pointer: String,
    pub equals: serde_yaml::Value,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitAssertions {
    pub commit: Option<Vec<GitCommitAssertion>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitCommitAssertion {
    pub message_contains: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorData {
    #[serde(rename = "errorType")]
    pub error_type: String,
    pub status_code: Option<u16>,
    pub message: String,
    pub details: Option<serde_yaml::Value>,
    pub retry_after_seconds: Option<u32>,
}

/// Helper for deserialising `{ "userInputs": [[100, "text"]], "target": "tui" }`
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserInputsEvent {
    #[serde(rename = "userInputs")]
    pub inputs: Vec<(u64, String)>,
    pub target: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserEditsEvent {
    #[serde(rename = "userEdits")]
    pub user_edits: HashMap<String, serde_yaml::Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserCommandEvent {
    #[serde(rename = "userCommand")]
    pub user_command: HashMap<String, serde_yaml::Value>,
}
