// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Remote Repository Enumerator - REST API Repository Discovery
//!
//! This module implements repository discovery via REST API calls
//! to remote Agent Harbor servers.

use crate::RestApiClient;
use ah_domain_types::Repository;
use async_trait::async_trait;

/// Remote REST API-based repository enumerator
///
/// Discovers repositories by querying remote Agent Harbor servers via REST API.
/// This allows the TUI to work with repositories hosted on remote servers.
pub struct RemoteRepositoriesEnumerator<C: RestApiClient> {
    /// REST API client for making requests to remote server
    client: C,
    /// Server URL for descriptive purposes
    server_url: String,
}

impl<C: RestApiClient> RemoteRepositoriesEnumerator<C> {
    /// Create a new remote repository enumerator
    pub fn new(client: C, server_url: String) -> Self {
        Self { client, server_url }
    }
}

#[async_trait]
impl<C: RestApiClient> super::RepositoriesEnumerator for RemoteRepositoriesEnumerator<C> {
    async fn list_repositories(&self) -> Vec<Repository> {
        match self.client.list_repositories(None, None).await {
            Ok(repos) => repos
                .into_iter()
                .map(|repo| Repository {
                    id: repo.id,
                    name: repo.display_name,
                    url: repo.remote_url.to_string(),
                    default_branch: repo.default_branch,
                })
                .collect(),
            Err(e) => {
                tracing::warn!("Failed to list repositories: {}", e);
                vec![]
            }
        }
    }

    fn description(&self) -> &str {
        &self.server_url
    }
}
