// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Error types for device management operations

use std::io;

/// Errors that can occur during device management operations
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Device not found: {0}")]
    DeviceNotFound(String),

    #[error("Device access denied: {0}")]
    AccessDenied(String),

    #[error("Mount operation failed: {0}")]
    Mount(String),

    #[error("Device configuration error: {0}")]
    Config(String),

    #[error("Cgroup delegation failed: {0}")]
    CgroupDelegation(String),
}

pub type Result<T> = std::result::Result<T, Error>;
