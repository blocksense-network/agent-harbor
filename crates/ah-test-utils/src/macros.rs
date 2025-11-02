// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Convenient macros for unified test logging
//!
//! These macros make it easier for developers to adopt the project's testing guidelines
//! by providing simple wrappers around the TestLogger functionality.

/// Logged assertion macro that logs both success and failure
///
/// This macro logs the assertion being performed and its result to the test log.
///
/// # Example
/// ```rust
/// logged_assert!(logger, value == expected, "Values should be equal");
/// ```
#[macro_export]
macro_rules! logged_assert {
    ($logger:expr, $condition:expr) => {
        logged_assert!($logger, $condition, stringify!($condition))
    };
    ($logger:expr, $condition:expr, $message:expr) => {
        if let Err(e) = $logger.log(&format!("Asserting: {}", $message)) {
            eprintln!("Warning: Failed to write to test log: {}", e);
        }

        if $condition {
            if let Err(e) = $logger.log("✓ Assertion passed") {
                eprintln!("Warning: Failed to write to test log: {}", e);
            }
        } else {
            if let Err(e) = $logger.log("✗ Assertion failed") {
                eprintln!("Warning: Failed to write to test log: {}", e);
            }
            panic!("Assertion failed: {}", $message);
        }
    };
}

/// Logged equality assertion macro
///
/// This macro logs the equality assertion being performed and its result.
///
/// # Example
/// ```rust
/// logged_assert_eq!(logger, actual_value, expected_value);
/// ```
#[macro_export]
macro_rules! logged_assert_eq {
    ($logger:expr, $left:expr, $right:expr) => {
        logged_assert_eq!(
            $logger,
            $left,
            $right,
            &format!(
                "assertion failed: `(left == right)`\n  left: `{:?}`,\n right: `{:?}`",
                $left, $right
            )
        )
    };
    ($logger:expr, $left:expr, $right:expr, $message:expr) => {
        if let Err(e) = $logger.log(&format!(
            "Asserting equality: {} == {}",
            stringify!($left),
            stringify!($right)
        )) {
            eprintln!("Warning: Failed to write to test log: {}", e);
        }

        if $left == $right {
            if let Err(e) = $logger.log("✓ Equality assertion passed") {
                eprintln!("Warning: Failed to write to test log: {}", e);
            }
        } else {
            if let Err(e) = $logger.log(&format!("✗ Equality assertion failed: {}", $message)) {
                eprintln!("Warning: Failed to write to test log: {}", e);
            }
            panic!("{}", $message);
        }
    };
}

#[cfg(test)]
mod tests {
    use crate::TestLogger;

    #[crate::logged_test]
    fn test_assertion_helpers() {
        logger.log("Testing logged assertion helpers").unwrap();

        // Test logged_assert failure
        let result = std::panic::catch_unwind(|| {
            let mut inner_logger = TestLogger::new("inner_test").unwrap();
            logged_assert!(inner_logger, false, "This should fail");
        });

        assert!(result.is_err(), "logged_assert should have panicked");
        logger.log("Verified logged_assert failure handling").unwrap();

        // Test logged_assert_eq failure
        let result = std::panic::catch_unwind(|| {
            let mut inner_logger = TestLogger::new("inner_test_2").unwrap();
            logged_assert_eq!(inner_logger, 1, 2);
        });

        assert!(result.is_err(), "logged_assert_eq should have panicked");
        logger.log("Verified logged_assert_eq failure handling").unwrap();
    }
}
