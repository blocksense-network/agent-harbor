// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Error types for the credentials management system

use std::path::PathBuf;
use thiserror::Error;

/// Result type alias for credentials operations
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur during credentials management operations
#[derive(Debug, Error)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML parsing error: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("TOML serialization error: {0}")]
    TomlSerialize(#[from] toml::ser::Error),

    #[error("JSON parsing error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Account not found: {0}")]
    AccountNotFound(String),

    #[error("Account already exists: {0}")]
    AccountExists(String),

    #[error("Invalid account name: {0}")]
    InvalidAccountName(String),

    #[error("Invalid agent type: {0}")]
    InvalidAgentType(String),

    #[error("Duplicate alias: {0}")]
    DuplicateAlias(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(PathBuf),

    #[error("Directory not accessible: {0}")]
    DirectoryNotAccessible(PathBuf),

    #[error("File corruption detected: {0}")]
    FileCorruption(PathBuf),

    #[error("Encryption error: {0}")]
    Encryption(String),
}
