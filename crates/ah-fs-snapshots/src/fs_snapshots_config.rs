// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Filesystem snapshots configuration types

use serde::{Deserialize, Serialize};

/// Filesystem snapshots configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsSnapshotsConfig {
    /// Filesystem snapshot provider
    #[serde(rename = "fs-snapshots")]
    pub provider: Option<String>,
    /// Working copy strategy
    #[serde(rename = "working-copy")]
    pub working_copy: Option<String>,
}
