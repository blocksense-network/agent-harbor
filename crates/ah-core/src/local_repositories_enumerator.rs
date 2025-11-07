// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Local Repository Enumerator - Local Database Repository Discovery
//!
//! This module implements repository discovery from the local database
//! as previously done in the local task manager.

use ah_domain_types::Repository;
use async_trait::async_trait;

use crate::DatabaseManager;
use crate::repositories_enumerator::RepositoriesEnumerator;

/// Local database-based repository enumerator
///
/// Discovers repositories from the local database by querying stored repository records.
/// This matches the behavior previously implemented in the local task manager.
pub struct LocalRepositoriesEnumerator {
    /// Database manager for accessing repository records
    db_manager: DatabaseManager,
}

impl LocalRepositoriesEnumerator {
    /// Create a new local repository enumerator with a database manager
    pub fn new(db_manager: DatabaseManager) -> Self {
        Self { db_manager }
    }
}

#[async_trait]
impl RepositoriesEnumerator for LocalRepositoriesEnumerator {
    async fn list_repositories(&self) -> Vec<Repository> {
        // Get repositories from the local database
        match self.db_manager.list_repositories() {
            Ok(repos) => repos
                .into_iter()
                .map(|repo_record| {
                    let remote_url = repo_record.remote_url.as_ref();
                    let root_path = repo_record.root_path.as_ref();
                    Repository {
                        id: repo_record.id.to_string(),
                        name: remote_url
                            .unwrap_or(root_path.unwrap_or(&"Unknown".to_string()))
                            .clone(),
                        url: remote_url.unwrap_or(&"".to_string()).clone(),
                        default_branch: repo_record
                            .default_branch
                            .unwrap_or_else(|| "main".to_string()),
                    }
                })
                .collect(),
            Err(e) => {
                tracing::warn!("Failed to list repositories: {}", e);
                Vec::new()
            }
        }
    }

    fn description(&self) -> &str {
        "Local database repository discovery"
    }
}
