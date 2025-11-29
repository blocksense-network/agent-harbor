// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Scenario-Format parser and playback utilities shared across Agent Harbor components.

mod error;
mod loader;
mod matching;
mod model;
mod playback;

pub use error::{Result, ScenarioError};
pub use loader::{ScenarioLoader, ScenarioRecord, ScenarioSource};
pub use matching::{MatchedScenario, ScenarioMatcher};
pub use model::*;
pub use playback::{
    PlaybackEvent, PlaybackEventKind, PlaybackIterator, PlaybackOptions, TimelinePosition,
};

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use tempfile::tempdir;

    use super::*;

    const SAMPLE: &str = r#"
name: demo
initialPrompt: "Fix the failing tests"
timeline:
  - llmResponse:
      - think:
          - relativeTime: 100
            content: "Looking into the issue"
  - agentToolUse:
      toolName: runCmd
      args:
        cmd: "npm test"
      progress:
        - relativeTime: 200
          content: "Running tests"
      result: "All tests passed"
      status: "ok"
  - baseTimeDelta: 500
  - llmResponse:
      - assistant:
          - relativeTime: 100
            content: "All good now"
  - complete: true
"#;

    const CONTROL: &str = r#"
name: control
initialPrompt: "Demonstrate control events"
timeline:
  - llmResponse:
      - think:
          - relativeTime: 100
            content: "Booting"
  - baseTimeDelta: 900
  - userInputs:
      - relativeTime: 1000
        input: "hello world"
        target: tui
  - llmResponse:
      - assistant:
          - relativeTime: 100
            content: "Response after a second"
  - assert:
      text:
        contains:
          - "Done"
  - complete: true
"#;

    const NEW_USER_INPUTS: &str = r#"
name: new_user_inputs
timeline:
  - baseTimeDelta: 100
  - userInputs:
      - relativeTime: 150
        input: "hello modern world"
        target: "tui"
  - complete: true
"#;

    const INVALID_USER_INPUTS: &str = r#"
name: invalid_user_inputs
timeline:
  - llmResponse:
      - think:
          - relativeTime: 100
            content: "wait"
  - userInputs:
      - relativeTime: 50
        input: "too early"
"#;

    const USER_INPUT_PROMPT: &str = r#"
name: user_prompt_demo
acp:
  capabilities:
    loadSession: true
timeline:
  - userInputs:
      - relativeTime: 0
        input: "before boundary"
  - baseTimeDelta: 0
  - sessionStart:
      sessionId: "test-session"
  - userInputs:
      - relativeTime: 0
        input: "after boundary"
"#;

    #[test]
    fn load_and_iterate_scenario() {
        let scenario: Scenario = serde_yaml::from_str(SAMPLE).expect("parse scenario");
        assert_eq!(scenario.name, "demo");
        assert_eq!(
            scenario.initial_prompt.as_deref(),
            Some("Fix the failing tests")
        );
        assert_eq!(scenario.timeline.len(), 5);

        let iterator = PlaybackIterator::new(&scenario, PlaybackOptions::default())
            .expect("build playback iterator");
        let events: Vec<_> = iterator.collect();
        assert!(events.iter().any(|e| matches!(e.kind, PlaybackEventKind::Thinking { .. })));
        assert!(events.iter().any(|e| matches!(e.kind, PlaybackEventKind::ToolStart { .. })));
        assert!(events.iter().any(|e| matches!(e.kind, PlaybackEventKind::Complete)));
    }

    #[test]
    fn loader_reads_directory_sources() {
        let dir = tempdir().expect("tempdir");
        let file_path = dir.path().join("demo.yaml");
        fs::write(&file_path, SAMPLE).expect("write sample");

        let loader =
            ScenarioLoader::from_sources([ScenarioSource::Directory(dir.path().to_path_buf())])
                .expect("load");
        assert_eq!(loader.scenarios().len(), 1);
        assert_eq!(loader.scenarios()[0].scenario.name, "demo");
        assert_eq!(loader.scenarios()[0].path, file_path);
    }

    #[test]
    fn matcher_prefers_closest_prompt() {
        let scenario_a: Scenario = serde_yaml::from_str(SAMPLE).unwrap();
        let mut scenario_b = scenario_a.clone();
        scenario_b.name = "other".into();
        scenario_b.initial_prompt = Some("Implement OAuth login".into());

        let records = vec![
            ScenarioRecord {
                scenario: scenario_a,
                path: PathBuf::from("a.yaml"),
            },
            ScenarioRecord {
                scenario: scenario_b,
                path: PathBuf::from("b.yaml"),
            },
        ];

        let matcher = ScenarioMatcher::new(&records);
        let matched = matcher.best_match("Please fix failing unit tests").expect("match scenario");
        assert_eq!(matched.scenario.name, "demo");
    }

    #[test]
    fn matcher_falls_back_when_no_initial_prompts() {
        let mut scenario_a: Scenario = serde_yaml::from_str(SAMPLE).unwrap();
        scenario_a.initial_prompt = None;
        let records = vec![ScenarioRecord {
            scenario: scenario_a.clone(),
            path: PathBuf::from("a.yaml"),
        }];
        let matcher = ScenarioMatcher::new(&records);
        let matched = matcher.best_match("anything").expect("match");
        assert_eq!(matched.scenario.name, scenario_a.name);
    }

    #[test]
    fn effective_initial_prompt_prefers_post_boundary_user_inputs() {
        let scenario: Scenario = serde_yaml::from_str(USER_INPUT_PROMPT).unwrap();
        assert_eq!(
            scenario.effective_initial_prompt().as_deref(),
            Some("after boundary")
        );
    }

    #[test]
    fn matcher_uses_effective_prompt_when_available() {
        let scenario: Scenario = serde_yaml::from_str(USER_INPUT_PROMPT).unwrap();
        let records = vec![ScenarioRecord {
            scenario,
            path: PathBuf::from("a.yaml"),
        }];

        let matcher: ScenarioMatcher<'_> = ScenarioMatcher::new(&records);
        let matched = matcher.best_match("after boundary").expect("match scenario");
        assert_eq!(matched.scenario.name, "user_prompt_demo");
    }

    #[test]
    fn playback_speed_adjusts_schedule() {
        let scenario: Scenario = serde_yaml::from_str(SAMPLE).unwrap();
        let fast = PlaybackIterator::new(
            &scenario,
            PlaybackOptions {
                speed_multiplier: 0.1,
            },
        )
        .unwrap();
        let slow = PlaybackIterator::new(
            &scenario,
            PlaybackOptions {
                speed_multiplier: 2.0,
            },
        )
        .unwrap();

        let fast_last = fast.last().unwrap();
        let slow_last = slow.last().unwrap();
        assert!(fast_last.at_ms < slow_last.at_ms);
    }

    #[test]
    fn playback_honors_control_and_user_events() {
        let scenario: Scenario = serde_yaml::from_str(CONTROL).unwrap();
        let events: Vec<_> =
            PlaybackIterator::new(&scenario, PlaybackOptions::default()).unwrap().collect();
        let mut saw_user_input = false;
        let mut saw_assert = false;
        for event in &events {
            match &event.kind {
                PlaybackEventKind::UserInput { .. } => saw_user_input = true,
                PlaybackEventKind::Assert(_) => saw_assert = true,
                _ => {}
            }
        }
        assert!(saw_user_input, "userInputs should emit playback events");
        assert!(saw_assert, "assert blocks should emit playback events");
    }

    #[test]
    fn playback_supports_object_user_inputs() {
        let scenario: Scenario = serde_yaml::from_str(NEW_USER_INPUTS).unwrap();
        let events: Vec<_> =
            PlaybackIterator::new(&scenario, PlaybackOptions::default()).unwrap().collect();
        let mut user_at = None;
        for event in &events {
            if let PlaybackEventKind::UserInput { value, target } = &event.kind {
                user_at = Some((event.at_ms, value.clone(), target.clone()));
            }
        }
        let (at_ms, value, target) = user_at.expect("user input emitted");
        assert_eq!(at_ms, 150);
        assert_eq!(value, "hello modern world");
        assert_eq!(target.as_deref(), Some("tui"));
    }

    #[test]
    fn playback_rejects_non_monotonic_user_inputs() {
        let scenario: Scenario = serde_yaml::from_str(INVALID_USER_INPUTS).unwrap();
        let err = PlaybackIterator::new(&scenario, PlaybackOptions::default());
        match err {
            Ok(_) => panic!("expected playback to fail"),
            Err(e) => {
                let msg = format!("{}", e);
                assert!(
                    msg.contains("userInputs relativeTime 50 is earlier"),
                    "unexpected error message: {msg}"
                );
            }
        }
    }

    #[test]
    fn validate_content_blocks() {
        use model::validation;
        use model::{ContentAnnotation, EmbeddedResource, RichContentBlock};

        // Valid text content
        let text_block = RichContentBlock::Text {
            text: "Hello world".to_string(),
            annotations: Some(vec![ContentAnnotation {
                priority: Some(0.8),
                audience: None,
                metadata: None,
            }]),
        };
        assert!(validation::validate_rich_content_block(&text_block, None).is_ok());

        // Invalid empty text
        let empty_text = RichContentBlock::Text {
            text: "".to_string(),
            annotations: None,
        };
        assert!(validation::validate_rich_content_block(&empty_text, None).is_err());

        // Valid image content
        let image_block = RichContentBlock::Image {
            mime_type: "image/png".to_string(),
            path: None,
            data: Some("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==".to_string()),
        };
        assert!(validation::validate_rich_content_block(&image_block, None).is_ok());

        // Invalid image MIME type
        let invalid_image = RichContentBlock::Image {
            mime_type: "text/plain".to_string(),
            path: None,
            data: Some("data".to_string()),
        };
        assert!(validation::validate_rich_content_block(&invalid_image, None).is_err());

        // Valid resource link
        let resource_link = RichContentBlock::ResourceLink {
            uri: "file:///workspace/README.md".to_string(),
            name: "README.md".to_string(),
            mime_type: Some("text/markdown".to_string()),
            title: None,
            description: None,
            size: Some(1024),
            annotations: None,
        };
        assert!(validation::validate_rich_content_block(&resource_link, None).is_ok());

        // Invalid resource link (empty URI)
        let invalid_link = RichContentBlock::ResourceLink {
            uri: "".to_string(),
            name: "test".to_string(),
            mime_type: None,
            title: None,
            description: None,
            size: None,
            annotations: None,
        };
        assert!(validation::validate_rich_content_block(&invalid_link, None).is_err());

        // Valid embedded resource
        let embedded_resource = RichContentBlock::Resource {
            resource: EmbeddedResource {
                uri: "file:///workspace/code.py".to_string(),
                mime_type: "text/x-python".to_string(),
                text: Some("print('hello')".to_string()),
            },
        };
        assert!(validation::validate_rich_content_block(&embedded_resource, None).is_ok());
    }

    #[test]
    fn validate_capability_requirements() {
        use model::validation;
        use model::{AcpPromptCapabilities, RichContentBlock};

        let image_block = RichContentBlock::Image {
            mime_type: "image/png".to_string(),
            path: None,
            data: Some("data".to_string()),
        };

        // Should fail without image capability
        let no_caps = Some(AcpPromptCapabilities {
            image: Some(false),
            audio: None,
            embedded_context: None,
        });
        assert!(
            validation::validate_content_capabilities(std::slice::from_ref(&image_block), &no_caps)
                .is_err()
        );

        // Should pass with image capability
        let image_caps = Some(AcpPromptCapabilities {
            image: Some(true),
            audio: None,
            embedded_context: None,
        });
        assert!(validation::validate_content_capabilities(&[image_block], &image_caps).is_ok());
    }

    #[test]
    fn validate_acp_config() {
        use model::{
            AcpCapabilities, AcpConfig, AcpMcpCapabilities, McpServerConfig, SessionStartData,
            TimelineEvent,
        };

        // Valid ACP config with MCP server
        let valid_config = AcpConfig {
            capabilities: Some(AcpCapabilities {
                load_session: Some(true),
                prompt_capabilities: None,
                mcp_capabilities: Some(AcpMcpCapabilities {
                    http: Some(true),
                    sse: Some(false),
                }),
            }),
            cwd: Some("/tmp/workspace".to_string()),
            mcp_servers: Some(vec![McpServerConfig {
                name: "filesystem".to_string(),
                command: Some("/usr/bin/mcp-server-filesystem".to_string()),
                args: Some(vec!["/tmp/workspace".to_string()]),
                env: Some([("PATH".to_string(), "/usr/bin".to_string())].into()),
            }]),
            unstable: None,
        };

        let scenario = Scenario {
            name: "test".to_string(),
            tags: vec![],
            terminal_ref: None,
            initial_prompt: None,
            repo: None,
            ah: None,
            server: None,
            acp: Some(valid_config),
            timeline: vec![TimelineEvent::SessionStart {
                meta: None,
                session_start: SessionStartData {
                    session_id: None,
                    expected_prompt_response: None,
                },
            }],
            rules: None,
            expect: None,
        };

        assert!(scenario.validate_acp_requirements().is_ok());

        // Invalid ACP config - empty server name
        let invalid_config = AcpConfig {
            capabilities: None,
            cwd: None,
            mcp_servers: Some(vec![McpServerConfig {
                name: "".to_string(),
                command: Some("/usr/bin/cmd".to_string()),
                args: None,
                env: None,
            }]),
            unstable: None,
        };

        let invalid_scenario = Scenario {
            name: "test".to_string(),
            tags: vec![],
            terminal_ref: None,
            initial_prompt: None,
            repo: None,
            ah: None,
            server: None,
            acp: Some(invalid_config),
            timeline: vec![],
            rules: None,
            expect: None,
        };

        assert!(invalid_scenario.validate_acp_requirements().is_err());
    }

    #[test]
    fn validate_setmodel_requires_unstable() {
        use model::{AcpConfig, SetModelData, TimelineEvent};

        // setModel without unstable flag should fail
        let config_without_unstable = AcpConfig {
            capabilities: None,
            cwd: None,
            mcp_servers: None,
            unstable: Some(false),
        };

        let scenario_without_unstable = Scenario {
            name: "test".to_string(),
            tags: vec![],
            terminal_ref: None,
            initial_prompt: None,
            repo: None,
            ah: None,
            server: None,
            acp: Some(config_without_unstable),
            timeline: vec![TimelineEvent::SetModel {
                meta: None,
                set_model: SetModelData {
                    model_id: "claude-3-sonnet".to_string(),
                },
            }],
            rules: None,
            expect: None,
        };

        assert!(scenario_without_unstable.validate_acp_requirements().is_err());

        // setModel with unstable flag should succeed
        let config_with_unstable = AcpConfig {
            capabilities: None,
            cwd: None,
            mcp_servers: None,
            unstable: Some(true),
        };

        let scenario_with_unstable = Scenario {
            name: "test".to_string(),
            tags: vec![],
            terminal_ref: None,
            initial_prompt: None,
            repo: None,
            ah: None,
            server: None,
            acp: Some(config_with_unstable),
            timeline: vec![TimelineEvent::SetModel {
                meta: None,
                set_model: SetModelData {
                    model_id: "claude-3-sonnet".to_string(),
                },
            }],
            rules: None,
            expect: None,
        };

        assert!(scenario_with_unstable.validate_acp_requirements().is_ok());
    }

    #[test]
    fn validate_agent_permission_request() {
        use model::{AgentPermissionRequestData, PermissionOption, TimelineEvent, UserDecision};

        // Test that permission request validation is called during scenario validation
        let request_data = AgentPermissionRequestData {
            session_id: None,
            tool_call: None,
            options: Some(vec![PermissionOption {
                id: "allow".to_string(),
                label: "Allow once".to_string(),
                kind: "allow_once".to_string(),
            }]),
            decision: Some(UserDecision {
                outcome: "selected".to_string(),
                option_id: Some("allow".to_string()),
            }),
            granted: None,
        };

        let scenario = Scenario {
            name: "test".to_string(),
            tags: vec![],
            terminal_ref: None,
            initial_prompt: None,
            repo: None,
            ah: None,
            server: None,
            acp: None,
            timeline: vec![TimelineEvent::AgentPermissionRequest {
                agent_permission_request: request_data,
                meta: None,
            }],
            rules: None,
            expect: None,
        };

        assert!(scenario.validate_acp_requirements().is_ok());

        // Valid permission request with explicit decision
        let valid_request = AgentPermissionRequestData {
            session_id: None,
            tool_call: None,
            options: Some(vec![
                PermissionOption {
                    id: "allow".to_string(),
                    label: "Allow once".to_string(),
                    kind: "allow_once".to_string(),
                },
                PermissionOption {
                    id: "deny".to_string(),
                    label: "Deny once".to_string(),
                    kind: "reject_once".to_string(),
                },
            ]),
            decision: Some(UserDecision {
                outcome: "selected".to_string(),
                option_id: Some("allow".to_string()),
            }),
            granted: None,
        };

        assert!(valid_request.validate().is_ok());
        let effective = valid_request.effective_decision().unwrap();
        assert_eq!(effective.outcome, "selected");
        assert_eq!(effective.option_id, Some("allow".to_string()));

        // Valid permission request with granted shorthand
        let shorthand_request = AgentPermissionRequestData {
            session_id: None,
            tool_call: None,
            options: Some(vec![PermissionOption {
                id: "allow".to_string(),
                label: "Allow once".to_string(),
                kind: "allow_once".to_string(),
            }]),
            decision: None,
            granted: Some(true),
        };

        assert!(shorthand_request.validate().is_ok());
        let effective = shorthand_request.effective_decision().unwrap();
        assert_eq!(effective.outcome, "selected");
        assert_eq!(effective.option_id, Some("allow".to_string()));

        // Invalid - unknown permission kind
        let invalid_kind = AgentPermissionRequestData {
            session_id: None,
            tool_call: None,
            options: Some(vec![PermissionOption {
                id: "invalid".to_string(),
                label: "Invalid".to_string(),
                kind: "invalid_kind".to_string(),
            }]),
            decision: Some(UserDecision {
                outcome: "selected".to_string(),
                option_id: Some("invalid".to_string()),
            }),
            granted: None,
        };

        assert!(invalid_kind.validate().is_err());

        // Invalid - decision without granted, but also granted
        let conflicting_fields = AgentPermissionRequestData {
            session_id: None,
            tool_call: None,
            options: Some(vec![]),
            decision: Some(UserDecision {
                outcome: "cancelled".to_string(),
                option_id: None,
            }),
            granted: Some(true),
        };

        assert!(conflicting_fields.validate().is_err());
    }

    #[test]
    fn parse_session_start_with_expected_prompt_response() {
        use model::SessionStartData;

        let yaml = r#"
sessionId: "test-session-123"
expectedPromptResponse:
  sessionId: "test-session-123"
  stopReason: "end_turn"
  usage:
    inputTokens: 150
    outputTokens: 75
    totalTokens: 225
"#;

        let parsed: SessionStartData = serde_yaml::from_str(yaml).unwrap();

        assert_eq!(parsed.session_id, Some("test-session-123".to_string()));

        let expected = parsed.expected_prompt_response.unwrap();
        assert_eq!(expected.session_id, Some("test-session-123".to_string()));
        assert_eq!(expected.stop_reason, Some("end_turn".to_string()));

        let usage = expected.usage.unwrap();
        assert_eq!(usage.input_tokens, Some(150));
        assert_eq!(usage.output_tokens, Some(75));
        assert_eq!(usage.total_tokens, Some(225));
    }

    #[test]
    fn user_inputs_expected_response_validates() {
        let yaml = r#"
name: expected_response_user_inputs
timeline:
  - userInputs:
      - relativeTime: 0
        input: "hello"
        expectedResponse:
          stopReason: "completed"
          usage:
            inputTokens: 1
            outputTokens: 2
            totalTokens: 3
    "#;

        let scenario: Scenario = serde_yaml::from_str(yaml).unwrap();
        assert!(scenario.validate_acp_requirements().is_ok());
    }

    #[test]
    fn initialize_meta_must_be_mapping() {
        use model::{ClientCapabilities, FilesystemCapabilities, InitializeData, TimelineEvent};

        let scenario = Scenario {
            name: "init_meta".into(),
            tags: vec![],
            terminal_ref: None,
            initial_prompt: None,
            repo: None,
            ah: None,
            server: None,
            acp: None,
            rules: None,
            timeline: vec![TimelineEvent::Initialize {
                initialize: InitializeData {
                    protocol_version: 1,
                    client_capabilities: ClientCapabilities {
                        fs: Some(FilesystemCapabilities {
                            read_text_file: Some(true),
                            write_text_file: Some(true),
                        }),
                        terminal: Some(true),
                    },
                    client_info: None,
                    meta: Some(serde_yaml::Value::String("oops".into())),
                    expected_response: None,
                },
            }],
            expect: None,
        };

        assert!(scenario.validate_acp_requirements().is_err());
    }

    #[test]
    fn validate_acp_capability_baseline() {
        use model::{
            AcpCapabilities, AcpConfig, AcpPromptCapabilities, AssistantStep, ContentBlock,
            RichContentBlock,
        };

        // Test that baseline capabilities (text, resource_link) are always allowed
        let baseline_config = AcpConfig {
            capabilities: Some(AcpCapabilities {
                load_session: None,
                prompt_capabilities: Some(AcpPromptCapabilities {
                    image: Some(false),
                    audio: Some(false),
                    embedded_context: Some(false),
                }),
                mcp_capabilities: None,
            }),
            cwd: None,
            mcp_servers: None,
            unstable: None,
        };

        let scenario_with_baseline = Scenario {
            name: "baseline_test".to_string(),
            tags: vec![],
            terminal_ref: None,
            initial_prompt: None,
            repo: None,
            ah: None,
            server: None,
            acp: Some(baseline_config),
            timeline: vec![TimelineEvent::LlmResponse {
                meta: None,
                llm_response: vec![ResponseElement::Assistant {
                    assistant: vec![AssistantStep {
                        relative_time: 100,
                        content: ContentBlock::Text("Hello world".to_string()),
                    }],
                }],
            }],
            rules: None,
            expect: None,
        };

        assert!(scenario_with_baseline.validate_acp_requirements().is_ok());

        // Test that extended capabilities require explicit enablement
        let extended_config = AcpConfig {
            capabilities: Some(AcpCapabilities {
                load_session: None,
                prompt_capabilities: Some(AcpPromptCapabilities {
                    image: Some(false),            // Image not supported
                    audio: Some(true),             // Audio supported
                    embedded_context: Some(false), // Embedded not supported
                }),
                mcp_capabilities: None,
            }),
            cwd: None,
            mcp_servers: None,
            unstable: None,
        };

        // This should fail - image content in user input without image capability
        let scenario_with_image = Scenario {
            name: "image_test".to_string(),
            tags: vec![],
            terminal_ref: None,
            initial_prompt: None,
            repo: None,
            ah: None,
            server: None,
            acp: Some(extended_config.clone()),
            timeline: vec![TimelineEvent::UserInputs {
                user_inputs: vec![UserInputEntry {
                    relative_time: 100,
                    input: InputContent::Rich(vec![RichContentBlock::Image {
                        mime_type: "image/png".to_string(),
                        path: None,
                        data: Some("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==".to_string()),
                    }]),
                    target: None,
                    meta: None,
                    expected_response: None,
                }],
            }],
            rules: None,
            expect: None,
        };

        assert!(scenario_with_image.validate_acp_requirements().is_err());

        // This should pass - audio content in user input with audio capability
        let scenario_with_audio = Scenario {
            name: "audio_test".to_string(),
            tags: vec![],
            terminal_ref: None,
            initial_prompt: None,
            repo: None,
            ah: None,
            server: None,
            acp: Some(extended_config),
            timeline: vec![TimelineEvent::UserInputs {
                user_inputs: vec![UserInputEntry {
                    relative_time: 100,
                    input: InputContent::Rich(vec![RichContentBlock::Audio {
                        mime_type: "audio/wav".to_string(),
                        path: None,
                        data: Some("UklGRiQAAABXQVZFZm10IBAAAAABAAEAQB8AAABfAAfAK4BEAGAAQACABkYXRhAgAAAAEA=".to_string()),
                    }]),
                    target: None,
                    meta: None,
                    expected_response: None,
                }],
            }],
            rules: None,
            expect: None,
        };

        assert!(scenario_with_audio.validate_acp_requirements().is_ok());
    }

    #[test]
    fn validate_mcp_server_transport_alignment() {
        use model::{AcpCapabilities, AcpConfig, AcpMcpCapabilities, McpServerConfig};

        // Test SSE deprecation warning (validation passes but warning printed)
        let sse_config = AcpConfig {
            capabilities: Some(AcpCapabilities {
                load_session: None,
                prompt_capabilities: None,
                mcp_capabilities: Some(AcpMcpCapabilities {
                    http: Some(true),
                    sse: Some(true), // This should trigger warning
                }),
            }),
            cwd: None,
            mcp_servers: Some(vec![McpServerConfig {
                name: "test_server".to_string(),
                command: Some("/usr/bin/server".to_string()),
                args: None,
                env: None,
            }]),
            unstable: None,
        };

        let scenario = Scenario {
            name: "sse_test".to_string(),
            tags: vec![],
            terminal_ref: None,
            initial_prompt: None,
            repo: None,
            ah: None,
            server: None,
            acp: Some(sse_config),
            timeline: vec![],
            rules: None,
            expect: None,
        };

        // Should validate successfully but print warning
        assert!(scenario.validate_acp_requirements().is_ok());

        // Test HTTP transport capability validation
        let http_only_config = AcpConfig {
            capabilities: Some(AcpCapabilities {
                load_session: None,
                prompt_capabilities: None,
                mcp_capabilities: Some(AcpMcpCapabilities {
                    http: Some(false), // HTTP not supported
                    sse: Some(false),
                }),
            }),
            cwd: None,
            mcp_servers: Some(vec![McpServerConfig {
                name: "http_server".to_string(),
                command: None, // No command = assumes HTTP/SSE transport
                args: None,
                env: None,
            }]),
            unstable: None,
        };

        let scenario_no_http = Scenario {
            name: "http_test".to_string(),
            tags: vec![],
            terminal_ref: None,
            initial_prompt: None,
            repo: None,
            ah: None,
            server: None,
            acp: Some(http_only_config),
            timeline: vec![],
            rules: None,
            expect: None,
        };

        // Should fail - server needs HTTP/SSE transport but neither is enabled
        assert!(scenario_no_http.validate_acp_requirements().is_err());
    }

    #[test]
    fn load_session_requires_boundary_and_capability_alignment() {
        use model::{AcpCapabilities, AcpConfig, SessionStartData, TimelineEvent};

        // loadSession enabled but no boundary -> fail
        let scenario_missing_boundary = Scenario {
            name: "missing_boundary".into(),
            tags: vec![],
            terminal_ref: None,
            initial_prompt: None,
            repo: None,
            ah: None,
            server: None,
            acp: Some(AcpConfig {
                capabilities: Some(AcpCapabilities {
                    load_session: Some(true),
                    prompt_capabilities: None,
                    mcp_capabilities: None,
                }),
                cwd: None,
                mcp_servers: None,
                unstable: None,
            }),
            timeline: vec![],
            rules: None,
            expect: None,
        };
        assert!(scenario_missing_boundary.validate_acp_requirements().is_err());

        // boundary present but capability disabled -> fail
        let scenario_missing_capability = Scenario {
            name: "missing_capability".into(),
            tags: vec![],
            terminal_ref: None,
            initial_prompt: None,
            repo: None,
            ah: None,
            server: None,
            acp: Some(AcpConfig {
                capabilities: Some(AcpCapabilities {
                    load_session: Some(false),
                    prompt_capabilities: None,
                    mcp_capabilities: None,
                }),
                cwd: None,
                mcp_servers: None,
                unstable: None,
            }),
            timeline: vec![TimelineEvent::SessionStart {
                meta: None,
                session_start: SessionStartData {
                    session_id: None,
                    expected_prompt_response: None,
                },
            }],
            rules: None,
            expect: None,
        };
        assert!(scenario_missing_capability.validate_acp_requirements().is_err());

        // capability enabled and boundary present -> ok
        let scenario_aligned = Scenario {
            name: "aligned".into(),
            tags: vec![],
            terminal_ref: None,
            initial_prompt: None,
            repo: None,
            ah: None,
            server: None,
            acp: Some(AcpConfig {
                capabilities: Some(AcpCapabilities {
                    load_session: Some(true),
                    prompt_capabilities: None,
                    mcp_capabilities: None,
                }),
                cwd: None,
                mcp_servers: None,
                unstable: None,
            }),
            timeline: vec![TimelineEvent::SessionStart {
                meta: None,
                session_start: SessionStartData {
                    session_id: Some("sess-1".into()),
                    expected_prompt_response: None,
                },
            }],
            rules: None,
            expect: None,
        };
        assert!(scenario_aligned.validate_acp_requirements().is_ok());
    }

    #[test]
    fn validate_content_block_edge_cases() {
        use model::{ContentAnnotation, RichContentBlock};
        let temp_dir = tempdir().expect("tempdir");
        let img_path = temp_dir.path().join("image.jpg");
        fs::write(&img_path, "fake").expect("write image");

        // Test empty text validation
        let empty_text = RichContentBlock::Text {
            text: "".to_string(),
            annotations: None,
        };
        assert!(validation::validate_rich_content_block(&empty_text, None).is_err());

        // Test invalid MIME types
        let invalid_image = RichContentBlock::Image {
            mime_type: "invalid/mime".to_string(),
            path: None,
            data: Some("data".to_string()),
        };
        assert!(validation::validate_rich_content_block(&invalid_image, None).is_err());

        let invalid_audio = RichContentBlock::Audio {
            mime_type: "invalid/audio".to_string(),
            path: None,
            data: Some("data".to_string()),
        };
        assert!(validation::validate_rich_content_block(&invalid_audio, None).is_err());

        // Test invalid base64
        let invalid_base64 = RichContentBlock::Image {
            mime_type: "image/png".to_string(),
            path: None,
            data: Some("invalid@base64!".to_string()),
        };
        assert!(validation::validate_rich_content_block(&invalid_base64, None).is_err());

        // Test invalid annotations
        let invalid_priority = RichContentBlock::Text {
            text: "test".to_string(),
            annotations: Some(vec![ContentAnnotation {
                priority: Some(1.5), // Invalid - should be 0.0-1.0
                audience: None,
                metadata: None,
            }]),
        };
        assert!(validation::validate_rich_content_block(&invalid_priority, None).is_err());

        // Test valid cases
        let valid_image = RichContentBlock::Image {
            mime_type: "image/jpeg".to_string(),
            path: Some("image.jpg".to_string()),
            data: None, // Path-based, no data needed
        };
        assert!(
            validation::validate_rich_content_block(&valid_image, Some(temp_dir.path())).is_ok()
        );

        let valid_resource = RichContentBlock::Resource {
            resource: model::EmbeddedResource {
                uri: "file:///workspace/file.py".to_string(),
                mime_type: "text/x-python".to_string(),
                text: Some("print('hello')".to_string()),
            },
        };
        assert!(validation::validate_rich_content_block(&valid_resource, None).is_ok());
    }

    #[test]
    fn validate_scenario_acp_integration() {
        use model::{AcpCapabilities, AcpConfig, AcpPromptCapabilities, RichContentBlock};

        // Test scenario with ACP config validates content properly
        let acp_config = AcpConfig {
            capabilities: Some(AcpCapabilities {
                load_session: None,
                prompt_capabilities: Some(AcpPromptCapabilities {
                    image: Some(true),
                    audio: Some(false),
                    embedded_context: Some(true),
                }),
                mcp_capabilities: None,
            }),
            cwd: None,
            mcp_servers: None,
            unstable: Some(true),
        };

        // Valid scenario with supported content types in user input
        let valid_scenario = Scenario {
            name: "integration_test".to_string(),
            tags: vec![],
            terminal_ref: None,
            initial_prompt: None,
            repo: None,
            ah: None,
            server: None,
            acp: Some(acp_config.clone()),
            timeline: vec![
                TimelineEvent::UserInputs {
                    user_inputs: vec![UserInputEntry {
                        relative_time: 100,
                        input: InputContent::Rich(vec![RichContentBlock::Image {
                            mime_type: "image/png".to_string(),
                            path: None,
                            data: Some("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==".to_string()),
                        }]),
                        target: None,
                        meta: None,
                        expected_response: None,
                    }],
                },
                TimelineEvent::SetModel {
        meta: None,
                    set_model: model::SetModelData {
                        model_id: "claude-3-sonnet".to_string(),
                    },
                },
            ],
            rules: None,
            expect: None,
        };

        assert!(valid_scenario.validate_acp_requirements().is_ok());

        // Invalid scenario - uses unsupported audio content in user input
        let invalid_scenario = Scenario {
            name: "invalid_integration_test".to_string(),
            tags: vec![],
            terminal_ref: None,
            initial_prompt: None,
            repo: None,
            ah: None,
            server: None,
            acp: Some(acp_config),
            timeline: vec![TimelineEvent::UserInputs {
                user_inputs: vec![UserInputEntry {
                    relative_time: 100,
                    input: InputContent::Rich(vec![RichContentBlock::Audio {
                        mime_type: "audio/wav".to_string(),
                        path: None,
                        data: Some("UklGRiQAAABXQVZFZm10IBAAAAABAAEAQB8AAABfAAfAK4BEAGAAQACABkYXRhAgAAAAEA=".to_string()),
                    }]),
                    target: None,
                    meta: None,
                    expected_response: None,
                }],
            }],
            rules: None,
            expect: None,
        };

        assert!(invalid_scenario.validate_acp_requirements().is_err());
    }

    #[test]
    fn validate_assistant_content_capabilities() {
        use model::{
            AcpCapabilities, AcpConfig, AcpPromptCapabilities, AssistantStep, ContentBlock,
            RichContentBlock, TimelineEvent,
        };

        let image_block = RichContentBlock::Image {
            mime_type: "image/png".to_string(),
            path: None,
            data: Some("Zg==".to_string()), // base64 for "f"
        };

        let scenario_without_image_cap = Scenario {
            name: "assistant_capabilities_fail".to_string(),
            tags: vec![],
            terminal_ref: None,
            initial_prompt: None,
            repo: None,
            ah: None,
            server: None,
            acp: Some(AcpConfig {
                capabilities: Some(AcpCapabilities {
                    load_session: None,
                    prompt_capabilities: Some(AcpPromptCapabilities {
                        image: Some(false),
                        audio: None,
                        embedded_context: None,
                    }),
                    mcp_capabilities: None,
                }),
                cwd: None,
                mcp_servers: None,
                unstable: None,
            }),
            timeline: vec![TimelineEvent::LlmResponse {
                meta: None,
                llm_response: vec![ResponseElement::Assistant {
                    assistant: vec![AssistantStep {
                        relative_time: 0,
                        content: ContentBlock::Rich(image_block.clone()),
                    }],
                }],
            }],
            rules: None,
            expect: None,
        };

        assert!(scenario_without_image_cap.validate_acp_requirements().is_err());

        let scenario_with_image_cap = Scenario {
            timeline: vec![TimelineEvent::LlmResponse {
                meta: None,
                llm_response: vec![ResponseElement::Assistant {
                    assistant: vec![AssistantStep {
                        relative_time: 0,
                        content: ContentBlock::Rich(image_block),
                    }],
                }],
            }],
            ..scenario_without_image_cap.clone()
        };

        let mut acp = scenario_with_image_cap.acp.clone().unwrap();
        if let Some(caps) = &mut acp.capabilities {
            caps.prompt_capabilities = Some(AcpPromptCapabilities {
                image: Some(true),
                audio: None,
                embedded_context: None,
            });
        }
        let mut scenario_with_image_cap = scenario_with_image_cap;
        scenario_with_image_cap.acp = Some(acp);

        assert!(scenario_with_image_cap.validate_acp_requirements().is_ok());
    }

    #[test]
    fn partition_by_session_start_splits_timeline() {
        use serde_yaml::Value;

        let scenario = Scenario {
            name: "partition".into(),
            tags: vec![],
            terminal_ref: None,
            initial_prompt: None,
            repo: None,
            ah: None,
            server: None,
            acp: Some(model::AcpConfig {
                capabilities: Some(model::AcpCapabilities {
                    load_session: Some(true),
                    prompt_capabilities: None,
                    mcp_capabilities: None,
                }),
                cwd: None,
                mcp_servers: None,
                unstable: None,
            }),
            timeline: vec![
                TimelineEvent::Status {
                    status: "before".into(),
                },
                TimelineEvent::SessionStart {
                    meta: Some(Value::String("meta".into())),
                    session_start: model::SessionStartData {
                        session_id: Some("sess".into()),
                        expected_prompt_response: None,
                    },
                },
                TimelineEvent::Status {
                    status: "after".into(),
                },
            ],
            rules: None,
            expect: None,
        };

        let partitioned = scenario.partition_by_session_start();
        assert_eq!(partitioned.historical.len(), 1);
        assert_eq!(partitioned.live.len(), 1);
        assert!(partitioned.session_start.is_some());
        assert!(partitioned.session_start_meta.is_some());
    }

    #[test]
    fn rules_are_resolved_with_defaults() {
        use model::{Scenario, SymbolTable};
        let yaml = r#"
name: rules_test
rules:
  - default: true
    config:
      server:
        mode: "mock"
      tags: ["base"]
timeline: []
"#;
        let symbols = SymbolTable::new();
        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let resolved = crate::loader::resolve_rules_recursive(value, &symbols).unwrap();
        let scenario: Scenario = serde_yaml::from_value(resolved).unwrap();
        assert_eq!(
            scenario.server.as_ref().and_then(|s| s.mode.clone()),
            Some("mock".into())
        );
        assert!(scenario.tags.contains(&"base".to_string()));
    }

    #[test]
    fn undefined_symbol_rules_do_not_error() {
        use model::{Scenario, SymbolTable};
        let yaml = r#"
name: rules_undefined
rules:
  - when: "$missing_flag"
    config:
      server:
        mode: "mock"
  - default: true
    config:
      server:
        mode: "none"
timeline: []
"#;
        let symbols = SymbolTable::new();
        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let resolved = crate::loader::resolve_rules_recursive(value, &symbols).unwrap();
        let scenario: Scenario = serde_yaml::from_value(resolved).unwrap();
        assert_eq!(scenario.server.unwrap().mode, Some("none".into()));
    }

    #[test]
    fn rule_symbols_control_merging() {
        use model::{Scenario, SymbolTable, SymbolValue};
        let yaml = r#"
name: symbol_rules
rules:
  - when: "$env == \"prod\""
    config:
      server:
        mode: "mock"
  - default: true
    config:
      server:
        mode: "none"
timeline: []
"#;
        let mut symbols = SymbolTable::new();
        symbols.define("env".into(), SymbolValue::String("prod".into()));
        let raw: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let resolved = crate::loader::resolve_rules_recursive(raw, &symbols).unwrap();
        let scenario: Scenario = serde_yaml::from_value(resolved).unwrap();
        assert_eq!(scenario.server.unwrap().mode, Some("mock".into()));
    }

    #[test]
    fn meta_validation_rejects_non_mapping() {
        use model::{AgentFileReadsData, FileReadSpec, TimelineEvent};

        let scenario = Scenario {
            name: "meta_test".to_string(),
            tags: vec![],
            terminal_ref: None,
            initial_prompt: None,
            repo: None,
            ah: None,
            server: None,
            acp: None,
            rules: None,
            timeline: vec![TimelineEvent::AgentFileReads {
                agent_file_reads: AgentFileReadsData {
                    files: vec![FileReadSpec {
                        path: "/tmp/test".to_string(),
                        expected_content: None,
                    }],
                },
                meta: Some(serde_yaml::Value::String("not a map".into())),
            }],
            expect: None,
        };

        assert!(scenario.validate_acp_requirements().is_err());
    }

    #[test]
    fn legacy_events_key_is_rejected() {
        let yaml = r#"
name: legacy
events:
  - think: "old"
"#;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("legacy.yaml");
        std::fs::write(&path, yaml).unwrap();

        let err = Scenario::from_file_with_symbols(&path, &model::SymbolTable::new())
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("legacy"),
            "expected legacy rejection, got {err}"
        );
    }
}
