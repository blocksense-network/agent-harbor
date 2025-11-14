// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Task-related domain types
//!
//! These types represent tasks, their states, and related business entities
//! that are shared across the Agent Harbor system.

use crate::AgentChoice;
use serde::{Deserialize, Serialize};

/// Task state - shared between REST API and local task management
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum TaskState {
    /// Draft task being edited
    Draft,
    /// Task is queued for execution
    Queued,
    /// Task is being provisioned
    Provisioning,
    /// Task is actively running
    Running,
    /// Task is being paused
    Pausing,
    /// Task execution is paused
    Paused,
    /// Task is resuming from pause
    Resuming,
    /// Task is being stopped
    Stopping,
    /// Task execution is stopped
    Stopped,
    /// Task completed successfully
    Completed,
    /// Task failed during execution
    Failed,
    /// Task was cancelled
    Cancelled,
    /// Task results were merged
    Merged,
}

impl std::fmt::Display for TaskState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status_str = match self {
            TaskState::Draft => "draft",
            TaskState::Queued => "queued",
            TaskState::Provisioning => "provisioning",
            TaskState::Running => "running",
            TaskState::Pausing => "pausing",
            TaskState::Paused => "paused",
            TaskState::Resuming => "resuming",
            TaskState::Stopping => "stopping",
            TaskState::Stopped => "stopped",
            TaskState::Completed => "completed",
            TaskState::Failed => "failed",
            TaskState::Cancelled => "cancelled",
            TaskState::Merged => "merged",
        };
        write!(f, "{}", status_str)
    }
}

/// Log levels for task events
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

/// Tool execution status
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolStatus {
    Started,
    Completed,
    Failed,
}

/// Draft task - represents a task being created/edited
/// Different from TaskCard as it has different lifecycle and structure
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DraftTask {
    pub id: String,
    pub description: String,
    pub repository: String,
    pub branch: String,
    pub selected_agents: Vec<AgentChoice>,
    pub created_at: String,
}

/// Task execution record - represents executed/running tasks in the domain
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskExecution {
    pub id: String,
    pub repository: String,
    pub branch: String,
    pub agents: Vec<AgentChoice>,
    pub state: TaskState,
    pub timestamp: String,
    pub activity: Vec<String>,                // For active tasks
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
    pub models: Vec<AgentChoice>,
}

impl TaskExecution {
    /// Add activity to an active task
    pub fn add_activity(&mut self, activity: String) {
        // Check if task is in an active/running state
        match self.state {
            TaskState::Queued
            | TaskState::Provisioning
            | TaskState::Running
            | TaskState::Pausing
            | TaskState::Paused
            | TaskState::Resuming
            | TaskState::Stopping
            | TaskState::Stopped => {
                self.activity.push(activity);
                // Keep only last 10 activities for memory efficiency
                if self.activity.len() > 10 {
                    self.activity.remove(0);
                }
            }
            _ => {} // Don't add activity for non-active tasks
        }
    }

    /// Get recent activity for display
    pub fn get_recent_activity(&self, count: usize) -> Vec<String> {
        // Check if task is in an active/running state
        match self.state {
            TaskState::Queued
            | TaskState::Provisioning
            | TaskState::Running
            | TaskState::Pausing
            | TaskState::Paused
            | TaskState::Resuming
            | TaskState::Stopping
            | TaskState::Stopped => {
                let recent: Vec<String> = self.activity.iter().rev().take(count).cloned().collect();
                let mut result: Vec<String> = recent.into_iter().rev().collect();

                // Always return exactly count lines, padding with empty strings at the beginning
                while result.len() < count {
                    result.insert(0, String::new());
                }
                result
            }
            _ => {
                vec![String::new(); count]
            }
        }
    }
}
