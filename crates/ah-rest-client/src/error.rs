//! Error types for the REST API client

use ah_rest_api_contract::ProblemDetails;
use reqwest::StatusCode;
use thiserror::Error;

/// Errors that can occur when using the REST API client
#[derive(Debug, Error)]
pub enum RestClientError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("URL parsing error: {0}")]
    Url(#[from] url::ParseError),

    #[error("API contract error: {0}")]
    ApiContract(#[from] ah_rest_api_contract::ApiContractError),

    #[error("Authentication error: {0}")]
    Auth(String),

    #[error("Server returned error status {status}: {details:?}")]
    ServerError {
        status: StatusCode,
        details: ProblemDetails,
    },

    #[error("Unexpected response format: {0}")]
    UnexpectedResponse(String),

    #[error("SSE stream error: {0}")]
    Sse(String),

    #[error("Connection timeout")]
    Timeout,

    #[error("Rate limited: {0}")]
    RateLimited(String),
}

/// Result type alias for REST client operations
pub type RestClientResult<T> = Result<T, RestClientError>;
