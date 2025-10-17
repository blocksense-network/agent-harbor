//! Server state management

use crate::config::ServerConfig;
use crate::executor::TaskExecutor;
use crate::models::DatabaseSessionStore;
use ah_local_db::Database;
use ah_rest_api_contract::*;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Shared server state
#[derive(Clone)]
pub struct AppState {
    /// Database connection
    pub db: Arc<Database>,

    /// Server configuration
    pub config: ServerConfig,

    /// Session store for managing sessions
    pub session_store: Arc<DatabaseSessionStore>,

    /// Task executor for running agent tasks
    pub task_executor: Arc<TaskExecutor>,
}

impl AppState {
    /// Create new app state
    pub async fn new(config: ServerConfig) -> anyhow::Result<Self> {
        let db = if config.database_path == ":memory:" {
            Arc::new(Database::open_in_memory()?)
        } else {
            Arc::new(Database::open(&config.database_path)?)
        };

        let session_store = Arc::new(DatabaseSessionStore::new(Arc::clone(&db)));
        let task_executor = Arc::new(TaskExecutor::new(Arc::clone(&db), Arc::clone(&session_store), config.config_file.clone()));

        Ok(Self {
            db,
            config,
            session_store,
            task_executor,
        })
    }

    /// Get database reference
    pub fn db(&self) -> &Database {
        &self.db
    }

    /// Get configuration reference
    pub fn config(&self) -> &ServerConfig {
        &self.config
    }
}
