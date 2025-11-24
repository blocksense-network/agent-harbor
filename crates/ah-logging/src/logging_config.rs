// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Logging configuration types

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::{CliLogLevel, CliLoggingArgs, LogFormat};

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LoggingConfig {
    /// Logging verbosity level
    #[serde(rename = "log-level")]
    pub level: Option<CliLogLevel>,
    /// Logging output format
    #[serde(rename = "log-format")]
    pub format: Option<LogFormat>,
    /// Directory for log files
    #[serde(rename = "log-dir")]
    pub log_dir: Option<PathBuf>,
    /// Log filename
    #[serde(rename = "log-file")]
    pub log_file: Option<String>,
}

impl LoggingConfig {
    /// Convert the configuration into CLI logging arguments (without applying defaults).
    pub fn to_cli_logging_args(&self) -> CliLoggingArgs {
        CliLoggingArgs {
            log_level: self.level,
            log_format: self.format,
            log_dir: self.log_dir.as_ref().map(|dir| dir.to_string_lossy().into_owned()),
            log_file: self.log_file.clone(),
        }
    }
}
