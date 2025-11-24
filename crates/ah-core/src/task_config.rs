// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Task/execution configuration types

use serde::{Deserialize, Serialize};

/// Task/execution configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskConfig {
    /// Enable OS notifications on task completion
    pub notifications: Option<bool>,
    /// Use VCS comment strings in editor
    #[serde(rename = "task-editor.use-vcs-comment-string")]
    pub task_editor_use_vcs_comment_string: Option<bool>,
    /// Path to task template file
    #[serde(rename = "task-template")]
    pub task_template: Option<String>,
}
