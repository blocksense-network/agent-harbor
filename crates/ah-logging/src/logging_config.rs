// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Logging configuration types

use serde::{Deserialize, Serialize};

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Logging verbosity level
    #[serde(rename = "log-level")]
    pub level: Option<String>,
}
