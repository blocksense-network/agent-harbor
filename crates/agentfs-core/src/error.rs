// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Error types for AgentFS Core

use std::io;

/// Core filesystem error type
#[derive(thiserror::Error, Debug)]
pub enum FsError {
    #[error("not found")]
    NotFound,
    #[error("already exists")]
    AlreadyExists,
    #[error("access denied")]
    AccessDenied,
    #[error("invalid argument")]
    InvalidArgument,
    #[error("name not allowed")]
    InvalidName,
    #[error("not a directory")]
    NotADirectory,
    #[error("is a directory")]
    IsADirectory,
    #[error("busy")]
    Busy,
    #[error("too many open files")]
    TooManyOpenFiles,
    #[error("bad file descriptor")]
    BadFileDescriptor,
    #[error("no space left")]
    NoSpace,
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("unsupported")]
    Unsupported,
    #[error("not implemented")]
    NotImplemented,
}

pub type FsResult<T> = Result<T, FsError>;
