// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Core error types for the AH system.

/// Core error type for all AH operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Task error: {message}")]
    Task { message: String },

    #[error("Session error: {message}")]
    Session { message: String },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("Database error: {0}")]
    Database(#[from] ah_local_db::Error),

    #[error("Generic error: {0}")]
    Generic(String),
}

impl Error {
    /// Create a new task-related error.
    pub fn task<S: Into<String>>(message: S) -> Self {
        Self::Task {
            message: message.into(),
        }
    }

    /// Create a new session-related error.
    pub fn session<S: Into<String>>(message: S) -> Self {
        Self::Session {
            message: message.into(),
        }
    }

    /// Create a new generic error.
    pub fn generic<S: Into<String>>(message: S) -> Self {
        Self::Generic(message.into())
    }
}
