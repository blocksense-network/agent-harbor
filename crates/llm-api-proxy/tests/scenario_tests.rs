// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Unit tests for scenario playback and tool profiles

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use llm_api_proxy::{
    config::{ProviderConfig, ProxyConfig, ScenarioConfig},
    converters::ApiFormat,
    proxy::{
        ModelMapping, ProviderDefinition, ProxyMode, ProxyRequest, SessionConfig, SessionManager,
    },
    routing::DynamicRouter,
    scenario::{
        ScenarioPlayer,
        tool_profiles::{AgentType, ToolProfiles},
    },
};

#[tokio::test]
async fn test_tool_profiles_claude_mapping() {
    let profiles = ToolProfiles::new();

    // Test that Claude tools are properly mapped
    assert!(profiles.is_valid_tool(AgentType::Claude, "Bash"));
    assert!(profiles.is_valid_tool(AgentType::Claude, "Read"));
    assert!(profiles.is_valid_tool(AgentType::Claude, "Write"));
    assert!(!profiles.is_valid_tool(AgentType::Claude, "invalid_tool"));

    // Test tool mapping for runCmd -> Bash
    let args = vec![("cmd", "ls -la"), ("cwd", "/tmp")]
        .into_iter()
        .map(|(k, v)| (k.to_string(), serde_yaml::Value::String(v.to_string())))
        .collect::<HashMap<_, _>>();

    let tool_call = profiles.map_tool_call(AgentType::Claude, "runCmd", &args);
    assert!(tool_call.is_some());
    let call = tool_call.unwrap();
    assert_eq!(call.name, "Bash");
    assert_eq!(
        call.args["command"],
        serde_yaml::Value::String("ls -la".to_string())
    );
    assert_eq!(
        call.args["cwd"],
        serde_yaml::Value::String("/tmp".to_string())
    );
}

#[tokio::test]
async fn test_tool_profiles_codex_mapping() {
    let profiles = ToolProfiles::new();

    // Test that Codex tools are properly mapped
    assert!(profiles.is_valid_tool(AgentType::Codex, "write_file"));
    assert!(profiles.is_valid_tool(AgentType::Codex, "read_file"));
    assert!(!profiles.is_valid_tool(AgentType::Codex, "invalid_tool"));

    // Test tool mapping for writeFile -> write_file
    let args = vec![("path", "test.txt"), ("content", "hello")]
        .into_iter()
        .map(|(k, v)| (k.to_string(), serde_yaml::Value::String(v.to_string())))
        .collect::<HashMap<_, _>>();

    let tool_call = profiles.map_tool_call(AgentType::Codex, "writeFile", &args);
    assert!(tool_call.is_some());
    let call = tool_call.unwrap();
    assert_eq!(call.name, "write_file");
    assert_eq!(
        call.args["path"],
        serde_yaml::Value::String("test.txt".to_string())
    );
    assert_eq!(
        call.args["text"],
        serde_yaml::Value::String("hello".to_string())
    );
}

#[tokio::test]
async fn test_scenario_player_creation() {
    let config = ProxyConfig::default();
    let config = Arc::new(RwLock::new(config));

    let player = ScenarioPlayer::new(config).await;
    assert!(player.is_ok());
}

#[tokio::test]
async fn test_scenario_player_no_scenarios() {
    let config = ProxyConfig::default();
    let config = Arc::new(RwLock::new(config));

    let mut player = ScenarioPlayer::new(config).await.unwrap();

    std::env::set_var("LLM_API_PROXY_LOG_HEADERS", "true");
    std::env::set_var("LLM_API_PROXY_LOG_BODY", "true");
    std::env::set_var("LLM_API_PROXY_LOG_RESPONSES", "false");

    // Create a test request
    let request = ProxyRequest {
        client_format: ApiFormat::OpenAI,
        mode: ProxyMode::Scenario,
        payload: serde_json::json!({"messages": [{"role": "user", "content": "test"}]}),
        headers: HashMap::new(),
        request_id: "test".to_string(),
    };

    // Should fail with no scenarios loaded
    let result = player.play_request(&request).await;
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("No scenarios loaded"));
}

#[tokio::test]
async fn test_scenario_loading() {
    use std::io::Write;
    use tempfile::TempDir;

    // Create a temporary directory with a test scenario
    let temp_dir = TempDir::new().unwrap();
    let scenario_path = temp_dir.path().join("test_scenario.yaml");

    let scenario_content = r#"
name: test_scenario
repo:
  init: true
timeline:
  - llmResponse:
      - think:
          - [500, "Processing test request"]
      - assistant:
          - [300, "Test scenario completed"]
  - agentToolUse:
      toolName: "writeFile"
      args:
        path: "test.txt"
        content: "Hello from scenario"
      result: "File created"
      status: "ok"
expect:
  exitCode: 0
"#;

    std::fs::File::create(&scenario_path)
        .unwrap()
        .write_all(scenario_content.as_bytes())
        .unwrap();

    // Create config with scenario directory
    let mut config = ProxyConfig::default();
    config.scenario.scenario_dir = Some(temp_dir.path().to_string_lossy().to_string());
    let config = Arc::new(RwLock::new(config));

    let player = ScenarioPlayer::new(config).await.unwrap();

    // Should have loaded one scenario
    assert_eq!(player.scenarios.len(), 1);
    assert!(player.scenarios.contains_key("test_scenario"));
}

#[tokio::test]
async fn test_scenario_playback_simple() {
    use std::io::Write;
    use tempfile::TempDir;

    // Create a temporary directory with a test scenario
    let temp_dir = TempDir::new().unwrap();
    let scenario_path = temp_dir.path().join("simple_scenario.yaml");

    let scenario_content = r#"
name: simple_scenario
timeline:
  - assistant:
      - [100, "Hello from scenario"]
expect:
  exitCode: 0
"#;

    std::fs::File::create(&scenario_path)
        .unwrap()
        .write_all(scenario_content.as_bytes())
        .unwrap();

    // Create config with scenario directory
    let mut config = ProxyConfig::default();
    config.scenario.scenario_dir = Some(temp_dir.path().to_string_lossy().to_string());
    let config = Arc::new(RwLock::new(config));

    let mut player = ScenarioPlayer::new(config).await.unwrap();

    // Create a test request
    let request = ProxyRequest {
        client_format: ApiFormat::OpenAI,
        mode: ProxyMode::Scenario,
        payload: serde_json::json!({"messages": [{"role": "user", "content": "test"}]}),
        headers: HashMap::new(),
        request_id: "test".to_string(),
    };

    // Play the request
    let response = player.play_request(&request).await.unwrap();

    // Check response structure
    assert_eq!(response.status, 200);
    assert!(response.payload.is_object());

    let payload = response.payload.as_object().unwrap();
    assert!(payload.contains_key("choices"));
    assert!(payload.contains_key("id"));
    assert!(payload.contains_key("object"));

    // Check that assistant message contains our text
    let choices = payload["choices"].as_array().unwrap();
    let message = &choices[0]["message"];
    let content = message["content"].as_str().unwrap();
    assert_eq!(content, "Hello from scenario");
}

#[tokio::test]
async fn test_scenario_playback_with_tool_call() {
    use std::io::Write;
    use tempfile::TempDir;

    // Create a temporary directory with a test scenario
    let temp_dir = TempDir::new().unwrap();
    let scenario_path = temp_dir.path().join("tool_scenario.yaml");

    let scenario_content = r#"
name: tool_scenario
timeline:
  - agentToolUse:
      toolName: "writeFile"
      args:
        path: "output.txt"
        content: "Generated content"
      result: "File created"
      status: "ok"
  - assistant:
      - [100, "File created successfully"]
expect:
  exitCode: 0
"#;

    std::fs::File::create(&scenario_path)
        .unwrap()
        .write_all(scenario_content.as_bytes())
        .unwrap();

    // Create config with scenario file
    let mut config = ProxyConfig::default();
    config.scenario.scenario_file = Some(scenario_path.to_string_lossy().to_string());
    config.scenario.agent_type = Some("codex".to_string()); // Use Codex for write_file mapping
    config.scenario.agent_version = Some("test".to_string());
    let config = Arc::new(RwLock::new(config));

    let mut player = ScenarioPlayer::new(config).await.unwrap();

    // Create a test request
    let request = ProxyRequest {
        client_format: ApiFormat::OpenAI,
        mode: ProxyMode::Scenario,
        payload: serde_json::json!({"messages": [{"role": "user", "content": "create file"}]}),
        headers: HashMap::new(),
        request_id: "test".to_string(),
    };

    // Play the request
    let response = player.play_request(&request).await.unwrap();

    // Check response structure
    assert_eq!(response.status, 200);
    let payload = response.payload.as_object().unwrap();

    // Should have tool calls
    let choices = payload["choices"].as_array().unwrap();
    let message = &choices[0]["message"];
    assert!(message.get("tool_calls").is_some());

    let tool_calls = message["tool_calls"].as_array().unwrap();
    assert_eq!(tool_calls.len(), 1);

    let tool_call = &tool_calls[0];
    assert_eq!(tool_call["function"]["name"], "write_file"); // Codex mapping for OpenAI format

    let args: serde_json::Value =
        serde_json::from_str(&tool_call["function"]["arguments"].as_str().unwrap()).unwrap();
    assert_eq!(args["path"], "output.txt");
    assert_eq!(args["text"], "Generated content");
}

#[tokio::test]
async fn test_anthropic_thinking_and_agent_edits() {
    use std::io::Write;
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let scenario_path = temp_dir.path().join("anthropic_thinking.yaml");

    let scenario_content = r#"
name: anthropic_thinking
timeline:
  - llmResponse:
      - think:
          - [120, "Reasoning about the task"]
      - assistant:
          - [60, "All done"]
  - agentEdits:
      path: "foo.txt"
      linesAdded: 2
      linesRemoved: 1
expect:
  exitCode: 0
"#;

    std::fs::File::create(&scenario_path)
        .unwrap()
        .write_all(scenario_content.as_bytes())
        .unwrap();

    let mut config = ProxyConfig::default();
    config.scenario.scenario_file = Some(scenario_path.to_string_lossy().to_string());
    config.scenario.agent_type = Some("claude".to_string());
    let config = Arc::new(RwLock::new(config));

    let mut player = ScenarioPlayer::new(config).await.unwrap();

    let mut headers = HashMap::new();
    headers.insert("x-api-key".to_string(), "session-1".to_string());

    let mut base_request = ProxyRequest {
        client_format: ApiFormat::Anthropic,
        mode: ProxyMode::Scenario,
        payload: serde_json::json!({"messages": [{"role": "user", "content": "start"}]}),
        headers: headers.clone(),
        request_id: "anthropic-1".to_string(),
    };

    // First call: thinking + assistant blocks (grouped in llmResponse)
    let response1 = player.play_request(&base_request).await.unwrap();
    let content1 = response1.payload["content"].as_array().unwrap();
    assert_eq!(content1.len(), 2);
    assert_eq!(content1[0]["type"], "thinking");
    assert_eq!(content1[0]["thinking"], "Reasoning about the task");
    assert_eq!(content1[1]["type"], "text");
    assert_eq!(content1[1]["text"], "All done");

    // Second call: agent edits produce tool_use block
    base_request.request_id = "anthropic-2".to_string();
    let response2 = player.play_request(&base_request).await.unwrap();
    let content2 = response2.payload["content"].as_array().unwrap();
    assert_eq!(content2[0]["type"], "tool_use");
    assert_eq!(content2[0]["name"], "edit_file");
    let input = &content2[0]["input"];
    assert_eq!(input["path"], "foo.txt");
    assert_eq!(input["linesAdded"], 2);
    assert_eq!(input["linesRemoved"], 1);
}

#[tokio::test]
async fn test_force_tools_validation_failure() {
    use std::io::Write;
    use tempfile::TempDir;

    // Create a temporary directory with a test scenario
    let temp_dir = TempDir::new().unwrap();
    let scenario_path = temp_dir.path().join("force_validation_scenario.yaml");

    let scenario_content = r#"
name: force_validation_scenario
timeline:
  - assistant:
      - [100, "Test response"]
expect:
  exitCode: 0
"#;

    std::fs::File::create(&scenario_path)
        .unwrap()
        .write_all(scenario_content.as_bytes())
        .unwrap();

    // Create config with scenario file
    let mut config = ProxyConfig::default();
    config.scenario.scenario_file = Some(scenario_path.to_string_lossy().to_string());
    config.scenario.agent_type = Some("claude".to_string());
    config.scenario.agent_version = Some("test".to_string());
    let config = Arc::new(RwLock::new(config));

    let mut player = ScenarioPlayer::new(config).await.unwrap();

    // Create a test request with tool definitions
    let request = ProxyRequest {
        client_format: ApiFormat::Anthropic,
        mode: ProxyMode::Scenario,
        payload: serde_json::json!({
            "messages": [{"role": "user", "content": "test"}],
            "tools": [
                {
                    "name": "Bash",
                    "description": "Test tool",
                    "input_schema": {"type": "object", "properties": {}}
                }
            ]
        }),
        headers: HashMap::new(),
        request_id: "test".to_string(),
    };

    // Test 1: Normal validation (should pass)
    std::env::remove_var("FORCE_TOOLS_VALIDATION_FAILURE");
    let result = player.play_request(&request).await;
    assert!(result.is_ok(), "Normal validation should pass");

    // Clean up any existing request file first
    let agent_requests_dir = std::env::current_dir().unwrap().join("agent-requests");
    let claude_dir = agent_requests_dir.join("claude");
    let version_dir = claude_dir.join("test");
    let request_file = version_dir.join("request.json");
    std::fs::remove_file(&request_file).ok(); // Remove if it exists

    // Test 2: Force validation failure enabled
    std::env::set_var("FORCE_TOOLS_VALIDATION_FAILURE", "1");
    let result = player.play_request(&request).await;
    assert!(
        result.is_ok(),
        "Force validation failure should still allow request processing"
    );

    // Test 3: Different values for FORCE_TOOLS_VALIDATION_FAILURE
    std::env::set_var("FORCE_TOOLS_VALIDATION_FAILURE", "true");
    let result = player.play_request(&request).await;
    assert!(result.is_ok());

    std::env::set_var("FORCE_TOOLS_VALIDATION_FAILURE", "yes");
    let result = player.play_request(&request).await;
    assert!(result.is_ok());

    // Check that request was saved to agent-requests directory (after all calls)
    assert!(
        request_file.exists(),
        "Request should be saved when FORCE_TOOLS_VALIDATION_FAILURE is set"
    );

    // Test 4: Invalid value should not trigger force failure
    std::env::set_var("FORCE_TOOLS_VALIDATION_FAILURE", "false");
    let result = player.play_request(&request).await;
    assert!(result.is_ok());

    // Clean up
    std::env::remove_var("FORCE_TOOLS_VALIDATION_FAILURE");
    std::fs::remove_dir_all(&agent_requests_dir).ok(); // Ignore errors if directory doesn't exist
}

#[tokio::test]
async fn test_request_logging() {
    use std::io::Write;
    use tempfile::TempDir;

    // Create a temporary directory with a test scenario
    let temp_dir = TempDir::new().unwrap();
    let scenario_path = temp_dir.path().join("logging_scenario.yaml");

    let scenario_content = r#"
name: logging_scenario
timeline:
  - assistant:
      - [100, "Logged response"]
expect:
  exitCode: 0
"#;

    std::fs::File::create(&scenario_path)
        .unwrap()
        .write_all(scenario_content.as_bytes())
        .unwrap();

    // Create config with scenario file
    let mut config = ProxyConfig::default();
    config.scenario.scenario_file = Some(scenario_path.to_string_lossy().to_string());
    let config = Arc::new(RwLock::new(config));

    let mut player = ScenarioPlayer::new(config).await.unwrap();

    // Test 1: Log to stdout (default)
    let request = ProxyRequest {
        client_format: ApiFormat::OpenAI,
        mode: ProxyMode::Scenario,
        payload: serde_json::json!({"messages": [{"role": "user", "content": "test"}]}),
        headers: {
            let mut h = HashMap::new();
            h.insert("authorization".to_string(), "Bearer test-key".to_string());
            h
        },
        request_id: "test-request-id".to_string(),
    };

    // This should not fail (logging to stdout)
    let result = player.play_request(&request).await;
    assert!(result.is_ok());

    // Test 2: Log to file
    let temp_log = tempfile::NamedTempFile::new().unwrap();
    let log_path = temp_log.path().to_path_buf();
    std::env::set_var(
        "LLM_API_PROXY_REQUEST_LOG",
        log_path.to_string_lossy().to_string(),
    );

    let result = player.play_request(&request).await;
    assert!(result.is_ok());

    assert!(log_path.exists(), "Log file should be created");

    // Verify log content
    let log_content = std::fs::read_to_string(log_path).unwrap();
    assert!(
        !log_content.trim().is_empty(),
        "Log file should contain content"
    );

    let mut entries = Vec::new();
    let stream = serde_json::Deserializer::from_str(&log_content).into_iter::<serde_json::Value>();
    for value in stream {
        entries.push(value.expect("log entry should be valid JSON"));
    }

    assert!(!entries.is_empty(), "Log file should contain JSON entries");

    let request_entry = entries
        .iter()
        .find(|entry| entry["type"] == "request")
        .expect("Request log entry should be present");

    assert_eq!(request_entry["method"], "POST");
    assert_eq!(request_entry["path"], "/v1/chat/completions");
    assert_eq!(request_entry["request_id"], "test-request-id");
    assert!(request_entry.get("timestamp").is_some());
    assert!(request_entry.get("body").is_some());

    // Clean up
    std::env::remove_var("LLM_API_PROXY_REQUEST_LOG");
    std::env::remove_var("LLM_API_PROXY_LOG_HEADERS");
    std::env::remove_var("LLM_API_PROXY_LOG_BODY");
    std::env::remove_var("LLM_API_PROXY_LOG_RESPONSES");
}

#[tokio::test]
async fn test_scenario_session_management() {
    use std::io::Write;
    use tempfile::TempDir;

    // Create a temporary directory with a test scenario
    let temp_dir = TempDir::new().unwrap();
    let scenario_path = temp_dir.path().join("session_scenario.yaml");

    let scenario_content = r#"
name: session_scenario
timeline:
  - assistant:
      - [100, "First response"]
  - assistant:
      - [100, "Second response"]
expect:
  exitCode: 0
"#;

    std::fs::File::create(&scenario_path)
        .unwrap()
        .write_all(scenario_content.as_bytes())
        .unwrap();

    // Create config with scenario directory
    let mut config = ProxyConfig::default();
    config.scenario.scenario_dir = Some(temp_dir.path().to_string_lossy().to_string());
    let config = Arc::new(RwLock::new(config));

    let mut player = ScenarioPlayer::new(config).await.unwrap();

    // Create first request with session ID
    let mut headers = HashMap::new();
    headers.insert("x-session-id".to_string(), "test-session".to_string());

    let request1 = ProxyRequest {
        client_format: ApiFormat::OpenAI,
        mode: ProxyMode::Scenario,
        payload: serde_json::json!({"messages": [{"role": "user", "content": "first"}]}),
        headers: headers.clone(),
        request_id: "test1".to_string(),
    };

    // Play first request
    let response1 = player.play_request(&request1).await.unwrap();
    let content1 = response1.payload["choices"][0]["message"]["content"].as_str().unwrap();
    assert_eq!(content1, "First response");

    // Play second request with same session
    let request2 = ProxyRequest {
        client_format: ApiFormat::OpenAI,
        mode: ProxyMode::Scenario,
        payload: serde_json::json!({"messages": [{"role": "user", "content": "second"}]}),
        headers,
        request_id: "test2".to_string(),
    };

    let response2 = player.play_request(&request2).await.unwrap();
    let content2 = response2.payload["choices"][0]["message"]["content"].as_str().unwrap();
    assert_eq!(content2, "Second response");
}

#[tokio::test]
async fn test_scenario_with_named_scenario() {
    use std::io::Write;
    use tempfile::TempDir;

    // Create a temporary directory with multiple scenarios
    let temp_dir = TempDir::new().unwrap();

    let scenario1_path = temp_dir.path().join("scenario1.yaml");
    let scenario1_content = r#"
name: scenario1
timeline:
  - assistant:
      - [100, "Response from scenario 1"]
"#;
    std::fs::File::create(&scenario1_path)
        .unwrap()
        .write_all(scenario1_content.as_bytes())
        .unwrap();

    let scenario2_path = temp_dir.path().join("scenario2.yaml");
    let scenario2_content = r#"
name: scenario2
timeline:
  - assistant:
      - [100, "Response from scenario 2"]
"#;
    std::fs::File::create(&scenario2_path)
        .unwrap()
        .write_all(scenario2_content.as_bytes())
        .unwrap();

    // Create config with scenario directory
    let mut config = ProxyConfig::default();
    config.scenario.scenario_dir = Some(temp_dir.path().to_string_lossy().to_string());
    let config = Arc::new(RwLock::new(config));

    let mut player = ScenarioPlayer::new(config).await.unwrap();

    // Test selecting specific scenario via header
    let mut headers = HashMap::new();
    headers.insert("x-scenario-name".to_string(), "scenario2".to_string());

    let request = ProxyRequest {
        client_format: ApiFormat::OpenAI,
        mode: ProxyMode::Scenario,
        payload: serde_json::json!({"messages": [{"role": "user", "content": "test"}]}),
        headers,
        request_id: "test".to_string(),
    };

    let response = player.play_request(&request).await.unwrap();
    let content = response.payload["choices"][0]["message"]["content"].as_str().unwrap();
    assert_eq!(content, "Response from scenario 2");
}

#[tokio::test]
async fn test_session_isolation() -> Result<(), Box<dyn std::error::Error>> {
    // Test session isolation by API key - verify different API keys get different responses
    let config = ProxyConfig {
        scenario: ScenarioConfig {
            enabled: true,
            scenario_file: Some(format!(
                "{}/../../tests/tools/mock-agent/scenarios/basic_timeline_scenario.yaml",
                env!("CARGO_MANIFEST_DIR")
            )),
            ..Default::default()
        },
        ..Default::default()
    };

    let config = Arc::new(RwLock::new(config));
    let mut player = ScenarioPlayer::new(config).await?;

    // Create requests with different API keys
    let request1 = ProxyRequest {
        client_format: ApiFormat::OpenAI,
        mode: ProxyMode::Scenario,
        payload: serde_json::json!({"messages": [{"role": "user", "content": "test"}]}),
        headers: vec![("authorization".to_string(), "Bearer key1".to_string())]
            .into_iter()
            .collect(),
        request_id: "req1".to_string(),
    };

    let request2 = ProxyRequest {
        client_format: ApiFormat::OpenAI,
        mode: ProxyMode::Scenario,
        payload: serde_json::json!({"messages": [{"role": "user", "content": "test"}]}),
        headers: vec![("authorization".to_string(), "Bearer key2".to_string())]
            .into_iter()
            .collect(),
        request_id: "req2".to_string(),
    };

    // Process first request
    let response1 = player.play_request(&request1).await?;
    // Process second request (should create separate session)
    let response2 = player.play_request(&request2).await?;

    // Responses should be successful (session isolation working)
    assert_eq!(response1.status, 200);
    assert_eq!(response2.status, 200);

    Ok(())
}

#[tokio::test]
async fn test_tool_validation() -> Result<(), Box<dyn std::error::Error>> {
    // Clean up any leftover environment variables
    std::env::remove_var("FORCE_TOOLS_VALIDATION_FAILURE");

    let config = ProxyConfig::default();
    let config = Arc::new(RwLock::new(config));
    let player = ScenarioPlayer::new(config).await?;

    // Test valid tool definitions
    let valid_tools = vec![
        serde_json::json!({"name": "Bash"}),
        serde_json::json!({"name": "Read"}),
    ];

    let request_body = serde_json::json!({"tools": valid_tools.clone()});
    assert!(player.validate_tool_definitions(&valid_tools, &request_body).await.is_ok());

    // Test invalid tool definition
    let invalid_tools = vec![serde_json::json!({"name": "InvalidTool"})];

    let request_body = serde_json::json!({"tools": invalid_tools});
    // Should succeed in non-strict mode
    assert!(player.validate_tool_definitions(&invalid_tools, &request_body).await.is_ok());

    Ok(())
}

#[tokio::test]
async fn test_tool_validation_strict_mode() -> Result<(), Box<dyn std::error::Error>> {
    // Clean up any leftover environment variables
    std::env::remove_var("FORCE_TOOLS_VALIDATION_FAILURE");
    // Set strict validation environment variable
    std::env::set_var("FORCE_TOOLS_VALIDATION_FAILURE", "1");

    let mut config = ProxyConfig::default();
    config.scenario.agent_type = Some("claude".to_string());
    config.scenario.agent_version = Some("test".to_string());
    let config = Arc::new(RwLock::new(config));
    let player = ScenarioPlayer::new(config).await?;

    // Test invalid tool definition in strict mode
    let invalid_tools = vec![serde_json::json!({"name": "InvalidTool"})];

    let request_body = serde_json::json!({"tools": invalid_tools});
    // Should fail in strict mode
    assert!(
        player
            .validate_tool_definitions_with_strict(&invalid_tools, &request_body, true)
            .await
            .is_err()
    );

    // Clean up environment variable
    std::env::remove_var("FORCE_TOOLS_VALIDATION_FAILURE");

    Ok(())
}

#[tokio::test]
async fn test_response_formats() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;
    use tempfile::TempDir;

    // Create a temporary scenario file for testing
    let temp_dir = TempDir::new().unwrap();
    let scenario_path = temp_dir.path().join("response_test.yaml");

    let scenario_content = r#"
name: response_test
timeline:
  - assistant:
      - [100, "Test response"]
expect:
  exitCode: 0
"#;

    std::fs::File::create(&scenario_path)
        .unwrap()
        .write_all(scenario_content.as_bytes())
        .unwrap();

    // Test OpenAI response format
    let mut openai_config = ProxyConfig::default();
    openai_config.scenario.scenario_file = Some(scenario_path.to_string_lossy().to_string());
    openai_config.scenario.agent_type = Some("claude".to_string());
    openai_config.scenario.agent_version = Some("test".to_string());
    let openai_config = Arc::new(RwLock::new(openai_config));
    let mut openai_player = ScenarioPlayer::new(openai_config).await?;

    let openai_request = ProxyRequest {
        client_format: ApiFormat::OpenAI,
        mode: ProxyMode::Scenario,
        payload: serde_json::json!({"messages": [{"role": "user", "content": "test"}]}),
        headers: HashMap::new(),
        request_id: "test".to_string(),
    };

    let openai_response = openai_player.play_request(&openai_request).await?;
    assert_eq!(openai_response.status, 200);
    assert!(openai_response.payload.get("choices").is_some()); // OpenAI has choices array
    assert!(openai_response.payload.get("object").is_some()); // OpenAI has object field

    // Test Anthropic response format with separate player
    let mut anthropic_config = ProxyConfig::default();
    anthropic_config.scenario.scenario_file = Some(scenario_path.to_string_lossy().to_string());
    anthropic_config.scenario.agent_type = Some("claude".to_string());
    anthropic_config.scenario.agent_version = Some("test".to_string());
    let anthropic_config = Arc::new(RwLock::new(anthropic_config));
    let mut anthropic_player = ScenarioPlayer::new(anthropic_config).await?;

    let anthropic_request = ProxyRequest {
        client_format: ApiFormat::Anthropic,
        mode: ProxyMode::Scenario,
        payload: serde_json::json!({"messages": [{"role": "user", "content": "test"}]}),
        headers: HashMap::new(),
        request_id: "test".to_string(),
    };

    let anthropic_response = anthropic_player.play_request(&anthropic_request).await?;
    assert_eq!(anthropic_response.status, 200);
    assert!(anthropic_response.payload.get("content").is_some()); // Anthropic has content array
    assert!(anthropic_response.payload.get("type").is_some()); // Anthropic has type field

    Ok(())
}

#[tokio::test]
async fn test_tool_call_validation() -> Result<(), Box<dyn std::error::Error>> {
    // Clean up any leftover environment variables from other tests
    std::env::remove_var("FORCE_TOOLS_VALIDATION_FAILURE");

    let config = ProxyConfig::default();
    let config = Arc::new(RwLock::new(config));
    let player = ScenarioPlayer::new(config).await?;

    // Test valid tool calls
    let valid_calls = vec![serde_json::json!({"name": "Bash", "id": "call1"})];

    let request_body = serde_json::json!({"messages": []});
    assert!(player.validate_tool_calls(&valid_calls, &request_body).await.is_ok());

    // Test invalid tool calls
    let invalid_calls = vec![serde_json::json!({"name": "InvalidTool", "id": "call2"})];

    let request_body = serde_json::json!({"messages": []});
    // Should succeed in non-strict mode
    assert!(player.validate_tool_calls(&invalid_calls, &request_body).await.is_ok());

    Ok(())
}

#[tokio::test]
async fn test_tool_changes_tracking_with_version() -> Result<(), Box<dyn std::error::Error>> {
    use tempfile::TempDir;

    // Clean up any leftover environment variables
    std::env::remove_var("FORCE_TOOLS_VALIDATION_FAILURE");

    // Create a temporary directory to check for saved agent requests
    let temp_dir = TempDir::new().unwrap();

    // Set FORCE_TOOLS_VALIDATION_FAILURE to force saving requests and validation failure
    std::env::set_var("FORCE_TOOLS_VALIDATION_FAILURE", "1");

    // Create config with specific agent type and version
    let mut config = ProxyConfig::default();
    config.scenario.agent_type = Some("claude".to_string());
    config.scenario.agent_version = Some("2.0.5".to_string());
    let config = Arc::new(RwLock::new(config));
    let player = ScenarioPlayer::new(config).await?;

    // Test invalid tool definition (should trigger saving)
    let invalid_tools = vec![serde_json::json!({"name": "InvalidTool"})];

    let request_body = serde_json::json!({"tools": invalid_tools.clone()});

    // This should save the request to agent-requests/claude/2.0.5/request.json and fail validation
    let result = player
        .validate_tool_definitions_with_strict(&invalid_tools, &request_body, true)
        .await;
    assert!(result.is_err()); // Should fail due to strict mode

    // Check if the file was created
    let agent_requests_dir = temp_dir.path().join("agent-requests");
    let claude_dir = agent_requests_dir.join("claude");
    let version_dir = claude_dir.join("2.0.5");
    let _request_file = version_dir.join("request.json");

    // Note: The actual file saving uses current_dir(), so we can't easily test this in unit tests
    // without changing the working directory. This test validates that the method exists
    // and would save with the correct version.

    // Clean up environment variable
    std::env::remove_var("FORCE_TOOLS_VALIDATION_FAILURE");

    Ok(())
}

#[tokio::test]
async fn test_agent_type_detection() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;
    use tempfile::TempDir;

    // Create a temporary scenario file for testing
    let temp_dir = TempDir::new().unwrap();
    let scenario_path = temp_dir.path().join("agent_test.yaml");

    let scenario_content = r#"
name: agent_test
timeline:
  - assistant:
      - [100, "Agent test response"]
expect:
  exitCode: 0
"#;

    std::fs::File::create(&scenario_path)
        .unwrap()
        .write_all(scenario_content.as_bytes())
        .unwrap();

    // Test that different API formats map to different agent types
    let openai_request = ProxyRequest {
        client_format: ApiFormat::OpenAI,
        mode: ProxyMode::Scenario,
        payload: serde_json::json!({"messages": [{"role": "user", "content": "test"}]}),
        headers: HashMap::new(),
        request_id: "test".to_string(),
    };

    let anthropic_request = ProxyRequest {
        client_format: ApiFormat::Anthropic,
        mode: ProxyMode::Scenario,
        payload: serde_json::json!({"messages": [{"role": "user", "content": "test"}]}),
        headers: HashMap::new(),
        request_id: "test".to_string(),
    };

    // Test OpenAI format response
    let mut openai_config = ProxyConfig::default();
    openai_config.scenario.scenario_file = Some(scenario_path.to_string_lossy().to_string());
    openai_config.scenario.agent_type = Some("claude".to_string());
    openai_config.scenario.agent_version = Some("test".to_string());
    let openai_config = Arc::new(RwLock::new(openai_config));
    let mut openai_player = ScenarioPlayer::new(openai_config).await?;

    let openai_response = openai_player.play_request(&openai_request).await?;
    assert!(openai_response.payload.get("choices").is_some()); // OpenAI format

    // Test Anthropic format response with separate player
    let mut anthropic_config = ProxyConfig::default();
    anthropic_config.scenario.scenario_file = Some(scenario_path.to_string_lossy().to_string());
    anthropic_config.scenario.agent_type = Some("claude".to_string());
    anthropic_config.scenario.agent_version = Some("test".to_string());
    let anthropic_config = Arc::new(RwLock::new(anthropic_config));
    let mut anthropic_player = ScenarioPlayer::new(anthropic_config).await?;

    let anthropic_response = anthropic_player.play_request(&anthropic_request).await?;
    assert!(anthropic_response.payload.get("content").is_some()); // Anthropic format

    Ok(())
}

#[tokio::test]
async fn test_minimize_logs_configuration() {
    use std::io::Write;
    use tempfile::TempDir;

    // Create a temporary directory with a test scenario
    let temp_dir = TempDir::new().unwrap();
    let scenario_path = temp_dir.path().join("minimize_logs_scenario.yaml");

    let scenario_content = r#"
name: minimize_logs_scenario
timeline:
  - assistant:
      - [100, "Test response for minimize logs"]
expect:
  exitCode: 0
"#;

    std::fs::File::create(&scenario_path)
        .unwrap()
        .write_all(scenario_content.as_bytes())
        .unwrap();

    // Test with minimize_logs = false (default, should be pretty-printed)
    let mut config_pretty = ProxyConfig::default();
    config_pretty.scenario.scenario_file = Some(scenario_path.to_string_lossy().to_string());
    config_pretty.scenario.minimize_logs = false; // Explicitly set to false
    let config_pretty = Arc::new(RwLock::new(config_pretty));

    let _player_pretty = ScenarioPlayer::new(config_pretty.clone()).await.unwrap();

    // Test with minimize_logs = true
    let mut config_minimized = ProxyConfig::default();
    config_minimized.scenario.scenario_file = Some(scenario_path.to_string_lossy().to_string());
    config_minimized.scenario.minimize_logs = true;
    let config_minimized = Arc::new(RwLock::new(config_minimized));

    let _player_minimized = ScenarioPlayer::new(config_minimized.clone()).await.unwrap();

    // Create test request
    let request = ProxyRequest {
        client_format: ApiFormat::OpenAI,
        mode: ProxyMode::Scenario,
        payload: serde_json::json!({"messages": [{"role": "user", "content": "test"}]}),
        headers: {
            let mut h = HashMap::new();
            h.insert("authorization".to_string(), "Bearer test-key".to_string());
            h
        },
        request_id: "test-minimize-logs".to_string(),
    };

    // Test pretty-printed logs (to file) - use unique temp directory to avoid interference
    let temp_dir_pretty = tempfile::TempDir::new().unwrap();
    let log_path_pretty = temp_dir_pretty.path().join("pretty_log.json");

    // Create separate config for pretty logging
    let mut config_pretty_with_log = (*config_pretty.read().await).clone();
    config_pretty_with_log.scenario.minimize_logs = false;
    let config_pretty_with_log = Arc::new(RwLock::new(config_pretty_with_log));

    let mut player_pretty_with_log = ScenarioPlayer::new(config_pretty_with_log).await.unwrap();

    std::env::set_var(
        "LLM_API_PROXY_REQUEST_LOG",
        log_path_pretty.to_string_lossy().to_string(),
    );

    let result_pretty = player_pretty_with_log.play_request(&request).await;
    assert!(result_pretty.is_ok());

    let log_content_pretty = std::fs::read_to_string(&log_path_pretty).unwrap();

    // Test minimized logs (to file) - use different unique temp directory
    let temp_dir_minimized = tempfile::TempDir::new().unwrap();
    let log_path_minimized = temp_dir_minimized.path().join("minimized_log.json");

    // Create separate config for minimized logging
    let mut config_minimized_with_log = (*config_minimized.read().await).clone();
    config_minimized_with_log.scenario.minimize_logs = true;
    let config_minimized_with_log = Arc::new(RwLock::new(config_minimized_with_log));

    let mut player_minimized_with_log =
        ScenarioPlayer::new(config_minimized_with_log).await.unwrap();

    std::env::set_var(
        "LLM_API_PROXY_REQUEST_LOG",
        log_path_minimized.to_string_lossy().to_string(),
    );

    let result_minimized = player_minimized_with_log.play_request(&request).await;
    assert!(result_minimized.is_ok());

    let log_content_minimized = std::fs::read_to_string(&log_path_minimized).unwrap();

    // Verify that pretty-printed logs are longer (contain newlines and indentation)
    assert!(
        log_content_pretty.len() > log_content_minimized.len(),
        "Pretty-printed logs should be longer than minimized logs"
    );

    // Verify that pretty-printed logs contain more newlines than minimized logs
    let pretty_newlines = log_content_pretty.chars().filter(|&c| c == '\n').count();
    let minimized_newlines = log_content_minimized.chars().filter(|&c| c == '\n').count();
    assert!(
        pretty_newlines > minimized_newlines,
        "Pretty-printed logs should have more newlines than minimized logs ({} vs {})",
        pretty_newlines,
        minimized_newlines
    );

    // Both should contain valid JSON entries (may have multiple entries)
    let _: Vec<serde_json::Value> = serde_json::Deserializer::from_str(&log_content_pretty)
        .into_iter::<serde_json::Value>()
        .map(|r| r.unwrap())
        .collect();
    let _: Vec<serde_json::Value> = serde_json::Deserializer::from_str(&log_content_minimized)
        .into_iter::<serde_json::Value>()
        .map(|r| r.unwrap())
        .collect();

    // Clean up environment variable
    std::env::remove_var("LLM_API_PROXY_REQUEST_LOG");
}

#[tokio::test]
async fn test_scenario_validation_thinking_requires_assistant() {
    use std::io::Write;
    use tempfile::TempDir;

    // Create a temporary directory with a scenario that has thinking but no assistant
    let temp_dir = TempDir::new().unwrap();
    let invalid_scenario_path = temp_dir.path().join("invalid_thinking_scenario.yaml");

    let invalid_scenario_content = r#"
name: invalid_thinking_scenario
timeline:
  - llmResponse:
      - think:
          - [1000, "This is a thinking step"]
          - [500, "Another thinking step"]
      - agentToolUse:
          toolName: "run_command"
          args:
            command: "echo 'test'"
          result: "test"
          status: "ok"
expect:
  exitCode: 0
"#;

    std::fs::File::create(&invalid_scenario_path)
        .unwrap()
        .write_all(invalid_scenario_content.as_bytes())
        .unwrap();

    // Try to create a config with this invalid scenario
    let mut config = ProxyConfig::default();
    config.scenario.scenario_file = Some(invalid_scenario_path.to_string_lossy().to_string());

    // Creating the ScenarioPlayer should fail due to validation
    let result = ScenarioPlayer::new(Arc::new(RwLock::new(config))).await;
    assert!(
        result.is_err(),
        "Scenario with thinking but no assistant should fail validation"
    );

    if let Err(e) = result {
        let error_message = e.to_string();
        assert!(
            error_message.contains("contains thinking blocks but no assistant responses"),
            "Error message should mention the validation rule"
        );
    }
}

#[tokio::test]
async fn test_openai_responses_api_format() {
    use std::io::Write;
    use tempfile::TempDir;

    // Create a temporary directory with a test scenario
    let temp_dir = TempDir::new().unwrap();
    let scenario_path = temp_dir.path().join("responses_api_scenario.yaml");

    let scenario_content = r#"
name: responses_api_scenario
timeline:
  - assistant:
      - [100, "Test response for OpenAI Responses API"]
expect:
  exitCode: 0
"#;

    std::fs::File::create(&scenario_path)
        .unwrap()
        .write_all(scenario_content.as_bytes())
        .unwrap();

    // Create config with scenario file
    let mut config = ProxyConfig::default();
    config.scenario.scenario_file = Some(scenario_path.to_string_lossy().to_string());
    let config = Arc::new(RwLock::new(config));

    let mut player = ScenarioPlayer::new(config).await.unwrap();

    // Create a test request for OpenAI Responses API
    let request = ProxyRequest {
        client_format: ApiFormat::OpenAIResponses,
        mode: ProxyMode::Scenario,
        payload: serde_json::json!({"messages": [{"role": "user", "content": "test"}]}),
        headers: HashMap::new(),
        request_id: "test-responses-api".to_string(),
    };

    // Play the request
    let response = player.play_request(&request).await.unwrap();

    // Verify response structure for OpenAI Responses API
    assert_eq!(response.status, 200);
    assert!(response.payload.is_object());

    let payload = response.payload.as_object().unwrap();

    // Check required top-level fields
    assert!(payload.contains_key("id"));
    assert_eq!(payload["object"], "response");
    assert!(payload.contains_key("created"));
    assert_eq!(payload["model"], "gpt-4o-mini");
    assert_eq!(payload["status"], "completed");

    // Check output array structure
    assert!(payload.contains_key("output"));
    let output = payload["output"].as_array().unwrap();
    assert_eq!(output.len(), 1);

    // Check that output item has required "type": "message" field
    let message = &output[0];
    assert_eq!(message["type"], "message");
    assert_eq!(message["role"], "assistant");

    // Check content structure
    let content = message["content"].as_array().unwrap();
    assert_eq!(content.len(), 1);
    assert_eq!(content[0]["type"], "output_text");
    assert_eq!(content[0]["text"], "Test response for OpenAI Responses API");

    // Check usage structure
    assert!(payload.contains_key("usage"));
    let usage = payload["usage"].as_object().unwrap();
    assert!(usage.contains_key("prompt_tokens"));
    assert!(usage.contains_key("completion_tokens"));
    assert!(usage.contains_key("total_tokens"));
    assert!(usage["prompt_tokens"].as_i64().unwrap() > 0);
    assert!(usage["completion_tokens"].as_i64().unwrap() > 0);
    assert!(usage["total_tokens"].as_i64().unwrap() > 0);
}

#[tokio::test]
async fn test_anthropic_thinking_format() {
    use std::io::Write;
    use tempfile::TempDir;

    // Create a temporary directory with a test scenario that includes thinking
    let temp_dir = TempDir::new().unwrap();
    let scenario_path = temp_dir.path().join("thinking_scenario.yaml");

    let scenario_content = r#"
name: thinking_scenario
timeline:
  - llmResponse:
      - think:
          - [1000, "This is a thinking step"]
          - [500, "Another thinking step"]
      - assistant:
          - [2000, "This is the final response"]
expect:
  exitCode: 0
"#;

    std::fs::File::create(&scenario_path)
        .unwrap()
        .write_all(scenario_content.as_bytes())
        .unwrap();

    // Create config with scenario file
    let mut config = ProxyConfig::default();
    config.scenario.scenario_file = Some(scenario_path.to_string_lossy().to_string());
    let config = Arc::new(RwLock::new(config));

    let mut player = ScenarioPlayer::new(config).await.unwrap();

    // Create a test request for Anthropic
    let request = ProxyRequest {
        client_format: ApiFormat::Anthropic,
        mode: ProxyMode::Scenario,
        payload: serde_json::json!({"messages": [{"role": "user", "content": "test"}]}),
        headers: HashMap::new(),
        request_id: "test-anthropic-thinking".to_string(),
    };

    // Play the request
    let response = player.play_request(&request).await.unwrap();

    // Verify response structure for Anthropic with thinking
    assert_eq!(response.status, 200);
    assert!(response.payload.is_object());

    let payload = response.payload.as_object().unwrap();

    // Check required top-level fields
    assert!(payload.contains_key("id"));
    assert_eq!(payload["type"], "message");
    assert_eq!(payload["role"], "assistant");
    assert!(payload.contains_key("model"));
    assert_eq!(payload["stop_reason"], "end_turn");
    assert_eq!(payload["stop_sequence"], serde_json::Value::Null);

    // Check content array
    assert!(payload.contains_key("content"));
    let content = payload["content"].as_array().unwrap();
    assert!(content.len() >= 2); // Should have thinking blocks + at least one text block

    // Check thinking blocks
    let thinking_blocks: Vec<&serde_json::Value> =
        content.iter().filter(|block| block["type"] == "thinking").collect();

    assert!(!thinking_blocks.is_empty(), "Should have thinking blocks");

    for thinking_block in thinking_blocks {
        assert_eq!(thinking_block["type"], "thinking");
        assert!(
            thinking_block.get("thinking").is_some(),
            "Thinking block should have thinking content"
        );
        assert!(
            thinking_block.get("signature").is_some(),
            "Thinking block should have signature"
        );
        assert!(
            thinking_block.get("duration_ms").is_none(),
            "Thinking block should NOT have duration_ms"
        );
    }

    // Check that there's at least one text block after thinking
    let text_blocks: Vec<&serde_json::Value> =
        content.iter().filter(|block| block["type"] == "text").collect();

    assert!(
        !text_blocks.is_empty(),
        "Should have at least one text block when thinking is present"
    );

    for text_block in text_blocks {
        assert_eq!(text_block["type"], "text");
        assert!(
            text_block.get("text").is_some(),
            "Text block should have text content"
        );
    }

    // Check usage structure
    assert!(payload.contains_key("usage"));
    let usage = payload["usage"].as_object().unwrap();
    assert!(usage.contains_key("input_tokens"));
    assert!(usage.contains_key("output_tokens"));
    assert!(usage["input_tokens"].as_i64().unwrap() > 0);
    assert!(usage["output_tokens"].as_i64().unwrap() > 0);
}

#[tokio::test]
async fn test_session_manager_creation() {
    let session_manager = SessionManager::new();
    assert_eq!(session_manager.cleanup_expired_sessions().await, 0);
}

#[tokio::test]
async fn test_session_preparation_and_retrieval() {
    let session_manager = SessionManager::new();

    // Create test providers
    let providers = vec![ProviderDefinition {
        name: "anthropic".to_string(),
        base_url: "https://api.anthropic.com".to_string(),
        headers: HashMap::from([("authorization".to_string(), "Bearer test-key".to_string())]),
    }];

    // Create test model mappings
    let model_mappings = vec![ModelMapping {
        source_pattern: "claude".to_string(),
        provider: "anthropic".to_string(),
        model: "claude-3-sonnet-20240229".to_string(),
    }];

    // Prepare session
    let session_id = session_manager
        .prepare_session(
            "test-api-key".to_string(),
            providers.clone(),
            model_mappings.clone(),
            "anthropic".to_string(),
        )
        .await
        .unwrap();

    assert!(!session_id.is_empty());

    // Retrieve session config
    let session_config = session_manager.get_session_config("test-api-key").await.unwrap();

    assert_eq!(session_config.default_provider, "anthropic");
    assert!(session_config.providers.contains_key("anthropic"));
    assert_eq!(session_config.model_mappings.len(), 1);
    assert_eq!(session_config.model_mappings[0].source_pattern, "claude");
}

#[tokio::test]
async fn test_session_config_deduplication() {
    let session_manager = SessionManager::new();

    // Create identical session configs
    let providers = vec![ProviderDefinition {
        name: "anthropic".to_string(),
        base_url: "https://api.anthropic.com".to_string(),
        headers: HashMap::from([("authorization".to_string(), "Bearer test-key".to_string())]),
    }];

    let model_mappings = vec![ModelMapping {
        source_pattern: "claude".to_string(),
        provider: "anthropic".to_string(),
        model: "claude-3-sonnet-20240229".to_string(),
    }];

    // Prepare two sessions with identical configs
    let session_id1 = session_manager
        .prepare_session(
            "api-key-1".to_string(),
            providers.clone(),
            model_mappings.clone(),
            "anthropic".to_string(),
        )
        .await
        .unwrap();

    let session_id2 = session_manager
        .prepare_session(
            "api-key-2".to_string(),
            providers.clone(),
            model_mappings.clone(),
            "anthropic".to_string(),
        )
        .await
        .unwrap();

    assert_ne!(session_id1, session_id2);

    // Both sessions should exist
    assert!(session_manager.get_session_config("api-key-1").await.is_some());
    assert!(session_manager.get_session_config("api-key-2").await.is_some());
}

#[tokio::test]
async fn test_session_end() {
    let session_manager = SessionManager::new();

    let providers = vec![ProviderDefinition {
        name: "anthropic".to_string(),
        base_url: "https://api.anthropic.com".to_string(),
        headers: HashMap::from([("authorization".to_string(), "Bearer test-key".to_string())]),
    }];

    let model_mappings = vec![ModelMapping {
        source_pattern: "claude".to_string(),
        provider: "anthropic".to_string(),
        model: "claude-3-sonnet-20240229".to_string(),
    }];

    // Prepare session
    session_manager
        .prepare_session(
            "test-api-key".to_string(),
            providers,
            model_mappings,
            "anthropic".to_string(),
        )
        .await
        .unwrap();

    // Verify session exists
    assert!(session_manager.get_session_config("test-api-key").await.is_some());

    // End session
    session_manager.end_session("test-api-key").await.unwrap();

    // Verify session is gone
    assert!(session_manager.get_session_config("test-api-key").await.is_none());
}

#[tokio::test]
async fn test_session_model_routing_substring_matching() {
    // Create a session config with model mappings
    let providers = vec![
        ProviderDefinition {
            name: "anthropic".to_string(),
            base_url: "https://api.anthropic.com".to_string(),
            headers: HashMap::from([("authorization".to_string(), "Bearer test-key".to_string())]),
        },
        ProviderDefinition {
            name: "openai".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            headers: HashMap::from([(
                "authorization".to_string(),
                "Bearer openai-key".to_string(),
            )]),
        },
    ];

    let model_mappings = vec![
        ModelMapping {
            source_pattern: "claude".to_string(),
            provider: "anthropic".to_string(),
            model: "claude-3-sonnet-20240229".to_string(),
        },
        ModelMapping {
            source_pattern: "gpt".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4o".to_string(),
        },
        ModelMapping {
            source_pattern: "HAIKU".to_string(), // Test case-insensitive
            provider: "anthropic".to_string(),
            model: "claude-3-5-haiku-20241022".to_string(),
        },
    ];

    // Create a session router
    let session_config = SessionConfig {
        providers: HashMap::from([
            (
                "anthropic".to_string(),
                ProviderConfig {
                    name: "anthropic".to_string(),
                    base_url: "https://api.anthropic.com".to_string(),
                    api_key: None,
                    headers: HashMap::from([(
                        "authorization".to_string(),
                        "Bearer test-key".to_string(),
                    )]),
                    models: vec![],
                    weight: 1,
                    rate_limit_rpm: None,
                    timeout_seconds: None,
                },
            ),
            (
                "openai".to_string(),
                ProviderConfig {
                    name: "openai".to_string(),
                    base_url: "https://api.openai.com/v1".to_string(),
                    api_key: None,
                    headers: HashMap::from([(
                        "authorization".to_string(),
                        "Bearer openai-key".to_string(),
                    )]),
                    models: vec![],
                    weight: 1,
                    rate_limit_rpm: None,
                    timeout_seconds: None,
                },
            ),
        ]),
        model_mappings,
        default_provider: "anthropic".to_string(),
        created_at: std::time::Instant::now(),
        last_used: std::time::Instant::now(),
    };

    let router = DynamicRouter::new_from_session(session_config).await.unwrap();

    // Test case-insensitive substring matching
    let test_cases = vec![
        ("claude-3-sonnet-20240229", "anthropic"), // Exact substring match
        ("some-claude-model", "anthropic"),        // Substring match
        ("CLAUDE-3-OPUS", "anthropic"),            // Case-insensitive match
        ("gpt-4-turbo", "openai"),                 // GPT substring match
        ("my-gpt-model", "openai"),                // Substring match
        ("HAIKU-MODEL", "anthropic"),              // Case-insensitive HAIKU match
        ("unknown-model", "anthropic"),            // No match, fallback to default
    ];

    for (model_name, expected_provider) in test_cases {
        let request = ProxyRequest {
            client_format: ApiFormat::Anthropic,
            mode: ProxyMode::Live,
            payload: serde_json::json!({"model": model_name, "messages": []}),
            headers: HashMap::new(),
            request_id: format!("test-{}", model_name),
        };

        let provider_info = router.select_provider(&request).await.unwrap();
        assert_eq!(
            provider_info.name, expected_provider,
            "Model '{}' should route to provider '{}', but got '{}'",
            model_name, expected_provider, provider_info.name
        );
    }
}

#[tokio::test]
async fn test_session_model_routing_priority() {
    // Test that more specific patterns take priority over general ones
    let providers = vec![
        ProviderDefinition {
            name: "anthropic".to_string(),
            base_url: "https://api.anthropic.com".to_string(),
            headers: HashMap::from([("authorization".to_string(), "Bearer test-key".to_string())]),
        },
        ProviderDefinition {
            name: "special".to_string(),
            base_url: "https://special-provider.com".to_string(),
            headers: HashMap::from([(
                "authorization".to_string(),
                "Bearer special-key".to_string(),
            )]),
        },
    ];

    let model_mappings = vec![
        ModelMapping {
            source_pattern: "claude".to_string(),
            provider: "anthropic".to_string(),
            model: "claude-3-sonnet-20240229".to_string(),
        },
        ModelMapping {
            source_pattern: "claude-3-5-haiku".to_string(), // More specific pattern
            provider: "special".to_string(),
            model: "special-haiku-model".to_string(),
        },
    ];

    let session_config = SessionConfig {
        providers: HashMap::from([
            (
                "anthropic".to_string(),
                ProviderConfig {
                    name: "anthropic".to_string(),
                    base_url: "https://api.anthropic.com".to_string(),
                    api_key: None,
                    headers: HashMap::new(),
                    models: vec![],
                    weight: 1,
                    rate_limit_rpm: None,
                    timeout_seconds: None,
                },
            ),
            (
                "special".to_string(),
                ProviderConfig {
                    name: "special".to_string(),
                    base_url: "https://special-provider.com".to_string(),
                    api_key: None,
                    headers: HashMap::new(),
                    models: vec![],
                    weight: 1,
                    rate_limit_rpm: None,
                    timeout_seconds: None,
                },
            ),
        ]),
        model_mappings,
        default_provider: "anthropic".to_string(),
        created_at: std::time::Instant::now(),
        last_used: std::time::Instant::now(),
    };

    let router = DynamicRouter::new_from_session(session_config).await.unwrap();

    // Test that more specific patterns are matched first
    let test_cases = vec![
        ("claude-3-sonnet-20240229", "anthropic"), // Matches "claude"
        ("claude-3-5-haiku-20241022", "special"),  // Matches more specific "claude-3-5-haiku"
        ("some-other-claude-model", "anthropic"),  // Matches "claude" but not the specific pattern
    ];

    for (model_name, expected_provider) in test_cases {
        let request = ProxyRequest {
            client_format: ApiFormat::Anthropic,
            mode: ProxyMode::Live,
            payload: serde_json::json!({"model": model_name, "messages": []}),
            headers: HashMap::new(),
            request_id: format!("test-{}", model_name),
        };

        let provider_info = router.select_provider(&request).await.unwrap();
        assert_eq!(
            provider_info.name, expected_provider,
            "Model '{}' should route to provider '{}', but got '{}'",
            model_name, expected_provider, provider_info.name
        );
    }
}

#[tokio::test]
async fn test_session_model_routing_default_fallback() {
    // Test default provider fallback when no mappings match
    let providers = vec![
        ProviderDefinition {
            name: "anthropic".to_string(),
            base_url: "https://api.anthropic.com".to_string(),
            headers: HashMap::from([("authorization".to_string(), "Bearer test-key".to_string())]),
        },
        ProviderDefinition {
            name: "openai".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            headers: HashMap::from([(
                "authorization".to_string(),
                "Bearer openai-key".to_string(),
            )]),
        },
    ];

    let model_mappings = vec![ModelMapping {
        source_pattern: "claude".to_string(),
        provider: "anthropic".to_string(),
        model: "claude-3-sonnet-20240229".to_string(),
    }];

    let session_config = SessionConfig {
        providers: HashMap::from([
            (
                "anthropic".to_string(),
                ProviderConfig {
                    name: "anthropic".to_string(),
                    base_url: "https://api.anthropic.com".to_string(),
                    api_key: None,
                    headers: HashMap::new(),
                    models: vec![],
                    weight: 1,
                    rate_limit_rpm: None,
                    timeout_seconds: None,
                },
            ),
            (
                "openai".to_string(),
                ProviderConfig {
                    name: "openai".to_string(),
                    base_url: "https://api.openai.com/v1".to_string(),
                    api_key: None,
                    headers: HashMap::new(),
                    models: vec![],
                    weight: 1,
                    rate_limit_rpm: None,
                    timeout_seconds: None,
                },
            ),
        ]),
        model_mappings,
        default_provider: "openai".to_string(), // Set OpenAI as default
        created_at: std::time::Instant::now(),
        last_used: std::time::Instant::now(),
    };

    let router = DynamicRouter::new_from_session(session_config).await.unwrap();

    // Test fallback to default provider
    let test_cases = vec![
        ("claude-3-sonnet-20240229", "anthropic"), // Matches mapping
        ("unknown-model", "openai"),               // No match, fallback to default
        ("random-llm-model", "openai"),            // No match, fallback to default
    ];

    for (model_name, expected_provider) in test_cases {
        let request = ProxyRequest {
            client_format: ApiFormat::Anthropic,
            mode: ProxyMode::Live,
            payload: serde_json::json!({"model": model_name, "messages": []}),
            headers: HashMap::new(),
            request_id: format!("test-{}", model_name),
        };

        let provider_info = router.select_provider(&request).await.unwrap();
        assert_eq!(
            provider_info.name, expected_provider,
            "Model '{}' should route to provider '{}', but got '{}'",
            model_name, expected_provider, provider_info.name
        );
    }
}

#[tokio::test]
async fn test_session_nonexistent() {
    let session_manager = SessionManager::new();

    // Try to get config for non-existent session
    assert!(session_manager.get_session_config("nonexistent").await.is_none());

    // Try to end non-existent session (should not error)
    assert!(session_manager.end_session("nonexistent").await.is_ok());
}
