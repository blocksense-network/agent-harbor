//! Filesystem snapshot provider abstractions for Agents Workflow.
//!
//! This crate provides abstractions for different filesystem snapshot technologies
//! (ZFS, Btrfs, etc.) to enable time travel capabilities in the AH system.

// Re-export all types from the traits crate
pub use ah_fs_snapshots_traits::*;
use async_trait::async_trait;
use std::path::{Path, PathBuf};

/// Auto-detect and return the appropriate provider for a given path.
pub fn provider_for(path: &Path) -> Result<Box<dyn FsSnapshotProvider>> {
    // Validate the path first
    validate_destination_path(path)?;

    // Find the provider with the highest capability score
    let mut best_provider: Option<Box<dyn FsSnapshotProvider>> = None;
    let mut best_score = 0;

    // Check ZFS provider if feature is enabled
    #[cfg(feature = "zfs")]
    {
        let zfs_provider = ah_fs_snapshots_zfs::ZfsProvider::new();
        let capabilities = zfs_provider.detect_capabilities(path);
        if capabilities.score > best_score {
            best_score = capabilities.score;
            best_provider = Some(Box::new(zfs_provider));
        }
    }

    // Check Btrfs provider if feature is enabled
    #[cfg(feature = "btrfs")]
    {
        let btrfs_provider = ah_fs_snapshots_btrfs::BtrfsProvider::new();
        let capabilities = btrfs_provider.detect_capabilities(path);
        if capabilities.score > best_score {
            best_score = capabilities.score;
            best_provider = Some(Box::new(btrfs_provider));
        }
    }

    // Check Git provider if feature is enabled (fallback for portability)
    #[cfg(feature = "git")]
    {
        let git_provider = ah_fs_snapshots_git::GitProvider::new();
        let capabilities = git_provider.detect_capabilities(path);
        if capabilities.score > best_score {
            best_score = capabilities.score;
            best_provider = Some(Box::new(git_provider));
        }
    }

    best_provider.ok_or_else(|| Error::provider("No suitable provider found"))
}

/// Validate a destination path for workspace creation.
fn validate_destination_path(dest: &Path) -> Result<()> {
    // Check if the destination path can be created as a directory
    if let Some(parent) = dest.parent() {
        if !parent.exists() {
            // Try to create parent directory to validate permissions
            std::fs::create_dir_all(parent).map_err(|e| {
                Error::provider(format!(
                    "Cannot create parent directory {}: {}",
                    parent.display(),
                    e
                ))
            })?;
            // Clean up the test directory
            if parent.exists() && std::fs::read_dir(parent).map_or(true, |mut d| d.next().is_none())
            {
                let _ = std::fs::remove_dir(parent);
            }
        }
    }

    // Ensure it's not trying to create in system directories
    let invalid_paths = ["/dev", "/proc", "/sys", "/run"];
    for invalid in &invalid_paths {
        if dest.starts_with(invalid) {
            return Err(Error::provider(format!(
                "Cannot create workspace in system directory: {}",
                dest.display()
            )));
        }
    }

    // Check for other invalid paths
    if dest == Path::new("/") {
        return Err(Error::provider("Cannot create workspace at root directory"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_path_validation() {
        // Valid paths should pass validation
        assert!(validate_destination_path(Path::new("/tmp/test")).is_ok());
        assert!(validate_destination_path(Path::new("./test")).is_ok());

        // Invalid paths should be rejected
        assert!(validate_destination_path(Path::new("/")).is_err());
        assert!(validate_destination_path(Path::new("/dev/null")).is_err());
        assert!(validate_destination_path(Path::new("/proc/version")).is_err());
        assert!(validate_destination_path(Path::new("/sys/class")).is_err());
        assert!(validate_destination_path(Path::new("/run/lock")).is_err());
    }

    #[tokio::test]
    async fn test_provider_selection_git_fallback() {
        #[cfg(feature = "git")]
        {
            use ah_repo::test_helpers::create_git_repo;

            // Create a git repository using the test helpers
            let repo = create_git_repo(None).await.unwrap();

            // Test provider selection - should select Git provider for git repos
            let provider_result = provider_for(&repo.path);
            assert!(
                provider_result.is_ok(),
                "Should find a provider for git repository"
            );

            let provider = provider_result.unwrap();
            let capabilities = provider.detect_capabilities(&repo.path);

            // Should have selected Git provider (highest score for git repos)
            assert_eq!(
                capabilities.kind,
                ah_fs_snapshots_traits::SnapshotProviderKind::Git
            );
            assert!(
                capabilities.score > 0,
                "Git provider should have positive score for git repos"
            );
        }

        #[cfg(not(feature = "git"))]
        {
            // If git feature is not enabled, this test should be skipped
            println!("Git feature not enabled, skipping git provider test");
        }
    }

    // Note: tests referencing copy fallback were removed with provider deprecation.
}
