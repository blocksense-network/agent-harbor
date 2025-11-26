// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Task execution engine for running agent tasks
//!
//! This module implements the task lifecycle state machine:
//! queued → provisioning → running → completed/failed

use crate::models::{DatabaseSessionStore, SessionStore};
use ah_core::{AgentExecutionConfig, AgentExecutor};
use ah_local_db::Database;
use ah_rest_api_contract::{Session, SessionEvent, SessionStatus};
use anyhow::Result;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tokio::time::{Duration, sleep};
use tracing::{debug, error, info};
use uuid;

/// Global snapshot cache for workspace snapshots
#[derive(Debug)]
struct SnapshotCache {
    /// Base directory for all snapshots
    #[allow(dead_code)]
    base_dir: PathBuf,
    /// Maximum disk space to use for snapshots (in bytes)
    #[allow(dead_code)]
    max_size: u64,
    /// Current size of all snapshots (in bytes)
    #[allow(dead_code)]
    current_size: u64,
    /// LRU tracking: (repo_url, commit) -> (snapshot_path, last_used, size)
    entries: HashMap<(String, String), (PathBuf, DateTime<Utc>, u64)>,
}

impl SnapshotCache {
    fn new(base_dir: PathBuf, max_size: u64) -> Self {
        Self {
            base_dir,
            max_size,
            current_size: 0,
            entries: HashMap::new(),
        }
    }

    /// Get snapshot path for a repository and commit, if it exists
    fn get_snapshot(&self, repo_url: &str, commit: &str) -> Option<PathBuf> {
        self.entries
            .get(&(repo_url.to_string(), commit.to_string()))
            .map(|(path, _, _)| path.clone())
    }

    /// Add or update a snapshot in the cache
    #[allow(dead_code)]
    fn add_snapshot(
        &mut self,
        repo_url: String,
        commit: String,
        snapshot_path: PathBuf,
        size: u64,
    ) {
        let key = (repo_url, commit);
        let now = Utc::now();

        // Remove old entry if it exists
        if let Some((_, _, old_size)) = self.entries.remove(&key) {
            self.current_size = self.current_size.saturating_sub(old_size);
        }

        // Add new entry
        self.entries.insert(key, (snapshot_path, now, size));
        self.current_size += size;

        // Evict old entries if over limit
        self.evict_if_needed();
    }

    /// Evict least recently used snapshots if over size limit
    #[allow(dead_code)]
    fn evict_if_needed(&mut self) {
        while self.current_size > self.max_size && !self.entries.is_empty() {
            // Find the oldest entry
            let oldest_key = self
                .entries
                .iter()
                .min_by_key(|(_, (_, time, _))| *time)
                .map(|(key, _)| key.clone());

            if let Some(key) = oldest_key {
                if let Some((path, _, size)) = self.entries.remove(&key) {
                    self.current_size = self.current_size.saturating_sub(size);
                    // TODO: Actually remove the snapshot directory
                    info!("Evicted snapshot: {:?}", path);
                }
            }
        }
    }
}

/// Task executor for managing the lifecycle of agent tasks
pub struct TaskExecutor {
    db: Arc<Database>,
    session_store: Arc<DatabaseSessionStore>,
    running_tasks: Arc<RwLock<HashMap<String, tokio::task::JoinHandle<()>>>>,
    max_concurrent_tasks: usize,
    snapshot_cache: Arc<RwLock<SnapshotCache>>,
    provisioning_lock: Arc<Mutex<()>>,
    agent_executor: Arc<AgentExecutor>,
}

impl TaskExecutor {
    /// Create a new task executor
    pub fn new(
        db: Arc<Database>,
        session_store: Arc<DatabaseSessionStore>,
        config_file: Option<String>,
    ) -> Self {
        // Create snapshot cache
        let snapshot_base_dir = std::env::temp_dir().join("ah-snapshots");
        let snapshot_cache = Arc::new(RwLock::new(SnapshotCache::new(
            snapshot_base_dir,
            10 * 1024 * 1024 * 1024, // 10GB default limit
        )));

        // Create agent executor configuration
        let config = AgentExecutionConfig {
            config_file,
            recording_disabled: false, // TODO: Check fs-snapshots setting from request or global config
        };

        let agent_executor = Arc::new(AgentExecutor::new(config));

        Self {
            db,
            session_store,
            running_tasks: Arc::new(RwLock::new(HashMap::new())),
            max_concurrent_tasks: 5, // Default max concurrent tasks
            snapshot_cache,
            provisioning_lock: Arc::new(Mutex::new(())),
            agent_executor,
        }
    }

    /// Pause a running task by updating its session status to Paused.
    pub async fn pause_task(&self, session_id: &str) -> anyhow::Result<()> {
        if let Some(mut session) = self.session_store.get_session(session_id).await? {
            session.session.status = SessionStatus::Paused;
            self.session_store.update_session(session_id, &session).await?;
            // Record a status event when possible
            let _ = self
                .session_store
                .add_session_event(
                    session_id,
                    SessionEvent::status(
                        SessionStatus::Paused,
                        chrono::Utc::now().timestamp_millis() as u64,
                    ),
                )
                .await;
        }
        Ok(())
    }

    /// Resume a paused task by updating its status to Running.
    pub async fn resume_task(&self, session_id: &str) -> anyhow::Result<()> {
        if let Some(mut session) = self.session_store.get_session(session_id).await? {
            session.session.status = SessionStatus::Running;
            self.session_store.update_session(session_id, &session).await?;
            let _ = self
                .session_store
                .add_session_event(
                    session_id,
                    SessionEvent::status(
                        SessionStatus::Running,
                        chrono::Utc::now().timestamp_millis() as u64,
                    ),
                )
                .await;
        }
        Ok(())
    }

    /// Inject a user/system message into the running session by recording it as a log event.
    /// TODO: forward to live agent runtime once available.
    pub async fn inject_message(&self, session_id: &str, message: &str) -> anyhow::Result<()> {
        let ts = chrono::Utc::now().timestamp_millis() as u64;
        let _ = self
            .session_store
            .add_session_event(
                session_id,
                SessionEvent::log(
                    ah_rest_api_contract::SessionLogLevel::Info,
                    format!("user: {}", message),
                    None,
                    ts,
                ),
            )
            .await;
        Ok(())
    }

    /// Start the task executor
    ///
    /// This begins the background task processing loop
    pub fn start(&self) {
        let executor = Arc::new(Self {
            db: Arc::clone(&self.db),
            session_store: Arc::clone(&self.session_store),
            running_tasks: Arc::clone(&self.running_tasks),
            max_concurrent_tasks: self.max_concurrent_tasks,
            snapshot_cache: Arc::clone(&self.snapshot_cache),
            provisioning_lock: Arc::clone(&self.provisioning_lock),
            agent_executor: Arc::clone(&self.agent_executor),
        });

        tokio::spawn(async move {
            executor.run().await;
        });
    }

    /// Main execution loop
    async fn run(&self) {
        info!("Task executor started");

        loop {
            // Check for queued tasks and start them
            if let Err(e) = self.process_queued_tasks().await {
                error!("Error processing queued tasks: {}", e);
            }

            // Clean up completed tasks
            if let Err(e) = self.cleanup_completed_tasks().await {
                error!("Error cleaning up completed tasks: {}", e);
            }

            // Sleep before next iteration
            sleep(Duration::from_secs(1)).await;
        }
    }

    /// Process queued tasks by transitioning them to provisioning and starting execution
    async fn process_queued_tasks(&self) -> Result<()> {
        let running_count = self.running_tasks.read().await.len();

        if running_count >= self.max_concurrent_tasks {
            debug!(
                "Max concurrent tasks ({}) reached, skipping queued task processing",
                self.max_concurrent_tasks
            );
            return Ok(());
        }

        // Find queued sessions
        let filter = ah_rest_api_contract::FilterQuery {
            status: None,
            agent: None,
            project_id: None,
            tenant_id: None,
        };
        let sessions = self.session_store.list_sessions(&filter).await?;
        let queued_sessions: Vec<_> = sessions
            .into_iter()
            .filter(|s| s.status == SessionStatus::Queued)
            .take(self.max_concurrent_tasks - running_count)
            .collect();

        for session in queued_sessions {
            if let Err(e) = self.start_task(&session.id).await {
                error!("Failed to start task {}: {}", session.id, e);
                // Mark as failed
                if let Some(mut internal_session) =
                    self.session_store.get_session(&session.id).await?
                {
                    internal_session.session.status = SessionStatus::Failed;
                    let _ = self.session_store.update_session(&session.id, &internal_session).await;
                }
            }
        }

        Ok(())
    }

    /// Start a specific task by transitioning it through the state machine
    async fn start_task(&self, session_id: &str) -> Result<()> {
        // Get the session
        let Some(mut internal_session) = self.session_store.get_session(session_id).await? else {
            return Ok(()); // Session not found
        };

        // Transition to provisioning
        internal_session.session.status = SessionStatus::Provisioning;
        self.session_store.update_session(session_id, &internal_session).await?;

        info!("Starting task {}: provisioning", session_id);

        // Provision workspace
        let snapshot_id = match self.provision_workspace(&internal_session.session).await {
            Ok(snapshot_id) => snapshot_id,
            Err(e) => {
                error!(
                    "Workspace provisioning failed for task {}: {}",
                    session_id, e
                );
                internal_session.session.status = SessionStatus::Failed;
                self.session_store.update_session(session_id, &internal_session).await?;
                return Err(e);
            }
        };

        // Transition to running and start the process
        internal_session.session.status = SessionStatus::Running;
        internal_session.session.started_at = Some(chrono::Utc::now());
        self.session_store.update_session(session_id, &internal_session).await?;

        info!("Starting task {}: running", session_id);

        // Start the agent process
        let handle = self
            .agent_executor
            .spawn_agent_process(
                session_id,
                &format!("{:?}", internal_session.session.agent.agent.software),
                "sonnet", // Default model for REST server
                &internal_session.session.task.prompt,
                ah_core::WorkingCopyMode::Snapshots, // Server uses snapshot mode
                Some(&std::path::PathBuf::from(
                    &internal_session.session.workspace.mount_path,
                )),
                snapshot_id,
            )
            .await?;

        // Store the running task handle
        self.running_tasks.write().await.insert(session_id.to_string(), handle);

        Ok(())
    }

    /// Provision workspace for the task
    ///
    /// This implements the snapshot caching strategy:
    /// 1. Check if snapshot exists for the commit
    /// 2. If yes, mount it as workspace
    /// 3. If no, acquire lock, checkout commit, let agent create snapshot
    async fn provision_workspace(&self, session: &Session) -> Result<Option<String>> {
        // Get repository and commit information
        let repo_url = session.vcs.repo_url.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Repository URL is required for workspace provisioning")
        })?;
        let commit =
            session.vcs.commit.as_ref().ok_or_else(|| {
                anyhow::anyhow!("Commit hash is required for workspace provisioning")
            })?;

        // Check if we have a cached snapshot
        let snapshot_path = {
            let cache = self.snapshot_cache.read().await;
            cache.get_snapshot(repo_url, commit).clone()
        };

        if let Some(_snapshot_path) = snapshot_path {
            info!("Using cached snapshot for {}@{}", repo_url, commit);
            let snapshot_id = format!("{}@{}", repo_url, commit);
            return Ok(Some(snapshot_id));
        }

        // No snapshot found, need to provision workspace
        info!(
            "No snapshot found for {}@{}, provisioning workspace",
            repo_url, commit
        );

        // Acquire provisioning lock to prevent concurrent provisioning
        let _lock = self.provisioning_lock.lock().await;

        // Double-check cache in case another task provisioned it while we waited
        let snapshot_path = {
            let cache = self.snapshot_cache.read().await;
            cache.get_snapshot(repo_url, commit).clone()
        };

        if let Some(_snapshot_path) = snapshot_path {
            info!(
                "Snapshot became available during lock wait for {}@{}",
                repo_url, commit
            );
            let snapshot_id = format!("{}@{}", repo_url, commit);
            return Ok(Some(snapshot_id));
        }

        // Perform checkout and initial build
        self.checkout_and_prepare_workspace(repo_url, commit).await?;

        // The agent will create the initial snapshot during execution
        Ok(None)
    }

    /// Checkout repository and prepare workspace for agent execution
    async fn checkout_and_prepare_workspace(&self, repo_url: &str, commit: &str) -> Result<()> {
        // Use the test filesystem instead of cloning real repositories
        // The test filesystem is mounted at /Volumes/AH_test_zfs/test_dataset
        let test_fs_path = PathBuf::from("/Volumes/AH_test_zfs/test_dataset");

        if !test_fs_path.exists() {
            return Err(anyhow::anyhow!(
                "Test filesystem not available at {:?}",
                test_fs_path
            ));
        }

        // Create a workspace directory within the test filesystem
        let workspace_id = format!("workspace-{}", uuid::Uuid::new_v4().simple());
        let workspace_path = test_fs_path.join("workspaces").join(workspace_id);

        std::fs::create_dir_all(&workspace_path)?;

        info!(
            "Using test filesystem workspace at {:?} for {}@{}",
            workspace_path, repo_url, commit
        );

        // Instead of cloning a real repository, create a minimal test repository structure
        self.create_test_repository_structure(&workspace_path)?;

        // Set up basic files that an agent might expect
        std::fs::write(
            workspace_path.join("README.md"),
            "# Test Repository\n\nThis is a test repository for agent execution.\n",
        )?;
        std::fs::write(
            workspace_path.join("package.json"),
            r#"{"name": "test-repo", "version": "1.0.0"}"#,
        )?;

        info!("Test workspace prepared at {:?}", workspace_path);

        // Store workspace path in session (would need to be added to session model)
        // For now, this is just preparation - the agent will handle the actual snapshot creation

        Ok(())
    }

    /// Create a minimal test repository structure
    fn create_test_repository_structure(&self, workspace_path: &Path) -> Result<()> {
        // Create basic directory structure
        std::fs::create_dir_all(workspace_path.join("src"))?;
        std::fs::create_dir_all(workspace_path.join("tests"))?;
        std::fs::create_dir_all(workspace_path.join("docs"))?;

        // Create some basic files
        std::fs::write(
            workspace_path.join("src").join("main.rs"),
            "fn main() { println!(\"Hello, world!\"); }",
        )?;
        std::fs::write(
            workspace_path.join("tests").join("test.rs"),
            "#[test] fn test_example() { assert!(true); }",
        )?;

        Ok(())
    }

    /// Clean up completed tasks
    async fn cleanup_completed_tasks(&self) -> Result<()> {
        let mut running_tasks = self.running_tasks.write().await;
        let completed_sessions: Vec<String> = running_tasks
            .iter()
            .filter_map(|(session_id, handle)| {
                if handle.is_finished() {
                    Some(session_id.clone())
                } else {
                    None
                }
            })
            .collect();

        for session_id in completed_sessions {
            info!("Cleaning up completed task {}", session_id);

            // Remove from running tasks
            running_tasks.remove(&session_id);

            // Update session status to completed
            if let Some(mut internal_session) = self.session_store.get_session(&session_id).await? {
                internal_session.session.status = SessionStatus::Completed;
                internal_session.session.ended_at = Some(chrono::Utc::now());
                let _ = self.session_store.update_session(&session_id, &internal_session).await;
            }

            // TODO: Clean up workspace, logs, etc.
        }

        Ok(())
    }

    /// Stop a running task
    pub async fn stop_task(&self, session_id: &str) -> Result<()> {
        // TODO: Implement proper process termination
        // For now, just mark as stopping
        if let Some(mut internal_session) = self.session_store.get_session(session_id).await? {
            internal_session.session.status = SessionStatus::Stopping;
            self.session_store.update_session(session_id, &internal_session).await?;
        }

        // Remove from running tasks if present
        self.running_tasks.write().await.remove(session_id);

        Ok(())
    }
}
