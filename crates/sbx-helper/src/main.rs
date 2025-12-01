// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Sandbox helper binary that becomes PID 1 in the sandbox environment.

use clap::Parser;

#[cfg(target_os = "linux")]
use sandbox_core::{NamespaceConfig, ProcessConfig, Sandbox};
#[cfg(target_os = "linux")]
use sandbox_fs::{FilesystemConfig, FilesystemManager};

#[cfg(target_os = "linux")]
use ah_logging::{Level, LogFormat, init};
#[cfg(target_os = "linux")]
use tracing::info;

use tracing::error;

/// Command line arguments for sbx-helper
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Command to execute in sandbox
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    command: Vec<String>,

    /// Working directory for the command
    #[arg(short = 'C', long)]
    working_dir: Option<String>,

    /// Enable debug mode
    #[arg(long)]
    debug: bool,

    /// Disable user namespace isolation
    ///
    /// Note: Disabling user namespaces may require CAP_SYS_ADMIN for other namespaces.
    /// User namespaces allow unprivileged creation of other namespace types.
    #[arg(long)]
    no_user_ns: bool,

    /// Disable mount namespace isolation
    ///
    /// Note: Mount operations require CAP_SYS_ADMIN (typically root privileges).
    #[arg(long)]
    no_mount_ns: bool,

    /// Disable PID namespace isolation
    ///
    /// Note: Creating PID namespaces requires CAP_SYS_ADMIN unless done within
    /// a user namespace created in the same unshare() call.
    #[arg(long)]
    no_pid_ns: bool,

    /// Read-write directory to allow in sandbox
    #[arg(long)]
    rw_dir: Option<String>,

    /// Enable static mode (blacklist + overlays) instead of dynamic mode
    #[arg(long)]
    static_mode: bool,

    /// Path to make writable via overlay filesystem (can be specified multiple times)
    #[arg(long)]
    overlay: Vec<String>,

    /// Path to blacklist in static mode (can be specified multiple times)
    #[arg(long)]
    blacklist: Vec<String>,

    /// Enable seccomp dynamic filesystem access control
    #[arg(long)]
    seccomp: bool,

    /// Enable debug mode for seccomp (allows ptrace operations)
    #[arg(long)]
    seccomp_debug: bool,

    /// Allow network access via slirp4netns
    #[arg(long)]
    allow_network: bool,

    /// Allow container workloads (enables /dev/fuse, cgroup delegation)
    #[arg(long)]
    allow_containers: bool,

    /// Allow KVM access for virtual machines
    #[arg(long)]
    allow_kvm: bool,
}

#[tokio::main]
#[cfg(target_os = "linux")]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Initialize logging
    let default_level = if args.debug {
        Level::DEBUG
    } else {
        Level::INFO
    };
    init("sbx-helper", default_level, LogFormat::Plaintext)?;

    info!("Starting sandbox helper with args: {:?}", args);

    // Create sandbox configuration
    // Network namespace is enabled by default for isolation; slirp4netns provides
    // internet access when --allow-network is specified.
    let namespace_config = NamespaceConfig {
        user_ns: !args.no_user_ns,
        mount_ns: !args.no_mount_ns,
        pid_ns: !args.no_pid_ns,
        uts_ns: true,
        ipc_ns: true,
        net_ns: true, // Network isolation enabled by default
        time_ns: false,
        uid_map: None,
        gid_map: None,
    };

    // Create process configuration
    let command = if args.command.is_empty() {
        vec!["/bin/sh".to_string()]
    } else {
        args.command.clone()
    };

    // Prepare environment variables for the sandboxed process
    let mut env_vars = std::env::vars().collect::<Vec<(String, String)>>();
    // Set environment variable to indicate we're running in a sandboxed test environment
    // This allows test programs to safely perform resource-intensive operations
    env_vars.push(("SANDBOX_TEST_MODE".to_string(), "1".to_string()));

    let process_config = ProcessConfig {
        command,
        working_dir: args.working_dir.clone(),
        env: env_vars,
        tmpfs_size: None,    // Use default tmpfs size for /tmp isolation
        net_isolation: true, // Network isolation enabled by default
        allow_internet: args.allow_network, // Internet access via slirp4netns if --allow-network
        agentfs_overlay: None, // No AgentFS overlay for sbx-helper (not using fs-snapshots)
    };

    // Create filesystem configuration
    let mut fs_config = FilesystemConfig::default();
    if let Some(rw_dir) = &args.rw_dir {
        fs_config.working_dir = Some(rw_dir.clone());
    }
    fs_config.static_mode = args.static_mode;
    fs_config.overlay_paths = args.overlay.clone();
    fs_config.blacklist_paths = args.blacklist.clone();

    // Initialize sandbox with cgroups, seccomp, and networking enabled
    let mut sandbox = Sandbox::with_namespace_config(namespace_config)
        .with_process_config(process_config)
        .with_default_cgroups();

    // Enable networking if requested
    if args.allow_network {
        sandbox = sandbox.with_default_network();
        info!("Network access enabled via slirp4netns");
    }

    // Enable seccomp if requested
    if args.seccomp {
        use sandbox_seccomp::SeccompConfig;
        use tokio::sync::mpsc;

        // Create a channel for supervisor communication
        // In this simple implementation, we deny all requests since there's no supervisor
        // TODO: Implement proper supervisor integration
        let (supervisor_tx, _supervisor_rx) = mpsc::unbounded_channel();

        let seccomp_config = SeccompConfig {
            debug_mode: args.seccomp_debug,
            supervisor_tx: Some(supervisor_tx),
            root_dir: std::path::PathBuf::from("/"),
        };

        sandbox = sandbox.with_seccomp(seccomp_config);
        info!("Seccomp dynamic filesystem access control enabled");
    }

    // Enable device management if requested
    if args.allow_containers || args.allow_kvm {
        if args.allow_containers && args.allow_kvm {
            sandbox = sandbox.with_container_and_vm_devices();
            info!("Device access enabled for both containers and VMs");
        } else if args.allow_containers {
            sandbox = sandbox.with_container_devices();
            info!("Device access enabled for containers (/dev/fuse, storage dirs)");
        } else if args.allow_kvm {
            sandbox = sandbox.with_vm_devices();
            info!("Device access enabled for VMs (/dev/kvm)");
        }
    }

    let fs_manager = FilesystemManager::with_config(fs_config);

    // Start sandbox (enter all namespaces in single unshare() call)
    // After this point, we are within the user namespace and have root privileges there
    sandbox.start().await?;

    // Add current process to cgroup if cgroups are enabled
    sandbox.add_process_to_cgroup(std::process::id())?;

    // Set up filesystem isolation (executed within user namespace context)
    fs_manager.setup_mounts().await?;

    // Set network target PID if networking is enabled
    if args.allow_network {
        let current_pid = std::process::id();
        sandbox.set_network_target_pid(current_pid)?;
        info!("Set network target PID to {}", current_pid);
    }

    // Execute the process as PID 1 in child process, wait for completion
    // Parent process will return after child completes
    match sandbox.exec_process().await {
        Ok(_) => {
            info!("Sandbox execution completed successfully");
            std::process::exit(0);
        }
        Err(e) => {
            error!("Failed to execute process: {}", e);
            std::process::exit(1);
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {
    error!("sbx-helper is only available on Linux systems");
    std::process::exit(1);
}
