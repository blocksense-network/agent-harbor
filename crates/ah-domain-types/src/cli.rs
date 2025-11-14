// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! CLI-specific domain types
//!
//! These types are used for command-line interface parsing and configuration.
//! They can be reused across different CLI applications in the Agent Harbor suite.

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

/// Log verbosity level for CLI argument parsing
///
/// This enum is used to control logging verbosity across all Agent Harbor CLI applications.
/// It provides a standardized set of log levels that can be configured via command-line flags.
#[derive(Clone, Debug, ValueEnum)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub enum CliLogLevel {
    /// Show only errors
    Error,
    /// Show warnings and errors
    Warn,
    /// Show info, warnings, and errors
    Info,
    /// Show debug info, info, warnings, and errors
    Debug,
    /// Show all log levels including trace
    Trace,
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

impl From<CliLogLevel> for tracing::Level {
    fn from(level: CliLogLevel) -> Self {
        match level {
            CliLogLevel::Error => tracing::Level::ERROR,
            CliLogLevel::Warn => tracing::Level::WARN,
            CliLogLevel::Info => tracing::Level::INFO,
            CliLogLevel::Debug => tracing::Level::DEBUG,
            CliLogLevel::Trace => tracing::Level::TRACE,
        }
    }
}
