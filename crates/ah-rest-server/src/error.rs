//! Server error types and handling

use ah_rest_api_contract::ProblemDetails;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};

/// Server result type
pub type ServerResult<T> = Result<T, ServerError>;

/// Server error types
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("Database error: {0}")]
    Database(#[from] ah_local_db::Error),

    #[error("Authentication error: {0}")]
    Auth(String),

    #[error("Authorization error: {0}")]
    Authorization(String),

    #[error("Validation error: {0}")]
    Validation(#[from] validator::ValidationErrors),

    #[error("Session not found: {0}")]
    SessionNotFound(String),

    #[error("Task not found: {0}")]
    TaskNotFound(String),

    #[error("Invalid request: {0}")]
    BadRequest(String),

    #[error("Internal server error: {0}")]
    Internal(String),

    #[error("Rate limited")]
    RateLimited,

    #[error("Not implemented: {0}")]
    NotImplemented(String),
}

impl ServerError {
    /// Convert error to Problem+JSON response
    pub fn to_problem(&self) -> ProblemDetails {
        match self {
            ServerError::Database(err) => ProblemDetails {
                problem_type: "https://docs.example.com/errors/database".to_string(),
                title: "Database Error".to_string(),
                status: Some(StatusCode::INTERNAL_SERVER_ERROR.as_u16()),
                detail: format!("Database operation failed: {}", err),
                errors: Default::default(),
            },
            ServerError::Auth(msg) => ProblemDetails {
                problem_type: "https://docs.example.com/errors/auth".to_string(),
                title: "Authentication Failed".to_string(),
                status: Some(StatusCode::UNAUTHORIZED.as_u16()),
                detail: msg.clone(),
                errors: Default::default(),
            },
            ServerError::Authorization(msg) => ProblemDetails {
                problem_type: "https://docs.example.com/errors/authz".to_string(),
                title: "Authorization Failed".to_string(),
                status: Some(StatusCode::FORBIDDEN.as_u16()),
                detail: msg.clone(),
                errors: Default::default(),
            },
            ServerError::Validation(err) => ProblemDetails {
                problem_type: "https://docs.example.com/errors/validation".to_string(),
                title: "Validation Error".to_string(),
                status: Some(StatusCode::BAD_REQUEST.as_u16()),
                detail: "Request validation failed".to_string(),
                errors: Default::default(), // TODO: Properly handle validation errors
            },
            ServerError::SessionNotFound(id) => ProblemDetails {
                problem_type: "https://docs.example.com/errors/not-found".to_string(),
                title: "Session Not Found".to_string(),
                status: Some(StatusCode::NOT_FOUND.as_u16()),
                detail: format!("Session with ID '{}' not found", id),
                errors: Default::default(),
            },
            ServerError::TaskNotFound(id) => ProblemDetails {
                problem_type: "https://docs.example.com/errors/not-found".to_string(),
                title: "Task Not Found".to_string(),
                status: Some(StatusCode::NOT_FOUND.as_u16()),
                detail: format!("Task with ID '{}' not found", id),
                errors: Default::default(),
            },
            ServerError::BadRequest(msg) => ProblemDetails {
                problem_type: "https://docs.example.com/errors/bad-request".to_string(),
                title: "Bad Request".to_string(),
                status: Some(StatusCode::BAD_REQUEST.as_u16()),
                detail: msg.clone(),
                errors: Default::default(),
            },
            ServerError::Internal(msg) => ProblemDetails {
                problem_type: "https://docs.example.com/errors/internal".to_string(),
                title: "Internal Server Error".to_string(),
                status: Some(StatusCode::INTERNAL_SERVER_ERROR.as_u16()),
                detail: msg.clone(),
                errors: Default::default(),
            },
            ServerError::RateLimited => ProblemDetails {
                problem_type: "https://docs.example.com/errors/rate-limited".to_string(),
                title: "Rate Limited".to_string(),
                status: Some(StatusCode::TOO_MANY_REQUESTS.as_u16()),
                detail: "Too many requests".to_string(),
                errors: Default::default(),
            },
            ServerError::NotImplemented(feature) => ProblemDetails {
                problem_type: "https://docs.example.com/errors/not-implemented".to_string(),
                title: "Not Implemented".to_string(),
                status: Some(StatusCode::NOT_IMPLEMENTED.as_u16()),
                detail: format!("Feature '{}' is not yet implemented", feature),
                errors: Default::default(),
            },
        }
    }
}

impl IntoResponse for ServerError {
    fn into_response(self) -> Response {
        let problem = self.to_problem();
        let status = StatusCode::from_u16(problem.status.unwrap_or(500)).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        (status, Json(problem)).into_response()
    }
}

/// Convert any error to ServerError
impl From<anyhow::Error> for ServerError {
    fn from(err: anyhow::Error) -> Self {
        ServerError::Internal(err.to_string())
    }
}

/// Convert validation errors
impl From<validator::ValidationError> for ServerError {
    fn from(err: validator::ValidationError) -> Self {
        ServerError::BadRequest(err.to_string())
    }
}

/// Convert IO errors
impl From<std::io::Error> for ServerError {
    fn from(err: std::io::Error) -> Self {
        ServerError::Internal(format!("IO error: {}", err))
    }
}
