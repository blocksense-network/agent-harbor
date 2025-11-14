# Logging Guidelines

## Overview

This document establishes the logging standards for the Agent Harbor codebase. Consistent logging practices are essential for debugging issues on end-user machines and cloud agent environments. These guidelines ensure that logs provide actionable diagnostic information while maintaining performance and security.

## Log Levels

Use the following log levels with these semantics:

### ERROR

- **Purpose**: Report failures that prevent core functionality from working
- **Examples**:
  - Database connection failures
  - Agent launch failures that cannot be recovered
  - Critical system resource exhaustion
- **When to use**: The application cannot continue with the current operation
- **Impact**: Users should be notified or the operation should fail

### WARN

- **Purpose**: Report recoverable issues or unexpected conditions that may indicate problems
- **Examples**:
  - Agent credential detection failures with fallbacks
  - Deprecated configuration usage
  - Network timeouts with automatic retry
- **When to use**: Something is wrong but the application can continue
- **Impact**: May require user attention or configuration changes

### INFO

- **Purpose**: Report significant application lifecycle events and state changes
- **Examples**:
  - Server startup/shutdown
  - Task launch/completion
  - Major configuration changes
- **When to use**: Normal operation milestones that users or operators care about
- **Impact**: Provides visibility into application behavior

### DEBUG

- **Purpose**: Report detailed internal state for development and troubleshooting
- **Examples**:
  - Function entry/exit with parameters
  - Configuration loading details
  - Connection establishment/teardown
- **When to use**: Information useful for developers debugging issues
- **Impact**: Verbose output that may be too noisy for normal operation

### TRACE

- **Purpose**: Report low-level internal details for deep debugging
- **Examples**:
  - Individual bytes in network protocols
  - Internal state machine transitions
  - Memory allocation details
- **When to use**: Extremely detailed debugging (compile-time disabled in release)
- **Impact**: Very high volume, performance impact in debug builds

## Log Level Selection Criteria

When choosing a log level, consider:

1. **Who is the audience?**
   - ERROR/WARN: End users, operators, support teams
   - INFO: Operators, monitoring systems
   - DEBUG: Developers, support engineers
   - TRACE: Core developers, detailed investigations

2. **What is the frequency?**
   - ERROR: Rare (failures)
   - WARN: Occasional (issues)
   - INFO: Regular but significant events
   - DEBUG: Frequent but bounded
   - TRACE: Very frequent, unbounded

3. **What is the performance impact?**
   - TRACE logs are compiled out in release builds
   - DEBUG logs may impact performance in high-throughput scenarios
   - INFO and above should have minimal performance impact

## Structured Logging

### Field Naming Conventions

Use consistent field names across the codebase:

- `component`: The crate or module name (e.g., `"ah-cli"`, `"ah-core"`)
- `operation`: The operation being performed (e.g., `"launch_agent"`, `"create_task"`)
- `user_id`: User identifier (redact if sensitive)
- `task_id`: Task/session identifier
- `agent_type`: Agent backend (e.g., `"claude"`, `"codex"`)
- `error`: Error message (use Display formatting)
- `error_details`: Additional error context
- `duration_ms`: Operation duration in milliseconds
- `file_path`: File system paths (redact sensitive paths)
- `url`: URLs (redact credentials)

### Correlation IDs

Include correlation IDs for request tracing:

```rust
use tracing::{info, instrument};

#[instrument(fields(operation = "launch_task", task_id = %task.id()))]
async fn launch_task(task: &Task) -> Result<(), Error> {
    info!("Starting task launch");
    // ... implementation ...
    info!("Task launch completed");
}
```

### Sensitive Data Handling

Never log sensitive information:

- API keys, passwords, tokens
- Personal user data
- File contents of configuration files
- Internal system paths that could be used for attacks

Use redaction helpers:

```rust
use ah_logging::redact;

let api_key = "sk-1234567890abcdef";
info!(api_key = %redact(api_key), "API key configured");
// Output: api_key="[REDACTED]"
```

## Default Log Levels

### Development Environment

- Default level: `debug`
- Enable `trace` with `RUST_LOG=trace` for detailed debugging
- Include source location information

### Release Environment

- Default level: `info`
- Suppress `debug` logs (`trace` logs are eliminated at compile-time)
- Optimize for performance and readability

### Binary-Specific Defaults

- **ah (CLI)**: `info` - Command-line operations should be visible
- **ah-rest-server**: `info` - API operations and errors
- **ah agent start**: `info` - Agent lifecycle events
- **AgentFS daemons**: `warn` - Only log issues, not normal operation
- **Recorder**: `debug` - Recording operations need detailed logging

## Configuration

### Environment Variables

- `RUST_LOG`: Standard tracing filter syntax (recommended for advanced users)
  - `RUST_LOG=info` - Default level
  - `RUST_LOG=ah_core=debug,ah_cli=info` - Component-specific levels
  - `RUST_LOG=trace,ah_agents=off` - Global trace but disable specific components

- `AH_LOG`: Agent Harbor specific log configuration (deprecated, prefer `RUST_LOG`)

### CLI Arguments

Most binaries accept a `--log-level` argument to control verbosity. When using clap's `ValueEnum`, it automatically provides help and validation:

```bash
# Run with debug logging
ah --log-level debug tui

# Run server with warning-only logging
ah-rest-server --log-level warn

# Get help for available log levels
ah --help  # Shows: --log-level <LOG_LEVEL>    [possible values: error, warn, info, debug, trace]
```

Available levels: `error`, `warn`, `info`, `debug`, `trace`

### Configuration File

```toml
log-level = "info"
log-format = "json"  # "json" or "text"
log-file-sources = false  # Include file:line in development

[log-level-by-component]
ah_core = "debug"
ah_agents = "warn"
```

## Output Formats

### Text Format (Development)

```
2024-01-15T10:30:45Z INFO  ah_core::task_manager: Task launched task_id=123 user_id=user@example.com
```

### JSON Format (Production)

```json
{
  "timestamp": "2024-01-15T10:30:45Z",
  "level": "INFO",
  "target": "ah_core::task_manager",
  "fields": {
    "message": "Task launched",
    "task_id": "123",
    "user_id": "user@example.com"
  }
}
```

## Performance Considerations

### Compile-Time Optimizations

- `trace!` macros are compiled out in release builds
- Use `debug_assertions` to conditionally enable expensive logging

```rust
#[cfg(debug_assertions)]
trace!("Expensive trace: {}", expensive_computation());
```

### Runtime Optimizations

- Avoid string formatting in hot paths for disabled log levels
- Use structured fields instead of string interpolation when possible
- Consider log sampling for high-frequency operations

## Implementation Guidelines

### Initialization

Choose the log level and format from CLI arguments or configuration, then use centralized logging initialization:

```rust
use ah_domain_types::CliLogLevel;
use ah_logging::{init, Level, LogFormat};
use clap::Parser;

#[derive(Parser)]
struct Args {
    /// Log level
    #[arg(short, long, default_value = "info")]
    log_level: CliLogLevel,

    /// Log format
    #[arg(long, default_value = "plaintext")]
    format: LogFormat,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let args = Args::parse();

    // Convert CliLogLevel enum to tracing::Level
    let log_level = args.log_level.into();

    init("ah_cli", log_level, args.format)?;

    // ... application logic ...
}
```

For file-based logging:

```rust
use ah_logging::{init_to_file, Level, LogFormat};
use std::path::Path;

// Log to a specific file with JSON format
let log_path = Path::new("agent-harbor.log");
init_to_file("my-app", Level::INFO, LogFormat::Json, log_path)?;
```

For platform-specific standard log locations:

```rust
use ah_logging::{init_to_standard_file, Level, LogFormat};

// Log to platform-standard location (e.g., ~/Library/Logs/agent-harbor.log on macOS)
init_to_standard_file("my-app", Level::INFO, LogFormat::Plaintext)?;
```

For console output with default format:

```rust
use ah_logging::{init_plaintext, Level};

// Simple plaintext logging to console
init_plaintext("my-app", Level::INFO)?;
```

### Error Logging

When logging errors, include context:

```rust
match operation().await {
    Ok(result) => info!("Operation succeeded"),
    Err(e) => error!(error = %e, "Operation failed"),
}
```

### Async Operation Logging

Use `#[instrument]` for async functions:

```rust
#[instrument(skip(config), fields(operation = "launch_agent"))]
async fn launch_agent(config: &Config) -> Result<(), Error> {
    info!("Launching agent");
    // ... implementation ...
    info!("Agent launched successfully");
}
```

## Testing Logging Behavior

### Unit Tests

Test that appropriate log messages are emitted:

```rust
#[test]
fn test_operation_logs() {
    let logs = capture_logs();
    operation().await;
    assert!(logs.contains("Operation completed"));
}
```

### Integration Tests

Verify log levels and formats in CI:

```rust
#[test]
fn test_release_logging() {
    // Ensure no trace logs in release
    #[cfg(not(debug_assertions))]
    assert!(!logs_contain_level("TRACE"));
}

#[test]
fn test_configurable_log_levels() {
    // Test that CLI log level arguments work correctly
    let args = vec!["--log-level", "debug"];
    let config = parse_args(args);
    assert!(matches!(config.log_level, CliLogLevel::Debug));

    // Test level conversion
    let level: Level = CliLogLevel::Debug.into();
    assert_eq!(level, Level::DEBUG);
}
```

## Migration Guide

### From println!/eprintln! Functions

Replace direct console output:

```rust
// Before
println!("Task {} started", task_id);

// After
info!(task_id = %task_id, "Task started");
```

### From log crate

Migrate from `log` to `tracing`:

```rust
// Before
log::info!("Task started");

// After
tracing::info!("Task started");
```

### From Custom Logging

Consolidate custom logging implementations to use the shared `ah_logging` crate.

## Enforcement

### CI Checks

- Lint rules ensure `println!` is not used in production code
- Build checks verify trace logs are compiled out in release
- Integration tests validate log output formats
- CLI tests verify log level configuration works correctly

### Code Review Guidelines

- Reviewers should check that appropriate log levels are used
- Sensitive data logging should be flagged
- Structured logging should be preferred over string formatting
