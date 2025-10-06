use crate::types::{Request, Response};
use std::process::Stdio;
use tokio::process::Command;
use tracing::{debug, error, info, warn};

pub async fn process_request(request: Request) -> Response {
    info!("Processing request: {:?}", request);

    match request {
        Request::Ping(_) => Response::success(),
        Request::ListZfsSnapshots(dataset) => {
            let dataset_str = String::from_utf8_lossy(&dataset).to_string();
            handle_zfs_list_snapshots(dataset_str).await
        }
        Request::CloneZfs((snapshot, clone)) => {
            let snapshot_str = String::from_utf8_lossy(&snapshot).to_string();
            let clone_str = String::from_utf8_lossy(&clone).to_string();
            handle_zfs_clone(snapshot_str, clone_str).await
        }
        Request::SnapshotZfs((source, snapshot)) => {
            let source_str = String::from_utf8_lossy(&source).to_string();
            let snapshot_str = String::from_utf8_lossy(&snapshot).to_string();
            handle_zfs_snapshot(source_str, snapshot_str).await
        }
        Request::DeleteZfs(target) => {
            let target_str = String::from_utf8_lossy(&target).to_string();
            handle_zfs_delete(target_str).await
        }
        Request::CloneBtrfs((source, destination)) => {
            let source_str = String::from_utf8_lossy(&source).to_string();
            let destination_str = String::from_utf8_lossy(&destination).to_string();
            handle_btrfs_clone(source_str, destination_str).await
        }
        Request::SnapshotBtrfs((source, destination)) => {
            let source_str = String::from_utf8_lossy(&source).to_string();
            let destination_str = String::from_utf8_lossy(&destination).to_string();
            handle_btrfs_snapshot(source_str, destination_str).await
        }
        Request::DeleteBtrfs(target) => {
            let target_str = String::from_utf8_lossy(&target).to_string();
            handle_btrfs_delete(target_str).await
        }
    }
}

async fn handle_zfs_clone(snapshot: String, clone: String) -> Response {
    debug!("Creating ZFS clone {} from {}", clone, snapshot);

    // Validate that the snapshot exists
    if !zfs_snapshot_exists(&snapshot).await {
        return Response::error(format!("ZFS snapshot {} does not exist", snapshot));
    }

    // Validate that the clone dataset doesn't already exist
    if zfs_dataset_exists(&clone).await {
        return Response::error(format!("ZFS dataset {} already exists", clone));
    }

    // Execute zfs clone with sudo
    match run_command("sudo", &["zfs", "clone", &snapshot, &clone]).await {
        Ok(_) => {
            // Get the mountpoint of the cloned dataset
            match get_zfs_mountpoint(&clone).await {
                Ok(mountpoint) => {
                    if mountpoint != "none" && mountpoint != "legacy" {
                        // Set ownership to the user who started the daemon
                        if let Some(user) = get_sudo_user() {
                            let _ = run_command("sudo", &["chown", "-R", &user, &mountpoint]).await;
                        }
                        Response::success_with_mountpoint(mountpoint)
                    } else {
                        Response::success()
                    }
                }
                Err(e) => {
                    warn!("Failed to get mountpoint for clone {}: {}", clone, e);
                    Response::success() // Clone succeeded but mountpoint unknown
                }
            }
        }
        Err(e) => {
            error!(
                "Failed to create ZFS clone {} from {}: {}",
                clone, snapshot, e
            );
            Response::error(format!(
                "Failed to create ZFS clone {} from {}: {}",
                clone, snapshot, e
            ))
        }
    }
}

async fn handle_zfs_snapshot(source: String, snapshot: String) -> Response {
    debug!("Creating ZFS snapshot {} from {}", snapshot, source);

    // Validate that the source dataset exists
    if !zfs_dataset_exists(&source).await {
        return Response::error(format!("ZFS dataset {} does not exist", source));
    }

    // Validate that the snapshot doesn't already exist
    if zfs_snapshot_exists(&snapshot).await {
        return Response::error(format!("ZFS snapshot {} already exists", snapshot));
    }

    // Execute zfs snapshot with sudo
    match run_command("sudo", &["zfs", "snapshot", &snapshot]).await {
        Ok(_) => Response::success(),
        Err(e) => {
            error!("Failed to create ZFS snapshot {}: {}", snapshot, e);
            Response::error(format!("Failed to create ZFS snapshot {}: {}", snapshot, e))
        }
    }
}

async fn handle_zfs_delete(target: String) -> Response {
    debug!("Deleting ZFS dataset {}", target);

    // Validate that the target dataset exists
    if !zfs_dataset_exists(&target).await {
        return Response::error(format!("ZFS dataset {} does not exist", target));
    }

    // Execute zfs destroy with sudo
    match run_command("sudo", &["zfs", "destroy", "-r", &target]).await {
        Ok(_) => Response::success(),
        Err(e) => {
            error!("Failed to delete ZFS dataset {}: {}", target, e);
            Response::error(format!("Failed to delete ZFS dataset {}: {}", target, e))
        }
    }
}

async fn handle_btrfs_clone(source: String, destination: String) -> Response {
    debug!(
        "Creating Btrfs subvolume snapshot {} from {}",
        destination, source
    );

    // Validate that the source subvolume exists
    if !btrfs_subvolume_exists(&source).await {
        return Response::error(format!("Btrfs subvolume {} does not exist", source));
    }

    // Validate that the destination doesn't already exist
    if std::path::Path::new(&destination).exists() {
        return Response::error(format!("Destination {} already exists", destination));
    }

    // Execute btrfs subvolume snapshot with sudo
    match run_command(
        "sudo",
        &["btrfs", "subvolume", "snapshot", &source, &destination],
    )
    .await
    {
        Ok(_) => {
            // Set ownership to the user who started the daemon
            if let Some(user) = get_sudo_user() {
                let _ = run_command("sudo", &["chown", "-R", &user, &destination]).await;
            }
            Response::success_with_path(destination)
        }
        Err(e) => {
            error!(
                "Failed to create Btrfs snapshot {} from {}: {}",
                destination, source, e
            );
            Response::error(format!(
                "Failed to create Btrfs snapshot {} from {}: {}",
                destination, source, e
            ))
        }
    }
}

async fn handle_btrfs_snapshot(source: String, destination: String) -> Response {
    // For Btrfs, clone and snapshot are the same operation (subvolume snapshot)
    handle_btrfs_clone(source, destination).await
}

async fn handle_btrfs_delete(target: String) -> Response {
    debug!("Deleting Btrfs subvolume {}", target);

    // Validate that the target subvolume exists
    if !btrfs_subvolume_exists(&target).await {
        return Response::error(format!("Btrfs subvolume {} does not exist", target));
    }

    // Execute btrfs subvolume delete with sudo
    match run_command("sudo", &["btrfs", "subvolume", "delete", "-R", &target]).await {
        Ok(_) => Response::success(),
        Err(e) => {
            error!("Failed to delete Btrfs subvolume {}: {}", target, e);
            Response::error(format!(
                "Failed to delete Btrfs subvolume {}: {}",
                target, e
            ))
        }
    }
}

async fn run_command(program: &str, args: &[&str]) -> Result<(), String> {
    debug!("Running command: {} {}", program, args.join(" "));

    let output = Command::new(program)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("Failed to execute command: {}", e))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(stderr.trim().to_string())
    }
}

async fn get_zfs_mountpoint(dataset: &str) -> Result<String, String> {
    let output = Command::new("sudo")
        .args(["zfs", "get", "-H", "-o", "value", "mountpoint", dataset])
        .output()
        .await
        .map_err(|e| format!("Failed to get mountpoint: {}", e))?;

    if output.status.success() {
        let mountpoint = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(mountpoint)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(stderr.trim().to_string())
    }
}

fn get_sudo_user() -> Option<String> {
    std::env::var("SUDO_USER").ok().or_else(|| std::env::var("USER").ok())
}

async fn handle_zfs_list_snapshots(dataset: String) -> Response {
    debug!("Listing ZFS snapshots for dataset: {}", dataset);

    // Run zfs list to get all snapshots for this dataset
    let result = Command::new("zfs")
        .args(["list", "-t", "snapshot", "-H", "-o", "name", "-r", &dataset])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await;

    match result {
        Ok(output) if output.status.success() => {
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
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Response::error(format!("ZFS list command failed: {}", stderr))
        }
        Err(e) => Response::error(format!("Failed to execute zfs list: {}", e)),
    }
}

pub async fn zfs_dataset_exists(dataset: &str) -> bool {
    run_command("zfs", &["list", dataset]).await.is_ok()
}

pub async fn zfs_snapshot_exists(snapshot: &str) -> bool {
    run_command("zfs", &["list", "-t", "snapshot", snapshot]).await.is_ok()
}

pub async fn btrfs_subvolume_exists(path: &str) -> bool {
    // Check if path exists and is a btrfs subvolume
    run_command("btrfs", &["subvolume", "show", path]).await.is_ok()
}
