// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Task-related data structures for the TUI
//!
//! This module defines the data models for different types of tasks
//! displayed in the task-centric interface.

use std::time::SystemTime;

/// Represents a model selection with instance count
#[derive(Debug, Clone, PartialEq)]
pub struct AgentSelection {
    pub model_name: String,
    pub instance_count: u32,
    pub selected: bool,
}

/// Represents the current state of a task
#[derive(Debug, Clone, PartialEq)]
pub enum TaskState {
    /// Task has been merged/closed - shows title + time indicator
    Merged {
        title: String,
        merged_at: SystemTime,
    },
    /// Agent has finished but task not yet merged
    Completed {
        title: String,
        completed_at: SystemTime,
        status: String,
    },
    /// Agent is currently working - shows live activity
    Active {
        title: String,
        started_at: SystemTime,
        current_action: String,
        action_detail: String,
        progress: Option<String>,
    },
    /// User started task input but hasn't launched yet
    Draft {
        description: String,
        created_at: SystemTime,
    },
    /// New task creation interface with button-based selectors and modal dialogs
    New {
        description: String,
        selected_repo: String,
        selected_branch: String,
        selected_models: Vec<AgentSelection>,
        focused_button: ButtonFocus,
        modal_state: Option<ModalState>,
    },
}

/// Represents which button is currently focused in the new task interface
#[derive(Debug, Clone, PartialEq)]
pub enum ButtonFocus {
    Description,
    Repository,
    Branch,
    Models,
    Go,
}

/// Represents the state of a modal dialog for fuzzy search selection
#[derive(Debug, Clone, PartialEq)]
pub enum ModalState {
    /// Repository selection modal
    RepositorySelect {
        query: String,
        options: Vec<String>,
        selected_index: usize,
    },
    /// Branch selection modal
    BranchSelect {
        query: String,
        options: Vec<String>,
        selected_index: usize,
    },
    /// Model selection modal
    ModelSelect {
        query: String,
        options: Vec<AgentSelection>,
        selected_index: usize,
    },
}

/// Represents a task in the UI
#[derive(Debug, Clone, PartialEq)]
pub struct Task {
    pub id: String,
    pub state: TaskState,
    pub created_at: SystemTime,
}

impl Task {
    /// Create a new merged task
    pub fn merged(id: String, title: String, merged_at: SystemTime) -> Self {
        Self {
            id,
            state: TaskState::Merged { title, merged_at },
            created_at: merged_at,
        }
    }

    /// Create a new completed task
    pub fn completed(id: String, title: String, completed_at: SystemTime, status: String) -> Self {
        Self {
            id,
            state: TaskState::Completed {
                title,
                completed_at,
                status,
            },
            created_at: completed_at,
        }
    }

    /// Create a new active task
    pub fn active(
        id: String,
        title: String,
        started_at: SystemTime,
        current_action: String,
        action_detail: String,
        progress: Option<String>,
    ) -> Self {
        Self {
            id,
            state: TaskState::Active {
                title,
                started_at,
                current_action,
                action_detail,
                progress,
            },
            created_at: started_at,
        }
    }

    /// Create a new draft task
    pub fn draft(id: String, description: String, created_at: SystemTime) -> Self {
        Self {
            id,
            state: TaskState::Draft {
                description,
                created_at,
            },
            created_at,
        }
    }

    /// Create a new task input with default selections
    pub fn new() -> Self {
        Self {
            id: "new".to_string(),
            state: TaskState::New {
                description: String::new(),
                selected_repo: "agent-harbor".to_string(),
                selected_branch: "main".to_string(),
                selected_models: create_default_models()
                    .into_iter()
                    .filter(|m| m.selected)
                    .collect(),
                focused_button: ButtonFocus::Description,
                modal_state: None,
            },
            created_at: SystemTime::now(),
        }
    }

    /// Get a display string for time relative to now
    pub fn time_ago(&self) -> String {
        let now = SystemTime::now();
        let duration = now.duration_since(self.created_at).unwrap_or_default();

        if duration.as_secs() < 60 {
            format!("{}s ago", duration.as_secs())
        } else if duration.as_secs() < 3600 {
            format!("{}m ago", duration.as_secs() / 60)
        } else if duration.as_secs() < 86400 {
            format!("{}h ago", duration.as_secs() / 3600)
        } else {
            format!("{}d ago", duration.as_secs() / 86400)
        }
    }

    /// Get the title for display (truncated if needed)
    pub fn display_title(&self, max_width: usize) -> String {
        let title = match &self.state {
            TaskState::Merged { title, .. } => title.clone(),
            TaskState::Completed { title, .. } => title.clone(),
            TaskState::Active { title, .. } => title.clone(),
            TaskState::Draft { description, .. } => description.clone(),
            TaskState::New {
                selected_repo,
                selected_branch,
                ..
            } => {
                if selected_repo.is_empty() && selected_branch.is_empty() {
                    "New task".to_string()
                } else {
                    format!("New task: {}:{}", selected_repo, selected_branch)
                }
            }
        };

        if title.len() <= max_width {
            title.clone()
        } else {
            format!("{}...", &title[..max_width.saturating_sub(3)])
        }
    }

    /// Check if this task is selectable
    pub fn is_selectable(&self) -> bool {
        !matches!(self.state, TaskState::New { .. })
    }
}

impl Default for Task {
    fn default() -> Self {
        Self::new()
    }
}

/// Sample repositories for the repo selector
pub fn create_sample_repos() -> Vec<String> {
    vec![
        "agent-harbor".to_string(),
        "frontend-ui".to_string(),
        "backend-api".to_string(),
        "documentation".to_string(),
    ]
}

/// Sample branches for the branch selector
pub fn create_sample_branches() -> Vec<String> {
    vec![
        "main".to_string(),
        "develop".to_string(),
        "feature/new-ui".to_string(),
        "hotfix/bug-fix".to_string(),
    ]
}

/// Create default model selections for new tasks
pub fn create_default_models() -> Vec<AgentSelection> {
    vec![
        AgentSelection {
            model_name: "GPT-4".to_string(),
            instance_count: 1,
            selected: true,
        },
        AgentSelection {
            model_name: "Claude-3".to_string(),
            instance_count: 0,
            selected: false,
        },
        AgentSelection {
            model_name: "Gemini".to_string(),
            instance_count: 0,
            selected: false,
        },
        AgentSelection {
            model_name: "Llama-3".to_string(),
            instance_count: 0,
            selected: false,
        },
    ]
}

/// Helper for creating sample tasks for development/testing
pub fn create_sample_tasks() -> Vec<Task> {
    let now = SystemTime::now();
    let one_hour_ago = now - std::time::Duration::from_secs(3600);
    let two_hours_ago = now - std::time::Duration::from_secs(7200);
    let one_day_ago = now - std::time::Duration::from_secs(86400);
    let two_days_ago = now - std::time::Duration::from_secs(172800);

    vec![
        Task::new(),
        Task::active(
            "task_001".to_string(),
            "Refactor database schema for better performance".to_string(),
            one_hour_ago,
            "Running tests".to_string(),
            "Executing integration test suite".to_string(),
            Some("45/67 tests passed".to_string()),
        ),
        Task::completed(
            "task_002".to_string(),
            "Add error handling for network timeouts".to_string(),
            two_hours_ago,
            "Ready for review".to_string(),
        ),
        Task::merged(
            "task_003".to_string(),
            "Update documentation for new API endpoints".to_string(),
            one_day_ago,
        ),
        Task::merged(
            "task_004".to_string(),
            "Fix memory leak in background worker process".to_string(),
            two_days_ago,
        ),
        Task::draft(
            "task_005".to_string(),
            "Implement dark mode toggle in settings".to_string(),
            now - std::time::Duration::from_secs(1800),
        ),
    ]
}
