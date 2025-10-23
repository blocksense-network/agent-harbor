// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Error types for API contract validation and parsing

use thiserror::Error;

/// Errors that can occur during API contract validation and parsing
#[derive(Debug, Error)]
pub enum ApiContractError {
    #[error("Validation error: {0}")]
    Validation(#[from] validator::ValidationErrors),

    #[error("JSON parsing error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("URL parsing error: {0}")]
    Url(#[from] url::ParseError),

    #[error("UUID parsing error: {0}")]
    Uuid(#[from] uuid::Error),

    #[error("Invalid session status: {0}")]
    InvalidSessionStatus(String),

    #[error("Invalid event type: {0}")]
    InvalidEventType(String),

    #[error("Invalid log level: {0}")]
    InvalidLogLevel(String),

    #[error("Invalid repo mode: {0}")]
    InvalidRepoMode(String),

    #[error("Invalid runtime type: {0}")]
    InvalidRuntimeType(String),

    #[error("Invalid delivery mode: {0}")]
    InvalidDeliveryMode(String),

    #[error("Invalid ULID format: {0}")]
    InvalidUlid(String),
}

/// Problem+JSON error response format as per RFC 7807
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct ProblemDetails {
    #[serde(rename = "type")]
    pub problem_type: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    pub detail: String,
    #[serde(skip_serializing_if = "std::collections::HashMap::is_empty", default)]
    pub errors: std::collections::HashMap<String, Vec<String>>,
}
