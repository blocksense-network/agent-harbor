// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Simple task manager daemon that keeps the LocalTaskManager alive for session lifetime.
//!
//! This is a best-effort shim to keep the task-manager socket and multiplexer session alive
//! across `ah task` invocations so recorder/monitoring can attach. It reuses the existing
//! LocalTaskManager factory.

use crate::task_manager_init::create_task_manager_no_recording;
use anyhow::Result;

/// Run the task manager daemon (blocking).
pub async fn run_task_manager_daemon() -> Result<()> {
    // Hold the TaskManager in scope; LocalTaskManager binds the socket in ctor.
    let _manager = create_task_manager_no_recording().map_err(anyhow::Error::msg)?;

    // Block forever (until killed). In a future iteration, add graceful shutdown signals.
    futures::future::pending::<()>().await;
    Ok(())
}
