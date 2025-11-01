// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Convert OpenAI API requests/responses to Anthropic format

use std::collections::HashMap;

use anthropic_ai_sdk::types::message as anthropic;
use async_openai::types as openai;
use chrono::Utc;
use serde_json::{Value, json};
use uuid::Uuid;

use super::ConversionResponse;

const DEFAULT_MAX_TOKENS: u32 = 1024;

/// Convert OpenAI chat completion request to Anthropic message request
pub fn convert_request(
    request: openai::CreateChatCompletionRequest,
) -> Result<ConversionResponse<anthropic::CreateMessageParams>, crate::Error> {
    let mut warnings = Vec::new();

    let mut system_segments: Vec<String> = Vec::new();
    let mut converted_messages: Vec<anthropic::Message> = Vec::new();

    for (idx, message) in request.messages.into_iter().enumerate() {
        match message {
            openai::ChatCompletionRequestMessage::System(system) => {
                let (text, mut local_warnings) = extract_system_content(system.content);
                if !text.is_empty() {
                    system_segments.push(text);
                } else {
                    warnings.push(format!("System message #{idx} was empty after conversion"));
                }
                warnings.append(&mut local_warnings);
            }
            openai::ChatCompletionRequestMessage::Developer(dev) => {
                let (text, mut local_warnings) = extract_developer_content(dev.content);
                if !text.is_empty() {
                    system_segments.push(text);
                } else {
                    warnings.push(format!(
                        "Developer message #{idx} was empty after conversion"
                    ));
                }
                warnings.append(&mut local_warnings);
            }
            openai::ChatCompletionRequestMessage::User(user) => {
                let (message, mut local_warnings) = convert_user_message(user);
                converted_messages.push(message);
                warnings.append(&mut local_warnings);
            }
            openai::ChatCompletionRequestMessage::Assistant(assistant) => {
                let (message, mut local_warnings) = convert_assistant_message(assistant);
                converted_messages.push(message);
                warnings.append(&mut local_warnings);
            }
            openai::ChatCompletionRequestMessage::Tool(tool) => {
                let (message, mut local_warnings) = convert_tool_message(tool);
                converted_messages.push(message);
                warnings.append(&mut local_warnings);
            }
            openai::ChatCompletionRequestMessage::Function(function) => {
                let (message, mut local_warnings) = convert_function_message(function);
                converted_messages.push(message);
                warnings.append(&mut local_warnings);
            }
        }
    }

    if converted_messages.is_empty() {
        return Err(crate::Error::Conversion {
            message: "No convertible messages present in OpenAI request".to_string(),
        });
    }

    // Validate message alternation after system message extraction
    validate_message_alternation(&converted_messages, &mut warnings);

    #[allow(deprecated)]
    let legacy_max_tokens = request.max_tokens;
    let max_tokens = request
        .max_completion_tokens
        .or(legacy_max_tokens)
        .unwrap_or(DEFAULT_MAX_TOKENS);

    let model = map_openai_model_to_anthropic(&request.model);

    let required = anthropic::RequiredMessageParams {
        model,
        messages: converted_messages,
        max_tokens,
    };

    let mut params = anthropic::CreateMessageParams::new(required);

    if !system_segments.is_empty() {
        params.system = Some(system_segments.join("\n"));
    }

    if let Some(temp) = request.temperature {
        params.temperature = Some(temp);
    }

    if let Some(top_p) = request.top_p {
        params.top_p = Some(top_p);
    }

    if let Some(stop) = request.stop {
        let stop_sequences = match stop {
            openai::Stop::String(s) => vec![s],
            openai::Stop::StringArray(arr) => arr,
        };
        if !stop_sequences.is_empty() {
            params.stop_sequences = Some(stop_sequences);
        }
    }

    if let Some(stream) = request.stream {
        params.stream = Some(stream);
    }

    if let Some(tools) = request.tools {
        match convert_tools(tools) {
            Ok(converted) => {
                if !converted.is_empty() {
                    params.tools = Some(converted);
                }
            }
            Err(errs) => warnings.extend(errs),
        }
    }

    if let Some(choice) = request.tool_choice {
        match convert_tool_choice(choice) {
            Some(tool_choice) => params.tool_choice = Some(tool_choice),
            None => warnings
                .push("Unable to map tool_choice to Anthropic; defaulting to auto".to_string()),
        }
    }

    if let Some(metadata) = request.metadata {
        match convert_metadata(metadata) {
            Ok(meta) => params.metadata = Some(meta),
            Err(warning) => warnings.push(warning),
        }
    }

    if request.response_format.is_some() {
        warnings.push(
            "OpenAI response_format is not supported by Anthropic and has been ignored".to_string(),
        );
    }

    if request.frequency_penalty.is_some() {
        warnings.push("frequency_penalty is not supported by Anthropic".to_string());
    }

    if request.presence_penalty.is_some() {
        warnings.push("presence_penalty is not supported by Anthropic".to_string());
    }

    Ok(ConversionResponse {
        payload: params,
        warnings,
    })
}

/// Convert Anthropic response to OpenAI response
#[allow(deprecated)]
pub fn convert_response(
    response: anthropic::CreateMessageResponse,
) -> Result<ConversionResponse<openai::CreateChatCompletionResponse>, crate::Error> {
    let mut warnings = Vec::new();

    let (message, tool_calls, mut content_warnings) = convert_anthropic_content(&response.content);
    warnings.append(&mut content_warnings);

    let finish_reason = response.stop_reason.and_then(|reason| map_stop_reason(reason));

    let usage = Some(openai::CompletionUsage {
        prompt_tokens: response.usage.input_tokens,
        completion_tokens: response.usage.output_tokens,
        total_tokens: response.usage.input_tokens + response.usage.output_tokens,
        ..Default::default()
    });

    let choice = openai::ChatChoice {
        index: 0,
        message: openai::ChatCompletionResponseMessage {
            content: message,
            refusal: None,
            role: openai::Role::Assistant,
            tool_calls: if tool_calls.is_empty() {
                None
            } else {
                Some(tool_calls)
            },
            function_call: None,
            audio: None,
        },
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

/// Convert OpenAI streaming chunk to Anthropic stream event
pub fn convert_stream_chunk(
    chunk: openai::CreateChatCompletionStreamResponse,
) -> Result<Option<ConversionResponse<anthropic::StreamEvent>>, crate::Error> {
    if chunk.choices.is_empty() {
        return Ok(None);
    }

    let choice = &chunk.choices[0];
    let warnings = Vec::new();

    if let Some(text) = &choice.delta.content {
        if !text.is_empty() {
            let event = anthropic::StreamEvent::ContentBlockDelta {
                index: 0,
                delta: anthropic::ContentBlockDelta::TextDelta { text: text.clone() },
            };
            return Ok(Some(ConversionResponse {
                payload: event,
                warnings,
            }));
        }
    }

    if let Some(finish) = choice.finish_reason {
        let event = anthropic::StreamEvent::MessageDelta {
            delta: anthropic::MessageDeltaContent {
                stop_reason: map_finish_reason(finish.clone()),
                stop_sequence: None,
            },
            usage: chunk.usage.as_ref().map(|usage| anthropic::StreamUsage {
                input_tokens: usage.prompt_tokens,
                output_tokens: usage.completion_tokens,
            }),
        };
        return Ok(Some(ConversionResponse {
            payload: event,
            warnings,
        }));
    }

    // Handle streaming tool call deltas
    if let Some(tool_calls) = &choice.delta.tool_calls {
        if let Some(tool_call) = tool_calls.first() {
            if let Some(function) = &tool_call.function {
                // Handle tool call ID (start of tool call)
                if let Some(id) = &tool_call.id {
                    if let (Some(name), Some(arguments)) = (&function.name, &function.arguments) {
                        // This is a complete tool call start - create ContentBlockStart
                        if !arguments.is_empty() {
                            // First send ContentBlockStart
                            let start_event = anthropic::StreamEvent::ContentBlockStart {
                                index: 0,
                                content_block: anthropic::ContentBlock::ToolUse {
                                    id: id.clone(),
                                    name: name.clone(),
                                    input: serde_json::from_str(arguments)
                                        .unwrap_or_else(|_| json!({})),
                                },
                            };
                            return Ok(Some(ConversionResponse {
                                payload: start_event,
                                warnings,
                            }));
                        }
                    }
                } else {
                    // Handle tool call argument continuation
                    if let Some(arguments) = &function.arguments {
                        if !arguments.is_empty() {
                            let event = anthropic::StreamEvent::ContentBlockDelta {
                                index: 0,
                                delta: anthropic::ContentBlockDelta::InputJsonDelta {
                                    partial_json: arguments.clone(),
                                },
                            };
                            return Ok(Some(ConversionResponse {
                                payload: event,
                                warnings,
                            }));
                        }
                    }
                }
            }
        }
    }

    Ok(None)
}

/// Convert Anthropic streaming event to OpenAI streaming chunk
pub fn convert_stream_event(
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
            anthropic::ContentBlockDelta::ThinkingDelta { .. }
            | anthropic::ContentBlockDelta::SignatureDelta { .. }
            | anthropic::ContentBlockDelta::InputJsonDelta { .. } => {
                warnings.push("Skipping non-text content block delta in stream".to_string());
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
            delta.stop_reason.and_then(map_stop_reason),
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

/// Validate that messages alternate properly between user and assistant roles
/// after system messages have been extracted to the system parameter
fn validate_message_alternation(messages: &[anthropic::Message], warnings: &mut Vec<String>) {
    use anthropic::Role;

    if messages.is_empty() {
        return;
    }

    // Check if first message is from assistant (should be user first, but assistant responses can start conversations)
    if matches!(messages[0].role, Role::Assistant) {
        warnings.push(
            "Conversation starts with assistant message, which may indicate missing user context"
                .to_string(),
        );
    }

    // Validate alternation for the rest of the conversation
    for (i, message) in messages.iter().enumerate() {
        if i == 0 {
            continue;
        }

        let prev_role = &messages[i - 1].role;
        let curr_role = &message.role;

        // Messages should alternate between User and Assistant
        match (prev_role, curr_role) {
            (Role::User, Role::Assistant) | (Role::Assistant, Role::User) => {
                // Valid alternation
            }
            (Role::User, Role::User) => {
                warnings.push(format!(
                    "Consecutive user messages detected at positions {} and {}",
                    i - 1,
                    i
                ));
            }
            (Role::Assistant, Role::Assistant) => {
                warnings.push(format!(
                    "Consecutive assistant messages detected at positions {} and {}",
                    i - 1,
                    i
                ));
            }
        }
    }
}

fn extract_system_content(
    content: openai::ChatCompletionRequestSystemMessageContent,
) -> (String, Vec<String>) {
    match content {
        openai::ChatCompletionRequestSystemMessageContent::Text(text) => (text, Vec::new()),
        openai::ChatCompletionRequestSystemMessageContent::Array(parts) => {
            let segments = parts
                .into_iter()
                .map(|part| match part {
                    openai::ChatCompletionRequestSystemMessageContentPart::Text(t) => t.text,
                })
                .collect::<Vec<_>>();
            (segments.join("\n"), Vec::new())
        }
    }
}

fn extract_developer_content(
    content: openai::ChatCompletionRequestDeveloperMessageContent,
) -> (String, Vec<String>) {
    match content {
        openai::ChatCompletionRequestDeveloperMessageContent::Text(text) => (text, Vec::new()),
        openai::ChatCompletionRequestDeveloperMessageContent::Array(parts) => {
            let joined = parts.into_iter().map(|p| p.text).collect::<Vec<_>>().join("\n");
            (joined, Vec::new())
        }
    }
}

fn convert_user_message(
    message: openai::ChatCompletionRequestUserMessage,
) -> (anthropic::Message, Vec<String>) {
    let mut warnings = Vec::new();
    let blocks = match message.content {
        openai::ChatCompletionRequestUserMessageContent::Text(text) => {
            vec![anthropic::ContentBlock::text(text)]
        }
        openai::ChatCompletionRequestUserMessageContent::Array(parts) => {
            let mut blocks = Vec::new();
            for part in parts {
                match part {
                    openai::ChatCompletionRequestUserMessageContentPart::Text(t) => {
                        blocks.push(anthropic::ContentBlock::text(t.text));
                    }
                    openai::ChatCompletionRequestUserMessageContentPart::ImageUrl(img) => {
                        if let Some(source) = convert_image_part(img.image_url) {
                            blocks.push(anthropic::ContentBlock::Image { source });
                        } else {
                            warnings.push(
                                "Image content could not be converted to Anthropic format"
                                    .to_string(),
                            );
                        }
                    }
                    openai::ChatCompletionRequestUserMessageContentPart::InputAudio(_) => {
                        warnings.push(
                            "Input audio content is not supported for Anthropic conversion"
                                .to_string(),
                        );
                    }
                }
            }
            if blocks.is_empty() {
                blocks.push(anthropic::ContentBlock::text(""));
            }
            blocks
        }
    };

    (
        anthropic::Message::new_blocks(anthropic::Role::User, blocks),
        warnings,
    )
}

fn convert_assistant_message(
    message: openai::ChatCompletionRequestAssistantMessage,
) -> (anthropic::Message, Vec<String>) {
    let warnings = Vec::new();
    let mut blocks: Vec<anthropic::ContentBlock> = Vec::new();

    if let Some(content) = message.content {
        match content {
            openai::ChatCompletionRequestAssistantMessageContent::Text(text) => {
                if !text.is_empty() {
                    blocks.push(anthropic::ContentBlock::text(text));
                }
            }
            openai::ChatCompletionRequestAssistantMessageContent::Array(parts) => {
                for part in parts {
                    match part {
                        openai::ChatCompletionRequestAssistantMessageContentPart::Text(t) => {
                            blocks.push(anthropic::ContentBlock::text(t.text));
                        }
                        openai::ChatCompletionRequestAssistantMessageContentPart::Refusal(r) => {
                            blocks.push(anthropic::ContentBlock::text(r.refusal));
                        }
                    }
                }
            }
        }
    }

    if let Some(tool_calls) = message.tool_calls {
        for call in tool_calls {
            let arguments = serde_json::from_str::<Value>(&call.function.arguments)
                .unwrap_or_else(|_| Value::String(call.function.arguments.clone()));
            blocks.push(anthropic::ContentBlock::ToolUse {
                id: call.id,
                name: call.function.name,
                input: arguments,
            });
        }
    }

    #[allow(deprecated)]
    if let Some(function_call) = message.function_call {
        let arguments = serde_json::from_str::<Value>(&function_call.arguments)
            .unwrap_or_else(|_| Value::String(function_call.arguments));
        blocks.push(anthropic::ContentBlock::ToolUse {
            id: format!("call_{}", Uuid::new_v4()),
            name: function_call.name,
            input: arguments,
        });
    }

    if blocks.is_empty() {
        blocks.push(anthropic::ContentBlock::text(""));
    }

    (
        anthropic::Message::new_blocks(anthropic::Role::Assistant, blocks),
        warnings,
    )
}

fn convert_tool_message(
    message: openai::ChatCompletionRequestToolMessage,
) -> (anthropic::Message, Vec<String>) {
    let warnings = Vec::new();
    let content = match message.content {
        openai::ChatCompletionRequestToolMessageContent::Text(text) => text,
        openai::ChatCompletionRequestToolMessageContent::Array(parts) => parts
            .into_iter()
            .map(|part| match part {
                openai::ChatCompletionRequestToolMessageContentPart::Text(t) => t.text,
            })
            .collect::<Vec<_>>()
            .join("\n"),
    };

    let block = anthropic::ContentBlock::ToolResult {
        tool_use_id: message.tool_call_id,
        content,
    };

    (
        anthropic::Message::new_blocks(anthropic::Role::User, vec![block]),
        warnings,
    )
}

fn convert_function_message(
    message: openai::ChatCompletionRequestFunctionMessage,
) -> (anthropic::Message, Vec<String>) {
    let warnings = Vec::new();
    let content = message.content.unwrap_or_default();
    let block = anthropic::ContentBlock::ToolResult {
        tool_use_id: format!("fn_{}", message.name),
        content,
    };

    (
        anthropic::Message::new_blocks(anthropic::Role::User, vec![block]),
        warnings,
    )
}

fn convert_image_part(image: openai::ImageUrl) -> Option<anthropic::ImageSource> {
    let url = image.url;
    if let Some(data_part) = url.strip_prefix("data:") {
        // Format: data:<media_type>;base64,<data>
        let mut split = data_part.splitn(2, ",");
        let meta = split.next()?;
        let data = split.next()?.to_string();
        let mut meta_parts = meta.splitn(2, ";");
        let media_type = meta_parts.next().unwrap_or("image/png");
        Some(anthropic::ImageSource {
            type_: "base64".to_string(),
            media_type: media_type.to_string(),
            data,
        })
    } else if url.starts_with("http://") || url.starts_with("https://") {
        // Handle URL images - Anthropic supports URLs directly
        Some(anthropic::ImageSource {
            type_: "url".to_string(),
            media_type: "image/jpeg".to_string(), // Default, could be inferred from URL
            data: url,
        })
    } else {
        // Unsupported image format
        None
    }
}

fn convert_tools(
    tools: Vec<openai::ChatCompletionTool>,
) -> Result<Vec<anthropic::Tool>, Vec<String>> {
    let mut converted = Vec::new();
    let mut warnings = Vec::new();

    for tool in tools {
        let function = tool.function;
        let parameters = function
            .parameters
            .unwrap_or_else(|| json!({"type": "object", "properties": {} }));
        converted.push(anthropic::Tool {
            name: function.name,
            description: function.description,
            input_schema: parameters,
        });
        if function.strict.unwrap_or(false) {
            warnings
                .push("OpenAI strict function schemas are not enforced in Anthropic".to_string());
        }
    }

    if warnings.is_empty() {
        Ok(converted)
    } else {
        Err(warnings)
    }
}

fn convert_tool_choice(
    choice: openai::ChatCompletionToolChoiceOption,
) -> Option<anthropic::ToolChoice> {
    match choice {
        openai::ChatCompletionToolChoiceOption::None => Some(anthropic::ToolChoice::None),
        openai::ChatCompletionToolChoiceOption::Auto => Some(anthropic::ToolChoice::Auto),
        openai::ChatCompletionToolChoiceOption::Required => Some(anthropic::ToolChoice::Any),
        openai::ChatCompletionToolChoiceOption::Named(named) => Some(anthropic::ToolChoice::Tool {
            name: named.function.name,
        }),
    }
}

fn convert_metadata(value: Value) -> Result<anthropic::Metadata, String> {
    if let Value::Object(map) = value {
        let mut fields = HashMap::new();
        for (key, val) in map {
            match val {
                Value::String(s) => {
                    fields.insert(key, s);
                }
                other => {
                    return Err(format!(
                        "Metadata value for key '{}' is not a string: {}",
                        key, other
                    ));
                }
            }
        }
        Ok(anthropic::Metadata { fields })
    } else {
        Err("Metadata must be a JSON object with string values".to_string())
    }
}

fn convert_anthropic_content(
    content: &[anthropic::ContentBlock],
) -> (
    Option<String>,
    Vec<openai::ChatCompletionMessageToolCall>,
    Vec<String>,
) {
    let mut text_segments = Vec::new();
    let mut tool_calls = Vec::new();
    let mut warnings = Vec::new();

    for block in content {
        match block {
            anthropic::ContentBlock::Text { text } => text_segments.push(text.clone()),
            anthropic::ContentBlock::ToolUse { id, name, input } => {
                let arguments = match serde_json::to_string(input) {
                    Ok(json) => json,
                    Err(err) => {
                        warnings.push(format!(
                            "Failed to serialize tool input for '{}': {}",
                            name, err
                        ));
                        "{}".to_string()
                    }
                };
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
                // Tool results are provided by the client in OpenAI flows, skip but note warning.
                warnings.push("Received tool_result in assistant response; skipping".to_string());
            }
            anthropic::ContentBlock::Image { .. } => {
                warnings.push("Image blocks in Anthropic response cannot be represented in OpenAI ChatCompletionResponse".to_string());
            }
            anthropic::ContentBlock::Thinking { .. }
            | anthropic::ContentBlock::RedactedThinking { .. } => {
                // Silently drop thinking content for OpenAI compatibility.
            }
        }
    }

    let content_string = if text_segments.is_empty() {
        None
    } else {
        Some(text_segments.join("\n\n"))
    };

    (content_string, tool_calls, warnings)
}

fn map_stop_reason(reason: anthropic::StopReason) -> Option<openai::FinishReason> {
    match reason {
        anthropic::StopReason::EndTurn | anthropic::StopReason::StopSequence => {
            Some(openai::FinishReason::Stop)
        }
        anthropic::StopReason::MaxTokens => Some(openai::FinishReason::Length),
        anthropic::StopReason::ToolUse => Some(openai::FinishReason::ToolCalls),
        anthropic::StopReason::Refusal => Some(openai::FinishReason::ContentFilter),
    }
}

fn map_finish_reason(reason: openai::FinishReason) -> Option<anthropic::StopReason> {
    match reason {
        openai::FinishReason::Stop => Some(anthropic::StopReason::EndTurn),
        openai::FinishReason::Length => Some(anthropic::StopReason::MaxTokens),
        openai::FinishReason::ToolCalls => Some(anthropic::StopReason::ToolUse),
        openai::FinishReason::ContentFilter => Some(anthropic::StopReason::Refusal),
        openai::FinishReason::FunctionCall => Some(anthropic::StopReason::ToolUse),
    }
}

fn map_openai_model_to_anthropic(openai_model: &str) -> String {
    if openai_model.to_ascii_lowercase().contains("claude")
        || openai_model.to_ascii_lowercase().contains("anthropic")
    {
        return openai_model.to_string();
    }

    if openai_model.starts_with("gpt-4.1") || openai_model.starts_with("gpt-4o") {
        "claude-3.5-sonnet-20241022".to_string()
    } else if openai_model.starts_with("gpt-4") {
        "claude-3-opus-20240229".to_string()
    } else {
        "claude-3-haiku-20240307".to_string()
    }
}

fn map_anthropic_model_to_openai(anthropic_model: &str) -> String {
    if anthropic_model.to_ascii_lowercase().contains("gpt") {
        anthropic_model.to_string()
    } else if anthropic_model.contains("opus") {
        "gpt-4.1".to_string()
    } else if anthropic_model.contains("sonnet") {
        "gpt-4o".to_string()
    } else {
        "gpt-4o-mini".to_string()
    }
}
