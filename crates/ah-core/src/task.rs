// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Task lifecycle management and orchestration.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Unique identifier for a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(pub u64);

/// Status of a task in its lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    /// Task has been created but not yet started.
    Pending,
    /// Task is currently running.
    Running,
    /// Task completed successfully.
    Completed,
    /// Task failed with an error.
    Failed,
    /// Task was cancelled.
    Cancelled,
}

/// Represents a task in the AH system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub name: String,
    pub description: String,
    pub status: TaskStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Task {
    /// Create a new task with the given parameters.
    pub fn new(id: TaskId, name: String, description: String) -> Self {
        let now = chrono::Utc::now();
        Self {
            id,
            name,
            description,
            status: TaskStatus::Pending,
            created_at: now,
            updated_at: now,
            metadata: HashMap::new(),
        }
    }

    /// Update the task status and modification time.
    pub fn update_status(&mut self, status: TaskStatus) {
        self.status = status;
        self.updated_at = chrono::Utc::now();
    }
}

/// Manages the lifecycle of tasks in the system.
#[derive(Debug, Clone)]
pub struct TaskStateManager {
    tasks: Arc<RwLock<HashMap<TaskId, Task>>>,
    next_id: Arc<RwLock<TaskId>>,
}

impl TaskStateManager {
    /// Create a new task state manager.
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
            next_id: Arc::new(RwLock::new(TaskId(1))),
        }
    }

    /// Create a new task and add it to the manager.
    pub async fn create_task(&self, name: String, description: String) -> crate::Result<TaskId> {
        let mut next_id = self.next_id.write().await;
        let task_id = *next_id;
        next_id.0 += 1;

        let task = Task::new(task_id, name, description);
        self.tasks.write().await.insert(task_id, task);

        Ok(task_id)
    }

    /// Get a task by its ID.
    pub async fn get_task(&self, id: TaskId) -> crate::Result<Option<Task>> {
        Ok(self.tasks.read().await.get(&id).cloned())
    }

    /// Update the status of a task.
    pub async fn update_task_status(&self, id: TaskId, status: TaskStatus) -> crate::Result<()> {
        let mut tasks = self.tasks.write().await;
        if let Some(task) = tasks.get_mut(&id) {
            task.update_status(status);
            Ok(())
        } else {
            Err(crate::Error::task(format!("Task {} not found", id.0)))
        }
    }

    /// List all tasks.
    pub async fn list_tasks(&self) -> crate::Result<Vec<Task>> {
        Ok(self.tasks.read().await.values().cloned().collect())
    }
}

impl Default for TaskStateManager {
    fn default() -> Self {
        Self::new()
    }
}
