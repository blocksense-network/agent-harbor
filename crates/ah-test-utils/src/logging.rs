// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Unified test logging infrastructure for Agent Harbor
//!
//! This module implements the project's testing guidelines from CLAUDE.md:
//!
//! 1. Each test MUST create a unique log file capturing its full output
//! 2. On success: tests print minimal output to keep logs out of AI context windows  
//! 3. On failure: tests print log path and file size for investigation
//! 4. Tests should be automated and defensive with proper error handling
//!
//! The logging pattern preserves context-budget for AI tools by avoiding large inline logs,
//! while retaining full fidelity in files for developers and agents to examine directly.

use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use thiserror::Error;
use uuid::Uuid;

/// Errors that can occur during test logging operations
#[derive(Error, Debug)]
pub enum TestLogError {
    #[error("Failed to create test log directory: {0}")]
    DirectoryCreation(#[from] std::io::Error),

    #[error("Failed to write to test log file: {path}")]
    WriteError { path: PathBuf },

    #[error("Invalid test name: {name}")]
    InvalidTestName { name: String },
}

/// Test logger that manages output according to project guidelines
pub struct TestLogger {
    log_path: PathBuf,
    writer: BufWriter<File>,
    test_name: String,
    start_time: DateTime<Utc>,
}

impl TestLogger {
    /// Create a new test logger for the specified test
    ///
    /// # Arguments
    /// * `test_name` - Name of the test (used for log file naming)
    ///
    /// # Returns
    /// * `Result<TestLogger, TestLogError>` - Logger instance or error
    ///
    /// # Example
    /// ```rust
    /// use ah_test_utils::TestLogger;
    ///
    /// let mut logger = TestLogger::new("test_example").unwrap();
    /// logger.log("Starting test operations").unwrap();
    /// // ... test logic ...
    /// logger.finish_success().unwrap();
    /// ```
    pub fn new(test_name: &str) -> Result<Self, TestLogError> {
        validate_test_name(test_name)?;

        let log_path = create_unique_test_log(test_name);
        let file = OpenOptions::new().create(true).write(true).truncate(true).open(&log_path)?;

        let writer = BufWriter::new(file);
        let start_time = Utc::now();

        let mut logger = Self {
            log_path,
            writer,
            test_name: test_name.to_string(),
            start_time,
        };

        // Write initial log header with test metadata
        logger.write_header()?;

        Ok(logger)
    }

    /// Log a message to the test log file
    ///
    /// # Arguments
    /// * `message` - Message to log
    ///
    /// # Returns
    /// * `Result<(), TestLogError>` - Success or error
    pub fn log(&mut self, message: &str) -> Result<(), TestLogError> {
        let timestamp = Utc::now().format("%H:%M:%S%.3f");
        writeln!(self.writer, "[{}] {}", timestamp, message).map_err(|_| {
            TestLogError::WriteError {
                path: self.log_path.clone(),
            }
        })?;
        self.writer.flush().map_err(|_| TestLogError::WriteError {
            path: self.log_path.clone(),
        })?;
        Ok(())
    }

    /// Return the path to the underlying log file.
    pub fn log_path(&self) -> &Path {
        &self.log_path
    }

    /// Log structured data as JSON
    ///
    /// # Arguments
    /// * `label` - Label for the data
    /// * `data` - Serializable data to log
    ///
    /// # Returns
    /// * `Result<(), TestLogError>` - Success or error
    pub fn log_json<T: serde::Serialize>(
        &mut self,
        label: &str,
        data: &T,
    ) -> Result<(), TestLogError> {
        let json = serde_json::to_string_pretty(data).map_err(|_| TestLogError::WriteError {
            path: self.log_path.clone(),
        })?;
        self.log(&format!("{}: {}", label, json))
    }

    /// Finish test successfully with minimal stdout output
    ///
    /// According to project guidelines, successful tests should print minimal output
    /// to keep logs out of AI context windows.
    ///
    /// # Returns
    /// * `Result<PathBuf, TestLogError>` - Log file path or error
    pub fn finish_success(mut self) -> Result<PathBuf, TestLogError> {
        let end_time = Utc::now();
        let duration = end_time.signed_duration_since(self.start_time);

        self.log(&format!(
            "Test completed successfully in {:.3}s",
            duration.num_milliseconds() as f64 / 1000.0
        ))?;

        // Flush and close the file
        self.writer.flush().map_err(|_| TestLogError::WriteError {
            path: self.log_path.clone(),
        })?;
        drop(self.writer);

        // Minimal stdout output for successful tests
        println!("✅ {} passed", self.test_name);

        Ok(self.log_path)
    }

    /// Finish test with failure, printing log path and size for investigation
    ///
    /// According to project guidelines, failed tests should print the log path
    /// and file size so developers (or agents) can open them directly.
    ///
    /// # Arguments
    /// * `error_message` - Description of the failure
    ///
    /// # Returns
    /// * `Result<PathBuf, TestLogError>` - Log file path or error
    pub fn finish_failure(mut self, error_message: &str) -> Result<PathBuf, TestLogError> {
        let end_time = Utc::now();
        let duration = end_time.signed_duration_since(self.start_time);

        self.log(&format!(
            "Test failed after {:.3}s: {}",
            duration.num_milliseconds() as f64 / 1000.0,
            error_message
        ))?;

        // Flush and close the file
        self.writer.flush().map_err(|_| TestLogError::WriteError {
            path: self.log_path.clone(),
        })?;
        drop(self.writer);

        // Print log path and size for investigation
        if let Ok(metadata) = fs::metadata(&self.log_path) {
            println!(
                "❌ {} failed - Log: {} ({} bytes)",
                self.test_name,
                self.log_path.display(),
                metadata.len()
            );
        } else {
            println!(
                "❌ {} failed - Log: {}",
                self.test_name,
                self.log_path.display()
            );
        }

        Ok(self.log_path)
    }

    /// Write the log file header with test metadata
    fn write_header(&mut self) -> Result<(), TestLogError> {
        writeln!(self.writer, "=== Agent Harbor Test Log ===")?;
        writeln!(self.writer, "Test: {}", self.test_name)?;
        writeln!(
            self.writer,
            "Started: {}",
            self.start_time.format("%Y-%m-%d %H:%M:%S UTC")
        )?;
        writeln!(self.writer, "Process: {}", std::process::id())?;

        if let Ok(thread_name) =
            std::thread::current().name().ok_or("unknown").map(|s| s.to_string())
        {
            writeln!(self.writer, "Thread: {}", thread_name)?;
        }

        writeln!(self.writer, "=== Log Output ===")?;
        writeln!(self.writer)?;

        self.writer.flush().map_err(|_| TestLogError::WriteError {
            path: self.log_path.clone(),
        })?;
        Ok(())
    }
}

/// Create a unique test log file path for the specified test name
///
/// This function implements the project requirement that each test MUST create
/// a unique log file. The log files are organized in a hierarchical structure
/// under the `target/test-logs` directory.
///
/// # Arguments
/// * `test_name` - Name of the test
///
/// # Returns
/// * `PathBuf` - Unique path for the test log file
///
/// # Path Structure
/// ```
/// target/test-logs/
/// ├── YYYY-MM-DD/
/// │   ├── test-name-HH-MM-SS-uuid.log
/// │   └── another-test-HH-MM-SS-uuid.log
/// └── ...
/// ```
///
/// # Example
/// ```rust
/// use ah_test_utils::create_unique_test_log;
///
/// let log_path = create_unique_test_log("test_example");
/// println!("Log file: {}", log_path.display());
/// ```
pub fn create_unique_test_log(test_name: &str) -> PathBuf {
    let workspace_root = find_workspace_root();
    let now = Utc::now();
    let date_str = now.format("%Y-%m-%d");
    let time_str = now.format("%H-%M-%S");
    let uuid = Uuid::new_v4();

    let log_dir = workspace_root.join("target").join("test-logs").join(date_str.to_string());

    // Ensure log directory exists
    fs::create_dir_all(&log_dir).unwrap_or_else(|e| {
        panic!(
            "Failed to create test log directory {}: {}",
            log_dir.display(),
            e
        );
    });

    // Create unique filename with timestamp and UUID
    let sanitized_name = sanitize_filename(test_name);
    let filename = format!("{}-{}-{}.log", sanitized_name, time_str, uuid);

    log_dir.join(filename)
}

/// Find the workspace root directory by looking for Cargo.toml
fn find_workspace_root() -> PathBuf {
    let current_dir = env::current_dir().expect("Failed to get current directory");

    let mut dir = current_dir.as_path();

    loop {
        let cargo_toml = dir.join("Cargo.toml");
        if cargo_toml.exists() {
            // Check if this is the workspace root by looking for [workspace] section
            if let Ok(content) = fs::read_to_string(&cargo_toml) {
                if content.contains("[workspace]") {
                    return dir.to_path_buf();
                }
            }
        }

        if let Some(parent) = dir.parent() {
            dir = parent;
        } else {
            // Fallback to current directory if workspace root not found
            return current_dir;
        }
    }
}

/// Sanitize a test name for use as a filename
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-' => c,
            _ => '_',
        })
        .collect()
}

/// Validate that a test name is suitable for use in logging
fn validate_test_name(name: &str) -> Result<(), TestLogError> {
    if name.is_empty() {
        return Err(TestLogError::InvalidTestName {
            name: name.to_string(),
        });
    }

    if name.len() > 200 {
        return Err(TestLogError::InvalidTestName {
            name: format!("Name too long: {} chars", name.len()),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    // use tempfile::TempDir; // Unused for now

    #[crate::logged_test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("test_normal"), "test_normal");
        assert_eq!(sanitize_filename("test-with-dashes"), "test-with-dashes");
        assert_eq!(sanitize_filename("test with spaces"), "test_with_spaces");
        assert_eq!(sanitize_filename("test/with/slashes"), "test_with_slashes");
        assert_eq!(sanitize_filename("test:with:colons"), "test_with_colons");
    }

    #[crate::logged_test]
    fn test_validate_test_name() {
        assert!(validate_test_name("valid_test").is_ok());
        assert!(validate_test_name("").is_err());
        assert!(validate_test_name(&"x".repeat(201)).is_err());
    }

    #[crate::logged_test]
    fn test_create_unique_test_log_creates_different_paths() {
        let path1 = create_unique_test_log("test1");
        let path2 = create_unique_test_log("test2");

        assert_ne!(path1, path2);
        assert!(path1.to_string_lossy().contains("test1"));
        assert!(path2.to_string_lossy().contains("test2"));
    }
}
