// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Database migration management.

use rusqlite::{Connection, params};

/// Database migration manager.
pub struct MigrationManager;

impl MigrationManager {
    /// Apply all pending migrations to the database.
    pub fn migrate(conn: &Connection) -> crate::Result<()> {
        // Create schema migrations table first
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );
            "#,
        )?;

        // Get current version
        let current_version = Self::current_version(conn)?.unwrap_or(0);

        // Apply migrations sequentially
        if current_version < 1 {
            Self::apply_migration_1(conn)?;
        }
        if current_version < 2 {
            Self::apply_migration_2(conn)?;
        }
        if current_version < 3 {
            Self::apply_migration_3(conn)?;
        }
        if current_version < 4 {
            Self::apply_migration_4(conn)?;
        }

        Ok(())
    }

    /// Apply migration version 1 - complete State-Persistence.md schema
    fn apply_migration_1(conn: &Connection) -> crate::Result<()> {
        conn.execute_batch(
            r#"
            -- Repositories known to the system (local path and/or remote URL)
            CREATE TABLE IF NOT EXISTS repos (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                vcs TEXT NOT NULL,
                root_path TEXT,
                remote_url TEXT,
                default_branch TEXT,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
                UNIQUE(root_path),
                UNIQUE(remote_url)
            );

            -- Workspaces are named logical groupings on some servers. Optional locally.
            CREATE TABLE IF NOT EXISTS workspaces (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                external_id TEXT,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
                UNIQUE(name)
            );

            -- Agents catalog (type + version descriptor)
            CREATE TABLE IF NOT EXISTS agents (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                version TEXT NOT NULL,
                metadata TEXT,
                UNIQUE(name, version)
            );

            -- Runtime definitions (devcontainer, local, disabled, etc.)
            CREATE TABLE IF NOT EXISTS runtimes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                type TEXT NOT NULL,
                devcontainer_path TEXT,
                metadata TEXT
            );

            -- Sessions are concrete agent runs bound to a repo (and optionally a workspace)
            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                repo_id INTEGER REFERENCES repos(id),
                workspace_id INTEGER REFERENCES workspaces(id),
                agent_id INTEGER REFERENCES agents(id),
                runtime_id INTEGER REFERENCES runtimes(id),
                multiplexer_kind TEXT,
                mux_session TEXT,
                mux_window INTEGER,
                pane_left TEXT,
                pane_right TEXT,
                pid_agent INTEGER,
                status TEXT NOT NULL,
                log_path TEXT,
                workspace_path TEXT,
                started_at TEXT NOT NULL,
                ended_at TEXT
            );

            -- Tasks capture user intent and parameters used to launch a session
            CREATE TABLE IF NOT EXISTS tasks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                prompt TEXT NOT NULL,
                branch TEXT,
                delivery TEXT,
                instances INTEGER DEFAULT 1,
                labels TEXT,
                browser_automation INTEGER NOT NULL DEFAULT 1,
                browser_profile TEXT,
                chatgpt_username TEXT,
                codex_workspace TEXT
            );

            -- Event log per session for diagnostics and incremental state
            CREATE TABLE IF NOT EXISTS events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                ts TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
                type TEXT NOT NULL,
                data TEXT
            );

            -- Filesystem snapshots associated with a session (see docs/fs-snapshots)
            CREATE TABLE IF NOT EXISTS fs_snapshots (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                ts TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
                provider TEXT NOT NULL,
                ref TEXT,
                path TEXT,
                parent_id INTEGER REFERENCES fs_snapshots(id) ON DELETE SET NULL,
                metadata TEXT
            );

            -- Key/value subsystem for small, fast lookups (scoped configuration, caches)
            CREATE TABLE IF NOT EXISTS kv (
                scope TEXT NOT NULL,
                k TEXT NOT NULL,
                v TEXT,
                PRIMARY KEY (scope, k)
            );

            -- Indexes for performance
            CREATE INDEX IF NOT EXISTS idx_events_session_ts ON events(session_id, ts);
            CREATE INDEX IF NOT EXISTS idx_fs_snapshots_session_ts ON fs_snapshots(session_id, ts);

            -- Mark migration as applied
            INSERT OR REPLACE INTO schema_migrations (version) VALUES (1);
            "#,
        )?;

        Ok(())
    }

    /// Apply migration version 2 - add agent and runtime config fields to sessions
    fn apply_migration_2(conn: &Connection) -> crate::Result<()> {
        conn.execute_batch(
            r#"
            ALTER TABLE sessions ADD COLUMN agent_config TEXT;
            ALTER TABLE sessions ADD COLUMN runtime_config TEXT;
            "#,
        )?;

        // Record the migration
        conn.execute("INSERT INTO schema_migrations (version) VALUES (2)", [])?;

        Ok(())
    }

    /// Apply migration version 3 - add repo_url and commit fields to tasks
    fn apply_migration_3(conn: &Connection) -> crate::Result<()> {
        conn.execute_batch(
            r#"
            ALTER TABLE tasks ADD COLUMN repo_url TEXT;
            ALTER TABLE tasks ADD COLUMN "commit" TEXT;
            "#,
        )?;

        // Record the migration
        conn.execute("INSERT INTO schema_migrations (version) VALUES (3)", [])?;

        Ok(())
    }

    /// Apply migration version 4 - add drafts table
    fn apply_migration_4(conn: &Connection) -> crate::Result<()> {
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS drafts (
                id TEXT PRIMARY KEY,
                description TEXT NOT NULL,
                repository TEXT NOT NULL,
                branch TEXT,
                models TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
            );
            "#,
        )?;

        // Record the migration
        conn.execute("INSERT INTO schema_migrations (version) VALUES (4)", [])?;

        Ok(())
    }

    /// Get the current schema version.
    pub fn current_version(conn: &Connection) -> crate::Result<Option<u32>> {
        let mut stmt = conn.prepare("SELECT MAX(version) FROM schema_migrations")?;

        let version: Option<u32> = stmt.query_row(params![], |row| row.get(0)).ok();

        Ok(version)
    }
}
