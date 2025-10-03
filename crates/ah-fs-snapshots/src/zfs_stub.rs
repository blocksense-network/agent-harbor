//! Stub implementation for ZFS provider when the zfs feature is not enabled.

use crate::*;

/// Stub ZFS provider that returns an error when used without the zfs feature.
pub struct ZfsProvider;

impl ZfsProvider {
    /// Create a new ZFS provider stub.
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl FsSnapshotProvider for ZfsProvider {
    fn kind(&self) -> SnapshotProviderKind {
        SnapshotProviderKind::Zfs
    }

    fn detect_capabilities(&self, _repo: &Path) -> ProviderCapabilities {
        ProviderCapabilities {
            kind: self.kind(),
            score: 0,
            supports_cow_overlay: false,
            notes: vec!["ZFS support not compiled in".to_string()],
        }
    }

    async fn prepare_writable_workspace(
        &self,
        _repo: &Path,
        _mode: WorkingCopyMode,
    ) -> Result<PreparedWorkspace> {
        Err(Error::provider("ZFS support not compiled in - enable the 'zfs' feature"))
    }

    async fn snapshot_now(&self, _ws: &PreparedWorkspace, _label: Option<&str>) -> Result<SnapshotRef> {
        Err(Error::provider("ZFS support not compiled in - enable the 'zfs' feature"))
    }

    async fn mount_readonly(&self, _snap: &SnapshotRef) -> Result<PathBuf> {
        Err(Error::provider("ZFS support not compiled in - enable the 'zfs' feature"))
    }

    async fn branch_from_snapshot(
        &self,
        _snap: &SnapshotRef,
        _mode: WorkingCopyMode,
    ) -> Result<PreparedWorkspace> {
        Err(Error::provider("ZFS support not compiled in - enable the 'zfs' feature"))
    }

    async fn cleanup(&self, _token: &str) -> Result<()> {
        Err(Error::provider("ZFS support not compiled in - enable the 'zfs' feature"))
    }
}
