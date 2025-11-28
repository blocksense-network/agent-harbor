// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::collections::HashMap;

/// Root Scenario-Format structure (see specs/Public/Scenario-Format.md).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scenario {
    pub name: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub terminal_ref: Option<String>,
    /// Optional natural-language description used for fuzzy matching.
    pub initial_prompt: Option<String>,
    pub repo: Option<RepoConfig>,
    pub ah: Option<AhConfig>,
    pub server: Option<ServerConfig>,
    #[serde(default)]
    pub timeline: Vec<TimelineEvent>,
    pub expect: Option<ExpectConfig>,
}

impl Scenario {
    /// Compute the effective initial prompt as defined in the spec:
    /// the first `userInputs` event after `sessionStart`, otherwise the
    /// first `userInputs` event in the timeline. Falls back to the legacy
    /// `initialPrompt` field when no prompt can be derived.
    pub fn effective_initial_prompt(&self) -> Option<String> {
        let mut first_any: Option<String> = None;
        let mut first_after_boundary: Option<String> = None;
        let after_session_start = false;

        for event in &self.timeline {
            if let TimelineEvent::UserInputs { user_inputs } = event {
                for entry in user_inputs {
                    if let Some(prompt) = extract_prompt_from_input_value(&entry.input) {
                        if after_session_start && first_after_boundary.is_none() {
                            first_after_boundary = Some(prompt.clone());
                        }
                        if first_any.is_none() {
                            first_any = Some(prompt);
                        }
                        break;
                    }
                }
            }
        }

        first_after_boundary.or(first_any).or_else(|| self.initial_prompt.clone())
    }

    /// Returns true if the scenario uses any legacy timeline constructs.
    pub fn has_legacy_timeline(&self) -> bool {
        false
    }
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
    /// Unified llmResponse section
    LlmResponse {
        #[serde(rename = "llmResponse")]
        llm_response: Vec<ResponseElement>,
    },
    /// Object-based userInputs event
    UserInputs {
        #[serde(rename = "userInputs")]
        user_inputs: Vec<UserInputEntry>,
    },
    AgentToolUse {
        #[serde(rename = "agentToolUse")]
        agent_tool_use: ToolUseData,
    },
    AgentEdits {
        #[serde(rename = "agentEdits")]
        agent_edits: FileEditData,
    },
    Assert {
        assert: AssertionData,
    },
    Status {
        status: String,
    },
    AdvanceMs {
        #[serde(rename = "baseTimeDelta", alias = "advanceMs")]
        base_time_delta: u64,
    },
    Screenshot {
        screenshot: String,
    },
    Merge {
        merge: bool,
    },
    Complete {
        complete: bool,
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
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThinkingStep {
    #[serde(rename = "relativeTime", alias = "timestamp")]
    pub relative_time: u64,
    #[serde(alias = "text")]
    pub content: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AssistantStep {
    #[serde(rename = "relativeTime", alias = "timestamp")]
    pub relative_time: u64,
    #[serde(rename = "content", alias = "text")]
    pub content: serde_yaml::Value,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressStep {
    #[serde(rename = "relativeTime", alias = "timestamp")]
    pub relative_time: u64,
    #[serde(alias = "text", alias = "content")]
    pub message: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
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

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolExecution {
    pub start_time_ms: Option<u64>,
    pub events: Vec<ToolExecutionEvent>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
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

/// Object-based user input entry with relative time offset.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserInputEntry {
    #[serde(rename = "relativeTime", alias = "timestamp")]
    pub relative_time: u64,
    pub input: serde_yaml::Value,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(rename = "_meta")]
    pub meta: Option<serde_yaml::Value>,
}

pub fn extract_prompt_from_user_inputs(value: &Value) -> Option<String> {
    match value {
        Value::Sequence(seq) => {
            for entry in seq {
                if let Some(prompt) = extract_prompt_from_user_input_entry(entry) {
                    return Some(prompt);
                }
            }
            None
        }
        other => extract_prompt_from_user_input_entry(other),
    }
}

pub fn extract_prompt_from_user_input_entry(value: &Value) -> Option<String> {
    match value {
        // Legacy tuple form: [delay_ms, "prompt text"]
        Value::Sequence(items) if items.len() >= 2 => {
            items.get(1).and_then(|v| v.as_str()).map(|s| s.to_string())
        }
        Value::String(text) => Some(text.clone()),
        Value::Mapping(map) => {
            let input_key = Value::String("input".to_string());
            if let Some(input) = map.get(&input_key) {
                return extract_prompt_from_input_value(input);
            }

            // Some formats may store text directly
            map.get(Value::String("text".to_string()))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        }
        _ => None,
    }
}

pub fn extract_prompt_from_input_value(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Sequence(seq) => {
            // Rich content blocks; pick the first textual element
            for block in seq {
                if let Some(text) = extract_text_from_content_block(block) {
                    return Some(text);
                }
            }
            None
        }
        other => extract_text_from_content_block(other),
    }
}

pub fn extract_text_from_content_block(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Mapping(map) => {
            if let Some(text) = map.get(Value::String("text".to_string())).and_then(|v| v.as_str())
            {
                return Some(text.to_string());
            }

            if let Some(content) =
                map.get(Value::String("content".to_string())).and_then(|v| v.as_str())
            {
                return Some(content.to_string());
            }

            // As a fallback, serialize the block to YAML and trim whitespace
            serde_yaml::to_string(value).ok().map(|s| s.trim().to_string())
        }
        _ => None,
    }
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
