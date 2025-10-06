//! IPC protocol definitions for TUI testing framework

/// Commands that can be sent from child process to test runner
#[derive(Debug, Clone, PartialEq)]
pub enum TestCommand {
    /// Request a screenshot capture with the given label
    Screenshot(String),
    /// Ping the test runner to check connectivity
    Ping,
}

/// Response from test runner to child process
#[derive(Debug, Clone, PartialEq)]
pub enum TestResponse {
    /// Operation completed successfully
    Ok,
    /// Operation failed
    Error(String),
}
