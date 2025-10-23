// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Error types for network operations.

use thiserror::Error;

/// Errors that can occur in network operations
#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Network setup failed: {0}")]
    Setup(String),

    #[error("Slirp4netns integration failed: {0}")]
    Slirp4netns(String),

    #[error("Veth setup failed: {0}")]
    Veth(String),

    #[error("Firewall configuration failed: {0}")]
    Firewall(String),
}
