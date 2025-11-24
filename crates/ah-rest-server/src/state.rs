// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Server state management

use crate::{config::ServerConfig, dependencies::TaskController, models::SessionStore};
use ah_local_db::Database;
use std::sync::Arc;

/// Shared server state
#[derive(Clone)]
pub struct AppState {
    /// Database connection
    pub db: Arc<Database>,

    /// Server configuration
    pub config: ServerConfig,

    /// Session store for managing sessions
    pub session_store: Arc<dyn SessionStore>,

    /// Optional task controller for lifecycle operations
    pub task_controller: Option<Arc<dyn TaskController>>,
}
