// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Error types for the TUI application

use ah_rest_client::RestClientError;
use thiserror::Error;

/// Errors that can occur in the TUI application
#[derive(Debug, Error)]
pub enum TuiError {
    #[error("REST client error: {0}")]
    RestClient(#[from] RestClientError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Terminal setup error: {0}")]
    Terminal(String),

    #[error("Event handling error: {0}")]
    Event(String),

    #[error("UI rendering error: {0}")]
    Rendering(String),

    #[error("Application state error: {0}")]
    State(String),
}

/// Result type alias for TUI operations
pub type TuiResult<T> = Result<T, TuiError>;
