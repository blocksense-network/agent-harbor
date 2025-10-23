// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Error types for seccomp operations.

use thiserror::Error;

/// Errors that can occur in seccomp operations
#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Seccomp filter installation failed: {0}")]
    FilterInstall(String),

    #[error("Notification handling failed: {0}")]
    Notification(String),

    #[error("Path resolution failed: {0}")]
    PathResolution(String),

    #[error("Policy violation: {0}")]
    PolicyViolation(String),
}
