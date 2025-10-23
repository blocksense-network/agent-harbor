// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! IPC protocol definitions for TUI testing framework

/// Commands that can be sent from child process to test runner
#[derive(Debug, Clone, PartialEq)]
pub enum TestCommand {
    /// Request a screenshot capture with the given label
    Screenshot(String),
    /// Terminate the tested program with the given exit code
    Exit(i32),
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
