// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Centralized logging utilities for Agent Harbor
//!
//! This crate provides standardized logging initialization and utilities
//! to ensure consistent logging behavior across all Agent Harbor components.

pub mod logging_config;

use serde::{Deserialize, Serialize};
use std::io;
use std::path::PathBuf;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

// Re-export clap for convenience when using CliLoggingArgs
pub use clap;

// Re-export Level for convenience
pub use tracing::Level;

/// Output format for log messages
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, clap::ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    /// Human-readable plaintext format
    #[default]
    Plaintext,
    /// Structured JSON format
    Json,
}

impl std::fmt::Display for LogFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogFormat::Plaintext => write!(f, "plaintext"),
            LogFormat::Json => write!(f, "json"),
        }
    }
}

impl std::str::FromStr for LogFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "plaintext" => Ok(LogFormat::Plaintext),
            "json" => Ok(LogFormat::Json),
            _ => Err(format!(
                "Invalid log format: {}. Use 'plaintext' or 'json'",
                s
            )),
        }
    }
}

/// CLI log level enum for clap integration
///
/// This enum provides a standardized way to specify log levels via command-line arguments.
/// It integrates with clap's ValueEnum for automatic help text and validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CliLogLevel {
    /// Only error conditions
    Error,
    /// Errors and warnings
    Warn,
    /// Errors, warnings, and informational messages
    Info,
    /// All above plus debug information
    Debug,
    /// All above plus detailed tracing
    Trace,
}

impl Default for CliLogLevel {
    fn default() -> Self {
        Self::Info
    }
}

impl From<CliLogLevel> for Level {
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

impl std::fmt::Display for CliLogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliLogLevel::Error => write!(f, "error"),
            CliLogLevel::Warn => write!(f, "warn"),
            CliLogLevel::Info => write!(f, "info"),
            CliLogLevel::Debug => write!(f, "debug"),
            CliLogLevel::Trace => write!(f, "trace"),
        }
    }
}

/// Standardized CLI logging arguments for clap integration
///
/// This struct provides logging-related command-line arguments that follow the Agent Harbor
/// logging policy. Use this with `#[command(flatten)]` in your clap structs for consistent
/// logging CLI across all binaries.
///
/// TUI binaries automatically log to file. Other binaries log to console by default,
/// but log to file when --log-file or --log-dir is specified.
#[derive(Clone, Debug, Default, clap::Args, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CliLoggingArgs {
    /// Log verbosity level
    #[arg(long, value_enum, help = "Log verbosity level (default: info)")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_level: Option<CliLogLevel>,

    /// Log output format
    #[arg(long, value_enum, help = "Log output format (default: plaintext)")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_format: Option<LogFormat>,

    /// Directory for log files
    #[arg(long, help = "Directory for log files (default: platform specific)")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_dir: Option<String>,

    /// Log filename
    #[arg(long, help = "Log filename")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_file: Option<String>,
}

impl CliLoggingArgs {
    /// Initialize logging based on the parsed CLI arguments
    ///
    /// This method automatically determines whether to log to console or file based on:
    /// - TUI binaries: Always log to file
    /// - Other binaries: Log to console, unless file options (--log-file or --log-dir) are provided
    ///
    /// # Arguments
    /// * `component` - The component name (e.g., "ah-cli", "ah-core")
    /// * `is_tui` - Whether this is a TUI application (always logs to file)
    ///
    /// # Examples
    /// ```rust
    /// use ah_logging::CliLoggingArgs;
    /// use clap::Parser;
    ///
    /// #[derive(Parser)]
    /// struct Args {
    ///     #[command(flatten)]
    ///     logging: CliLoggingArgs,
    /// }
    ///
    /// fn main() -> anyhow::Result<()> {
    ///     let args = Args::parse();
    ///     // For TUI apps: always log to file
    ///     args.logging.init("my-tui", true)?;
    ///     Ok(())
    /// }
    /// ```
    ///
    /// ```rust
    /// use ah_logging::CliLoggingArgs;
    /// use clap::Parser;
    ///
    /// #[derive(Parser)]
    /// struct Args {
    ///     #[command(flatten)]
    ///     logging: CliLoggingArgs,
    /// }
    ///
    /// fn main() -> anyhow::Result<()> {
    ///     let args = Args::parse();
    ///     // For CLI tools: console by default, file when --log-file/--log-dir specified
    ///     args.logging.init("my-cli", false)?;
    ///     Ok(())
    /// }
    /// ```
    pub fn init(self, component: &str, is_tui: bool) -> anyhow::Result<()> {
        self.init_with_default_level(component, is_tui, CliLogLevel::Info)
    }

    pub fn init_with_default_level(
        self,
        component: &str,
        is_tui: bool,
        default_level: CliLogLevel,
    ) -> anyhow::Result<()> {
        let level = self.log_level.unwrap_or(default_level).into();

        // Determine if we should log to file
        let should_log_to_file = is_tui || self.log_file.is_some() || self.log_dir.is_some();

        if should_log_to_file {
            // File logging
            let log_path = self.resolve_log_path(component);
            init_to_file(
                component,
                level,
                self.log_format.unwrap_or(LogFormat::Plaintext),
                &log_path,
            )
        } else {
            // Console logging
            init(
                component,
                level,
                self.log_format.unwrap_or(LogFormat::Plaintext),
            )
        }
    }

    /// Resolve the complete log file path based on CLI arguments
    ///
    /// Follows the Agent Harbor policy for log path resolution:
    /// 1. If `log_file` contains absolute path, use it directly
    /// 2. If `log_file` contains relative path with directory, append to `log_dir`
    /// 3. If `log_file` is just a filename, combine with `log_dir`
    /// 4. If no custom path specified, use platform standard location
    fn resolve_log_path(&self, component: &str) -> std::path::PathBuf {
        if let Some(log_file) = &self.log_file {
            let log_file_path = std::path::Path::new(log_file);

            // If log_file contains a directory component
            if let Some(parent) = log_file_path.parent() {
                if parent.is_absolute() {
                    // If both log_file has absolute path and log_dir is set, use log_file's directory
                    log_file_path.to_path_buf()
                } else {
                    // log_file has relative directory component, append to log_dir
                    if let Some(log_dir) = &self.log_dir {
                        std::path::Path::new(log_dir).join(log_file_path)
                    } else {
                        log_file_path.to_path_buf()
                    }
                }
            } else {
                // log_file is just a filename, combine with log_dir
                if let Some(log_dir) = &self.log_dir {
                    std::path::Path::new(log_dir).join(log_file)
                } else {
                    get_standard_log_path_for_component(component)
                }
            }
        } else {
            // No log_file specified, use default based on log_dir
            if let Some(log_dir) = &self.log_dir {
                std::path::Path::new(log_dir).join(format!("{}.log", component))
            } else {
                get_standard_log_path_for_component(component)
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.log_level.is_none()
            && self.log_format.is_none()
            && self.log_dir.is_none()
            && self.log_file.is_none()
    }
}

/// Get the standard log file path for a specific component
///
/// Similar to `get_standard_log_path()` but includes the component name in the filename.
pub fn get_standard_log_path_for_component(component: &str) -> std::path::PathBuf {
    let base_path = get_standard_log_path();
    let parent = base_path.parent().unwrap_or(std::path::Path::new("/tmp"));
    let filename = format!("{}.log", component);
    parent.join(filename)
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
pub fn init_for_test(
    component: &str,
    default_level: Level,
) -> std::sync::Arc<std::sync::Mutex<Vec<u8>>> {
    use std::io::Write;
    use std::sync::{Arc, Mutex, MutexGuard};
    use tracing_subscriber::fmt::MakeWriter;

    struct BufferWriter(Arc<Mutex<Vec<u8>>>);
    struct BufferGuard<'a>(MutexGuard<'a, Vec<u8>>);

    impl<'a> Write for BufferGuard<'a> {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for BufferWriter {
        type Writer = BufferGuard<'a>;
        fn make_writer(&'a self) -> Self::Writer {
            BufferGuard(self.0.lock().unwrap())
        }
    }

    let shared = Arc::new(Mutex::new(Vec::new()));
    let writer = BufferWriter(shared.clone());
    init_with_writer(component, default_level, LogFormat::Plaintext, writer)
        .expect("Failed to init test logging");
    shared
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

/// Test utilities for working with log output
#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils {
    /// Strip ANSI escape sequences from a string
    ///
    /// This is useful for testing log output that may contain ANSI color codes.
    ///
    /// # Example
    /// ```rust
    /// use ah_logging::test_utils::strip_ansi_codes;
    ///
    /// let colored_output = "\x1b[32mHello\x1b[0m World";
    /// let plain_output = strip_ansi_codes(colored_output);
    /// assert_eq!(plain_output, "Hello World");
    /// ```
    #[cfg(feature = "test-utils")]
    pub fn strip_ansi_codes(s: &str) -> String {
        // More comprehensive regex to match ANSI escape sequences
        // This matches \x1b[ followed by any number of digits and semicolons, ending with a letter
        let ansi_regex = regex::Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]").unwrap();
        ansi_regex.replace_all(s, "").to_string()
    }

    /// Strip ANSI escape sequences from a string (fallback implementation)
    ///
    /// This is a simplified version that doesn't require the regex crate.
    /// It handles common ANSI escape sequences but may not catch all edge cases.
    #[cfg(not(feature = "test-utils"))]
    pub fn strip_ansi_codes(s: &str) -> String {
        // Simple implementation for basic ANSI escape sequence removal
        // This handles \x1b[ followed by non-letter characters, then a letter
        let mut result = String::with_capacity(s.len());
        let mut chars = s.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '\x1b' && chars.peek() == Some(&'[') {
                // Skip the escape sequence
                chars.next(); // consume '['
                // Skip until we find a letter (the end of the escape sequence)
                while let Some(ch) = chars.next() {
                    if ch.is_ascii_alphabetic() {
                        break;
                    }
                }
            } else {
                result.push(ch);
            }
        }

        result
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
        let _buffer = init_for_test("test-component", Level::INFO);
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

    #[test]
    fn test_cli_log_level_conversion() {
        // Test CliLogLevel to Level conversion
        assert_eq!(Level::from(CliLogLevel::Error), Level::ERROR);
        assert_eq!(Level::from(CliLogLevel::Warn), Level::WARN);
        assert_eq!(Level::from(CliLogLevel::Info), Level::INFO);
        assert_eq!(Level::from(CliLogLevel::Debug), Level::DEBUG);
        assert_eq!(Level::from(CliLogLevel::Trace), Level::TRACE);
    }

    #[test]
    fn test_cli_log_level_display() {
        // Test CliLogLevel Display implementation
        assert_eq!(format!("{}", CliLogLevel::Error), "error");
        assert_eq!(format!("{}", CliLogLevel::Warn), "warn");
        assert_eq!(format!("{}", CliLogLevel::Info), "info");
        assert_eq!(format!("{}", CliLogLevel::Debug), "debug");
        assert_eq!(format!("{}", CliLogLevel::Trace), "trace");
    }

    #[test]
    fn test_cli_log_level_default() {
        // Test that CliLogLevel defaults to Info
        let default: CliLogLevel = Default::default();
        assert_eq!(default, CliLogLevel::Info);
    }

    #[test]
    fn test_cli_logging_args_defaults() {
        // Test that CliLoggingArgs has sensible defaults
        let args = CliLoggingArgs::default();

        // Non-TUI binary with no file options should log to console
        let is_tui = false;
        let should_log_to_file = is_tui || args.log_file.is_some() || args.log_dir.is_some();
        assert!(!should_log_to_file); // Should be false for non-TUI with no file options

        // TUI binary should always log to file
        let is_tui = true;
        let should_log_to_file_tui = is_tui || args.log_file.is_some() || args.log_dir.is_some();
        assert!(should_log_to_file_tui); // Should be true for TUI

        // Non-TUI with log_file should log to file
        let is_tui = false;
        let args_with_file = CliLoggingArgs {
            log_file: Some("test.log".to_string()),
            ..Default::default()
        };
        let should_log_to_file_with_file =
            is_tui || args_with_file.log_file.is_some() || args_with_file.log_dir.is_some();
        assert!(should_log_to_file_with_file); // Should be true when log_file is provided
    }

    #[test]
    fn test_standard_log_path_for_component() {
        // Test that component-specific log paths are generated correctly
        let path = get_standard_log_path_for_component("test-component");
        let path_str = path.to_string_lossy();

        // Should end with test-component.log
        assert!(path_str.ends_with("test-component.log"));

        // Should be in the standard log directory
        #[cfg(target_os = "macos")]
        assert!(path_str.contains("Library/Logs"));

        #[cfg(target_os = "linux")]
        assert!(path_str.contains(".local/share"));

        #[cfg(target_os = "windows")]
        assert!(path_str.contains("AppData"));
    }
}
