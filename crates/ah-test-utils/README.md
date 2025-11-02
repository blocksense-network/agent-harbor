# Agent Harbor Test Utilities (ah-test-utils)

A unified testing infrastructure crate that implements Agent Harbor's testing guidelines for consistent, AI-friendly test logging.

## Overview

This crate provides standardized testing utilities that follow the project's testing guidelines from `CLAUDE.md`:

1. **Each test MUST create a unique log file** capturing its full output
2. **On success**: tests print minimal output to keep logs out of AI context windows
3. **On failure**: tests print log path and file size for investigation
4. **Tests should be automated** and defensive with proper error handling

## Key Features

- **Unified logging**: Consistent log file creation and management
- **AI-friendly output**: Preserves context-budget for AI tools
- **Defensive error handling**: Comprehensive error reporting
- **Convenient macros**: Simplified adoption for developers
- **Structured logging**: Support for JSON data logging

## Quick Start

### Basic Usage with Attribute Macros

```rust
use ah_test_utils::logged_test;

#[logged_test]
fn test_my_feature() {
    logger.log("Starting my feature test").unwrap();

    // Your test logic here
    let result = my_function();

    if result.is_ok() {
        logger.log("Feature test completed successfully").unwrap();
    } else {
        logger.log("Feature test failed").unwrap();
        panic!("Test failed");
    }
}
```

### Manual Control with `TestLoggerGuard`

```rust
use ah_test_utils::TestLoggerGuard;

#[test]
fn test_with_manual_control() {
    let mut guard = TestLoggerGuard::new("test_with_manual_control").unwrap();
    guard.logger().log("Running manual control example").unwrap();
    // ... perform test logic ...
    guard.logger().log("Manual control example completed").unwrap();

    guard.finish_success().unwrap();
}
```

### Using Assertion Helpers

```rust
use ah_test_utils::{logged_assert, logged_assert_eq};

#[ah_test_utils::logged_test]
fn test_my_feature_simple() {
    logger.log("Testing my feature with helpers").unwrap();

    let result = my_function();
    logged_assert!(logger, result.is_ok(), "Function should succeed");

    let value = result.unwrap();
    logged_assert_eq!(logger, value, expected_value);

    logger.log("Test completed successfully").unwrap();
}
```

## Log File Structure

Test logs are organized hierarchically under `target/test-logs/`:

```
target/test-logs/
├── 2025-11-01/
│   ├── test_my_feature-14-30-45-uuid.log
│   ├── test_another_feature-14-31-02-uuid.log
│   └── ...
├── 2025-11-02/
│   └── ...
```

Each log file contains:

```
=== Agent Harbor Test Log ===
Test: test_my_feature
Started: 2025-11-01 14:30:45 UTC
Process: 12345
Thread: main
=== Log Output ===

[14:30:45.123] Starting my feature test
[14:30:45.124] Testing component initialization
[14:30:45.125] ✓ Assertion passed
[14:30:45.126] Test completed successfully in 0.003s
```

## API Reference

### TestLogger

The main logging interface that manages test output according to project guidelines.

#### Methods

- `TestLogger::new(test_name: &str) -> Result<TestLogger, TestLogError>`
  - Creates a new test logger with unique log file
  - Automatically writes test metadata header

- `logger.log(message: &str) -> Result<(), TestLogError>`
  - Logs a timestamped message to the test log file

- `logger.log_json<T>(label: &str, data: &T) -> Result<(), TestLogError>`
  - Logs structured data as formatted JSON

- `logger.finish_success() -> Result<PathBuf, TestLogError>`
  - Completes test successfully with minimal stdout output
  - Returns path to log file

- `logger.finish_failure(error: &str) -> Result<PathBuf, TestLogError>`
  - Completes test with failure, printing log path and size
  - Returns path to log file for investigation

### Macros

#### `logged_assert!(logger, condition, message)`

Logs assertion attempts and results, panics on failure.

#### `logged_assert_eq!(logger, left, right, message)`

Logs equality assertions and results, panics on failure.

### Helper Functions

#### `create_unique_test_log(test_name: &str) -> PathBuf`

Creates a unique log file path following the project's directory structure.

## Integration Examples

### CLI Tests

```rust
use ah_cli::{Cli, Commands};
use ah_test_utils::TestLogger;

#[test]
fn test_cli_parsing() {
    let mut logger = TestLogger::new("test_cli_parsing").unwrap();

    logger.log("Testing CLI argument parsing").unwrap();

    let args = vec!["ah", "agent", "start", "--task", "example"];
    logger.log(&format!("Parsing args: {:?}", args)).unwrap();

    match Cli::try_parse_from(args) {
        Ok(cli) => {
            logger.log("CLI parsing succeeded").unwrap();
            logger.log_json("parsed_command", &cli.command).unwrap();
            logger.finish_success().unwrap();
        },
        Err(e) => {
            logger.finish_failure(&format!("CLI parsing failed: {}", e)).unwrap();
            panic!("CLI parsing failed: {}", e);
        }
    }
}
```

### Integration Tests

use ah_test_utils::logged_tokio_test;

# [logged_tokio_test]

async fn test_end_to_end_workflow() {
logger.log("Starting end-to-end workflow test").unwrap();

    // Setup
    let session = create_test_session().await;
    logger.log(&format!("Created test session: {}", session.id)).unwrap();

    // Execute
    let task = create_task(&session, "test task").await;
    logger.log(&format!("Created task: {}", task.id)).unwrap();

    let result = execute_task(&task).await;
    if result.is_err() {
        logger.log("Task execution reported error").unwrap();
        panic!("Task execution should succeed");
    }

    // Verify
    let final_state = get_session_state(&session).await;
    assert_eq!(final_state.status, SessionStatus::Completed);

    logger.log("End-to-end workflow test completed successfully").unwrap();

}

````

## Benefits

### For Developers

- **Consistent patterns**: Standardized logging across all tests
- **Easy adoption**: Simple API and helpful macros
- **Better debugging**: Detailed logs with timestamps and structure
- **Error visibility**: Clear failure reporting with log paths

### For AI Agents

- **Context preservation**: Minimal stdout keeps context windows clean
- **Full fidelity**: Complete logs available in files when needed
- **Structured data**: JSON logging for complex test data
- **Clear failures**: Log paths and sizes for direct investigation

### For CI/CD

- **Automated friendly**: No interactive processes required
- **Failure analysis**: Log files preserved for post-mortem analysis
- **Performance tracking**: Test timing and metadata captured
- **Space efficient**: Organized log directory structure

## Best Practices

### DO

- Use `TestLogger::new()` at the start of every test
- Log test intentions and intermediate steps
- Use `logged_assert!` macros for better traceability
- Call `finish_success()` or `finish_failure()` appropriately
- Log structured data with `log_json()` when helpful

### DON'T

- Use `println!` or `eprintln!` in tests (use `logger.log()`)
- Forget to call finish methods (use macros to avoid this)
- Create tests that require manual interaction
- Skip logging for "simple" tests (consistency is key)
- Disable assertions to make tests pass

### Migration Guide

To migrate existing tests:

1. Add `ah-test-utils` to `[dev-dependencies]`
2. Import `TestLogger` or macros
3. Replace `println!` with `logger.log()`
4. Add success/failure handling
5. Use `logged_assert!` for better traceability

#### Before

```rust
#[test]
fn test_feature() {
    println!("Testing feature");
    let result = my_function();
    assert!(result.is_ok());
    println!("Test passed");
}
````

#### After

```rust
#[ah_test_utils::logged_test]
fn test_feature() {
    logger.log("Testing feature").unwrap();
    let result = my_function();
    logged_assert!(logger, result.is_ok(), "Function should succeed");
    logger.log("Test passed").unwrap();
}
```

## Project Integration

This crate is designed to be the foundation for all Agent Harbor testing. It implements the testing guidelines from `CLAUDE.md` and provides the infrastructure needed for:

- **Test Coverage Improvement Plan**: Foundational logging for new tests
- **AI Agent Development**: Context-aware test output management
- **Quality Assurance**: Consistent testing patterns across the codebase
- **Debugging Support**: Comprehensive failure analysis

## Contributing

When adding new testing utilities:

1. Follow the established patterns in this crate
2. Ensure compatibility with existing TestLogger interface
3. Add comprehensive tests for new functionality
4. Update documentation and examples
5. Consider AI agent context management in design decisions

## References

- `CLAUDE.md` - Project testing guidelines (lines 52-59)
- `docs/Test-Coverage-Improvement-Plan.md` - Testing strategy
- `crates/ah-mux/tests/README.md` - Example test patterns
- `specs/Public/Testing-Architecture.md` - Testing architecture
