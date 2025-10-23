// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Agent-related domain types
//!
//! Types related to AI agents, models, and their configurations.

use serde::{Deserialize, Serialize};

/// Agent/model selection with instance count
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectedModel {
    pub name: String,
    pub count: usize,
}
