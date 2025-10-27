// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Remote Workspace Files Enumerator - REST API File Discovery
//!
//! This module implements file discovery via REST API calls
//! to remote Agent Harbor servers.

use crate::RestApiClient;
use crate::workspace_files_enumerator::{
    RepositoryError, RepositoryFile, WorkspaceFilesEnumerator,
};
use async_trait::async_trait;
use futures::stream;

/// Remote REST API-based workspace files enumerator
///
/// Discovers repository files by querying remote Agent Harbor servers via REST API.
/// This allows the TUI to work with files hosted on remote servers.
pub struct RemoteWorkspaceFilesEnumerator<C: RestApiClient> {
    /// REST API client for making remote calls
    client: C,
    /// Repository ID to enumerate files for
    repository_id: String,
}

impl<C: RestApiClient> RemoteWorkspaceFilesEnumerator<C> {
    /// Create a new remote workspace files enumerator
    pub fn new(client: C, repository_id: String) -> Self {
        Self {
            client,
            repository_id,
        }
    }
}

#[async_trait]
impl<C: RestApiClient> WorkspaceFilesEnumerator for RemoteWorkspaceFilesEnumerator<C> {
    async fn stream_repository_files(
        &self,
    ) -> Result<crate::workspace_files_enumerator::FileStream, RepositoryError> {
        match self.client.get_repository_files(&self.repository_id).await {
            Ok(files) => {
                let repository_files: Vec<RepositoryFile> = files
                    .into_iter()
                    .map(|file| RepositoryFile {
                        path: file.path,
                        detail: file.detail,
                    })
                    .collect();

                let stream = stream::iter(repository_files.into_iter().map(Ok));
                Ok(Box::pin(stream))
            }
            Err(e) => Err(RepositoryError::Other(format!(
                "Failed to get repository files: {}",
                e
            ))),
        }
    }
}
