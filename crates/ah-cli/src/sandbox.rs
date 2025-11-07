// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use ah_fs_snapshots::{
    FsSnapshotProvider, PreparedWorkspace, SnapshotProviderKind, WorkingCopyMode,
};
use anyhow::{Context, Result};
use clap::Args;
#[cfg(target_os = "linux")]
use sandbox_core::ProcessConfig;
use std::path::{Path, PathBuf};

/// Arguments for running a command in a sandbox
#[derive(Args, Clone)]
pub struct SandboxRunArgs {
    /// Sandbox type (currently only 'local' is supported)
    #[arg(long = "type", default_value = "local")]
    pub sandbox_type: String,

    /// Allow internet access via slirp4netns
    #[arg(long = "allow-network", value_name = "BOOL", default_value = "no")]
    pub allow_network: String,

    /// Enable container device access (/dev/fuse, storage dirs)
    #[arg(long = "allow-containers", value_name = "BOOL", default_value = "no")]
    pub allow_containers: String,

    /// Enable KVM device access for VMs (/dev/kvm)
    #[arg(long = "allow-kvm", value_name = "BOOL", default_value = "no")]
    pub allow_kvm: String,

    /// Enable dynamic filesystem access control
    #[arg(long = "seccomp", value_name = "BOOL", default_value = "no")]
    pub seccomp: String,

    /// Enable debugging operations in sandbox
    #[arg(long = "seccomp-debug", value_name = "BOOL", default_value = "no")]
    pub seccomp_debug: String,

    /// Additional writable paths to bind mount
    #[arg(long = "mount-rw", value_name = "PATH")]
    pub mount_rw: Vec<PathBuf>,

    /// Paths to promote to copy-on-write overlays
    #[arg(long = "overlay", value_name = "PATH")]
    pub overlay: Vec<PathBuf>,

    /// Command and arguments to run in the sandbox
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub command: Vec<String>,
}

impl SandboxRunArgs {
    /// Execute the sandbox run command
    pub async fn run(self) -> Result<()> {
        let SandboxRunArgs {
            sandbox_type,
            allow_network,
            allow_containers,
            allow_kvm,
            seccomp,
            seccomp_debug,
            mount_rw,
            overlay,
            command,
        } = self;

        if sandbox_type != "local" {
            return Err(anyhow::anyhow!(
                "Only 'local' sandbox type is currently supported"
            ));
        }

        let allow_network = parse_bool_flag(&allow_network)?;
        let allow_containers = parse_bool_flag(&allow_containers)?;
        let allow_kvm = parse_bool_flag(&allow_kvm)?;
        let seccomp = parse_bool_flag(&seccomp)?;
        let seccomp_debug = parse_bool_flag(&seccomp_debug)?;

        if command.is_empty() {
            return Err(anyhow::anyhow!("No command specified to run in sandbox"));
        }

        let workspace_path =
            std::env::current_dir().context("Failed to get current working directory")?;

        let prepared_workspace = prepare_workspace_with_fallback(&workspace_path)
            .await
            .context("Failed to prepare writable workspace with any provider")?;

        println!(
            "Prepared workspace at: {}",
            prepared_workspace.exec_path.display()
        );
        println!("Working copy mode: {:?}", prepared_workspace.working_copy);
        println!("Provider: {:?}", prepared_workspace.provider);

        if prepared_workspace.provider == SnapshotProviderKind::Disable {
            println!("⚠️  No filesystem snapshot provider available; using in-place workspace");
        }

        let cleanup_token = prepared_workspace.cleanup_token.clone();
        let provider_kind = prepared_workspace.provider;
        let env_vars: Vec<(String, String)> = std::env::vars().collect();

        #[cfg(target_os = "linux")]
        let result: Result<()> = {
            let exec_dir = prepared_workspace.exec_path.clone();

            println!(
                "Running command inside sandbox workspace: {}",
                exec_dir.display()
            );
            println!("Command: {:?}", command);
            println!("Configuration:");
            println!("  Allow network: {}", allow_network);
            println!("  Allow containers: {}", allow_containers);
            println!("  Allow KVM: {}", allow_kvm);
            println!("  Seccomp: {}", seccomp);
            println!("  Seccomp debug: {}", seccomp_debug);
            println!("  Mount RW paths: {:?}", mount_rw);
            println!("  Overlay paths: {:?}", overlay);

            let mut sandbox = create_sandbox_from_args(
                allow_network,
                allow_containers,
                allow_kvm,
                seccomp,
                seccomp_debug,
                &mount_rw,
                &overlay,
                Some(exec_dir.as_path()),
            )?
            .with_process_config(ProcessConfig {
                command,
                working_dir: Some(exec_dir.to_string_lossy().to_string()),
                env: env_vars,
            });

            let exec_result = sandbox.exec_process().await;

            if let Err(err) = sandbox.stop() {
                eprintln!("⚠️  Sandbox stop cleanup encountered an error: {}", err);
            }

            if let Err(err) = sandbox.cleanup().await {
                eprintln!(
                    "⚠️  Sandbox filesystem cleanup encountered an error: {}",
                    err
                );
            }

            let outcome =
                exec_result.context("Failed to execute command inside sandbox").map(|_| {
                    println!("✅ Sandbox command completed successfully");
                });

            cleanup_prepared_workspace(&workspace_path, provider_kind, &cleanup_token);

            outcome
        };

        #[cfg(not(target_os = "linux"))]
        let result: Result<()> = {
            cleanup_prepared_workspace(&workspace_path, provider_kind, &cleanup_token);
            Err(anyhow::anyhow!(
                "Sandbox functionality is only available on Linux"
            ))
        };

        result
    }
}

/// Prepare a writable workspace using FS snapshots with fallback logic
pub async fn prepare_workspace_with_fallback(
    workspace_path: &std::path::Path,
) -> Result<PreparedWorkspace> {
    // Try providers in order of preference: ZFS -> Btrfs -> Git
    #[cfg_attr(
        not(any(feature = "zfs", feature = "btrfs", feature = "git")),
        allow(unused_mut)
    )]
    let mut providers_to_try: Vec<(&str, fn() -> Result<Box<dyn FsSnapshotProvider>>)> = Vec::new();

    #[cfg(feature = "zfs")]
    providers_to_try.push(("ZFS", || -> Result<Box<dyn FsSnapshotProvider>> {
        Ok(Box::new(ah_fs_snapshots_zfs::ZfsProvider::new()) as Box<dyn FsSnapshotProvider>)
    }));

    #[cfg(feature = "btrfs")]
    providers_to_try.push(("Btrfs", || -> Result<Box<dyn FsSnapshotProvider>> {
        Ok(Box::new(ah_fs_snapshots_btrfs::BtrfsProvider::new()) as Box<dyn FsSnapshotProvider>)
    }));

    #[cfg(feature = "git")]
    providers_to_try.push(("Git", || -> Result<Box<dyn FsSnapshotProvider>> {
        Ok(Box::new(ah_fs_snapshots_git::GitProvider::new()) as Box<dyn FsSnapshotProvider>)
    }));

    for (name, provider_fn) in providers_to_try {
        let provider = provider_fn()?;
        let capabilities = provider.detect_capabilities(workspace_path);

        if capabilities.score > 0 {
            // Try CoW overlay mode first if supported, otherwise try in-place mode
            let modes_to_try = if capabilities.supports_cow_overlay {
                vec![WorkingCopyMode::CowOverlay]
            } else {
                vec![WorkingCopyMode::InPlace]
            };

            for mode in modes_to_try {
                println!("  Trying mode: {:?}", mode);
                match provider.prepare_writable_workspace(workspace_path, mode) {
                    Ok(workspace) => {
                        println!(
                            "Successfully prepared workspace with {} provider using {:?}",
                            name, mode
                        );
                        return Ok(workspace);
                    }
                    Err(err) => {
                        tracing::debug!(
                            "Snapshot provider {} failed in {:?} mode: {}",
                            name,
                            mode,
                            err
                        );
                        continue;
                    }
                }
            }
        }
    }

    println!("⚠️  No filesystem snapshot providers available; falling back to in-place workspace");
    Ok(PreparedWorkspace {
        exec_path: workspace_path.to_path_buf(),
        working_copy: WorkingCopyMode::InPlace,
        provider: SnapshotProviderKind::Disable,
        cleanup_token: ah_fs_snapshots::generate_unique_id(),
    })
}

/// Create a sandbox instance configured from CLI parameters
#[cfg(target_os = "linux")]
pub fn create_sandbox_from_args(
    allow_network: bool,
    allow_containers: bool,
    allow_kvm: bool,
    seccomp: bool,
    seccomp_debug: bool,
    mount_rw: &[PathBuf],
    overlay: &[PathBuf],
    working_dir: Option<&Path>,
) -> Result<sandbox_core::Sandbox> {
    let mut sandbox = sandbox_core::Sandbox::new();

    #[cfg(not(feature = "seccomp"))]
    let _ = (seccomp, seccomp_debug);

    sandbox = sandbox.with_default_cgroups();

    if allow_network {
        sandbox = sandbox.with_default_network();
    }

    #[cfg(feature = "seccomp")]
    if seccomp {
        let root_dir = working_dir
            .map(|dir| dir.to_path_buf())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));

        let seccomp_config = sandbox_seccomp::SeccompConfig {
            debug_mode: seccomp_debug,
            supervisor_tx: None,
            root_dir,
        };
        sandbox = sandbox.with_seccomp(seccomp_config);
    }

    if allow_containers || allow_kvm {
        if allow_containers && allow_kvm {
            sandbox = sandbox.with_container_and_vm_devices();
        } else if allow_containers {
            sandbox = sandbox.with_container_devices();
        } else if allow_kvm {
            sandbox = sandbox.with_vm_devices();
        }
    }

    let mut fs_config = sandbox_core::FilesystemConfig::default();

    if let Some(dir) = working_dir {
        fs_config.working_dir = Some(dir.to_string_lossy().to_string());
    }

    for path in mount_rw {
        let path_str = path.to_string_lossy().to_string();
        fs_config.bind_mounts.push((path_str.clone(), path_str.clone()));
    }

    for path in overlay {
        fs_config.overlay_paths.push(path.to_string_lossy().to_string());
    }

    sandbox = sandbox.with_filesystem(fs_config);

    Ok(sandbox)
}

/// Create a sandbox instance configured from CLI parameters (non-Linux stub)
#[cfg(not(target_os = "linux"))]
pub fn create_sandbox_from_args(
    _allow_network: bool,
    _allow_containers: bool,
    _allow_kvm: bool,
    _seccomp: bool,
    _seccomp_debug: bool,
    _mount_rw: &[PathBuf],
    _overlay: &[PathBuf],
    _working_dir: Option<&Path>,
) -> Result<()> {
    Err(anyhow::anyhow!(
        "Sandbox functionality is only available on Linux"
    ))
}

fn cleanup_prepared_workspace(
    workspace_path: &Path,
    provider_kind: SnapshotProviderKind,
    cleanup_token: &str,
) {
    if provider_kind == SnapshotProviderKind::Disable {
        // No provider resources were allocated; nothing to clean up.
        return;
    }

    match ah_fs_snapshots::provider_for(workspace_path) {
        Ok(provider) => {
            if let Err(err) = provider.cleanup(cleanup_token) {
                eprintln!(
                    "⚠️  Failed to cleanup sandbox workspace (token={}): {}",
                    cleanup_token, err
                );
            }
        }
        Err(err) => {
            eprintln!(
                "⚠️  Unable to clean up sandbox workspace (provider lookup failed): {}",
                err
            );
        }
    }
}

/// Parse a boolean flag string (yes/no, true/false, 1/0)
pub fn parse_bool_flag(s: &str) -> Result<bool> {
    match s.to_lowercase().as_str() {
        "yes" | "true" | "1" => Ok(true),
        "no" | "false" | "0" => Ok(false),
        _ => Err(anyhow::anyhow!(
            "Invalid boolean value: '{}'. Expected yes/no, true/false, or 1/0",
            s
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bool_flag() {
        assert!(parse_bool_flag("yes").unwrap());
        assert!(parse_bool_flag("true").unwrap());
        assert!(parse_bool_flag("1").unwrap());
        assert!(!parse_bool_flag("no").unwrap());
        assert!(!parse_bool_flag("false").unwrap());
        assert!(!parse_bool_flag("0").unwrap());
        assert!(parse_bool_flag("invalid").is_err());
    }

    #[test]
    #[ignore = "TODO: Add support for GHA CI"]
    fn test_sandbox_filesystem_isolation_cli_integration() {
        // Integration test for `ah agent sandbox` command CLI functionality
        // This tests that the sandbox command accepts parameters and attempts execution
        use std::process::Command;

        // Build path to the ah binary (similar to the task integration tests)
        let cargo_manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
            .unwrap_or_else(|_| "/Users/zahary/blocksense/agent-harbor/cli".to_string());
        let binary_path = if cargo_manifest_dir.contains("/crates/") {
            std::path::Path::new(&cargo_manifest_dir).join("../../target/debug/ah")
        } else {
            std::path::Path::new(&cargo_manifest_dir).join("target/debug/ah")
        };

        // Test 1: Basic sandbox command parsing and execution attempt
        let mut cmd = Command::new(&binary_path);
        cmd.args([
            "agent",
            "sandbox",
            "--type",
            "local",
            "--allow-network",
            "no",
            "--",
            "echo",
            "sandbox test",
        ]);

        let output = cmd.output().expect("Failed to run ah agent sandbox command");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        println!("Sandbox command stdout: {}", stdout);
        if !stderr.is_empty() {
            println!("Sandbox command stderr: {}", stderr);
        }

        // The command should attempt to run (may fail due to missing FS providers or permissions)
        // We're testing that the CLI accepts the parameters and attempts execution
        if !output.status.success() {
            // Common expected failures in test environments:
            // - No filesystem snapshot providers available
            // - Insufficient permissions for sandboxing
            // - Missing kernel features
            assert!(
                stderr.contains("Failed to prepare sandbox workspace")
                    || stderr.contains("No filesystem snapshot provider")
                    || stderr.contains("permission denied")
                    || stderr.contains("Operation not permitted")
                    || stderr.contains("Sandbox functionality is only available on Linux"),
                "Unexpected failure: stdout={}, stderr={}",
                stdout,
                stderr
            );
            println!(
                "⚠️  Sandbox command failed as expected in test environment (missing providers/permissions)"
            );
        } else {
            println!("✅ Sandbox command executed successfully");
        }

        // Test 2: Invalid sandbox type rejection
        let mut cmd_invalid = Command::new(&binary_path);
        cmd_invalid.args([
            "agent",
            "sandbox",
            "--type",
            "invalid-type",
            "--",
            "echo",
            "test",
        ]);

        let output_invalid = cmd_invalid.output().expect("Failed to run invalid sandbox command");
        assert!(
            !output_invalid.status.success(),
            "Invalid sandbox type should be rejected"
        );

        let stderr_invalid = String::from_utf8_lossy(&output_invalid.stderr);
        assert!(
            stderr_invalid.contains("Only 'local' sandbox type is currently supported"),
            "Should reject invalid sandbox type: {}",
            stderr_invalid
        );

        println!("✅ CLI integration test for `ah agent sandbox` command completed");
        println!("   This test verifies that:");
        println!("   1. `ah agent sandbox` accepts CLI parameters");
        println!("   2. Invalid sandbox types are properly rejected");
        println!("   3. Command attempts execution (may fail in test environments)");
        println!(
            "   Note: Full sandbox execution requires ZFS/Btrfs providers and proper permissions"
        );
    }
}
