// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Database models and persistence operations.

use ah_domain_types::TaskStatus;
use rusqlite::params;
use serde::{Deserialize, Serialize};

/// Unique identifier for a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(pub u64);

/// Represents a task in the AH system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub name: String,
    pub description: String,
    pub status: TaskStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

/// Database model for repositories.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoRecord {
    pub id: i64,
    pub vcs: String,
    pub root_path: Option<String>,
    pub remote_url: Option<String>,
    pub default_branch: Option<String>,
    pub created_at: String,
}

/// Database model for workspaces.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceRecord {
    pub id: i64,
    pub name: String,
    pub external_id: Option<String>,
    pub created_at: String,
}

/// Database model for agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRecord {
    pub id: i64,
    pub name: String,
    pub version: String,
    pub metadata: Option<String>,
}

/// Database model for runtimes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeRecord {
    pub id: i64,
    pub type_: String, // `type` is a keyword in Rust
    pub devcontainer_path: Option<String>,
    pub metadata: Option<String>,
}

/// Database model for sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRecord {
    pub id: String,
    pub repo_id: Option<i64>,
    pub workspace_id: Option<i64>,
    pub agent_id: Option<i64>,
    pub runtime_id: Option<i64>,
    pub multiplexer_kind: Option<String>,
    pub mux_session: Option<String>,
    pub mux_window: Option<i64>,
    pub pane_left: Option<String>,
    pub pane_right: Option<String>,
    pub pid_agent: Option<i64>,
    pub status: String,
    pub log_path: Option<String>,
    pub workspace_path: Option<String>,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub agent_config: Option<String>,
    pub runtime_config: Option<String>,
}

/// Database model for tasks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    pub id: i64,
    pub session_id: String,
    pub prompt: String,
    pub repo_url: Option<String>,
    pub branch: Option<String>,
    pub commit: Option<String>,
    pub delivery: Option<String>,
    pub instances: Option<i64>,
    pub labels: Option<String>,
    pub browser_automation: i64,
    pub browser_profile: Option<String>,
    pub chatgpt_username: Option<String>,
    pub codex_workspace: Option<String>,
}

impl TaskRecord {
    /// Create from a local Task.
    pub fn from_task(task: &Task) -> Self {
        Self {
            id: task.id.0 as i64,
            session_id: "".to_string(), // Will be set when creating task record
            prompt: task.description.clone(),
            repo_url: None,
            branch: None,
            commit: None,
            delivery: None,
            instances: None,
            labels: None,
            browser_automation: 1, // Default to enabled
            browser_profile: None,
            chatgpt_username: None,
            codex_workspace: None,
        }
    }

    /// Convert to a local Task.
    pub fn to_task(&self) -> crate::Result<Task> {
        let metadata = std::collections::HashMap::new(); // Default empty metadata

        Ok(Task {
            id: TaskId(self.id as u64),
            name: format!("Task {}", self.id),
            description: self.prompt.clone(),
            status: TaskStatus::Pending, // Default status
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            metadata,
        })
    }
}

/// Database model for events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRecord {
    pub id: i64,
    pub session_id: String,
    pub ts: String,
    pub type_: String, // `type` is a keyword in Rust
    pub data: Option<String>,
}

/// Database model for filesystem snapshots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsSnapshotRecord {
    pub id: i64,
    pub session_id: String,
    pub ts: String,
    pub provider: String,
    pub ref_: Option<String>, // `ref` is a keyword in Rust
    pub path: Option<String>,
    pub parent_id: Option<i64>,
    pub metadata: Option<String>,
}

pub struct DraftRecord {
    pub id: String,
    pub description: String,
    pub repository: String,
    pub branch: Option<String>,
    pub models: String, // JSON string
    pub created_at: String,
    pub updated_at: String,
}

/// Database operations for repositories.
pub struct RepoStore<'a> {
    conn: &'a rusqlite::Connection,
}

impl<'a> RepoStore<'a> {
    pub fn new(conn: &'a rusqlite::Connection) -> Self {
        Self { conn }
    }

    pub fn insert(&self, record: &RepoRecord) -> crate::Result<i64> {
        self.conn.execute(
            r#"
            INSERT INTO repos (vcs, root_path, remote_url, default_branch, created_at)
            VALUES (?, ?, ?, ?, ?)
            "#,
            params![
                record.vcs,
                record.root_path,
                record.remote_url,
                record.default_branch,
                record.created_at
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_by_root_path(&self, root_path: &str) -> crate::Result<Option<RepoRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, vcs, root_path, remote_url, default_branch, created_at
            FROM repos WHERE root_path = ?
            "#,
        )?;

        let mut rows = stmt.query_map(params![root_path], |row| {
            Ok(RepoRecord {
                id: row.get(0)?,
                vcs: row.get(1)?,
                root_path: row.get(2)?,
                remote_url: row.get(3)?,
                default_branch: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?;

        match rows.next() {
            Some(Ok(record)) => Ok(Some(record)),
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
    }

    pub fn list(&self) -> crate::Result<Vec<RepoRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, vcs, root_path, remote_url, default_branch, created_at
            FROM repos ORDER BY created_at DESC
            "#,
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(RepoRecord {
                id: row.get(0)?,
                vcs: row.get(1)?,
                root_path: row.get(2)?,
                remote_url: row.get(3)?,
                default_branch: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?;

        let mut repos = Vec::new();
        for row in rows {
            repos.push(row?);
        }
        Ok(repos)
    }

    pub fn get_by_id(&self, id: i64) -> crate::Result<Option<RepoRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, vcs, root_path, remote_url, default_branch, created_at
            FROM repos WHERE id = ?
            "#,
        )?;

        let mut rows = stmt.query_map(params![id], |row| {
            Ok(RepoRecord {
                id: row.get(0)?,
                vcs: row.get(1)?,
                root_path: row.get(2)?,
                remote_url: row.get(3)?,
                default_branch: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?;

        match rows.next() {
            Some(Ok(record)) => Ok(Some(record)),
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
    }
}

/// Database operations for agents.
pub struct AgentStore<'a> {
    conn: &'a rusqlite::Connection,
}

impl<'a> AgentStore<'a> {
    pub fn new(conn: &'a rusqlite::Connection) -> Self {
        Self { conn }
    }

    pub fn insert(&self, record: &AgentRecord) -> crate::Result<i64> {
        self.conn.execute(
            r#"
            INSERT INTO agents (name, version, metadata)
            VALUES (?, ?, ?)
            "#,
            params![record.name, record.version, record.metadata],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_by_name_version(
        &self,
        name: &str,
        version: &str,
    ) -> crate::Result<Option<AgentRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, name, version, metadata
            FROM agents WHERE name = ? AND version = ?
            "#,
        )?;

        let mut rows = stmt.query_map(params![name, version], |row| {
            Ok(AgentRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                version: row.get(2)?,
                metadata: row.get(3)?,
            })
        })?;

        match rows.next() {
            Some(Ok(record)) => Ok(Some(record)),
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
    }
}

/// Database operations for runtimes.
pub struct RuntimeStore<'a> {
    conn: &'a rusqlite::Connection,
}

impl<'a> RuntimeStore<'a> {
    pub fn new(conn: &'a rusqlite::Connection) -> Self {
        Self { conn }
    }

    pub fn insert(&self, record: &RuntimeRecord) -> crate::Result<i64> {
        self.conn.execute(
            r#"
            INSERT INTO runtimes (type, devcontainer_path, metadata)
            VALUES (?, ?, ?)
            "#,
            params![record.type_, record.devcontainer_path, record.metadata],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_or_insert_local(&self) -> crate::Result<i64> {
        // Try to find existing local runtime
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id FROM runtimes WHERE type = 'local'
            "#,
        )?;

        let mut rows = stmt.query_map(params![], |row| row.get::<_, i64>(0))?;

        if let Some(Ok(id)) = rows.next() {
            return Ok(id);
        }

        // Insert new local runtime
        self.conn.execute(
            r#"
            INSERT INTO runtimes (type, devcontainer_path, metadata)
            VALUES ('local', NULL, NULL)
            "#,
            params![],
        )?;
        Ok(self.conn.last_insert_rowid())
    }
}

/// Database operations for sessions.
pub struct SessionStore<'a> {
    conn: &'a rusqlite::Connection,
}

impl<'a> SessionStore<'a> {
    pub fn new(conn: &'a rusqlite::Connection) -> Self {
        Self { conn }
    }

    pub fn insert(&self, record: &SessionRecord) -> crate::Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO sessions (id, repo_id, workspace_id, agent_id, runtime_id, multiplexer_kind, mux_session, mux_window, pane_left, pane_right, pid_agent, status, log_path, workspace_path, started_at, ended_at, agent_config, runtime_config)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            params![
                record.id,
                record.repo_id,
                record.workspace_id,
                record.agent_id,
                record.runtime_id,
                record.multiplexer_kind,
                record.mux_session,
                record.mux_window,
                record.pane_left,
                record.pane_right,
                record.pid_agent,
                record.status,
                record.log_path,
                record.workspace_path,
                record.started_at,
                record.ended_at,
                record.agent_config,
                record.runtime_config
            ],
        )?;
        Ok(())
    }

    pub fn get(&self, session_id: &str) -> crate::Result<Option<SessionRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, repo_id, workspace_id, agent_id, runtime_id, multiplexer_kind, mux_session, mux_window, pane_left, pane_right, pid_agent, status, log_path, workspace_path, started_at, ended_at, agent_config, runtime_config
            FROM sessions WHERE id = ?
            "#,
        )?;

        let mut rows = stmt.query_map(params![session_id], |row| {
            Ok(SessionRecord {
                id: row.get(0)?,
                repo_id: row.get(1)?,
                workspace_id: row.get(2)?,
                agent_id: row.get(3)?,
                runtime_id: row.get(4)?,
                multiplexer_kind: row.get(5)?,
                mux_session: row.get(6)?,
                mux_window: row.get(7)?,
                pane_left: row.get(8)?,
                pane_right: row.get(9)?,
                pid_agent: row.get(10)?,
                status: row.get(11)?,
                log_path: row.get(12)?,
                workspace_path: row.get(13)?,
                started_at: row.get(14)?,
                ended_at: row.get(15)?,
                agent_config: row.get(16)?,
                runtime_config: row.get(17)?,
            })
        })?;

        match rows.next() {
            Some(Ok(record)) => Ok(Some(record)),
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
    }

    pub fn update_status(
        &self,
        session_id: &str,
        status: &str,
        ended_at: Option<&str>,
    ) -> crate::Result<()> {
        self.conn.execute(
            r#"
            UPDATE sessions
            SET status = ?, ended_at = ?
            WHERE id = ?
            "#,
            params![status, ended_at, session_id],
        )?;
        Ok(())
    }

    pub fn update(&self, record: &SessionRecord) -> crate::Result<()> {
        self.conn.execute(
            r#"
            UPDATE sessions
            SET repo_id = ?, workspace_id = ?, agent_id = ?, runtime_id = ?, multiplexer_kind = ?, mux_session = ?, mux_window = ?, pane_left = ?, pane_right = ?, pid_agent = ?, status = ?, log_path = ?, workspace_path = ?, started_at = ?, ended_at = ?, agent_config = ?, runtime_config = ?
            WHERE id = ?
            "#,
            params![
                record.repo_id,
                record.workspace_id,
                record.agent_id,
                record.runtime_id,
                record.multiplexer_kind,
                record.mux_session,
                record.mux_window,
                record.pane_left,
                record.pane_right,
                record.pid_agent,
                record.status,
                record.log_path,
                record.workspace_path,
                record.started_at,
                record.ended_at,
                record.agent_config,
                record.runtime_config,
                record.id
            ],
        )?;
        Ok(())
    }

    pub fn delete(&self, session_id: &str) -> crate::Result<()> {
        self.conn.execute("DELETE FROM sessions WHERE id = ?", params![session_id])?;
        Ok(())
    }

    pub fn list(&self) -> crate::Result<Vec<SessionRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, repo_id, workspace_id, agent_id, runtime_id, multiplexer_kind, mux_session, mux_window, pane_left, pane_right, pid_agent, status, log_path, workspace_path, started_at, ended_at, agent_config, runtime_config
            FROM sessions
            ORDER BY started_at DESC
            "#,
        )?;

        let records = stmt.query_map([], |row| {
            Ok(SessionRecord {
                id: row.get(0)?,
                repo_id: row.get(1)?,
                workspace_id: row.get(2)?,
                agent_id: row.get(3)?,
                runtime_id: row.get(4)?,
                multiplexer_kind: row.get(5)?,
                mux_session: row.get(6)?,
                mux_window: row.get(7)?,
                pane_left: row.get(8)?,
                pane_right: row.get(9)?,
                pid_agent: row.get(10)?,
                status: row.get(11)?,
                log_path: row.get(12)?,
                workspace_path: row.get(13)?,
                started_at: row.get(14)?,
                ended_at: row.get(15)?,
                agent_config: row.get(16)?,
                runtime_config: row.get(17)?,
            })
        })?;

        let mut result = Vec::new();
        for record in records {
            result.push(record?);
        }
        Ok(result)
    }
}

/// Database operations for tasks.
pub struct TaskStore<'a> {
    conn: &'a rusqlite::Connection,
}

impl<'a> TaskStore<'a> {
    pub fn new(conn: &'a rusqlite::Connection) -> Self {
        Self { conn }
    }

    pub fn insert(&self, record: &TaskRecord) -> crate::Result<i64> {
        self.conn.execute(
            r#"
            INSERT INTO tasks (session_id, prompt, repo_url, branch, "commit", delivery, instances, labels, browser_automation, browser_profile, chatgpt_username, codex_workspace)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            params![
                record.session_id,
                record.prompt,
                record.repo_url,
                record.branch,
                record.commit,
                record.delivery,
                record.instances,
                record.labels,
                record.browser_automation,
                record.browser_profile,
                record.chatgpt_username,
                record.codex_workspace
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_by_session(&self, session_id: &str) -> crate::Result<Option<TaskRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, session_id, prompt, repo_url, branch, "commit", delivery, instances, labels, browser_automation, browser_profile, chatgpt_username, codex_workspace
            FROM tasks WHERE session_id = ?
            "#,
        )?;

        let mut rows = stmt.query_map(params![session_id], |row| {
            Ok(TaskRecord {
                id: row.get(0)?,
                session_id: row.get(1)?,
                prompt: row.get(2)?,
                repo_url: row.get(3)?,
                branch: row.get(4)?,
                commit: row.get(5)?,
                delivery: row.get(6)?,
                instances: row.get(7)?,
                labels: row.get(8)?,
                browser_automation: row.get(9)?,
                browser_profile: row.get(10)?,
                chatgpt_username: row.get(11)?,
                codex_workspace: row.get(12)?,
            })
        })?;

        match rows.next() {
            Some(Ok(record)) => Ok(Some(record)),
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
    }
}

/// Database operations for filesystem snapshots.
pub struct FsSnapshotStore<'a> {
    conn: &'a rusqlite::Connection,
}

impl<'a> FsSnapshotStore<'a> {
    pub fn new(conn: &'a rusqlite::Connection) -> Self {
        Self { conn }
    }

    pub fn insert(&self, record: &FsSnapshotRecord) -> crate::Result<i64> {
        self.conn.execute(
            r#"
            INSERT INTO fs_snapshots (session_id, ts, provider, ref, path, parent_id, metadata)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
            params![
                record.session_id,
                record.ts,
                record.provider,
                record.ref_,
                record.path,
                record.parent_id,
                record.metadata
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list_by_session(&self, session_id: &str) -> crate::Result<Vec<FsSnapshotRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, session_id, ts, provider, ref, path, parent_id, metadata
            FROM fs_snapshots
            WHERE session_id = ?
            ORDER BY ts ASC
            "#,
        )?;

        let records = stmt.query_map(params![session_id], |row| {
            Ok(FsSnapshotRecord {
                id: row.get(0)?,
                session_id: row.get(1)?,
                ts: row.get(2)?,
                provider: row.get(3)?,
                ref_: row.get(4)?,
                path: row.get(5)?,
                parent_id: row.get(6)?,
                metadata: row.get(7)?,
            })
        })?;

        let mut snapshots = Vec::new();
        for record in records {
            snapshots.push(record?);
        }
        Ok(snapshots)
    }
}

/// Database operations for key-value store.
pub struct KvStore<'a> {
    conn: &'a rusqlite::Connection,
}

impl<'a> KvStore<'a> {
    pub fn new(conn: &'a rusqlite::Connection) -> Self {
        Self { conn }
    }

    pub fn set(&self, scope: &str, key: &str, value: Option<&str>) -> crate::Result<()> {
        self.conn.execute(
            r#"
            INSERT OR REPLACE INTO kv (scope, k, v)
            VALUES (?, ?, ?)
            "#,
            params![scope, key, value],
        )?;
        Ok(())
    }

    pub fn get(&self, scope: &str, key: &str) -> crate::Result<Option<String>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT v FROM kv WHERE scope = ? AND k = ?
            "#,
        )?;

        let mut rows =
            stmt.query_map(params![scope, key], |row| row.get::<_, Option<String>>(0))?;

        match rows.next() {
            Some(Ok(Some(value))) => Ok(Some(value)),
            Some(Ok(None)) => Ok(None),
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
    }
}

/// Database operations for drafts.
pub struct DraftStore<'a> {
    conn: &'a rusqlite::Connection,
}

impl<'a> DraftStore<'a> {
    pub fn new(conn: &'a rusqlite::Connection) -> Self {
        Self { conn }
    }

    pub fn insert(&self, record: &DraftRecord) -> crate::Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO drafts (id, description, repository, branch, models, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
            params![
                record.id,
                record.description,
                record.repository,
                record.branch,
                record.models,
                record.created_at,
                record.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn get_by_id(&self, id: &str) -> crate::Result<Option<DraftRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, description, repository, branch, models, created_at, updated_at
            FROM drafts WHERE id = ?
            "#,
        )?;

        let mut rows = stmt.query_map(params![id], |row| {
            Ok(DraftRecord {
                id: row.get(0)?,
                description: row.get(1)?,
                repository: row.get(2)?,
                branch: row.get(3)?,
                models: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })?;

        match rows.next() {
            Some(Ok(record)) => Ok(Some(record)),
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
    }

    pub fn list(&self) -> crate::Result<Vec<DraftRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, description, repository, branch, models, created_at, updated_at
            FROM drafts ORDER BY updated_at DESC
            "#,
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(DraftRecord {
                id: row.get(0)?,
                description: row.get(1)?,
                repository: row.get(2)?,
                branch: row.get(3)?,
                models: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })?;

        let mut drafts = Vec::new();
        for row in rows {
            drafts.push(row?);
        }
        Ok(drafts)
    }

    pub fn update(&self, record: &DraftRecord) -> crate::Result<()> {
        self.conn.execute(
            r#"
            UPDATE drafts SET description = ?, repository = ?, branch = ?, models = ?, updated_at = ?
            WHERE id = ?
            "#,
            params![
                record.description,
                record.repository,
                record.branch,
                record.models,
                record.updated_at,
                record.id
            ],
        )?;
        Ok(())
    }

    pub fn delete(&self, id: &str) -> crate::Result<()> {
        self.conn.execute("DELETE FROM drafts WHERE id = ?", params![id])?;
        Ok(())
    }
}
