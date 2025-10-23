// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Repository-related domain types
//!
//! Types related to version control repositories and their metadata.

use serde::{Deserialize, Serialize};

/// Repository information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Repository {
    pub id: String,
    pub name: String,
    pub url: String,
    pub default_branch: String,
}

/// Branch information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Branch {
    pub name: String,
    pub is_default: bool,
    pub last_commit: Option<String>,
}
