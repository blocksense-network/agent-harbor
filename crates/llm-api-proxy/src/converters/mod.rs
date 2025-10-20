// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! API format conversion between different LLM providers
//!
//! This module provides bidirectional conversion between:
//! - OpenAI API format
//! - Anthropic API format
//! - Other provider formats as needed
//!
//! The conversions are based on the Helicone ai-gateway mapper implementations,
//! adapted for our use case.

pub mod anthropic_to_openai;
pub mod openai_to_anthropic;

use anthropic_ai_sdk::types as anthropic;
use async_openai::types as openai;
use serde::{Deserialize, Serialize};

/// Supported API formats
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApiFormat {
    /// OpenAI API format
    OpenAI,
    /// OpenAI Responses API format
    OpenAIResponses,
    /// Anthropic API format
    Anthropic,
}

/// Request for API conversion
#[derive(Debug, Clone)]
pub struct ConversionRequest<T> {
    /// Source API format
    pub source_format: ApiFormat,
    /// Target API format
    pub target_format: ApiFormat,
    /// The request payload to convert
    pub payload: T,
}

/// Response from API conversion
#[derive(Debug, Clone)]
pub struct ConversionResponse<T> {
    /// The converted response payload
    pub payload: T,
    /// Any warnings during conversion
    pub warnings: Vec<String>,
}

/// Trait for types that can be converted between API formats
pub trait Convertible {
    /// Convert from one API format to another
    fn convert(
        &self,
        from: ApiFormat,
        to: ApiFormat,
    ) -> Result<ConversionResponse<Self>, crate::Error>
    where
        Self: Sized;
}

/// Convert OpenAI request to Anthropic request
pub fn openai_to_anthropic_request(
    request: openai::CreateChatCompletionRequest,
) -> Result<ConversionResponse<anthropic::message::CreateMessageParams>, crate::Error> {
    openai_to_anthropic::convert_request(request)
}

/// Convert Anthropic response to OpenAI response
pub fn anthropic_to_openai_response(
    response: anthropic::message::CreateMessageResponse,
) -> Result<ConversionResponse<openai::CreateChatCompletionResponse>, crate::Error> {
    anthropic_to_openai::convert_response(response)
}

/// Convert streaming responses
pub mod streaming {
    use super::*;

    /// Convert OpenAI streaming chunk to Anthropic format
    pub fn openai_to_anthropic_chunk(
        chunk: openai::CreateChatCompletionStreamResponse,
    ) -> Result<Option<ConversionResponse<anthropic::message::StreamEvent>>, crate::Error> {
        openai_to_anthropic::convert_stream_chunk(chunk)
    }

    /// Convert Anthropic streaming event to OpenAI format
    pub fn anthropic_to_openai_chunk(
        event: anthropic::message::StreamEvent,
    ) -> Result<Option<ConversionResponse<openai::CreateChatCompletionStreamResponse>>, crate::Error>
    {
        anthropic_to_openai::convert_stream_chunk(event)
    }
}
