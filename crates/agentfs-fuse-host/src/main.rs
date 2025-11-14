// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! AgentFS FUSE Host — Linux/macOS filesystem adapter
//!
//! This binary implements a FUSE host that mounts AgentFS volumes
//! using libfuse (Linux) or macFUSE (macOS).

#[cfg(all(feature = "fuse", target_os = "linux"))]
mod adapter;

#[cfg(all(feature = "fuse", target_os = "linux"))]
use adapter::AgentFsFuse;
use agentfs_core::FsConfig;
use anyhow::Result;
use clap::Parser;
use std::fs;
use std::path::PathBuf;
use tracing::{info, warn};

#[derive(Parser)]
struct Args {
    /// Mount point for the filesystem
    mount_point: PathBuf,

    /// Configuration file (JSON)
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Allow other users to access the filesystem
    #[arg(long)]
    allow_other: bool,

    /// Allow root to access the filesystem
    #[arg(long)]
    allow_root: bool,

    /// Auto unmount on process exit
    #[arg(long)]
    auto_unmount: bool,
}

fn load_config(config_path: Option<PathBuf>) -> Result<FsConfig> {
    match config_path {
        Some(path) => {
            let content = fs::read_to_string(&path)?;
            let config: FsConfig = serde_json::from_str(&content)?;
            Ok(config)
        }
        None => {
            // Default configuration
            Ok(FsConfig::default())
        }
    }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    info!("Starting AgentFS FUSE Host");
    info!("Mount point: {}", args.mount_point.display());

    let config = load_config(args.config)?;
    info!("Configuration loaded: {:?}", config);

    #[cfg(all(feature = "fuse", target_os = "linux"))]
    {
        let filesystem = AgentFsFuse::new(config.clone())?;

        let mut mount_options = vec![
            fuser::MountOption::FSName("agentfs".to_string()),
            fuser::MountOption::Subtype("agentfs".to_string()),
        ];

        info!(
            "Cache policy: attr={}ms entry={}ms negative={}ms readdir_plus={} auto_cache={} writeback_cache={}",
            config.cache.attr_ttl_ms,
            config.cache.entry_ttl_ms,
            config.cache.negative_ttl_ms,
            config.cache.enable_readdir_plus,
            config.cache.auto_cache,
            config.cache.writeback_cache
        );

        if args.allow_other {
            mount_options.push(fuser::MountOption::AllowOther);
        }

        if args.allow_root {
            mount_options.push(fuser::MountOption::AllowRoot);
        }

        if args.auto_unmount {
            mount_options.push(fuser::MountOption::AutoUnmount);
        }

        info!("Mounting filesystem...");
        fuser::mount2(filesystem, &args.mount_point, &mount_options)?;
    }

    #[cfg(not(all(feature = "fuse", target_os = "linux")))]
    {
        warn!("FUSE support not compiled in. This binary is for testing only.");
        info!(
            "AgentFS core initialized successfully with config: {:?}",
            config
        );
        info!("To enable FUSE support, compile with: cargo build --features fuse");
        // In a real implementation, we might want to keep the process running
        // or provide alternative functionality
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_config_loading_default() {
        let config = load_config(None).unwrap();
        assert!(config.enable_xattrs);
        assert!(!config.enable_ads);
    }

    #[test]
    fn test_config_loading_from_file() {
        let mut temp_file = NamedTempFile::new().unwrap();
        let config_json = r#"{
            "case_sensitivity": "Sensitive",
            "memory": {
                "max_bytes_in_memory": 1048576,
                "spill_directory": null
            },
            "limits": {
                "max_open_handles": 100,
                "max_branches": 10,
                "max_snapshots": 50
            },
            "cache": {
                "attr_ttl_ms": 500,
                "entry_ttl_ms": 500,
                "negative_ttl_ms": 500,
                "enable_readdir_plus": false,
                "auto_cache": false,
                "writeback_cache": true
            },
            "enable_xattrs": false,
            "enable_ads": true,
            "track_events": true,
            "security": {
                "enforce_posix_permissions": false,
                "default_uid": 0,
                "default_gid": 0,
                "enable_windows_acl_compat": false,
                "root_bypass_permissions": false
            },
            "backstore": {
                "InMemory": null
            },
            "overlay": {
                "enabled": false,
                "lower_root": null,
                "copyup_mode": "Lazy"
            },
            "interpose": {
                "enabled": false,
                "max_copy_bytes": 1048576,
                "require_reflink": false,
                "allow_windows_reparse": false
            }
        }"#;
        temp_file.write_all(config_json.as_bytes()).unwrap();
        temp_file.flush().unwrap();

        let config_path = Some(temp_file.path().to_path_buf());
        let config = load_config(config_path).unwrap();

        assert_eq!(config.limits.max_open_handles, 100);
        assert_eq!(config.limits.max_branches, 10);
        assert_eq!(config.limits.max_snapshots, 50);
        assert_eq!(config.cache.attr_ttl_ms, 500);
        assert!(!config.enable_xattrs);
        assert!(config.enable_ads);
        assert!(config.track_events);
    }

    #[cfg(feature = "fuse")]
    #[test]
    fn test_adapter_creation() {
        let config = FsConfig::default();
        let adapter = adapter::AgentFsFuse::new(config);
        assert!(adapter.is_ok());
    }
}
