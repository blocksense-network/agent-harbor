// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! RAII guard that manages [`TestLogger`] lifecycle for test macros.

use std::path::PathBuf;

use crate::{TestLogError, TestLogger};

/// Guard ensuring that each test finalizes its log correctly.
///
/// The guard calls [`TestLogger::finish_success`] when `finish_success` is
/// invoked explicitly. If the guard is dropped without being marked as
/// completed, or during a panic unwind, it records the failure via
/// [`TestLogger::finish_failure`].
pub struct TestLoggerGuard {
    logger: Option<TestLogger>,
    log_path: PathBuf,
    completed: bool,
}

impl TestLoggerGuard {
    /// Create a new guard for the given test name.
    pub fn new(test_name: &str) -> Result<Self, TestLogError> {
        let logger = TestLogger::new(test_name)?;
        let log_path = logger.log_path().to_path_buf();
        Ok(Self {
            logger: Some(logger),
            log_path,
            completed: false,
        })
    }

    /// Borrow the underlying logger for writing test diagnostics.
    pub fn logger(&mut self) -> &mut TestLogger {
        self.logger.as_mut().expect("TestLoggerGuard logger already finalized")
    }

    /// Mark the test as successful and finalize the log.
    pub fn finish_success(mut self) -> Result<PathBuf, TestLogError> {
        self.completed = true;
        if let Some(logger) = self.logger.take() {
            logger.finish_success()
        } else {
            Ok(self.log_path.clone())
        }
    }

    /// Mark the test as failed with a message and finalize the log.
    pub fn finish_failure<S: AsRef<str>>(mut self, message: S) -> Result<PathBuf, TestLogError> {
        self.completed = true;
        if let Some(logger) = self.logger.take() {
            logger.finish_failure(message.as_ref())
        } else {
            Ok(self.log_path.clone())
        }
    }

    /// Retrieve the path to the log file without finalizing the guard.
    pub fn log_path(&self) -> &PathBuf {
        &self.log_path
    }
}

impl Drop for TestLoggerGuard {
    fn drop(&mut self) {
        if self.completed {
            return;
        }

        if let Some(logger) = self.logger.take() {
            let reason = if std::thread::panicking() {
                "test panicked"
            } else {
                "test exited without calling finish_success()"
            };

            if let Err(err) = logger.finish_failure(reason) {
                eprintln!(
                    "failed to finalize TestLogger in Drop for {}: {}",
                    self.log_path.display(),
                    err
                );
            }
        }
    }
}
