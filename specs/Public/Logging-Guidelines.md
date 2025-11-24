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

## Command-Line Interface Policy

All Agent Harbor binaries MUST follow a consistent logging CLI interface. This ensures users can reliably control logging behavior across the entire software suite.

### Required CLI Parameters

Every Agent Harbor binary MUST support these logging parameters:

#### `--log-level <LEVEL>`

Controls the verbosity of log output.

**Required Values:**

- `error` - Only error conditions
- `warn` - Errors and warnings
- `info` - Errors, warnings, and informational messages (default for release builds)
- `debug` - All above plus debug information
- `trace` - All above plus detailed tracing

**Default Values by Binary Type:**

- **CLI tools** (`ah`, `ah-cli`): `info`
- **Servers/daemons** (`ah-rest-server`, `fs-snapshots-daemon`): `info`
- **TUI applications** (`ah-tui`): `info`
- **Agent binaries**: `info`
- **Test/development binaries**: `debug`

**Implementation:**

```rust
#[derive(clap::ValueEnum, Clone, Debug)]
pub enum CliLogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl From<CliLogLevel> for tracing::Level {
    fn from(level: CliLogLevel) -> Self {
        match level {
            CliLogLevel::Error => Level::ERROR,
            CliLogLevel::Warn => Level::WARN,
            CliLogLevel::Info => Level::INFO,
            CliLogLevel::Debug => Level::DEBUG,
            CliLogLevel::Trace => Level::TRACE,
        }
    }
}
```

### Automatic Output Destination Selection

Log output destination is determined automatically based on binary type and provided options:

#### **TUI Binaries (Interactive Applications)**

- **Always log to file** - No console output to avoid cluttering the interface
- Use `--log-file` and `--log-dir` to customize the log location
- Default to platform-standard log location if no file options provided

#### **Non-TUI Binaries (Servers, CLI Tools, Daemons)**

- **Log to console by default** - Output goes to stderr in plaintext format
- **Log to file when file options provided** - When `--log-file` or `--log-dir` (or both) are specified, output goes to file in JSON format

**Platform-Specific Log Locations:**

- **macOS**: `~/Library/Logs/agent-harbor/{binary-name}.log`
- **Linux**: `~/.local/share/agent-harbor/{binary-name}.log`
- **Windows**: `%APPDATA%\agent-harbor\{binary-name}.log`

#### Optional CLI Parameters

These parameters control log output format and file location:

#### `--log-format <FORMAT>`

Controls the output format for log messages.

**Values:**

- `plaintext` - Human-readable format (default)
- `json` - Structured JSON format

#### `--log-file <PATH>`

Specifies a custom log file path.

**Behavior:**

- For TUI binaries: Customizes the log file location (always logs to file)
- For other binaries: Enables file logging and specifies the filename
- Supports absolute and relative paths
- Directory components are resolved relative to `--log-dir` if specified

#### `--log-dir <DIRECTORY>`

Specifies the directory for log files.

**Behavior:**

- For TUI binaries: Customizes the log directory (always logs to file)
- For other binaries: Enables file logging and specifies the directory
- Used as the base directory when `--log-file` contains relative paths
- Defaults to platform-specific log directory when file logging is enabled

### CLI Usage Examples

#### **TUI Applications (Always log to file)**

```bash
# Basic usage - logs to platform-standard location
./ah-tui

# Debug logging to default file location
./ah-tui --log-level debug

# Custom log file location
./ah-tui --log-dir /var/log --log-file ah-tui.log
```

#### **CLI Tools (Log to console by default)**

```bash
# Basic usage - console output with info level
./ah-cli command

# Debug logging to console
./ah-cli --log-level debug command

# Enable file logging
./ah-cli --log-file ah-cli.log command

# File logging with custom directory
./ah-cli --log-dir /tmp/logs --log-file debug.log command
```

#### **Servers/Daemons (Log to console by default)**

```bash
# Basic usage - console output
./ah-rest-server

# Enable file logging
./ah-rest-server --log-file server.log

# JSON logging for production
./ah-rest-server --log-file server.log --log-format json
```

### Implementation Requirements

All binaries MUST use the standardized logging initialization helpers from `ah_logging`:

#### For Basic CLI Tools (Console logging by default)

```rust
use ah_logging::CliLoggingArgs;
use clap::Parser;

#[derive(Parser)]
struct Args {
    #[command(flatten)]
    logging: CliLoggingArgs,

    // ... other args
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    args.logging.init("my-binary", false)?;  // false = not a TUI binary
    // ... application logic
}
```

#### For TUI Applications (File logging by default)

```rust
use ah_logging::CliLoggingArgs;
use clap::Parser;

#[derive(Parser)]
struct Args {
    #[command(flatten)]
    logging: CliLoggingArgs,

    // ... other args
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    args.logging.init("my-tui", true)?;  // true = TUI binary, always logs to file
    // ... application logic
}
```

#### For Advanced Control

```rust
use ah_logging::{CliLoggingArgs, CliLogLevel, LogFormat};
use clap::Parser;

#[derive(Parser)]
struct Args {
    /// Log level
    #[arg(long, default_value = "info")]
    log_level: CliLogLevel,

    /// Log to file instead of console
    #[arg(long)]
    log_to_file: bool,

    /// Log format
    #[arg(long, default_value = "plaintext")]
    log_format: LogFormat,

    // ... other args
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    if args.log_to_file {
        ah_logging::init_to_standard_file("my-binary", args.log_level.into(), args.log_format)?;
    } else {
        ah_logging::init("my-binary", args.log_level.into(), args.log_format)?;
    }

    // ... application logic
}
```

### Help Text Standards

CLI help text MUST follow these conventions:

```bash
$ binary --help
Usage: binary [OPTIONS] [ARGS...]

Options:
      --log-level <LEVEL>     Log verbosity level [default: info] [possible values: error, warn, info, debug, trace]
      --log-format <FORMAT>   Log output format [default: plaintext] [possible values: plaintext, json]
      --log-dir <DIR>         Directory for log files
      --log-file <FILE>       Log filename
```

### Environment Variable Integration

CLI arguments MUST work alongside `RUST_LOG` environment variable:

```bash
# CLI takes precedence over RUST_LOG
RUST_LOG=debug ./binary --log-level info  # Uses INFO level

# RUST_LOG provides component-specific control
./binary --log-level debug RUST_LOG=my_binary=trace,other_crate=off
```

### Testing Requirements

All binaries MUST include tests for logging configuration:

```rust
#[test]
fn test_log_level_parsing() {
    let args = vec!["--log-level", "debug"];
    let config = parse_args(args);
    assert!(matches!(config.log_level, CliLogLevel::Debug));
}

#[test]
fn test_file_logging() {
    let temp_dir = tempfile::tempdir().unwrap();
    let log_path = temp_dir.path().join("test.log");

    // Test that file logging creates the expected file
    init_to_file("test", Level::INFO, LogFormat::Plaintext, &log_path).unwrap();
    assert!(log_path.exists());
}
```

### Configuration File

```toml
log-level = "info"
log-format = "json"  # "json" or "plaintext"
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

- Lint rules ensure `println!` is not used in production code (with exceptions documented below)
- Build checks verify trace logs are compiled out in release
- Integration tests validate log output formats
- CLI tests verify log level configuration works correctly

### Code Review Guidelines

- Reviewers should check that appropriate log levels are used
- Sensitive data logging should be flagged
- Structured logging should be preferred over string formatting

### Exceptions and Escape Hatches

While `println!` and `eprintln!` are generally disallowed in production code, there are legitimate exceptions:

#### ✅ **CLI User Interface Output**

Commands that produce user-visible output (not logs) may use `println!`/`eprintln!`. Examples:

- `ah config get` - displays configuration values to stdout
- `ah health` - displays health check results to stdout
- `ah task get` - displays processed task content to stdout
- `ah agent start --output text-normalized` - displays agent output to stdout

These are **not logging** - they are the primary output of CLI commands that users expect to see.

#### ✅ **Test Code**

Test functions may use `println!`/`eprintln!` for debugging test failures. These are typically temporary and acceptable in test contexts.

#### ✅ **Error Messages for Critical Failures**

Before logging is initialized, `eprintln!` may be used to report initialization failures.

#### ❌ **What Still Requires Logging**

- Internal operation status (use `tracing::info!`/`tracing::debug!`)
- Error conditions (use `tracing::error!`/`tracing::warn!`)
- Performance metrics (use `tracing::info!`)
- Debug information (use `tracing::debug!`/`tracing::trace!`)

#### **Clippy Configuration**

The lint rule is configured in `clippy.toml`:

```toml
disallowed-methods = [
    # Prevent use of println!/eprintln! in production code - use tracing::info!/tracing::error! instead
    "std::io::_print",
    "std::io::_eprint"
]
```

To suppress the lint for legitimate CLI output, use `#[allow(clippy::disallowed_methods)]` on the specific function or module.
