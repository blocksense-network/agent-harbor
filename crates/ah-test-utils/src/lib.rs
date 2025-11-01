// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Agent Harbor Test Utilities
//!
//! This crate provides unified testing infrastructure and helpers for all Agent Harbor tests.
//! It implements the project's testing guidelines from CLAUDE.md, specifically:
//!
//! - Each test MUST create a unique log file capturing its full output
//! - On success: tests print minimal output to keep logs out of AI context windows
//! - On failure: tests print log path and file size for investigation
//! - Tests should be defensive and handle all potential errors

pub mod logging;
pub mod macros;

pub use logging::{TestLogError, TestLogger, create_unique_test_log};

// Macros are automatically available at the crate root via #[macro_export]

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Test that the unified logging system creates unique log files
    /// and properly manages test output according to project guidelines.
    #[test]
    fn test_logging_creates_unique_files() {
        let log_path1 = create_unique_test_log("test_logging_creates_unique_files_1");
        let log_path2 = create_unique_test_log("test_logging_creates_unique_files_2");

        // Verify paths are different
        assert_ne!(log_path1, log_path2);

        // Verify directories exist
        assert!(log_path1.parent().unwrap().exists());
        assert!(log_path2.parent().unwrap().exists());

        // Verify files can be created
        fs::write(&log_path1, "test content 1").unwrap();
        fs::write(&log_path2, "test content 2").unwrap();

        // Verify content is different
        let content1 = fs::read_to_string(&log_path1).unwrap();
        let content2 = fs::read_to_string(&log_path2).unwrap();
        assert_eq!(content1, "test content 1");
        assert_eq!(content2, "test content 2");

        // Clean up
        fs::remove_file(&log_path1).unwrap();
        fs::remove_file(&log_path2).unwrap();

        println!("✅ test_logging_creates_unique_files passed");
    }

    /// Test that TestLogger properly captures output and handles success/failure scenarios
    /// according to the project's testing guidelines.
    #[test]
    fn test_logger_success_and_failure_patterns() {
        let mut logger = TestLogger::new("test_logger_success_and_failure_patterns").unwrap();

        // Test writing to log
        logger.log("Starting test operation").unwrap();
        logger.log("Performing intermediate step").unwrap();
        logger.log("Operation completed successfully").unwrap();

        // Test success pattern (minimal stdout output)
        let result = logger.finish_success();
        assert!(result.is_ok());

        println!("✅ test_logger_success_and_failure_patterns passed");
    }
}
