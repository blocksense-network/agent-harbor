// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Remote Branch Enumerator - REST API Branch Discovery
//!
//! This module implements branch discovery via REST API calls
//! to remote Agent Harbor servers.

use crate::RestApiClient;
use ah_domain_types::Branch;
use async_trait::async_trait;

/// Remote REST API-based branch enumerator
///
/// Discovers branches by querying remote Agent Harbor servers via REST API.
/// This allows the TUI to work with branches hosted on remote servers.
pub struct RemoteBranchesEnumerator<C: RestApiClient> {
    /// REST API client for making requests to remote server
    client: C,
    /// Server URL for descriptive purposes
    server_url: String,
}

impl<C: RestApiClient> RemoteBranchesEnumerator<C> {
    /// Create a new remote branch enumerator
    pub fn new(client: C, server_url: String) -> Self {
        Self { client, server_url }
    }
}

#[async_trait]
impl<C: RestApiClient> super::BranchesEnumerator for RemoteBranchesEnumerator<C> {
    async fn list_branches(&self, repository_id: &str) -> Vec<Branch> {
        match self.client.get_repository_branches(repository_id).await {
            Ok(branch_infos) => branch_infos
                .into_iter()
                .map(|info| Branch {
                    name: info.name,
                    is_default: info.is_default,
                    last_commit: info.last_commit,
                })
                .collect(),
            Err(e) => {
                tracing::warn!(
                    "Failed to get branches for repository {} from remote API: {}",
                    repository_id,
                    e
                );
                Vec::new()
            }
        }
    }

    fn description(&self) -> &str {
        &self.server_url
    }
}
