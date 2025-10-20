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
//! ## ALGORITHM:
//!
//! ```text
//! FOR each API request with api_key:
//!     IF api_key not seen before:
//!         Create new session with scenario timeline
//!         Reset timeline to beginning
//!
//!     Get current session for api_key:
//!
//!     Skip events that don't generate API responses, advance to next response-generating event/group
//!     WHILE there are more events AND current event is not response-generating:
//!         CASE event.type:
//!             "complete" -> Mark scenario as completed (handled by test harness)
//!             "merge" -> Mark session for merging (handled by test harness)
//!             "advanceMs" -> Advance logical time (handled by test harness)
//!             "userInputs" -> Simulate user input (handled by test harness)
//!             "userCommands" -> Execute user command (handled by test harness)
//!             "userEdits" -> Apply user file edits (handled by test harness)
//!         Advance to next event
//!
//!     IF no more events:
//!         Return final assistant message
//!
//!     // Collect all response elements for this turn (supports both grouped and individual events)
//!     response_parts = []
//!     IF current_event.type == "llmResponse":
//!         // Grouped response: collect all sub-events and execute assertions (see below)
//!         response_parts.extend(current_event.sub_events)
//!     ELSE IF current_event.type in ["think", "runCmd", "grep", "readFile", "listDir", "find", "sed", "agentEdits", "assistant"]:
//!         // Individual response: treat as single-element group (legacy support)
//!         response_parts.append(current_event)
//!
//!     // Note: Tools validation is performed when the CLIENT makes API requests,
//!     // not during scenario processing. The server validates that tool definitions
//!     // sent by the coding agent in tool_calls match the current tools profile.
//!
//!     // Coalesce response parts based on LLM API style (OpenAI vs Anthropic)
//!     // OpenAI: thinking -> internal (not in response), text + tool_calls -> assistant message
//!     // Anthropic: thinking + text + tool_calls -> content blocks in single response
//!     api_response = coalesce_response_parts(response_parts, llm_api_style)
//!
//!     Return api_response
//!
//!     Advance session timeline pointer past consumed event(s)
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

        Ok(scenario)
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
}

/// Response element in an LLM response
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
pub enum ResponseElement {
    /// Thinking event
    Think { think: Vec<ThinkingStep> },
    /// Tool use event
    AgentToolUse { agent_tool_use: ToolUseData },
    /// File edits event
    AgentEdits { agent_edits: FileEditData },
    /// Assistant response event
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

        // Follow the algorithm: skip non-response events, collect response parts
        let response_parts = self.collect_response_parts(scenario)?;
        let aggregated = self.process_response_parts(response_parts)?;

        // Generate API response based on client format (not agent type)
        let response = self.generate_api_response(aggregated, request.client_format)?;

        Ok(response)
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
    fn generate_minimal_response(&self, client_format: crate::converters::ApiFormat) -> Result<ProxyResponse> {
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
                })
            }
        }
    }

    /// Collect response parts from the current scenario event (equivalent to Python _collect_response_parts)
    fn collect_response_parts(&mut self, scenario: &Scenario) -> Result<Vec<ResponsePart>> {
        let mut response_parts = Vec::new();

        // Process events that should execute before the next response (assertions, control events)
        while self.current_event_index < scenario.timeline.len() {
            let current_event = &scenario.timeline[self.current_event_index];

            // Check if this is a response-generating event
            let event_type = self.get_event_type(current_event);

            match event_type.as_deref() {
                // Skip non-response-generating events (handled by test harness)
                Some("complete") | Some("merge") | Some("advanceMs") | Some("userInputs")
                | Some("userCommands") | Some("userEdits") => {
                    self.current_event_index += 1;
                    continue;
                }
                // Execute assertions before next response to verify expected outcomes
                Some("assert") => {
                    self.execute_assertion(current_event)?;
                    self.current_event_index += 1;
                    continue;
                }
                // Individual response events
                Some("think") | Some("runCmd") | Some("grep") | Some("readFile")
                | Some("listDir") | Some("find") | Some("sed") | Some("editFile")
                | Some("writeFile") | Some("task") | Some("webFetch") | Some("webSearch")
                | Some("todoWrite") | Some("notebookEdit") | Some("exitPlanMode")
                | Some("bashOutput") | Some("killShell") | Some("slashCommand")
                | Some("agentEdits") | Some("agentToolUse") | Some("assistant") => {
                    let part = self.event_to_response_part(current_event)?;
                    response_parts.push(part);
                    self.current_event_index += 1;
                    break;
                }
                // Grouped response event
                Some("llmResponse") => {
                    if let TimelineEvent::LlmResponse { llm_response } = current_event {
                        for element in llm_response {
                            let part = self.response_element_to_response_part(element)?;
                            response_parts.push(part);
                        }
                    }
                    self.current_event_index += 1;
                    break;
                }
                _ => {
                    // Unknown event type - skip
                    self.current_event_index += 1;
                    continue;
                }
            }
        }

        // If no response parts were collected (scenario ended), provide a default interactive response
        if response_parts.is_empty() && self.current_event_index >= scenario.timeline.len() {
            // Scenario has ended - provide a default response to keep the session interactive
            response_parts.push(ResponsePart::Assistant(vec![AssistantStep(
                1000,
                "I'm ready to help with your next coding task. What would you like to do?"
                    .to_string(),
            )]));
        }

        Ok(response_parts)
    }

    /// Convert timeline event to response part
    fn event_to_response_part(&self, event: &TimelineEvent) -> Result<ResponsePart> {
        match event {
            TimelineEvent::Event(data) => {
                if let Some(think_steps) = data.get("think") {
                    let steps: Vec<ThinkingStep> = serde_yaml::from_value(think_steps.clone())
                        .map_err(|e| Error::Scenario {
                            message: format!("Failed to parse think steps: {}", e),
                        })?;
                    Ok(ResponsePart::Think(steps))
                } else if let Some(tool_data) = data.get("agentToolUse") {
                    let tool_data: ToolUseData = serde_yaml::from_value(tool_data.clone())
                        .map_err(|e| Error::Scenario {
                            message: format!("Failed to parse tool use data: {}", e),
                        })?;
                    Ok(ResponsePart::ToolUse(tool_data))
                } else if let Some(edit_data) = data.get("agentEdits") {
                    let edit_data: FileEditData = serde_yaml::from_value(edit_data.clone())
                        .map_err(|e| Error::Scenario {
                            message: format!("Failed to parse file edit data: {}", e),
                        })?;
                    Ok(ResponsePart::FileEdit(edit_data))
                } else if let Some(assistant_steps) = data.get("assistant") {
                    let steps: Vec<AssistantStep> = serde_yaml::from_value(assistant_steps.clone())
                        .map_err(|e| Error::Scenario {
                            message: format!("Failed to parse assistant steps: {}", e),
                        })?;
                    Ok(ResponsePart::Assistant(steps))
                } else {
                    Err(Error::Scenario {
                        message: "Unsupported event type in timeline".to_string(),
                    })
                }
            }
            TimelineEvent::Control { .. } => Err(Error::Scenario {
                message: "Control events should be filtered out".to_string(),
            }),
            TimelineEvent::LlmResponse { .. } => Err(Error::Scenario {
                message: "llmResponse should be handled separately".to_string(),
            }),
            TimelineEvent::Assert { .. } => Err(Error::Scenario {
                message: "Assert events should be handled separately".to_string(),
            }),
        }
    }

    /// Convert response element to response part
    fn response_element_to_response_part(&self, element: &ResponseElement) -> Result<ResponsePart> {
        match element {
            ResponseElement::Think { think: steps } => Ok(ResponsePart::Think(steps.clone())),
            ResponseElement::AgentToolUse {
                agent_tool_use: tool_data,
            } => Ok(ResponsePart::ToolUse(tool_data.clone())),
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
                .enumerate()
                .map(|(idx, call)| {
                    serde_json::json!({
                        "id": format!("call_{}", idx),
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
            "model": "mock-model",
            "choices": choices,
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
                "duration_ms": step.0,
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
                "input_tokens": 0,
                "output_tokens": 0
            }
        });

        Ok(ProxyResponse {
            status: 200,
            payload,
            headers: HashMap::new(),
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
            "role": "assistant",
            "content": content_parts,
        }));

        let payload = serde_json::json!({
            "id": format!("resp-{}", Uuid::new_v4()),
            "object": "response",
            "created": Utc::now().timestamp(),
            "model": "mock-model",
            "status": "completed",
            "output": output_items,
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
        })
    }

    /// Execute assertion event to verify filesystem state and other conditions
    fn execute_assertion(&self, event: &TimelineEvent) -> Result<()> {
        match event {
            TimelineEvent::Assert { assert } => {
                // Execute filesystem assertions
                if let Some(fs_assertions) = &assert.fs {
                    self.execute_filesystem_assertions(fs_assertions)?;
                }
                // Note: Other assertion types (text, json, git) are not yet implemented
                // They would be handled here in the future
            }
            TimelineEvent::Event(data) => {
                // Handle legacy assert events in Event format
                if let Some(assert_value) = data.get("assert") {
                    let assertion_data: AssertionData = serde_yaml::from_value(assert_value.clone())
                        .map_err(|e| Error::Scenario {
                            message: format!("Failed to parse legacy assert event: {}", e),
                        })?;

                    // Execute filesystem assertions
                    if let Some(fs_assertions) = &assertion_data.fs {
                        self.execute_filesystem_assertions(fs_assertions)?;
                    }
                    // Note: Other assertion types (text, json, git) are not yet implemented
                    // They would be handled here in the future
                } else {
                    return Err(Error::Scenario {
                        message: "Invalid assertion event format".to_string(),
                    });
                }
            }
            _ => {
                return Err(Error::Scenario {
                    message: "Not an assertion event".to_string(),
                });
            }
        };

        Ok(())
    }

    /// Execute filesystem assertions (fs.exists and fs.notExists)
    fn execute_filesystem_assertions(&self, assertions: &FilesystemAssertions) -> Result<()> {
        // Check files/directories that must exist
        if let Some(exists_paths) = &assertions.exists {
            for path_str in exists_paths {
                let path = std::path::Path::new(path_str);
                if !path.exists() {
                    return Err(Error::Scenario {
                        message: format!("Filesystem assertion failed: path '{}' does not exist", path_str),
                    });
                }
                println!("✓ Filesystem assertion passed: '{}' exists", path_str);
            }
        }

        // Check files/directories that must not exist
        if let Some(not_exists_paths) = &assertions.not_exists {
            for path_str in not_exists_paths {
                let path = std::path::Path::new(path_str);
                if path.exists() {
                    return Err(Error::Scenario {
                        message: format!("Filesystem assertion failed: path '{}' exists but should not", path_str),
                    });
                }
                println!("✓ Filesystem assertion passed: '{}' does not exist", path_str);
            }
        }

        Ok(())
    }

    /// Get event type from timeline event
    fn get_event_type(&self, event: &TimelineEvent) -> Option<String> {
        match event {
            TimelineEvent::Event(data) => {
                // For legacy events, check for type field or infer from keys
                if let Some(serde_yaml::Value::String(type_str)) = data.get("type") {
                    Some(type_str.clone())
                } else {
                    // Infer from keys
                    let keys: Vec<&String> = data.keys().collect();
                    if keys.contains(&&"think".to_string()) {
                        Some("think".to_string())
                    } else if keys.contains(&&"agentToolUse".to_string()) {
                        Some("agentToolUse".to_string())
                    } else if keys.contains(&&"agentEdits".to_string()) {
                        Some("agentEdits".to_string())
                    } else if keys.contains(&&"assistant".to_string()) {
                        Some("assistant".to_string())
                    } else if keys.contains(&&"assert".to_string()) {
                        Some("assert".to_string())
                    } else {
                        None
                    }
                }
            }
            TimelineEvent::Control { event_type, .. } => Some(event_type.clone()),
            TimelineEvent::LlmResponse { .. } => Some("llmResponse".to_string()),
            TimelineEvent::Assert { .. } => Some("assert".to_string()),
        }
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
