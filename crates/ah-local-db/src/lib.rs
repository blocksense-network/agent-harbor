// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! SQLite database management for local Agent Harbor state.
//!
//! This crate provides persistent storage for tasks, sessions, agent recordings,
//! and other local state using SQLite as the backing database.

pub mod connection;
pub mod migrations;
pub mod models;
pub mod schema;

/// Result type for database operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Error type for database operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Migration error: {message}")]
    Migration { message: String },

    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Generic database error: {0}")]
    Generic(String),
}

impl Error {
    /// Create a new migration error.
    pub fn migration<S: Into<String>>(message: S) -> Self {
        Self::Migration {
            message: message.into(),
        }
    }

    /// Create a new generic database error.
    pub fn generic<S: Into<String>>(message: S) -> Self {
        Self::Generic(message.into())
    }
}

/// Database connection and management.
pub use connection::Database;

/// Database models and operations.
pub use models::{
    AgentRecord, AgentStore, FsSnapshotRecord, FsSnapshotStore, KvStore, RepoRecord, RepoStore,
    RuntimeRecord, RuntimeStore, SessionRecord, SessionStore, TaskRecord, TaskStore,
};

/// Schema definitions and constants.
pub use schema::*;
