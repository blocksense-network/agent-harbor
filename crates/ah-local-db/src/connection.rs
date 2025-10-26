// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Database connection management.

use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Database connection wrapper with connection pooling support.
#[derive(Debug, Clone)]
pub struct Database {
    connection: Arc<std::sync::Mutex<Connection>>,
}

impl Database {
    /// Get the default database path based on AH_HOME environment variable or platform defaults.
    ///
    /// Priority order (per State-Persistence.md):
    /// 1. AH_HOME environment variable (custom)
    /// 2. Platform-specific defaults:
    ///    - Linux: `${XDG_STATE_HOME:-~/.local/state}/agent-harbor/state.db`
    ///    - macOS: `~/Library/Application Support/agent-harbor/state.db`
    ///    - Windows: `%LOCALAPPDATA%\agent-harbor\state.db`
    pub fn default_path() -> crate::Result<PathBuf> {
        // Check AH_HOME first
        if let Ok(ah_home) = std::env::var("AH_HOME") {
            return Ok(PathBuf::from(ah_home).join("state.db"));
        }

        // Platform-specific defaults
        #[cfg(target_os = "linux")]
        {
            let xdg_state_home =
                std::env::var("XDG_STATE_HOME").map(PathBuf::from).unwrap_or_else(|_| {
                    let home = std::env::var("HOME").expect("HOME environment variable not set");
                    PathBuf::from(home).join(".local").join("state")
                });
            Ok(xdg_state_home.join("agent-harbor").join("state.db"))
        }

        #[cfg(target_os = "macos")]
        {
            let home = std::env::var("HOME").expect("HOME environment variable not set");
            Ok(PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("agent-harbor")
                .join("state.db"))
        }

        #[cfg(target_os = "windows")]
        {
            let local_appdata =
                std::env::var("LOCALAPPDATA").expect("LOCALAPPDATA environment variable not set");
            Ok(PathBuf::from(local_appdata).join("agent-harbor").join("state.db"))
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            // Fallback for other platforms
            let home = std::env::var("HOME").expect("HOME environment variable not set");
            Ok(PathBuf::from(home).join(".agent-harbor").join("state.db"))
        }
    }

    /// Open the database at the default path.
    pub fn open_default() -> crate::Result<Self> {
        let path = Self::default_path()?;
        Self::open(&path)
    }
}

impl Database {
    /// Open a new database connection at the specified path.
    ///
    /// If the path doesn't exist, the database will be created.
    pub fn open<P: AsRef<Path>>(path: P) -> crate::Result<Self> {
        let path = path.as_ref();

        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                crate::Error::generic(format!(
                    "Failed to create database directory {}: {}",
                    parent.display(),
                    e
                ))
            })?;
        }

        let conn = Connection::open(path)?;
        Self::initialize_schema(&conn)?;
        Ok(Self {
            connection: Arc::new(std::sync::Mutex::new(conn)),
        })
    }

    /// Open an in-memory database for testing.
    pub fn open_in_memory() -> crate::Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::initialize_schema(&conn)?;
        Ok(Self {
            connection: Arc::new(std::sync::Mutex::new(conn)),
        })
    }

    /// Initialize the database schema.
    fn initialize_schema(conn: &Connection) -> crate::Result<()> {
        // Enable WAL mode for better concurrency
        conn.pragma_update(None, "journal_mode", "WAL")?;

        // Apply migrations to bring schema up to date
        crate::migrations::MigrationManager::migrate(conn)?;

        Ok(())
    }

    /// Get a reference to the underlying connection.
    ///
    /// This method provides access to the connection for executing queries.
    /// The caller must ensure proper locking if used concurrently.
    pub fn connection(&self) -> &std::sync::Mutex<Connection> {
        &self.connection
    }

    /// Execute a transaction with automatic rollback on error.
    pub fn transaction<F, T>(&self, f: F) -> crate::Result<T>
    where
        F: FnOnce(&Connection) -> crate::Result<T>,
    {
        let conn = self.connection.lock().map_err(|e| {
            crate::Error::generic(format!("Failed to acquire database lock: {}", e))
        })?;

        let tx = conn.unchecked_transaction()?;
        match f(&tx) {
            Ok(result) => {
                tx.commit()?;
                Ok(result)
            }
            Err(e) => {
                tx.rollback()?;
                Err(e)
            }
        }
    }
}
