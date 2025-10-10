//! Workspace Files - Async service for loading repository file information
//!
//! This service provides asynchronous streaming of repository files and related
//! metadata. Files are produced incrementally as VCS ls-files output is processed.
//! It supports dependency injection for testing through the WorkspaceFiles trait.

use std::path::PathBuf;
use std::pin::Pin;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use ah_repo::{VcsRepo, VcsError};

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
pub trait WorkspaceFiles: Send + Sync {
    /// Stream repository files incrementally as they are discovered
    async fn stream_repository_files(&self) -> Result<FileStream, RepositoryError>;

    /// Check if the workspace directory is a git repository
    async fn is_git_repository(&self) -> bool;
}

/// Type alias for VCS errors
pub type RepositoryError = VcsError;

/// Default implementation using VCS repository
pub struct GitWorkspaceFiles {
    vcs_repo: Option<VcsRepo>,
}

impl GitWorkspaceFiles {
    pub fn new(workspace_dir: PathBuf) -> Self {
        let vcs_repo = VcsRepo::new(&workspace_dir).ok();
        Self { vcs_repo }
    }
}

#[async_trait]
impl WorkspaceFiles for GitWorkspaceFiles {
    async fn stream_repository_files(&self) -> Result<FileStream, RepositoryError> {
        let vcs_repo = self.vcs_repo.as_ref()
            .ok_or_else(|| VcsError::RepositoryNotFound("Not a VCS repository".to_string()))?;

        let string_stream = vcs_repo.stream_tracked_files().await?;

        // Transform the stream from String to RepositoryFile
        let file_stream = string_stream.map(|result| {
            result.map(|path| RepositoryFile {
                path,
                detail: Some("Tracked file".to_string()),
            })
        });

        Ok(Box::pin(file_stream))
    }

    async fn is_git_repository(&self) -> bool {
        self.vcs_repo.is_some()
    }
}

/// Mock implementation for testing
#[cfg(test)]
pub struct MockWorkspaceFiles {
    pub files: Vec<RepositoryFile>,
    pub is_git_repo: bool,
    pub delay: std::time::Duration,
}

#[cfg(test)]
impl MockWorkspaceFiles {
    pub fn new(files: Vec<RepositoryFile>, is_git_repo: bool) -> Self {
        Self {
            files,
            is_git_repo,
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
            true,
        )
    }
}

#[cfg(test)]
#[async_trait]
impl WorkspaceFiles for MockWorkspaceFiles {
    async fn stream_repository_files(&self) -> Result<FileStream, RepositoryError> {
        if !self.is_git_repo {
            return Err(RepositoryError::RepositoryNotFound("Not a VCS repository".to_string()));
        }

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

    async fn is_git_repository(&self) -> bool {
        self.is_git_repo
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
        // Test with fake time to run at full speed
        let mock_time = time::pause();
        let start_time = time::Instant::now();

        let service = MockWorkspaceFiles::with_test_files()
            .with_delay(Duration::from_millis(100));

        // This would normally take 200ms (2 files × 100ms), but with fake time it completes instantly
        let mut stream = service.stream_repository_files().await.unwrap();
        let mut files = Vec::new();

        while let Some(result) = stream.next().await {
            files.push(result.unwrap());
        }

        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "src/main.rs");
        assert_eq!(files[1].path, "Cargo.toml");

        // Verify that fake time advanced by the expected amount (2 files × 100ms delay each)
        let elapsed = start_time.elapsed();
        assert_eq!(elapsed, Duration::from_millis(200));

        mock_time.resume();
    }

    #[tokio::test]
    async fn test_mock_service_no_delay() {
        let service = MockWorkspaceFiles::with_test_files();

        let mut stream = service.stream_repository_files().await.unwrap();
        let mut files = Vec::new();

        while let Some(result) = stream.next().await {
            files.push(result.unwrap());
        }

        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "src/main.rs");
        assert_eq!(files[1].path, "Cargo.toml");
    }

    #[tokio::test]
    async fn test_mock_service_not_git_repo() {
        let service = MockWorkspaceFiles::new(vec![], false);

        let result = service.stream_repository_files().await;
        assert!(matches!(result, Err(RepositoryError::RepositoryNotFound(_))));

        assert!(!service.is_git_repository().await);
    }

    #[tokio::test]
    async fn test_mock_service_empty() {
        let service = MockWorkspaceFiles::new(vec![], true);

        let mut stream = service.stream_repository_files().await.unwrap();
        let files: Vec<_> = stream.collect().await;

        assert_eq!(files.len(), 0);
    }

    #[tokio::test]
    async fn test_stream_collection() {
        let service = MockWorkspaceFiles::with_test_files();

        // Collect all results from the stream
        let stream = service.stream_repository_files().await.unwrap();
        let files: Vec<_> = stream.collect().await;

        assert_eq!(files.len(), 2);
        assert!(files[0].as_ref().unwrap().path == "src/main.rs");
        assert!(files[1].as_ref().unwrap().path == "Cargo.toml");
    }

    #[tokio::test]
    async fn test_stream_partial_collection() {
        let service = MockWorkspaceFiles::with_test_files();

        let mut stream = service.stream_repository_files().await.unwrap();

        // Take only the first file
        let first = stream.next().await.unwrap().unwrap();
        assert_eq!(first.path, "src/main.rs");

        // Take the second file
        let second = stream.next().await.unwrap().unwrap();
        assert_eq!(second.path, "Cargo.toml");

        // Stream should be exhausted
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn test_custom_files() {
        let custom_files = vec![
            RepositoryFile {
                path: "lib.rs".to_string(),
                detail: Some("Library file".to_string()),
            },
            RepositoryFile {
                path: "main.rs".to_string(),
                detail: Some("Entry point".to_string()),
            },
            RepositoryFile {
                path: "Cargo.toml".to_string(),
                detail: Some("Package manifest".to_string()),
            },
        ];

        let service = MockWorkspaceFiles::new(custom_files, true);

        let stream = service.stream_repository_files().await.unwrap();
        let files: Vec<_> = stream.collect().await;

        assert_eq!(files.len(), 3);
        assert_eq!(files[0].as_ref().unwrap().path, "lib.rs");
        assert_eq!(files[0].as_ref().unwrap().detail, Some("Library file".to_string()));
        assert_eq!(files[1].as_ref().unwrap().path, "main.rs");
        assert_eq!(files[1].as_ref().unwrap().detail, Some("Entry point".to_string()));
        assert_eq!(files[2].as_ref().unwrap().path, "Cargo.toml");
        assert_eq!(files[2].as_ref().unwrap().detail, Some("Package manifest".to_string()));
    }

    #[tokio::test]
    async fn test_service_trait_objects() {
        // Test that we can use trait objects
        let service: Box<dyn WorkspaceFiles> = Box::new(MockWorkspaceFiles::with_test_files());

        let stream = service.stream_repository_files().await.unwrap();
        let files: Vec<_> = stream.collect().await;

        assert_eq!(files.len(), 2);
        assert!(service.is_git_repository().await);
    }

    #[tokio::test]
    async fn test_delay_precision() {
        let mock_time = time::pause();

        let service = MockWorkspaceFiles::with_test_files()
            .with_delay(Duration::from_millis(50));

        let start = time::Instant::now();

        let mut stream = service.stream_repository_files().await.unwrap();

        // First file should complete after 50ms
        let first = stream.next().await.unwrap().unwrap();
        assert_eq!(start.elapsed(), Duration::from_millis(50));
        assert_eq!(first.path, "src/main.rs");

        // Second file should complete after another 50ms (total 100ms)
        let second = stream.next().await.unwrap().unwrap();
        assert_eq!(start.elapsed(), Duration::from_millis(100));
        assert_eq!(second.path, "Cargo.toml");

        mock_time.resume();
    }

    #[tokio::test]
    async fn test_concurrent_streams() {
        let service1 = MockWorkspaceFiles::with_test_files();
        let service2 = MockWorkspaceFiles::with_test_files();

        let (result1, result2) = tokio::join!(
            async {
                let stream = service1.stream_repository_files().await.unwrap();
                stream.collect::<Vec<_>>().await
            },
            async {
                let stream = service2.stream_repository_files().await.unwrap();
                stream.collect::<Vec<_>>().await
            }
        );

        assert_eq!(result1.len(), 2);
        assert_eq!(result2.len(), 2);
    }

    #[tokio::test]
    async fn test_stream_error_handling() {
        // Test that errors are properly propagated through the stream
        let service = MockWorkspaceFiles::new(vec![], false);

        // Should fail immediately when trying to create stream
        let result = service.stream_repository_files().await;
        assert!(matches!(result, Err(RepositoryError::RepositoryNotFound(_))));
    }

    #[tokio::test]
    async fn test_large_number_of_files() {
        // Test with many files to ensure streaming works efficiently
        let mut files = Vec::new();
        for i in 0..1000 {
            files.push(RepositoryFile {
                path: format!("file_{}.rs", i),
                detail: Some(format!("File number {}", i)),
            });
        }

        let service = MockWorkspaceFiles::new(files, true);

        let stream = service.stream_repository_files().await.unwrap();
        let collected: Vec<_> = stream.collect().await;

        assert_eq!(collected.len(), 1000);
        assert_eq!(collected[0].as_ref().unwrap().path, "file_0.rs");
        assert_eq!(collected[999].as_ref().unwrap().path, "file_999.rs");
    }
}

