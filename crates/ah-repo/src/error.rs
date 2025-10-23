// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use thiserror::Error;

#[derive(Debug, Error)]
pub enum VcsError {
    #[error("Repository not found from path: {0}")]
    RepositoryNotFound(String),

    #[error("VCS type not found for repository at: {0}")]
    VcsTypeNotFound(String),

    #[error("Invalid branch name: {0}")]
    InvalidBranchName(String),

    #[error("Branch '{0}' is a protected branch")]
    ProtectedBranch(String),

    #[error("Branch '{0}' already exists")]
    BranchExists(String),

    #[error("Branch '{0}' does not exist")]
    BranchNotFound(String),

    #[error("Command execution failed: {command} (exit code: {exit_code})")]
    CommandFailed {
        command: String,
        exit_code: i32,
        stderr: String,
    },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("UTF-8 encoding error: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),

    #[error("Regex error: {0}")]
    Regex(#[from] regex::Error),

    #[error("Unknown VCS error: {0}")]
    Other(String),
}

pub type VcsResult<T> = Result<T, VcsError>;
