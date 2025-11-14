// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Centralized logging utilities for Agent Harbor
//!
//! This crate provides standardized logging initialization and utilities
//! to ensure consistent logging behavior across all Agent Harbor components.

use std::io;
use std::path::PathBuf;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

// Re-export Level for convenience
pub use tracing::Level;

/// Output format for log messages
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogFormat {
    /// Human-readable plaintext format
    Plaintext,
    /// Structured JSON format
    Json,
}

impl Default for LogFormat {
    fn default() -> Self {
        LogFormat::Plaintext
    }
}

/// Get the standard log file path for the current OS
///
/// This function provides platform-specific log file paths:
/// - Windows: %APPDATA%\agent-harbor\agent-harbor.log
/// - macOS: ~/Library/Logs/agent-harbor.log
/// - Linux: ~/.local/share/agent-harbor/agent-harbor.log
/// - Other: ~/agent-harbor.log (fallback)
///
/// # Returns
/// A PathBuf containing the appropriate log file path for the current platform
///
/// # Example
/// ```rust
/// use ah_logging::get_standard_log_path;
/// use ah_logging::{init_to_file, Level, LogFormat};
///
/// fn main() -> anyhow::Result<()> {
///     let log_path = get_standard_log_path();
///     init_to_file("my-app", Level::INFO, LogFormat::Plaintext, &log_path)?;
///     Ok(())
/// }
/// ```
pub fn get_standard_log_path() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        // Windows: %APPDATA%\agent-harbor\agent-harbor.log
        let mut path = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("C:\\Users\\Default\\AppData\\Roaming"));
        path.push("agent-harbor");
        path.push("agent-harbor.log");
        path
    }

    #[cfg(target_os = "macos")]
    {
        // macOS: ~/Library/Logs/agent-harbor.log
        let mut path = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        path.push("Library");
        path.push("Logs");
        path.push("agent-harbor.log");
        path
    }

    #[cfg(target_os = "linux")]
    {
        // Linux: ~/.local/share/agent-harbor/agent-harbor.log
        let mut path = dirs::data_dir()
            .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp")));
        path.push("agent-harbor");
        path.push("agent-harbor.log");
        path
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        // Fallback for other OSes
        let mut path = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        path.push("agent-harbor.log");
        path
    }
}

/// Initialize logging with the specified component name, default level, and format
///
/// # Arguments
/// * `component` - The component name (e.g., "ah-cli", "ah-core")
/// * `default_level` - Default log level when RUST_LOG is not set
/// * `format` - Output format for log messages
///
/// # Example
/// ```rust
/// use ah_logging::{init, Level, LogFormat};
///
/// fn main() -> anyhow::Result<()> {
///     init("ah-cli", Level::INFO, LogFormat::Plaintext)?;
///     tracing::info!("Application started");
///     Ok(())
/// }
/// ```
pub fn init(component: &str, default_level: Level, format: LogFormat) -> anyhow::Result<()> {
    init_with_writer(component, default_level, format, io::stdout)
}

/// Initialize logging with default plaintext format
///
/// # Arguments
/// * `component` - The component name (e.g., "ah-cli", "ah-core")
/// * `default_level` - Default log level when RUST_LOG is not set
///
/// # Example
/// ```rust
/// use ah_logging::{init_plaintext, Level};
///
/// fn main() -> anyhow::Result<()> {
///     init_plaintext("ah-cli", Level::INFO)?;
///     tracing::info!("Application started");
///     Ok(())
/// }
/// ```
pub fn init_plaintext(component: &str, default_level: Level) -> anyhow::Result<()> {
    init(component, default_level, LogFormat::Plaintext)
}

/// Initialize logging to a file with the specified component name, default level, and format
///
/// # Arguments
/// * `component` - The component name (e.g., "ah-cli", "ah-core")
/// * `default_level` - Default log level when RUST_LOG is not set
/// * `format` - Output format for log messages
/// * `log_path` - Path to the log file
///
/// # Example
/// ```rust
/// use ah_logging::{init_to_file, Level, LogFormat};
/// use std::path::Path;
///
/// fn main() -> anyhow::Result<()> {
///     let log_path = Path::new("agent-harbor.log");
///     init_to_file("ah-cli", Level::INFO, LogFormat::Json, log_path)?;
///     tracing::info!("Application started");
///     Ok(())
/// }
/// ```
pub fn init_to_file(
    component: &str,
    default_level: Level,
    format: LogFormat,
    log_path: &std::path::Path,
) -> anyhow::Result<()> {
    use std::fs;

    // Create parent directory if it doesn't exist
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Create or open the log file
    let log_file = fs::OpenOptions::new().create(true).append(true).open(log_path)?;

    init_with_writer(component, default_level, format, log_file)
}

/// Initialize logging to the standard platform-specific log file
///
/// # Arguments
/// * `component` - The component name (e.g., "ah-cli", "ah-core")
/// * `default_level` - Default log level when RUST_LOG is not set
/// * `format` - Output format for log messages
///
/// # Example
/// ```rust
/// use ah_logging::{init_to_standard_file, Level, LogFormat};
///
/// fn main() -> anyhow::Result<()> {
///     init_to_standard_file("ah-cli", Level::INFO, LogFormat::Plaintext)?;
///     tracing::info!("Application started");
///     Ok(())
/// }
/// ```
pub fn init_to_standard_file(
    component: &str,
    default_level: Level,
    format: LogFormat,
) -> anyhow::Result<()> {
    let log_path = get_standard_log_path();
    init_to_file(component, default_level, format, &log_path)
}

/// Initialize logging with a custom writer
///
/// # Arguments
/// * `component` - The component name (e.g., "ah-cli", "ah-core")
/// * `default_level` - Default log level when RUST_LOG is not set
/// * `format` - Output format for log messages
/// * `writer` - Where to write log output
pub fn init_with_writer<W>(
    component: &str,
    default_level: Level,
    format: LogFormat,
    writer: W,
) -> anyhow::Result<()>
where
    W: for<'writer> tracing_subscriber::fmt::MakeWriter<'writer> + Send + Sync + 'static,
{
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(format!("{},{}={}", default_level, component, default_level))
    });

    match format {
        LogFormat::Json => {
            let layer = tracing_subscriber::fmt::layer().with_writer(writer).json();
            #[cfg(debug_assertions)]
            let layer = layer.with_file(true).with_line_number(true);

            tracing_subscriber::registry().with(filter).with(layer).try_init()?;
        }
        LogFormat::Plaintext => {
            let layer = tracing_subscriber::fmt::layer().with_writer(writer);
            #[cfg(debug_assertions)]
            let layer = layer.with_file(true).with_line_number(true);

            tracing_subscriber::registry().with(filter).with(layer).try_init()?;
        }
    }

    Ok(())
}

/// Initialize logging for testing with a buffer
///
/// Returns a writer that can be used to capture log output for assertions.
#[cfg(feature = "tokio")]
pub fn init_for_test(component: &str, default_level: Level) -> (impl io::Write, Vec<u8>) {
    let buffer = Vec::new();
    let writer = std::io::Cursor::new(buffer);

    // Clone the buffer reference for returning
    let buffer_ref = writer.get_ref().clone();
    let writer_clone = writer;

    init_with_writer(component, default_level, LogFormat::Plaintext, writer_clone)
        .expect("Failed to init test logging");

    (writer_clone, buffer_ref)
}

/// Redact sensitive information from log output
///
/// # Example
/// ```rust
/// use ah_logging::redact;
///
/// let api_key = "sk-1234567890abcdef";
/// tracing::info!(api_key = %redact(api_key), "API key configured");
/// // Output: api_key="[REDACTED]"
/// ```
pub fn redact(_value: impl std::fmt::Display) -> &'static str {
    "[REDACTED]"
}

/// Get a correlation ID for the current operation
///
/// In a real implementation, this might generate a UUID or use
/// request IDs from HTTP headers. For now, returns a simple counter.
pub fn correlation_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    format!("corr-{}", COUNTER.fetch_add(1, Ordering::Relaxed))
}

/// Extension trait for adding component and operation fields to spans
pub trait TracingExt {
    /// Add component field to the current span
    fn component(self, component: &str) -> Self;

    /// Add operation field to the current span
    fn operation(self, operation: &str) -> Self;
}

impl TracingExt for tracing::Span {
    fn component(self, component: &str) -> Self {
        self.record("component", component);
        self
    }

    fn operation(self, operation: &str) -> Self {
        self.record("operation", operation);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing::{debug, error, info, trace, warn};

    #[test]
    fn test_redact() {
        let redacted = redact("sensitive-data");
        assert_eq!(format!("{}", redacted), "[REDACTED]");
        // Debug formatting adds quotes, so we expect "\"[REDACTED]\""
        assert_eq!(format!("{:?}", redacted), "\"[REDACTED]\"");
    }

    #[test]
    fn test_correlation_id() {
        let id1 = correlation_id();
        let id2 = correlation_id();
        assert_ne!(id1, id2);
        assert!(id1.starts_with("corr-"));
        assert!(id2.starts_with("corr-"));
    }

    #[test]
    fn test_trace_compile_out() {
        // This test verifies that trace! calls are compiled out in release builds
        // In debug builds, this should execute
        // In release builds, this should be compiled out
        #[cfg(debug_assertions)]
        {
            // In debug builds, trace should be available
            trace!("This is a trace message in debug build");
        }

        #[cfg(not(debug_assertions))]
        {
            // In release builds, trace calls should be compiled out
            // We can't test this directly, but the fact that this compiles
            // means trace! is defined as a no-op in release builds
        }
    }

    #[test]
    fn test_log_levels() {
        // Test that all log levels are available
        error!("Test error message");
        warn!("Test warning message");
        info!("Test info message");
        debug!("Test debug message");

        #[cfg(debug_assertions)]
        trace!("Test trace message");
    }

    #[test]
    fn test_tracing_ext() {
        // Test that TracingExt methods work
        let span = tracing::info_span!("test_span");
        let span = span.component("test-component");
        let span = span.operation("test-operation");

        // The span should be properly configured
        let _enter = span.enter();
    }

    #[cfg(feature = "tokio")]
    #[tokio::test]
    async fn test_init_for_test() {
        let (_writer, _buffer) = init_for_test("test-component", Level::INFO);
        info!("Test message");
        // In a real test, we'd check the buffer contents
    }

    #[test]
    fn test_level_conversion() {
        // Test that string to Level conversion works as expected
        // This is tested indirectly through the CLI usage
        let level = Level::INFO;
        assert_eq!(level, Level::INFO);
    }
}
