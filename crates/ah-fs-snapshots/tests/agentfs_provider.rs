// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use ah_fs_snapshots::AgentFsProvider;
use ah_fs_snapshots_traits::{FsSnapshotProvider, SnapshotProviderKind, WorkingCopyMode};
use anyhow::Result;
#[cfg(all(feature = "agentfs", target_os = "macos"))]
use tempfile::TempDir;

#[cfg(all(feature = "agentfs", target_os = "linux"))]
use fs_snapshots_test_harness::agentfs;
#[cfg(all(feature = "agentfs", target_os = "linux"))]
use tracing::info;

#[cfg(all(feature = "agentfs", target_os = "macos"))]
#[serial_test::file_serial(agentfs)]
#[test]
fn agentfs_prepare_snapshot_and_cleanup_cycle() -> Result<()> {
    std::env::set_var("AH_ENABLE_AGENTFS_PROVIDER", "1");

    let temp_repo = TempDir::new()?;
    std::fs::write(
        temp_repo.path().join("README.md"),
        "AgentFS provider smoke test",
    )?;
    std::fs::write(temp_repo.path().join("config.toml"), "key = \"value\"")?;

    let provider = AgentFsProvider::new();

    let workspace = provider.prepare_writable_workspace(temp_repo.path(), WorkingCopyMode::Auto)?;
    assert_eq!(workspace.provider, SnapshotProviderKind::AgentFs);
    assert_eq!(workspace.exec_path, temp_repo.path());
    assert!(
        workspace.exec_path.exists(),
        "AgentFS workspace path should exist"
    );

    let snapshot = provider.snapshot_now(&workspace, Some("initial-checkpoint"))?;
    assert_eq!(snapshot.provider, SnapshotProviderKind::AgentFs);
    assert_eq!(snapshot.label.as_deref(), Some("initial-checkpoint"));

    let branch_workspace = provider.branch_from_snapshot(&snapshot, WorkingCopyMode::Auto)?;
    assert_eq!(branch_workspace.provider, SnapshotProviderKind::AgentFs);
    assert_eq!(branch_workspace.exec_path, temp_repo.path());
    assert!(
        branch_workspace.exec_path.exists(),
        "AgentFS branch workspace path should exist"
    );

    let snapshots = provider.list_snapshots(temp_repo.path())?;
    assert!(!snapshots.is_empty());

    provider.cleanup(&workspace.cleanup_token)?;
    provider.cleanup(&branch_workspace.cleanup_token)?;

    Ok(())
}

#[cfg(all(feature = "agentfs", target_os = "linux"))]
#[test]
fn agentfs_prepare_snapshot_and_cleanup_cycle_fuse() -> Result<()> {
    let harness = match agentfs::FuseHarness::new() {
        Ok(harness) => harness,
        Err(err) => {
            info!("Skipping AgentFS provider test: {err}");
            return Ok(());
        }
    };

    if !harness.socket_path().exists() {
        info!(
            "Skipping AgentFS provider test: daemon socket missing at {}",
            harness.socket_path().display()
        );
        return Ok(());
    }

    let repo = match harness.prepare_repo("agentfs-provider-smoke") {
        Ok(repo) => repo,
        Err(err) => {
            info!(
                "Skipping AgentFS provider test: failed to prepare repo ({}). Ensure /tmp/agentfs is writable.",
                err
            );
            return Ok(());
        }
    };
    std::fs::write(repo.path().join("README.md"), "AgentFS provider smoke test")?;
    std::fs::write(repo.path().join("config.toml"), "key = \"value\"")?;

    let provider = AgentFsProvider::new();

    let workspace = provider.prepare_writable_workspace(repo.path(), WorkingCopyMode::Auto)?;
    assert_eq!(workspace.provider, SnapshotProviderKind::AgentFs);
    assert_eq!(workspace.exec_path, repo.path());
    assert!(workspace.exec_path.exists());

    let snapshot = provider.snapshot_now(&workspace, Some("initial-checkpoint"))?;
    assert_eq!(snapshot.provider, SnapshotProviderKind::AgentFs);
    assert_eq!(snapshot.label.as_deref(), Some("initial-checkpoint"));

    let branch_workspace = provider.branch_from_snapshot(&snapshot, WorkingCopyMode::Auto)?;
    assert_eq!(branch_workspace.provider, SnapshotProviderKind::AgentFs);
    assert_eq!(branch_workspace.exec_path, repo.path());
    assert!(branch_workspace.exec_path.exists());

    let snapshots = provider.list_snapshots(repo.path())?;
    assert!(!snapshots.is_empty());

    provider.cleanup(&workspace.cleanup_token)?;
    provider.cleanup(&branch_workspace.cleanup_token)?;

    Ok(())
}

#[serial_test::file_serial(agentfs)]
#[test]
fn agentfs_invalid_repo_path_rejected() -> Result<()> {
    std::env::set_var("AH_ENABLE_AGENTFS_PROVIDER", "1");

    let provider = AgentFsProvider::new();
    let invalid_repo = std::path::Path::new("/nonexistent/provider-matrix-invalid-repo");
    let result = provider.prepare_writable_workspace(invalid_repo, WorkingCopyMode::Auto);
    assert!(
        result.is_err(),
        "AgentFS provider should reject invalid repository paths"
    );

    Ok(())
}
