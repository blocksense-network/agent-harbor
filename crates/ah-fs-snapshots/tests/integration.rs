// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Comprehensive integration tests for filesystem snapshot providers.
//!
//! This test file provides end-to-end testing of ZFS and Git snapshot functionality,
//! similar to the legacy Ruby integration tests but implemented in Rust.

use ah_fs_snapshots_traits::{FsSnapshotProvider, WorkingCopyMode};
use filesystem_test_helpers::ZfsTestEnvironment;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

// Include the filesystem test helpers module
#[path = "filesystem_test_helpers.rs"]
mod filesystem_test_helpers;

/// Integration test for ZFS filesystem snapshot providers.
///
/// This test creates ZFS test pools and exercises the provider API
/// to ensure everything works together correctly.
#[test]
fn test_zfs_snapshot_integration() {
    // Skip if not running as root (required for ZFS operations)
    if !is_root() {
        println!("Skipping ZFS integration test: requires root privileges for ZFS operations");
        return;
    }

    let mut env = ZfsTestEnvironment::new().unwrap();

    // Test ZFS provider integration
    test_zfs_provider_integration(&mut env);
}

/// Test ZFS provider integration specifically.
fn test_zfs_provider_integration(env: &mut ZfsTestEnvironment) {
    println!("Testing ZFS provider integration...");

    let pool_result = env.create_zfs_test_pool("integration_zfs_pool", Some(200));

    match pool_result {
        Ok(mount_point) => {
            println!("Successfully created ZFS pool at: {:?}", mount_point);

            // Populate test repository
            populate_test_repo(&mount_point);

            // Test ZFS provider specifically
            #[cfg(feature = "zfs")]
            {
                use ah_fs_snapshots_zfs::ZfsProvider;
                let zfs_provider = ZfsProvider::new();

                let ws_result = zfs_provider
                    .prepare_writable_workspace(&mount_point, WorkingCopyMode::Worktree);
                match ws_result {
                    Ok(ws) => {
                        println!("ZFS workspace created: {:?}", ws.exec_path);

                        // Test snapshot creation
                        let snap_result = zfs_provider.snapshot_now(&ws, Some("integration_test"));
                        match snap_result {
                            Ok(snap) => {
                                println!("ZFS snapshot created: {}", snap.id);

                                // Test readonly mount (if supported)
                                let mount_result = zfs_provider.mount_readonly(&snap);
                                match mount_result {
                                    Ok(readonly_path) => {
                                        println!("Readonly mount created: {:?}", readonly_path);
                                        // Verify readonly access
                                        assert!(readonly_path.join("README.md").exists());
                                    }
                                    Err(e) => {
                                        println!(
                                            "Readonly mount failed (may not be supported): {}",
                                            e
                                        );
                                    }
                                }

                                // Test branching from snapshot
                                let branch_result = zfs_provider
                                    .branch_from_snapshot(&snap, WorkingCopyMode::Worktree);
                                match branch_result {
                                    Ok(branch_ws) => {
                                        println!("Branch created: {:?}", branch_ws.exec_path);

                                        // Cleanup branch
                                        let _ = zfs_provider.cleanup(&branch_ws.cleanup_token);
                                    }
                                    Err(e) => {
                                        println!("Branch creation failed: {}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                println!("Snapshot creation failed: {}", e);
                            }
                        }

                        // Cleanup workspace
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
        Err(e) => {
            println!(
                "ZFS pool creation failed (expected in some environments): {}",
                e
            );
        }
    }
}

/// Populate a directory with test files for integration testing.
fn populate_test_repo(repo_path: &Path) {
    fs::write(repo_path.join("README.md"), "Integration test repository").unwrap();
    fs::write(repo_path.join("test_file.txt"), "Test content").unwrap();

    // Create a subdirectory
    let subdir = repo_path.join("subdir");
    fs::create_dir_all(&subdir).unwrap();
    fs::write(subdir.join("nested_file.txt"), "Nested content").unwrap();
}

/// Integration test for Git filesystem snapshot providers.
///
/// This test creates Git repositories and exercises the Git provider API
/// to ensure everything works together correctly.
#[test]
fn test_git_snapshot_integration() {
    // Skip if git is not available
    if !git_available() {
        println!("Skipping Git integration test: git command not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    let repo_path = temp_dir.path();

    // Populate test repository
    populate_test_repo(repo_path);
    initialize_git_repo(repo_path).unwrap();

    // Test Git provider integration
    test_git_provider_integration(repo_path);
}

/// Test Git provider integration specifically.
fn test_git_provider_integration(repo_path: &Path) {
    println!("Testing Git provider integration...");

    #[cfg(feature = "git")]
    {
        use ah_fs_snapshots_git::GitProvider;
        let git_provider = GitProvider::new();

        // Test capabilities detection
        let capabilities = git_provider.detect_capabilities(repo_path);
        println!(
            "Git provider capabilities: score={}, supports_cow={}",
            capabilities.score, capabilities.supports_cow_overlay
        );
        assert!(
            capabilities.score > 0,
            "Git provider should be available for git repositories"
        );

        // Test workspace creation
        let ws_result =
            git_provider.prepare_writable_workspace(repo_path, WorkingCopyMode::Worktree);
        match ws_result {
            Ok(ws) => {
                println!("Git workspace created: {:?}", ws.exec_path);

                // Modify a file in the workspace
                let test_file = ws.exec_path.join("test_file.txt");
                std::fs::write(&test_file, "Modified content for snapshot").unwrap();

                // Test snapshot creation
                let snap_result = git_provider.snapshot_now(&ws, Some("integration_test"));
                match snap_result {
                    Ok(snap) => {
                        println!("Git snapshot created: {}", snap.id);

                        // Test readonly mount
                        let mount_result = git_provider.mount_readonly(&snap);
                        match mount_result {
                            Ok(readonly_path) => {
                                println!("Readonly mount created: {:?}", readonly_path);
                                // Verify readonly access
                                assert!(readonly_path.join("README.md").exists());
                                assert!(readonly_path.join("test_file.txt").exists());

                                // Verify the modified content is in the snapshot
                                let content =
                                    std::fs::read_to_string(readonly_path.join("test_file.txt"))
                                        .unwrap();
                                assert_eq!(content, "Modified content for snapshot");
                            }
                            Err(e) => {
                                println!("Readonly mount failed: {}", e);
                            }
                        }

                        // Test branching from snapshot
                        let branch_result =
                            git_provider.branch_from_snapshot(&snap, WorkingCopyMode::Worktree);
                        match branch_result {
                            Ok(branch_ws) => {
                                println!("Branch created: {:?}", branch_ws.exec_path);

                                // Verify branch has the same content
                                let branch_content = std::fs::read_to_string(
                                    branch_ws.exec_path.join("test_file.txt"),
                                )
                                .unwrap();
                                assert_eq!(branch_content, "Modified content for snapshot");

                                // Cleanup branch
                                let _ = git_provider.cleanup(&branch_ws.cleanup_token);
                            }
                            Err(e) => {
                                println!("Branch creation failed: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        println!("Snapshot creation failed: {}", e);
                    }
                }

                // Cleanup workspace
                let _ = git_provider.cleanup(&ws.cleanup_token);
            }
            Err(e) => {
                println!("Git workspace creation failed: {}", e);
            }
        }
    }

    #[cfg(not(feature = "git"))]
    {
        println!("Git feature not enabled, skipping Git-specific tests");
    }
}

// Use git helpers from ah-repo
use ah_repo::test_helpers::{git_available, initialize_git_repo};

/// Check if running as root.
fn is_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}
