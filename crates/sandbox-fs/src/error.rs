// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Error types for filesystem operations.

use thiserror::Error;

/// Errors that can occur in filesystem operations
#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Mount operation failed: {0}")]
    Mount(String),

    #[error("Path resolution failed: {0}")]
    PathResolution(String),

    #[error("Overlay setup failed: {0}")]
    Overlay(String),

    #[error("Permission denied: {0}")]
    Permission(String),
}
