//! Server state management

use ah_local_db::Database;
use ah_rest_api_contract::*;
use crate::config::ServerConfig;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Shared server state
#[derive(Clone)]
pub struct AppState {
    /// Database connection
    pub db: Arc<Database>,

    /// Server configuration
    pub config: ServerConfig,

    /// In-memory session store for active sessions (for demo purposes)
    /// In production, this would be replaced with proper session management
    pub active_sessions: Arc<RwLock<std::collections::HashMap<String, Session>>>,
}

impl AppState {
    /// Create new app state
    pub async fn new(config: ServerConfig) -> anyhow::Result<Self> {
        let db = if config.database_path == ":memory:" {
            Arc::new(Database::open_in_memory()?)
        } else {
            Arc::new(Database::open(&config.database_path)?)
        };

        Ok(Self {
            db,
            config,
            active_sessions: Arc::new(RwLock::new(std::collections::HashMap::new())),
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
