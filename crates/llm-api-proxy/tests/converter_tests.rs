// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use anthropic_ai_sdk::types::message as anthropic;
use async_openai::types as openai;
use chrono::Utc;
use llm_api_proxy::converters::{anthropic_to_openai, openai_to_anthropic};
use serde_json::json;

#[ah_test_utils::logged_test]
fn test_openai_request_to_anthropic() {
    let request = openai::CreateChatCompletionRequest {
        model: "gpt-4o-mini".to_string(),
        messages: vec![
            openai::ChatCompletionRequestMessage::System(
                openai::ChatCompletionRequestSystemMessage {
                    content: openai::ChatCompletionRequestSystemMessageContent::Text(
                        "You are a code assistant".to_string(),
                    ),
                    name: None,
                },
            ),
            openai::ChatCompletionRequestMessage::User(openai::ChatCompletionRequestUserMessage {
                content: openai::ChatCompletionRequestUserMessageContent::Text(
                    "Write a hello world program".to_string(),
                ),
                name: None,
            }),
            openai::ChatCompletionRequestMessage::Assistant(
                openai::ChatCompletionRequestAssistantMessage {
                    content: Some(openai::ChatCompletionRequestAssistantMessageContent::Text(
                        "Calling tool".to_string(),
                    )),
                    tool_calls: Some(vec![openai::ChatCompletionMessageToolCall {
                        id: "call_1".to_string(),
                        r#type: openai::ChatCompletionToolType::Function,
                        function: openai::FunctionCall {
                            name: "write_file".to_string(),
                            arguments: json!({"path":"main.rs","text":"fn main() {}"}).to_string(),
                        },
                    }]),
                    ..Default::default()
                },
            ),
            openai::ChatCompletionRequestMessage::Tool(openai::ChatCompletionRequestToolMessage {
                content: openai::ChatCompletionRequestToolMessageContent::Text(
                    "File written".to_string(),
                ),
                tool_call_id: "call_1".to_string(),
            }),
        ],
        max_completion_tokens: Some(128),
        ..Default::default()
    };

    let response = openai_to_anthropic::convert_request(request).expect("conversion");
    let params = response.payload;

    assert_eq!(params.model, "claude-3.5-sonnet-20241022");
    assert_eq!(params.max_tokens, 128);
    assert_eq!(params.messages.len(), 3);

    let assistant_blocks = match &params.messages[1].content {
        anthropic::MessageContent::Blocks { content } => content,
        _ => panic!("expected blocks"),
    };
    assert!(
        assistant_blocks
            .iter()
            .any(|block| matches!(block, anthropic::ContentBlock::ToolUse { .. }))
    );
}

#[ah_test_utils::logged_test]
fn test_anthropic_response_to_openai() {
    let response = anthropic::CreateMessageResponse {
        content: vec![
            anthropic::ContentBlock::text("All done"),
            anthropic::ContentBlock::ToolUse {
                id: "tool_1".to_string(),
                name: "run_command".to_string(),
                input: json!({"cmd":"ls"}),
            },
        ],
        id: "resp_123".to_string(),
        model: "claude-3-opus-20240229".to_string(),
        role: anthropic::Role::Assistant,
        stop_reason: Some(anthropic::StopReason::ToolUse),
        stop_sequence: None,
        type_: "message".to_string(),
        usage: anthropic::Usage {
            input_tokens: 10,
            output_tokens: 5,
        },
    };

    let result = anthropic_to_openai::convert_response(response).expect("conversion");
    let completion = result.payload;
    assert_eq!(completion.choices.len(), 1);
    let choice = &completion.choices[0];
    assert_eq!(choice.message.role, openai::Role::Assistant);
    assert!(
        choice
            .message
            .tool_calls
            .as_ref()
            .map(|calls| !calls.is_empty())
            .unwrap_or(false)
    );
    assert_eq!(choice.finish_reason, Some(openai::FinishReason::ToolCalls));
    assert_eq!(completion.usage.as_ref().unwrap().total_tokens, 15);
}

#[ah_test_utils::logged_test]
fn test_stream_event_conversions() {
    let chunk = openai::CreateChatCompletionStreamResponse {
        id: "chunk_1".to_string(),
        choices: vec![openai::ChatChoiceStream {
            index: 0,
            delta: {
                #[allow(deprecated)]
                openai::ChatCompletionStreamResponseDelta {
                    content: Some("Hello".to_string()),
                    tool_calls: None,
                    role: Some(openai::Role::Assistant),
                    refusal: None,
                    function_call: None,
                }
            },
            finish_reason: None,
            logprobs: None,
        }],
        created: Utc::now().timestamp() as u32,
        model: "gpt-4o-mini".to_string(),
        service_tier: None,
        system_fingerprint: None,
        object: "chat.completion.chunk".to_string(),
        usage: None,
    };

    let anthropic_event = openai_to_anthropic::convert_stream_chunk(chunk)
        .expect("stream convert")
        .unwrap()
        .payload;

    match anthropic_event {
        anthropic::StreamEvent::ContentBlockDelta { .. } => {}
        other => panic!("Unexpected event: {:?}", other),
    }

    let openai_chunk =
        anthropic_to_openai::convert_stream_chunk(anthropic::StreamEvent::ContentBlockDelta {
            index: 0,
            delta: anthropic::ContentBlockDelta::TextDelta {
                text: "World".to_string(),
            },
        })
        .expect("stream back")
        .unwrap()
        .payload;

    assert_eq!(
        openai_chunk.choices[0].delta.content.as_deref(),
        Some("World")
    );
}
