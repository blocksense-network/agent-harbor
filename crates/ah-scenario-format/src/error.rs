// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use thiserror::Error;

/// Convenient result alias for scenario operations.
pub type Result<T> = std::result::Result<T, ScenarioError>;

/// Errors that can occur while loading or playing back scenarios.
#[derive(Debug, Error)]
pub enum ScenarioError {
    /// Underlying IO error while accessing scenario files.
    #[error("Scenario IO error: {0}")]
    Io(#[from] std::io::Error),

    /// YAML parsing error.
    #[error("Scenario parse error: {0}")]
    Parse(#[from] serde_yaml::Error),

    /// Scenario file was missing required fields.
    #[error("Scenario validation error: {0}")]
    Validation(String),

    /// No scenarios were loaded.
    #[error("No scenarios loaded")]
    Empty,

    /// Playback encountered an unsupported construct.
    #[error("Scenario playback error: {0}")]
    Playback(String),
}
