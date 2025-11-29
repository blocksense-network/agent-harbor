// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
    pub acp: Option<AcpConfig>,
    #[serde(default)]
    pub rules: Option<Rules>,
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
        let mut after_session_start = false;

        for event in &self.timeline {
            match event {
                TimelineEvent::SessionStart { .. } => {
                    after_session_start = true;
                    first_after_boundary = None; // Reset when we hit a new boundary
                }
                TimelineEvent::UserInputs { user_inputs } => {
                    for entry in user_inputs {
                        if let Some(prompt) = extract_prompt_from_input_content(&entry.input) {
                            if after_session_start && first_after_boundary.is_none() {
                                first_after_boundary = Some(prompt.clone());
                            }
                            if first_any.is_none() {
                                first_any = Some(prompt);
                            }
                            break; // Only take the first input from each userInputs event
                        }
                    }
                }
                _ => {}
            }
        }

        first_after_boundary.or(first_any).or_else(|| self.initial_prompt.clone())
    }

    /// Returns true if the scenario uses any legacy timeline constructs.
    pub fn has_legacy_timeline(&self) -> bool {
        false
    }

    /// Split timeline events into historical (pre-boundary) and live (post-boundary)
    /// segments using the first `sessionStart` marker. If no boundary is present,
    /// all events are treated as live.
    pub fn partition_by_session_start(&self) -> SessionTimeline<'_> {
        let mut historical = Vec::new();
        let mut live = Vec::new();
        let mut session_start: Option<&SessionStartData> = None;
        let mut session_start_meta: Option<&serde_yaml::Value> = None;
        let mut after_boundary = false;

        for event in &self.timeline {
            match event {
                TimelineEvent::SessionStart {
                    session_start: data,
                    meta,
                } => {
                    if session_start.is_none() {
                        session_start = Some(data);
                        session_start_meta = meta.as_ref();
                    }
                    after_boundary = true;
                }
                _ => {
                    if after_boundary {
                        live.push(event);
                    } else {
                        historical.push(event);
                    }
                }
            }
        }

        SessionTimeline {
            historical,
            live,
            session_start,
            session_start_meta,
        }
    }

    /// Validate the scenario against ACP specification requirements
    pub fn validate_acp_requirements(&self) -> Result<(), String> {
        self.validate_acp_requirements_with_base(None)
    }

    /// Validate the scenario against ACP specification requirements with optional
    /// filesystem context (used to validate referenced resources).
    pub fn validate_acp_requirements_with_base(
        &self,
        base_dir: Option<&Path>,
    ) -> Result<(), String> {
        let mut session_start_seen: Option<&SessionStartData> = None;
        let effective_prompt_caps = compute_effective_prompt_caps(&self.acp);

        // Track loadSession capability alignment with timeline boundaries.
        let _load_session_enabled = self
            .acp
            .as_ref()
            .and_then(|a| a.capabilities.as_ref())
            .and_then(|c| c.load_session)
            .unwrap_or(false);

        // Validate ACP configuration if present
        if let Some(acp) = &self.acp {
            validate_acp_config(acp)?;
        }

        // Validate content blocks in timeline
        for event in &self.timeline {
            match event {
                TimelineEvent::UserInputs { user_inputs, .. } => {
                    for input in user_inputs {
                        // Validate _meta field if present
                        if let Some(meta) = &input.meta {
                            validate_meta_field(meta)?;
                        }
                        if let Some(expected) = &input.expected_response {
                            validate_expected_prompt_response(expected)?;
                        }

                        match &input.input {
                            InputContent::Rich(blocks) => {
                                for block in blocks {
                                    validation::validate_rich_content_block(block, base_dir)?;
                                }
                                // Validate capability requirements for rich content
                                validation::validate_content_capabilities(
                                    blocks,
                                    &Some(effective_prompt_caps.clone()),
                                )?;
                            }
                            InputContent::Text(_) => {
                                // Text content is always allowed
                            }
                        }
                    }
                }
                TimelineEvent::LlmResponse {
                    llm_response, meta, ..
                } => {
                    // Validate _meta field if present
                    if let Some(meta) = meta {
                        validate_meta_field(meta)?;
                    }

                    for element in llm_response {
                        if let ResponseElement::Assistant { assistant } = element {
                            for step in assistant {
                                match &step.content {
                                    ContentBlock::Rich(block) => {
                                        validation::validate_rich_content_block(block, base_dir)?;
                                        validation::validate_content_capabilities(
                                            std::slice::from_ref(block),
                                            &Some(effective_prompt_caps.clone()),
                                        )?;
                                    }
                                    ContentBlock::Text(_) => {
                                        // Text content is always allowed
                                    }
                                }
                            }
                        }
                    }
                }
                TimelineEvent::SetModel { meta, .. } => {
                    // setModel is an unstable ACP feature - require explicit opt-in
                    if let Some(acp) = &self.acp {
                        if !acp.unstable.unwrap_or(false) {
                            return Err("setModel event requires unstable ACP features to be enabled (set 'unstable: true' in acp config)".to_string());
                        }
                    } else {
                        return Err("setModel event requires ACP configuration with unstable features enabled".to_string());
                    }
                    if let Some(meta) = meta {
                        validate_meta_field(meta)?;
                    }
                }
                TimelineEvent::AgentPermissionRequest {
                    agent_permission_request,
                    meta,
                } => {
                    // Validate permission request data
                    agent_permission_request.validate()?;
                    if let Some(meta) = meta {
                        validate_meta_field(meta)?;
                    }
                }
                TimelineEvent::AgentFileReads {
                    meta: Some(meta), ..
                } => {
                    validate_meta_field(meta)?;
                }
                TimelineEvent::AgentFileReads { .. } => {}
                TimelineEvent::Log {
                    meta: Some(meta), ..
                } => {
                    validate_meta_field(meta)?;
                }
                TimelineEvent::Log { .. } => {}
                TimelineEvent::Initialize { initialize } => {
                    if let Some(meta) = &initialize.meta {
                        validate_meta_field(meta)?;
                    }
                    if let Some(expected) = &initialize.expected_response {
                        if let Some(meta) = &expected.meta {
                            validate_meta_field(meta)?;
                        }
                    }
                }
                TimelineEvent::SessionStart {
                    session_start,
                    meta,
                } => {
                    if session_start_seen.is_some() {
                        return Err(
                            "Only a single sessionStart boundary is supported per scenario"
                                .to_string(),
                        );
                    }
                    session_start_seen = Some(session_start);
                    if let Some(meta) = meta {
                        validate_meta_field(meta)?;
                    }
                    if let Some(expected) = &session_start.expected_prompt_response {
                        validate_expected_prompt_response(expected)?;
                    }
                }
                TimelineEvent::AgentPlan {
                    meta: Some(meta), ..
                } => {
                    validate_meta_field(meta)?;
                }
                TimelineEvent::AgentPlan { .. } => {}
                TimelineEvent::SetMode {
                    meta: Some(meta), ..
                } => {
                    validate_meta_field(meta)?;
                }
                TimelineEvent::SetMode { .. } => {}
                _ => {} // Other events don't have content blocks to validate
            }
        }

        // Enforce alignment between capability advertisement and timeline usage.
        let has_session_boundary = session_start_seen.is_some();
        if let Some(acp_cfg) = &self.acp {
            if let Some(load_cap) = acp_cfg.capabilities.as_ref().and_then(|c| c.load_session) {
                if load_cap && !has_session_boundary {
                    return Err("acp.capabilities.loadSession is true but no sessionStart boundary was found in the timeline".to_string());
                }
                if !load_cap && has_session_boundary {
                    return Err("timeline contains sessionStart but acp.capabilities.loadSession is not enabled".to_string());
                }
            }
        }

        Ok(())
    }
}

/// Compute effective prompt capabilities, falling back to baseline when ACP config is absent.
fn compute_effective_prompt_caps(acp: &Option<AcpConfig>) -> AcpPromptCapabilities {
    acp.as_ref()
        .and_then(|a| a.capabilities.as_ref())
        .map(|caps| caps.effective_prompt_capabilities())
        .unwrap_or_else(|| AcpPromptCapabilities {
            image: Some(false),
            audio: Some(false),
            embedded_context: Some(false),
        })
}

/// Partitioned view of a scenario timeline around an optional sessionStart boundary.
#[derive(Debug)]
pub struct SessionTimeline<'a> {
    pub historical: Vec<&'a TimelineEvent>,
    pub live: Vec<&'a TimelineEvent>,
    pub session_start: Option<&'a SessionStartData>,
    pub session_start_meta: Option<&'a serde_yaml::Value>,
}

/// Validate _meta field structure
pub fn validate_meta_field(meta: &serde_yaml::Value) -> Result<(), String> {
    // Basic validation - _meta should be a mapping
    match meta {
        serde_yaml::Value::Mapping(_) => Ok(()),
        _ => Err("_meta field must be a mapping/object".to_string()),
    }
}

/// Validate expected prompt response blocks (sessionStart and per-userInputs)
fn validate_expected_prompt_response(expected: &ExpectedPromptResponse) -> Result<(), String> {
    if let Some(meta) = &expected.meta {
        validate_meta_field(meta)?;
    }
    if let Some(stop_reason) = &expected.stop_reason {
        if stop_reason.trim().is_empty() {
            return Err("expectedResponse.stopReason cannot be empty".to_string());
        }
    }
    if let Some(session_id) = &expected.session_id {
        if session_id.trim().is_empty() {
            return Err("expectedResponse.sessionId cannot be empty".to_string());
        }
    }
    Ok(())
}

/// Validate ACP configuration
pub fn validate_acp_config(acp: &AcpConfig) -> Result<(), String> {
    // Validate MCP server configurations
    if let Some(servers) = &acp.mcp_servers {
        let caps = acp.capabilities.as_ref().and_then(|c| c.mcp_capabilities.as_ref());
        for server in servers {
            if server.name.is_empty() {
                return Err("MCP server name cannot be empty".to_string());
            }
            // For stdio transport (command-based), validate command is present
            if server.command.is_some() && server.command.as_ref().unwrap().is_empty() {
                return Err(format!("MCP server '{}' has empty command", server.name));
            }
            // Validate environment variable names
            if let Some(env) = &server.env {
                for key in env.keys() {
                    if key.is_empty() {
                        return Err(
                            "MCP server environment variable name cannot be empty".to_string()
                        );
                    }
                    // Basic validation - environment variable names should not contain certain characters
                    if key.contains('=') || key.contains('\0') {
                        return Err(format!("Invalid environment variable name: {}", key));
                    }
                }
            }
            // If no command, treat as HTTP/SSE transport and require capability declarations
            if server.command.is_none() {
                match caps {
                    Some(mcp_caps)
                        if mcp_caps.http.unwrap_or(false) || mcp_caps.sse.unwrap_or(false) => {}
                    _ => {
                        return Err(format!(
                            "MCP server '{}' appears to use HTTP/SSE transport but neither http nor sse capability is enabled",
                            server.name
                        ));
                    }
                }
            }
        }
    }

    // Validate capabilities consistency
    if let Some(capabilities) = &acp.capabilities {
        // Validate MCP capabilities against server configurations
        if let Some(mcp_caps) = &capabilities.mcp_capabilities {
            // If SSE transport is enabled, warn (it's deprecated)
            if mcp_caps.sse.unwrap_or(false) {
                warn_sse_deprecated();
            }
        }
    }

    Ok(())
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
#[serde(rename_all = "camelCase")]
pub struct AcpConfig {
    pub capabilities: Option<AcpCapabilities>,
    pub cwd: Option<String>,
    pub mcp_servers: Option<Vec<McpServerConfig>>,
    pub unstable: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpCapabilities {
    pub load_session: Option<bool>,
    pub prompt_capabilities: Option<AcpPromptCapabilities>,
    pub mcp_capabilities: Option<AcpMcpCapabilities>,
}

impl AcpCapabilities {
    /// Get the effective prompt capabilities, ensuring baseline support
    pub fn effective_prompt_capabilities(&self) -> AcpPromptCapabilities {
        self.prompt_capabilities.clone().unwrap_or(AcpPromptCapabilities {
            // Baseline: all agents MUST support text and resource_link
            image: Some(false),
            audio: Some(false),
            embedded_context: Some(false),
        })
    }
}

#[allow(clippy::disallowed_methods)]
fn warn_sse_deprecated() {
    eprintln!("Warning: SSE transport for MCP is deprecated in ACP");
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpPromptCapabilities {
    pub image: Option<bool>,
    pub audio: Option<bool>,
    pub embedded_context: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpMcpCapabilities {
    pub http: Option<bool>,
    pub sse: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    pub name: String,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub env: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum TimelineEvent {
    /// Unified llmResponse section
    LlmResponse {
        #[serde(rename = "llmResponse")]
        llm_response: Vec<ResponseElement>,
        #[serde(rename = "_meta")]
        meta: Option<serde_yaml::Value>,
    },
    /// Object-based userInputs event
    UserInputs {
        #[serde(rename = "userInputs")]
        user_inputs: Vec<UserInputEntry>,
    },
    AgentToolUse {
        #[serde(rename = "agentToolUse")]
        agent_tool_use: ToolUseData,
        #[serde(rename = "_meta")]
        meta: Option<serde_yaml::Value>,
    },
    AgentEdits {
        #[serde(rename = "agentEdits")]
        agent_edits: FileEditData,
        #[serde(rename = "_meta")]
        meta: Option<serde_yaml::Value>,
    },
    /// ACP initialize capability negotiation
    Initialize {
        #[serde(rename = "initialize")]
        initialize: InitializeData,
    },
    /// ACP session boundary marker for loadSession functionality
    SessionStart {
        #[serde(rename = "sessionStart")]
        session_start: SessionStartData,
        #[serde(rename = "_meta")]
        meta: Option<serde_yaml::Value>,
    },
    /// Agent plan creation/updates
    AgentPlan {
        #[serde(rename = "agentPlan")]
        agent_plan: AgentPlanData,
        #[serde(rename = "_meta")]
        meta: Option<serde_yaml::Value>,
    },
    /// Mode switching
    SetMode {
        #[serde(rename = "setMode")]
        set_mode: SetModeData,
        #[serde(rename = "_meta")]
        meta: Option<serde_yaml::Value>,
    },
    /// Model switching (unstable)
    SetModel {
        #[serde(rename = "setModel")]
        set_model: SetModelData,
        #[serde(rename = "_meta")]
        meta: Option<serde_yaml::Value>,
    },
    /// User cancel session
    UserCancelSession {
        #[serde(rename = "userCancelSession")]
        user_cancel_session: bool,
    },
    /// Agent file reads simulation (translates to one or more file read operations)
    AgentFileReads {
        #[serde(rename = "agentFileReads")]
        agent_file_reads: AgentFileReadsData,
        #[serde(rename = "_meta")]
        meta: Option<serde_yaml::Value>,
    },
    AgentPermissionRequest {
        #[serde(rename = "agentPermissionRequest")]
        agent_permission_request: AgentPermissionRequestData,
        #[serde(rename = "_meta")]
        meta: Option<serde_yaml::Value>,
    },
    Assert {
        assert: AssertionData,
    },
    Status {
        status: String,
    },
    Log {
        log: String,
        #[serde(rename = "_meta")]
        meta: Option<serde_yaml::Value>,
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
    #[serde(rename = "agentPlan")]
    AgentPlan { agent_plan: AgentPlanData },
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
    pub content: ContentBlock,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ContentBlock {
    /// Simple text content
    Text(String),
    /// Rich content block with type and fields
    Rich(RichContentBlock),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum RichContentBlock {
    Text {
        text: String,
        annotations: Option<Vec<ContentAnnotation>>,
    },
    Image {
        mime_type: String,
        path: Option<String>,
        data: Option<String>, // base64 encoded
    },
    Audio {
        mime_type: String,
        path: Option<String>,
        data: Option<String>, // base64 encoded
    },
    Resource {
        resource: EmbeddedResource,
    },
    ResourceLink {
        uri: String,
        name: String,
        mime_type: Option<String>,
        title: Option<String>,
        description: Option<String>,
        size: Option<u64>,
        annotations: Option<Vec<ContentAnnotation>>,
    },
    Diff {
        path: String,
        old_text: Option<String>,
        new_text: String, // Required per ACP spec
    },
    Plan {
        entries: Vec<PlanEntry>,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentAnnotation {
    pub priority: Option<f64>,
    pub audience: Option<Vec<String>>,
    pub metadata: Option<HashMap<String, serde_yaml::Value>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedResource {
    pub uri: String,
    pub mime_type: String,
    pub text: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressStep {
    #[serde(rename = "relativeTime", alias = "timestamp")]
    pub relative_time: u64,
    #[serde(alias = "text", alias = "content")]
    pub message: String,
    /// Optional expected content to validate against streamed tool updates.
    #[serde(default)]
    pub expect_output: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolUseData {
    #[serde(rename = "toolName")]
    pub tool_name: String,
    #[serde(default)]
    pub args: HashMap<String, serde_yaml::Value>,
    /// Optional stable tool call id if provided by scenario author.
    #[serde(rename = "toolCallId", default)]
    pub tool_call_id: Option<String>,
    pub progress: Option<Vec<ProgressStep>>,
    pub result: Option<serde_yaml::Value>,
    pub status: Option<String>,
    pub tool_execution: Option<ToolExecution>,
    #[serde(default)]
    pub meta: Option<serde_yaml::Value>,
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
    pub acp: Option<AcpAssertions>,
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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpAssertions {
    pub session: Option<Vec<AcpSessionAssertion>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpSessionAssertion {
    pub stop_reason: Option<String>,
    pub usage: Option<TokenUsage>,
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
    pub input: InputContent,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(rename = "_meta")]
    pub meta: Option<serde_yaml::Value>,
    #[serde(rename = "expectedResponse")]
    pub expected_response: Option<ExpectedPromptResponse>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum InputContent {
    /// Simple text input
    Text(String),
    /// Rich content blocks (array of content blocks)
    Rich(Vec<RichContentBlock>),
}

pub fn extract_prompt_from_input_content(content: &InputContent) -> Option<String> {
    match content {
        InputContent::Text(text) => Some(text.clone()),
        InputContent::Rich(blocks) => {
            for block in blocks {
                if let Some(text) = extract_text_from_rich_content_block(block) {
                    return Some(text);
                }
            }
            None
        }
    }
}

pub fn extract_text_from_rich_content_block(block: &RichContentBlock) -> Option<String> {
    match block {
        RichContentBlock::Text { text, .. } => Some(text.clone()),
        RichContentBlock::Resource { resource } => resource.text.clone(),
        _ => None,
    }
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
                if let Some(text) = extract_text_from_content_block_value(block) {
                    return Some(text);
                }
            }
            None
        }
        other => extract_text_from_content_block_value(other),
    }
}

pub fn extract_text_from_content_block(content: &ContentBlock) -> Option<String> {
    match content {
        ContentBlock::Text(text) => Some(text.clone()),
        ContentBlock::Rich(block) => extract_text_from_rich_content_block(block),
    }
}

pub fn extract_text_from_content_block_value(value: &Value) -> Option<String> {
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

/// Validation functions for ACP content blocks
pub mod validation {
    use super::*;

    /// Validate a rich content block against ACP specification
    pub fn validate_rich_content_block(
        block: &RichContentBlock,
        base_dir: Option<&Path>,
    ) -> Result<(), String> {
        match block {
            RichContentBlock::Text { text, annotations } => {
                if text.is_empty() {
                    return Err("Text content cannot be empty".to_string());
                }
                if let Some(annotations) = annotations {
                    for annotation in annotations {
                        validate_single_annotation(annotation)?;
                    }
                }
                Ok(())
            }
            RichContentBlock::Image {
                mime_type,
                path,
                data,
            } => {
                validate_image_mime_type(mime_type)?;
                if path.is_none() && data.is_none() {
                    return Err("Image content must include either 'path' or 'data'".to_string());
                }
                if let Some(path) = path {
                    validate_resource_path(path, base_dir)?;
                }
                if let Some(data) = data {
                    validate_base64_data(data)?;
                }
                Ok(())
            }
            RichContentBlock::Audio {
                mime_type,
                path,
                data,
            } => {
                validate_audio_mime_type(mime_type)?;
                if path.is_none() && data.is_none() {
                    return Err("Audio content must include either 'path' or 'data'".to_string());
                }
                if let Some(path) = path {
                    validate_resource_path(path, base_dir)?;
                }
                if let Some(data) = data {
                    validate_base64_data(data)?;
                }
                Ok(())
            }
            RichContentBlock::Resource { resource } => validate_embedded_resource(resource),
            RichContentBlock::ResourceLink {
                uri,
                name,
                mime_type,
                title: _,
                description: _,
                size: _,
                annotations,
            } => {
                if uri.is_empty() {
                    return Err("Resource link URI cannot be empty".to_string());
                }
                if name.is_empty() {
                    return Err("Resource link name cannot be empty".to_string());
                }
                if let Some(mime_type) = mime_type {
                    validate_mime_type(mime_type)?;
                }
                // size is u64, so it's always >= 0
                if let Some(annotations) = annotations {
                    for annotation in annotations {
                        validate_single_annotation(annotation)?;
                    }
                }
                Ok(())
            }
            RichContentBlock::Diff {
                path,
                old_text: _,
                new_text,
            } => {
                if path.is_empty() {
                    return Err("Diff path cannot be empty".to_string());
                }
                if !Path::new(path).is_absolute() {
                    return Err("Diff path must be absolute to avoid ambiguity".to_string());
                }
                if new_text.is_empty() {
                    return Err("Diff newText cannot be empty".to_string());
                }
                // old_text is optional for diff content
                Ok(())
            }
            RichContentBlock::Plan { entries } => {
                if entries.is_empty() {
                    return Err("Plan must have at least one entry".to_string());
                }
                for entry in entries {
                    if entry.content.is_empty() {
                        return Err("Plan entry content cannot be empty".to_string());
                    }
                    if !matches!(entry.priority.as_str(), "high" | "medium" | "low") {
                        return Err(format!(
                            "Invalid plan entry priority '{}'; expected high|medium|low",
                            entry.priority
                        ));
                    }
                    if !matches!(
                        entry.status.as_str(),
                        "pending" | "in_progress" | "completed"
                    ) {
                        return Err(format!(
                            "Invalid plan entry status '{}'; expected pending|in_progress|completed",
                            entry.status
                        ));
                    }
                }
                Ok(())
            }
        }
    }

    /// Validate MIME type for images
    fn validate_image_mime_type(mime_type: &str) -> Result<(), String> {
        let valid_types = [
            "image/png",
            "image/jpeg",
            "image/jpg",
            "image/gif",
            "image/webp",
            "image/svg+xml",
            "image/bmp",
            "image/tiff",
        ];
        if valid_types.contains(&mime_type) {
            Ok(())
        } else {
            Err(format!(
                "Invalid image MIME type: {}. Supported types: {:?}",
                mime_type, valid_types
            ))
        }
    }

    /// Validate MIME type for audio
    fn validate_audio_mime_type(mime_type: &str) -> Result<(), String> {
        let valid_types = [
            "audio/wav",
            "audio/mp3",
            "audio/mpeg",
            "audio/mp4",
            "audio/ogg",
            "audio/flac",
            "audio/aac",
            "audio/webm",
        ];
        if valid_types.contains(&mime_type) {
            Ok(())
        } else {
            Err(format!(
                "Invalid audio MIME type: {}. Supported types: {:?}",
                mime_type, valid_types
            ))
        }
    }

    /// Validate any MIME type format
    fn validate_mime_type(mime_type: &str) -> Result<(), String> {
        if mime_type.contains('/') && !mime_type.contains(char::is_whitespace) {
            Ok(())
        } else {
            Err(format!("Invalid MIME type format: {}", mime_type))
        }
    }

    /// Validate base64 encoded data
    fn validate_base64_data(data: &str) -> Result<(), String> {
        // Basic validation - check if it looks like base64
        if data.is_empty() {
            return Err("Base64 data cannot be empty".to_string());
        }
        // Check for valid base64 characters
        for c in data.chars() {
            if !c.is_alphanumeric() && c != '+' && c != '/' && c != '=' {
                return Err(format!("Invalid base64 character: {}", c));
            }
        }
        Ok(())
    }

    /// Validate a referenced resource path (image/audio) against the scenario directory.
    fn validate_resource_path(path_str: &str, base_dir: Option<&Path>) -> Result<(), String> {
        if path_str.trim().is_empty() {
            return Err("Resource path cannot be empty".to_string());
        }
        let path = PathBuf::from(path_str);
        let resolved = if path.is_absolute() {
            path
        } else if let Some(root) = base_dir {
            root.join(path)
        } else {
            // Without a base directory we can't resolve; skip existence check.
            return Ok(());
        };

        if !resolved.exists() {
            return Err(format!(
                "Resource path {:?} does not exist (resolved from {:?})",
                resolved, base_dir
            ));
        }
        if !resolved.is_file() {
            return Err(format!("Resource path {:?} is not a file", resolved));
        }
        Ok(())
    }

    /// Validate embedded resource
    fn validate_embedded_resource(resource: &EmbeddedResource) -> Result<(), String> {
        if resource.uri.is_empty() {
            return Err("Embedded resource URI cannot be empty".to_string());
        }
        if resource.text.is_none() {
            return Err("Embedded resource must have text content".to_string());
        }
        validate_mime_type(&resource.mime_type)?;
        Ok(())
    }

    /// Validate a single annotation
    fn validate_single_annotation(annotation: &ContentAnnotation) -> Result<(), String> {
        if let Some(priority) = annotation.priority {
            if !(0.0..=1.0).contains(&priority) {
                return Err(format!(
                    "Priority must be between 0.0 and 1.0, got: {}",
                    priority
                ));
            }
        }
        // audience and metadata validation could be added here if needed
        Ok(())
    }

    /// Validate ACP capability requirements for content blocks
    pub fn validate_content_capabilities(
        blocks: &[RichContentBlock],
        capabilities: &Option<AcpPromptCapabilities>,
    ) -> Result<(), String> {
        let caps = capabilities.as_ref().unwrap_or(&AcpPromptCapabilities {
            // Baseline: all agents MUST support text and resource_link
            image: Some(false),
            audio: Some(false),
            embedded_context: Some(false),
        });

        for block in blocks {
            match block {
                RichContentBlock::Image { .. } => {
                    if !caps.image.unwrap_or(false) {
                        return Err("Image content requires 'image' prompt capability".to_string());
                    }
                }
                RichContentBlock::Audio { .. } => {
                    if !caps.audio.unwrap_or(false) {
                        return Err("Audio content requires 'audio' prompt capability".to_string());
                    }
                }
                RichContentBlock::Resource { .. } => {
                    if !caps.embedded_context.unwrap_or(false) {
                        return Err("Embedded resource content requires 'embeddedContext' prompt capability".to_string());
                    }
                }
                RichContentBlock::Text { .. }
                | RichContentBlock::ResourceLink { .. }
                | RichContentBlock::Diff { .. }
                | RichContentBlock::Plan { .. } => {
                    // These are baseline capabilities - all agents MUST support them
                }
            }
        }
        Ok(())
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

/// ACP initialize capability negotiation event
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeData {
    pub protocol_version: u32,
    pub client_capabilities: ClientCapabilities,
    pub client_info: Option<ClientInfo>,
    #[serde(rename = "_meta")]
    pub meta: Option<serde_yaml::Value>,
    pub expected_response: Option<ExpectedInitializeResponse>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientCapabilities {
    pub fs: Option<FilesystemCapabilities>,
    pub terminal: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilesystemCapabilities {
    pub read_text_file: Option<bool>,
    pub write_text_file: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpectedInitializeResponse {
    pub protocol_version: u32,
    pub agent_capabilities: AcpCapabilities,
    #[serde(rename = "_meta")]
    pub meta: Option<serde_yaml::Value>,
}

/// ACP session boundary marker for loadSession functionality
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStartData {
    pub session_id: Option<String>,
    pub expected_prompt_response: Option<ExpectedPromptResponse>,
}

/// Expected response for the first prompt after session start
/// Expected response for the first prompt after a session boundary.
/// Extends ACP PromptResponse with additional fields for testing purposes.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpectedPromptResponse {
    /// Expected session ID in the response (for validation)
    pub session_id: Option<String>,
    /// Expected stop reason (matches ACP PromptResponse.stopReason)
    pub stop_reason: Option<String>,
    /// Expected token usage (extension for testing, not part of ACP spec)
    pub usage: Option<TokenUsage>,
    #[serde(rename = "_meta")]
    pub meta: Option<serde_yaml::Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    pub total_tokens: Option<u32>,
}

/// Agent plan creation/updates
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentPlanData {
    pub entries: Vec<PlanEntry>,
    pub plan_update: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanEntry {
    pub content: String,
    pub priority: String, // "high", "medium", "low"
    pub status: String,   // "pending", "in_progress", "completed"
}

/// Mode switching
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetModeData {
    pub mode_id: String,
}

/// Model switching (unstable)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetModelData {
    pub model_id: String,
}

/// Rules and conditional configuration
#[derive(Debug, Clone, Deserialize)]
#[serde(transparent)]
pub struct Rules {
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<bool>,
    pub config: serde_yaml::Value,
}

/// Symbol table for rule evaluation
#[derive(Debug, Clone, Default)]
pub struct SymbolTable {
    symbols: std::collections::HashMap<String, SymbolValue>,
}

#[derive(Debug, Clone)]
pub enum SymbolValue {
    String(String),
    Number(i64),
    Boolean(bool),
}

impl SymbolTable {
    pub fn new() -> Self {
        Self {
            symbols: std::collections::HashMap::new(),
        }
    }

    /// Build from CLI-style KEY=VAL pairs
    pub fn from_kv_pairs(pairs: &[String]) -> Self {
        let mut table = SymbolTable::new();
        for pair in pairs {
            if pair.trim().is_empty() {
                continue;
            }
            let mut parts = pair.splitn(2, '=');
            let key = parts.next().unwrap().trim();
            if key.is_empty() {
                continue;
            }
            let value_str = parts.next().unwrap_or("").trim();
            let value = if value_str.eq_ignore_ascii_case("true") {
                SymbolValue::Boolean(true)
            } else if value_str.eq_ignore_ascii_case("false") {
                SymbolValue::Boolean(false)
            } else if let Ok(num) = value_str.parse::<i64>() {
                SymbolValue::Number(num)
            } else {
                SymbolValue::String(value_str.to_string())
            };
            table.define(key.to_string(), value);
        }
        table
    }

    /// Build a symbol table from a comma-separated env var like `KEY=val,FLAG=true,N=3`.
    /// Unknown or empty env vars yield an empty table.
    pub fn from_env_var(var: &str) -> Self {
        let mut table = SymbolTable::new();
        if let Ok(raw) = std::env::var(var) {
            for pair in raw.split(',') {
                if pair.trim().is_empty() {
                    continue;
                }
                let mut parts = pair.splitn(2, '=');
                let key = parts.next().unwrap().trim();
                if key.is_empty() {
                    continue;
                }
                let value_str = parts.next().unwrap_or("").trim();
                let value = if value_str.eq_ignore_ascii_case("true") {
                    SymbolValue::Boolean(true)
                } else if value_str.eq_ignore_ascii_case("false") {
                    SymbolValue::Boolean(false)
                } else if let Ok(num) = value_str.parse::<i64>() {
                    SymbolValue::Number(num)
                } else {
                    SymbolValue::String(value_str.to_string())
                };
                table.define(key.to_string(), value);
            }
        }
        table
    }

    pub fn define(&mut self, key: String, value: SymbolValue) {
        self.symbols.insert(key, value);
    }

    pub fn get(&self, key: &str) -> Option<&SymbolValue> {
        self.symbols.get(key)
    }

    pub fn is_defined(&self, key: &str) -> bool {
        self.symbols.contains_key(key)
    }

    /// Evaluate a rule condition
    pub fn evaluate_condition(&self, condition: &str) -> Result<bool, String> {
        // Simple condition evaluation supporting:
        // - $symbol (existence check)
        // - $symbol == value, $symbol != value
        // - $symbol < value, $symbol <= value, $symbol > value, $symbol >= value

        let condition = condition.trim();
        if let Some(stripped) = condition.strip_prefix('$') {
            let symbol_name = stripped.trim();

            // Check for comparisons
            if let Some((op_pos, op)) = Self::find_comparison_operator(symbol_name) {
                let (symbol_part, value_part) = symbol_name.split_at(op_pos);
                let symbol_name = symbol_part.trim();
                let value_str = &value_part[op.len()..].trim();

                if let Some(symbol_value) = self.get(symbol_name) {
                    return self.evaluate_comparison(symbol_value, op, value_str);
                } else {
                    // Undefined symbol = condition does not match
                    return Ok(false);
                }
            } else {
                // Simple existence check
                return Ok(self.is_defined(symbol_name));
            }
        }

        Err(format!("Invalid condition format: {}", condition))
    }

    fn find_comparison_operator(s: &str) -> Option<(usize, &'static str)> {
        let operators = ["==", "!=", "<=", ">=", "<", ">"];
        for &op in &operators {
            if let Some(pos) = s.find(op) {
                return Some((pos, op));
            }
        }
        None
    }

    fn evaluate_comparison(
        &self,
        symbol_value: &SymbolValue,
        op: &str,
        value_str: &str,
    ) -> Result<bool, String> {
        match symbol_value {
            SymbolValue::Number(n) => {
                let value: i64 =
                    value_str.parse().map_err(|_| format!("Invalid number: {}", value_str))?;

                match op {
                    "==" => Ok(*n == value),
                    "!=" => Ok(*n != value),
                    "<" => Ok(*n < value),
                    "<=" => Ok(*n <= value),
                    ">" => Ok(*n > value),
                    ">=" => Ok(*n >= value),
                    _ => Err(format!("Unsupported operator: {}", op)),
                }
            }
            SymbolValue::String(s) => {
                if !value_str.starts_with('"') || !value_str.ends_with('"') {
                    return Err(format!("String values must be quoted: {}", value_str));
                }
                let value = &value_str[1..value_str.len() - 1];

                match op {
                    "==" => Ok(s == value),
                    "!=" => Ok(s != value),
                    _ => Err(format!("Unsupported string operator: {}", op)),
                }
            }
            SymbolValue::Boolean(b) => {
                let value: bool =
                    value_str.parse().map_err(|_| format!("Invalid boolean: {}", value_str))?;

                match op {
                    "==" => Ok(*b == value),
                    "!=" => Ok(*b != value),
                    _ => Err(format!("Unsupported boolean operator: {}", op)),
                }
            }
        }
    }
}

/// Rule evaluation and merging functions
pub fn evaluate_rules(rules: &Rules, symbols: &SymbolTable) -> Result<serde_yaml::Value, String> {
    let mut merged_config = serde_yaml::Value::Null;
    let mut has_matches = false;

    for rule in &rules.rules {
        let matches = if let Some(condition) = &rule.when {
            symbols.evaluate_condition(condition)?
        } else if rule.default.unwrap_or(false) && !has_matches {
            // Default rule applies only if no previous rules matched
            true
        } else {
            false
        };

        if matches {
            has_matches = true;
            merged_config = merge_yaml_values(merged_config, rule.config.clone())?;
        }
    }

    Ok(merged_config)
}

fn merge_yaml_values(
    base: serde_yaml::Value,
    overlay: serde_yaml::Value,
) -> Result<serde_yaml::Value, String> {
    match (base, overlay) {
        (serde_yaml::Value::Null, overlay) => Ok(overlay),
        (base, serde_yaml::Value::Null) => Ok(base),
        (serde_yaml::Value::Mapping(mut base_map), serde_yaml::Value::Mapping(overlay_map)) => {
            for (key, value) in overlay_map {
                base_map.insert(key, value);
            }
            Ok(serde_yaml::Value::Mapping(base_map))
        }
        (_, overlay) => Ok(overlay), // Overlay takes precedence for non-mapping values
    }
}

/// Agent file reads simulation (translates to one or more file read operations)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentFileReadsData {
    pub files: Vec<FileReadSpec>,
}

/// Specification for reading a single file
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileReadSpec {
    pub path: String,
    pub expected_content: Option<serde_yaml::Value>,
}

/// Agent permission request simulation
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentPermissionRequestData {
    pub session_id: Option<String>,
    pub tool_call: Option<serde_yaml::Value>,
    pub options: Option<Vec<PermissionOption>>,
    pub decision: Option<UserDecision>,
    pub granted: Option<bool>, // shorthand for decision
}

impl AgentPermissionRequestData {
    /// Validate the permission request data
    pub fn validate(&self) -> Result<(), String> {
        // Validate permission options
        if let Some(options) = &self.options {
            for option in options {
                match option.kind.as_str() {
                    "allow_once" | "allow_always" | "reject_once" | "reject_always" => {
                        // Valid kinds
                    }
                    invalid => {
                        return Err(format!(
                            "Invalid permission option kind: {}. Must be one of: allow_once, allow_always, reject_once, reject_always",
                            invalid
                        ));
                    }
                }
            }
        }

        // Validate decision vs granted shorthand
        match (&self.decision, self.granted) {
            (Some(_), Some(_)) => {
                return Err("Cannot specify both 'decision' and 'granted' fields".to_string());
            }
            (None, None) => {
                return Err("Must specify either 'decision' or 'granted' field".to_string());
            }
            (Some(decision), None) => {
                // Validate decision
                if decision.outcome == "selected" {
                    if decision.option_id.is_none() {
                        return Err("'optionId' is required when outcome is 'selected'".to_string());
                    }
                    // Validate that the option_id exists in options
                    if let Some(options) = &self.options {
                        let option_exists =
                            options.iter().any(|opt| Some(&opt.id) == decision.option_id.as_ref());
                        if !option_exists {
                            return Err(format!(
                                "Selected optionId '{}' not found in provided options",
                                decision.option_id.as_ref().unwrap()
                            ));
                        }
                    }
                } else if decision.outcome != "cancelled" {
                    return Err(format!(
                        "Invalid outcome: {}. Must be 'selected' or 'cancelled'",
                        decision.outcome
                    ));
                }
            }
            (None, Some(_granted)) => {
                // Validate shorthand - ensure we have options to map to
                if self.options.is_none() {
                    return Err(
                        "'options' must be specified when using 'granted' shorthand".to_string()
                    );
                }
                // The actual mapping will be done during scenario execution
            }
        }

        Ok(())
    }

    /// Get the effective decision, resolving shorthand if needed
    pub fn effective_decision(&self) -> Result<UserDecision, String> {
        match (&self.decision, self.granted) {
            (Some(decision), None) => Ok(decision.clone()),
            (None, Some(granted)) => {
                // Resolve shorthand
                if let Some(options) = &self.options {
                    let target_kind = if granted { "allow_once" } else { "reject_once" };
                    if let Some(option) = options.iter().find(|opt| opt.kind == target_kind) {
                        Ok(UserDecision {
                            outcome: "selected".to_string(),
                            option_id: Some(option.id.clone()),
                        })
                    } else {
                        Err(format!(
                            "No '{}' option found in permission options",
                            target_kind
                        ))
                    }
                } else {
                    Err("Cannot resolve granted shorthand without options".to_string())
                }
            }
            _ => Err("Invalid permission request configuration".to_string()),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionOption {
    pub id: String,
    pub label: String,
    pub kind: String, // "allow_once", "allow_always", "reject_once", "reject_always"
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserDecision {
    pub outcome: String, // "selected", "cancelled"
    pub option_id: Option<String>,
}
