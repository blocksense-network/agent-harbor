// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Serializable DTOs for task manager IPC.

use crate::task_manager::{StartingPoint, TaskLaunchParams};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchTaskRequest {
    pub params: TaskLaunchParams,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchTaskResponse {
    pub session_ids: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum DaemonRequest {
    LaunchTask(Box<LaunchTaskRequest>),
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum DaemonResponse {
    LaunchTaskResult(LaunchTaskResponse),
    Pong,
    Error { message: String },
}

/// Convert a DTO starting point to TaskLaunchParams StartingPoint
pub fn dto_starting_point(sp: &StartingPoint) -> StartingPoint {
    sp.clone()
}
