// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Mock storage backend for testing fault injection and error handling
//!
//! This module provides a configurable mock implementation of `StorageBackend`
//! that can simulate various failure scenarios for testing purposes.

use crate::error::FsResult;
use crate::storage::StorageBackend;
use crate::{ContentId, FallocateMode, FsError};
use libc::EIO;
use std::collections::HashMap;
use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Custom predicate function type for fault injection
pub type FaultPredicate = Arc<dyn Fn(&str, u64) -> Option<FsError> + Send + Sync>;

/// Configurable failure behavior for mock storage operations
pub enum FailureBehavior {
    /// Never fail - all operations succeed
    AlwaysSucceed,

    /// Fail after N successful calls to a specific operation
    /// Example: FailAfter { op: "write", count: 5 } fails on the 6th write
    FailAfter {
        op: &'static str,
        count: u64,
        error_fn: Arc<dyn Fn() -> FsError + Send + Sync>,
    },

    /// Fail for the first N calls to a specific operation
    /// Example: FailFor { op: "read", count: 3 } fails the first 3 reads
    FailFor {
        op: &'static str,
        count: u64,
        error_fn: Arc<dyn Fn() -> FsError + Send + Sync>,
    },

    /// Always fail a specific operation with a specific error
    AlwaysFail {
        op: &'static str,
        error_fn: Arc<dyn Fn() -> FsError + Send + Sync>,
    },

    /// Custom predicate function that determines whether to fail
    /// The function receives (operation_name, call_count) and returns Some(error) to fail
    Custom(FaultPredicate),
}

impl Default for FailureBehavior {
    fn default() -> Self {
        Self::AlwaysSucceed
    }
}

/// Mock storage backend that wraps a real backend and injects configurable failures
///
/// This is a decorator that delegates all operations to an inner `StorageBackend`
/// but can be configured to fail operations according to a `FailureBehavior` policy.
///
/// # Example
///
/// ```ignore
/// use agentfs_core::testing::mock_storage::{MockStorageBackend, FailureBehavior, enospc_error};
/// use agentfs_core::storage::InMemoryBackend;
/// use std::sync::Arc;
///
/// let base = Arc::new(InMemoryBackend::new());
/// let mock = Arc::new(MockStorageBackend::with_behavior(
///     base,
///     FailureBehavior::FailAfter {
///         op: "write",
///         count: 5,
///         error_fn: Arc::new(enospc_error),
///     }
/// ));
///
/// // First 5 writes succeed, 6th fails with ENOSPC
/// ```
pub struct MockStorageBackend {
    inner: Arc<dyn StorageBackend>,
    behavior: Mutex<FailureBehavior>,
    call_counts: Mutex<HashMap<String, AtomicU64>>,
}

impl MockStorageBackend {
    /// Create a new mock backend that wraps the given storage and never fails
    pub fn new(inner: Arc<dyn StorageBackend>) -> Self {
        Self {
            inner,
            behavior: Mutex::new(FailureBehavior::AlwaysSucceed),
            call_counts: Mutex::new(HashMap::new()),
        }
    }

    /// Create a new mock backend with the specified failure behavior
    pub fn with_behavior(inner: Arc<dyn StorageBackend>, behavior: FailureBehavior) -> Self {
        Self {
            inner,
            behavior: Mutex::new(behavior),
            call_counts: Mutex::new(HashMap::new()),
        }
    }

    /// Update the failure behavior at runtime
    pub fn set_behavior(&self, behavior: FailureBehavior) {
        *self.behavior.lock().unwrap() = behavior;
    }

    /// Get the number of times a specific operation has been called
    pub fn call_count(&self, op: &str) -> u64 {
        self.call_counts
            .lock()
            .unwrap()
            .get(op)
            .map(|c| c.load(Ordering::SeqCst))
            .unwrap_or(0)
    }

    /// Reset all call counters to zero
    pub fn reset_counters(&self) {
        let counts = self.call_counts.lock().unwrap();
        for counter in counts.values() {
            counter.store(0, Ordering::SeqCst);
        }
    }

    /// Check if the current operation should fail based on the configured behavior
    fn check_fault(&self, op: &str) -> Result<(), FsError> {
        // Increment call counter
        let mut counts_guard = self.call_counts.lock().unwrap();
        let counter = counts_guard.entry(op.to_string()).or_insert_with(|| AtomicU64::new(0));
        let current_count = counter.fetch_add(1, Ordering::SeqCst);
        drop(counts_guard);

        // Check failure behavior - need to hold the lock to access the behavior
        let behavior_guard = self.behavior.lock().unwrap();
        match &*behavior_guard {
            FailureBehavior::AlwaysSucceed => Ok(()),

            FailureBehavior::FailAfter {
                op: target,
                count: threshold,
                error_fn,
            } => {
                if op == *target && current_count >= *threshold {
                    Err(error_fn())
                } else {
                    Ok(())
                }
            }

            FailureBehavior::FailFor {
                op: target,
                count: limit,
                error_fn,
            } => {
                if op == *target && current_count < *limit {
                    Err(error_fn())
                } else {
                    Ok(())
                }
            }

            FailureBehavior::AlwaysFail {
                op: target,
                error_fn,
            } => {
                if op == *target {
                    Err(error_fn())
                } else {
                    Ok(())
                }
            }

            FailureBehavior::Custom(predicate) => {
                if let Some(err) = predicate(op, current_count) {
                    Err(err)
                } else {
                    Ok(())
                }
            }
        }
    }
}

impl StorageBackend for MockStorageBackend {
    fn read(&self, id: ContentId, offset: u64, buf: &mut [u8]) -> FsResult<usize> {
        self.check_fault("read")?;
        self.inner.read(id, offset, buf)
    }

    fn write(&self, id: ContentId, offset: u64, data: &[u8]) -> FsResult<usize> {
        self.check_fault("write")?;
        self.inner.write(id, offset, data)
    }

    fn truncate(&self, id: ContentId, new_len: u64) -> FsResult<()> {
        self.check_fault("truncate")?;
        self.inner.truncate(id, new_len)
    }

    fn allocate(&self, initial: &[u8]) -> FsResult<ContentId> {
        self.check_fault("allocate")?;
        self.inner.allocate(initial)
    }

    fn clone_cow(&self, base: ContentId) -> FsResult<ContentId> {
        self.check_fault("clone_cow")?;
        self.inner.clone_cow(base)
    }

    fn sync(&self, id: ContentId, data_only: bool) -> FsResult<()> {
        self.check_fault("sync")?;
        self.inner.sync(id, data_only)
    }

    fn fallocate(&self, id: ContentId, mode: FallocateMode, offset: u64, len: u64) -> FsResult<()> {
        self.check_fault("fallocate")?;
        self.inner.fallocate(id, mode, offset, len)
    }

    fn copy_range(
        &self,
        src: ContentId,
        src_offset: u64,
        dst: ContentId,
        dst_offset: u64,
        len: u64,
    ) -> FsResult<u64> {
        self.check_fault("copy_range")?;
        self.inner.copy_range(src, src_offset, dst, dst_offset, len)
    }

    fn seal(&self, id: ContentId) -> FsResult<()> {
        self.check_fault("seal")?;
        self.inner.seal(id)
    }
}

/// Helper function to create a simple EIO error for testing
pub fn eio_error() -> FsError {
    FsError::Io(io::Error::from_raw_os_error(EIO))
}

/// Helper function to create an ENOSPC error for testing
pub fn enospc_error() -> FsError {
    FsError::NoSpace
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::InMemoryBackend;

    #[test]
    fn mock_storage_always_succeed() {
        let base = Arc::new(InMemoryBackend::new());
        let mock = MockStorageBackend::new(base);

        // Should succeed
        let content_id = mock.allocate(b"test data").unwrap();
        assert!(mock.seal(content_id).is_ok());
        assert_eq!(mock.call_count("allocate"), 1);
        assert_eq!(mock.call_count("seal"), 1);
    }

    #[test]
    fn mock_storage_fail_after_count() {
        let base = Arc::new(InMemoryBackend::new());
        let mock = MockStorageBackend::with_behavior(
            base,
            FailureBehavior::FailAfter {
                op: "write",
                count: 2,
                error_fn: Arc::new(eio_error),
            },
        );

        let content_id = mock.allocate(b"test").unwrap();

        // First 2 writes succeed
        assert!(mock.write(content_id, 0, b"a").is_ok());
        assert!(mock.write(content_id, 1, b"b").is_ok());

        // 3rd write fails
        assert!(matches!(
            mock.write(content_id, 2, b"c"),
            Err(FsError::Io(_))
        ));

        // Further writes also fail
        assert!(matches!(
            mock.write(content_id, 3, b"d"),
            Err(FsError::Io(_))
        ));

        assert_eq!(mock.call_count("write"), 4);
    }

    #[test]
    fn mock_storage_fail_for_count() {
        let base = Arc::new(InMemoryBackend::new());
        let mock = MockStorageBackend::with_behavior(
            base,
            FailureBehavior::FailFor {
                op: "allocate",
                count: 2,
                error_fn: Arc::new(enospc_error),
            },
        );

        // First 2 allocations fail
        assert!(matches!(mock.allocate(b"a"), Err(FsError::NoSpace)));
        assert!(matches!(mock.allocate(b"b"), Err(FsError::NoSpace)));

        // 3rd succeeds
        assert!(mock.allocate(b"c").is_ok());

        assert_eq!(mock.call_count("allocate"), 3);
    }

    #[test]
    fn mock_storage_always_fail() {
        let base = Arc::new(InMemoryBackend::new());
        let mock = MockStorageBackend::with_behavior(
            base,
            FailureBehavior::AlwaysFail {
                op: "sync",
                error_fn: Arc::new(eio_error),
            },
        );

        let content_id = mock.allocate(b"test").unwrap();

        // sync always fails
        assert!(matches!(mock.sync(content_id, false), Err(FsError::Io(_))));
        assert!(matches!(mock.sync(content_id, true), Err(FsError::Io(_))));

        assert_eq!(mock.call_count("sync"), 2);
    }

    #[test]
    fn mock_storage_custom_predicate() {
        let base = Arc::new(InMemoryBackend::new());
        let mock = MockStorageBackend::with_behavior(
            base,
            FailureBehavior::Custom(Arc::new(|op, count| {
                // Fail every 3rd read
                if op == "read" && count % 3 == 2 {
                    Some(eio_error())
                } else {
                    None
                }
            })),
        );

        let content_id = mock.allocate(b"test data here").unwrap();
        let mut buf = [0u8; 4];

        // Call 0, 1 succeed
        assert!(mock.read(content_id, 0, &mut buf).is_ok());
        assert!(mock.read(content_id, 0, &mut buf).is_ok());

        // Call 2 fails
        assert!(matches!(
            mock.read(content_id, 0, &mut buf),
            Err(FsError::Io(_))
        ));

        // Call 3, 4 succeed
        assert!(mock.read(content_id, 0, &mut buf).is_ok());
        assert!(mock.read(content_id, 0, &mut buf).is_ok());

        // Call 5 fails
        assert!(matches!(
            mock.read(content_id, 0, &mut buf),
            Err(FsError::Io(_))
        ));

        assert_eq!(mock.call_count("read"), 6);
    }

    #[test]
    fn mock_storage_reset_counters() {
        let base = Arc::new(InMemoryBackend::new());
        let mock = MockStorageBackend::new(base);

        mock.allocate(b"test1").unwrap();
        mock.allocate(b"test2").unwrap();
        assert_eq!(mock.call_count("allocate"), 2);

        mock.reset_counters();
        assert_eq!(mock.call_count("allocate"), 0);

        mock.allocate(b"test3").unwrap();
        assert_eq!(mock.call_count("allocate"), 1);
    }

    #[test]
    fn mock_storage_runtime_behavior_change() {
        let base = Arc::new(InMemoryBackend::new());
        let mock = MockStorageBackend::new(base);

        let content_id = mock.allocate(b"test").unwrap();

        // Initially succeeds
        assert!(mock.write(content_id, 0, b"a").is_ok());

        // Change behavior to always fail writes
        mock.set_behavior(FailureBehavior::AlwaysFail {
            op: "write",
            error_fn: Arc::new(enospc_error),
        });

        // Now fails
        assert!(matches!(
            mock.write(content_id, 1, b"b"),
            Err(FsError::NoSpace)
        ));

        // Change back to always succeed
        mock.set_behavior(FailureBehavior::AlwaysSucceed);

        // Works again (note: counter continues from before)
        assert!(mock.write(content_id, 2, b"c").is_ok());
    }
}
