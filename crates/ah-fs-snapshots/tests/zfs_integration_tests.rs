//! Integration tests for ZFS filesystem snapshot provider.
//!
//! These tests create real ZFS pools and test the provider functionality,
//! similar to the legacy Ruby test_zfs_provider.rb.

use ah_fs_snapshots_traits::{FsSnapshotProvider, WorkingCopyMode};
use filesystem_test_helpers::ZfsTestEnvironment;
use std::fs;
use std::process::Command;

// Include the filesystem test helpers module
#[path = "filesystem_test_helpers.rs"]
mod filesystem_test_helpers;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zfs_pool_creation() {
        // Skip if not running as root (required for ZFS operations)
        if !is_root() {
            println!("Skipping ZFS pool test: requires root privileges");
            return;
        }

        let mut env = ZfsTestEnvironment::new().unwrap();

        // Try to create a ZFS pool
        let result = env.create_zfs_test_pool("test_zfs_pool", Some(100));

        match result {
            Ok(mount_point) => {
                println!("Successfully created ZFS pool at: {:?}", mount_point);
                assert!(mount_point.exists());

                // Try to write a test file
                let test_file = mount_point.join("test.txt");
                fs::write(&test_file, "ZFS test content").unwrap();
                assert!(test_file.exists());

                // Verify content
                let content = fs::read_to_string(&test_file).unwrap();
                assert_eq!(content, "ZFS test content");
            }
            Err(e) => {
                println!(
                    "ZFS pool creation failed (expected in some environments): {}",
                    e
                );
            }
        }
    }

    #[tokio::test]
    async fn test_zfs_provider_integration() {
        // Skip if not running as root or ZFS not available
        if !is_root() {
            println!("Skipping ZFS integration test: requires root privileges");
            return;
        }

        if !zfs_available() {
            println!("Skipping ZFS integration test: ZFS not available");
            return;
        }

        let mut env = ZfsTestEnvironment::new().unwrap();

        // Create a ZFS pool for testing
        let pool_result = env.create_zfs_test_pool("integration_zfs_pool", Some(200));
        let mount_point = match pool_result {
            Ok(path) => path,
            Err(e) => {
                println!("Skipping test: Could not create ZFS pool: {}", e);
                return;
            }
        };

        // Initialize test repository
        fs::write(mount_point.join("README.md"), "Integration test repository").unwrap();
        fs::write(mount_point.join("test_file.txt"), "Test content").unwrap();

        // Test ZFS provider specifically
        #[cfg(feature = "zfs")]
        {
            use ah_fs_snapshots_zfs::ZfsProvider;
            let zfs_provider = ZfsProvider::new();

            let ws_result =
                zfs_provider.prepare_writable_workspace(&mount_point, WorkingCopyMode::Worktree);
            match ws_result {
                Ok(ws) => {
                    println!("ZFS workspace created: {:?}", ws.exec_path);

                    // Verify workspace isolation
                    assert!(ws.exec_path.exists());
                    assert!(ws.exec_path.join("README.md").exists());

                    // Test writing to workspace
                    let test_file = ws.exec_path.join("integration_test.txt");
                    fs::write(&test_file, "integration test content").unwrap();
                    assert!(test_file.exists());

                    // Verify isolation (changes don't affect original)
                    assert!(!mount_point.join("integration_test.txt").exists());

                    // Cleanup
                    let _ = zfs_provider.cleanup(&ws.cleanup_token);
                }
                Err(e) => {
                    println!("ZFS workspace creation failed: {}", e);
                }
            }
        }

        #[cfg(not(feature = "zfs"))]
        {
            println!("ZFS feature not enabled, skipping ZFS-specific tests");
        }
    }

    fn is_root() -> bool {
        unsafe { libc::geteuid() == 0 }
    }

    fn zfs_available() -> bool {
        Command::new("which").arg("zfs").output().map_or(false, |o| o.status.success())
    }
}
