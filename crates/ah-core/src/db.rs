// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Database integration for task and session persistence.

use ah_local_db::models::{DraftRecord, DraftStore};
use ah_local_db::{
    AgentRecord, AgentStore, Database, FsSnapshotRecord, FsSnapshotStore, RepoRecord, RepoStore,
    RuntimeStore, SessionRecord, SessionStore, TaskRecord, TaskStore,
};
use ah_repo::VcsRepo;
use std::path::Path;

/// Database manager for AH core operations.
pub struct DatabaseManager {
    db: Database,
}

impl DatabaseManager {
    /// Create a new database manager with default database path.
    pub fn new() -> crate::Result<Self> {
        let db = Database::open_default()?;
        Ok(Self { db })
    }

    /// Create a new database manager with custom database path.
    pub fn with_path<P: AsRef<Path>>(path: P) -> crate::Result<Self> {
        let db = Database::open(path)?;
        Ok(Self { db })
    }

    /// Get or create repository record.
    pub fn get_or_create_repo(&self, repo: &VcsRepo) -> crate::Result<i64> {
        let root_path = repo.root().to_string_lossy().to_string();
        let default_branch = repo.default_branch();

        let mut conn = self.db.connection().lock().map_err(|e| {
            crate::Error::generic(format!("Failed to acquire database lock: {}", e))
        })?;

        let repo_store = RepoStore::new(&conn);

        // Try to find existing repo
        if let Some(existing) = repo_store.get_by_root_path(&root_path)? {
            return Ok(existing.id);
        }

        // Create new repo record
        let record = RepoRecord {
            id: 0, // Will be set by autoincrement
            vcs: repo.vcs_type().to_string(),
            root_path: Some(root_path),
            remote_url: repo.default_remote_http_url().ok().flatten(),
            default_branch: Some(default_branch.to_string()),
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        Ok(repo_store.insert(&record)?)
    }

    /// List all repositories.
    pub fn list_repositories(&self) -> crate::Result<Vec<RepoRecord>> {
        let mut conn = self.db.connection().lock().map_err(|e| {
            crate::Error::generic(format!("Failed to acquire database lock: {}", e))
        })?;

        let repo_store = RepoStore::new(&conn);
        repo_store
            .list()
            .map_err(|e| crate::Error::generic(format!("Failed to list repositories: {}", e)))
    }

    /// Get repository by ID.
    pub fn get_repository_by_id(&self, id: i64) -> crate::Result<Option<RepoRecord>> {
        let mut conn = self.db.connection().lock().map_err(|e| {
            crate::Error::generic(format!("Failed to acquire database lock: {}", e))
        })?;

        let repo_store = RepoStore::new(&conn);
        repo_store
            .get_by_id(id)
            .map_err(|e| crate::Error::generic(format!("Failed to get repository by ID: {}", e)))
    }

    /// List all draft tasks.
    pub fn list_drafts(&self) -> crate::Result<Vec<DraftRecord>> {
        let mut conn = self.db.connection().lock().map_err(|e| {
            crate::Error::generic(format!("Failed to acquire database lock: {}", e))
        })?;

        let draft_store = DraftStore::new(&conn);
        draft_store
            .list()
            .map_err(|e| crate::Error::generic(format!("Failed to list drafts: {}", e)))
    }

    /// Get draft by ID.
    pub fn get_draft_by_id(&self, id: &str) -> crate::Result<Option<DraftRecord>> {
        let mut conn = self.db.connection().lock().map_err(|e| {
            crate::Error::generic(format!("Failed to acquire database lock: {}", e))
        })?;

        let draft_store = DraftStore::new(&conn);
        draft_store
            .get_by_id(id)
            .map_err(|e| crate::Error::generic(format!("Failed to get draft by ID: {}", e)))
    }

    /// Save a draft task (insert or update).
    pub fn save_draft(&self, record: &DraftRecord) -> crate::Result<()> {
        let mut conn = self.db.connection().lock().map_err(|e| {
            crate::Error::generic(format!("Failed to acquire database lock: {}", e))
        })?;

        let draft_store = DraftStore::new(&conn);

        // Check if draft already exists
        if draft_store.get_by_id(&record.id)?.is_some() {
            // Update existing draft
            draft_store
                .update(record)
                .map_err(|e| crate::Error::generic(format!("Failed to update draft: {}", e)))
        } else {
            // Insert new draft
            draft_store
                .insert(record)
                .map_err(|e| crate::Error::generic(format!("Failed to insert draft: {}", e)))
        }
    }

    /// Delete a draft task.
    pub fn delete_draft(&self, id: &str) -> crate::Result<()> {
        let mut conn = self.db.connection().lock().map_err(|e| {
            crate::Error::generic(format!("Failed to acquire database lock: {}", e))
        })?;

        let draft_store = DraftStore::new(&conn);
        draft_store
            .delete(id)
            .map_err(|e| crate::Error::generic(format!("Failed to delete draft: {}", e)))
    }

    /// Get or create agent record.
    pub fn get_or_create_agent(&self, name: &str, version: &str) -> crate::Result<i64> {
        let mut conn = self.db.connection().lock().map_err(|e| {
            crate::Error::generic(format!("Failed to acquire database lock: {}", e))
        })?;

        let agent_store = AgentStore::new(&conn);

        // Try to find existing agent
        if let Some(existing) = agent_store.get_by_name_version(name, version)? {
            return Ok(existing.id);
        }

        // Create new agent record
        let record = AgentRecord {
            id: 0, // Will be set by autoincrement
            name: name.to_string(),
            version: version.to_string(),
            metadata: None,
        };

        Ok(agent_store.insert(&record)?)
    }

    /// Get or create runtime record (defaults to local).
    pub fn get_or_create_local_runtime(&self) -> crate::Result<i64> {
        let mut conn = self.db.connection().lock().map_err(|e| {
            crate::Error::generic(format!("Failed to acquire database lock: {}", e))
        })?;

        let runtime_store = RuntimeStore::new(&conn);
        Ok(runtime_store.get_or_insert_local()?)
    }

    /// Create a new session record.
    pub fn create_session(&self, session_record: &SessionRecord) -> crate::Result<()> {
        let mut conn = self.db.connection().lock().map_err(|e| {
            crate::Error::generic(format!("Failed to acquire database lock: {}", e))
        })?;

        let session_store = SessionStore::new(&conn);
        Ok(session_store.insert(session_record)?)
    }

    /// Create a new task record.
    pub fn create_task_record(&self, task_record: &TaskRecord) -> crate::Result<i64> {
        let mut conn = self.db.connection().lock().map_err(|e| {
            crate::Error::generic(format!("Failed to acquire database lock: {}", e))
        })?;

        let task_store = TaskStore::new(&conn);
        Ok(task_store.insert(task_record)?)
    }

    /// Create a filesystem snapshot record.
    pub fn create_fs_snapshot(&self, snapshot_record: &FsSnapshotRecord) -> crate::Result<i64> {
        let mut conn = self.db.connection().lock().map_err(|e| {
            crate::Error::generic(format!("Failed to acquire database lock: {}", e))
        })?;

        let snapshot_store = FsSnapshotStore::new(&conn);
        Ok(snapshot_store.insert(snapshot_record)?)
    }

    /// Update session status.
    pub fn update_session_status(
        &self,
        session_id: &str,
        status: &str,
        ended_at: Option<&str>,
    ) -> crate::Result<()> {
        let mut conn = self.db.connection().lock().map_err(|e| {
            crate::Error::generic(format!("Failed to acquire database lock: {}", e))
        })?;

        let session_store = SessionStore::new(&conn);
        Ok(session_store.update_status(session_id, status, ended_at)?)
    }

    /// Generate a new ULID-style session ID.
    pub fn generate_session_id() -> String {
        uuid::Uuid::new_v4().to_string()
    }

    /// Get access to the underlying database for advanced operations.
    pub fn database(&self) -> &Database {
        &self.db
    }
}

impl Default for DatabaseManager {
    fn default() -> Self {
        Self::new().expect("Failed to create default database manager")
    }
}
