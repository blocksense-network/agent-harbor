#![cfg(all(feature = "agentfs", target_os = "macos"))]
// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use ah_fs_snapshots::AgentFsProvider;
use ah_fs_snapshots_traits::{FsSnapshotProvider, SnapshotProviderKind, WorkingCopyMode};
use anyhow::Result;
use tempfile::TempDir;

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
