// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Btrfs snapshot provider implementation for Agent Harbor.

use ah_fs_snapshots_traits::{
    FsSnapshotProvider, PreparedWorkspace, ProviderCapabilities, Result, SnapshotProviderKind,
    SnapshotRef, WorkingCopyMode,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;

/// Btrfs snapshot provider implementation.
#[derive(Default)]
pub struct BtrfsProvider;

impl BtrfsProvider {
    /// Create a new Btrfs provider.
    pub fn new() -> Self {
        Self
    }

    /// Check if Btrfs is available on this system.
    fn btrfs_available() -> bool {
        // Only available on Linux
        if !cfg!(target_os = "linux") {
            return false;
        }

        // Check if btrfs command exists
        std::process::Command::new("which")
            .arg("btrfs")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Get the filesystem type for a given path.
    fn fs_type(path: &Path) -> Result<String> {
        let output = std::process::Command::new("stat")
            .args(["-f", "-c", "%T"])
            .arg(path)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()?;

        if !output.status.success() {
            return Err(ah_fs_snapshots_traits::Error::provider(
                "Failed to determine filesystem type",
            ));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Get the Btrfs subvolume for a given path.
    fn get_subvolume_for_path(&self, path: &Path) -> Result<String> {
        let output = std::process::Command::new("btrfs")
            .args(["subvolume", "show"])
            .arg(path)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()?;

        if !output.status.success() {
            return Err(ah_fs_snapshots_traits::Error::provider(format!(
                "Path is not in a Btrfs subvolume: {}",
                path.display()
            )));
        }

        // The first line should contain the subvolume path
        let output_str = String::from_utf8_lossy(&output.stdout);
        if let Some(first_line) = output_str.lines().next() {
            Ok(first_line.trim().to_string())
        } else {
            Err(ah_fs_snapshots_traits::Error::provider(
                "Failed to parse btrfs subvolume show output",
            ))
        }
    }

    /// Execute a Btrfs command.
    fn execute_btrfs_command(&self, args: &[&str]) -> Result<String> {
        let output = std::process::Command::new("btrfs")
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ah_fs_snapshots_traits::Error::provider(format!(
                "Btrfs command failed: {}",
                stderr
            )));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Generate a unique identifier for Btrfs resources.
    fn generate_unique_id(&self) -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        format!("ah_{}_{}", std::process::id(), timestamp)
    }

    /// Parse timestamp from snapshot directory name if it follows a pattern like "ah_1234567890_123"
    fn parse_snapshot_timestamp(&self, dir_name: &str) -> Option<u64> {
        // Try to extract timestamp from ah_ format
        if dir_name.starts_with("ah_") {
            let parts: Vec<&str> = dir_name.split('_').collect();
            if parts.len() >= 2 {
                parts[1].parse::<u64>().ok()
            } else {
                None
            }
        } else {
            None
        }
    }
}

impl FsSnapshotProvider for BtrfsProvider {
    fn kind(&self) -> SnapshotProviderKind {
        SnapshotProviderKind::Btrfs
    }

    fn prepare_writable_workspace_with_branch(
        &self,
        _repo: &Path,
        mode: WorkingCopyMode,
        branch_id: &str,
    ) -> Result<PreparedWorkspace> {
        match mode {
            WorkingCopyMode::CowOverlay => {
                // For Btrfs, the branch_id is the path to an existing subvolume
                let path = PathBuf::from(branch_id);

                if !path.exists() {
                    return Err(ah_fs_snapshots_traits::Error::provider(format!(
                        "Btrfs subvolume path '{}' does not exist",
                        branch_id
                    )));
                }

                if !path.is_dir() {
                    return Err(ah_fs_snapshots_traits::Error::provider(format!(
                        "Btrfs subvolume path '{}' is not a directory",
                        branch_id
                    )));
                }

                // Verify it's actually a btrfs subvolume
                let output = std::process::Command::new("btrfs")
                    .args(["subvolume", "show"])
                    .arg(&path)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .output()?;

                if !output.status.success() {
                    return Err(ah_fs_snapshots_traits::Error::provider(format!(
                        "Path '{}' is not a Btrfs subvolume",
                        branch_id
                    )));
                }

                tracing::info!(
                    branch_id = %branch_id,
                    "Reusing existing Btrfs subvolume"
                );

                Ok(PreparedWorkspace {
                    exec_path: path,
                    working_copy: mode,
                    provider: self.kind(),
                    cleanup_token: format!("btrfs:reuse:{}", branch_id),
                })
            }
            _ => Err(ah_fs_snapshots_traits::Error::provider(
                "Btrfs branch reuse only supports CowOverlay mode",
            )),
        }
    }

    fn detect_capabilities(&self, repo: &Path) -> ProviderCapabilities {
        if !Self::btrfs_available() {
            return ProviderCapabilities {
                kind: self.kind(),
                score: 0,
                supports_cow_overlay: false,
                notes: vec!["Btrfs command not available".to_string()],
            };
        }

        match Self::fs_type(repo) {
            Ok(fs_type) if fs_type == "btrfs" => match self.get_subvolume_for_path(repo) {
                Ok(subvolume) => ProviderCapabilities {
                    kind: self.kind(),
                    score: 80,
                    supports_cow_overlay: true,
                    notes: vec![format!("Using Btrfs subvolume: {}", subvolume)],
                },
                Err(_) => ProviderCapabilities {
                    kind: self.kind(),
                    score: 0,
                    supports_cow_overlay: false,
                    notes: vec!["Path is not in a Btrfs subvolume".to_string()],
                },
            },
            Ok(fs_type) => ProviderCapabilities {
                kind: self.kind(),
                score: 0,
                supports_cow_overlay: false,
                notes: vec![format!("Path is on {} filesystem, not Btrfs", fs_type)],
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
                    cleanup_token: format!("btrfs:inplace:{}", repo.display()),
                })
            }
            WorkingCopyMode::CowOverlay => {
                // Btrfs CoW overlay mode: create a writable subvolume snapshot that
                // the agent can mutate while leaving the source tree untouched.
                let unique_id = ah_fs_snapshots_traits::generate_unique_id();
                let workspace_path = repo.with_file_name(format!("ah_workspace_{}", unique_id));

                self.execute_btrfs_command(&[
                    "subvolume",
                    "snapshot",
                    repo.to_str().unwrap(),
                    workspace_path.to_str().unwrap(),
                ])?;

                Ok(PreparedWorkspace {
                    exec_path: workspace_path.clone(),
                    working_copy: mode,
                    provider: self.kind(),
                    cleanup_token: format!("btrfs:cow:{}", workspace_path.display()),
                })
            }
            WorkingCopyMode::Worktree | WorkingCopyMode::Auto => {
                // Fall back to worktree mode for Btrfs (simpler implementation)
                Err(ah_fs_snapshots_traits::Error::provider(
                    "Btrfs worktree mode not implemented - use CowOverlay",
                ))
            }
        }
    }

    fn snapshot_now(&self, ws: &PreparedWorkspace, label: Option<&str>) -> Result<SnapshotRef> {
        let unique_id = self.generate_unique_id();
        let snapshot_path =
            ws.exec_path.with_file_name(format!("ah_session_snapshot_{}", unique_id));

        // Create readonly snapshot of the current workspace
        self.execute_btrfs_command(&[
            "subvolume",
            "snapshot",
            "-r",
            ws.exec_path.to_str().unwrap(),
            snapshot_path.to_str().unwrap(),
        ])?;

        let mut meta = HashMap::new();
        meta.insert(
            "source_path".to_string(),
            ws.exec_path.to_string_lossy().to_string(),
        );
        meta.insert(
            "snapshot_path".to_string(),
            snapshot_path.to_string_lossy().to_string(),
        );
        meta.insert("timestamp".to_string(), chrono::Utc::now().to_rfc3339());

        Ok(SnapshotRef {
            id: format!("btrfs_snapshot_{}", unique_id),
            label: label.map(|s| s.to_string()),
            provider: self.kind(),
            meta,
        })
    }

    fn mount_readonly(&self, snap: &SnapshotRef) -> Result<PathBuf> {
        // For Btrfs, the snapshot is already a readonly subvolume mounted at its path
        if let Some(snapshot_path) = snap.meta.get("snapshot_path") {
            let path = PathBuf::from(snapshot_path);
            if path.exists() {
                Ok(path)
            } else {
                Err(ah_fs_snapshots_traits::Error::provider(
                    "Btrfs snapshot path does not exist",
                ))
            }
        } else {
            Err(ah_fs_snapshots_traits::Error::provider(
                "Btrfs snapshot missing path metadata",
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
                let branch_path = snap
                    .meta
                    .get("snapshot_path")
                    .map(|p| Path::new(p).with_file_name(format!("ah_branch_{}", unique_id)))
                    .unwrap_or_else(|| PathBuf::from(format!("ah_branch_{}", unique_id)));

                // Create a writable snapshot from the readonly snapshot
                self.execute_btrfs_command(&[
                    "subvolume",
                    "snapshot",
                    snap.meta.get("snapshot_path").unwrap(),
                    branch_path.to_str().unwrap(),
                ])?;

                Ok(PreparedWorkspace {
                    exec_path: branch_path.clone(),
                    working_copy: mode,
                    provider: self.kind(),
                    cleanup_token: format!("btrfs:branch:{}", branch_path.display()),
                })
            }
            _ => Err(ah_fs_snapshots_traits::Error::provider(
                "Btrfs branching only supports CowOverlay mode",
            )),
        }
    }

    fn list_snapshots(
        &self,
        directory: &Path,
    ) -> Result<Vec<ah_fs_snapshots_traits::SnapshotInfo>> {
        use ah_fs_snapshots_traits::SnapshotInfo;

        // For Btrfs, look in the parent directory for snapshot-like directories
        if let Some(parent) = directory.parent() {
            let mut snapshots = Vec::new();

            // Look for .snapshot directories or snapshot-like directories
            if let Ok(entries) = std::fs::read_dir(parent) {
                for entry in entries.flatten() {
                    if let Ok(file_type) = entry.file_type() {
                        if file_type.is_dir() {
                            let path = entry.path();
                            if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                                // Check if this looks like a snapshot directory
                                if dir_name.contains("snapshot") || dir_name.starts_with("ah_") {
                                    // Try to parse timestamp from directory name
                                    let created_at = self
                                        .parse_snapshot_timestamp(dir_name)
                                        .unwrap_or_else(|| {
                                            std::time::SystemTime::now()
                                                .duration_since(std::time::UNIX_EPOCH)
                                                .unwrap_or_default()
                                                .as_secs()
                                        });

                                    let snapshot_ref = ah_fs_snapshots_traits::SnapshotRef {
                                        id: path.to_string_lossy().to_string(),
                                        label: Some(dir_name.to_string()),
                                        provider:
                                            ah_fs_snapshots_traits::SnapshotProviderKind::Btrfs,
                                        meta: std::collections::HashMap::new(),
                                    };

                                    snapshots.push(SnapshotInfo {
                                        snapshot: snapshot_ref,
                                        created_at,
                                        session_id: None,
                                    });
                                }
                            }
                        }
                    }
                }
            }

            Ok(snapshots)
        } else {
            Ok(Vec::new())
        }
    }

    fn cleanup(&self, token: &str) -> Result<()> {
        if token.starts_with("btrfs:inplace:") {
            // Nothing to cleanup for in-place mode
            Ok(())
        } else if token.starts_with("btrfs:cow:") {
            // Format: btrfs:cow:snapshot_path
            let snapshot_path = token.strip_prefix("btrfs:cow:").unwrap_or(token);
            let path = Path::new(snapshot_path);

            if path.exists() {
                // Delete the subvolume
                let _ = self.execute_btrfs_command(&["subvolume", "delete", snapshot_path]);
            }
            Ok(())
        } else if token.starts_with("btrfs:branch:") {
            // Format: btrfs:branch:branch_path
            let branch_path = token.strip_prefix("btrfs:branch:").unwrap_or(token);
            let path = Path::new(branch_path);

            if path.exists() {
                // Delete the subvolume
                let _ = self.execute_btrfs_command(&["subvolume", "delete", branch_path]);
            }
            Ok(())
        } else if token.starts_with("btrfs:reuse:") {
            // Format: btrfs:reuse:branch_id
            // For reused branches, we don't destroy them - they are meant to persist
            tracing::debug!(
                token = %token,
                "Skipping cleanup for reused Btrfs subvolume (persistence intended)"
            );
            Ok(())
        } else {
            Err(ah_fs_snapshots_traits::Error::provider(format!(
                "Invalid Btrfs cleanup token: {}",
                token
            )))
        }
    }
}
