//! Core shared behaviors for basic provider operations.
//!
//! This module contains the core test behaviors that should be implemented by all
//! filesystem snapshot providers, ported from the legacy Ruby provider_core_test_behavior.rb.

use ah_fs_snapshots_traits::*;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Test helper that provides common functionality for provider tests.
pub struct ProviderTestHelper {
    /// Temporary directory for test repositories.
    pub temp_dir: TempDir,
    /// Test repository path.
    pub repo_path: PathBuf,
}

impl ProviderTestHelper {
    /// Create a new test helper with a temporary repository.
    pub fn new() -> std::result::Result<Self, Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let repo_path = temp_dir.path().join("test_repo");

        // Initialize test repository
        fs::create_dir(&repo_path)?;
        fs::write(repo_path.join("README.md"), "test repo content")?;
        fs::write(repo_path.join("test_file.txt"), "additional content")?;

        Ok(Self {
            temp_dir,
            repo_path,
        })
    }

    /// Get the test repository content.
    pub fn test_repo_content(&self) -> &str {
        "test repo content"
    }
}

/// Core test behaviors that all providers should implement.
pub trait ProviderCoreTestBehavior {
    /// Get the provider instance to test.
    fn create_test_provider(&self) -> ah_fs_snapshots_traits::Result<Box<dyn FsSnapshotProvider>>;

    /// Get the provider test helper.
    fn test_helper(&self) -> &ProviderTestHelper;

    /// Get reason why provider should be skipped, or None if it should run.
    fn provider_skip_reason(&self) -> Option<&str> {
        None
    }

    /// Create a workspace destination path for testing.
    fn create_workspace_destination(
        &self,
        suffix: Option<&str>,
    ) -> ah_fs_snapshots_traits::Result<PathBuf> {
        let pid = std::process::id();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| ah_fs_snapshots_traits::Error::provider(format!("Time error: {}", e)))?
            .as_secs();

        let base_name = match suffix {
            Some(s) => format!("workspace_{}_{}_{}", s, pid, timestamp),
            None => format!("workspace_{}_{}", pid, timestamp),
        };

        Ok(self.test_helper().temp_dir.path().join(base_name))
    }

    /// Cleanup a test workspace.
    fn cleanup_test_workspace(
        &self,
        workspace_dir: &Path,
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        if workspace_dir.exists() {
            fs::remove_dir_all(workspace_dir)?;
        }
        Ok(())
    }

    /// Verify cleanup behavior (to be implemented by specific provider tests).
    fn verify_cleanup_behavior(
        &self,
        _workspace_dir: &Path,
        _result_path: &Path,
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // Default implementation - providers can override
        Ok(())
    }
}

/// Performance expectations for providers.
pub trait ProviderPerformanceTestBehavior: ProviderCoreTestBehavior {
    /// Expected maximum time for workspace creation in seconds.
    fn expected_max_creation_time(&self) -> f64 {
        5.0 // Default 5 seconds
    }

    /// Expected maximum time for workspace cleanup in seconds.
    fn expected_max_cleanup_time(&self) -> f64 {
        3.0 // Default 3 seconds
    }

    /// Expected number of concurrent operations for testing.
    fn expected_concurrent_count(&self) -> usize {
        5 // Default 5 concurrent operations
    }

    /// Whether this provider supports space efficiency testing (CoW).
    fn supports_space_efficiency_test(&self) -> bool {
        false // Default false
    }

    /// Expected maximum space usage for CoW operations in bytes.
    fn expected_max_space_usage(&self) -> u64 {
        1024 * 1024 // Default 1MB
    }

    /// Measure space usage for space efficiency testing.
    fn measure_space_usage(&self) -> u64 {
        0 // Default implementation returns 0
    }
}
