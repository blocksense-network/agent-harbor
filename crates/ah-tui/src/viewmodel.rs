// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! ViewModel layer - derived presentation state shaped for rendering
//!
//! The ViewModel transforms domain state into presentation-ready data
//! (strings, selection flags, focus indicators) that the View can consume.

use crate::app::AppState;
use crate::task::Task;
use ah_rest_api_contract::{AgentCapability, Project, Repository};

/// ViewModel represents the presentation state derived from the Model
/// This is what the UI rendering code consumes - pure data, no business logic
#[derive(Debug, Clone, PartialEq)]
pub struct ViewModel {
    /// List of tasks to display
    pub tasks: Vec<Task>,
    /// Currently selected task index
    pub selected_task_index: usize,
    /// Whether there are unsaved draft changes
    pub has_unsaved_draft: bool,
    /// Current loading state
    pub loading: bool,
    /// Current error message (if any)
    pub error_message: Option<String>,
}

impl ViewModel {
    /// Create a ViewModel from the current AppState
    pub fn from_state(state: &AppState) -> Self {
        Self {
            tasks: state.tasks.clone(),
            selected_task_index: state.selected_task_index,
            has_unsaved_draft: state.has_unsaved_draft,
            loading: state.loading,
            error_message: state.error.clone(),
        }
    }

    /// Get the selected task (useful for assertions)
    pub fn selected_task(&self) -> Option<&Task> {
        self.tasks.get(self.selected_task_index)
    }

    /// Check if a specific task is selected by ID
    pub fn is_task_selected(&self, task_id: &str) -> bool {
        self.tasks
            .get(self.selected_task_index)
            .map(|t| t.id == task_id)
            .unwrap_or(false)
    }
}
