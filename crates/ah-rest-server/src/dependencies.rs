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
}

#[async_trait]
impl TaskController for TaskExecutor {
    async fn stop_task(&self, session_id: &str) -> anyhow::Result<()> {
        self.stop_task(session_id).await?;
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
