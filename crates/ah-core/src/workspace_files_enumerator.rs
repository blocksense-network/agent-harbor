// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Workspace Files Enumerator - Async service for loading repository file information
//!
//! This service provides asynchronous streaming of repository files and related
//! metadata. Files are produced incrementally as VCS ls-files output is processed.
//! It supports dependency injection for testing through the WorkspaceFilesEnumerator trait.

use ah_repo::{VcsError, VcsRepo};
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use std::path::PathBuf;
use std::pin::Pin;
use tokio::sync::mpsc;

/// Repository file item with metadata
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct RepositoryFile {
    pub path: String,
    pub detail: Option<String>,
}

/// Stream type for repository files
pub type FileStream = Pin<Box<dyn Stream<Item = Result<RepositoryFile, RepositoryError>> + Send>>;

/// Trait for workspace file services
#[async_trait]
pub trait WorkspaceFilesEnumerator: Send + Sync {
    /// Stream repository files incrementally as they are discovered
    async fn stream_repository_files(&self) -> Result<FileStream, RepositoryError>;
}

/// Type alias for VCS errors
pub type RepositoryError = VcsError;

#[async_trait]
impl WorkspaceFilesEnumerator for VcsRepo {
    async fn stream_repository_files(&self) -> Result<FileStream, RepositoryError> {
        let string_stream = self.stream_tracked_files().await?;

        // Transform the stream from String to RepositoryFile
        let file_stream = string_stream.map(|result| {
            result.map(|path| RepositoryFile {
                path,
                detail: Some("Tracked file".to_string()),
            })
        });

        Ok(Box::pin(file_stream))
    }
}

/// Mock implementation for testing
#[cfg(test)]
pub struct MockWorkspaceFilesEnumerator {
    pub files: Vec<RepositoryFile>,
    pub delay: std::time::Duration,
}

#[cfg(test)]
impl MockWorkspaceFilesEnumerator {
    pub fn new(files: Vec<RepositoryFile>) -> Self {
        Self {
            files,
            delay: std::time::Duration::from_millis(0),
        }
    }

    pub fn with_delay(mut self, delay: std::time::Duration) -> Self {
        self.delay = delay;
        self
    }

    pub fn with_test_files() -> Self {
        Self::new(
            vec![
                RepositoryFile {
                    path: "src/main.rs".to_string(),
                    detail: Some("Tracked file".to_string()),
                },
                RepositoryFile {
                    path: "Cargo.toml".to_string(),
                    detail: Some("Tracked file".to_string()),
                },
            ],
        )
    }
}

#[cfg(test)]
#[async_trait]
impl WorkspaceFilesEnumerator for MockWorkspaceFilesEnumerator {
    async fn stream_repository_files(&self) -> Result<FileStream, RepositoryError> {
        let files = self.files.clone();
        let delay = self.delay;

        // Use stream::unfold to create the stream
        let stream = futures::stream::unfold(files.into_iter(), move |mut iter| {
            let delay = delay;
            async move {
                match iter.next() {
                    Some(file) => {
                        if !delay.is_zero() {
                            tokio::time::sleep(delay).await;
                        }
                        Some((Ok(file), iter))
                    }
                    None => None,
                }
            }
        });

        Ok(Box::pin(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use std::sync::Arc;
    use tokio::time::{self, Duration};

    #[tokio::test]
    async fn test_mock_service_with_delay() {
        let start_time = time::Instant::now();

        let service = MockWorkspaceFilesEnumerator::with_test_files().with_delay(Duration::from_millis(100));

        // This should take at least 200ms (2 files × 100ms delay each)
        let mut stream = service.stream_repository_files().await.unwrap();
        let mut files = Vec::new();

        while let Some(result) = stream.next().await {
            files.push(result.unwrap());
        }

        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "src/main.rs");
        assert_eq!(files[1].path, "Cargo.toml");

        // Verify that time advanced by at least the expected amount (2 files × 100ms delay each)
        let elapsed = start_time.elapsed();
        assert!(elapsed >= Duration::from_millis(200));
    }

    #[tokio::test]
    async fn test_mock_service_no_delay() {
        let service = MockWorkspaceFilesEnumerator::with_test_files();

        let mut stream = service.stream_repository_files().await.unwrap();
        let mut files = Vec::new();

        while let Some(result) = stream.next().await {
            files.push(result.unwrap());
        }

        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "src/main.rs");
        assert_eq!(files[1].path, "Cargo.toml");
    }


}
