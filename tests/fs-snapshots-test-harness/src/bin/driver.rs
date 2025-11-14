// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

#[cfg(all(feature = "agentfs", target_os = "macos"))]
use anyhow::Context;
use anyhow::Result;
use clap::{Parser, Subcommand};
use fs_snapshots_test_harness::{assert_driver_exists, assert_interpose_shim_exists, scenarios};
use std::env;
#[cfg(all(feature = "agentfs", target_os = "macos"))]
use std::fs;

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

    println!("shim located at {}", shim_path.display());
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn run_shim_smoke() -> Result<()> {
    println!("shim smoke test unsupported on this platform");
    Ok(())
}

fn run_provider_matrix(provider: String) -> Result<()> {
    let provider = provider.to_lowercase();

    if provider == "agentfs" {
        #[cfg(all(feature = "agentfs", target_os = "macos"))]
        {
            let shim_path = assert_interpose_shim_exists()?;
            env::set_var("DYLD_INSERT_LIBRARIES", &shim_path);
            env::set_var("AH_ENABLE_AGENTFS_PROVIDER", "1");
            let socket_dir = env::temp_dir().join(format!("agentfs-driver-{}", std::process::id()));
            fs::create_dir_all(&socket_dir)?;
            let socket_path = socket_dir.join("agentfs.sock");
            let _ = fs::remove_file(&socket_path);
            env::set_var("AGENTFS_INTERPOSE_SOCKET", &socket_path);
            let exe_path =
                env::current_exe().context("failed to resolve driver executable path")?;
            env::set_var("AGENTFS_INTERPOSE_EXE", &exe_path);
            println!(
                "AgentFS interpose shim configured for matrix run: {}",
                shim_path.display()
            );
            println!(
                "AgentFS provider opt-in flag set to {:?}",
                env::var("AH_ENABLE_AGENTFS_PROVIDER").ok()
            );
        }

        #[cfg(not(all(feature = "agentfs", target_os = "macos")))]
        {
            println!(
                "AgentFS provider matrix unavailable (feature disabled or unsupported platform)"
            );
            return Ok(());
        }
    }

    scenarios::provider_matrix(&provider)
}
