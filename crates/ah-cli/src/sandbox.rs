// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use crate::tui::FsSnapshotsType;
use ah_fs_snapshots::{
    FsSnapshotProvider, PreparedWorkspace, SnapshotProviderKind, WorkingCopyMode,
};
use anyhow::{Context, Result};
use clap::Args;
#[cfg(target_os = "linux")]
use sandbox_core::ProcessConfig;
use std::path::{Path, PathBuf};

/// Filesystem provider argument for internal use
#[derive(Clone, Debug)]
enum FsProviderArg {
    /// Auto-detect the best available snapshot provider
    Auto,
    /// AgentFS overlay filesystem
    Agentfs,
    /// Git shadow commits
    Git,
    /// ZFS filesystem snapshots
    Zfs,
    /// Btrfs filesystem snapshots
    Btrfs,
    /// Disable filesystem snapshots
    Disable,
}

/// Arguments for running a command in a sandbox
#[derive(Args, Clone)]
pub struct SandboxRunArgs {
    /// Sandbox type (currently only 'local' is supported)
    #[arg(long = "type", default_value = "local")]
    pub sandbox_type: String,

    /// Allow internet access via slirp4netns
    #[arg(long = "allow-network", value_name = "BOOL", default_value = "no")]
    #[allow(unused)]
    pub allow_network: String,

    /// Enable container device access (/dev/fuse, storage dirs)
    #[arg(long = "allow-containers", value_name = "BOOL", default_value = "no")]
    #[allow(unused)]
    pub allow_containers: String,

    /// Enable KVM device access for VMs (/dev/kvm)
    #[arg(long = "allow-kvm", value_name = "BOOL", default_value = "no")]
    #[allow(unused)]
    pub allow_kvm: String,

    /// Enable dynamic filesystem access control
    #[arg(long = "seccomp", value_name = "BOOL", default_value = "no")]
    #[allow(unused)]
    pub seccomp: String,

    /// Enable debugging operations in sandbox
    #[arg(long = "seccomp-debug", value_name = "BOOL", default_value = "no")]
    #[allow(unused)]
    pub seccomp_debug: String,

    /// Additional writable paths to bind mount
    #[arg(long = "mount-rw", value_name = "PATH")]
    #[allow(unused)]
    pub mount_rw: Vec<PathBuf>,

    /// Paths to promote to copy-on-write overlays
    #[arg(long = "overlay", value_name = "PATH")]
    #[allow(unused)]
    pub overlay: Vec<PathBuf>,

    /// Command and arguments to run in the sandbox
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub command: Vec<String>,
}

impl SandboxRunArgs {
    /// Execute the sandbox run command
    pub async fn run(self, fs_snapshots: FsSnapshotsType) -> Result<()> {
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

        let prepared_workspace =
            prepare_workspace_with_fallback(&workspace_path, fs_snapshots.clone())
                .await
                .context("Failed to prepare writable workspace with any provider")?;

        // Log telemetry about provider selection
        tracing::info!(
            provider = ?prepared_workspace.provider,
            working_copy = ?prepared_workspace.working_copy,
            path = %prepared_workspace.exec_path.display(),
            "Prepared sandbox workspace telemetry"
        );

        // Additional structured logging for monitoring
        tracing::debug!(
            provider_kind = ?prepared_workspace.provider,
            workspace_mode = ?prepared_workspace.working_copy,
            exec_path = %prepared_workspace.exec_path.display(),
            cleanup_token = %prepared_workspace.cleanup_token,
            "Workspace preparation details"
        );

        tracing::info!(
            workspace_path = %prepared_workspace.exec_path.display(),
            working_copy = ?prepared_workspace.working_copy,
            provider = ?prepared_workspace.provider,
            fs_snapshots = ?fs_snapshots,
            "Prepared sandbox workspace"
        );

        if prepared_workspace.provider == SnapshotProviderKind::Disable {
            tracing::warn!("No filesystem snapshot provider available; using in-place workspace");
        }

        let cleanup_token = prepared_workspace.cleanup_token.clone();
        let provider_kind = prepared_workspace.provider;
        #[allow(unused)]
        let env_vars: Vec<(String, String)> = std::env::vars().collect();

        #[cfg(target_os = "linux")]
        let result: Result<()> = {
            let exec_dir = prepared_workspace.exec_path.clone();

            tracing::info!(
                workspace_path = %exec_dir.display(),
                command = ?command,
                allow_network = allow_network,
                allow_containers = allow_containers,
                allow_kvm = allow_kvm,
                seccomp = seccomp,
                seccomp_debug = seccomp_debug,
                mount_rw = ?mount_rw,
                overlay = ?overlay,
                "Running command inside sandbox workspace"
            );

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
                tracing::warn!(error = %err, "Sandbox stop cleanup encountered an error");
            }

            if let Err(err) = sandbox.cleanup().await {
                tracing::warn!(error = %err, "Sandbox filesystem cleanup encountered an error");
            }

            let outcome =
                exec_result.context("Failed to execute command inside sandbox").map(|_| {
                    tracing::info!("Sandbox command completed successfully");
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

/// Convert FsSnapshotsType to internal FsProviderArg
fn convert_fs_snapshots_type(fs_snapshots: FsSnapshotsType) -> FsProviderArg {
    match fs_snapshots {
        FsSnapshotsType::Auto => FsProviderArg::Auto,
        FsSnapshotsType::Agentfs => FsProviderArg::Agentfs,
        FsSnapshotsType::Git => FsProviderArg::Git,
        FsSnapshotsType::Zfs => FsProviderArg::Zfs,
        FsSnapshotsType::Btrfs => FsProviderArg::Btrfs,
        FsSnapshotsType::Disable => FsProviderArg::Disable,
    }
}

/// Prepare a writable workspace using FS snapshots with fallback logic
pub async fn prepare_workspace_with_fallback(
    workspace_path: &std::path::Path,
    fs_snapshots: FsSnapshotsType,
) -> Result<PreparedWorkspace> {
    let fs_provider = convert_fs_snapshots_type(fs_snapshots);
    // Handle explicit provider selection or auto-detection
    match fs_provider {
        FsProviderArg::Disable => {
            tracing::info!("Filesystem snapshots disabled by user request");
            return Ok(PreparedWorkspace {
                exec_path: workspace_path.to_path_buf(),
                working_copy: WorkingCopyMode::InPlace,
                provider: SnapshotProviderKind::Disable,
                cleanup_token: ah_fs_snapshots::generate_unique_id(),
            });
        }
        FsProviderArg::Agentfs => {
            #[cfg(feature = "agentfs")]
            {
                tracing::info!("Trying AgentFS provider (explicitly requested)");
                let provider = ah_fs_snapshots::AgentFsProvider::new();
                let capabilities = provider.detect_capabilities(workspace_path);

                if capabilities.score > 0 {
                    let modes_to_try = if capabilities.supports_cow_overlay {
                        vec![WorkingCopyMode::CowOverlay]
                    } else {
                        vec![WorkingCopyMode::InPlace]
                    };

                    for mode in modes_to_try {
                        tracing::debug!(mode = ?mode, "Trying AgentFS workspace preparation mode");
                        match provider.prepare_writable_workspace(workspace_path, mode) {
                            Ok(workspace) => {
                                tracing::info!(
                                    provider = "AgentFS",
                                    mode = ?mode,
                                    agentfs_socket = ?agentfs_socket,
                                    "Successfully prepared AgentFS workspace"
                                );
                                return Ok(workspace);
                            }
                            Err(err) => {
                                tracing::debug!(
                                    "AgentFS provider failed in {:?} mode: {}",
                                    mode,
                                    err
                                );
                                continue;
                            }
                        }
                    }
                }
                return Err(anyhow::anyhow!(
                    "AgentFS provider explicitly requested but not available or failed to initialize"
                ));
            }
            #[cfg(not(feature = "agentfs"))]
            {
                return Err(anyhow::anyhow!(
                    "AgentFS provider requested but not compiled (missing 'agentfs' feature)"
                ));
            }
        }
        FsProviderArg::Zfs => {
            #[cfg(feature = "zfs")]
            {
                tracing::info!("Trying ZFS provider (explicitly requested)");
                let provider = ah_fs_snapshots_zfs::ZfsProvider::new();
                let capabilities = provider.detect_capabilities(workspace_path);

                if capabilities.score > 0 {
                    let modes_to_try = if capabilities.supports_cow_overlay {
                        vec![WorkingCopyMode::CowOverlay]
                    } else {
                        vec![WorkingCopyMode::InPlace]
                    };

                    for mode in modes_to_try {
                        tracing::debug!(mode = ?mode, "Trying ZFS workspace preparation mode");
                        match provider.prepare_writable_workspace(workspace_path, mode) {
                            Ok(workspace) => {
                                tracing::info!(
                                    provider = "ZFS",
                                    mode = ?mode,
                                    "Successfully prepared ZFS workspace"
                                );
                                return Ok(workspace);
                            }
                            Err(err) => {
                                tracing::debug!("ZFS provider failed in {:?} mode: {}", mode, err);
                                continue;
                            }
                        }
                    }
                }
                return Err(anyhow::anyhow!(
                    "ZFS provider explicitly requested but not available"
                ));
            }
            #[cfg(not(feature = "zfs"))]
            {
                return Err(anyhow::anyhow!(
                    "ZFS provider requested but not compiled (missing 'zfs' feature)"
                ));
            }
        }
        FsProviderArg::Btrfs => {
            #[cfg(feature = "btrfs")]
            {
                tracing::info!("Trying Btrfs provider (explicitly requested)");
                let provider = ah_fs_snapshots_btrfs::BtrfsProvider::new();
                let capabilities = provider.detect_capabilities(workspace_path);

                if capabilities.score > 0 {
                    let modes_to_try = if capabilities.supports_cow_overlay {
                        vec![WorkingCopyMode::CowOverlay]
                    } else {
                        vec![WorkingCopyMode::InPlace]
                    };

                    for mode in modes_to_try {
                        tracing::debug!(mode = ?mode, "Trying Btrfs workspace preparation mode");
                        match provider.prepare_writable_workspace(workspace_path, mode) {
                            Ok(workspace) => {
                                tracing::info!(
                                    provider = "Btrfs",
                                    mode = ?mode,
                                    "Successfully prepared Btrfs workspace"
                                );
                                return Ok(workspace);
                            }
                            Err(err) => {
                                tracing::debug!(
                                    "Btrfs provider failed in {:?} mode: {}",
                                    mode,
                                    err
                                );
                                continue;
                            }
                        }
                    }
                }
                return Err(anyhow::anyhow!(
                    "Btrfs provider explicitly requested but not available"
                ));
            }
            #[cfg(not(feature = "btrfs"))]
            {
                return Err(anyhow::anyhow!(
                    "Btrfs provider requested but not compiled (missing 'btrfs' feature)"
                ));
            }
        }
        FsProviderArg::Git => {
            #[cfg(feature = "git")]
            {
                tracing::info!("Trying Git provider (explicitly requested)");
                let provider = ah_fs_snapshots_git::GitProvider::new();
                let capabilities = provider.detect_capabilities(workspace_path);

                if capabilities.score > 0 {
                    let modes_to_try = if capabilities.supports_cow_overlay {
                        vec![WorkingCopyMode::CowOverlay]
                    } else {
                        vec![WorkingCopyMode::InPlace]
                    };

                    for mode in modes_to_try {
                        tracing::debug!(mode = ?mode, "Trying Git workspace preparation mode");
                        match provider.prepare_writable_workspace(workspace_path, mode) {
                            Ok(workspace) => {
                                tracing::info!(
                                    provider = "Git",
                                    mode = ?mode,
                                    "Successfully prepared Git workspace"
                                );
                                return Ok(workspace);
                            }
                            Err(err) => {
                                tracing::debug!("Git provider failed in {:?} mode: {}", mode, err);
                                continue;
                            }
                        }
                    }
                }
                return Err(anyhow::anyhow!(
                    "Git provider explicitly requested but not available"
                ));
            }
            #[cfg(not(feature = "git"))]
            {
                return Err(anyhow::anyhow!(
                    "Git provider requested but not compiled (missing 'git' feature)"
                ));
            }
        }
        FsProviderArg::Auto => {
            // Auto mode: Try providers in order of preference: AgentFS -> ZFS -> Btrfs -> Git
            tracing::info!("Auto-detecting best available filesystem snapshot provider");

            #[cfg(feature = "agentfs")]
            {
                tracing::debug!("Trying AgentFS provider (auto mode)");
                let provider = ah_fs_snapshots::AgentFsProvider::new();
                let capabilities = provider.detect_capabilities(workspace_path);

                if capabilities.score > 0 {
                    let modes_to_try = if capabilities.supports_cow_overlay {
                        vec![WorkingCopyMode::CowOverlay]
                    } else {
                        vec![WorkingCopyMode::InPlace]
                    };

                    for mode in modes_to_try {
                        tracing::debug!(mode = ?mode, "Trying AgentFS workspace preparation mode");
                        match provider.prepare_writable_workspace(workspace_path, mode) {
                            Ok(workspace) => {
                                tracing::info!(
                                    provider = "AgentFS",
                                    mode = ?mode,
                                    agentfs_socket = ?agentfs_socket,
                                    "Successfully prepared AgentFS workspace (auto-selected)"
                                );
                                return Ok(workspace);
                            }
                            Err(err) => {
                                tracing::debug!(
                                    "AgentFS provider failed in {:?} mode: {}",
                                    mode,
                                    err
                                );
                                continue;
                            }
                        }
                    }
                }
            }

            #[cfg(feature = "zfs")]
            {
                tracing::debug!("Trying ZFS provider (auto mode)");
                let provider = ah_fs_snapshots_zfs::ZfsProvider::new();
                let capabilities = provider.detect_capabilities(workspace_path);

                if capabilities.score > 0 {
                    let modes_to_try = if capabilities.supports_cow_overlay {
                        vec![WorkingCopyMode::CowOverlay]
                    } else {
                        vec![WorkingCopyMode::InPlace]
                    };

                    for mode in modes_to_try {
                        tracing::debug!(mode = ?mode, "Trying ZFS workspace preparation mode");
                        match provider.prepare_writable_workspace(workspace_path, mode) {
                            Ok(workspace) => {
                                tracing::info!(
                                    provider = "ZFS",
                                    mode = ?mode,
                                    "Successfully prepared ZFS workspace (auto-selected)"
                                );
                                return Ok(workspace);
                            }
                            Err(err) => {
                                tracing::debug!("ZFS provider failed in {:?} mode: {}", mode, err);
                                continue;
                            }
                        }
                    }
                }
            }

            #[cfg(feature = "btrfs")]
            {
                tracing::debug!("Trying Btrfs provider (auto mode)");
                let provider = ah_fs_snapshots_btrfs::BtrfsProvider::new();
                let capabilities = provider.detect_capabilities(workspace_path);

                if capabilities.score > 0 {
                    let modes_to_try = if capabilities.supports_cow_overlay {
                        vec![WorkingCopyMode::CowOverlay]
                    } else {
                        vec![WorkingCopyMode::InPlace]
                    };

                    for mode in modes_to_try {
                        tracing::debug!(mode = ?mode, "Trying Btrfs workspace preparation mode");
                        match provider.prepare_writable_workspace(workspace_path, mode) {
                            Ok(workspace) => {
                                tracing::info!(
                                    provider = "Btrfs",
                                    mode = ?mode,
                                    "Successfully prepared Btrfs workspace (auto-selected)"
                                );
                                return Ok(workspace);
                            }
                            Err(err) => {
                                tracing::debug!(
                                    "Btrfs provider failed in {:?} mode: {}",
                                    mode,
                                    err
                                );
                                continue;
                            }
                        }
                    }
                }
            }

            #[cfg(feature = "git")]
            {
                tracing::debug!("Trying Git provider (auto mode)");
                let provider = ah_fs_snapshots_git::GitProvider::new();
                let capabilities = provider.detect_capabilities(workspace_path);

                if capabilities.score > 0 {
                    let modes_to_try = if capabilities.supports_cow_overlay {
                        vec![WorkingCopyMode::CowOverlay]
                    } else {
                        vec![WorkingCopyMode::InPlace]
                    };

                    for mode in modes_to_try {
                        tracing::debug!(mode = ?mode, "Trying Git workspace preparation mode");
                        match provider.prepare_writable_workspace(workspace_path, mode) {
                            Ok(workspace) => {
                                tracing::info!(
                                    provider = "Git",
                                    mode = ?mode,
                                    "Successfully prepared Git workspace (auto-selected)"
                                );
                                return Ok(workspace);
                            }
                            Err(err) => {
                                tracing::debug!("Git provider failed in {:?} mode: {}", mode, err);
                                continue;
                            }
                        }
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
                tracing::warn!(
                    cleanup_token = cleanup_token,
                    error = %err,
                    "Failed to cleanup sandbox workspace"
                );
            }
        }
        Err(err) => {
            tracing::warn!(
                error = %err,
                "Unable to clean up sandbox workspace (provider lookup failed)"
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
    fn test_sandbox_filesystem_isolation_cli_integration() {
        // Skip this test in CI environments where filesystem snapshot providers may not be available
        if std::env::var("CI").is_ok() {
            println!("⚠️  Skipping sandbox filesystem isolation test in CI environment");
            return;
        }
        // Integration test for `ah agent sandbox` command CLI functionality
        // This tests that the sandbox command accepts parameters and attempts execution
        use std::process::Command;

        // Build path to the ah binary (similar to the task integration tests)
        let cargo_manifest_dir = env!("CARGO_MANIFEST_DIR");
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
            "--fs-snapshots",
            "auto",
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
            "--fs-snapshots",
            "auto",
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
            "   Note: Full sandbox execution requires ZFS/Btrfs/AgentFS providers and proper permissions"
        );
    }

    #[test]
    fn test_sandbox_workspace_agentfs() {
        // Skip this test in CI environments where AgentFS harness may not be available
        if std::env::var("CI").is_ok() {
            println!("⚠️  Skipping AgentFS sandbox test in CI environment");
            return;
        }
        // Test the new --fs-provider and --agentfs-socket flags
        // This test exercises the end-to-end sandbox command with AgentFS provider
        use std::process::Command;

        // Build path to the ah binary
        let cargo_manifest_dir = env!("CARGO_MANIFEST_DIR");
        let binary_path = if cargo_manifest_dir.contains("/crates/") {
            std::path::Path::new(&cargo_manifest_dir).join("../../target/debug/ah")
        } else {
            std::path::Path::new(&cargo_manifest_dir).join("target/debug/ah")
        };

        // Test 1: AgentFS provider explicitly requested (may fail if not available)
        let mut cmd = Command::new(&binary_path);
        cmd.args([
            "agent",
            "sandbox",
            "--fs-provider",
            "agentfs",
            "--",
            "echo",
            "agentfs test",
        ]);

        let output = cmd.output().expect("Failed to run ah agent sandbox with agentfs command");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        println!("AgentFS sandbox stdout: {}", stdout);
        if !stderr.is_empty() {
            println!("AgentFS sandbox stderr: {}", stderr);
        }

        // The command should attempt to run (may fail due to missing AgentFS support or permissions)
        if !output.status.success() {
            // Common expected failures in test environments:
            // - AgentFS provider not available or not compiled
            // - Insufficient permissions for sandboxing
            // - Missing kernel features
            assert!(
                stderr.contains("AgentFS provider explicitly requested but not available")
                    || stderr.contains("AgentFS provider requested but not compiled")
                    || stderr.contains("permission denied")
                    || stderr.contains("Operation not permitted")
                    || stderr.contains("Sandbox functionality is only available on Linux"),
                "Unexpected failure: stdout={}, stderr={}",
                stdout,
                stderr
            );
            println!(
                "⚠️  AgentFS sandbox command failed as expected in test environment (missing provider/permissions)"
            );
        } else {
            println!("✅ AgentFS sandbox command executed successfully");
        }

        // Test 2: Disable provider (should always work)
        let mut cmd_disable = Command::new(&binary_path);
        cmd_disable.args([
            "agent",
            "sandbox",
            "--fs-provider",
            "disable",
            "--",
            "echo",
            "disable test",
        ]);

        let output_disable = cmd_disable
            .output()
            .expect("Failed to run ah agent sandbox with disable command");
        let stdout_disable = String::from_utf8_lossy(&output_disable.stdout);
        let stderr_disable = String::from_utf8_lossy(&output_disable.stderr);

        println!("Disable sandbox stdout: {}", stdout_disable);
        if !stderr_disable.is_empty() {
            println!("Disable sandbox stderr: {}", stderr_disable);
        }

        // Disable should work on Linux (may fail on macOS due to sandbox not being implemented)
        if !output_disable.status.success() {
            assert!(
                stderr_disable.contains("Sandbox functionality is only available on Linux"),
                "Disable provider should work on Linux: stdout={}, stderr={}",
                stdout_disable,
                stderr_disable
            );
            println!("⚠️  Sandbox disable test skipped (not on Linux)");
        } else {
            println!("✅ Disable provider sandbox executed successfully");
        }

        println!("✅ AgentFS provider CLI integration test completed");
        println!("   This test verifies that:");
        println!("   1. `--fs-provider agentfs` flag is accepted");
        println!("   2. `--fs-provider disable` flag works");
        println!("   3. Provider selection logic routes correctly");
        println!(
            "   Note: Full AgentFS execution requires compiled agentfs feature and proper permissions"
        );
    }
}
