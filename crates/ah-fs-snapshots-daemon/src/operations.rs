#![allow(dead_code)]

// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use crate::fuse_manager::AgentfsFuseManager;
use crate::interpose_manager::AgentfsInterposeManager;
use crate::types::{
    AgentfsFuseMountRequest, AgentfsInterposeMountHints, AgentfsInterposeMountRequest, Response,
};
use libc::geteuid;
use std::process::{Output, Stdio};
use std::sync::Arc;
use tokio::process::Command;
use tracing::{debug, error, warn};

pub async fn handle_mount_agentfs_fuse(
    manager: &AgentfsFuseManager,
    request: AgentfsFuseMountRequest,
    session_id: &str,
) -> Response {
    debug!(operation = "handle_mount_agentfs_fuse", session_id = %session_id, mount_point = ?String::from_utf8_lossy(&request.mount_point), timeout_ms = %request.mount_timeout_ms, "Handling FUSE mount request");

    match manager.mount(request).await {
        Ok(status) => {
            debug!(operation = "handle_mount_agentfs_fuse_success", session_id = %session_id, mount_point = ?String::from_utf8_lossy(&status.mount_point), state = ?status.state, "FUSE mount request completed successfully");
            Response::agentfs_fuse_status(status)
        }
        Err(err) => {
            debug!(operation = "handle_mount_agentfs_fuse_error", session_id = %session_id, error = %err, "FUSE mount request failed");
            Response::error(err.to_string())
        }
    }
}

pub async fn handle_mount_agentfs_interpose(
    state: &Arc<crate::server::DaemonState>,
    request: AgentfsInterposeMountRequest,
    hints: Option<AgentfsInterposeMountHints>,
    session_id: &str,
) -> Response {
    let manager = state.interpose_manager();
    debug!(operation = "handle_mount_agentfs_interpose", session_id = %session_id, repo_root_len = %request.repo_root.len(), timeout_ms = %request.mount_timeout_ms, has_hints = %hints.is_some(), "Handling interpose mount request");

    match manager
        .mount(
            request,
            hints,
            state.log_level(),
            state.log_to_file(),
            state.log_dir(),
            session_id,
        )
        .await
    {
        Ok(status) => {
            debug!(operation = "handle_mount_agentfs_interpose_success", session_id = %session_id, socket_path = ?status.socket_path, runtime_dir = ?status.runtime_dir, state = ?status.state, "Interpose mount request completed successfully");
            Response::agentfs_interpose_status(status)
        }
        Err(err) => {
            debug!(operation = "handle_mount_agentfs_interpose_error", session_id = %session_id, error = %err, "Interpose mount request failed");
            Response::error(err.to_string())
        }
    }
}

pub async fn handle_unmount_agentfs_interpose(manager: &AgentfsInterposeManager) -> Response {
    match manager.unmount().await {
        Ok(()) => Response::success(),
        Err(err) => Response::error(err.to_string()),
    }
}

pub async fn handle_status_agentfs_interpose(manager: &AgentfsInterposeManager) -> Response {
    let status = manager.status().await;
    Response::agentfs_interpose_status(status)
}

pub async fn handle_unmount_agentfs_fuse(manager: &AgentfsFuseManager) -> Response {
    debug!(
        operation = "handle_unmount_agentfs_fuse",
        "Handling FUSE unmount request"
    );

    match manager.unmount().await {
        Ok(()) => {
            debug!(
                operation = "handle_unmount_agentfs_fuse_success",
                "FUSE unmount request completed successfully"
            );
            Response::success()
        }
        Err(err) => {
            debug!(operation = "handle_unmount_agentfs_fuse_error", error = %err, "FUSE unmount request failed");
            Response::error(err.to_string())
        }
    }
}

pub async fn handle_status_agentfs_fuse(manager: &AgentfsFuseManager) -> Response {
    debug!(
        operation = "handle_status_agentfs_fuse",
        "Handling FUSE status request"
    );
    let status = manager.status().await;
    debug!(operation = "handle_status_agentfs_fuse_response", mount_point = ?String::from_utf8_lossy(&status.mount_point), state = ?status.state, "FUSE status request completed");
    Response::agentfs_fuse_status(status)
}

pub async fn handle_zfs_clone(snapshot: String, clone: String) -> Response {
    debug!(operation = "zfs_clone", snapshot = %snapshot, clone = %clone, "Creating ZFS clone");

    // Validate that the snapshot exists
    if !zfs_snapshot_exists(&snapshot).await {
        debug!(operation = "zfs_clone", snapshot = %snapshot, "Snapshot does not exist; returning error");
        return Response::error(format!("ZFS snapshot {} does not exist", snapshot));
    }

    // Validate that the clone dataset doesn't already exist
    if zfs_dataset_exists(&clone).await {
        debug!(operation = "zfs_clone", clone = %clone, "Clone dataset already exists; returning error");
        return Response::error(format!("ZFS dataset {} already exists", clone));
    }

    // Execute zfs clone with elevated privileges
    match run_privileged_command("zfs", &["clone", &snapshot, &clone]).await {
        Ok(_) => {
            // Get the mountpoint of the cloned dataset
            match get_zfs_mountpoint(&clone).await {
                Ok(mountpoint) => match mountpoint.as_str() {
                    "none" | "legacy" => {
                        debug!(operation = "zfs_clone", clone = %clone, mountpoint = %mountpoint, "Clone has mountpoint; returning success without mountpoint");
                        Response::success()
                    }
                    _ => {
                        debug!(operation = "zfs_clone", clone = %clone, mountpoint = %mountpoint, "Clone mounted (preserving original ownership)");
                        Response::success_with_mountpoint(mountpoint)
                    }
                },
                Err(e) => {
                    warn!(operation = "zfs_clone", clone = %clone, error = %e, "Failed to get mountpoint for clone");
                    Response::success() // Clone succeeded but mountpoint unknown
                }
            }
        }
        Err(e) => {
            error!(operation = "zfs_clone", clone = %clone, snapshot = %snapshot, error = %e, "Failed to create ZFS clone");
            debug!(operation = "zfs_clone", clone = %clone, error = %e, "Returning error to client for clone");
            Response::error(format!(
                "Failed to create ZFS clone {} from {}: {}",
                clone, snapshot, e
            ))
        }
    }
}

pub async fn handle_zfs_snapshot(source: String, snapshot: String) -> Response {
    debug!(operation = "zfs_snapshot", source = %source, snapshot = %snapshot, "Creating ZFS snapshot");

    // Validate that the source dataset exists
    if !zfs_dataset_exists(&source).await {
        return Response::error(format!("ZFS dataset {} does not exist", source));
    }

    // Validate that the snapshot doesn't already exist
    if zfs_snapshot_exists(&snapshot).await {
        return Response::error(format!("ZFS snapshot {} already exists", snapshot));
    }

    // Execute zfs snapshot with elevated privileges
    match run_privileged_command("zfs", &["snapshot", &snapshot]).await {
        Ok(_) => Response::success(),
        Err(e) => {
            error!(operation = "zfs_snapshot", snapshot = %snapshot, error = %e, "Failed to create ZFS snapshot");
            Response::error(format!("Failed to create ZFS snapshot {}: {}", snapshot, e))
        }
    }
}

pub async fn handle_zfs_delete(target: String) -> Response {
    debug!(operation = "zfs_delete", target = %target, "Deleting ZFS dataset");

    // Validate that the target dataset exists
    if !zfs_dataset_exists(&target).await {
        return Response::error(format!("ZFS dataset {} does not exist", target));
    }

    // Execute zfs destroy with elevated privileges
    match run_privileged_command("zfs", &["destroy", "-r", &target]).await {
        Ok(_) => Response::success(),
        Err(e) => {
            error!(operation = "zfs_delete", target = %target, error = %e, "Failed to delete ZFS dataset");
            Response::error(format!("Failed to delete ZFS dataset {}: {}", target, e))
        }
    }
}

pub async fn handle_btrfs_clone(source: String, destination: String) -> Response {
    debug!(operation = "btrfs_clone", source = %source, destination = %destination, "Creating Btrfs subvolume snapshot");

    // Validate that the source subvolume exists
    if !btrfs_subvolume_exists(&source).await {
        return Response::error(format!("Btrfs subvolume {} does not exist", source));
    }

    // Validate that the destination doesn't already exist
    if std::path::Path::new(&destination).exists() {
        return Response::error(format!("Destination {} already exists", destination));
    }

    // Execute btrfs subvolume snapshot with elevated privileges
    match run_privileged_command("btrfs", &["subvolume", "snapshot", &source, &destination]).await {
        Ok(_) => {
            // Set ownership to the user who started the daemon
            if let Some(user) = get_sudo_user() {
                let _ = run_privileged_command("chown", &["-R", &user, &destination]).await;
            }
            Response::success_with_path(destination)
        }
        Err(e) => {
            error!(operation = "btrfs_clone", source = %source, destination = %destination, error = %e, "Failed to create Btrfs snapshot");
            Response::error(format!(
                "Failed to create Btrfs snapshot {} from {}: {}",
                destination, source, e
            ))
        }
    }
}

pub async fn handle_btrfs_snapshot(source: String, destination: String) -> Response {
    // For Btrfs, clone and snapshot are the same operation (subvolume snapshot)
    handle_btrfs_clone(source, destination).await
}

pub async fn handle_btrfs_delete(target: String) -> Response {
    debug!(operation = "btrfs_delete", target = %target, "Deleting Btrfs subvolume");

    // Validate that the target subvolume exists
    if !btrfs_subvolume_exists(&target).await {
        return Response::error(format!("Btrfs subvolume {} does not exist", target));
    }

    // Execute btrfs subvolume delete with elevated privileges
    match run_privileged_command("btrfs", &["subvolume", "delete", "-R", &target]).await {
        Ok(_) => Response::success(),
        Err(e) => {
            error!(operation = "btrfs_delete", target = %target, error = %e, "Failed to delete Btrfs subvolume");
            Response::error(format!(
                "Failed to delete Btrfs subvolume {}: {}",
                target, e
            ))
        }
    }
}

fn running_as_root() -> bool {
    unsafe { geteuid() == 0 }
}

async fn run_privileged_command(program: &str, args: &[&str]) -> Result<Output, String> {
    if running_as_root() {
        run_command(program, args).await
    } else {
        let mut full_args: Vec<String> = Vec::with_capacity(args.len() + 2);
        full_args.push("-n".to_string());
        full_args.push(program.to_string());
        full_args.extend(args.iter().map(|s| s.to_string()));
        let ref_args: Vec<&str> = full_args.iter().map(|s| s.as_str()).collect();
        run_command("sudo", &ref_args).await
    }
}

async fn run_command(program: &str, args: &[&str]) -> Result<Output, String> {
    debug!(operation = "run_command", program = %program, args = ?args, "Running command");

    let output = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("Failed to execute command '{}': {}", program, e))?;

    if output.status.success() {
        Ok(output)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(stderr.trim().to_string())
    }
}

async fn get_zfs_mountpoint(dataset: &str) -> Result<String, String> {
    let output =
        run_privileged_command("zfs", &["get", "-H", "-o", "value", "mountpoint", dataset]).await?;

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn get_sudo_user() -> Option<String> {
    std::env::var("SUDO_USER").ok().or_else(|| std::env::var("USER").ok())
}

pub async fn handle_zfs_list_snapshots(dataset: String) -> Response {
    debug!(operation = "zfs_list_snapshots", dataset = %dataset, "Listing ZFS snapshots for dataset");

    // Run zfs list to get all snapshots for this dataset
    let result = run_privileged_command(
        "zfs",
        &["list", "-t", "snapshot", "-H", "-o", "name", "-r", &dataset],
    )
    .await;

    match result {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let snapshots: Vec<String> = stdout
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(|line| line.trim().to_string())
                .collect();

            match serde_json::to_string(&snapshots) {
                Ok(json) => Response::success_with_list(json),
                Err(e) => Response::error(format!("Failed to serialize snapshot list: {}", e)),
            }
        }
        Err(e) => Response::error(format!("Failed to execute zfs list: {}", e)),
    }
}

pub async fn zfs_dataset_exists(dataset: &str) -> bool {
    run_privileged_command("zfs", &["list", dataset]).await.is_ok()
}

pub async fn zfs_snapshot_exists(snapshot: &str) -> bool {
    run_privileged_command("zfs", &["list", "-t", "snapshot", snapshot])
        .await
        .is_ok()
}

pub async fn btrfs_subvolume_exists(path: &str) -> bool {
    // Check if path exists and is a btrfs subvolume
    run_privileged_command("btrfs", &["subvolume", "show", path]).await.is_ok()
}
