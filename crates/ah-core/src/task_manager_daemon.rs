// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Task manager daemon: keeps a LocalTaskManager alive and accepts launch requests over IPC.

use crate::{
    task_manager_dto::{DaemonRequest, DaemonResponse, LaunchTaskResponse},
    task_manager_init::create_dashboard_task_manager,
};
use anyhow::{Context, Result};
use serde_json;
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Mutex;
use tracing::{error, info};

/// Location of the daemon control socket (JSON/length-prefixed IPC)
pub fn daemon_socket_path() -> PathBuf {
    PathBuf::from("/tmp/ah/task-manager-daemon.sock")
}

async fn handle_connection(
    mut stream: UnixStream,
    manager: std::sync::Arc<dyn crate::TaskManager>,
) -> Result<()> {
    // Read length-prefixed request
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_le_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).await?;

    let req: DaemonRequest =
        serde_json::from_slice(&buf).context("Failed to parse daemon request")?;

    let resp = match req {
        DaemonRequest::Ping => DaemonResponse::Pong,
        DaemonRequest::LaunchTask(req) => match manager.launch_task(req.params).await {
            crate::TaskLaunchResult::Success { session_ids } => {
                DaemonResponse::LaunchTaskResult(LaunchTaskResponse {
                    session_ids,
                    error: None,
                })
            }
            crate::TaskLaunchResult::Failure { error } => {
                DaemonResponse::LaunchTaskResult(LaunchTaskResponse {
                    session_ids: vec![],
                    error: Some(error),
                })
            }
        },
    };

    let resp_bytes = serde_json::to_vec(&resp)?;
    let resp_len = (resp_bytes.len() as u32).to_le_bytes();
    stream.write_all(&resp_len).await?;
    stream.write_all(&resp_bytes).await?;
    Ok(())
}

/// Run the task manager daemon (blocking). Binds the daemon socket and serves requests.
pub async fn run_task_manager_daemon() -> Result<()> {
    let socket_path = daemon_socket_path();
    let _ = std::fs::remove_file(&socket_path);
    if let Some(dir) = socket_path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let listener = UnixListener::bind(&socket_path)
        .with_context(|| format!("Failed to bind daemon socket at {}", socket_path.display()))?;

    // Keep a LocalTaskManager alive for the session lifetime so record/follow can
    // connect to its task-manager socket even after the launching CLI exits.
    let manager = create_dashboard_task_manager().map_err(anyhow::Error::msg)?;
    let manager = std::sync::Arc::new(Mutex::new(manager));

    info!("Task manager daemon listening at {}", socket_path.display());

    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                let mgr = manager.clone();
                tokio::spawn(async move {
                    let guard = mgr.lock().await;
                    if let Err(e) = handle_connection(stream, guard.clone()).await {
                        error!("daemon request from {:?} failed: {}", addr, e);
                    }
                });
            }
            Err(e) => {
                error!("daemon accept error: {}", e);
            }
        }
    }
}
