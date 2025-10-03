//! Error types for filesystem snapshot operations.

use std::path::PathBuf;

/// Error type for filesystem snapshot operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Filesystem not supported for path: {}", .path.display())]
    UnsupportedFilesystem { path: PathBuf },

    #[error("Destination already exists: {}", .path.display())]
    DestinationExists { path: PathBuf },

    #[error("Snapshot creation failed: {message}")]
    SnapshotCreationFailed { message: String },

    #[error("Snapshot cleanup failed: {message}")]
    SnapshotCleanupFailed { message: String },

    #[error("Provider error: {message}")]
    Provider { message: String },
}

impl Error {
    /// Create a new snapshot creation error.
    pub fn snapshot_creation<S: Into<String>>(message: S) -> Self {
        Self::SnapshotCreationFailed {
            message: message.into(),
        }
    }

    /// Create a new snapshot cleanup error.
    pub fn snapshot_cleanup<S: Into<String>>(message: S) -> Self {
        Self::SnapshotCleanupFailed {
            message: message.into(),
        }
    }

    /// Create a new provider error.
    pub fn provider<S: Into<String>>(message: S) -> Self {
        Self::Provider {
            message: message.into(),
        }
    }
}
