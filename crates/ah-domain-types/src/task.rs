//! Task-related domain types
//!
//! These types represent tasks, their states, and related business entities
//! that are shared across the Agent Harbor system.

use serde::{Deserialize, Serialize};
use crate::agent::SelectedModel;

/// Task execution states as defined in PRD
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum TaskState {
    /// Draft task being edited
    Draft,
    /// Active task running
    Active,
    /// Completed task
    Completed,
    /// Merged task
    Merged,
}

/// Draft task - represents a task being created/edited
/// Different from TaskCard as it has different lifecycle and structure
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DraftTask {
    pub id: String,
    pub description: String,
    pub repository: String,
    pub branch: String,
    pub models: Vec<SelectedModel>,
    pub created_at: String,
}

/// Task execution record - represents executed/running tasks in the domain
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskExecution {
    pub id: String,
    pub repository: String,
    pub branch: String,
    pub agents: Vec<SelectedModel>,
    pub state: TaskState,
    pub timestamp: String,
    pub activity: Vec<String>, // For active tasks
    pub delivery_status: Vec<DeliveryStatus>, // For completed/merged tasks
}

/// Delivery status for completed tasks
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DeliveryStatus {
    BranchCreated,
    PullRequestCreated { pr_number: u32, title: String },
    PullRequestMerged { pr_number: u32 },
}
/// Task information from external systems (simplified for now)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskInfo {
    pub id: String,
    pub title: String,
    pub status: String,
    pub repository: String,
    pub branch: String,
    pub created_at: String,
    pub models: Vec<String>,
}

impl TaskExecution {
    /// Add activity to an active task
    pub fn add_activity(&mut self, activity: String) {
        if self.state == TaskState::Active {
            self.activity.push(activity);
            // Keep only last 10 activities for memory efficiency
            if self.activity.len() > 10 {
                self.activity.remove(0);
            }
        }
    }

    /// Get recent activity for display
    pub fn get_recent_activity(&self, count: usize) -> Vec<String> {
        if self.state == TaskState::Active {
            let recent: Vec<String> = self.activity.iter()
                .rev()
                .take(count)
                .cloned()
                .collect();
            let mut result: Vec<String> = recent.into_iter().rev().collect();

            // Always return exactly count lines, padding with empty strings at the beginning
            while result.len() < count {
                result.insert(0, String::new());
            }
            result
        } else {
            vec![String::new(); count]
        }
    }
}
