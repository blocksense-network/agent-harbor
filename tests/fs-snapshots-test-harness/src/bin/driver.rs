// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

#[cfg(all(feature = "agentfs", target_os = "linux"))]
use ah_fs_snapshots_daemon::client::AgentfsFuseStatusData;
#[cfg(all(feature = "agentfs", target_os = "macos"))]
use anyhow::Context;
use anyhow::Result;
use clap::{Parser, Subcommand};
use fs_snapshots_test_harness::agentfs::{self, Transport};
#[cfg(target_os = "macos")]
use fs_snapshots_test_harness::assert_interpose_shim_exists;
use fs_snapshots_test_harness::{assert_driver_exists, scenarios};
#[cfg(target_os = "macos")]
use std::env;
#[cfg(all(feature = "agentfs", target_os = "macos"))]
use std::fs;
use tracing::info;

#[cfg(all(feature = "agentfs", target_os = "linux"))]
use fs_snapshots_test_harness::agentfs::{BackstoreSpec, FuseHarness};

#[derive(Parser, Debug)]
#[command(
    name = "fs-snapshots-harness-driver",
    about = "Launch scenarios that exercise filesystem snapshot providers via an external process."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Verify that the AgentFS interpose shim can be located and (on macOS)
    /// arranged for loading via DYLD_INSERT_LIBRARIES.
    ShimSmoke,
    /// Exercise the Git snapshot provider end-to-end (provider detection,
    /// workspace preparation, snapshot creation, readonly mount, branch).
    GitSnapshot,
    /// Exercise the Btrfs snapshot provider using an external process.
    #[cfg(feature = "btrfs")]
    BtrfsSnapshot,
    /// Exercise the ZFS snapshot provider using an external process (requires
    /// root privileges and ZFS tooling to be installed).
    #[cfg(feature = "zfs")]
    ZfsSnapshot,
    /// Run the provider matrix – this is meant to be dispatched by the Rust
    /// test harness so each provider can run in its own child process.
    ProviderMatrix {
        #[arg(long)]
        provider: String,
    },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt().with_writer(std::io::stdout).init();

    // Ensure the driver binary path is resolved (this also provides nicer error
    // messages if the crate is misbuilt).
    let _ = assert_driver_exists()?;

    let cli = Cli::parse();
    match cli.command {
        Command::ShimSmoke => run_shim_smoke()?,
        Command::GitSnapshot => scenarios::git_snapshot_scenario()?,
        #[cfg(feature = "btrfs")]
        Command::BtrfsSnapshot => scenarios::btrfs_snapshot_scenario()?,
        #[cfg(feature = "zfs")]
        Command::ZfsSnapshot => scenarios::zfs_snapshot_scenario()?,
        Command::ProviderMatrix { provider } => run_provider_matrix(provider)?,
    };

    Ok(())
}

#[cfg(target_os = "macos")]
fn run_shim_smoke() -> Result<()> {
    let shim_path = assert_interpose_shim_exists()?;
    env::set_var("DYLD_INSERT_LIBRARIES", &shim_path);

    info!(path = %shim_path.display(), "shim located");
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn run_shim_smoke() -> Result<()> {
    info!("shim smoke test unsupported on this platform");
    Ok(())
}

fn run_provider_matrix(provider: String) -> Result<()> {
    let provider = provider.to_lowercase();

    if provider == "agentfs" {
        match agentfs::requested_transport() {
            Transport::Interpose => {
                #[cfg(all(feature = "agentfs", target_os = "macos"))]
                {
                    let shim_path = assert_interpose_shim_exists()?;
                    env::set_var("DYLD_INSERT_LIBRARIES", &shim_path);
                    env::set_var("AH_ENABLE_AGENTFS_PROVIDER", "1");
                    let socket_dir =
                        env::temp_dir().join(format!("agentfs-driver-{}", std::process::id()));
                    fs::create_dir_all(&socket_dir)?;
                    let socket_path = socket_dir.join("agentfs.sock");
                    let _ = fs::remove_file(&socket_path);
                    env::set_var("AGENTFS_INTERPOSE_SOCKET", &socket_path);
                    let exe_path =
                        env::current_exe().context("failed to resolve driver executable path")?;
                    env::set_var("AGENTFS_INTERPOSE_EXE", &exe_path);
                    info!(path = %shim_path.display(), "AgentFS interpose shim configured for matrix run");
                    info!(flag = ?env::var("AH_ENABLE_AGENTFS_PROVIDER").ok(), "AgentFS provider opt-in flag set");
                }

                #[cfg(not(all(feature = "agentfs", target_os = "macos")))]
                {
                    info!("AgentFS interpose transport unavailable on this platform");
                    return Ok(());
                }
            }
            Transport::Fuse => {
                #[cfg(all(feature = "agentfs", target_os = "linux"))]
                {
                    run_agentfs_fuse_matrix()?;
                    return Ok(());
                }

                #[cfg(not(all(feature = "agentfs", target_os = "linux")))]
                {
                    info!("AgentFS FUSE transport unavailable on this platform");
                    return Ok(());
                }
            }
        }
    }

    scenarios::provider_matrix(&provider)
}

#[cfg(all(feature = "agentfs", target_os = "linux"))]
fn run_agentfs_fuse_matrix() -> Result<()> {
    let harness = match FuseHarness::new() {
        Ok(harness) => harness,
        Err(err) => {
            info!("Skipping AgentFS FUSE matrix: {err}");
            return Ok(());
        }
    };
    let specs = agentfs::parse_backstore_matrix();
    if specs.is_empty() {
        info!("No AgentFS backstores configured; skipping FUSE matrix");
        return Ok(());
    }

    std::env::set_var(agentfs::ENV_TRANSPORT, "fuse");
    std::env::set_var(
        "AH_FS_SNAPSHOTS_DAEMON_SOCKET",
        harness.socket_path().to_string_lossy().to_string(),
    );

    for spec in specs {
        std::env::set_var("AGENTFS_FUSE_BACKSTORE", spec.label());
        std::env::set_var(
            "AGENTFS_FUSE_MOUNT_POINT",
            harness.mount_point().to_string_lossy().to_string(),
        );
        std::env::set_var(
            "AGENTFS_FUSE_REPO_ROOT",
            harness.repo_root().to_string_lossy().to_string(),
        );

        let status = match harness.ensure_mounted(&spec) {
            Ok(status) => status,
            Err(err) => {
                info!(
                    "Skipping AgentFS backstore {}: failed to mount via daemon: {}",
                    spec.label(),
                    err
                );
                continue;
            }
        };
        log_fuse_status(&spec, &status);

        scenarios::provider_matrix("agentfs")?;
    }

    Ok(())
}

#[cfg(all(feature = "agentfs", target_os = "linux"))]
fn log_fuse_status(backstore: &BackstoreSpec, status: &AgentfsFuseStatusData) {
    let mount_point = String::from_utf8_lossy(&status.mount_point);
    let log_path = String::from_utf8_lossy(&status.log_path);
    let runtime_dir = String::from_utf8_lossy(&status.runtime_dir);
    let last_error = if status.last_error.is_empty() {
        None
    } else {
        Some(String::from_utf8_lossy(&status.last_error).to_string())
    };

    info!(
        backstore = backstore.label(),
        pid = status.pid,
        restart_count = status.restart_count,
        mount_point = %mount_point,
        log_path = %log_path,
        runtime_dir = %runtime_dir,
        "AgentFS FUSE status ready"
    );

    if let Some(err) = last_error {
        info!(
            backstore = backstore.label(),
            last_error = err,
            "AgentFS daemon reported last_error"
        );
    }
}
