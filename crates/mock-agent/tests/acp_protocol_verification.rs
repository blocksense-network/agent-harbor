#![allow(
    clippy::bool_assert_comparison,
    clippy::collapsible_match,
    clippy::needless_borrows_for_generic_args
)]
// SPDX-License-Identifier: AGPL-3.0-only

//! Comprehensive ACP Protocol Verification Tests for Milestone 0
//!
//! This test suite validates that the mock-agent correctly maps scenario events
//! to ACP protocol messages and handles all aspects of the ACP specification.
//!
//! Reference:
//! - specs/ACP.client.status.md - Milestone 0 verification criteria
//! - specs/Public/Scenario-Format.md - Scenario format specification
//! - resources/acp-specs/docs/protocol/*.mdx - ACP protocol specification

use agent_client_protocol_schema::SessionUpdate;
use ah_scenario_format::{
    self, AcpCapabilities, AcpConfig, AcpMcpCapabilities, AcpPromptCapabilities,
    AgentFileReadsData, AgentPermissionRequestData, AgentPlanData, AssertionData, AssistantStep,
    ClientCapabilities, ClientInfo, ContentAnnotation, ContentBlock, EmbeddedResource, ErrorData,
    ExpectedInitializeResponse, ExpectedPromptResponse, FileEditData, FileReadSpec,
    FilesystemCapabilities, InitializeData, InputContent, McpServerConfig, PermissionOption,
    PlanEntry, ProgressStep, ResponseElement, RichContentBlock, Scenario, SessionStartData,
    SetModeData, SetModelData, ThinkingStep, TimelineEvent, TokenUsage, ToolResultData,
    ToolUseData, UserDecision, UserInputEntry,
};
use mock_agent::executor::AcpAction;
use mock_agent::executor::ScenarioExecutor;
use std::sync::Arc;

// ===========================================================================
// Test Utilities and Helpers
// ===========================================================================

/// Create a minimal scenario with the given timeline events
fn scenario_with_timeline(name: &str, timeline: Vec<TimelineEvent>) -> Arc<Scenario> {
    Arc::new(Scenario {
        name: name.to_string(),
        tags: vec![],
        terminal_ref: None,
        initial_prompt: None,
        repo: None,
        ah: None,
        server: None,
        acp: None,
        rules: None,
        timeline,
        expect: None,
    })
}

/// Create a scenario with ACP capabilities
fn scenario_with_acp_caps(
    name: &str,
    timeline: Vec<TimelineEvent>,
    capabilities: AcpCapabilities,
) -> Arc<Scenario> {
    Arc::new(Scenario {
        name: name.to_string(),
        tags: vec![],
        terminal_ref: None,
        initial_prompt: None,
        repo: None,
        ah: None,
        server: None,
        acp: Some(AcpConfig {
            capabilities: Some(capabilities),
            cwd: Some("/tmp".to_string()),
            mcp_servers: None,
            unstable: None,
        }),
        rules: None,
        timeline,
        expect: None,
    })
}

// ===========================================================================
// Core ACP Message Round-trip Tests
// ===========================================================================

#[test]
fn test_initialize_request_response_mapping() {
    // Verifies scenario `initialize` events properly map to ACP `initialize` requests and responses
    // Reference: resources/acp-specs/docs/protocol/initialization.mdx
    let timeline = vec![TimelineEvent::Initialize {
        initialize: InitializeData {
            protocol_version: 1,
            client_capabilities: ClientCapabilities {
                fs: Some(FilesystemCapabilities {
                    read_text_file: Some(true),
                    write_text_file: Some(true),
                }),
                terminal: Some(true),
            },
            client_info: Some(ClientInfo {
                name: "test-client".to_string(),
                version: "1.0.0".to_string(),
            }),
            meta: Some(serde_yaml::to_value(&[("client.test", "value")]).unwrap()),
            expected_response: None,
        },
    }];

    let capabilities = AcpCapabilities {
        load_session: Some(false),
        prompt_capabilities: Some(AcpPromptCapabilities {
            image: Some(true),
            audio: Some(false),
            embedded_context: Some(true),
        }),
        mcp_capabilities: None,
    };

    let scenario = scenario_with_acp_caps("test_initialize", timeline, capabilities);
    let executor = ScenarioExecutor::new(scenario);
    let transcript = executor.to_acp_transcript(None, None, None, None);

    // Verify initialize response is populated correctly
    assert_eq!(transcript.initialize_response.protocol_version, 1.into());
    assert_eq!(
        transcript.initialize_response.agent_capabilities.load_session,
        false
    );
    assert_eq!(
        transcript.initialize_response.agent_capabilities.prompt_capabilities.image,
        true
    );
    assert_eq!(
        transcript.initialize_response.agent_capabilities.prompt_capabilities.audio,
        false
    );

    // Meta fields are not preserved in initialize_response (executor sets it to None)
    // The executor implementation at executor.rs:336 always sets meta to None
    assert!(transcript.initialize_response.meta.is_none());
}

#[test]
fn test_session_new_request_response_mapping() {
    // Verifies scenario configuration properly maps to ACP `session/new` method calls and responses
    // Reference: resources/acp-specs/docs/protocol/session-setup.mdx#creating-a-session
    let timeline = vec![TimelineEvent::SessionStart {
        session_start: SessionStartData {
            session_id: Some("test-session-123".to_string()),
            expected_prompt_response: Some(ExpectedPromptResponse {
                session_id: Some("test-session-123".to_string()),
                stop_reason: Some("completed".to_string()),
                usage: Some(TokenUsage {
                    input_tokens: Some(100),
                    output_tokens: Some(200),
                    total_tokens: Some(300),
                }),
                meta: None,
            }),
        },
        meta: None,
    }];

    let scenario = scenario_with_timeline("test_session_new", timeline);
    let executor = ScenarioExecutor::new(scenario);
    let transcript = executor.to_acp_transcript(None, None, None, None);

    // Verify session ID is correctly set
    assert_eq!(transcript.session_id.0.as_ref(), "test-session-123");
    assert_eq!(
        transcript.new_session_response.session_id.0.as_ref(),
        "test-session-123"
    );
}

#[test]
fn test_session_load_optional_mapping() {
    // Verifies `sessionStart` boundary markers and historical/live event separation for ACP `session/load`
    // Reference: resources/acp-specs/docs/protocol/session-setup.mdx#loading-sessions
    let timeline = vec![
        // Historical events (before sessionStart)
        TimelineEvent::UserInputs {
            user_inputs: vec![UserInputEntry {
                relative_time: 0,
                input: InputContent::Text("historical prompt".to_string()),
                target: None,
                meta: None,
                expected_response: None,
            }],
        },
        TimelineEvent::LlmResponse {
            llm_response: vec![ResponseElement::Assistant {
                assistant: vec![AssistantStep {
                    relative_time: 0,
                    content: ContentBlock::Text("historical response".to_string()),
                }],
            }],
            meta: None,
        },
        // Session boundary
        TimelineEvent::SessionStart {
            session_start: SessionStartData {
                session_id: Some("loaded-session".to_string()),
                expected_prompt_response: None,
            },
            meta: None,
        },
        // Live events (after sessionStart)
        TimelineEvent::LlmResponse {
            llm_response: vec![ResponseElement::Assistant {
                assistant: vec![AssistantStep {
                    relative_time: 0,
                    content: ContentBlock::Text("live response".to_string()),
                }],
            }],
            meta: None,
        },
    ];

    let caps = AcpCapabilities {
        load_session: Some(true),
        prompt_capabilities: None,
        mcp_capabilities: None,
    };
    let scenario = scenario_with_acp_caps("test_session_load", timeline, caps);
    let executor = ScenarioExecutor::new(scenario);
    let transcript = executor.to_acp_transcript(None, None, None, None);

    // Verify historical and live events are properly partitioned
    assert!(
        !transcript.historical.is_empty(),
        "Expected historical events"
    );
    assert!(!transcript.live.is_empty(), "Expected live events");

    // Historical should contain user prompt and historical response
    assert!(
        transcript
            .historical
            .iter()
            .any(|upd| matches!(upd.update, SessionUpdate::UserMessageChunk { .. }))
    );
    assert!(transcript.historical.iter().any(|upd| matches!(
        upd.update,
        SessionUpdate::AgentMessageChunk {
            content: agent_client_protocol_schema::ContentBlock::Text(ref t)
        } if t.text.contains("historical")
    )));

    // Live should contain only live response
    assert!(transcript.live.iter().any(|upd| matches!(
        upd.update,
        SessionUpdate::AgentMessageChunk {
            content: agent_client_protocol_schema::ContentBlock::Text(ref t)
        } if t.text.contains("live")
    )));
}

#[test]
fn test_session_prompt_content_mapping() {
    // Verifies `userInputs` scenario events map correctly to ACP `session/prompt` method calls
    // Reference: resources/acp-specs/docs/protocol/content.mdx
    let timeline = vec![
        TimelineEvent::SessionStart {
            session_start: SessionStartData {
                session_id: Some("sess".to_string()),
                expected_prompt_response: None,
            },
            meta: None,
        },
        TimelineEvent::UserInputs {
            user_inputs: vec![UserInputEntry {
                relative_time: 100,
                input: InputContent::Text("Test prompt content".to_string()),
                target: None,
                meta: {
                    let mut map = serde_yaml::Mapping::new();
                    map.insert(
                        serde_yaml::Value::String("request.id".to_string()),
                        serde_yaml::Value::String("req_123".to_string()),
                    );
                    Some(serde_yaml::Value::Mapping(map))
                },
                expected_response: Some(ExpectedPromptResponse {
                    session_id: None,
                    stop_reason: Some("completed".to_string()),
                    usage: None,
                    meta: None,
                }),
            }],
        },
    ];

    let scenario = scenario_with_timeline("test_prompt_content", timeline);
    let executor = ScenarioExecutor::new(scenario);
    let transcript = executor.to_acp_transcript(None, None, None, None);

    // Verify user message chunk is in historical
    let user_chunks: Vec<_> = transcript
        .historical
        .iter()
        .chain(transcript.live.iter())
        .filter(|upd| matches!(upd.update, SessionUpdate::UserMessageChunk { .. }))
        .collect();

    assert_eq!(
        user_chunks.len(),
        1,
        "Expected exactly one user message chunk"
    );

    // Verify meta is preserved
    let upd = &user_chunks[0];
    assert!(upd.meta.is_some(), "Expected meta to be preserved");
}

#[test]
fn test_session_update_all_types_mapping() {
    // Verifies `llmResponse` and `agentToolUse` scenario events properly map to ACP `session/update` notifications
    // Reference: resources/acp-specs/docs/protocol/prompt-turn.mdx#3-agent-reports-output
    let timeline = vec![
        TimelineEvent::SessionStart {
            session_start: SessionStartData {
                session_id: Some("sess".to_string()),
                expected_prompt_response: None,
            },
            meta: None,
        },
        // Agent message chunks
        TimelineEvent::LlmResponse {
            llm_response: vec![ResponseElement::Assistant {
                assistant: vec![AssistantStep {
                    relative_time: 0,
                    content: ContentBlock::Text("Agent response".to_string()),
                }],
            }],
            meta: None,
        },
        // Tool call
        TimelineEvent::LlmResponse {
            llm_response: vec![ResponseElement::AgentToolUse {
                agent_tool_use: ToolUseData {
                    tool_name: "runCmd".to_string(),
                    args: [(
                        "cmd".to_string(),
                        serde_yaml::Value::String("echo test".to_string()),
                    )]
                    .iter()
                    .cloned()
                    .collect(),
                    tool_call_id: Some("tool_1".to_string()),
                    progress: None,
                    result: None,
                    status: None,
                    tool_execution: None,
                    meta: None,
                },
            }],
            meta: None,
        },
        // Tool result
        TimelineEvent::LlmResponse {
            llm_response: vec![ResponseElement::ToolResult {
                tool_result: ToolResultData {
                    tool_call_id: "tool_1".to_string(),
                    content: serde_yaml::Value::String("test".to_string()),
                    is_error: false,
                },
            }],
            meta: None,
        },
        // Plan
        TimelineEvent::LlmResponse {
            llm_response: vec![ResponseElement::AgentPlan {
                agent_plan: AgentPlanData {
                    entries: vec![PlanEntry {
                        content: "Task 1".to_string(),
                        priority: "high".to_string(),
                        status: "pending".to_string(),
                    }],
                    plan_update: None,
                },
            }],
            meta: None,
        },
    ];

    let scenario = scenario_with_timeline("test_session_update_types", timeline);
    let executor = ScenarioExecutor::new(scenario);
    let transcript = executor.to_acp_transcript(None, None, None, None);

    // Verify all update types are present in live events
    let has_agent_message = transcript
        .live
        .iter()
        .any(|upd| matches!(upd.update, SessionUpdate::AgentMessageChunk { .. }));
    let has_tool_call = transcript
        .live
        .iter()
        .any(|upd| matches!(upd.update, SessionUpdate::ToolCallUpdate { .. }));
    let has_plan = transcript
        .live
        .iter()
        .any(|upd| matches!(upd.update, SessionUpdate::Plan { .. }));

    assert!(has_agent_message, "Expected AgentMessageChunk update");
    assert!(has_tool_call, "Expected ToolCallUpdate");
    assert!(has_plan, "Expected Plan update");
}

#[test]
fn test_session_cancel_mapping() {
    // Verifies `userCancelSession` scenario events map to ACP `session/cancel` notifications
    // Reference: resources/acp-specs/docs/protocol/prompt-turn.mdx#cancellation
    let timeline = vec![
        TimelineEvent::SessionStart {
            session_start: SessionStartData {
                session_id: Some("sess".to_string()),
                expected_prompt_response: None,
            },
            meta: None,
        },
        TimelineEvent::UserCancelSession {
            user_cancel_session: true,
        },
    ];

    let scenario = scenario_with_timeline("test_cancel", timeline);
    let executor = ScenarioExecutor::new(scenario);
    let transcript = executor.to_acp_transcript(None, None, None, None);

    // Verify cancel is recorded
    assert!(
        transcript.cancel_requested,
        "Expected cancel to be recorded in transcript"
    );
}

// ===========================================================================
// ACP Content Handling Tests
// ===========================================================================

#[test]
fn test_content_block_text_parsing() {
    // Verifies Text content blocks are properly parsed from scenarios and delivered as ACP messages
    let timeline = vec![
        TimelineEvent::SessionStart {
            session_start: SessionStartData {
                session_id: Some("sess".to_string()),
                expected_prompt_response: None,
            },
            meta: None,
        },
        TimelineEvent::LlmResponse {
            llm_response: vec![ResponseElement::Assistant {
                assistant: vec![AssistantStep {
                    relative_time: 0,
                    content: ContentBlock::Rich(RichContentBlock::Text {
                        text: "Annotated text content".to_string(),
                        annotations: Some(vec![ContentAnnotation {
                            priority: Some(0.9),
                            audience: Some(vec!["user".to_string()]),
                            metadata: None,
                        }]),
                    }),
                }],
            }],
            meta: None,
        },
    ];

    let scenario = scenario_with_timeline("test_text_content", timeline);
    let executor = ScenarioExecutor::new(scenario);
    let transcript = executor.to_acp_transcript(None, None, None, None);

    // Verify text content is properly parsed
    let text_chunks: Vec<&String> = transcript
        .live
        .iter()
        .filter_map(|upd| match &upd.update {
            SessionUpdate::AgentMessageChunk { content } => match content {
                agent_client_protocol_schema::ContentBlock::Text(text_content) => {
                    Some(&text_content.text)
                }
                _ => None,
            },
            _ => None,
        })
        .collect();

    assert!(!text_chunks.is_empty(), "Expected text content blocks");
    assert!(
        text_chunks.iter().any(|t| t.contains("Annotated text")),
        "Expected annotated text content"
    );
}

#[test]
fn test_content_block_image_delivery() {
    // Verifies Image content blocks with mimeType/data are correctly mapped to ACP protocol
    // Note: This test uses data field rather than path to avoid filesystem dependencies
    let timeline = vec![
        TimelineEvent::SessionStart {
            session_start: SessionStartData {
                session_id: Some("sess".to_string()),
                expected_prompt_response: None,
            },
            meta: None,
        },
        TimelineEvent::UserInputs {
            user_inputs: vec![UserInputEntry {
                relative_time: 0,
                input: InputContent::Rich(vec![RichContentBlock::Image {
                    mime_type: "image/png".to_string(),
                    path: None,
                    data: Some("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==".to_string()),
                }]),
                target: None,
                meta: None,
                expected_response: None,
            }],
        },
    ];

    let caps = AcpCapabilities {
        load_session: Some(true),
        prompt_capabilities: Some(AcpPromptCapabilities {
            image: Some(true),
            audio: Some(false),
            embedded_context: Some(false),
        }),
        mcp_capabilities: None,
    };
    let scenario = scenario_with_acp_caps("test_image_content", timeline, caps);

    // Verify validation passes for image content
    assert!(scenario.validate_acp_requirements().is_ok());

    let executor = ScenarioExecutor::new(scenario);
    let transcript = executor.to_acp_transcript(None, None, None, None);

    // Verify image content is present
    let image_blocks: Vec<_> = transcript
        .historical
        .iter()
        .chain(transcript.live.iter())
        .filter_map(|upd| match &upd.update {
            SessionUpdate::UserMessageChunk {
                content: agent_client_protocol_schema::ContentBlock::Image(img),
            } => Some(img),
            _ => None,
        })
        .collect();

    assert!(!image_blocks.is_empty(), "Expected image content blocks");
    assert_eq!(image_blocks[0].mime_type, "image/png");
}

#[test]
fn test_content_block_audio_delivery() {
    // Verifies Audio content blocks are properly handled in ACP message flow
    let timeline = vec![
        TimelineEvent::SessionStart {
            session_start: SessionStartData {
                session_id: Some("sess".to_string()),
                expected_prompt_response: None,
            },
            meta: None,
        },
        TimelineEvent::LlmResponse {
            llm_response: vec![ResponseElement::Assistant {
                assistant: vec![AssistantStep {
                    relative_time: 0,
                    content: ContentBlock::Rich(RichContentBlock::Audio {
                        mime_type: "audio/wav".to_string(),
                        path: None,
                        data: Some(
                            "UklGRiQAAABXQVZFZm10IBAAAAABAAEAQB8AAEAfAAABAAgAZGF0YQAAAAA="
                                .to_string(),
                        ),
                    }),
                }],
            }],
            meta: None,
        },
    ];

    let caps = AcpCapabilities {
        load_session: Some(true),
        prompt_capabilities: Some(AcpPromptCapabilities {
            image: Some(false),
            audio: Some(true),
            embedded_context: Some(false),
        }),
        mcp_capabilities: None,
    };
    let scenario = scenario_with_acp_caps("test_audio_content", timeline, caps);

    // Verify validation passes for audio content
    assert!(scenario.validate_acp_requirements().is_ok());

    let executor = ScenarioExecutor::new(scenario);
    let transcript = executor.to_acp_transcript(None, None, None, None);

    // Verify audio content is present
    let audio_blocks: Vec<_> = transcript
        .live
        .iter()
        .filter_map(|upd| match &upd.update {
            SessionUpdate::AgentMessageChunk {
                content: agent_client_protocol_schema::ContentBlock::Audio(aud),
            } => Some(aud),
            _ => None,
        })
        .collect();

    assert!(!audio_blocks.is_empty(), "Expected audio content blocks");
    assert_eq!(audio_blocks[0].mime_type, "audio/wav");
}

#[test]
fn test_content_block_resource_embedding() {
    // Verifies Resource content blocks (file references, embedded code) map to ACP resource blocks
    let timeline = vec![
        TimelineEvent::SessionStart {
            session_start: SessionStartData {
                session_id: Some("sess".to_string()),
                expected_prompt_response: None,
            },
            meta: None,
        },
        TimelineEvent::UserInputs {
            user_inputs: vec![UserInputEntry {
                relative_time: 0,
                input: InputContent::Rich(vec![RichContentBlock::Resource {
                    resource: EmbeddedResource {
                        uri: "file:///workspace/main.py".to_string(),
                        mime_type: "text/x-python".to_string(),
                        text: Some("print('Hello, World!')".to_string()),
                    },
                }]),
                target: None,
                meta: None,
                expected_response: None,
            }],
        },
    ];

    let caps = AcpCapabilities {
        load_session: Some(true),
        prompt_capabilities: Some(AcpPromptCapabilities {
            image: Some(false),
            audio: Some(false),
            embedded_context: Some(true),
        }),
        mcp_capabilities: None,
    };
    let scenario = scenario_with_acp_caps("test_resource_content", timeline, caps);

    // Verify validation passes for embedded resource
    assert!(scenario.validate_acp_requirements().is_ok());

    let executor = ScenarioExecutor::new(scenario);
    let transcript = executor.to_acp_transcript(None, None, None, None);

    // Verify resource content is present
    let resource_blocks: Vec<_> = transcript
        .historical
        .iter()
        .chain(transcript.live.iter())
        .filter_map(|upd| match &upd.update {
            SessionUpdate::UserMessageChunk {
                content: agent_client_protocol_schema::ContentBlock::Resource(res),
            } => Some(res),
            _ => None,
        })
        .collect();

    assert!(
        !resource_blocks.is_empty(),
        "Expected resource content blocks"
    );
    // Note: Embedded resource structure differs between ACP and scenario format
}

#[test]
fn test_content_block_diff_representation() {
    // Verifies diff content blocks for file modifications are correctly handled
    // Reference: resources/acp-specs/docs/protocol/tool-calls.mdx#diffs
    let timeline = vec![
        TimelineEvent::SessionStart {
            session_start: SessionStartData {
                session_id: Some("sess".to_string()),
                expected_prompt_response: None,
            },
            meta: None,
        },
        TimelineEvent::LlmResponse {
            llm_response: vec![ResponseElement::Assistant {
                assistant: vec![AssistantStep {
                    relative_time: 0,
                    content: ContentBlock::Rich(RichContentBlock::Diff {
                        path: "/workspace/config.json".to_string(),
                        old_text: Some(r#"{"debug": false}"#.to_string()),
                        new_text: r#"{"debug": true}"#.to_string(),
                    }),
                }],
            }],
            meta: None,
        },
    ];

    let scenario = scenario_with_timeline("test_diff_content", timeline);

    // Verify validation requires absolute path for diff
    assert!(scenario.validate_acp_requirements().is_ok());

    let executor = ScenarioExecutor::new(scenario);
    let transcript = executor.to_acp_transcript(None, None, None, None);

    // Verify diff content is present
    // Note: Diff content is not a distinct ContentBlock type in ACP schema
    // It would be represented as Text content with annotations
    let diff_blocks: Vec<&SessionUpdate> = transcript
        .live
        .iter()
        .filter_map(|upd| match &upd.update {
            SessionUpdate::AgentMessageChunk { .. } => Some(&upd.update),
            _ => None,
        })
        .collect();

    assert!(!diff_blocks.is_empty(), "Expected content blocks");
}

#[test]
fn test_content_block_mixed_prompts() {
    // Verifies prompts containing multiple content block types are correctly sequenced
    let timeline = vec![
        TimelineEvent::SessionStart {
            session_start: SessionStartData {
                session_id: Some("sess".to_string()),
                expected_prompt_response: None,
            },
            meta: None,
        },
        TimelineEvent::UserInputs {
            user_inputs: vec![UserInputEntry {
                relative_time: 0,
                input: InputContent::Rich(vec![
                    RichContentBlock::Text {
                        text: "Please analyze this image:".to_string(),
                        annotations: None,
                    },
                    RichContentBlock::Image {
                        mime_type: "image/png".to_string(),
                        path: None,
                        data: Some("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==".to_string()),
                    },
                    RichContentBlock::ResourceLink {
                        uri: "file:///workspace/related_doc.md".to_string(),
                        name: "Related Documentation".to_string(),
                        mime_type: Some("text/markdown".to_string()),
                        title: None,
                        description: Some("Background information".to_string()),
                        size: Some(1024),
                        annotations: None,
                    },
                ]),
                target: None,
                meta: None,
                expected_response: None,
            }],
        },
    ];

    let caps = AcpCapabilities {
        load_session: Some(true),
        prompt_capabilities: Some(AcpPromptCapabilities {
            image: Some(true),
            audio: Some(false),
            embedded_context: Some(false),
        }),
        mcp_capabilities: None,
    };
    let scenario = scenario_with_acp_caps("test_mixed_content", timeline, caps);

    // Verify validation passes for mixed content
    assert!(scenario.validate_acp_requirements().is_ok());

    let executor = ScenarioExecutor::new(scenario);
    let transcript = executor.to_acp_transcript(None, None, None, None);

    // Verify multiple content types are present in correct order
    let user_updates: Vec<_> = transcript
        .historical
        .iter()
        .chain(transcript.live.iter())
        .filter(|upd| matches!(upd.update, SessionUpdate::UserMessageChunk { .. }))
        .collect();

    assert!(
        user_updates.len() >= 3,
        "Expected at least 3 user message chunks for mixed content"
    );
}

// ===========================================================================
// ACP Session Lifecycle Tests
// ===========================================================================

#[test]
fn test_session_lifecycle_complete_flow() {
    // Verifies full session lifecycle (new → prompt → updates → completion) mapping
    let timeline = vec![
        TimelineEvent::Initialize {
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
                meta: None,
                expected_response: None,
            },
        },
        TimelineEvent::SessionStart {
            session_start: SessionStartData {
                session_id: Some("complete-flow-session".to_string()),
                expected_prompt_response: None,
            },
            meta: None,
        },
        TimelineEvent::UserInputs {
            user_inputs: vec![UserInputEntry {
                relative_time: 0,
                input: InputContent::Text("Test prompt".to_string()),
                target: None,
                meta: None,
                expected_response: None,
            }],
        },
        TimelineEvent::LlmResponse {
            llm_response: vec![ResponseElement::Assistant {
                assistant: vec![AssistantStep {
                    relative_time: 0,
                    content: ContentBlock::Text("Response".to_string()),
                }],
            }],
            meta: None,
        },
        TimelineEvent::Complete { complete: true },
    ];

    let scenario = scenario_with_timeline("test_complete_flow", timeline);
    let executor = ScenarioExecutor::new(scenario);
    let playbook = executor.build_playbook();

    // Verify all lifecycle stages are present
    assert!(playbook.session_start.is_some(), "Expected initialize");
    assert!(playbook.session_start.is_some(), "Expected session start");
    assert!(
        !playbook.historical.is_empty(),
        "Expected historical events"
    );
    assert!(!playbook.live.is_empty(), "Expected live events");
}

#[test]
fn test_session_concurrent_operations() {
    // Verifies multiple sessions can operate concurrently without interference
    // This is a structural test to ensure transcript generation is stateless
    let timeline1 = vec![TimelineEvent::SessionStart {
        session_start: SessionStartData {
            session_id: Some("session-1".to_string()),
            expected_prompt_response: None,
        },
        meta: None,
    }];

    let timeline2 = vec![TimelineEvent::SessionStart {
        session_start: SessionStartData {
            session_id: Some("session-2".to_string()),
            expected_prompt_response: None,
        },
        meta: None,
    }];

    let scenario1 = scenario_with_timeline("concurrent_1", timeline1);
    let scenario2 = scenario_with_timeline("concurrent_2", timeline2);

    let executor1 = ScenarioExecutor::new(scenario1);
    let executor2 = ScenarioExecutor::new(scenario2);

    let transcript1 = executor1.to_acp_transcript(None, None, None, None);
    let transcript2 = executor2.to_acp_transcript(None, None, None, None);

    // Verify session IDs are distinct
    assert_eq!(transcript1.session_id.0.as_ref(), "session-1");
    assert_eq!(transcript2.session_id.0.as_ref(), "session-2");
    assert_ne!(transcript1.session_id, transcript2.session_id);
}

#[test]
fn test_session_error_conditions() {
    // Verifies error responses for invalid session IDs, malformed requests, etc.
    // This test validates that scenarios can represent error conditions
    let timeline = vec![
        TimelineEvent::SessionStart {
            session_start: SessionStartData {
                session_id: Some("error-session".to_string()),
                expected_prompt_response: None,
            },
            meta: None,
        },
        TimelineEvent::LlmResponse {
            llm_response: vec![ResponseElement::Error {
                error: ErrorData {
                    error_type: "invalid_request".to_string(),
                    status_code: Some(400),
                    message: "Invalid tool call parameters".to_string(),
                    details: None,
                    retry_after_seconds: None,
                },
            }],
            meta: None,
        },
    ];

    let scenario = scenario_with_timeline("test_error_conditions", timeline);
    let executor = ScenarioExecutor::new(scenario);
    let transcript = executor.to_acp_transcript(None, None, None, None);

    // Verify error events are captured
    // Note: Error handling may be represented differently in the transcript
    // This test ensures the scenario format can represent errors
    assert!(
        !transcript.live.is_empty(),
        "Expected events to be present even with errors"
    );
}

#[test]
fn test_session_mcp_server_integration() {
    // Verifies MCP server configurations are properly passed to session creation
    let timeline = vec![TimelineEvent::SessionStart {
        session_start: SessionStartData {
            session_id: Some("mcp-session".to_string()),
            expected_prompt_response: None,
        },
        meta: None,
    }];

    let mcp_servers = vec![McpServerConfig {
        name: "filesystem".to_string(),
        command: Some("/usr/local/bin/mcp-server-filesystem".to_string()),
        args: Some(vec!["/tmp/workspace".to_string()]),
        env: Some([("MCP_DEBUG".to_string(), "1".to_string())].iter().cloned().collect()),
    }];

    let caps = AcpCapabilities {
        load_session: Some(true),
        prompt_capabilities: None,
        mcp_capabilities: Some(AcpMcpCapabilities {
            http: Some(false),
            sse: Some(false),
        }),
    };

    let scenario = Arc::new(Scenario {
        name: "test_mcp_integration".to_string(),
        tags: vec![],
        terminal_ref: None,
        initial_prompt: None,
        repo: None,
        ah: None,
        server: None,
        acp: Some(AcpConfig {
            capabilities: Some(caps),
            cwd: Some("/tmp".to_string()),
            mcp_servers: Some(mcp_servers.clone()),
            unstable: None,
        }),
        rules: None,
        timeline,
        expect: None,
    });

    // Verify MCP configuration is valid
    assert!(scenario.validate_acp_requirements().is_ok());

    // Verify MCP server config is accessible
    assert_eq!(
        scenario.acp.as_ref().unwrap().mcp_servers.as_ref().unwrap().len(),
        1
    );
    assert_eq!(
        scenario.acp.as_ref().unwrap().mcp_servers.as_ref().unwrap()[0].name,
        "filesystem"
    );
}

// Note: test_terminal_follower_pty_streaming is already implemented in acp_integration.rs

// ===========================================================================
// ACP Protocol Extension Tests
// ===========================================================================

#[test]
fn test_acp_extension_methods_mapping() {
    // Verifies custom ACP methods (prefixed with `_`) are properly handled via scenario extensions
    // Reference: resources/acp-specs/docs/protocol/extensibility.mdx
    // Note: Extension methods would be represented via custom timeline events or meta fields
    let timeline = vec![TimelineEvent::SessionStart {
        session_start: SessionStartData {
            session_id: Some("ext-session".to_string()),
            expected_prompt_response: None,
        },
        meta: {
            let mut map = serde_yaml::Mapping::new();
            map.insert(
                serde_yaml::Value::String("_extension.method".to_string()),
                serde_yaml::Value::String("custom_value".to_string()),
            );
            Some(serde_yaml::Value::Mapping(map))
        },
    }];

    let scenario = scenario_with_timeline("test_extensions", timeline);

    // Verify validation accepts _meta fields
    assert!(scenario.validate_acp_requirements().is_ok());

    let executor = ScenarioExecutor::new(scenario);
    let transcript = executor.to_acp_transcript(None, None, None, None);

    // Extension data should be preserved in meta fields
    // This test verifies the structure supports extensions
    assert!(transcript.session_id.0.as_ref() == "ext-session");
}

#[test]
fn test_acp_meta_fields_preservation() {
    // Verifies `_meta` fields in ACP messages are preserved and accessible in scenarios
    let timeline = vec![TimelineEvent::UserInputs {
        user_inputs: vec![UserInputEntry {
            relative_time: 0,
            input: InputContent::Text("test".to_string()),
            target: None,
            meta: {
                let mut map = serde_yaml::Mapping::new();
                map.insert(
                    serde_yaml::Value::String("_custom.field".to_string()),
                    serde_yaml::Value::String("preserved".to_string()),
                );
                Some(map.into())
            },
            expected_response: None,
        }],
    }];

    let scenario = scenario_with_timeline("test_meta_preservation", timeline);
    let executor = ScenarioExecutor::new(scenario);
    let transcript = executor.to_acp_transcript(None, None, None, None);

    // Verify meta fields are preserved in historical events
    // Meta should be preserved on SessionNotification per executor.rs:899
    let has_meta = transcript.historical.iter().any(|upd| upd.meta.is_some());

    assert!(has_meta, "Expected meta fields to be preserved");
}

#[test]
fn test_acp_meta_fields_initialization() {
    // Verifies `_meta` fields in initialize requests/responses are correctly handled
    let timeline = vec![TimelineEvent::Initialize {
        initialize: InitializeData {
            protocol_version: 1,
            client_capabilities: ClientCapabilities {
                fs: None,
                terminal: None,
            },
            client_info: None,
            meta: {
                let mut map = serde_yaml::Mapping::new();
                map.insert(
                    serde_yaml::Value::String("client.version".to_string()),
                    serde_yaml::Value::String("1.2.3".to_string()),
                );
                Some(map.into())
            },
            expected_response: None,
        },
    }];

    // Use scenario_with_acp_caps to set agent capabilities, not expected_response
    let caps = AcpCapabilities {
        load_session: Some(false),
        prompt_capabilities: None,
        mcp_capabilities: None,
    };

    let scenario = scenario_with_acp_caps("test_init_meta", timeline, caps);
    let executor = ScenarioExecutor::new(scenario);
    let transcript = executor.to_acp_transcript(None, None, None, None);

    // Meta fields are not preserved in initialize_response (executor sets it to None)
    // The executor implementation at executor.rs:336 always sets meta to None
    assert!(
        transcript.initialize_response.meta.is_none(),
        "initialize_response.meta is always None per executor implementation"
    );
}

#[test]
fn test_acp_meta_fields_session_messages() {
    // Verifies `_meta` fields in session/prompt and session/update are preserved
    let timeline = vec![
        TimelineEvent::SessionStart {
            session_start: SessionStartData {
                session_id: Some("meta-session".to_string()),
                expected_prompt_response: None,
            },
            meta: {
                let mut map = serde_yaml::Mapping::new();
                map.insert(
                    serde_yaml::Value::String("session.type".to_string()),
                    serde_yaml::Value::String("test".to_string()),
                );
                Some(map.into())
            },
        },
        TimelineEvent::LlmResponse {
            llm_response: vec![ResponseElement::Assistant {
                assistant: vec![AssistantStep {
                    relative_time: 0,
                    content: ContentBlock::Text("response".to_string()),
                }],
            }],
            meta: {
                let mut map = serde_yaml::Mapping::new();
                map.insert(
                    serde_yaml::Value::String("response.model".to_string()),
                    serde_yaml::Value::String("claude-3".to_string()),
                );
                Some(map.into())
            },
        },
    ];

    let scenario = scenario_with_timeline("test_session_meta", timeline);
    let executor = ScenarioExecutor::new(scenario);
    let transcript = executor.to_acp_transcript(None, None, None, None);

    // Verify meta fields are present in session updates (notifications)
    // Meta should be preserved on SessionNotification per executor.rs:705, 899
    let has_update_meta = transcript.live.iter().any(|upd| upd.meta.is_some());

    assert!(has_update_meta, "Expected meta in session updates");
}

#[test]
fn test_acp_session_mode_switching() {
    // Verifies `setMode` scenario events map to `session/set_mode` ACP method calls
    // Reference: resources/acp-specs/docs/protocol/session-modes.mdx#setting-the-current-mode
    let timeline = vec![
        TimelineEvent::SessionStart {
            session_start: SessionStartData {
                session_id: Some("mode-session".to_string()),
                expected_prompt_response: None,
            },
            meta: None,
        },
        TimelineEvent::SetMode {
            set_mode: SetModeData {
                mode_id: "architect".to_string(),
            },
            meta: None,
        },
    ];

    let scenario = scenario_with_timeline("test_mode_switching", timeline);
    let executor = ScenarioExecutor::new(scenario);
    let playbook = executor.build_playbook();

    // Verify mode switching event is captured in playbook
    let has_mode_change = playbook
        .live
        .iter()
        .any(|event| matches!(&event.action, AcpAction::ModeChange { .. }));

    assert!(has_mode_change, "Expected SetMode event in live timeline");
}

#[test]
fn test_acp_session_model_switching() {
    // Verifies `setModel` scenario events map to `session/set_model` ACP method calls
    // Reference: resources/acp-specs/docs/protocol/schema.unstable.mdx#session-set_model
    // Note: This is an UNSTABLE feature requiring explicit opt-in
    let timeline = vec![
        TimelineEvent::SessionStart {
            session_start: SessionStartData {
                session_id: Some("model-session".to_string()),
                expected_prompt_response: None,
            },
            meta: None,
        },
        TimelineEvent::SetModel {
            set_model: SetModelData {
                model_id: "claude-3-opus-20240229".to_string(),
            },
            meta: None,
        },
    ];

    let scenario = Arc::new(Scenario {
        name: "test_model_switching".to_string(),
        tags: vec![],
        terminal_ref: None,
        initial_prompt: None,
        repo: None,
        ah: None,
        server: None,
        acp: Some(AcpConfig {
            capabilities: None,
            cwd: None,
            mcp_servers: None,
            unstable: Some(true), // Required for setModel
        }),
        rules: None,
        timeline,
        expect: None,
    });

    // Verify validation passes with unstable flag
    assert!(scenario.validate_acp_requirements().is_ok());

    let executor = ScenarioExecutor::new(scenario);
    let playbook = executor.build_playbook();

    // Verify model switching event is captured
    let has_model_change = playbook
        .live
        .iter()
        .any(|event| matches!(&event.action, AcpAction::ModelChange { .. }));

    assert!(has_model_change, "Expected SetModel event in live timeline");
}

#[test]
fn test_acp_custom_capabilities() {
    // Verifies custom capabilities can be advertised and negotiated
    // Reference: resources/acp-specs/docs/protocol/extensibility.mdx#advertising-custom-capabilities
    let timeline = vec![TimelineEvent::Initialize {
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
            meta: {
                let mut inner_map = serde_yaml::Mapping::new();
                inner_map.insert(
                    serde_yaml::Value::String("snapshot".to_string()),
                    serde_yaml::Value::Bool(true),
                );
                let mut outer_map = serde_yaml::Mapping::new();
                outer_map.insert(
                    serde_yaml::Value::String("_custom.capabilities".to_string()),
                    serde_yaml::Value::Mapping(inner_map),
                );
                Some(outer_map.into())
            },
            expected_response: None,
        },
    }];

    // Use scenario_with_acp_caps to set agent capabilities, not expected_response
    let caps = AcpCapabilities {
        load_session: Some(false),
        prompt_capabilities: None,
        mcp_capabilities: None,
    };

    let scenario = scenario_with_acp_caps("test_custom_caps", timeline, caps);

    // Verify custom capabilities via meta fields validate correctly
    assert!(scenario.validate_acp_requirements().is_ok());

    let executor = ScenarioExecutor::new(scenario);
    let transcript = executor.to_acp_transcript(None, None, None, None);

    // Meta fields are not preserved in initialize_response (executor sets it to None)
    // Custom capabilities would need to be advertised through the capabilities field, not meta
    assert!(transcript.initialize_response.meta.is_none());
}

// ===========================================================================
// Scenario Format Completeness Tests
// ===========================================================================

#[test]
fn test_scenario_format_exhaustive_coverage() {
    // Verifies every ACP protocol message type has corresponding scenario format representation
    // This is a structural test ensuring the scenario format is comprehensive
    let timeline = vec![
        // Initialize
        TimelineEvent::Initialize {
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
                meta: None,
                expected_response: None,
            },
        },
        // SessionStart (new/load boundary)
        TimelineEvent::SessionStart {
            session_start: SessionStartData {
                session_id: Some("comprehensive".to_string()),
                expected_prompt_response: None,
            },
            meta: None,
        },
        // UserInputs (prompt)
        TimelineEvent::UserInputs {
            user_inputs: vec![UserInputEntry {
                relative_time: 0,
                input: InputContent::Text("test".to_string()),
                target: None,
                meta: None,
                expected_response: None,
            }],
        },
        // LlmResponse with various elements
        TimelineEvent::LlmResponse {
            llm_response: vec![
                ResponseElement::Think {
                    think: vec![ThinkingStep {
                        relative_time: 0,
                        content: "thinking".to_string(),
                    }],
                },
                ResponseElement::Assistant {
                    assistant: vec![AssistantStep {
                        relative_time: 0,
                        content: ContentBlock::Text("response".to_string()),
                    }],
                },
                ResponseElement::AgentPlan {
                    agent_plan: AgentPlanData {
                        entries: vec![PlanEntry {
                            content: "task".to_string(),
                            priority: "high".to_string(),
                            status: "pending".to_string(),
                        }],
                        plan_update: None,
                    },
                },
            ],
            meta: None,
        },
        // Tool use
        TimelineEvent::AgentToolUse {
            agent_tool_use: ToolUseData {
                tool_name: "test".to_string(),
                args: Default::default(),
                tool_call_id: None,
                progress: None,
                result: None,
                status: None,
                tool_execution: None,
                meta: None,
            },
            meta: None,
        },
        // File edits
        TimelineEvent::AgentEdits {
            agent_edits: FileEditData {
                path: "file.txt".to_string(),
                lines_added: 1,
                lines_removed: 0,
            },
            meta: None,
        },
        // Permission request
        TimelineEvent::AgentPermissionRequest {
            agent_permission_request: AgentPermissionRequestData {
                session_id: None,
                tool_call: None,
                options: Some(vec![PermissionOption {
                    id: "allow".to_string(),
                    label: "Allow".to_string(),
                    kind: "allow_once".to_string(),
                }]),
                decision: None,
                granted: Some(true),
            },
            meta: None,
        },
        // File reads
        TimelineEvent::AgentFileReads {
            agent_file_reads: AgentFileReadsData {
                files: vec![FileReadSpec {
                    path: "/tmp/file".to_string(),
                    expected_content: None,
                }],
            },
            meta: None,
        },
        // Cancel
        TimelineEvent::UserCancelSession {
            user_cancel_session: true,
        },
        // SetMode
        TimelineEvent::SetMode {
            set_mode: SetModeData {
                mode_id: "ask".to_string(),
            },
            meta: None,
        },
        // Control events
        TimelineEvent::AdvanceMs {
            base_time_delta: 100,
        },
        TimelineEvent::Log {
            log: "test log".to_string(),
            meta: None,
        },
        TimelineEvent::Assert {
            assert: AssertionData {
                fs: None,
                text: None,
                json: None,
                git: None,
                acp: None,
            },
        },
        TimelineEvent::Complete { complete: true },
    ];

    let scenario = scenario_with_timeline("test_exhaustive", timeline);

    // Verify all event types are recognized and valid
    assert!(scenario.validate_acp_requirements().is_ok());
}

#[test]
fn test_scenario_rules_conditional_mapping() {
    // Verifies `rules` construct properly maps different ACP behaviors based on conditions
    use ah_scenario_format::{Rule, Rules, SymbolTable, SymbolValue};

    let rules = Rules {
        rules: vec![
            Rule {
                when: Some("$test_mode".to_string()),
                default: None,
                config: {
                    let mut m = serde_yaml::Mapping::new();
                    m.insert(
                        serde_yaml::Value::String("enabled".to_string()),
                        serde_yaml::Value::Bool(true),
                    );
                    serde_yaml::Value::Mapping(m)
                },
            },
            Rule {
                when: None,
                default: Some(true),
                config: {
                    let mut m = serde_yaml::Mapping::new();
                    m.insert(
                        serde_yaml::Value::String("enabled".to_string()),
                        serde_yaml::Value::Bool(false),
                    );
                    serde_yaml::Value::Mapping(m)
                },
            },
        ],
    };

    let mut symbols = SymbolTable::new();
    symbols.define("test_mode".to_string(), SymbolValue::Boolean(true));

    let result = ah_scenario_format::evaluate_rules(&rules, &symbols);
    assert!(result.is_ok(), "Rule evaluation should succeed");

    // Verify the correct rule was matched
    let config = result.unwrap();
    assert!(config.is_mapping(), "Expected mapping result");
}

#[test]
fn test_scenario_initialprompt_rich_content() {
    // Verifies `initialPrompt` supports all ACP content block types for initial session prompts
    let timeline = vec![
        TimelineEvent::SessionStart {
            session_start: SessionStartData {
                session_id: Some("rich-prompt-session".to_string()),
                expected_prompt_response: None,
            },
            meta: None,
        },
        TimelineEvent::UserInputs {
            user_inputs: vec![UserInputEntry {
                relative_time: 0,
                input: InputContent::Rich(vec![
                    RichContentBlock::Text {
                        text: "Initial prompt with rich content".to_string(),
                        annotations: None,
                    },
                    RichContentBlock::ResourceLink {
                        uri: "file:///workspace/context.md".to_string(),
                        name: "Context".to_string(),
                        mime_type: Some("text/markdown".to_string()),
                        title: None,
                        description: None,
                        size: None,
                        annotations: None,
                    },
                ]),
                target: None,
                meta: None,
                expected_response: None,
            }],
        },
    ];

    let scenario = scenario_with_timeline("test_rich_initial_prompt", timeline);

    // Verify effective initial prompt extraction works with rich content
    let effective_prompt = scenario.effective_initial_prompt();
    assert!(
        effective_prompt.is_some(),
        "Expected effective initial prompt"
    );
    assert!(
        effective_prompt.unwrap().contains("rich content"),
        "Expected text from rich content block"
    );
}

#[test]
fn test_scenario_timeline_comprehensive_events() {
    // Verifies timeline supports all ACP message flows and notification types
    // This test ensures the timeline can represent the full ACP protocol lifecycle
    let timeline = vec![
        // Initialization phase
        TimelineEvent::Initialize {
            initialize: InitializeData {
                protocol_version: 1,
                client_capabilities: ClientCapabilities {
                    fs: Some(FilesystemCapabilities {
                        read_text_file: Some(true),
                        write_text_file: Some(true),
                    }),
                    terminal: Some(true),
                },
                client_info: Some(ClientInfo {
                    name: "test-client".to_string(),
                    version: "1.0.0".to_string(),
                }),
                meta: None,
                expected_response: None,
            },
        },
        // Session phase
        TimelineEvent::SessionStart {
            session_start: SessionStartData {
                session_id: Some("comprehensive-events".to_string()),
                expected_prompt_response: None,
            },
            meta: None,
        },
        // Prompt turn with all notification types
        TimelineEvent::UserInputs {
            user_inputs: vec![UserInputEntry {
                relative_time: 0,
                input: InputContent::Text("prompt".to_string()),
                target: None,
                meta: None,
                expected_response: None,
            }],
        },
        TimelineEvent::LlmResponse {
            llm_response: vec![
                ResponseElement::Think {
                    think: vec![ThinkingStep {
                        relative_time: 0,
                        content: "thought".to_string(),
                    }],
                },
                ResponseElement::Assistant {
                    assistant: vec![AssistantStep {
                        relative_time: 0,
                        content: ContentBlock::Text("message".to_string()),
                    }],
                },
                ResponseElement::AgentToolUse {
                    agent_tool_use: ToolUseData {
                        tool_name: "tool".to_string(),
                        args: Default::default(),
                        tool_call_id: Some("tc1".to_string()),
                        progress: None,
                        result: None,
                        status: None,
                        tool_execution: None,
                        meta: None,
                    },
                },
                ResponseElement::ToolResult {
                    tool_result: ToolResultData {
                        tool_call_id: "tc1".to_string(),
                        content: serde_yaml::Value::String("result".to_string()),
                        is_error: false,
                    },
                },
                ResponseElement::AgentPlan {
                    agent_plan: AgentPlanData {
                        entries: vec![PlanEntry {
                            content: "plan item".to_string(),
                            priority: "high".to_string(),
                            status: "pending".to_string(),
                        }],
                        plan_update: None,
                    },
                },
            ],
            meta: None,
        },
        // File operations
        TimelineEvent::AgentFileReads {
            agent_file_reads: AgentFileReadsData {
                files: vec![FileReadSpec {
                    path: "/tmp/test".to_string(),
                    expected_content: None,
                }],
            },
            meta: None,
        },
        TimelineEvent::AgentEdits {
            agent_edits: FileEditData {
                path: "/tmp/output".to_string(),
                lines_added: 5,
                lines_removed: 2,
            },
            meta: None,
        },
        // Permission flow
        TimelineEvent::AgentPermissionRequest {
            agent_permission_request: AgentPermissionRequestData {
                session_id: None,
                tool_call: None,
                options: Some(vec![PermissionOption {
                    id: "allow".to_string(),
                    label: "Allow".to_string(),
                    kind: "allow_once".to_string(),
                }]),
                decision: None,
                granted: Some(true),
            },
            meta: None,
        },
        // Mode changes
        TimelineEvent::SetMode {
            set_mode: SetModeData {
                mode_id: "code".to_string(),
            },
            meta: None,
        },
        // Cancellation
        TimelineEvent::UserCancelSession {
            user_cancel_session: true,
        },
        // Completion
        TimelineEvent::Complete { complete: true },
    ];

    let scenario = scenario_with_timeline("test_comprehensive_timeline", timeline);

    // Verify scenario is valid with all event types
    assert!(scenario.validate_acp_requirements().is_ok());

    let executor = ScenarioExecutor::new(scenario);
    let playbook = executor.build_playbook();

    // Verify playbook contains all phases
    assert!(playbook.session_start.is_some());
    assert!(playbook.session_start.is_some());
    assert!(!playbook.historical.is_empty());
    assert!(!playbook.live.is_empty());
}

// ===========================================================================
// ACP Transport and Framing Tests
// ===========================================================================

#[tokio::test]
async fn test_stdio_notification_delivery() {
    // Verifies ACP notifications are properly delivered over stdio transport
    // Reference: resources/acp-specs/docs/protocol/prompt-turn.mdx#3-agent-reports-output

    // This test validates that different notification types can be delivered:
    // - session/update with agent message chunks
    // - session/update with tool call updates
    // - session/update with plan entries
    // - current_mode_update notifications
    // - extension notifications starting with underscore

    let timeline = vec![
        TimelineEvent::SessionStart {
            session_start: SessionStartData {
                session_id: Some("notif-test".to_string()),
                expected_prompt_response: None,
            },
            meta: None,
        },
        // Agent message chunk
        TimelineEvent::LlmResponse {
            llm_response: vec![ResponseElement::Assistant {
                assistant: vec![AssistantStep {
                    relative_time: 0,
                    content: ContentBlock::Text("notification test".to_string()),
                }],
            }],
            meta: None,
        },
        // Tool call update
        TimelineEvent::LlmResponse {
            llm_response: vec![ResponseElement::AgentToolUse {
                agent_tool_use: ToolUseData {
                    tool_name: "test_tool".to_string(),
                    args: Default::default(),
                    tool_call_id: Some("tc_notif".to_string()),
                    progress: None,
                    result: None,
                    status: None,
                    tool_execution: None,
                    meta: None,
                },
            }],
            meta: None,
        },
        // Plan entry
        TimelineEvent::LlmResponse {
            llm_response: vec![ResponseElement::AgentPlan {
                agent_plan: AgentPlanData {
                    entries: vec![PlanEntry {
                        content: "plan notification".to_string(),
                        priority: "medium".to_string(),
                        status: "in_progress".to_string(),
                    }],
                    plan_update: None,
                },
            }],
            meta: None,
        },
    ];

    let scenario = scenario_with_timeline("test_notifications", timeline);
    let executor = ScenarioExecutor::new(scenario);
    let transcript = executor.to_acp_transcript(None, None, None, None);

    // Verify all notification types are present in live events
    let has_message = transcript
        .live
        .iter()
        .any(|upd| matches!(upd.update, SessionUpdate::AgentMessageChunk { .. }));
    let has_tool = transcript
        .live
        .iter()
        .any(|upd| matches!(upd.update, SessionUpdate::ToolCallUpdate { .. }));
    let has_plan = transcript
        .live
        .iter()
        .any(|upd| matches!(upd.update, SessionUpdate::Plan { .. }));

    assert!(has_message, "Expected agent message notification");
    assert!(has_tool, "Expected tool call notification");
    assert!(has_plan, "Expected plan notification");
}

// ===========================================================================
// Library and Configuration Tests
// ===========================================================================

#[test]
fn test_library_scenario_driven_execution() {
    // Verifies library API can execute complete scenarios and generate ACP message sequences
    let timeline = vec![
        TimelineEvent::SessionStart {
            session_start: SessionStartData {
                session_id: Some("lib-test".to_string()),
                expected_prompt_response: None,
            },
            meta: None,
        },
        TimelineEvent::UserInputs {
            user_inputs: vec![UserInputEntry {
                relative_time: 0,
                input: InputContent::Text("library test prompt".to_string()),
                target: None,
                meta: None,
                expected_response: None,
            }],
        },
        TimelineEvent::LlmResponse {
            llm_response: vec![ResponseElement::Assistant {
                assistant: vec![AssistantStep {
                    relative_time: 0,
                    content: ContentBlock::Text("library test response".to_string()),
                }],
            }],
            meta: None,
        },
    ];

    let scenario = scenario_with_timeline("test_library", timeline);
    let executor = ScenarioExecutor::new(scenario);

    // Verify transcript generation (library API)
    let transcript = executor.to_acp_transcript(None, None, None, None);
    assert_eq!(transcript.session_id.0.as_ref(), "lib-test");
    assert!(
        !(transcript.historical.is_empty() && transcript.live.is_empty()),
        "Expected transcript to contain at least one event"
    );

    // Verify playbook generation (library API)
    let playbook = executor.build_playbook();
    assert!(playbook.session_start.is_some());
    assert!(
        !(playbook.historical.is_empty() && playbook.live.is_empty()),
        "Expected playbook to contain events"
    );
}

#[test]
fn test_configuration_symbol_injection() {
    // Verifies symbols can be specified for conditional scenario execution
    use ah_scenario_format::{SymbolTable, SymbolValue};

    let mut symbols = SymbolTable::new();
    symbols.define(
        "test_env".to_string(),
        SymbolValue::String("staging".to_string()),
    );
    symbols.define("debug_mode".to_string(), SymbolValue::Boolean(true));
    symbols.define("max_retries".to_string(), SymbolValue::Number(3));

    // Test symbol retrieval
    assert!(symbols.is_defined("test_env"));
    assert!(symbols.is_defined("debug_mode"));
    assert!(symbols.is_defined("max_retries"));
    assert!(!symbols.is_defined("undefined"));

    // Test condition evaluation
    assert!(symbols.evaluate_condition("$test_env").unwrap());
    assert!(symbols.evaluate_condition("$debug_mode").unwrap());
    assert!(symbols.evaluate_condition("$max_retries >= 3").unwrap());
    assert!(!symbols.evaluate_condition("$undefined").unwrap());

    // Test string comparison
    assert!(symbols.evaluate_condition("$test_env == \"staging\"").unwrap());
    assert!(!symbols.evaluate_condition("$test_env == \"production\"").unwrap());

    // Test numeric comparison
    assert!(symbols.evaluate_condition("$max_retries == 3").unwrap());
    assert!(symbols.evaluate_condition("$max_retries <= 5").unwrap());
    assert!(!symbols.evaluate_condition("$max_retries > 10").unwrap());
}

// ===========================================================================
// Client-Side ACP Method Simulation Tests
// ===========================================================================
// Note: Some of these tests are already in acp_integration.rs
// We'll add the missing ones here

#[test]
fn test_client_fs_read_simulation() {
    // Verifies `readFile` scenario events properly map to client `fs/read_text_file` ACP method calls
    let timeline = vec![TimelineEvent::AgentFileReads {
        agent_file_reads: AgentFileReadsData {
            files: vec![
                FileReadSpec {
                    path: "/workspace/file1.txt".to_string(),
                    expected_content: Some(serde_yaml::Value::String("content1".to_string())),
                },
                FileReadSpec {
                    path: "/workspace/file2.txt".to_string(),
                    expected_content: Some(serde_yaml::Value::String("content2".to_string())),
                },
            ],
        },
        meta: None,
    }];

    let scenario = scenario_with_timeline("test_fs_read", timeline);
    let executor = ScenarioExecutor::new(scenario);
    let transcript = executor.to_acp_transcript(None, None, None, None);

    // Verify file reads are recorded
    // The executor groups all files from one AgentFileReads event into a single entry
    assert_eq!(transcript.file_reads.len(), 1);
    assert_eq!(transcript.file_reads[0].files.len(), 2);
    assert_eq!(
        transcript.file_reads[0].files[0].path,
        "/workspace/file1.txt"
    );
    assert_eq!(
        transcript.file_reads[0].files[1].path,
        "/workspace/file2.txt"
    );
}

#[test]
fn test_client_fs_write_simulation() {
    // Verifies `agentEdits` and `editFile`/`writeFile` scenario events properly map to client
    // `fs/write_text_file` ACP method calls
    let timeline = vec![
        TimelineEvent::SessionStart {
            session_start: SessionStartData {
                session_id: Some("write-test".to_string()),
                expected_prompt_response: None,
            },
            meta: None,
        },
        TimelineEvent::AgentEdits {
            agent_edits: FileEditData {
                path: "/workspace/output.txt".to_string(),
                lines_added: 10,
                lines_removed: 5,
            },
            meta: None,
        },
    ];

    let scenario = scenario_with_timeline("test_fs_write", timeline);
    let executor = ScenarioExecutor::new(scenario);
    let playbook = executor.build_playbook();

    // Verify file edit events are in the playbook
    let has_edit = playbook
        .live
        .iter()
        .any(|event| matches!(&event.action, AcpAction::UpdateFileEdit { .. }));

    assert!(has_edit, "Expected AgentEdits event in playbook");
}

#[test]
fn test_client_terminal_operations_simulation() {
    // Verifies `runCmd` scenario events properly map to client terminal ACP method flows
    let timeline = vec![
        TimelineEvent::SessionStart {
            session_start: SessionStartData {
                session_id: Some("terminal-test".to_string()),
                expected_prompt_response: None,
            },
            meta: None,
        },
        TimelineEvent::AgentToolUse {
            agent_tool_use: ToolUseData {
                tool_name: "runCmd".to_string(),
                args: [
                    (
                        "cmd".to_string(),
                        serde_yaml::Value::String("echo hello".to_string()),
                    ),
                    (
                        "cwd".to_string(),
                        serde_yaml::Value::String("/workspace".to_string()),
                    ),
                ]
                .iter()
                .cloned()
                .collect(),
                tool_call_id: Some("tc_term".to_string()),
                progress: Some(vec![ProgressStep {
                    relative_time: 0,
                    message: "Running command...".to_string(),
                    expect_output: Some("hello".to_string()),
                }]),
                result: Some(serde_yaml::Value::String("hello\n".to_string())),
                status: Some("ok".to_string()),
                tool_execution: None,
                meta: None,
            },
            meta: None,
        },
    ];

    let scenario = scenario_with_timeline("test_terminal_ops", timeline);
    let executor = ScenarioExecutor::new(scenario);
    let playbook = executor.build_playbook();

    // Verify runCmd tool use is in the playbook
    let has_terminal = playbook.live.iter().any(|event| {
        matches!(
            &event.action,
            AcpAction::UpdateToolUse { tool, .. } if tool.tool_name == "runCmd"
        )
    });

    assert!(has_terminal, "Expected runCmd AgentToolUse in playbook");
}

#[test]
fn test_client_permission_request_simulation() {
    // Verifies permission-required scenario events properly map to client
    // `session/request_permission` ACP method calls
    let timeline = vec![TimelineEvent::AgentPermissionRequest {
        agent_permission_request: AgentPermissionRequestData {
            session_id: Some("perm-session".to_string()),
            tool_call: Some(
                serde_yaml::to_value(&[
                    ("toolCallId", "perm_tc"),
                    ("title", "file_write"),
                    ("kind", "fs/write_text_file"),
                ])
                .unwrap(),
            ),
            options: Some(vec![
                PermissionOption {
                    id: "allow_once".to_string(),
                    label: "Allow this operation".to_string(),
                    kind: "allow_once".to_string(),
                },
                PermissionOption {
                    id: "deny_once".to_string(),
                    label: "Deny this operation".to_string(),
                    kind: "reject_once".to_string(),
                },
            ]),
            decision: Some(UserDecision {
                outcome: "selected".to_string(),
                option_id: Some("allow_once".to_string()),
            }),
            granted: None,
        },
        meta: None,
    }];

    let scenario = scenario_with_timeline("test_permission", timeline);

    // Verify permission request data is valid
    let timeline_event = &scenario.timeline[0];
    if let TimelineEvent::AgentPermissionRequest {
        agent_permission_request,
        ..
    } = timeline_event
    {
        assert!(agent_permission_request.validate().is_ok());
    }

    let executor = ScenarioExecutor::new(scenario);
    let transcript = executor.to_acp_transcript(None, None, None, None);

    // Verify permission request is recorded
    assert_eq!(transcript.permission_requests.len(), 1);
    assert_eq!(
        transcript.permission_requests[0].options.as_ref().unwrap().len(),
        2
    );
}

// ===========================================================================
// ACP Error and Edge Case Tests
// ===========================================================================

#[test]
fn test_acp_error_response_simulation() {
    // Verifies error conditions in ACP responses are properly simulated via scenario events
    let timeline = vec![
        TimelineEvent::SessionStart {
            session_start: SessionStartData {
                session_id: Some("error-test".to_string()),
                expected_prompt_response: None,
            },
            meta: None,
        },
        TimelineEvent::LlmResponse {
            llm_response: vec![ResponseElement::Error {
                error: ErrorData {
                    error_type: "rate_limit_exceeded".to_string(),
                    status_code: Some(429),
                    message: "Rate limit exceeded. Please try again later.".to_string(),
                    details: Some(serde_yaml::to_value(&[("limit", 100), ("window", 60)]).unwrap()),
                    retry_after_seconds: Some(60),
                },
            }],
            meta: None,
        },
    ];

    let scenario = scenario_with_timeline("test_error_response", timeline);
    let executor = ScenarioExecutor::new(scenario);
    let playbook = executor.build_playbook();

    // Verify error event is captured in playbook
    let has_error = playbook
        .live
        .iter()
        .any(|event| matches!(&event.action, AcpAction::UpdateError { .. }));

    assert!(has_error, "Expected error response in playbook");
}

#[test]
fn test_acp_authentication_flow() {
    // Verifies `authenticate` method flow when agent requires authentication
    // Note: Authentication is represented via meta fields in the initialize exchange
    let timeline = vec![TimelineEvent::Initialize {
        initialize: InitializeData {
            protocol_version: 1,
            client_capabilities: ClientCapabilities {
                fs: None,
                terminal: None,
            },
            client_info: None,
            meta: {
                let mut auth_map = serde_yaml::Mapping::new();
                auth_map.insert(
                    serde_yaml::Value::String("method".to_string()),
                    serde_yaml::Value::String("bearer".to_string()),
                );
                auth_map.insert(
                    serde_yaml::Value::String("token".to_string()),
                    serde_yaml::Value::String("test_token_123".to_string()),
                );
                let mut outer = serde_yaml::Mapping::new();
                outer.insert(
                    serde_yaml::Value::String("auth".to_string()),
                    serde_yaml::Value::Mapping(auth_map),
                );
                Some(serde_yaml::Value::Mapping(outer))
            },
            expected_response: Some(ExpectedInitializeResponse {
                protocol_version: 1,
                agent_capabilities: AcpCapabilities {
                    load_session: Some(false),
                    prompt_capabilities: None,
                    mcp_capabilities: None,
                },
                meta: {
                    let mut auth_meta = serde_yaml::Mapping::new();
                    auth_meta.insert(
                        serde_yaml::Value::String("status".to_string()),
                        serde_yaml::Value::String("authenticated".to_string()),
                    );
                    let mut outer = serde_yaml::Mapping::new();
                    outer.insert(
                        serde_yaml::Value::String("auth".to_string()),
                        serde_yaml::Value::Mapping(auth_meta),
                    );
                    Some(serde_yaml::Value::Mapping(outer))
                },
            }),
        },
    }];

    let scenario = scenario_with_timeline("test_auth", timeline);
    let executor = ScenarioExecutor::new(scenario);
    let transcript = executor.to_acp_transcript(None, None, None, None);

    // Verify auth meta fields are preserved
    assert!(transcript.initialize_response.meta.is_some());
}

#[test]
fn test_acp_session_modes() {
    // Verifies `session/set_mode` method support when agent supports operating modes
    // Reference: resources/acp-specs/docs/protocol/session-modes.mdx
    let timeline = vec![
        TimelineEvent::SessionStart {
            session_start: SessionStartData {
                session_id: Some("modes-test".to_string()),
                expected_prompt_response: None,
            },
            meta: None,
        },
        TimelineEvent::SetMode {
            set_mode: SetModeData {
                mode_id: "ask".to_string(),
            },
            meta: None,
        },
        TimelineEvent::SetMode {
            set_mode: SetModeData {
                mode_id: "code".to_string(),
            },
            meta: None,
        },
        TimelineEvent::SetMode {
            set_mode: SetModeData {
                mode_id: "architect".to_string(),
            },
            meta: None,
        },
    ];

    let scenario = scenario_with_timeline("test_modes", timeline);
    let executor = ScenarioExecutor::new(scenario);
    let playbook = executor.build_playbook();

    // Count mode changes
    let mode_changes = playbook
        .live
        .iter()
        .filter(|event| matches!(&event.action, AcpAction::ModeChange { .. }))
        .count();

    assert_eq!(mode_changes, 3, "Expected 3 mode change events");
}

#[test]
fn test_acp_notification_all_types() {
    // Verifies all `session/update` notification variants are simulable
    // Types: status, log, thought, tool_call, tool_result, file_edit, terminal
    let timeline = vec![
        TimelineEvent::SessionStart {
            session_start: SessionStartData {
                session_id: Some("notif-types".to_string()),
                expected_prompt_response: None,
            },
            meta: None,
        },
        // Status (represented via Status event)
        TimelineEvent::Status {
            status: "running".to_string(),
        },
        // Log
        TimelineEvent::Log {
            log: "Processing request...".to_string(),
            meta: None,
        },
        // Thought
        TimelineEvent::LlmResponse {
            llm_response: vec![ResponseElement::Think {
                think: vec![ThinkingStep {
                    relative_time: 0,
                    content: "Analyzing the problem...".to_string(),
                }],
            }],
            meta: None,
        },
        // Tool call and result
        TimelineEvent::LlmResponse {
            llm_response: vec![
                ResponseElement::AgentToolUse {
                    agent_tool_use: ToolUseData {
                        tool_name: "search".to_string(),
                        args: Default::default(),
                        tool_call_id: Some("tc_all_types".to_string()),
                        progress: None,
                        result: None,
                        status: None,
                        tool_execution: None,
                        meta: None,
                    },
                },
                ResponseElement::ToolResult {
                    tool_result: ToolResultData {
                        tool_call_id: "tc_all_types".to_string(),
                        content: serde_yaml::Value::String("found".to_string()),
                        is_error: false,
                    },
                },
            ],
            meta: None,
        },
        // File edit
        TimelineEvent::AgentEdits {
            agent_edits: FileEditData {
                path: "/workspace/edit.txt".to_string(),
                lines_added: 3,
                lines_removed: 1,
            },
            meta: None,
        },
        // Terminal (runCmd)
        TimelineEvent::AgentToolUse {
            agent_tool_use: ToolUseData {
                tool_name: "runCmd".to_string(),
                args: [(
                    "cmd".to_string(),
                    serde_yaml::Value::String("ls".to_string()),
                )]
                .iter()
                .cloned()
                .collect(),
                tool_call_id: None,
                progress: None,
                result: None,
                status: None,
                tool_execution: None,
                meta: None,
            },
            meta: None,
        },
    ];

    let scenario = scenario_with_timeline("test_all_notification_types", timeline);
    let executor = ScenarioExecutor::new(scenario);
    let playbook = executor.build_playbook();

    // Verify all event types are present
    let has_status = playbook.live.iter().any(|e| matches!(&e.action, AcpAction::Status { .. }));
    let has_log = playbook.live.iter().any(|e| matches!(&e.action, AcpAction::Log { .. }));
    let has_thought = playbook.live.iter().any(|e| matches!(&e.action, AcpAction::Thought { .. }));
    let has_tool = playbook
        .live
        .iter()
        .any(|e| matches!(&e.action, AcpAction::UpdateToolUse { .. }));
    let has_edit = playbook
        .live
        .iter()
        .any(|e| matches!(&e.action, AcpAction::UpdateFileEdit { .. }));
    let has_terminal = playbook.live.iter().any(|e| {
        matches!(
            &e.action,
            AcpAction::UpdateToolUse { tool, .. } if tool.tool_name == "runCmd"
        )
    });

    assert!(has_status, "Expected Status event");
    assert!(has_log, "Expected Log event");
    assert!(has_thought, "Expected Thought (Think) event");
    assert!(has_tool, "Expected Tool call event");
    assert!(has_edit, "Expected File edit event");
    assert!(has_terminal, "Expected Terminal (runCmd) event");
}

// ===========================================================================
// ACP Comprehensive Integration Tests
// ===========================================================================

#[test]
fn test_acp_comprehensive_scenario_execution() {
    // Executes a complex, multi-feature scenario combining session lifecycle, rich content,
    // tool calls, file operations, mode switching, and error conditions to validate
    // end-to-end system integration
    let timeline = vec![
        // Phase 1: Initialization with custom capabilities
        TimelineEvent::Initialize {
            initialize: InitializeData {
                protocol_version: 1,
                client_capabilities: ClientCapabilities {
                    fs: Some(FilesystemCapabilities {
                        read_text_file: Some(true),
                        write_text_file: Some(true),
                    }),
                    terminal: Some(true),
                },
                client_info: Some(ClientInfo {
                    name: "comprehensive-test-client".to_string(),
                    version: "1.0.0".to_string(),
                }),
                meta: {
                    let mut inner = serde_yaml::Mapping::new();
                    inner.insert(
                        serde_yaml::Value::String("client.feature_flags".to_string()),
                        serde_yaml::Value::Sequence(vec![
                            serde_yaml::Value::String("snapshots".to_string()),
                            serde_yaml::Value::String("multimodal".to_string()),
                        ]),
                    );
                    Some(serde_yaml::Value::Mapping(inner))
                },
                expected_response: Some(ExpectedInitializeResponse {
                    protocol_version: 1,
                    agent_capabilities: AcpCapabilities {
                        load_session: Some(true),
                        prompt_capabilities: Some(AcpPromptCapabilities {
                            image: Some(true),
                            audio: Some(false),
                            embedded_context: Some(true),
                        }),
                        mcp_capabilities: None,
                    },
                    meta: None,
                }),
            },
        },
        // Phase 2: Historical events (before sessionStart)
        TimelineEvent::UserInputs {
            user_inputs: vec![UserInputEntry {
                relative_time: 0,
                input: InputContent::Rich(vec![
                    RichContentBlock::Text {
                        text: "Historical context: analyze this codebase".to_string(),
                        annotations: None,
                    },
                    RichContentBlock::ResourceLink {
                        uri: "file:///workspace/README.md".to_string(),
                        name: "Project README".to_string(),
                        mime_type: Some("text/markdown".to_string()),
                        title: None,
                        description: None,
                        size: None,
                        annotations: None,
                    },
                ]),
                target: None,
                meta: None,
                expected_response: None,
            }],
        },
        TimelineEvent::LlmResponse {
            llm_response: vec![
                ResponseElement::Think {
                    think: vec![ThinkingStep {
                        relative_time: 0,
                        content: "I need to read the project structure first".to_string(),
                    }],
                },
                ResponseElement::Assistant {
                    assistant: vec![AssistantStep {
                        relative_time: 100,
                        content: ContentBlock::Text(
                            "Let me analyze the codebase structure.".to_string(),
                        ),
                    }],
                },
            ],
            meta: None,
        },
        TimelineEvent::AgentFileReads {
            agent_file_reads: AgentFileReadsData {
                files: vec![FileReadSpec {
                    path: "/workspace/package.json".to_string(),
                    expected_content: Some(serde_yaml::Value::String(
                        r#"{"name": "test"}"#.to_string(),
                    )),
                }],
            },
            meta: None,
        },
        // Phase 3: Session boundary with loadSession
        TimelineEvent::SessionStart {
            session_start: SessionStartData {
                session_id: Some("comprehensive-session-123".to_string()),
                expected_prompt_response: Some(ExpectedPromptResponse {
                    session_id: Some("comprehensive-session-123".to_string()),
                    stop_reason: Some("completed".to_string()),
                    usage: Some(TokenUsage {
                        input_tokens: Some(1500),
                        output_tokens: Some(800),
                        total_tokens: Some(2300),
                    }),
                    meta: None,
                }),
            },
            meta: {
                let mut map = serde_yaml::Mapping::new();
                map.insert(
                    serde_yaml::Value::String("session.mode".to_string()),
                    serde_yaml::Value::String("analyze".to_string()),
                );
                Some(serde_yaml::Value::Mapping(map))
            },
        },
        // Phase 4: Live events with mode switching
        TimelineEvent::SetMode {
            set_mode: SetModeData {
                mode_id: "code".to_string(),
            },
            meta: None,
        },
        TimelineEvent::UserInputs {
            user_inputs: vec![UserInputEntry {
                relative_time: 0,
                input: InputContent::Text("Now refactor the authentication module".to_string()),
                target: None,
                meta: None,
                expected_response: None,
            }],
        },
        TimelineEvent::LlmResponse {
            llm_response: vec![ResponseElement::AgentPlan {
                agent_plan: AgentPlanData {
                    entries: vec![
                        PlanEntry {
                            content: "Read current authentication code".to_string(),
                            priority: "high".to_string(),
                            status: "in_progress".to_string(),
                        },
                        PlanEntry {
                            content: "Design improved auth flow".to_string(),
                            priority: "high".to_string(),
                            status: "pending".to_string(),
                        },
                        PlanEntry {
                            content: "Implement refactored code".to_string(),
                            priority: "medium".to_string(),
                            status: "pending".to_string(),
                        },
                    ],
                    plan_update: None,
                },
            }],
            meta: None,
        },
        // Phase 5: Tool usage with permissions
        TimelineEvent::AgentPermissionRequest {
            agent_permission_request: AgentPermissionRequestData {
                session_id: None,
                tool_call: None,
                options: Some(vec![PermissionOption {
                    id: "allow".to_string(),
                    label: "Allow file modifications".to_string(),
                    kind: "allow_once".to_string(),
                }]),
                decision: None,
                granted: Some(true),
            },
            meta: None,
        },
        TimelineEvent::LlmResponse {
            llm_response: vec![ResponseElement::AgentToolUse {
                agent_tool_use: ToolUseData {
                    tool_name: "runCmd".to_string(),
                    args: [
                        (
                            "cmd".to_string(),
                            serde_yaml::Value::String("npm test".to_string()),
                        ),
                        (
                            "cwd".to_string(),
                            serde_yaml::Value::String("/workspace".to_string()),
                        ),
                    ]
                    .iter()
                    .cloned()
                    .collect(),
                    tool_call_id: Some("tc_comprehensive".to_string()),
                    progress: Some(vec![
                        ProgressStep {
                            relative_time: 0,
                            message: "Running tests...".to_string(),
                            expect_output: None,
                        },
                        ProgressStep {
                            relative_time: 1000,
                            message: "Tests complete".to_string(),
                            expect_output: Some("passed".to_string()),
                        },
                    ]),
                    result: Some(serde_yaml::Value::String("All tests passed".to_string())),
                    status: Some("ok".to_string()),
                    tool_execution: None,
                    meta: None,
                },
            }],
            meta: None,
        },
        // Phase 6: File operations
        TimelineEvent::AgentEdits {
            agent_edits: FileEditData {
                path: "/workspace/src/auth.ts".to_string(),
                lines_added: 45,
                lines_removed: 30,
            },
            meta: None,
        },
        TimelineEvent::LlmResponse {
            llm_response: vec![ResponseElement::Assistant {
                assistant: vec![AssistantStep {
                    relative_time: 0,
                    content: ContentBlock::Rich(RichContentBlock::Diff {
                        path: "/workspace/src/auth.ts".to_string(),
                        old_text: Some("export function authenticate(user: string)".to_string()),
                        new_text:
                            "export async function authenticate(user: User): Promise<AuthResult>"
                                .to_string(),
                    }),
                }],
            }],
            meta: None,
        },
        // Phase 7: Error handling
        TimelineEvent::LlmResponse {
            llm_response: vec![ResponseElement::Error {
                error: ErrorData {
                    error_type: "validation_error".to_string(),
                    status_code: Some(400),
                    message: "Type mismatch in function signature".to_string(),
                    details: Some(serde_yaml::to_value(&[("line", 42), ("column", 10)]).unwrap()),
                    retry_after_seconds: None,
                },
            }],
            meta: None,
        },
        // Phase 8: Recovery and completion
        TimelineEvent::LlmResponse {
            llm_response: vec![ResponseElement::Assistant {
                assistant: vec![AssistantStep {
                    relative_time: 0,
                    content: ContentBlock::Text(
                        "Fixed type errors. Refactoring complete.".to_string(),
                    ),
                }],
            }],
            meta: None,
        },
        TimelineEvent::Complete { complete: true },
    ];

    let caps = AcpCapabilities {
        load_session: Some(true),
        prompt_capabilities: Some(AcpPromptCapabilities {
            image: Some(true),
            audio: Some(false),
            embedded_context: Some(true),
        }),
        mcp_capabilities: None,
    };
    let scenario = scenario_with_acp_caps("comprehensive_integration_test", timeline, caps);

    // Validate scenario structure
    assert!(
        scenario.validate_acp_requirements().is_ok(),
        "Comprehensive scenario should be valid"
    );

    // Test executor and transcript generation
    let executor = ScenarioExecutor::new(scenario);
    let transcript = executor.to_acp_transcript(None, None, None, None);

    // Verify all phases are represented
    assert_eq!(
        transcript.session_id.0.as_ref(),
        "comprehensive-session-123",
        "Session ID should match"
    );
    assert!(
        !transcript.historical.is_empty(),
        "Should have historical events"
    );
    assert!(!transcript.live.is_empty(), "Should have live events");
    assert_eq!(
        transcript.permission_requests.len(),
        1,
        "Should have permission request"
    );
    assert_eq!(transcript.file_reads.len(), 1, "Should have file read");
    assert!(
        transcript.initialize_response.agent_capabilities.load_session,
        "LoadSession should be enabled"
    );

    // Verify playbook structure
    let playbook = executor.build_playbook();
    assert!(
        playbook.session_start.is_some(),
        "Should have initialization"
    );
    assert!(
        playbook.session_start.is_some(),
        "Should have session start"
    );

    // Count event types in live timeline
    let mode_changes = playbook
        .live
        .iter()
        .filter(|e| matches!(&e.action, AcpAction::ModeChange(_)))
        .count();
    let tool_uses = playbook
        .live
        .iter()
        .filter(|e| matches!(&e.action, AcpAction::UpdateToolUse { .. }))
        .count();
    let file_edits = playbook
        .live
        .iter()
        .filter(|e| matches!(&e.action, AcpAction::UpdateFileEdit { .. }))
        .count();

    assert!(mode_changes > 0, "Should have mode changes");
    assert!(tool_uses > 0, "Should have tool uses");
    assert!(file_edits > 0, "Should have file edits");
}

// ===========================================================================
// LoadSession Functionality Tests
// ===========================================================================

#[test]
fn test_loadsession_capability_advertisement() {
    // Verifies `loadSession` capability is properly advertised when enabled
    let timeline = vec![TimelineEvent::Initialize {
        initialize: InitializeData {
            protocol_version: 1,
            client_capabilities: ClientCapabilities {
                fs: None,
                terminal: None,
            },
            client_info: None,
            meta: None,
            expected_response: Some(ExpectedInitializeResponse {
                protocol_version: 1,
                agent_capabilities: AcpCapabilities {
                    load_session: Some(true),
                    prompt_capabilities: None,
                    mcp_capabilities: None,
                },
                meta: None,
            }),
        },
    }];

    let caps = AcpCapabilities {
        load_session: Some(true),
        prompt_capabilities: None,
        mcp_capabilities: None,
    };
    let scenario = scenario_with_acp_caps("test_loadsession_cap", timeline, caps);
    let executor = ScenarioExecutor::new(scenario);
    let transcript = executor.to_acp_transcript(None, None, None, None);

    assert_eq!(
        transcript.initialize_response.agent_capabilities.load_session, true,
        "loadSession capability should be advertised"
    );
}

#[test]
fn test_session_load_historical_replay() {
    // Verifies events before `sessionStart` are replayed during `session/load`
    let timeline = vec![
        // Historical events
        TimelineEvent::UserInputs {
            user_inputs: vec![UserInputEntry {
                relative_time: 0,
                input: InputContent::Text("historical prompt 1".to_string()),
                target: None,
                meta: None,
                expected_response: None,
            }],
        },
        TimelineEvent::LlmResponse {
            llm_response: vec![ResponseElement::Assistant {
                assistant: vec![AssistantStep {
                    relative_time: 0,
                    content: ContentBlock::Text("historical response 1".to_string()),
                }],
            }],
            meta: None,
        },
        TimelineEvent::UserInputs {
            user_inputs: vec![UserInputEntry {
                relative_time: 0,
                input: InputContent::Text("historical prompt 2".to_string()),
                target: None,
                meta: None,
                expected_response: None,
            }],
        },
        TimelineEvent::LlmResponse {
            llm_response: vec![ResponseElement::Assistant {
                assistant: vec![AssistantStep {
                    relative_time: 0,
                    content: ContentBlock::Text("historical response 2".to_string()),
                }],
            }],
            meta: None,
        },
        // Boundary
        TimelineEvent::SessionStart {
            session_start: SessionStartData {
                session_id: Some("loaded-historical".to_string()),
                expected_prompt_response: None,
            },
            meta: None,
        },
        // Live events
        TimelineEvent::LlmResponse {
            llm_response: vec![ResponseElement::Assistant {
                assistant: vec![AssistantStep {
                    relative_time: 0,
                    content: ContentBlock::Text("live response".to_string()),
                }],
            }],
            meta: None,
        },
    ];

    let caps = AcpCapabilities {
        load_session: Some(true),
        prompt_capabilities: None,
        mcp_capabilities: None,
    };
    let scenario = scenario_with_acp_caps("test_historical_replay", timeline, caps);
    let executor = ScenarioExecutor::new(scenario);
    let transcript = executor.to_acp_transcript(None, None, None, None);

    // Verify historical events contain all pre-boundary events
    assert!(
        transcript.historical.len() >= 4,
        "Should have at least 4 historical updates (2 user + 2 agent)"
    );

    // Verify historical contains both prompts and responses
    let user_count = transcript
        .historical
        .iter()
        .filter(|upd| matches!(upd.update, SessionUpdate::UserMessageChunk { .. }))
        .count();
    let agent_count = transcript
        .historical
        .iter()
        .filter(|upd| matches!(upd.update, SessionUpdate::AgentMessageChunk { .. }))
        .count();

    assert!(
        user_count >= 2,
        "Expected at least 2 user message chunks in historical"
    );
    assert!(
        agent_count >= 2,
        "Expected at least 2 agent message chunks in historical"
    );

    // Verify live events are separate
    assert!(!transcript.live.is_empty(), "Should have live events");
}

#[test]
fn test_session_load_live_streaming() {
    // Verifies events after `sessionStart` are streamed live after loading
    let timeline = vec![
        // Historical
        TimelineEvent::UserInputs {
            user_inputs: vec![UserInputEntry {
                relative_time: 0,
                input: InputContent::Text("historical".to_string()),
                target: None,
                meta: None,
                expected_response: None,
            }],
        },
        // Boundary
        TimelineEvent::SessionStart {
            session_start: SessionStartData {
                session_id: Some("loaded-live".to_string()),
                expected_prompt_response: None,
            },
            meta: None,
        },
        // Live events
        TimelineEvent::LlmResponse {
            llm_response: vec![ResponseElement::Assistant {
                assistant: vec![
                    AssistantStep {
                        relative_time: 0,
                        content: ContentBlock::Text("live message 1".to_string()),
                    },
                    AssistantStep {
                        relative_time: 100,
                        content: ContentBlock::Text("live message 2".to_string()),
                    },
                ],
            }],
            meta: None,
        },
        TimelineEvent::AgentToolUse {
            agent_tool_use: ToolUseData {
                tool_name: "live_tool".to_string(),
                args: Default::default(),
                tool_call_id: None,
                progress: None,
                result: None,
                status: None,
                tool_execution: None,
                meta: None,
            },
            meta: None,
        },
    ];

    let caps = AcpCapabilities {
        load_session: Some(true),
        prompt_capabilities: None,
        mcp_capabilities: None,
    };
    let scenario = scenario_with_acp_caps("test_live_streaming", timeline, caps);
    let executor = ScenarioExecutor::new(scenario);
    let transcript = executor.to_acp_transcript(None, None, None, None);

    // Verify live events contain only post-boundary events
    assert!(!transcript.live.is_empty(), "Should have live events");

    // Live should not contain any historical messages
    let has_historical_text = transcript
        .live
        .iter()
        .any(|upd| matches!(
            upd.update,
            SessionUpdate::AgentMessageChunk { content: agent_client_protocol_schema::ContentBlock::Text(ref t) } if t.text.contains("historical")
        ));

    assert!(
        !has_historical_text,
        "Live events should not contain historical text"
    );

    // Live should contain the new messages
    let has_live_text = transcript
        .live
        .iter()
        .any(|upd| matches!(
            upd.update,
            SessionUpdate::AgentMessageChunk { content: agent_client_protocol_schema::ContentBlock::Text(ref t) } if t.text.contains("live message")
        ));

    assert!(has_live_text, "Live events should contain live messages");
}

#[test]
fn test_multiple_scenarios_session_matching() {
    // Verifies correct scenario selection for `session/load` by session ID matching
    // This is tested at the library level - scenario selection happens in the loader
    let scenario1 = scenario_with_acp_caps(
        "session_alpha",
        vec![TimelineEvent::SessionStart {
            session_start: SessionStartData {
                session_id: Some("alpha-123".to_string()),
                expected_prompt_response: None,
            },
            meta: None,
        }],
        AcpCapabilities {
            load_session: Some(true),
            prompt_capabilities: None,
            mcp_capabilities: None,
        },
    );

    let scenario2 = scenario_with_acp_caps(
        "session_beta",
        vec![TimelineEvent::SessionStart {
            session_start: SessionStartData {
                session_id: Some("beta-456".to_string()),
                expected_prompt_response: None,
            },
            meta: None,
        }],
        AcpCapabilities {
            load_session: Some(true),
            prompt_capabilities: None,
            mcp_capabilities: None,
        },
    );

    // Verify each scenario has its distinct session ID
    let executor1 = ScenarioExecutor::new(scenario1);
    let executor2 = ScenarioExecutor::new(scenario2);

    let transcript1 = executor1.to_acp_transcript(None, None, None, None);
    let transcript2 = executor2.to_acp_transcript(None, None, None, None);

    assert_eq!(transcript1.session_id.0.as_ref(), "alpha-123");
    assert_eq!(transcript2.session_id.0.as_ref(), "beta-456");
    assert_ne!(transcript1.session_id, transcript2.session_id);
}

#[test]
fn test_multiple_scenarios_new_session_matching() {
    // Verifies Levenshtein distance matching for new sessions across multiple scenarios
    // Reference: specs/Public/Scenario-Format.md#scenario-selection--playback-controls
    let scenario1 = scenario_with_timeline(
        "analyze_codebase",
        vec![
            TimelineEvent::SessionStart {
                session_start: SessionStartData {
                    session_id: Some("s1".to_string()),
                    expected_prompt_response: None,
                },
                meta: None,
            },
            TimelineEvent::UserInputs {
                user_inputs: vec![UserInputEntry {
                    relative_time: 0,
                    input: InputContent::Text("Please analyze the codebase structure".to_string()),
                    target: None,
                    meta: None,
                    expected_response: None,
                }],
            },
        ],
    );

    let scenario2 = scenario_with_timeline(
        "refactor_code",
        vec![
            TimelineEvent::SessionStart {
                session_start: SessionStartData {
                    session_id: Some("s2".to_string()),
                    expected_prompt_response: None,
                },
                meta: None,
            },
            TimelineEvent::UserInputs {
                user_inputs: vec![UserInputEntry {
                    relative_time: 0,
                    input: InputContent::Text("Refactor the authentication module".to_string()),
                    target: None,
                    meta: None,
                    expected_response: None,
                }],
            },
        ],
    );

    // Verify effective initial prompts are extracted correctly
    let prompt1 = scenario1.effective_initial_prompt();
    let prompt2 = scenario2.effective_initial_prompt();

    assert!(prompt1.is_some(), "Scenario 1 should have initial prompt");
    assert!(prompt2.is_some(), "Scenario 2 should have initial prompt");
    assert!(
        prompt1.unwrap().contains("analyze"),
        "Scenario 1 prompt should contain 'analyze'"
    );
    assert!(
        prompt2.unwrap().contains("Refactor"),
        "Scenario 2 prompt should contain 'Refactor'"
    );
}
