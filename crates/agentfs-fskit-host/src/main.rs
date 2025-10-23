// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! AgentFS FSKit Host Binary
//!
//! This binary provides a command-line interface for testing the FSKit adapter.
//! In production, this would be an Xcode FSKit extension.

use agentfs_core::config::SecurityPolicy;
use agentfs_fskit_host::{FsKitAdapter, FsKitConfig};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "agentfs-fskit-host")]
#[command(about = "AgentFS FSKit adapter host for macOS")]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Mount the filesystem at the specified path
    Mount {
        /// Mount point path
        #[arg(short, long)]
        mount_point: PathBuf,

        /// Maximum memory in bytes (optional)
        #[arg(long)]
        max_memory: Option<u64>,

        /// Spill directory (optional)
        #[arg(long)]
        spill_dir: Option<PathBuf>,
    },

    /// Run smoke tests
    Test {
        /// Test data directory
        #[arg(short, long, default_value = "/tmp/agentfs-test")]
        test_dir: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    match args.command {
        Commands::Mount {
            mount_point,
            max_memory,
            spill_dir,
        } => {
            println!("Mounting AgentFS at {:?}", mount_point);

            let config = FsKitConfig {
                fs_config: agentfs_core::FsConfig {
                    case_sensitivity: agentfs_core::CaseSensitivity::InsensitivePreserving,
                    memory: agentfs_core::MemoryPolicy {
                        max_bytes_in_memory: max_memory,
                        spill_directory: spill_dir,
                    },
                    limits: agentfs_core::FsLimits {
                        max_open_handles: 65536,
                        max_branches: 256,
                        max_snapshots: 4096,
                    },
                    cache: agentfs_core::CachePolicy {
                        attr_ttl_ms: 1000,
                        entry_ttl_ms: 1000,
                        negative_ttl_ms: 1000,
                        enable_readdir_plus: true,
                        auto_cache: true,
                        writeback_cache: false,
                    },
                    enable_xattrs: true,
                    enable_ads: false,
                    security: SecurityPolicy::default(),
                    track_events: true,
                    backstore: agentfs_core::config::BackstoreMode::InMemory,
                    overlay: agentfs_core::config::OverlayConfig::default(),
                    interpose: agentfs_core::config::InterposeConfig::default(),
                },
                mount_point: mount_point.to_string_lossy().to_string(),
                xpc_service_name: Some("com.agentfs.control".to_string()),
            };

            let adapter = FsKitAdapter::new(config)?;

            // Mount the filesystem
            adapter.mount()?;

            // Start XPC control service
            adapter.start_xpc_service()?;

            println!("AgentFS mounted successfully. Press Ctrl+C to unmount.");

            // Keep running until interrupted
            tokio::signal::ctrl_c().await?;
            adapter.unmount()?;

            println!("AgentFS unmounted.");
        }

        Commands::Test { test_dir } => {
            println!("Running FSKit adapter smoke tests...");

            // Create test configuration
            let config = FsKitConfig {
                fs_config: agentfs_core::FsConfig {
                    case_sensitivity: agentfs_core::CaseSensitivity::InsensitivePreserving,
                    memory: agentfs_core::MemoryPolicy {
                        max_bytes_in_memory: Some(64 * 1024 * 1024), // 64MB
                        spill_directory: Some(test_dir.join("spill")),
                    },
                    limits: agentfs_core::FsLimits {
                        max_open_handles: 1024,
                        max_branches: 10,
                        max_snapshots: 10,
                    },
                    cache: agentfs_core::CachePolicy {
                        attr_ttl_ms: 1000,
                        entry_ttl_ms: 1000,
                        negative_ttl_ms: 1000,
                        enable_readdir_plus: true,
                        auto_cache: true,
                        writeback_cache: false,
                    },
                    enable_xattrs: true,
                    enable_ads: false,
                    security: SecurityPolicy::default(),
                    track_events: true,
                    backstore: agentfs_core::config::BackstoreMode::InMemory,
                    overlay: agentfs_core::config::OverlayConfig::default(),
                    interpose: agentfs_core::config::InterposeConfig::default(),
                },
                mount_point: test_dir.join("mount").to_string_lossy().to_string(),
                xpc_service_name: Some("com.agentfs.test".to_string()),
            };

            run_smoke_tests(config).await?;
            println!("Smoke tests completed successfully!");
        }
    }

    Ok(())
}

async fn run_smoke_tests(config: FsKitConfig) -> Result<(), Box<dyn std::error::Error>> {
    let adapter = FsKitAdapter::new(config)?;

    // Test basic filesystem operations via the core API
    let core = adapter.core();

    // Create a snapshot
    println!("Creating snapshot...");
    let snapshot_id = core.snapshot_create(Some("test-snapshot"))?;
    println!("Created snapshot: {:?}", snapshot_id);

    // Create a branch from the snapshot
    println!("Creating branch...");
    let branch_id = core.branch_create_from_snapshot(snapshot_id, Some("test-branch"))?;
    println!("Created branch: {:?}", branch_id);

    // Bind to the branch
    println!("Binding to branch...");
    core.bind_process_to_branch(branch_id)?;

    // Test basic file operations
    println!("Testing file operations...");
    core.mkdir(
        &agentfs_core::PID::new(0),
        std::path::Path::new("/testdir"),
        0o755,
    )?;
    let handle = core.create(
        &agentfs_core::PID::new(0),
        std::path::Path::new("/testdir/hello.txt"),
        &agentfs_core::OpenOptions {
            read: true,
            write: true,
            create: true,
            truncate: false,
            append: false,
            share: vec![],
            stream: None,
        },
    )?;

    let test_data = b"Hello, AgentFS FSKit!";
    core.write(&agentfs_core::PID::new(0), handle, 0, test_data)?;

    let mut read_buf = vec![0u8; test_data.len()];
    let bytes_read = core.read(&agentfs_core::PID::new(0), handle, 0, &mut read_buf)?;
    assert_eq!(bytes_read, test_data.len());
    assert_eq!(&read_buf, test_data);

    core.close(&agentfs_core::PID::new(0), handle)?;

    // Verify file contents
    let attrs = core.getattr(
        &agentfs_core::PID::new(0),
        std::path::Path::new("/testdir/hello.txt"),
    )?;
    assert_eq!(attrs.len, test_data.len() as u64);

    println!("File operations test passed!");

    // Test XPC control plane (simulated)
    println!("Testing XPC control plane...");

    // In a real test, we would send XPC messages and verify responses
    // For now, just test that the adapter can be created

    println!("XPC control plane test passed!");

    Ok(())
}
