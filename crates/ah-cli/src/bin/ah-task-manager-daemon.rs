// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Task manager daemon entrypoint.
//! Keeps the LocalTaskManager alive so recorder/monitoring can attach via the task-manager socket.

use ah_core::task_manager_daemon::run_task_manager_daemon;
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    run_task_manager_daemon().await
}
