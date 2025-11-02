// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! ZFS snapshot provider implementation for Agent Harbor.

use ah_fs_snapshots_daemon::client::{DaemonClient, DaemonError};
use ah_fs_snapshots_traits::{
    FsSnapshotProvider, PreparedWorkspace, ProviderCapabilities, Result, SnapshotInfo,
    SnapshotProviderKind, SnapshotRef, WorkingCopyMode,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;

/// ZFS snapshot provider implementation.
pub struct ZfsProvider {
    daemon_client: DaemonClient,
}

impl ZfsProvider {
    /// Create a new ZFS provider.
    pub fn new() -> Self {
        Self {
            daemon_client: DaemonClient::new(),
        }
    }

    /// Check if ZFS is available on this system.
    fn zfs_available() -> bool {
        // Available on Linux and macOS (OpenZFS)
        if !cfg!(any(target_os = "linux", target_os = "macos")) {
            return false;
        }

        // Check if zfs command exists
        std::process::Command::new("which")
            .arg("zfs")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Get the filesystem type for a given path.
    fn fs_type(path: &Path) -> Result<String> {
        // Try Linux syntax first (stat -f -c %T)
        let linux_output = std::process::Command::new("stat")
            .args(["-f", "-c", "%T"])
            .arg(path)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()?;

        if linux_output.status.success() {
            return Ok(String::from_utf8_lossy(&linux_output.stdout).trim().to_string());
        }

        // Fall back to checking mount output for ZFS (works on macOS and Linux)
        let mount_output = std::process::Command::new("mount")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()?;

        if mount_output.status.success() {
            let mount_stdout = String::from_utf8_lossy(&mount_output.stdout);
            // Check if the path is on a ZFS mount
            for line in mount_stdout.lines() {
                if line.contains("zfs") {
                    // Extract the mount point and check if our path is under it
                    if let Some(mount_point) = line.split_whitespace().nth(2) {
                        if path.starts_with(mount_point) {
                            return Ok("zfs".to_string());
                        }
                    }
                }
            }
        }

        Err(ah_fs_snapshots_traits::Error::provider(
            "Failed to determine filesystem type",
        ))
    }

    /// Get the ZFS dataset for a given path.
    fn get_dataset_for_path(&self, path: &Path) -> Result<String> {
        let output = std::process::Command::new("zfs")
            .args(["list", "-H", "-o", "name,mountpoint"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()?;

        if !output.status.success() {
            return Err(ah_fs_snapshots_traits::Error::provider(
                "Failed to list ZFS datasets",
            ));
        }

        let datasets: Vec<(String, String)> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.split('\t').collect();
                if parts.len() == 2 {
                    let (name, mount) = (parts[0].to_string(), parts[1].to_string());
                    // Filter out non-mounted datasets and root
                    if mount != "none"
                        && mount != "legacy"
                        && mount != "/"
                        && path.starts_with(&mount)
                    {
                        Some((name, mount))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();

        // Find the dataset with the longest mountpoint that contains the path
        let best_match = datasets
            .into_iter()
            .filter(|(_, mount)| path.starts_with(Path::new(mount)))
            .max_by_key(|(_, mount)| mount.len());

        match best_match {
            Some((dataset, _)) => Ok(dataset),
            None => Err(ah_fs_snapshots_traits::Error::UnsupportedFilesystem {
                path: path.to_path_buf(),
            }),
        }
    }

    /// Check if daemon is available and responsive.
    fn daemon_available(&self) -> bool {
        self.daemon_client.ping().is_ok()
    }

    /// Generate a unique identifier for ZFS resources.
    fn generate_unique_id(&self) -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        format!("ah_{}_{}", std::process::id(), timestamp)
    }

    /// Find the dataset that contains the given path.
    fn find_dataset_for_path(&self, path: &Path) -> Result<String> {
        let output = std::process::Command::new("zfs")
            .args(["list", "-H", "-o", "name", "-t", "filesystem"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()?;

        if !output.status.success() {
            return Err(ah_fs_snapshots_traits::Error::provider(
                "Failed to list ZFS datasets",
            ));
        }

        let datasets = String::from_utf8(output.stdout)
            .map_err(|_| ah_fs_snapshots_traits::Error::provider("Invalid ZFS output"))?;

        let path_str = path.to_string_lossy();

        // Find the most specific dataset that contains this path
        let mut best_match: Option<String> = None;
        for dataset in datasets.lines() {
            let dataset = dataset.trim();
            if dataset.is_empty() {
                continue;
            }

            // Get mountpoint for this dataset
            let mountpoint_output = std::process::Command::new("zfs")
                .args(["get", "-H", "-o", "value", "mountpoint", dataset])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .output()?;

            if mountpoint_output.status.success() {
                let mountpoint = String::from_utf8(mountpoint_output.stdout)
                    .unwrap_or_default()
                    .trim()
                    .to_string();

                if !mountpoint.is_empty() && mountpoint != "-" && path_str.starts_with(&mountpoint)
                {
                    // Check if this is a better match (longer mountpoint path)
                    if let Some(ref current_best) = best_match {
                        if mountpoint.len() > current_best.len() {
                            best_match = Some(dataset.to_string());
                        }
                    } else {
                        best_match = Some(dataset.to_string());
                    }
                }
            }
        }

        best_match.ok_or_else(|| {
            ah_fs_snapshots_traits::Error::provider(format!(
                "No ZFS dataset found for path: {}",
                path.display()
            ))
        })
    }

    /// Parse timestamp from snapshot name if it follows a pattern like "snapshot_1234567890"
    fn parse_snapshot_timestamp(&self, snapshot_name: &str) -> Option<u64> {
        // Try to extract timestamp from common patterns
        if snapshot_name.starts_with("snapshot_") {
            snapshot_name.strip_prefix("snapshot_")?.parse::<u64>().ok()
        } else if snapshot_name.chars().all(|c| c.is_ascii_digit()) {
            snapshot_name.parse::<u64>().ok()
        } else {
            None
        }
    }
}

impl FsSnapshotProvider for ZfsProvider {
    fn kind(&self) -> SnapshotProviderKind {
        SnapshotProviderKind::Zfs
    }

    fn detect_capabilities(&self, repo: &Path) -> ProviderCapabilities {
        if !Self::zfs_available() {
            return ProviderCapabilities {
                kind: self.kind(),
                score: 0,
                supports_cow_overlay: false,
                notes: vec!["ZFS command not available".to_string()],
            };
        }

        match Self::fs_type(repo) {
            Ok(fs_type) if fs_type == "zfs" => match self.get_dataset_for_path(repo) {
                Ok(dataset) => ProviderCapabilities {
                    kind: self.kind(),
                    score: 90,
                    supports_cow_overlay: true,
                    notes: vec![format!("Using ZFS dataset: {}", dataset)],
                },
                Err(_) => ProviderCapabilities {
                    kind: self.kind(),
                    score: 0,
                    supports_cow_overlay: false,
                    notes: vec!["No ZFS dataset found for path".to_string()],
                },
            },
            Ok(fs_type) => ProviderCapabilities {
                kind: self.kind(),
                score: 0,
                supports_cow_overlay: false,
                notes: vec![format!("Path is on {} filesystem, not ZFS", fs_type)],
            },
            Err(e) => ProviderCapabilities {
                kind: self.kind(),
                score: 0,
                supports_cow_overlay: false,
                notes: vec![format!("Failed to detect filesystem: {}", e)],
            },
        }
    }

    fn prepare_writable_workspace(
        &self,
        repo: &Path,
        mode: WorkingCopyMode,
    ) -> Result<PreparedWorkspace> {
        match mode {
            WorkingCopyMode::InPlace => {
                // For in-place mode, just return the repo path directly
                Ok(PreparedWorkspace {
                    exec_path: repo.to_path_buf(),
                    working_copy: mode,
                    provider: self.kind(),
                    cleanup_token: format!("zfs:inplace:{}", repo.display()),
                })
            }
            WorkingCopyMode::CowOverlay => {
                // Check if daemon is available before proceeding
                if !self.daemon_available() {
                    return Err(ah_fs_snapshots_traits::Error::provider(
                        "ZFS daemon not available - required for privileged operations",
                    ));
                }

                // ZFS CoW overlay mode: create snapshot + clone
                let dataset = self.get_dataset_for_path(repo)?;
                let unique_id = ah_fs_snapshots_traits::generate_unique_id();
                let snapshot_name = format!("{}@ah_snapshot_{}", dataset, unique_id);
                let clone_name = format!("{}-ah_clone_{}", dataset, unique_id);

                // Create snapshot via daemon
                self.daemon_client.snapshot_zfs(&dataset, &snapshot_name).map_err(|e| {
                    ah_fs_snapshots_traits::Error::provider(format!(
                        "Failed to create ZFS snapshot: {}",
                        e
                    ))
                })?;

                // Create clone via daemon
                let mountpoint =
                    self.daemon_client.clone_zfs(&snapshot_name, &clone_name).map_err(|e| {
                        ah_fs_snapshots_traits::Error::provider(format!(
                            "Failed to create ZFS clone: {}",
                            e
                        ))
                    })?;

                let exec_path = match mountpoint {
                    Some(path) if path != "none" && path != "legacy" => PathBuf::from(path),
                    _ => {
                        // Clone not auto-mounted, find where it should be
                        return Err(ah_fs_snapshots_traits::Error::provider(
                            "ZFS clone not mounted - manual mounting not yet implemented",
                        ));
                    }
                };

                Ok(PreparedWorkspace {
                    exec_path: exec_path.clone(),
                    working_copy: mode,
                    provider: self.kind(),
                    cleanup_token: format!("zfs:cow:{}:{}", snapshot_name, clone_name),
                })
            }
            WorkingCopyMode::Worktree | WorkingCopyMode::Auto => {
                // Fall back to worktree mode for ZFS (simpler implementation)
                // In practice, ZFS would typically use CoW overlay
                Err(ah_fs_snapshots_traits::Error::provider(
                    "ZFS worktree mode not implemented - use CowOverlay",
                ))
            }
        }
    }

    fn snapshot_now(&self, ws: &PreparedWorkspace, label: Option<&str>) -> Result<SnapshotRef> {
        let dataset = self.get_dataset_for_path(&ws.exec_path)?;
        let unique_id = ah_fs_snapshots_traits::generate_unique_id();
        let snapshot_name = format!("{}@ah_session_{}", dataset, unique_id);

        // Create snapshot via daemon
        self.daemon_client.snapshot_zfs(&dataset, &snapshot_name).map_err(|e| {
            ah_fs_snapshots_traits::Error::provider(format!("Failed to create ZFS snapshot: {}", e))
        })?;

        let mut meta = HashMap::new();
        meta.insert("dataset".to_string(), dataset.clone());
        meta.insert("timestamp".to_string(), chrono::Utc::now().to_rfc3339());

        Ok(SnapshotRef {
            id: snapshot_name,
            label: label.map(|s| s.to_string()),
            provider: self.kind(),
            meta,
        })
    }

    fn mount_readonly(&self, snap: &SnapshotRef) -> Result<PathBuf> {
        // For ZFS, snapshots are typically accessed by mounting the snapshot directly
        // This is a simplified implementation
        let snapshot_path = format!(
            "{}/.zfs/snapshot/{}",
            snap.meta.get("dataset").unwrap_or(&"".to_string()),
            snap.id.split('@').next_back().unwrap_or("")
        );
        let mount_path = PathBuf::from(snapshot_path);

        if mount_path.exists() {
            Ok(mount_path)
        } else {
            Err(ah_fs_snapshots_traits::Error::provider(
                "ZFS snapshot not accessible via .zfs directory",
            ))
        }
    }

    fn branch_from_snapshot(
        &self,
        snap: &SnapshotRef,
        mode: WorkingCopyMode,
    ) -> Result<PreparedWorkspace> {
        match mode {
            WorkingCopyMode::CowOverlay => {
                let unique_id = self.generate_unique_id();
                let clone_name = format!(
                    "{}-ah_branch_{}",
                    snap.meta.get("dataset").unwrap_or(&"".to_string()),
                    unique_id
                );

                // Create clone from the snapshot via daemon
                let mountpoint =
                    self.daemon_client.clone_zfs(&snap.id, &clone_name).map_err(|e| {
                        ah_fs_snapshots_traits::Error::provider(format!(
                            "Failed to create ZFS clone: {}",
                            e
                        ))
                    })?;

                let exec_path = match mountpoint {
                    Some(path) if path != "none" && path != "legacy" => PathBuf::from(path),
                    _ => {
                        return Err(ah_fs_snapshots_traits::Error::provider(
                            "ZFS clone not mounted - manual mounting not yet implemented",
                        ));
                    }
                };

                Ok(PreparedWorkspace {
                    exec_path,
                    working_copy: mode,
                    provider: self.kind(),
                    cleanup_token: format!("zfs:branch:{}", clone_name),
                })
            }
            _ => Err(ah_fs_snapshots_traits::Error::provider(
                "ZFS branching only supports CowOverlay mode",
            )),
        }
    }

    fn list_snapshots(
        &self,
        directory: &Path,
    ) -> Result<Vec<ah_fs_snapshots_traits::SnapshotInfo>> {
        use ah_fs_snapshots_traits::SnapshotInfo;

        // Find the dataset for this directory
        let dataset = self.find_dataset_for_path(directory)?;
        let snapshots = self.daemon_client.list_zfs_snapshots(&dataset).map_err(|e| {
            ah_fs_snapshots_traits::Error::provider(format!("Failed to list ZFS snapshots: {}", e))
        })?;

        let mut result = Vec::new();
        for snap_name in snapshots {
            if let Some(snapshot_part) = snap_name.split('@').nth(1) {
                let created_at =
                    self.parse_snapshot_timestamp(snapshot_part).unwrap_or_else(|| {
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs()
                    });

                let snapshot_ref = SnapshotRef {
                    id: snap_name.clone(),
                    label: Some(snapshot_part.to_string()),
                    provider: SnapshotProviderKind::Zfs,
                    meta: HashMap::new(),
                };

                result.push(SnapshotInfo {
                    snapshot: snapshot_ref,
                    created_at,
                    session_id: None,
                });
            }
        }

        Ok(result)
    }

    fn cleanup(&self, token: &str) -> Result<()> {
        if token.starts_with("zfs:inplace:") {
            // Nothing to cleanup for in-place mode
            Ok(())
        } else if token.starts_with("zfs:cow:") {
            // Format: zfs:cow:snapshot_name:clone_name
            let parts: Vec<&str> = token.split(':').collect();
            if parts.len() >= 4 {
                let snapshot = parts[2];
                let clone = parts[3];

                // Destroy clone first, then snapshot via daemon
                let _ = self.daemon_client.delete_zfs(clone);
                let _ = self.daemon_client.delete_zfs(snapshot);
            }
            Ok(())
        } else if token.starts_with("zfs:branch:") {
            // Format: zfs:branch:clone_name
            let clone = token.strip_prefix("zfs:branch:").unwrap_or(token);
            let _ = self.daemon_client.delete_zfs(clone);
            Ok(())
        } else {
            Err(ah_fs_snapshots_traits::Error::provider(format!(
                "Invalid ZFS cleanup token: {}",
                token
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[ah_test_utils::logged_test]
    fn test_zfs_provider_creation() {
        let provider = ZfsProvider::new();
        assert_eq!(provider.kind(), SnapshotProviderKind::Zfs);
    }

    #[ah_test_utils::logged_test]
    fn test_zfs_capabilities_on_non_zfs_path() {
        let provider = ZfsProvider::new();
        let capabilities = provider.detect_capabilities(Path::new("/tmp"));

        // On a non-ZFS path, should have low score
        assert_eq!(capabilities.kind, SnapshotProviderKind::Zfs);
        assert_eq!(capabilities.score, 0);
        assert!(!capabilities.supports_cow_overlay);
    }

    #[ah_test_utils::logged_test]
    fn test_zfs_inplace_workspace_creation() {
        let provider = ZfsProvider::new();
        let repo_path = Path::new("/tmp/test_repo");

        let result = provider.prepare_writable_workspace(repo_path, WorkingCopyMode::InPlace);

        // Should succeed even without ZFS
        assert!(result.is_ok());
        let ws = result.unwrap();
        assert_eq!(ws.working_copy, WorkingCopyMode::InPlace);
        assert_eq!(ws.provider, SnapshotProviderKind::Zfs);
        assert!(ws.cleanup_token.starts_with("zfs:inplace:"));
    }

    #[ah_test_utils::logged_test]
    fn test_zfs_worktree_mode_not_implemented() {
        let provider = ZfsProvider::new();
        let repo_path = Path::new("/tmp/test_repo");

        let result = provider.prepare_writable_workspace(repo_path, WorkingCopyMode::Worktree);

        // Should fail with not implemented error
        assert!(result.is_err());
    }

    #[ah_test_utils::logged_test]
    fn test_zfs_auto_mode_falls_back_to_worktree() {
        let provider = ZfsProvider::new();
        let repo_path = Path::new("/tmp/test_repo");

        let result = provider.prepare_writable_workspace(repo_path, WorkingCopyMode::Auto);

        // Should fail with not implemented error (same as worktree)
        assert!(result.is_err());
    }

    #[ah_test_utils::logged_test]
    fn test_cleanup_invalid_token() {
        let provider = ZfsProvider::new();
        let result = provider.cleanup("invalid:token");

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid ZFS cleanup token"));
    }

    #[ah_test_utils::logged_test]
    fn test_cleanup_inplace_token() {
        let provider = ZfsProvider::new();
        let result = provider.cleanup("zfs:inplace:/some/path");

        // Should succeed (no-op)
        assert!(result.is_ok());
    }

    #[ah_test_utils::logged_test]
    fn test_generate_unique_id() {
        let id1 = ah_fs_snapshots_traits::generate_unique_id();
        let id2 = ah_fs_snapshots_traits::generate_unique_id();

        // IDs should be different
        assert_ne!(id1, id2);

        // Should contain process ID
        let pid = std::process::id().to_string();
        assert!(id1.contains(&pid));
    }
}
