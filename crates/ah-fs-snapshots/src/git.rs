// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Git-based filesystem snapshot provider for Agent Harbor.
//!
//! This provider implements filesystem snapshots using Git's native capabilities,
//! providing a portable fallback for environments without native CoW filesystems
//! like ZFS or Btrfs. It uses shadow repositories with object sharing and
//! git worktrees for efficient workspace management.

use ah_fs_snapshots_traits::{
    FsSnapshotProvider, PreparedWorkspace, ProviderCapabilities, Result, SnapshotProviderKind,
    SnapshotRef, WorkingCopyMode,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use tracing::debug;

/// Git-based snapshot provider implementation.
pub struct GitProvider {
    /// Base directory for shadow repositories (defaults to ~/.cache/ah/git-shadows)
    shadow_repo_dir: PathBuf,
    /// Base directory for worktrees (defaults to ~/.cache/ah/git-worktrees)
    worktree_dir: PathBuf,
    /// Whether to include untracked files in snapshots (defaults to false)
    include_untracked: bool,
    /// Tracks readonly mounts so we can release them on cleanup.
    readonly_mounts: Arc<Mutex<Vec<ReadonlyMount>>>,
}

#[derive(Clone, Debug)]
struct ReadonlyMount {
    path: PathBuf,
    shadow_repo: PathBuf,
}

impl GitProvider {
    /// Create a new Git provider with default configuration.
    pub fn new() -> Self {
        Self {
            shadow_repo_dir: dirs::cache_dir()
                .unwrap_or_else(|| PathBuf::from("/tmp"))
                .join("ah")
                .join("git-shadows"),
            worktree_dir: dirs::cache_dir()
                .unwrap_or_else(|| PathBuf::from("/tmp"))
                .join("ah")
                .join("git-worktrees"),
            include_untracked: false,
            readonly_mounts: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Create a new Git provider with custom configuration.
    pub fn with_config(
        shadow_repo_dir: PathBuf,
        worktree_dir: PathBuf,
        include_untracked: bool,
    ) -> Self {
        Self {
            shadow_repo_dir,
            worktree_dir,
            include_untracked,
            readonly_mounts: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Check if Git is available on this system.
    fn git_available() -> bool {
        std::process::Command::new("git")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Check if the given path is a Git repository.
    fn is_git_repo(path: &Path) -> bool {
        std::process::Command::new("git")
            .args(["-C", &path.to_string_lossy(), "rev-parse", "--git-dir"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Get the path to the shadow repository for a given primary repository.
    fn shadow_repo_path(&self, primary_repo: &Path) -> PathBuf {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        primary_repo
            .canonicalize()
            .unwrap_or_else(|_| primary_repo.to_path_buf())
            .hash(&mut hasher);
        let hash = hasher.finish();

        self.shadow_repo_dir.join(format!("{:x}", hash))
    }

    /// Get the path for a worktree for a given session and branch.
    fn worktree_path(&self, session_id: &str, branch_id: &str) -> PathBuf {
        self.worktree_dir.join(format!("{}_{}", session_id, branch_id))
    }

    /// Ensure the shadow repository exists and is properly configured.
    fn ensure_shadow_repo(&self, repo_path: &Path) -> Result<PathBuf> {
        let shadow_path = self.shadow_repo_path(repo_path);

        if !shadow_path.exists() {
            // Create shadow repository
            std::fs::create_dir_all(&shadow_path).map_err(|e| {
                ah_fs_snapshots_traits::Error::provider(format!(
                    "Failed to create shadow repo directory: {}",
                    e
                ))
            })?;

            // Initialize bare repository
            let status = Command::new("git")
                .args(["init", "--bare"])
                .arg(&shadow_path)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map_err(|e| {
                    ah_fs_snapshots_traits::Error::provider(format!(
                        "Failed to init shadow repo: {}",
                        e
                    ))
                })?;

            if !status.success() {
                return Err(ah_fs_snapshots_traits::Error::provider(
                    "Failed to initialize shadow repository",
                ));
            }

            // Configure shadow repository
            let config_commands = vec![
                vec!["config", "gc.auto", "0"],
                vec!["config", "receive.denyCurrentBranch", "ignore"],
            ];

            for args in config_commands {
                let status = Command::new("git")
                    .args(&args)
                    .current_dir(&shadow_path)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .map_err(|e| {
                        ah_fs_snapshots_traits::Error::provider(format!(
                            "Failed to configure shadow repo: {}",
                            e
                        ))
                    })?;

                if !status.success() {
                    return Err(ah_fs_snapshots_traits::Error::provider(
                        "Failed to configure shadow repository",
                    ));
                }
            }

            // Get the main repository's .git/objects path
            let main_git_objects = self.get_main_repo_objects_path(repo_path)?;

            // Add alternates to share objects with main repo
            let alternates_file = shadow_path.join("objects").join("info").join("alternates");

            std::fs::create_dir_all(alternates_file.parent().unwrap()).map_err(|e| {
                ah_fs_snapshots_traits::Error::provider(format!(
                    "Failed to create alternates dir: {}",
                    e
                ))
            })?;

            let alternates_content = format!("{}\n", main_git_objects.display());
            std::fs::write(&alternates_file, alternates_content).map_err(|e| {
                ah_fs_snapshots_traits::Error::provider(format!(
                    "Failed to write alternates file: {}",
                    e
                ))
            })?;
        }

        Ok(shadow_path)
    }

    /// Get the path to the main repository's .git/objects directory
    fn get_main_repo_objects_path(&self, repo_path: &Path) -> Result<PathBuf> {
        // Use git rev-parse --git-common-dir to get the main repo path
        let output = Command::new("git")
            .args(["rev-parse", "--git-common-dir"])
            .current_dir(repo_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .map_err(|e| {
                ah_fs_snapshots_traits::Error::provider(format!(
                    "Failed to get git common dir: {}",
                    e
                ))
            })?;

        if !output.status.success() {
            return Err(ah_fs_snapshots_traits::Error::provider(
                "Failed to get git common directory",
            ));
        }

        let common_dir = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
        let main_git_dir = if common_dir.is_absolute() {
            common_dir
        } else {
            repo_path.join(common_dir)
        };

        Ok(main_git_dir.join("objects"))
    }

    /// Create a snapshot commit in the shadow repository.
    fn create_snapshot_commit(
        &self,
        worktree_repo: &Path,
        shadow_repo: &Path,
        label: Option<&str>,
    ) -> Result<String> {
        // Create a temporary index for staging changes
        let temp_index = tempfile::NamedTempFile::new().map_err(|e| {
            ah_fs_snapshots_traits::Error::provider(format!("Failed to create temp index: {}", e))
        })?;
        let temp_index_path = temp_index.path();

        // Get the current HEAD commit from worktree repo
        let head_commit = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(worktree_repo)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .map_err(|e| {
                ah_fs_snapshots_traits::Error::provider(format!("Failed to get HEAD commit: {}", e))
            })?;

        if !head_commit.status.success() {
            return Err(ah_fs_snapshots_traits::Error::provider(
                "Failed to get HEAD commit from worktree repository",
            ));
        }

        let head_commit = String::from_utf8_lossy(&head_commit.stdout).trim().to_string();

        // Initialize the temporary index with the current HEAD tree
        let read_tree_output = Command::new("git")
            .args(["read-tree", &head_commit])
            .current_dir(worktree_repo)
            .env("GIT_INDEX_FILE", temp_index_path)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| {
                ah_fs_snapshots_traits::Error::provider(format!(
                    "Failed to initialize index: {}",
                    e
                ))
            })?;

        if !read_tree_output.status.success() {
            let stderr = String::from_utf8_lossy(&read_tree_output.stderr);
            return Err(ah_fs_snapshots_traits::Error::provider(format!(
                "Failed to initialize index: {}",
                stderr
            )));
        }

        // Stage all changes using git add
        let mut add_args = vec!["add", "--all"];
        if self.include_untracked {
            add_args.push("--no-ignore-removal");
        }

        let output = Command::new("git")
            .args(&add_args)
            .current_dir(worktree_repo)
            .env("GIT_INDEX_FILE", temp_index_path)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| {
                ah_fs_snapshots_traits::Error::provider(format!("Failed to stage changes: {}", e))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ah_fs_snapshots_traits::Error::provider(format!(
                "Failed to stage changes for snapshot: {}",
                stderr
            )));
        }

        // Create commit message
        let commit_message = match label {
            Some(label) => format!("AH Snapshot: {}", label),
            None => "AH Snapshot".to_string(),
        };

        // Create commit using git commit-tree
        let mut commit_tree_args = vec!["commit-tree", "-m", &commit_message, "-p", &head_commit];

        // Read the tree from the temporary index
        let tree_output = Command::new("git")
            .args(["write-tree"])
            .current_dir(worktree_repo)
            .env("GIT_INDEX_FILE", temp_index_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| {
                ah_fs_snapshots_traits::Error::provider(format!("Failed to write tree: {}", e))
            })?;

        if !tree_output.status.success() {
            let stderr = String::from_utf8_lossy(&tree_output.stderr);
            return Err(ah_fs_snapshots_traits::Error::provider(format!(
                "Failed to write tree from index: {}",
                stderr
            )));
        }

        let tree_hash = String::from_utf8_lossy(&tree_output.stdout).trim().to_string();
        commit_tree_args.push(&tree_hash);

        let commit_output = Command::new("git")
            .args(&commit_tree_args)
            .current_dir(shadow_repo)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| {
                ah_fs_snapshots_traits::Error::provider(format!("Failed to create commit: {}", e))
            })?;

        if !commit_output.status.success() {
            let stderr = String::from_utf8_lossy(&commit_output.stderr);
            return Err(ah_fs_snapshots_traits::Error::provider(format!(
                "Failed to create snapshot commit: {}",
                stderr
            )));
        }

        Ok(String::from_utf8_lossy(&commit_output.stdout).trim().to_string())
    }

    fn cleanup_readonly_mounts(&self) -> Result<()> {
        let mut mounts = self.readonly_mounts.lock().unwrap();
        if mounts.is_empty() {
            return Ok(());
        }

        let mut retained = Vec::new();
        let mut failures = Vec::new();

        for mount in mounts.drain(..) {
            let debug = std::env::var("FS_SNAPSHOTS_HARNESS_DEBUG").is_ok();
            if debug {
                debug!(
                    "GitProvider: cleaning readonly worktree {} (shadow repo {})",
                    mount.path.display(),
                    mount.shadow_repo.display()
                );
            }

            let removal_status = Command::new("git")
                .args(["worktree", "remove", "--force"])
                .arg(&mount.path)
                .current_dir(&mount.shadow_repo)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();

            if debug {
                match &removal_status {
                    Ok(status) => debug!(
                        "GitProvider: git worktree remove status for {}: {}",
                        mount.path.display(),
                        status
                    ),
                    Err(err) => debug!(
                        "GitProvider: git worktree remove spawn failed for {}: {}",
                        mount.path.display(),
                        err
                    ),
                }
            }

            if mount.path.exists() {
                if debug {
                    debug!(
                        "GitProvider: readonly worktree still exists after git remove: {}",
                        mount.path.display()
                    );
                }
                if let Err(err) = std::fs::remove_dir_all(&mount.path) {
                    failures.push(format!(
                        "{} (fs remove failed: {})",
                        mount.path.display(),
                        err
                    ));
                } else if debug {
                    debug!(
                        "GitProvider: fs::remove_dir_all succeeded for {}",
                        mount.path.display()
                    );
                }
            }

            if mount.path.exists() {
                let detail = match removal_status {
                    Ok(status) => format!("git worktree remove exit status {}", status),
                    Err(err) => format!("git worktree remove failed to start: {}", err),
                };
                failures.push(format!(
                    "{} (still exists after cleanup; {})",
                    mount.path.display(),
                    detail
                ));
                retained.push(mount);
            } else if debug {
                debug!(
                    "GitProvider: readonly worktree removed successfully: {}",
                    mount.path.display()
                );
            }
        }

        *mounts = retained;

        if failures.is_empty() {
            Ok(())
        } else {
            Err(ah_fs_snapshots_traits::Error::provider(format!(
                "Failed to cleanup readonly worktrees: {}",
                failures.join("; ")
            )))
        }
    }
}

impl Default for GitProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl FsSnapshotProvider for GitProvider {
    fn kind(&self) -> SnapshotProviderKind {
        SnapshotProviderKind::Git
    }

    fn prepare_writable_workspace_with_branch(
        &self,
        _repo: &Path,
        mode: WorkingCopyMode,
        branch_id: &str,
    ) -> Result<PreparedWorkspace> {
        match mode {
            WorkingCopyMode::Worktree | WorkingCopyMode::CowOverlay | WorkingCopyMode::Auto => {
                // For Git, the branch_id is the path to an existing worktree
                let path = PathBuf::from(branch_id);

                if !path.exists() {
                    return Err(ah_fs_snapshots_traits::Error::provider(format!(
                        "Git worktree path '{}' does not exist",
                        branch_id
                    )));
                }

                if !path.is_dir() {
                    return Err(ah_fs_snapshots_traits::Error::provider(format!(
                        "Git worktree path '{}' is not a directory",
                        branch_id
                    )));
                }

                // Verify it's actually a git worktree or repository
                let is_git = Command::new("git")
                    .args(["-C", &path.to_string_lossy(), "rev-parse", "--git-dir"])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);

                if !is_git {
                    return Err(ah_fs_snapshots_traits::Error::provider(format!(
                        "Path '{}' is not a Git repository or worktree",
                        branch_id
                    )));
                }

                tracing::info!(
                    branch_id = %branch_id,
                    "Reusing existing Git worktree"
                );

                Ok(PreparedWorkspace {
                    exec_path: path,
                    working_copy: WorkingCopyMode::Worktree,
                    provider: self.kind(),
                    cleanup_token: format!("git:reuse:{}", branch_id),
                })
            }
            WorkingCopyMode::InPlace => Err(ah_fs_snapshots_traits::Error::provider(
                "Git branch reuse does not support InPlace mode",
            )),
        }
    }

    fn detect_capabilities(&self, repo: &Path) -> ProviderCapabilities {
        if !Self::git_available() {
            return ProviderCapabilities {
                kind: self.kind(),
                score: 0,
                supports_cow_overlay: false,
                notes: vec!["Git command not available".to_string()],
            };
        }

        if !Self::is_git_repo(repo) {
            return ProviderCapabilities {
                kind: self.kind(),
                score: 0,
                supports_cow_overlay: false,
                notes: vec!["Path is not a Git repository".to_string()],
            };
        }

        // Git provider has a moderate score as a fallback
        ProviderCapabilities {
            kind: self.kind(),
            score: 10,
            supports_cow_overlay: false, // Git doesn't support true CoW
            notes: vec![
                "Git-based snapshots available".to_string(),
                format!("Shadow repo: {}", self.shadow_repo_path(repo).display()),
            ],
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
                    cleanup_token: format!("git:inplace:{}", repo.display()),
                })
            }
            WorkingCopyMode::Worktree => {
                // Create a git worktree for isolated workspace
                let session_id = ah_fs_snapshots_traits::generate_unique_id();
                let branch_name = format!("ah-worktree-{}", session_id);
                let worktree_path = self.worktree_path(&session_id, "main");

                // Create worktree
                let status = Command::new("git")
                    .args([
                        "worktree",
                        "add",
                        "--detach",
                        &worktree_path.to_string_lossy(),
                        "HEAD",
                    ])
                    .current_dir(repo)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .map_err(|e| {
                        ah_fs_snapshots_traits::Error::provider(format!(
                            "Failed to create worktree: {}",
                            e
                        ))
                    })?;

                if !status.success() {
                    return Err(ah_fs_snapshots_traits::Error::provider(
                        "Failed to create git worktree",
                    ));
                }

                Ok(PreparedWorkspace {
                    exec_path: worktree_path,
                    working_copy: mode,
                    provider: self.kind(),
                    cleanup_token: format!("git:worktree:{}:{}", session_id, branch_name),
                })
            }
            WorkingCopyMode::CowOverlay | WorkingCopyMode::Auto => {
                // Fall back to worktree mode for Git
                self.prepare_writable_workspace(repo, WorkingCopyMode::Worktree)
            }
        }
    }

    fn snapshot_now(&self, ws: &PreparedWorkspace, label: Option<&str>) -> Result<SnapshotRef> {
        let shadow_repo = self.ensure_shadow_repo(&ws.exec_path)?;
        let commit_hash = self.create_snapshot_commit(&ws.exec_path, &shadow_repo, label)?;

        let mut meta = HashMap::new();
        meta.insert(
            "shadow_repo".to_string(),
            shadow_repo.to_string_lossy().to_string(),
        );
        meta.insert("timestamp".to_string(), chrono::Utc::now().to_rfc3339());

        Ok(SnapshotRef {
            id: commit_hash,
            label: label.map(|s| s.to_string()),
            provider: self.kind(),
            meta,
        })
    }

    fn mount_readonly(&self, snap: &SnapshotRef) -> Result<PathBuf> {
        // For Git, create a temporary worktree at the snapshot commit
        let shadow_repo =
            PathBuf::from(snap.meta.get("shadow_repo").as_ref().ok_or_else(|| {
                ah_fs_snapshots_traits::Error::provider("Missing shadow_repo in snapshot metadata")
            })?);

        let session_id = ah_fs_snapshots_traits::generate_unique_id();
        let worktree_path = self.worktree_path(&session_id, "readonly");

        let status = Command::new("git")
            .args([
                "worktree",
                "add",
                "--detach",
                &worktree_path.to_string_lossy(),
                &snap.id,
            ])
            .current_dir(&shadow_repo)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|e| {
                ah_fs_snapshots_traits::Error::provider(format!(
                    "Failed to create readonly worktree: {}",
                    e
                ))
            })?;

        if !status.success() {
            return Err(ah_fs_snapshots_traits::Error::provider(
                "Failed to create readonly worktree for snapshot",
            ));
        }

        self.readonly_mounts.lock().unwrap().push(ReadonlyMount {
            path: worktree_path.clone(),
            shadow_repo,
        });

        Ok(worktree_path)
    }

    fn branch_from_snapshot(
        &self,
        snap: &SnapshotRef,
        mode: WorkingCopyMode,
    ) -> Result<PreparedWorkspace> {
        match mode {
            WorkingCopyMode::Worktree => {
                let shadow_repo =
                    PathBuf::from(snap.meta.get("shadow_repo").as_ref().ok_or_else(|| {
                        ah_fs_snapshots_traits::Error::provider(
                            "Missing shadow_repo in snapshot metadata",
                        )
                    })?);

                let session_id = ah_fs_snapshots_traits::generate_unique_id();
                let branch_name = format!("ah-branch-{}", session_id);
                let worktree_path = self.worktree_path(&session_id, &branch_name);

                // Create worktree from snapshot commit
                let status = Command::new("git")
                    .args([
                        "worktree",
                        "add",
                        "--detach",
                        &worktree_path.to_string_lossy(),
                        &snap.id,
                    ])
                    .current_dir(&shadow_repo)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .map_err(|e| {
                        ah_fs_snapshots_traits::Error::provider(format!(
                            "Failed to create branch worktree: {}",
                            e
                        ))
                    })?;

                if !status.success() {
                    return Err(ah_fs_snapshots_traits::Error::provider(
                        "Failed to create worktree for branch",
                    ));
                }

                Ok(PreparedWorkspace {
                    exec_path: worktree_path,
                    working_copy: mode,
                    provider: self.kind(),
                    cleanup_token: format!("git:branch:{}:{}", session_id, branch_name),
                })
            }
            _ => Err(ah_fs_snapshots_traits::Error::provider(
                "Git branching only supports Worktree mode",
            )),
        }
    }

    fn list_snapshots(
        &self,
        _directory: &Path,
    ) -> Result<Vec<ah_fs_snapshots_traits::SnapshotInfo>> {
        // Git provider doesn't store snapshots in a listable format like ZFS
        // Return empty list for now - Git snapshots are managed through git refs
        Ok(Vec::new())
    }

    fn cleanup(&self, token: &str) -> Result<()> {
        if token.starts_with("git:inplace:") {
            // Nothing to cleanup for in-place mode
            self.cleanup_readonly_mounts()
        } else if token.starts_with("git:worktree:") {
            // Format: git:worktree:session_id:branch_name
            let parts: Vec<&str> = token.split(':').collect();
            if parts.len() >= 4 {
                let session_id = parts[2];
                let branch_name = parts[3];
                let worktree_path = self.worktree_path(session_id, branch_name);

                // Remove worktree
                let _ = Command::new("git")
                    .args(["worktree", "remove", "--force"])
                    .arg(&worktree_path)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();

                // Remove worktree directory if it still exists
                let _ = std::fs::remove_dir_all(&worktree_path);
            }
            self.cleanup_readonly_mounts()
        } else if token.starts_with("git:branch:") {
            // Format: git:branch:session_id:branch_name
            let parts: Vec<&str> = token.split(':').collect();
            if parts.len() >= 4 {
                let session_id = parts[2];
                let branch_name = parts[3];
                let worktree_path = self.worktree_path(session_id, branch_name);

                // Remove worktree
                let _ = Command::new("git")
                    .args(["worktree", "remove", "--force"])
                    .arg(&worktree_path)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();

                // Remove worktree directory if it still exists
                let _ = std::fs::remove_dir_all(&worktree_path);
            }
            self.cleanup_readonly_mounts()
        } else if token.starts_with("git:reuse:") {
            // Format: git:reuse:worktree_path
            // For reused worktrees, we don't destroy them - they are meant to persist
            tracing::debug!(
                token = %token,
                "Skipping cleanup for reused Git worktree (persistence intended)"
            );
            self.cleanup_readonly_mounts()
        } else {
            Err(ah_fs_snapshots_traits::Error::provider(format!(
                "Invalid Git cleanup token: {}",
                token
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_provider_creation() {
        let provider = GitProvider::new();
        assert_eq!(provider.kind(), SnapshotProviderKind::Git);
    }

    #[test]
    fn test_git_capabilities_on_non_git_path() {
        let provider = GitProvider::new();
        let capabilities = provider.detect_capabilities(Path::new("/tmp"));

        // On a non-git path, should have low score
        assert_eq!(capabilities.kind, SnapshotProviderKind::Git);
        assert_eq!(capabilities.score, 0);
        assert!(!capabilities.supports_cow_overlay);
    }

    #[test]
    fn test_git_inplace_workspace_creation() {
        let provider = GitProvider::new();
        let repo_path = Path::new("/tmp/test_repo");

        let result = provider.prepare_writable_workspace(repo_path, WorkingCopyMode::InPlace);

        // Should succeed even without git repo
        assert!(result.is_ok());
        let ws = result.unwrap();
        assert_eq!(ws.working_copy, WorkingCopyMode::InPlace);
        assert_eq!(ws.provider, SnapshotProviderKind::Git);
        assert!(ws.cleanup_token.starts_with("git:inplace:"));
    }

    #[test]
    fn test_git_worktree_mode_not_implemented() {
        let provider = GitProvider::new();
        let repo_path = Path::new("/tmp/test_repo");

        let result = provider.prepare_writable_workspace(repo_path, WorkingCopyMode::Worktree);

        // Should fail without a real git repo
        assert!(result.is_err());
    }

    #[test]
    fn test_git_auto_mode_falls_back_to_worktree() {
        let provider = GitProvider::new();
        let repo_path = Path::new("/tmp/test_repo");

        let result = provider.prepare_writable_workspace(repo_path, WorkingCopyMode::Auto);

        // Should fail (same as worktree without real repo)
        assert!(result.is_err());
    }

    #[test]
    fn test_cleanup_invalid_token() {
        let provider = GitProvider::new();
        let result = provider.cleanup("invalid:token");

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid Git cleanup token"));
    }

    #[test]
    fn test_cleanup_inplace_token() {
        let provider = GitProvider::new();
        let result = provider.cleanup("git:inplace:/some/path");

        // Should succeed (no-op)
        assert!(result.is_ok());
    }

    #[test]
    fn test_generate_unique_id() {
        let id1 = ah_fs_snapshots_traits::generate_unique_id();
        let id2 = ah_fs_snapshots_traits::generate_unique_id();

        // IDs should be different
        assert_ne!(id1, id2);

        // Should contain process ID
        let pid = std::process::id().to_string();
        assert!(id1.contains(&pid));
    }

    #[test]
    fn test_shadow_repo_path_deterministic() {
        let provider = GitProvider::new();
        let repo_path = Path::new("/tmp/test_repo");

        let path1 = provider.shadow_repo_path(repo_path);
        let path2 = provider.shadow_repo_path(repo_path);

        // Should be deterministic for the same path
        assert_eq!(path1, path2);
    }
}
