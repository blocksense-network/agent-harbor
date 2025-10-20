use ah_fs_snapshots::{FsSnapshotProvider, PreparedWorkspace, WorkingCopyMode};
use anyhow::{Context, Result};
use clap::Args;
use std::path::PathBuf;

#[cfg(target_os = "linux")]
use sandbox_core::Sandbox;

#[cfg(target_os = "linux")]
use sandbox_seccomp;

/// Arguments for running a command in a sandbox
#[derive(Args)]
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
        // Validate arguments
        if self.sandbox_type != "local" {
            return Err(anyhow::anyhow!(
                "Only 'local' sandbox type is currently supported"
            ));
        }

        // Parse boolean flags
        let allow_network = parse_bool_flag(&self.allow_network)?;
        let allow_containers = parse_bool_flag(&self.allow_containers)?;
        let allow_kvm = parse_bool_flag(&self.allow_kvm)?;
        let seccomp = parse_bool_flag(&self.seccomp)?;
        let seccomp_debug = parse_bool_flag(&self.seccomp_debug)?;

        if self.command.is_empty() {
            return Err(anyhow::anyhow!("No command specified to run in sandbox"));
        }

        // Get current working directory as the workspace to snapshot
        let workspace_path =
            std::env::current_dir().context("Failed to get current working directory")?;

        // Prepare writable workspace using FS snapshots
        // Try providers in order of preference: ZFS -> Btrfs -> Git
        let prepared_workspace = prepare_workspace_with_fallback(&workspace_path)
            .await
            .context("Failed to prepare writable workspace with any provider")?;

        println!(
            "Prepared workspace at: {}",
            prepared_workspace.exec_path.display()
        );
        println!("Working copy mode: {:?}", prepared_workspace.working_copy);
        println!("Provider: {:?}", prepared_workspace.provider);

        // TODO: Configure and launch sandbox with prepared workspace
        println!("Sandbox run command would execute: {:?}", self.command);
        println!("Configuration:");
        println!("  Type: {}", self.sandbox_type);
        println!("  Allow network: {}", allow_network);
        println!("  Allow containers: {}", allow_containers);
        println!("  Allow KVM: {}", allow_kvm);
        println!("  Seccomp: {}", seccomp);
        println!("  Seccomp debug: {}", seccomp_debug);
        println!("  Mount RW paths: {:?}", self.mount_rw);
        println!("  Overlay paths: {:?}", self.overlay);

        // Cleanup the prepared workspace (in real implementation, this would be done after sandbox exits)
        // Note: We need to keep track of the provider that created the workspace for cleanup
        println!("Note: Workspace cleanup would happen here in production implementation");

        Ok(())
    }
}

/// Prepare a writable workspace using FS snapshots with fallback logic
pub async fn prepare_workspace_with_fallback(
    workspace_path: &std::path::Path,
) -> Result<PreparedWorkspace> {
    // Try providers in order of preference: ZFS -> Btrfs -> Git
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
                    Err(e) => {
                        continue;
                    }
                }
            }
        }
    }

    Err(anyhow::anyhow!(
        "No filesystem snapshot provider could prepare a workspace"
    ))
}

/// Create a sandbox instance configured from CLI parameters
#[cfg(target_os = "linux")]
pub fn create_sandbox_from_args(
    allow_network: &str,
    allow_containers: &str,
    allow_kvm: &str,
    seccomp: &str,
    seccomp_debug: &str,
    _mount_rw: &[PathBuf],
    _overlay: &[PathBuf],
) -> Result<sandbox_core::Sandbox> {
    let allow_network = parse_bool_flag(allow_network)?;
    let allow_containers = parse_bool_flag(allow_containers)?;
    let allow_kvm = parse_bool_flag(allow_kvm)?;
    let seccomp = parse_bool_flag(seccomp)?;
    let seccomp_debug = parse_bool_flag(seccomp_debug)?;

    // Start with default sandbox configuration
    let mut sandbox = sandbox_core::Sandbox::new();

    // Enable cgroups by default for resource control
    sandbox = sandbox.with_default_cgroups();

    // Configure networking
    if allow_network {
        sandbox = sandbox.with_default_network();
        // TODO: Set target PID for slirp4netns when we have the process
    }

    // Configure seccomp
    #[cfg(feature = "seccomp")]
    if seccomp {
        let seccomp_config = sandbox_seccomp::SeccompConfig {
            debug_mode: seccomp_debug,
            supervisor_tx: None, // TODO: Set up supervisor communication
            root_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")),
        };
        sandbox = sandbox.with_seccomp(seccomp_config);
    }

    // Configure devices
    if allow_containers || allow_kvm {
        if allow_containers && allow_kvm {
            sandbox = sandbox.with_container_and_vm_devices();
        } else if allow_containers {
            sandbox = sandbox.with_container_devices();
        } else if allow_kvm {
            sandbox = sandbox.with_vm_devices();
        }
    }

    // TODO: Handle mount_rw and overlay paths
    // This would require extending sandbox-fs to accept additional bind mounts and overlays

    Ok(sandbox)
}

/// Create a sandbox instance configured from CLI parameters (non-Linux stub)
#[cfg(not(target_os = "linux"))]
pub fn create_sandbox_from_args(
    _allow_network: &str,
    _allow_containers: &str,
    _allow_kvm: &str,
    _seccomp: &str,
    _seccomp_debug: &str,
    _mount_rw: &[PathBuf],
    _overlay: &[PathBuf],
) -> Result<()> {
    Err(anyhow::anyhow!(
        "Sandbox functionality is only available on Linux"
    ))
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
    #[ignore = "requires manual setup and can hang indefinitely"]
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
                    || stderr.contains("Operation not permitted"),
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
