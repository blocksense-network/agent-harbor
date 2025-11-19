// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Common traits and types for filesystem snapshot providers.
//!
//! This crate contains the shared abstractions used by all filesystem snapshot providers
//! to avoid circular dependencies between the main crate and provider implementations.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub mod error;

/// Result type for filesystem snapshot operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Error type for filesystem snapshot operations.
pub use error::Error;

/// Provider kinds for filesystem snapshot operations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SnapshotProviderKind {
    /// Auto-detect the best provider for the given path.
    Auto,
    /// ZFS snapshot provider.
    Zfs,
    /// Btrfs snapshot provider.
    Btrfs,
    /// AgentFS user-space filesystem provider.
    AgentFs,
    /// Git-based snapshot provider.
    Git,
    /// Disable snapshotting entirely.
    Disable,
}

/// Working copy modes for prepared workspaces.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
pub enum WorkingCopyMode {
    /// Auto-detect the best working copy mode.
    Auto,
    /// Use copy-on-write overlay mounting to preserve path stability.
    CowOverlay,
    /// Use a separate worktree directory.
    Worktree,
    /// Execute directly in the original working copy (no isolation).
    InPlace,
}

/// Capabilities detected for a provider on a given repository path.
#[derive(Clone, Debug)]
pub struct ProviderCapabilities {
    /// The kind of provider.
    pub kind: SnapshotProviderKind,
    /// Capability score (higher is better, 0-100).
    pub score: u8,
    /// Whether this provider supports copy-on-write overlay mode.
    pub supports_cow_overlay: bool,
    /// Detection notes shown in diagnostics.
    pub notes: Vec<String>,
}

/// A prepared workspace ready for agent execution.
#[derive(Clone, Debug)]
pub struct PreparedWorkspace {
    /// Path where agent processes should run.
    pub exec_path: PathBuf,
    /// The working copy mode used.
    pub working_copy: WorkingCopyMode,
    /// The provider kind used.
    pub provider: SnapshotProviderKind,
    /// Opaque handle for idempotent teardown across crashes/process boundaries.
    pub cleanup_token: String,
}

/// Reference to a snapshot created by a provider.
#[derive(Clone, Debug)]
pub struct SnapshotRef {
    /// Provider-opaque snapshot identifier.
    pub id: String,
    /// Optional user-visible label.
    pub label: Option<String>,
    /// Provider kind that created this snapshot.
    pub provider: SnapshotProviderKind,
    /// Additional metadata.
    pub meta: HashMap<String, String>,
}

/// Information about a snapshot.
#[derive(Clone, Debug)]
pub struct SnapshotInfo {
    /// The snapshot reference.
    pub snapshot: SnapshotRef,
    /// When the snapshot was created (Unix timestamp).
    pub created_at: u64,
    /// Optional session ID this snapshot belongs to.
    pub session_id: Option<String>,
}

/// Generate a unique identifier for resources.
/// This function provides thread-safe, globally unique identifiers across all snapshot providers.
pub fn generate_unique_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("ah_{}_{}_{}", std::process::id(), timestamp, counter)
}

/// Core trait for filesystem snapshot providers.
pub trait FsSnapshotProvider: Send + Sync {
    /// Return the kind of this provider.
    fn kind(&self) -> SnapshotProviderKind;

    /// Detect capabilities for the current host/repo.
    fn detect_capabilities(&self, repo: &Path) -> ProviderCapabilities;

    /// Create a session workspace (independent or in-place) for the selected working-copy mode.
    fn prepare_writable_workspace(
        &self,
        repo: &Path,
        mode: WorkingCopyMode,
    ) -> Result<PreparedWorkspace>;

    /// Snapshot current workspace state; label is optional UI hint.
    fn snapshot_now(&self, ws: &PreparedWorkspace, label: Option<&str>) -> Result<SnapshotRef>;

    /// Read-only inspection mount for a snapshot (optional).
    fn mount_readonly(&self, snap: &SnapshotRef) -> Result<PathBuf>;

    /// Create a new writable workspace (branch) from a snapshot.
    fn branch_from_snapshot(
        &self,
        snap: &SnapshotRef,
        mode: WorkingCopyMode,
    ) -> Result<PreparedWorkspace>;

    /// List all snapshots for a directory.
    fn list_snapshots(&self, directory: &Path) -> Result<Vec<SnapshotInfo>>;

    /// Cleanup/destroy any resources created by this provider (workspaces, mounts).
    fn cleanup(&self, token: &str) -> Result<()>;
}
