// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Dependency wiring for the REST server

use crate::{
    config::ServerConfig,
    executor::TaskExecutor,
    models::{DatabaseSessionStore, SessionStore},
    state::AppState,
};
use ah_local_db::Database;
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

/// Task control surface that can be injected into handlers
#[async_trait]
pub trait TaskController: Send + Sync {
    /// Stop a running task/session
    async fn stop_task(&self, session_id: &str) -> anyhow::Result<()>;

    /// Pause a running task/session (best-effort; may be a no-op if backend lacks support).
    async fn pause_task(&self, _session_id: &str) -> anyhow::Result<()> {
        Ok(())
    }

    /// Resume a paused task/session (best-effort; may be a no-op if backend lacks support).
    async fn resume_task(&self, _session_id: &str) -> anyhow::Result<()> {
        Ok(())
    }

    /// Inject a user/system message into a running task (best-effort, may noop if backend
    /// does not support live message delivery yet).
    async fn inject_message(&self, _session_id: &str, _message: &str) -> anyhow::Result<()> {
        Ok(())
    }
}

#[async_trait]
impl TaskController for TaskExecutor {
    async fn stop_task(&self, session_id: &str) -> anyhow::Result<()> {
        self.stop_task(session_id).await?;
        Ok(())
    }

    async fn pause_task(&self, session_id: &str) -> anyhow::Result<()> {
        tracing::debug!("pause_task stub for session_id={session_id}");
        Ok(())
    }

    async fn resume_task(&self, session_id: &str) -> anyhow::Result<()> {
        tracing::debug!("resume_task stub for session_id={session_id}");
        Ok(())
    }

    async fn inject_message(&self, session_id: &str, message: &str) -> anyhow::Result<()> {
        // TODO: wire to real TaskManager once live agent injection is implemented.
        tracing::debug!("inject_message stub: session_id={session_id}, message_len={}", message.len());
        Ok(())
    }
}

/// Default dependency builder that mirrors the legacy in-process setup
pub struct DefaultServerDependencies {
    state: AppState,
}

impl DefaultServerDependencies {
    /// Build default dependencies (SQLite + TaskExecutor) and start background workers
    pub async fn new(config: ServerConfig) -> Result<Self> {
        let db = if config.database_path == ":memory:" {
            Arc::new(Database::open_in_memory()?)
        } else {
            Arc::new(Database::open(&config.database_path)?)
        };

        let session_store = Arc::new(DatabaseSessionStore::new(Arc::clone(&db)));

        let task_executor = Arc::new(TaskExecutor::new(
            Arc::clone(&db),
            Arc::clone(&session_store),
            config.config_file.clone(),
        ));
        task_executor.start();

        let session_store_trait: Arc<dyn SessionStore> = session_store.clone();
        let task_controller: Arc<dyn TaskController> = task_executor.clone();

        let state = AppState {
            db,
            config,
            session_store: session_store_trait,
            task_controller: Some(task_controller),
        };

        Ok(Self { state })
    }

    /// Consume the dependency builder and return the resulting app state
    pub fn into_state(self) -> AppState {
        self.state
    }
}
