// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Scenario playback engine for deterministic testing
//!
//! This module implements the Scenario-Format.md specification for
//! deterministic playback of LLM interactions.
//!
//!
//! ## OVERVIEW:
//! The mock server simulates OpenAI/Anthropic API endpoints for deterministic testing.
//! It does NOT execute tools/processes itself - it only returns properly formatted API responses
//! that suggest tool usage. Tool execution happens separately in the agent client.
//!
//! The server is started with a specific TOOLS PROFILE (command-line option --tools-profile) that defines
//! the valid tool schemas for a particular coding agent (Codex, Claude, Gemini, etc.). This profile
//! determines how scenario events like agentEdits and agentToolUse are mapped to specific tool call
//! responses that are valid for that agent.
//!
//! TOOLS MAPPING PRINCIPLE: Scenario events represent a superset of all possible tools across all agents.
//! The tools profile provides mappings from scenario tool names to agent-specific tool implementations.
//! For example:
//! - Scenario `grep` → Claude: native `grep` tool → Other agents: `run_terminal_cmd` with `grep` command
//! - Scenario `read_file` → Claude: native `read_file` tool → Other agents: `run_terminal_cmd` with `cat` command
//!
//! TOOL CHANGES TRACKING: When tool validation fails in strict mode (--strict-tools-validation)
//! with FORCE_TOOLS_VALIDATION_FAILURE=1 set in the environment, the server automatically saves the complete
//! API request to agent-requests/{agent_name}/{version}/request.json. This creates a historical record
//! of how third-party coding agents' tool definitions change over time, maintained in git.
//!
//! The FORCE_TOOLS_VALIDATION_FAILURE environment variable forces all tool validation to fail, ensuring that
//! real agent requests get captured even when their tools are normally considered valid.
//!
//! In strict tools validation mode (command-line option --strict-tools-validation), the server
//! immediately aborts if it encounters an unfamiliar tool definition, helping developers quickly
//! identify missing tool profiles and mappings during development.
//!
//! ## KEY PRINCIPLES:
//! 1. Session Isolation: Each unique API key represents a separate client session
//! 2. Timeline-Based Responses: Scenarios define deterministic sequences of agent behavior
//! 3. Protocol Compliance: Responses follow exact OpenAI/Anthropic API schemas with proper coalescing
//! 4. Provider-Specific Thinking: OpenAI keeps thinking internal (not in responses), Anthropic can expose thinking blocks
//! 5. Client Tool Validation: Server validates tool definitions sent by clients in API requests
//! 6. No Tool Execution: Server only suggests tool calls and edits, never executes them
//! 7. llmResponse Grouping: Multiple response elements can be grouped into single API responses
//! 8. Tool Evolution Tracking: Failed validations automatically save requests for historical tracking
//!
//! ## CONTENT-BASED SCENARIO MATCHING:
//!
//! The scenario player analyzes conversation history to find scenario events that match
//! the actual user inputs and tool results, then returns the appropriate following responses.
//! This allows agents to make extra requests without breaking the scenario flow.
//!
//! ```text
//! FOR each API request with conversation history:
//!     IF scenario not started:
//!         Check if request contains meaningful content, start scenario if so
//!
//!     Analyze last message in conversation history:
//!         IF role == "user": extract user input content, find matching userInputs event
//!                            in scenario, return the immediately following llmResponse.
//!         IF role == "tool": extract tool result content, find matching agentToolUse event
//!                            in scenario, return the immediately following llmResponse.
//!
//!     IF no matching scenario event found:
//!         Return minimal response (agent made unexpected request)
//! ```
//!
//! The test server processes timeline events in order, executing assertions
//! before returning the next response to verify that the expected outcomes
//! of previous responses have been met. This ensures filesystem state and
//! other conditions are validated immediately after tool execution, providing
//! deterministic testing of agent behavior and outcomes.
//!
//! ## COALESCING RULES:
//! - OpenAI: Thinking content is kept internal and NOT included in API responses. Only text content and tool_calls appear in the assistant message. Thinking is processed but remains hidden from the agent client.
//! - Anthropic: Thinking content can be exposed as separate "thinking" blocks in the response content array, alongside "text" blocks and "tool_use" blocks, all within a single API response.
//!
//! ## NOTE:
//! The mock server skips over events that don't generate API responses.
//! "llmResponse" groups, "think", "agentToolUse", "agentEdits", and "assistant" events produce API responses.
//! Other events are processed for test harness coordination but don't return data to the agent client.
//!
//! ## SESSION MANAGEMENT:
//! - API keys are arbitrary strings (any valid key works)
//! - Sessions persist across multiple API calls with same key
//! - Fresh API key = fresh scenario execution
//! - Enables concurrent testing without server restarts
//!
//! ## RESPONSE FORMATS:
//! - OpenAI: {"choices": [{"message": {"role": "assistant", "content": "text_content_only", "tool_calls": [...]}}]} - thinking is processed internally but NOT exposed in the response
//! - Anthropic: {"content": [{"type": "thinking", "thinking": "thinking_content"}, {"type": "text", "text": "text_content"}, {"type": "tool_use", ...}]} - thinking is exposed as separate content blocks
//!
//! ## TOOL CHANGES TRACKING:
//! When tool validation fails in strict mode, the complete API request is saved to:
//! agent-requests/{agent_name}/{version}/request.json
//!
//! This creates a git-tracked historical record of third-party agent API evolution:
//! - Captures the exact tool definitions agents send
//! - Enables updating tool profiles as agents change
//! - Provides evidence for tool mapping decisions
//! - Tracks API schema evolution over time
//!
//! This design enables deterministic, replayable testing of agent workflows with realistic LLM response patterns.
//!

pub mod tool_profiles;

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

use serde_json::{Map as JsonMap, Value as JsonValue, json};

use crate::{
    config::ProxyConfig,
    converters::ApiFormat,
    error::{Error, Result},
    proxy::{ProxyRequest, ProxyResponse},
};

// Add missing dependencies for timeline processing
use chrono::Utc;
use uuid::Uuid;

/// Scenario player for deterministic playback
pub struct ScenarioPlayer {
    config: Arc<RwLock<ProxyConfig>>,
    pub scenarios: HashMap<String, Scenario>,
    active_sessions: HashMap<String, ScenarioSession>,
    tool_profiles: Arc<tool_profiles::ToolProfiles>,
}

impl ScenarioPlayer {
    /// Create a new scenario player
    pub async fn new(config: Arc<RwLock<ProxyConfig>>) -> Result<Self> {
        let mut scenarios = HashMap::new();
        let tool_profiles = Arc::new(tool_profiles::ToolProfiles::new());

        // Load scenarios if scenario directory or file is configured
        {
            let config_guard = config.read().await;
            if let Some(scenario_dir) = &config_guard.scenario.scenario_dir {
                Self::load_scenarios_from_dir(&mut scenarios, Path::new(scenario_dir)).await?;
            } else if let Some(scenario_file) = &config_guard.scenario.scenario_file {
                let scenario = Self::load_scenario_from_file(Path::new(scenario_file)).await?;
                scenarios.insert("test".to_string(), scenario);
            }
        }

        Ok(Self {
            config,
            scenarios,
            active_sessions: HashMap::new(),
            tool_profiles,
        })
    }

    /// Play a request using scenario playback (implements the full mock server algorithm)
    pub async fn play_request(&mut self, request: &ProxyRequest) -> Result<ProxyResponse> {
        // Log complete request with headers and body for debugging and tool changes tracking
        self.log_request(request).await?;

        // Handle FORCE_TOOLS_VALIDATION_FAILURE environment variable (tool changes tracking)
        self.handle_force_validation_failure(&request.payload).await?;

        // Extract session ID from API key (session isolation principle)
        let session_id = self.extract_session_id(request)?;

        // Determine agent type from request
        let agent_type = self.determine_agent_type(request);

        // Find matching scenario first (before borrowing sessions mutably)
        let scenario = self.find_scenario_for_request(request).await?;
        let scenario = scenario.clone();

        // Get or create session for this API key
        let session = self.active_sessions.entry(session_id.clone()).or_insert_with(|| {
            ScenarioSession::new(session_id.clone(), self.tool_profiles.clone(), agent_type)
        });

        // Process the request using the full algorithm
        let response = session.process_request(request, &scenario).await?;

        // Log response for debugging and tool changes tracking
        self.log_response(request, &response).await?;

        Ok(response)
    }

    /// Determine agent type from request or configuration
    fn determine_agent_type(&self, request: &ProxyRequest) -> tool_profiles::AgentType {
        // First try to determine from config
        if let Ok(config_guard) = self.config.try_read() {
            if let Some(agent_type_str) = &config_guard.scenario.agent_type {
                return match agent_type_str.as_str() {
                    "claude" => tool_profiles::AgentType::Claude,
                    "codex" => tool_profiles::AgentType::Codex,
                    "gemini" => tool_profiles::AgentType::Gemini,
                    "opencode" => tool_profiles::AgentType::Opencode,
                    "qwen" => tool_profiles::AgentType::Qwen,
                    "cursor-cli" => tool_profiles::AgentType::CursorCli,
                    "goose" => tool_profiles::AgentType::Goose,
                    _ => tool_profiles::AgentType::Claude, // Default to Claude
                };
            }
        }

        // Fall back to API format detection
        match request.client_format {
            ApiFormat::Anthropic => tool_profiles::AgentType::Claude,
            ApiFormat::OpenAI | ApiFormat::OpenAIResponses => tool_profiles::AgentType::Codex,
        }
    }

    /// Extract session ID from API request (session isolation by API key)
    fn extract_session_id(&self, request: &ProxyRequest) -> Result<String> {
        // Extract API key from Authorization header
        if let Some(auth_header) = request.headers.get("authorization") {
            if let Some(bearer_token) = auth_header.strip_prefix("Bearer ") {
                return Ok(bearer_token.to_string());
            }
        }

        // Try alternative headers
        if let Some(api_key) = request.headers.get("api-key") {
            return Ok(api_key.to_string());
        }

        // For Anthropic requests
        if let Some(api_key) = request.headers.get("x-api-key") {
            return Ok(api_key.to_string());
        }

        // Default session for testing
        Ok("default-session".to_string())
    }

    /// Validate tool definitions from client requests (tool changes tracking)
    pub async fn validate_tool_definitions(
        &self,
        tool_definitions: &[serde_json::Value],
        request_body: &serde_json::Value,
    ) -> Result<()> {
        let strict = self.config.read().await.scenario.strict_tools_validation;
        self.validate_tool_definitions_with_strict(tool_definitions, request_body, strict)
            .await
    }

    /// Validate tool definitions with explicit strict mode control
    pub async fn validate_tool_definitions_with_strict(
        &self,
        tool_definitions: &[serde_json::Value],
        request_body: &serde_json::Value,
        strict: bool,
    ) -> Result<()> {
        if tool_definitions.is_empty() {
            return Ok(());
        }

        // Check FORCE_TOOLS_VALIDATION_FAILURE environment variable
        let force_validation_failure = std::env::var("FORCE_TOOLS_VALIDATION_FAILURE")
            .unwrap_or_default()
            .to_lowercase();

        let force_failure_enabled =
            matches!(force_validation_failure.as_str(), "1" | "true" | "yes");

        // Get agent type from config
        let config_guard = self.config.read().await;
        let agent_type = config_guard.scenario.agent_type.as_deref().unwrap_or("claude");
        let agent_version = config_guard.scenario.agent_version.as_deref().unwrap_or("unknown");

        let agent_type_enum = match agent_type {
            "claude" => tool_profiles::AgentType::Claude,
            "codex" => tool_profiles::AgentType::Codex,
            _ => tool_profiles::AgentType::Claude, // Default to Claude
        };

        let valid_tools = self.tool_profiles.valid_tools_for_agent_type(agent_type_enum);

        for tool_def in tool_definitions {
            if let Some(tool_name) = tool_def.get("name").and_then(|n| n.as_str()) {
                // Force validation failure if FORCE_TOOLS_VALIDATION_FAILURE is set
                let is_invalid = !valid_tools.contains(tool_name) || force_failure_enabled;

                if is_invalid {
                    let error_msg = if force_failure_enabled {
                        format!(
                            "Tool validation forced to fail by FORCE_TOOLS_VALIDATION_FAILURE for '{}'",
                            tool_name
                        )
                    } else {
                        format!(
                            "Tool '{}' is not in the valid tools profile for {}",
                            tool_name, agent_type
                        )
                    };

                    println!("TOOLS VALIDATION ERROR: {}", error_msg);

                    // Save the request for tracking tool definition changes
                    self.save_agent_request(request_body, tool_name, &error_msg, agent_version)?;

                    // Check if strict validation is enabled
                    if strict {
                        return Err(Error::Scenario {
                            message: format!("Strict tools validation failed: {}", error_msg),
                        });
                    }
                }
            }
        }

        Ok(())
    }

    /// Handle FORCE_TOOLS_VALIDATION_FAILURE environment variable for tool changes tracking
    async fn handle_force_validation_failure(
        &self,
        request_body: &serde_json::Value,
    ) -> Result<()> {
        let force_validation_failure = std::env::var("FORCE_TOOLS_VALIDATION_FAILURE")
            .unwrap_or_default()
            .to_lowercase();

        let force_failure_enabled =
            matches!(force_validation_failure.as_str(), "1" | "true" | "yes");

        if force_failure_enabled {
            // Get agent type from config for directory structure
            let config_guard = self.config.read().await;
            let agent_type = config_guard.scenario.agent_type.as_deref().unwrap_or("claude");
            let agent_version = config_guard.scenario.agent_version.as_deref().unwrap_or("unknown");

            self.save_agent_request(
                request_body,
                &format!("{}_request", agent_type),
                &format!("Capturing real {} request", agent_type),
                agent_version,
            )?;
        }

        Ok(())
    }

    /// Log complete response for debugging and tool changes tracking
    async fn log_response(&self, request: &ProxyRequest, response: &ProxyResponse) -> Result<()> {
        let request_log_template = self.logging_template();
        if request_log_template.as_deref() == Some("none") {
            return Ok(());
        }

        let log_responses = env_flag("LLM_API_PROXY_LOG_RESPONSES", false);
        if !log_responses {
            return Ok(());
        }

        let api_key = self.extract_session_id(request).unwrap_or_else(|_| "unknown".to_string());
        let scenario_name = self.current_scenario_name(request);

        let log_path = request_log_template
            .unwrap()
            .replace("{scenario}", &scenario_name)
            .replace("{key}", &api_key);

        let mut entry = JsonMap::new();
        entry.insert("timestamp".into(), json!(Utc::now().to_rfc3339()));
        entry.insert("type".into(), json!("response"));
        entry.insert("method".into(), json!("POST"));
        entry.insert(
            "path".into(),
            json!(match request.client_format {
                ApiFormat::OpenAI => "/v1/chat/completions",
                ApiFormat::OpenAIResponses => "/v1/responses",
                ApiFormat::Anthropic => "/v1/messages",
            }),
        );
        entry.insert("request_id".into(), json!(request.request_id.clone()));
        entry.insert("client_format".into(), json!(request.client_format));
        entry.insert("scenario".into(), json!(scenario_name));
        entry.insert("api_key".into(), json!(api_key));
        entry.insert("status".into(), json!(response.status));
        entry.insert("response".into(), response.payload.clone());
        entry.insert("response_headers".into(), json!(response.headers.clone()));

        let minimize_logs = self.config.read().await.scenario.minimize_logs;
        self.write_log_entry(&log_path, JsonValue::Object(entry), minimize_logs)
    }

    /// Log complete request with headers and body for debugging and tool changes tracking
    async fn log_request(&self, request: &ProxyRequest) -> Result<()> {
        let request_log_template = self.logging_template();
        if request_log_template.as_deref() == Some("none") {
            return Ok(());
        }

        let log_headers = env_flag("LLM_API_PROXY_LOG_HEADERS", true);
        let log_body = env_flag("LLM_API_PROXY_LOG_BODY", true);

        if !log_headers && !log_body {
            return Ok(());
        }

        let api_key = self.extract_session_id(request).unwrap_or_else(|_| "unknown".to_string());
        let scenario_name = self.current_scenario_name(request);

        let log_path = request_log_template
            .unwrap()
            .replace("{scenario}", &scenario_name)
            .replace("{key}", &api_key);

        let mut entry = JsonMap::new();
        entry.insert("timestamp".into(), json!(Utc::now().to_rfc3339()));
        entry.insert("type".into(), json!("request"));
        entry.insert("method".into(), json!("POST"));
        entry.insert(
            "path".into(),
            json!(match request.client_format {
                ApiFormat::OpenAI => "/v1/chat/completions",
                ApiFormat::OpenAIResponses => "/v1/responses",
                ApiFormat::Anthropic => "/v1/messages",
            }),
        );
        entry.insert("request_id".into(), json!(request.request_id.clone()));
        entry.insert("client_format".into(), json!(request.client_format));
        entry.insert("scenario".into(), json!(scenario_name));
        entry.insert("api_key".into(), json!(api_key));

        if log_headers {
            entry.insert("headers".into(), json!(request.headers.clone()));
        }
        if log_body {
            entry.insert("body".into(), request.payload.clone());
        }

        let minimize_logs = self.config.read().await.scenario.minimize_logs;
        self.write_log_entry(&log_path, JsonValue::Object(entry), minimize_logs)
    }

    fn logging_template(&self) -> Option<String> {
        Some(
            std::env::var("REQUEST_LOG_TEMPLATE")
                .or_else(|_| std::env::var("LLM_API_PROXY_REQUEST_LOG"))
                .unwrap_or_else(|_| "none".to_string()),
        )
    }

    fn current_scenario_name(&self, request: &ProxyRequest) -> String {
        request
            .headers
            .get("x-scenario-name")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string())
    }

    fn write_log_entry(&self, log_path: &str, entry: JsonValue, minimize: bool) -> Result<()> {
        let json_string = if minimize {
            serde_json::to_string(&entry).unwrap_or_else(|_| "{}".to_string())
        } else {
            serde_json::to_string_pretty(&entry).unwrap_or_else(|_| "{}".to_string())
        };

        if log_path == "stdout" {
            println!("{}", json_string);
            return Ok(());
        }

        if let Some(parent) = std::path::Path::new(log_path).parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::Scenario {
                message: format!("Failed to create log directory: {}", e),
            })?;
        }

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)
            .map_err(|e| Error::Scenario {
                message: format!("Failed to open log file {}: {}", log_path, e),
            })?;

        use std::io::Write;
        writeln!(file, "{}", json_string).map_err(|e| Error::Scenario {
            message: format!("Failed to write to log file {}: {}", log_path, e),
        })?;

        Ok(())
    }

    /// Validate tool calls from client requests
    pub async fn validate_tool_calls(
        &self,
        tool_calls: &[serde_json::Value],
        request_body: &serde_json::Value,
    ) -> Result<()> {
        let strict = self.config.read().await.scenario.strict_tools_validation;
        self.validate_tool_calls_with_strict(tool_calls, request_body, strict).await
    }

    /// Validate tool calls with explicit strict mode control
    pub async fn validate_tool_calls_with_strict(
        &self,
        tool_calls: &[serde_json::Value],
        request_body: &serde_json::Value,
        strict: bool,
    ) -> Result<()> {
        if tool_calls.is_empty() {
            return Ok(());
        }

        // Check FORCE_TOOLS_VALIDATION_FAILURE environment variable
        let force_validation_failure = std::env::var("FORCE_TOOLS_VALIDATION_FAILURE")
            .unwrap_or_default()
            .to_lowercase();

        let force_failure_enabled =
            matches!(force_validation_failure.as_str(), "1" | "true" | "yes");

        // Get agent type from config
        let config_guard = self.config.read().await;
        let agent_type = config_guard.scenario.agent_type.as_deref().unwrap_or("claude");
        let agent_version = config_guard.scenario.agent_version.as_deref().unwrap_or("unknown");

        let agent_type_enum = match agent_type {
            "claude" => tool_profiles::AgentType::Claude,
            "codex" => tool_profiles::AgentType::Codex,
            _ => tool_profiles::AgentType::Claude, // Default to Claude
        };

        let valid_tools = self.tool_profiles.valid_tools_for_agent_type(agent_type_enum);

        for tool_call in tool_calls {
            let tool_name = tool_call
                .get("name")
                .or_else(|| tool_call.get("function").and_then(|f| f.get("name")))
                .and_then(|n| n.as_str())
                .unwrap_or("unknown_tool");

            // Force validation failure if FORCE_TOOLS_VALIDATION_FAILURE is set
            let is_invalid = !valid_tools.contains(tool_name) || force_failure_enabled;

            if is_invalid {
                let error_msg = if force_failure_enabled {
                    format!(
                        "Tool validation forced to fail by FORCE_TOOLS_VALIDATION_FAILURE for '{}'",
                        tool_name
                    )
                } else {
                    format!("Unknown tool '{}' for profile '{}'", tool_name, agent_type)
                };

                println!("TOOLS VALIDATION ERROR: {}", error_msg);
                println!(
                    "TOOL CALL DUMP: {}",
                    serde_json::to_string_pretty(tool_call).unwrap_or_default()
                );

                // Save the request for tracking tool definition changes
                self.save_agent_request(request_body, tool_name, &error_msg, agent_version)?;

                // Check if strict validation is enabled
                if strict {
                    return Err(Error::Scenario {
                        message: format!("Strict tools validation failed: {}", error_msg),
                    });
                }
            }
        }

        Ok(())
    }

    /// Save agent request for tracking tool definition changes
    fn save_agent_request(
        &self,
        request_body: &serde_json::Value,
        _tool_name: &str,
        _error_msg: &str,
        agent_version: &str,
    ) -> Result<()> {
        // Create directory structure: agent-requests/{agent_name}/{version}/
        let base_dir = std::env::current_dir()
            .map_err(|e| Error::Scenario {
                message: format!("Failed to get current directory: {}", e),
            })?
            .join("agent-requests");

        // Get agent type from config for directory structure
        let agent_type = {
            let config_guard = self.config.try_read().ok();
            config_guard
                .and_then(|c| c.scenario.agent_type.clone())
                .unwrap_or_else(|| "claude".to_string())
        };

        let agent_dir = base_dir.join(agent_type);
        let version_dir = agent_dir.join(agent_version);

        std::fs::create_dir_all(&version_dir).map_err(|e| Error::Scenario {
            message: format!("Failed to create agent-requests directory: {}", e),
        })?;

        // Use simple filename: request.json
        let request_file = version_dir.join("request.json");

        // Save just the raw request JSON as sent by the agent
        let json_str = serde_json::to_string_pretty(request_body).map_err(|e| Error::Scenario {
            message: format!("Failed to serialize request: {}", e),
        })?;

        std::fs::write(&request_file, json_str).map_err(|e| Error::Scenario {
            message: format!("Failed to write agent request: {}", e),
        })?;

        println!("SAVED AGENT REQUEST: {}", request_file.display());
        Ok(())
    }

    /// Load scenarios from a directory
    async fn load_scenarios_from_dir(
        scenarios: &mut HashMap<String, Scenario>,
        dir: &Path,
    ) -> Result<()> {
        if !dir.exists() {
            return Ok(());
        }

        // Load all YAML files in the directory
        let mut dir_entries = tokio::fs::read_dir(dir).await.map_err(|e| Error::Scenario {
            message: format!("Failed to read scenario directory: {}", e),
        })?;

        while let Some(entry) = dir_entries.next_entry().await.map_err(|e| Error::Scenario {
            message: format!("Failed to read directory entry: {}", e),
        })? {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("yaml")
                || path.extension().and_then(|s| s.to_str()) == Some("yml")
            {
                let scenario = Self::load_scenario_from_file(&path).await?;
                scenarios.insert(scenario.name.clone(), scenario);
            }
        }

        Ok(())
    }

    /// Load a single scenario from a YAML file
    async fn load_scenario_from_file(path: &Path) -> Result<Scenario> {
        let content = tokio::fs::read_to_string(path).await.map_err(|e| Error::Scenario {
            message: format!("Failed to read scenario file {}: {}", path.display(), e),
        })?;

        let scenario: Scenario = serde_yaml::from_str(&content).map_err(|e| Error::Scenario {
            message: format!("Failed to parse scenario file {}: {}", path.display(), e),
        })?;

        // Validate the scenario structure
        Self::validate_scenario(&scenario)?;

        Ok(scenario)
    }

    /// Validate scenario structure and constraints
    fn validate_scenario(scenario: &Scenario) -> Result<()> {
        for event in &scenario.timeline {
            match event {
                TimelineEvent::LlmResponse { llm_response } => {
                    let mut has_thinking = false;
                    let mut has_assistant = false;

                    for element in llm_response {
                        match element {
                            ResponseElement::Think { .. } => {
                                has_thinking = true;
                            }
                            ResponseElement::Assistant { .. } => {
                                has_assistant = true;
                            }
                            _ => {}
                        }
                    }

                    // If there's thinking, there must also be an assistant response
                    if has_thinking && !has_assistant {
                        return Err(Error::Scenario {
                            message:
                                "llmResponse contains thinking blocks but no assistant responses"
                                    .to_string(),
                        });
                    }
                }
                TimelineEvent::Event(data) => {
                    // Also validate llmResponse in Event variant
                    if let Some(serde_yaml::Value::Sequence(elements)) = data.get("llmResponse") {
                        let mut has_thinking = false;
                        let mut has_assistant = false;

                        for element_value in elements {
                            let element: ResponseElement =
                                serde_yaml::from_value(element_value.clone()).map_err(|e| {
                                    Error::Scenario {
                                        message: format!("Failed to parse response element: {}", e),
                                    }
                                })?;
                            match element {
                                ResponseElement::Think { .. } => {
                                    has_thinking = true;
                                }
                                ResponseElement::Assistant { .. } => {
                                    has_assistant = true;
                                }
                                _ => {}
                            }
                        }

                        if has_thinking && !has_assistant {
                            return Err(Error::Scenario {
                                message: "llmResponse contains thinking blocks but no assistant responses".to_string(),
                            });
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// Find the appropriate scenario for a request
    async fn find_scenario_for_request(&self, request: &ProxyRequest) -> Result<&Scenario> {
        // Try to get scenario name from headers first
        if let Some(scenario_name) = request.headers.get("x-scenario-name") {
            if let Some(scenario) = self.scenarios.get(scenario_name) {
                return Ok(scenario);
            }
        }

        // If no scenarios loaded, return error
        if self.scenarios.is_empty() {
            return Err(Error::Scenario {
                message: "No scenarios loaded. Configure scenario_dir in proxy config.".to_string(),
            });
        }

        // For now, return the first scenario as default
        // TODO: Implement smarter scenario selection based on request content
        let first_scenario = self.scenarios.values().next().unwrap();
        Ok(first_scenario)
    }
}

/// Scenario data structure based on Scenario-Format.md
#[derive(Debug, Clone, serde::Deserialize)]
pub struct Scenario {
    /// Scenario name
    pub name: String,
    /// Scenario tags
    #[serde(default)]
    pub tags: Vec<String>,
    /// Terminal configuration reference
    pub terminal_ref: Option<String>,
    /// Compatibility flags
    pub compat: Option<CompatibilityFlags>,
    /// Repository setup
    pub repo: Option<RepoConfig>,
    /// AH command configuration
    pub ah: Option<AhConfig>,
    /// Server configuration
    pub server: Option<ServerConfig>,
    /// Timeline of events
    pub timeline: Vec<TimelineEvent>,
    /// Expected results
    pub expect: Option<ExpectConfig>,
}

/// Compatibility flags
#[derive(Debug, Clone, serde::Deserialize)]
pub struct CompatibilityFlags {
    pub allow_inline_terminal: Option<bool>,
    pub allow_type_steps: Option<bool>,
}

/// Repository configuration
#[derive(Debug, Clone, serde::Deserialize)]
pub struct RepoConfig {
    pub init: Option<bool>,
    pub branch: Option<String>,
    pub dir: Option<String>,
    pub files: Option<Vec<FileConfig>>,
}

/// File configuration for seeding repository
#[derive(Debug, Clone, serde::Deserialize)]
pub struct FileConfig {
    pub path: String,
    pub contents: serde_yaml::Value,
}

/// AH command configuration
#[derive(Debug, Clone, serde::Deserialize)]
pub struct AhConfig {
    pub cmd: String,
    pub flags: Vec<String>,
    pub env: Option<HashMap<String, String>>,
}

/// Server configuration
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ServerConfig {
    pub mode: Option<String>,
    pub llm_api_style: Option<String>,
    pub coalesce_thinking_with_tool_use: Option<bool>,
}

/// What the agent is currently expecting based on conversation history
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentExpectation {
    /// Agent expects tool execution results (after assistant message with tool calls)
    ToolResults,
    /// Agent expects user input (after assistant message without tool calls)
    UserInput,
    /// Agent expects assistant response (after user message)
    AssistantResponse,
    /// Agent expects next response (after tool result - could be assistant or user)
    NextResponse,
}

/// Timeline event (unified event sequence)
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
pub enum TimelineEvent {
    /// LLM response event (groups multiple response elements)
    LlmResponse { llm_response: Vec<ResponseElement> },
    /// Individual events for backward compatibility
    Event(HashMap<String, serde_yaml::Value>),
    /// Control events
    Control {
        #[serde(rename = "type")]
        event_type: String,
        #[serde(flatten)]
        data: HashMap<String, serde_yaml::Value>,
    },
    /// Assertion event for verifying filesystem state and other conditions
    Assert { assert: AssertionData },
    /// Error event for modeling error conditions (rate limiting, invalid requests, etc.)
    Error { error: ErrorData },
}

/// Response element in an LLM response
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
pub enum ResponseElement {
    /// Thinking event
    #[serde(rename = "think")]
    Think { think: Vec<ThinkingStep> },
    /// Tool use event
    AgentToolUse {
        #[serde(rename = "agentToolUse")]
        agent_tool_use: ToolUseData,
    },
    /// Tool result event (for multi-turn conversations)
    #[serde(rename = "toolResult")]
    ToolResult { tool_result: ToolResultData },
    /// Error event (generates LLM API error response)
    #[serde(rename = "error")]
    Error { error: ErrorData },
    /// File edits event
    AgentEdits {
        #[serde(rename = "agentEdits")]
        agent_edits: FileEditData,
    },
    /// Assistant response event
    #[serde(rename = "assistant")]
    Assistant { assistant: Vec<AssistantStep> },
}

/// Thinking step
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ThinkingStep(pub u64, pub String); // (milliseconds, text)

/// Tool use data
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ToolUseData {
    #[serde(rename = "toolName")]
    pub tool_name: String,
    pub args: HashMap<String, serde_yaml::Value>,
    pub progress: Option<Vec<ProgressStep>>,
    pub result: Option<serde_yaml::Value>,
    pub status: Option<String>,
}

/// Tool result data (for multi-turn conversations)
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ToolResultData {
    #[serde(rename = "toolCallId")]
    pub tool_call_id: String,
    pub content: serde_yaml::Value,
    #[serde(default)]
    pub is_error: bool,
}

/// Progress step
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ProgressStep(pub u64, pub String); // (milliseconds, message)

/// File edit data
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEditData {
    pub path: String,
    pub lines_added: u32,
    pub lines_removed: u32,
}

/// Assistant response step
#[derive(Debug, Clone, serde::Deserialize)]
pub struct AssistantStep(pub u64, pub String); // (milliseconds, text)

/// Expected results configuration
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ExpectConfig {
    pub exit_code: Option<i32>,
    pub artifacts: Option<Vec<ArtifactExpectation>>,
}

/// Artifact expectation
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ArtifactExpectation {
    #[serde(rename = "type")]
    pub artifact_type: String,
    pub pattern: Option<String>,
}

/// Assertion data for verifying filesystem state and other conditions
#[derive(Debug, Clone, serde::Deserialize)]
pub struct AssertionData {
    pub fs: Option<FilesystemAssertions>,
    pub text: Option<TextAssertions>,
    pub json: Option<JsonAssertions>,
    pub git: Option<GitAssertions>,
}

/// Filesystem assertions
#[derive(Debug, Clone, serde::Deserialize)]
pub struct FilesystemAssertions {
    pub exists: Option<Vec<String>>,
    pub not_exists: Option<Vec<String>>,
}

/// Text assertions
#[derive(Debug, Clone, serde::Deserialize)]
pub struct TextAssertions {
    pub contains: Option<Vec<String>>,
}

/// JSON assertions
#[derive(Debug, Clone, serde::Deserialize)]
pub struct JsonAssertions {
    pub file: Option<Vec<JsonFileAssertion>>,
}

/// JSON file assertion
#[derive(Debug, Clone, serde::Deserialize)]
pub struct JsonFileAssertion {
    pub path: String,
    pub pointer: String,
    pub equals: serde_yaml::Value,
}

/// Git assertions
#[derive(Debug, Clone, serde::Deserialize)]
pub struct GitAssertions {
    pub commit: Option<Vec<GitCommitAssertion>>,
}

/// Git commit assertion
#[derive(Debug, Clone, serde::Deserialize)]
pub struct GitCommitAssertion {
    pub message_contains: Option<String>,
}

/// Error data for modeling error conditions
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ErrorData {
    #[serde(rename = "errorType")]
    pub error_type: String,
    pub status_code: Option<u16>,
    pub message: String,
    pub details: Option<serde_yaml::Value>,
    pub retry_after_seconds: Option<u32>,
}

/// Tool call generated from scenario events
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub args: HashMap<String, serde_yaml::Value>,
}

#[derive(Debug, Default)]
struct AggregatedResponse {
    assistant_text: String,
    tool_calls: Vec<ToolCall>,
    thinking_steps: Vec<ThinkingStep>,
}

/// Response part collected from timeline events
#[derive(Debug, Clone)]
pub enum ResponsePart {
    Think(Vec<ThinkingStep>),
    ToolUse(ToolUseData),
    ToolResult(ToolResultData),
    FileEdit(FileEditData),
    Assistant(Vec<AssistantStep>),
}

/// Active scenario session
#[derive(Debug, Clone)]
pub struct ScenarioSession {
    pub session_id: String,
    pub current_event_index: usize,
    pub start_time: std::time::Instant,
    pub tool_profiles: Arc<tool_profiles::ToolProfiles>,
    pub agent_type: tool_profiles::AgentType,
    pub scenario_started: bool,
}

impl ScenarioSession {
    /// Create a new scenario session
    pub fn new(
        session_id: String,
        tool_profiles: Arc<tool_profiles::ToolProfiles>,
        agent_type: tool_profiles::AgentType,
    ) -> Self {
        Self {
            session_id,
            current_event_index: 0,
            start_time: std::time::Instant::now(),
            tool_profiles,
            agent_type,
            scenario_started: false,
        }
    }

    /// Process a request using the scenario timeline (equivalent to Python server.py algorithm)
    pub async fn process_request(
        &mut self,
        request: &ProxyRequest,
        scenario: &Scenario,
    ) -> Result<ProxyResponse> {
        // Check if this request contains meaningful content before starting scenario playback
        if !self.scenario_started {
            if !self.is_meaningful_request(request) {
                // Return a minimal response to keep the client connection alive
                // but don't start scenario playback yet
                return self.generate_minimal_response(request.client_format);
            }
            // This is the first meaningful request - start scenario playback
            self.scenario_started = true;
        }

        // Analyze what the agent is currently expecting based on conversation history
        let expectation = self.analyze_agent_expectation(request)?;

        // Find the next appropriate scenario event based on agent expectation
        self.find_next_scenario_event(
            scenario,
            expectation,
            request,
            request.client_format,
            request.streaming,
        )
    }

    /// Find the next scenario event that matches the agent's current expectation
    fn find_next_scenario_event(
        &mut self,
        scenario: &Scenario,
        expectation: AgentExpectation,
        request: &ProxyRequest,
        client_format: crate::converters::ApiFormat,
        streaming: bool,
    ) -> Result<ProxyResponse> {
        match expectation {
            AgentExpectation::AssistantResponse => {
                // Agent sent a user message - try content-based matching first, then fall back to sequential
                match self.find_response_for_user_input(scenario, request, client_format, streaming)
                {
                    Ok(response) => Ok(response),
                    Err(_) => {
                        self.find_next_sequential_response(scenario, client_format, streaming)
                    }
                }
            }
            AgentExpectation::NextResponse => {
                // Agent sent a tool result - try content-based matching first, then fall back to sequential
                match self.find_response_for_tool_result(
                    scenario,
                    request,
                    client_format,
                    streaming,
                ) {
                    Ok(response) => Ok(response),
                    Err(_) => {
                        self.find_next_sequential_response(scenario, client_format, streaming)
                    }
                }
            }
            AgentExpectation::ToolResults | AgentExpectation::UserInput => {
                // These are handled by the test harness, not the mock LLM API server
                // Return a minimal response to keep the connection alive
                self.generate_minimal_response(client_format)
            }
        }
    }

    /// Find the llmResponse that follows a matching userInput event
    fn find_response_for_user_input(
        &mut self,
        scenario: &Scenario,
        request: &ProxyRequest,
        client_format: crate::converters::ApiFormat,
        streaming: bool,
    ) -> Result<ProxyResponse> {
        // Extract user input content from the last message
        let user_content = self.extract_last_user_content(request)?;

        // Scan timeline from beginning to find matching userInput and its following llmResponse
        for (idx, event) in scenario.timeline.iter().enumerate() {
            if let TimelineEvent::Event(data) = event {
                if let Some(user_inputs) = data.get("userInputs") {
                    // Check if this userInputs event matches our user content
                    if self.user_input_matches(user_inputs, &user_content)? {
                        // Found matching userInput, look for the next llmResponse
                        for next_idx in (idx + 1)..scenario.timeline.len() {
                            match &scenario.timeline[next_idx] {
                                TimelineEvent::LlmResponse { llm_response } => {
                                    // Found the following llmResponse - process it
                                    let mut response_parts = Vec::new();
                                    for element in llm_response {
                                        match element {
                                            ResponseElement::Error { error } => {
                                                return self.generate_error_response(
                                                    error.clone(),
                                                    client_format,
                                                );
                                            }
                                            _ => {
                                                let part = self
                                                    .response_element_to_response_part(&element)?;
                                                response_parts.push(part);
                                            }
                                        }
                                    }
                                    let aggregated = self.process_response_parts(response_parts)?;
                                    return if streaming {
                                        self.generate_streaming_response(aggregated, client_format)
                                    } else {
                                        self.generate_api_response(aggregated, client_format)
                                    };
                                }
                                TimelineEvent::Event(data) => {
                                    if let Some(llm_response_value) = data.get("llmResponse") {
                                        let llm_response: Vec<ResponseElement> =
                                            serde_yaml::from_value(llm_response_value.clone())
                                                .map_err(|e| Error::Scenario {
                                                    message: format!(
                                                        "Failed to parse llmResponse: {}",
                                                        e
                                                    ),
                                                })?;
                                        let mut response_parts = Vec::new();
                                        for element in llm_response {
                                            match element {
                                                ResponseElement::Error { error } => {
                                                    return self.generate_error_response(
                                                        error.clone(),
                                                        client_format,
                                                    );
                                                }
                                                _ => {
                                                    let part = self
                                                        .response_element_to_response_part(
                                                            &element,
                                                        )?;
                                                    response_parts.push(part);
                                                }
                                            }
                                        }
                                        let aggregated =
                                            self.process_response_parts(response_parts)?;
                                        return if streaming {
                                            self.generate_streaming_response(
                                                aggregated,
                                                client_format,
                                            )
                                        } else {
                                            self.generate_api_response(aggregated, client_format)
                                        };
                                    }
                                }
                                _ => {}
                            }
                        }
                        return Err(Error::Scenario {
                            message: "Found matching userInput but no following llmResponse"
                                .to_string(),
                        });
                    }
                }
            }
        }

        Err(Error::Scenario {
            message: format!("No matching userInput found for content: {}", user_content),
        })
    }

    /// Find the llmResponse that follows a matching tool execution event
    fn find_response_for_tool_result(
        &mut self,
        scenario: &Scenario,
        request: &ProxyRequest,
        client_format: crate::converters::ApiFormat,
        streaming: bool,
    ) -> Result<ProxyResponse> {
        // Extract tool result content from the last message
        let tool_content = self.extract_last_tool_content(request)?;

        // Scan timeline to find matching agentToolUse and its following llmResponse
        for (idx, event) in scenario.timeline.iter().enumerate() {
            if let TimelineEvent::Event(data) = event {
                if let Some(agent_tool_use) = data.get("agentToolUse") {
                    // Check if this tool use matches our tool result
                    if self.tool_execution_matches(agent_tool_use, &tool_content)? {
                        // Found matching tool execution, look for the next llmResponse
                        for next_idx in (idx + 1)..scenario.timeline.len() {
                            match &scenario.timeline[next_idx] {
                                TimelineEvent::LlmResponse { llm_response } => {
                                    // Found the following llmResponse - process it
                                    let mut response_parts = Vec::new();
                                    for element in llm_response {
                                        match element {
                                            ResponseElement::Error { error } => {
                                                return self.generate_error_response(
                                                    error.clone(),
                                                    client_format,
                                                );
                                            }
                                            _ => {
                                                let part = self
                                                    .response_element_to_response_part(&element)?;
                                                response_parts.push(part);
                                            }
                                        }
                                    }
                                    let aggregated = self.process_response_parts(response_parts)?;
                                    return if streaming {
                                        self.generate_streaming_response(aggregated, client_format)
                                    } else {
                                        self.generate_api_response(aggregated, client_format)
                                    };
                                }
                                TimelineEvent::Event(data) => {
                                    if let Some(llm_response_value) = data.get("llmResponse") {
                                        let llm_response: Vec<ResponseElement> =
                                            serde_yaml::from_value(llm_response_value.clone())
                                                .map_err(|e| Error::Scenario {
                                                    message: format!(
                                                        "Failed to parse llmResponse: {}",
                                                        e
                                                    ),
                                                })?;
                                        let mut response_parts = Vec::new();
                                        for element in llm_response {
                                            match element {
                                                ResponseElement::Error { error } => {
                                                    return self.generate_error_response(
                                                        error.clone(),
                                                        client_format,
                                                    );
                                                }
                                                _ => {
                                                    let part = self
                                                        .response_element_to_response_part(
                                                            &element,
                                                        )?;
                                                    response_parts.push(part);
                                                }
                                            }
                                        }
                                        let aggregated =
                                            self.process_response_parts(response_parts)?;
                                        return if streaming {
                                            self.generate_streaming_response(
                                                aggregated,
                                                client_format,
                                            )
                                        } else {
                                            self.generate_api_response(aggregated, client_format)
                                        };
                                    }
                                }
                                _ => {}
                            }
                        }
                        return Err(Error::Scenario {
                            message: "Found matching tool execution but no following llmResponse"
                                .to_string(),
                        });
                    }
                }
            }
        }

        Err(Error::Scenario {
            message: format!(
                "No matching tool execution found for content: {:?}",
                tool_content
            ),
        })
    }

    /// Extract content from the last user message
    fn extract_last_user_content(&self, request: &ProxyRequest) -> Result<String> {
        let messages =
            request.payload.get("messages").and_then(|m| m.as_array()).ok_or_else(|| {
                Error::Scenario {
                    message: "No messages found in request payload".to_string(),
                }
            })?;

        let last_message = messages.last().ok_or_else(|| Error::Scenario {
            message: "Empty messages array".to_string(),
        })?;

        let role =
            last_message
                .get("role")
                .and_then(|r| r.as_str())
                .ok_or_else(|| Error::Scenario {
                    message: "Message missing role field".to_string(),
                })?;

        if role != "user" {
            return Err(Error::Scenario {
                message: format!("Expected user message, got: {}", role),
            });
        }

        let content = last_message.get("content").ok_or_else(|| Error::Scenario {
            message: "User message missing content field".to_string(),
        })?;

        match content {
            serde_json::Value::String(text) => Ok(text.clone()),
            serde_json::Value::Array(blocks) => {
                // Handle array content (e.g., Anthropic format)
                let mut text_parts = Vec::new();
                for block in blocks {
                    if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                        text_parts.push(text.to_string());
                    }
                }
                Ok(text_parts.join("\n"))
            }
            _ => Err(Error::Scenario {
                message: "Unsupported content format in user message".to_string(),
            }),
        }
    }

    /// Extract tool result content from the last tool message
    fn extract_last_tool_content(&self, request: &ProxyRequest) -> Result<serde_json::Value> {
        let messages =
            request.payload.get("messages").and_then(|m| m.as_array()).ok_or_else(|| {
                Error::Scenario {
                    message: "No messages found in request payload".to_string(),
                }
            })?;

        let last_message = messages.last().ok_or_else(|| Error::Scenario {
            message: "Empty messages array".to_string(),
        })?;

        let role =
            last_message
                .get("role")
                .and_then(|r| r.as_str())
                .ok_or_else(|| Error::Scenario {
                    message: "Message missing role field".to_string(),
                })?;

        if role != "tool" {
            return Err(Error::Scenario {
                message: format!("Expected tool message, got: {}", role),
            });
        }

        let content = last_message.get("content").ok_or_else(|| Error::Scenario {
            message: "Tool message missing content field".to_string(),
        })?;

        Ok(content.clone())
    }

    /// Check if a scenario userInputs event matches the actual user input content
    fn user_input_matches(
        &self,
        scenario_user_inputs: &serde_yaml::Value,
        actual_content: &str,
    ) -> Result<bool> {
        // Handle the scenario userInputs format: [[milliseconds, "text"], ...]
        let inputs_array = scenario_user_inputs.as_sequence().ok_or_else(|| Error::Scenario {
            message: "userInputs should be an array".to_string(),
        })?;

        for input_item in inputs_array {
            if let Some(input_array) = input_item.as_sequence() {
                if input_array.len() >= 2 {
                    if let Some(text) = input_array[1].as_str() {
                        // Simple substring match - could be made more sophisticated
                        if actual_content.contains(text) {
                            return Ok(true);
                        }
                    }
                }
            }
        }

        Ok(false)
    }

    /// Check if a scenario agentToolUse event matches the actual tool result content
    fn tool_execution_matches(
        &self,
        scenario_tool_use: &serde_yaml::Value,
        actual_tool_content: &serde_json::Value,
    ) -> Result<bool> {
        // For now, do a simple check - look for the expected result in the scenario
        // This could be enhanced to match tool_call_id, tool_name, etc.

        // Extract expected result from scenario tool use
        if let Some(result) = scenario_tool_use.get("result") {
            match (result, actual_tool_content) {
                (serde_yaml::Value::String(expected), serde_json::Value::String(actual)) => {
                    // Simple string match
                    Ok(expected == actual)
                }
                (serde_yaml::Value::String(expected), serde_json::Value::Array(content_blocks)) => {
                    // Check if any content block contains the expected result
                    for block in content_blocks {
                        if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                            if text.contains(expected) {
                                return Ok(true);
                            }
                        }
                    }
                    Ok(false)
                }
                _ => {
                    // For other formats, just return true for now (could be enhanced)
                    Ok(true)
                }
            }
        } else {
            // If no expected result in scenario, accept any tool result
            Ok(true)
        }
    }

    /// Find the next response in the scenario sequentially (fallback for backward compatibility)
    fn find_next_sequential_response(
        &mut self,
        scenario: &Scenario,
        client_format: crate::converters::ApiFormat,
        streaming: bool,
    ) -> Result<ProxyResponse> {
        // Scan through the scenario timeline from current position for any response event
        while self.current_event_index < scenario.timeline.len() {
            let event = &scenario.timeline[self.current_event_index];

            match event {
                TimelineEvent::LlmResponse { llm_response } => {
                    // Found an llmResponse - process it
                    let mut response_parts = Vec::new();
                    for element in llm_response {
                        match element {
                            ResponseElement::Error { error } => {
                                return self.generate_error_response(error.clone(), client_format);
                            }
                            _ => {
                                let part = self.response_element_to_response_part(&element)?;
                                response_parts.push(part);
                            }
                        }
                    }
                    self.current_event_index += 1;
                    let aggregated = self.process_response_parts(response_parts)?;
                    return if streaming {
                        self.generate_streaming_response(aggregated, client_format)
                    } else {
                        self.generate_api_response(aggregated, client_format)
                    };
                }
                TimelineEvent::Event(data) => {
                    // Check for llmResponse events
                    if let Some(llm_response_value) = data.get("llmResponse") {
                        let llm_response: Vec<ResponseElement> =
                            serde_yaml::from_value(llm_response_value.clone()).map_err(|e| {
                                Error::Scenario {
                                    message: format!("Failed to parse llmResponse: {}", e),
                                }
                            })?;
                        self.current_event_index += 1;
                        let mut response_parts = Vec::new();
                        for element in llm_response {
                            match element {
                                ResponseElement::Error { error } => {
                                    return self
                                        .generate_error_response(error.clone(), client_format);
                                }
                                _ => {
                                    let part = self.response_element_to_response_part(&element)?;
                                    response_parts.push(part);
                                }
                            }
                        }
                        let aggregated = self.process_response_parts(response_parts)?;
                        return if streaming {
                            self.generate_streaming_response(aggregated, client_format)
                        } else {
                            self.generate_api_response(aggregated, client_format)
                        };
                    }
                    // Check for agentToolUse events
                    if let Some(agent_tool_use_value) = data.get("agentToolUse") {
                        let agent_tool_use: ToolUseData =
                            serde_yaml::from_value(agent_tool_use_value.clone()).map_err(|e| {
                                Error::Scenario {
                                    message: format!("Failed to parse agentToolUse: {}", e),
                                }
                            })?;
                        self.current_event_index += 1;
                        let response_part = ResponsePart::ToolUse(agent_tool_use);
                        let aggregated = self.process_response_parts(vec![response_part])?;
                        return if streaming {
                            self.generate_streaming_response(aggregated, client_format)
                        } else {
                            self.generate_api_response(aggregated, client_format)
                        };
                    }
                    // Check for agentEdits events
                    if let Some(agent_edits_value) = data.get("agentEdits") {
                        let agent_edits: FileEditData =
                            serde_yaml::from_value(agent_edits_value.clone()).map_err(|e| {
                                Error::Scenario {
                                    message: format!("Failed to parse agentEdits: {}", e),
                                }
                            })?;
                        self.current_event_index += 1;
                        let response_part = ResponsePart::FileEdit(agent_edits);
                        let aggregated = self.process_response_parts(vec![response_part])?;
                        return if streaming {
                            self.generate_streaming_response(aggregated, client_format)
                        } else {
                            self.generate_api_response(aggregated, client_format)
                        };
                    }
                    // Check for old-style assistant events
                    if let Some(assistant_steps) = data.get("assistant") {
                        let steps: Vec<AssistantStep> =
                            serde_yaml::from_value(assistant_steps.clone()).map_err(|e| {
                                Error::Scenario {
                                    message: format!("Failed to parse assistant steps: {}", e),
                                }
                            })?;
                        self.current_event_index += 1;
                        let aggregated =
                            self.process_response_parts(vec![ResponsePart::Assistant(steps)])?;
                        return if streaming {
                            self.generate_streaming_response(aggregated, client_format)
                        } else {
                            self.generate_api_response(aggregated, client_format)
                        };
                    }
                    // Skip other events
                    self.current_event_index += 1;
                }
                // Skip control events, assertions, etc.
                _ => {
                    self.current_event_index += 1;
                }
            }
        }

        // No more responses found - return minimal response without advancing scenario
        self.generate_minimal_response(client_format)
    }

    /// Analyze conversation history to determine what the agent is currently expecting
    fn analyze_agent_expectation(&self, request: &ProxyRequest) -> Result<AgentExpectation> {
        // Extract messages from request payload
        let messages =
            request.payload.get("messages").and_then(|m| m.as_array()).ok_or_else(|| {
                Error::Scenario {
                    message: "No messages found in request payload".to_string(),
                }
            })?;

        if messages.is_empty() {
            return Err(Error::Scenario {
                message: "Empty messages array in request".to_string(),
            });
        }

        // Get the last message
        let last_message = messages.last().unwrap();

        // Determine expectation based on last message role and content
        let role =
            last_message
                .get("role")
                .and_then(|r| r.as_str())
                .ok_or_else(|| Error::Scenario {
                    message: "Message missing role field".to_string(),
                })?;

        match role {
            "assistant" => {
                // Check if assistant message has tool calls
                if let Some(tool_calls) = last_message.get("tool_calls") {
                    if tool_calls.is_array() && !tool_calls.as_array().unwrap().is_empty() {
                        return Ok(AgentExpectation::ToolResults);
                    }
                }
                // Assistant message without tool calls - agent expects user input
                Ok(AgentExpectation::UserInput)
            }
            "user" => {
                // User message - agent expects assistant response
                Ok(AgentExpectation::AssistantResponse)
            }
            "tool" => {
                // Tool result - agent expects next response (could be assistant or user input)
                Ok(AgentExpectation::NextResponse)
            }
            "system" => {
                // System message - shouldn't normally happen at the end, but treat as expecting assistant response
                Ok(AgentExpectation::AssistantResponse)
            }
            _ => Err(Error::Scenario {
                message: format!("Unknown message role: {}", role),
            }),
        }
    }

    /// Check if a request contains meaningful content that should trigger scenario playback
    fn is_meaningful_request(&self, request: &ProxyRequest) -> bool {
        // Check if the request payload contains messages with substantial content
        if let Some(messages) = request.payload.get("messages") {
            if let Some(messages_array) = messages.as_array() {
                for message in messages_array {
                    if let Some(content) = message.get("content") {
                        match content {
                            serde_json::Value::String(text) => {
                                // Consider requests with more than 3 characters as meaningful
                                // This filters out very short test requests like "count" but allows
                                // legitimate short requests like "test"
                                if text.len() > 3 {
                                    return true;
                                }
                            }
                            serde_json::Value::Array(content_blocks) => {
                                // Handle array content (for complex content blocks)
                                for block in content_blocks {
                                    if let Some(text) = block.get("text") {
                                        if let Some(text_str) = text.as_str() {
                                            if text_str.len() > 3 {
                                                return true;
                                            }
                                        }
                                    }
                                }
                            }
                            _ => continue,
                        }
                    }
                }
            }
        }
        false
    }

    /// Generate a minimal response to keep client connection alive before scenario starts
    fn generate_minimal_response(
        &self,
        client_format: crate::converters::ApiFormat,
    ) -> Result<ProxyResponse> {
        match client_format {
            crate::converters::ApiFormat::Anthropic => {
                let content = vec![serde_json::json!({
                    "type": "text",
                    "text": "Initializing..."
                })];

                let payload = serde_json::json!({
                    "id": format!("msg_{}", Uuid::new_v4()),
                    "type": "message",
                    "role": "assistant",
                    "model": "claude-3-5-sonnet-20241022",
                    "content": content,
                    "stop_reason": "end_turn",
                    "stop_sequence": null,
                    "usage": {
                        "input_tokens": 0,
                        "output_tokens": 0
                    }
                });

                Ok(ProxyResponse {
                    status: 200,
                    payload,
                    headers: HashMap::new(),
                    sse_data: None,
                })
            }
            crate::converters::ApiFormat::OpenAI => {
                let payload = serde_json::json!({
                    "id": format!("chatcmpl-{}", Uuid::new_v4()),
                    "object": "chat.completion",
                    "created": chrono::Utc::now().timestamp(),
                    "model": "gpt-4",
                    "choices": [{
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": "Initializing..."
                        },
                        "finish_reason": "stop"
                    }],
                    "usage": {
                        "prompt_tokens": 0,
                        "completion_tokens": 0,
                        "total_tokens": 0
                    }
                });

                Ok(ProxyResponse {
                    status: 200,
                    payload,
                    headers: HashMap::new(),
                    sse_data: None,
                })
            }
            crate::converters::ApiFormat::OpenAIResponses => {
                let payload = serde_json::json!({
                    "id": format!("resp-{}", Uuid::new_v4()),
                    "object": "response",
                    "created": chrono::Utc::now().timestamp(),
                    "model": "gpt-4",
                    "status": "completed",
                    "output": [{
                        "role": "assistant",
                        "content": [{
                            "type": "output_text",
                            "text": "Initializing..."
                        }]
                    }],
                    "usage": {
                        "prompt_tokens": 0,
                        "completion_tokens": 0,
                        "total_tokens": 0
                    }
                });

                Ok(ProxyResponse {
                    status: 200,
                    payload,
                    headers: HashMap::new(),
                    sse_data: None,
                })
            }
        }
    }

    /// Convert response element to response part
    fn response_element_to_response_part(&self, element: &ResponseElement) -> Result<ResponsePart> {
        match element {
            ResponseElement::Think { think: steps } => Ok(ResponsePart::Think(steps.clone())),
            ResponseElement::AgentToolUse {
                agent_tool_use: tool_data,
            } => Ok(ResponsePart::ToolUse(tool_data.clone())),
            ResponseElement::ToolResult { tool_result } => {
                Ok(ResponsePart::ToolResult(tool_result.clone()))
            }
            ResponseElement::Error { error } => {
                // Error elements within llmResponse should generate error responses
                Err(Error::Scenario {
                    message: format!("LLM API error: {} - {}", error.error_type, error.message),
                })
            }
            ResponseElement::AgentEdits {
                agent_edits: edit_data,
            } => Ok(ResponsePart::FileEdit(edit_data.clone())),
            ResponseElement::Assistant { assistant: steps } => {
                Ok(ResponsePart::Assistant(steps.clone()))
            }
        }
    }

    /// Process collected response parts into assistant text and tool calls
    fn process_response_parts(
        &self,
        response_parts: Vec<ResponsePart>,
    ) -> Result<AggregatedResponse> {
        let mut aggregate = AggregatedResponse::default();

        for part in response_parts {
            match part {
                ResponsePart::Think(steps) => {
                    aggregate.thinking_steps.extend(steps);
                }
                ResponsePart::Assistant(steps) => {
                    for step in steps {
                        aggregate.assistant_text.push_str(&step.1);
                    }
                }
                ResponsePart::ToolUse(tool_data) => {
                    if let Some(call) = self.tool_profiles.map_tool_call(
                        self.agent_type,
                        &tool_data.tool_name,
                        &tool_data.args,
                    ) {
                        aggregate.tool_calls.push(call);
                    }
                }
                ResponsePart::ToolResult(_result_data) => {
                    // Tool results are handled by the client/agent, not the mock server
                    // They appear as user messages in subsequent turns
                    // For now, we skip them in response generation
                }
                ResponsePart::FileEdit(edit_data) => {
                    let mut args = HashMap::new();
                    args.insert(
                        "path".to_string(),
                        serde_yaml::Value::String(edit_data.path.clone()),
                    );
                    args.insert(
                        "linesAdded".to_string(),
                        serde_yaml::Value::Number(serde_yaml::Number::from(
                            edit_data.lines_added as u64,
                        )),
                    );
                    args.insert(
                        "linesRemoved".to_string(),
                        serde_yaml::Value::Number(serde_yaml::Number::from(
                            edit_data.lines_removed as u64,
                        )),
                    );

                    if let Some(call) =
                        self.tool_profiles.map_tool_call(self.agent_type, "agentEdits", &args)
                    {
                        aggregate.tool_calls.push(call);
                    } else {
                        aggregate.tool_calls.push(ToolCall {
                            id: format!("call_{}", uuid::Uuid::new_v4()),
                            name: "edit_file".to_string(),
                            args,
                        });
                    }
                }
            }
        }

        Ok(aggregate)
    }

    /// Generate API response based on format (implements coalescing rules)
    fn generate_api_response(
        &self,
        aggregate: AggregatedResponse,
        client_format: crate::converters::ApiFormat,
    ) -> Result<ProxyResponse> {
        let AggregatedResponse {
            assistant_text,
            tool_calls,
            thinking_steps,
        } = aggregate;

        match client_format {
            crate::converters::ApiFormat::Anthropic => {
                self.generate_anthropic_response(assistant_text, tool_calls, thinking_steps)
            }
            crate::converters::ApiFormat::OpenAI => {
                self.generate_openai_response(assistant_text, tool_calls)
            }
            crate::converters::ApiFormat::OpenAIResponses => {
                self.generate_openai_responses_response(assistant_text, tool_calls)
            }
        }
    }

    /// Generate OpenAI format response (thinking kept internal, text + tool_calls in assistant message)
    fn generate_openai_response(
        &self,
        assistant_text: String,
        tool_calls: Vec<ToolCall>,
    ) -> Result<ProxyResponse> {
        let mut choices = Vec::new();
        let mut message = serde_json::json!({
            "role": "assistant",
            "content": assistant_text
        });

        if !tool_calls.is_empty() {
            let openai_tool_calls: Vec<serde_json::Value> = tool_calls
                .into_iter()
                .map(|call| {
                    serde_json::json!({
                        "id": call.id,
                        "type": "function",
                        "function": {
                            "name": call.name,
                            "arguments": serde_json::to_string(&call.args).unwrap_or_default()
                        }
                    })
                })
                .collect();

            message["tool_calls"] = serde_json::Value::Array(openai_tool_calls);
        } else {
            // Remove tool_calls field if empty
            message.as_object_mut().unwrap().remove("tool_calls");
        }

        choices.push(serde_json::json!({
            "index": 0,
            "message": message,
            "finish_reason": "stop"
        }));

        let payload = serde_json::json!({
            "id": format!("chatcmpl-{}", Uuid::new_v4()),
            "object": "chat.completion",
            "created": Utc::now().timestamp(),
            "model": "gpt-4o-mini",
            "choices": choices,
            "usage": {
                "prompt_tokens": 100,
                "completion_tokens": 50,
                "total_tokens": 150
            }
        });

        Ok(ProxyResponse {
            status: 200,
            payload,
            headers: HashMap::new(),
            sse_data: None,
        })
    }

    /// Generate Anthropic format response (thinking + text + tool_calls as content blocks)
    fn generate_anthropic_response(
        &self,
        assistant_text: String,
        tool_calls: Vec<ToolCall>,
        thinking_steps: Vec<ThinkingStep>,
    ) -> Result<ProxyResponse> {
        let mut content = Vec::new();

        for step in thinking_steps {
            content.push(serde_json::json!({
                "type": "thinking",
                "thinking": step.1,
            }));
        }

        // Add text content if present
        if !assistant_text.is_empty() {
            content.push(serde_json::json!({
                "type": "text",
                "text": assistant_text
            }));
        }

        // Add tool use blocks
        for call in tool_calls.into_iter() {
            content.push(serde_json::json!({
                "type": "tool_use",
                "id": call.id,
                "name": call.name,
                "input": call.args
            }));
        }

        let payload = serde_json::json!({
            "id": format!("msg_{}", Uuid::new_v4()),
            "type": "message",
            "role": "assistant",
            "model": "claude-3-5-sonnet-20241022",
            "content": content,
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {
                "input_tokens": 100,
                "output_tokens": 50
            }
        });

        Ok(ProxyResponse {
            status: 200,
            payload,
            headers: HashMap::new(),
            sse_data: None,
        })
    }

    /// Generate OpenAI Responses API payload
    fn generate_openai_responses_response(
        &self,
        assistant_text: String,
        tool_calls: Vec<ToolCall>,
    ) -> Result<ProxyResponse> {
        let mut output_items = Vec::new();

        // Build assistant message content
        let mut content_parts = Vec::new();
        if !assistant_text.is_empty() {
            content_parts.push(serde_json::json!({
                "type": "output_text",
                "text": assistant_text,
            }));
        }

        for call in tool_calls.into_iter() {
            content_parts.push(serde_json::json!({
                "type": "tool_use",
                "id": call.id,
                "name": call.name,
                "input": call.args,
            }));
        }

        output_items.push(serde_json::json!({
            "type": "message",
            "role": "assistant",
            "content": content_parts,
        }));

        let payload = serde_json::json!({
            "id": format!("resp-{}", Uuid::new_v4()),
            "object": "response",
            "created": Utc::now().timestamp(),
            "model": "gpt-4o-mini",
            "status": "completed",
            "output": output_items,
            "usage": {
                "prompt_tokens": 100,
                "completion_tokens": 50,
                "total_tokens": 150
            }
        });

        Ok(ProxyResponse {
            status: 200,
            payload,
            headers: HashMap::new(),
            sse_data: None,
        })
    }

    /// Generate error response from error data
    fn generate_error_response(
        &self,
        error_data: ErrorData,
        client_format: crate::converters::ApiFormat,
    ) -> Result<ProxyResponse> {
        let status_code = error_data.status_code.unwrap_or(400);
        let mut headers = HashMap::new();

        if let Some(retry_after) = error_data.retry_after_seconds {
            headers.insert("retry-after".to_string(), retry_after.to_string());
        }

        let payload = match client_format {
            crate::converters::ApiFormat::Anthropic => {
                serde_json::json!({
                    "type": "error",
                    "error": {
                        "type": error_data.error_type,
                        "message": error_data.message
                    }
                })
            }
            crate::converters::ApiFormat::OpenAI
            | crate::converters::ApiFormat::OpenAIResponses => {
                serde_json::json!({
                    "error": {
                        "type": error_data.error_type,
                        "message": error_data.message,
                        "details": error_data.details
                    }
                })
            }
        };

        Ok(ProxyResponse {
            status: status_code,
            payload,
            headers,
            sse_data: None,
        })
    }

    /// Generate streaming response with SSE events
    fn generate_streaming_response(
        &self,
        aggregate: AggregatedResponse,
        client_format: crate::converters::ApiFormat,
    ) -> Result<ProxyResponse> {
        let AggregatedResponse {
            assistant_text,
            tool_calls,
            thinking_steps,
        } = aggregate;

        let mut sse_events = Vec::new();

        match client_format {
            crate::converters::ApiFormat::Anthropic => {
                // Generate message_start
                let message_start = serde_json::json!({
                    "type": "message_start",
                    "message": {
                        "id": format!("msg_{}", uuid::Uuid::new_v4()),
                        "type": "message",
                        "role": "assistant",
                        "model": "claude-3-5-sonnet-20241022",
                        "content": [],
                        "stop_reason": null,
                        "stop_sequence": null,
                        "usage": {
                            "input_tokens": 100,
                            "output_tokens": 50
                        }
                    }
                });
                sse_events.push(format!("event: message_start\ndata: {}\n\n", message_start));

                // Generate thinking blocks if present
                for (thinking_idx, thinking_step) in thinking_steps.iter().enumerate() {
                    let ThinkingStep(_, text) = thinking_step;

                    // Content block start for thinking
                    let content_block_start = serde_json::json!({
                        "type": "content_block_start",
                        "index": thinking_idx,
                        "content_block": {
                            "type": "thinking",
                            "thinking": ""
                        }
                    });
                    sse_events.push(format!(
                        "event: content_block_start\ndata: {}\n\n",
                        content_block_start
                    ));

                    // Content block deltas for thinking text (realistic chunking)
                    let chunks = self.chunk_text(text, client_format);
                    for chunk in chunks {
                        let content_block_delta = serde_json::json!({
                            "type": "content_block_delta",
                            "index": thinking_idx,
                            "delta": {
                                "type": "thinking_delta",
                                "thinking": chunk
                            }
                        });
                        sse_events.push(format!(
                            "event: content_block_delta\ndata: {}\n\n",
                            content_block_delta
                        ));
                    }

                    // Content block stop
                    let content_block_stop = serde_json::json!({
                        "type": "content_block_stop",
                        "index": thinking_idx
                    });
                    sse_events.push(format!(
                        "event: content_block_stop\ndata: {}\n\n",
                        content_block_stop
                    ));
                }

                // Calculate the starting index for text content after thinking blocks
                let text_start_index = thinking_steps.len();

                // Generate assistant text blocks if present
                if !assistant_text.is_empty() {
                    let content_block_start = serde_json::json!({
                        "type": "content_block_start",
                        "index": text_start_index,
                        "content_block": {
                            "type": "text",
                            "text": ""
                        }
                    });
                    sse_events.push(format!(
                        "event: content_block_start\ndata: {}\n\n",
                        content_block_start
                    ));

                    // Content block deltas for assistant text (realistic chunking)
                    let chunks = self.chunk_text(&assistant_text, client_format);
                    for chunk in chunks {
                        let content_block_delta = serde_json::json!({
                            "type": "content_block_delta",
                            "index": text_start_index,
                            "delta": {
                                "type": "text_delta",
                                "text": chunk
                            }
                        });
                        sse_events.push(format!(
                            "event: content_block_delta\ndata: {}\n\n",
                            content_block_delta
                        ));
                    }

                    // Content block stop
                    let content_block_stop = serde_json::json!({
                        "type": "content_block_stop",
                        "index": text_start_index
                    });
                    sse_events.push(format!(
                        "event: content_block_stop\ndata: {}\n\n",
                        content_block_stop
                    ));
                }

                // Generate tool calls if present
                for (i, tool_call) in tool_calls.iter().enumerate() {
                    let tool_index = if thinking_steps.is_empty() && assistant_text.is_empty() {
                        i
                    } else if thinking_steps.is_empty() || assistant_text.is_empty() {
                        thinking_steps.len() + (if assistant_text.is_empty() { 0 } else { 1 })
                    } else {
                        thinking_steps.len() + 1
                    } + i;
                    let content_block_start = serde_json::json!({
                        "type": "content_block_start",
                        "index": tool_index,
                        "content_block": {
                            "type": "tool_use",
                            "id": tool_call.id,
                            "name": tool_call.name,
                            "input": serde_json::to_value(&tool_call.args).unwrap_or(serde_json::json!({}))
                        }
                    });
                    sse_events.push(format!(
                        "event: content_block_start\ndata: {}\n\n",
                        content_block_start
                    ));

                    // Tool use doesn't need deltas, just the block
                    let content_block_stop = serde_json::json!({
                        "type": "content_block_stop",
                        "index": tool_index
                    });
                    sse_events.push(format!(
                        "event: content_block_stop\ndata: {}\n\n",
                        content_block_stop
                    ));
                }

                // Final message delta
                let message_delta = serde_json::json!({
                    "type": "message_delta",
                    "delta": {
                        "stop_reason": "end_turn",
                        "usage": {
                            "output_tokens": 150
                        }
                    }
                });
                sse_events.push(format!("event: message_delta\ndata: {}\n\n", message_delta));

                // Message stop
                let message_stop = serde_json::json!({
                    "type": "message_stop"
                });
                sse_events.push(format!("event: message_stop\ndata: {}\n\n", message_stop));
            }
            crate::converters::ApiFormat::OpenAI => {
                // For OpenAI, streaming is simpler - just text deltas
                // OpenAI doesn't expose thinking in streaming responses

                // Create the choice structure
                let choice = serde_json::json!({
                    "index": 0,
                    "delta": {
                        "role": "assistant",
                        "content": null
                    },
                    "finish_reason": null
                });

                let chunk = serde_json::json!({
                    "id": format!("chatcmpl-{}", uuid::Uuid::new_v4()),
                    "object": "chat.completion.chunk",
                    "created": chrono::Utc::now().timestamp(),
                    "model": "gpt-4o",
                    "choices": [choice]
                });
                sse_events.push(format!("data: {}\n\n", chunk));

                // Send content chunks (realistic chunking)
                let chunks = self.chunk_text(&assistant_text, client_format);
                for chunk_text in chunks {
                    let choice = serde_json::json!({
                        "index": 0,
                        "delta": {
                            "content": chunk_text
                        },
                        "finish_reason": null
                    });

                    let chunk = serde_json::json!({
                        "id": format!("chatcmpl-{}", uuid::Uuid::new_v4()),
                        "object": "chat.completion.chunk",
                        "created": chrono::Utc::now().timestamp(),
                        "model": "gpt-4o",
                        "choices": [choice]
                    });
                    sse_events.push(format!("data: {}\n\n", chunk));
                }

                // Final chunk with finish_reason
                let choice = serde_json::json!({
                    "index": 0,
                    "delta": {},
                    "finish_reason": "stop"
                });

                let chunk = serde_json::json!({
                    "id": format!("chatcmpl-{}", uuid::Uuid::new_v4()),
                    "object": "chat.completion.chunk",
                    "created": chrono::Utc::now().timestamp(),
                    "model": "gpt-4o",
                    "choices": [choice],
                    "usage": {
                        "prompt_tokens": 100,
                        "completion_tokens": 50,
                        "total_tokens": 150
                    }
                });
                sse_events.push(format!("data: {}\n\n", chunk));
                sse_events.push("data: [DONE]\n\n".to_string());
            }
            crate::converters::ApiFormat::OpenAIResponses => {
                // Similar to OpenAI but with different structure
                let output_item = serde_json::json!({
                    "type": "message",
                    "role": "assistant",
                    "content": []
                });

                let response = serde_json::json!({
                    "id": format!("resp-{}", uuid::Uuid::new_v4()),
                    "object": "response",
                    "created": chrono::Utc::now().timestamp(),
                    "model": "gpt-4o-mini",
                    "status": "in_progress",
                    "output": [output_item]
                });
                sse_events.push(format!("data: {}\n\n", response));

                // Content deltas (realistic chunking)
                let chunks = self.chunk_text(&assistant_text, client_format);
                for chunk_text in chunks {
                    let output_item = serde_json::json!({
                        "type": "message",
                        "role": "assistant",
                        "content": [{
                            "type": "output_text",
                            "text": chunk_text
                        }]
                    });

                    let response = serde_json::json!({
                        "id": format!("resp-{}", uuid::Uuid::new_v4()),
                        "object": "response",
                        "created": chrono::Utc::now().timestamp(),
                        "model": "gpt-4o-mini",
                        "status": "in_progress",
                        "output": [output_item]
                    });
                    sse_events.push(format!("data: {}\n\n", response));
                }

                // Final response
                let output_item = serde_json::json!({
                    "type": "message",
                    "role": "assistant",
                    "content": [{
                        "type": "output_text",
                        "text": assistant_text
                    }]
                });

                let response = serde_json::json!({
                    "id": format!("resp-{}", uuid::Uuid::new_v4()),
                    "object": "response",
                    "created": chrono::Utc::now().timestamp(),
                    "model": "gpt-4o-mini",
                    "status": "completed",
                    "output": [output_item],
                    "usage": {
                        "input_tokens": 100,
                        "output_tokens": 50,
                        "total_tokens": 150
                    }
                });
                sse_events.push(format!("data: {}\n\n", response));
            }
        }

        // Combine all SSE events into a single response
        let sse_data = sse_events.join("");

        Ok(ProxyResponse {
            status: 200,
            payload: serde_json::Value::Null,
            headers: std::collections::HashMap::from([
                ("content-type".to_string(), "text/event-stream".to_string()),
                ("cache-control".to_string(), "no-cache".to_string()),
            ]),
            sse_data: Some(sse_data),
        })
    }

    /// Helper function to chunk text into realistic pieces for streaming based on API style
    fn chunk_text(&self, text: &str, client_format: crate::converters::ApiFormat) -> Vec<String> {
        // Different chunking strategies based on API provider behavior
        let (min_chunk_size, max_chunk_size) = match client_format {
            // Anthropic tends to emit larger chunks (5-15 characters)
            crate::converters::ApiFormat::Anthropic => (5, 15),
            // OpenAI emits smaller chunks (1-3 characters)
            crate::converters::ApiFormat::OpenAI
            | crate::converters::ApiFormat::OpenAIResponses => (1, 3),
        };

        let mut chunks = Vec::new();
        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            // Vary chunk size slightly for realism
            let chunk_size = if chars.len() - i <= min_chunk_size {
                chars.len() - i // Last chunk gets remaining characters
            } else {
                let variation = (i % 3) as usize; // Simple pseudo-random variation
                (min_chunk_size + variation).min(max_chunk_size).min(chars.len() - i)
            };

            let chunk: String = chars[i..i + chunk_size].iter().collect();
            chunks.push(chunk);
            i += chunk_size;
        }

        chunks
    }
}

fn env_flag(name: &str, default: bool) -> bool {
    std::env::var(name)
        .map(|value| match value.to_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => true,
            "0" | "false" | "no" | "off" => false,
            _ => default,
        })
        .unwrap_or(default)
}
