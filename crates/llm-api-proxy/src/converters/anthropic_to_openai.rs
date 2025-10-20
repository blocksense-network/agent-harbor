// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Convert Anthropic API responses to OpenAI compatible structures

use anthropic_ai_sdk::types::message as anthropic;
use async_openai::types as openai;
use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

use super::ConversionResponse;

/// Convert Anthropic response to OpenAI response
#[allow(deprecated)]
pub fn convert_response(
    response: anthropic::CreateMessageResponse,
) -> Result<ConversionResponse<openai::CreateChatCompletionResponse>, crate::Error> {
    let mut warnings = Vec::new();

    let (content, tool_calls, mut local_warnings) = convert_content_blocks(&response.content);
    warnings.append(&mut local_warnings);

    let finish_reason = response.stop_reason.and_then(map_stop_reason_to_finish);

    let usage = Some(openai::CompletionUsage {
        prompt_tokens: response.usage.input_tokens,
        completion_tokens: response.usage.output_tokens,
        total_tokens: response.usage.input_tokens + response.usage.output_tokens,
        ..Default::default()
    });

    let message = openai::ChatCompletionResponseMessage {
        content,
        refusal: None,
        role: openai::Role::Assistant,
        tool_calls: if tool_calls.is_empty() {
            None
        } else {
            Some(tool_calls)
        },
        function_call: None,
        audio: None,
    };

    let choice = openai::ChatChoice {
        index: 0,
        message,
        finish_reason,
        logprobs: None,
    };

    let payload = openai::CreateChatCompletionResponse {
        id: format!("chatcmpl-{}", response.id),
        choices: vec![choice],
        created: Utc::now().timestamp() as u32,
        model: map_anthropic_model_to_openai(&response.model),
        service_tier: None,
        system_fingerprint: None,
        object: "chat.completion".to_string(),
        usage,
    };

    Ok(ConversionResponse { payload, warnings })
}

/// Convert OpenAI chat completion response into Anthropic format
pub fn convert_response_to_anthropic(
    response: openai::CreateChatCompletionResponse,
) -> Result<ConversionResponse<anthropic::CreateMessageResponse>, crate::Error> {
    let warnings = Vec::new();

    if response.choices.is_empty() {
        return Err(crate::Error::Conversion {
            message: "OpenAI response does not contain any choices".to_string(),
        });
    }

    let first_choice = &response.choices[0];
    let mut blocks = Vec::new();
    if let Some(content) = &first_choice.message.content {
        if !content.is_empty() {
            blocks.push(anthropic::ContentBlock::text(content.clone()));
        }
    }

    if let Some(tool_calls) = &first_choice.message.tool_calls {
        for call in tool_calls {
            let input = serde_json::from_str::<Value>(&call.function.arguments)
                .unwrap_or_else(|_| Value::String(call.function.arguments.clone()));
            blocks.push(anthropic::ContentBlock::ToolUse {
                id: call.id.clone(),
                name: call.function.name.clone(),
                input,
            });
        }
    }

    if blocks.is_empty() {
        blocks.push(anthropic::ContentBlock::text(""));
    }

    let stop_reason = first_choice.finish_reason.and_then(map_finish_reason_to_stop);

    let usage = response.usage.clone().unwrap_or_default();

    let payload = anthropic::CreateMessageResponse {
        content: blocks,
        id: response.id,
        model: map_openai_model_to_anthropic(&response.model),
        role: anthropic::Role::Assistant,
        stop_reason,
        stop_sequence: None,
        type_: "message".to_string(),
        usage: anthropic::Usage {
            input_tokens: usage.prompt_tokens,
            output_tokens: usage.completion_tokens,
        },
    };

    Ok(ConversionResponse { payload, warnings })
}

/// Convert Anthropic stream event into OpenAI stream chunk
pub fn convert_stream_chunk(
    event: anthropic::StreamEvent,
) -> Result<Option<ConversionResponse<openai::CreateChatCompletionStreamResponse>>, crate::Error> {
    let mut warnings = Vec::new();

    let (delta, finish_reason, usage) = match event {
        anthropic::StreamEvent::ContentBlockDelta { delta, .. } => match delta {
            anthropic::ContentBlockDelta::TextDelta { text } => (
                #[allow(deprecated)]
                openai::ChatCompletionStreamResponseDelta {
                    content: Some(text),
                    function_call: None,
                    tool_calls: None,
                    role: Some(openai::Role::Assistant),
                    refusal: None,
                },
                None,
                None,
            ),
            anthropic::ContentBlockDelta::InputJsonDelta { partial_json } => (
                #[allow(deprecated)]
                openai::ChatCompletionStreamResponseDelta {
                    content: None,
                    function_call: None,
                    tool_calls: Some(vec![openai::ChatCompletionMessageToolCallChunk {
                        index: 0,
                        id: None,
                        r#type: Some(openai::ChatCompletionToolType::Function),
                        function: Some(openai::FunctionCallStream {
                            name: None,
                            arguments: Some(partial_json),
                        }),
                    }]),
                    role: Some(openai::Role::Assistant),
                    refusal: None,
                },
                None,
                None,
            ),
            anthropic::ContentBlockDelta::ThinkingDelta { .. }
            | anthropic::ContentBlockDelta::SignatureDelta { .. } => {
                warnings.push(
                    "Skipping Anthropic thinking delta in OpenAI stream conversion".to_string(),
                );
                return Ok(None);
            }
        },
        anthropic::StreamEvent::MessageDelta { delta, usage } => (
            #[allow(deprecated)]
            openai::ChatCompletionStreamResponseDelta {
                content: None,
                function_call: None,
                tool_calls: None,
                role: Some(openai::Role::Assistant),
                refusal: None,
            },
            delta.stop_reason.and_then(map_stop_reason_to_finish),
            usage.map(|u| openai::CompletionUsage {
                prompt_tokens: u.input_tokens,
                completion_tokens: u.output_tokens,
                total_tokens: u.input_tokens + u.output_tokens,
                ..Default::default()
            }),
        ),
        anthropic::StreamEvent::MessageStop => (
            #[allow(deprecated)]
            openai::ChatCompletionStreamResponseDelta {
                content: None,
                function_call: None,
                tool_calls: None,
                role: Some(openai::Role::Assistant),
                refusal: None,
            },
            Some(openai::FinishReason::Stop),
            None,
        ),
        anthropic::StreamEvent::MessageStart { .. }
        | anthropic::StreamEvent::ContentBlockStart { .. }
        | anthropic::StreamEvent::ContentBlockStop { .. }
        | anthropic::StreamEvent::Ping => {
            return Ok(None);
        }
        anthropic::StreamEvent::Error { error } => {
            return Err(crate::Error::Conversion {
                message: format!("Anthropic stream error: {}", error.message),
            });
        }
    };

    let chunk = openai::CreateChatCompletionStreamResponse {
        id: format!("chatcmpl-{}", Uuid::new_v4()),
        choices: vec![openai::ChatChoiceStream {
            index: 0,
            delta,
            finish_reason,
            logprobs: None,
        }],
        created: Utc::now().timestamp() as u32,
        model: "anthropic-proxy".to_string(),
        service_tier: None,
        system_fingerprint: None,
        object: "chat.completion.chunk".to_string(),
        usage,
    };

    Ok(Some(ConversionResponse {
        payload: chunk,
        warnings,
    }))
}

/// Convert OpenAI stream chunk into Anthropic stream event
pub fn convert_stream_chunk_to_anthropic(
    chunk: openai::CreateChatCompletionStreamResponse,
) -> Result<Option<ConversionResponse<anthropic::StreamEvent>>, crate::Error> {
    if chunk.choices.is_empty() {
        return Ok(None);
    }

    let choice = &chunk.choices[0];

    if let Some(content) = &choice.delta.content {
        if !content.is_empty() {
            let event = anthropic::StreamEvent::ContentBlockDelta {
                index: 0,
                delta: anthropic::ContentBlockDelta::TextDelta {
                    text: content.clone(),
                },
            };
            return Ok(Some(ConversionResponse {
                payload: event,
                warnings: Vec::new(),
            }));
        }
    }

    if let Some(tool_calls) = &choice.delta.tool_calls {
        if let Some(call) = tool_calls.first() {
            if let Some(function) = call.function.as_ref() {
                if let Some(arguments) = &function.arguments {
                    let event = anthropic::StreamEvent::ContentBlockDelta {
                        index: 0,
                        delta: anthropic::ContentBlockDelta::InputJsonDelta {
                            partial_json: arguments.clone(),
                        },
                    };
                    return Ok(Some(ConversionResponse {
                        payload: event,
                        warnings: Vec::new(),
                    }));
                }
            }
        }
    }

    if let Some(finish) = choice.finish_reason {
        let event = anthropic::StreamEvent::MessageDelta {
            delta: anthropic::MessageDeltaContent {
                stop_reason: map_finish_reason_to_stop(finish.clone()),
                stop_sequence: None,
            },
            usage: chunk.usage.as_ref().map(|usage| anthropic::StreamUsage {
                input_tokens: usage.prompt_tokens,
                output_tokens: usage.completion_tokens,
            }),
        };
        return Ok(Some(ConversionResponse {
            payload: event,
            warnings: Vec::new(),
        }));
    }

    Ok(None)
}

fn convert_content_blocks(
    blocks: &[anthropic::ContentBlock],
) -> (
    Option<String>,
    Vec<openai::ChatCompletionMessageToolCall>,
    Vec<String>,
) {
    let mut text_segments = Vec::new();
    let mut tool_calls = Vec::new();
    let mut warnings = Vec::new();

    for block in blocks {
        match block {
            anthropic::ContentBlock::Text { text } => text_segments.push(text.clone()),
            anthropic::ContentBlock::ToolUse { id, name, input } => {
                let arguments = serde_json::to_string(input).unwrap_or_else(|_| "{}".to_string());
                tool_calls.push(openai::ChatCompletionMessageToolCall {
                    id: id.clone(),
                    r#type: openai::ChatCompletionToolType::Function,
                    function: openai::FunctionCall {
                        name: name.clone(),
                        arguments,
                    },
                });
            }
            anthropic::ContentBlock::ToolResult { .. } => {
                warnings.push(
                    "ToolResult blocks are not included in OpenAI assistant responses".to_string(),
                );
            }
            anthropic::ContentBlock::Image { .. } => {
                warnings.push(
                    "Image blocks cannot be represented in OpenAI chat responses".to_string(),
                );
            }
            anthropic::ContentBlock::Thinking { .. }
            | anthropic::ContentBlock::RedactedThinking { .. } => {
                // Drop thinking content for OpenAI compatibility.
            }
        }
    }

    let content = if text_segments.is_empty() {
        None
    } else {
        Some(text_segments.join("\n\n"))
    };

    (content, tool_calls, warnings)
}

fn map_stop_reason_to_finish(reason: anthropic::StopReason) -> Option<openai::FinishReason> {
    match reason {
        anthropic::StopReason::EndTurn | anthropic::StopReason::StopSequence => {
            Some(openai::FinishReason::Stop)
        }
        anthropic::StopReason::MaxTokens => Some(openai::FinishReason::Length),
        anthropic::StopReason::ToolUse => Some(openai::FinishReason::ToolCalls),
        anthropic::StopReason::Refusal => Some(openai::FinishReason::ContentFilter),
    }
}

fn map_finish_reason_to_stop(reason: openai::FinishReason) -> Option<anthropic::StopReason> {
    match reason {
        openai::FinishReason::Stop => Some(anthropic::StopReason::EndTurn),
        openai::FinishReason::Length => Some(anthropic::StopReason::MaxTokens),
        openai::FinishReason::ToolCalls | openai::FinishReason::FunctionCall => {
            Some(anthropic::StopReason::ToolUse)
        }
        openai::FinishReason::ContentFilter => Some(anthropic::StopReason::Refusal),
    }
}

fn map_anthropic_model_to_openai(anthropic_model: &str) -> String {
    if anthropic_model.to_ascii_lowercase().contains("gpt") {
        anthropic_model.to_string()
    } else if anthropic_model.contains("sonnet") {
        "gpt-4o".to_string()
    } else if anthropic_model.contains("opus") {
        "gpt-4.1".to_string()
    } else {
        "gpt-4o-mini".to_string()
    }
}

fn map_openai_model_to_anthropic(openai_model: &str) -> String {
    if openai_model.to_ascii_lowercase().contains("claude")
        || openai_model.to_ascii_lowercase().contains("anthropic")
    {
        openai_model.to_string()
    } else if openai_model.starts_with("gpt-4o") || openai_model.starts_with("gpt-4.1") {
        "claude-3.5-sonnet-20241022".to_string()
    } else if openai_model.starts_with("gpt-4") {
        "claude-3-opus-20240229".to_string()
    } else {
        "claude-3-haiku-20240307".to_string()
    }
}
