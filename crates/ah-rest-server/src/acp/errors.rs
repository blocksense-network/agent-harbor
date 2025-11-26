// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Error types used by the ACP gateway skeleton.

use std::io;

/// Result alias for ACP gateway operations
pub type AcpResult<T> = Result<T, AcpError>;

/// ACP gateway errors (placeholder for richer taxonomy in later milestones)
#[derive(Debug, thiserror::Error)]
pub enum AcpError {
    /// Gateway was invoked while disabled in configuration
    #[error("ACP gateway is disabled in configuration")]
    Disabled,

    /// Transport-level failures (socket bind, accept, stdio pipes, etc.)
    #[error("ACP transport error: {0}")]
    Transport(#[from] io::Error),

    /// Internal errors that will later be mapped to ACP JSON-RPC errors
    #[error("ACP internal error: {0}")]
    Internal(String),
}
