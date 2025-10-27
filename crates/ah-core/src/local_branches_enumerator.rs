// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Local Branch Enumerator - Local Database Branch Discovery
//!
//! This module implements branch discovery from local repositories stored in the database.

use ah_domain_types::Branch;
use async_trait::async_trait;

use crate::DatabaseManager;

/// Local database-based branch enumerator
///
/// Discovers branches from local repositories by:
/// 1. Querying repository information from the database
/// 2. Using ah-repo to get branches from the actual repository path
/// 3. Constructing Branch objects with appropriate metadata
pub struct LocalBranchesEnumerator {
    /// Database manager for accessing repository records
    db_manager: DatabaseManager,
}

impl LocalBranchesEnumerator {
    /// Create a new local branch enumerator with a database manager
    pub fn new(db_manager: DatabaseManager) -> Self {
        Self { db_manager }
    }
}

#[async_trait]
impl super::BranchesEnumerator for LocalBranchesEnumerator {
    async fn list_branches(&self, repository_id: &str) -> Vec<Branch> {
        // Parse repository ID as integer to get repo info from database
        let repo_id = match repository_id.parse::<i64>() {
            Ok(id) => id,
            Err(_) => {
                tracing::warn!("Invalid repository ID: {}", repository_id);
                return Vec::new();
            }
        };

        // Get repository from database
        let repo_record = match self.db_manager.get_repository_by_id(repo_id) {
            Ok(Some(record)) => record,
            Ok(None) => {
                tracing::warn!("Repository with ID {} not found", repo_id);
                return Vec::new();
            }
            Err(e) => {
                tracing::warn!("Failed to get repository {}: {}", repo_id, e);
                return Vec::new();
            }
        };

        // Check if repository has a root path
        let root_path = match repo_record.root_path {
            Some(path) => path,
            None => {
                tracing::warn!("Repository {} has no root path", repo_id);
                return Vec::new();
            }
        };

        // Open VCS repository
        let repo = match ah_repo::VcsRepo::new(&root_path) {
            Ok(repo) => repo,
            Err(e) => {
                tracing::warn!("Failed to open repository at {}: {}", root_path, e);
                return Vec::new();
            }
        };

        // Get branches from repository
        let branch_names = match repo.branches() {
            Ok(names) => names,
            Err(e) => {
                tracing::warn!(
                    "Failed to get branches from repository {}: {}",
                    root_path,
                    e
                );
                return Vec::new();
            }
        };

        // Map branch names to Branch objects
        let default_branch = repo_record.default_branch.unwrap_or_else(|| "main".to_string());

        branch_names
            .into_iter()
            .map(|name| Branch {
                name: name.clone(),
                is_default: name == default_branch,
                last_commit: None, // Could be populated if needed
            })
            .collect()
    }

    fn description(&self) -> &str {
        "Local database branch discovery"
    }
}
