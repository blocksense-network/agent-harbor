// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Convenient macros for unified test logging
//!
//! These macros make it easier for developers to adopt the project's testing guidelines
//! by providing simple wrappers around the TestLogger functionality.

/// Convenience macro to create a test function with unified logging
///
/// This macro automatically creates a TestLogger, handles success/failure cases,
/// and follows the project's testing guidelines.
///
/// # Example
/// ```rust
/// use ah_test_utils::logged_test;
///
/// logged_test!(test_my_feature, logger, {
///     logger.log("Testing my feature").unwrap();
///     
///     let result = my_function();
///     logged_assert_eq!(logger, result, expected_value);
///     
///     logger.log("Feature test completed successfully").unwrap();
/// });
/// ```
#[macro_export]
macro_rules! logged_test {
    ($test_name:ident, $logger_name:ident, $body:block) => {
        #[test]
        fn $test_name() {
            // Create test logger
            let mut $logger_name = match $crate::TestLogger::new(stringify!($test_name)) {
                Ok(logger) => logger,
                Err(e) => panic!("Failed to create test logger: {}", e),
            };
            
            // Execute the test body with logger in scope
            $body
            
            // If we reach here, test passed
            let _ = $logger_name.finish_success();
        }
    };
}

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
        logged_assert_eq!($logger, $left, $right, 
            &format!("assertion failed: `(left == right)`\n  left: `{:?}`,\n right: `{:?}`", $left, $right))
    };
    ($logger:expr, $left:expr, $right:expr, $message:expr) => {
        if let Err(e) = $logger.log(&format!("Asserting equality: {} == {}", 
            stringify!($left), stringify!($right))) {
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
    use super::*;
    use crate::TestLogger;

    // Note: This test demonstrates the macro but can't be run in the module
    // because the macro expects a logger variable to be available in scope

    #[test]
    fn test_macro_failure_handling() {
        // This test verifies that our macro properly handles failures
        // We can't use the logged_test! macro here because it would interfere
        // with the panic catching we need to do
        
        let mut logger = TestLogger::new("test_macro_failure_handling").unwrap();
        
        logger.log("Testing that macros handle failures correctly").unwrap();
        
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
        
        logger.finish_success().unwrap();
    }
}