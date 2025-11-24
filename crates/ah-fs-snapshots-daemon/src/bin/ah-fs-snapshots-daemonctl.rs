// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use std::env;
use std::io::{self, Write};
use std::path::PathBuf;

use ah_fs_snapshots_daemon::client::{
    AgentfsFuseBackstore, AgentfsFuseMountRequest, AgentfsFuseState, AgentfsFuseStatusData,
    AgentfsHostFsBackstore, AgentfsInterposeMountHints, AgentfsInterposeMountRequest,
    AgentfsInterposeStatusData, AgentfsRamDiskBackstore, DEFAULT_SOCKET_PATH, DaemonClient,
};
use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use serde::Serialize;

#[derive(Parser, Debug)]
#[command(
    name = "ah-fs-snapshots-daemonctl",
    about = "Control utility for ah-fs-snapshots-daemon"
)]
struct Cli {
    /// Path to the daemon Unix socket
    #[arg(long, default_value = DEFAULT_SOCKET_PATH)]
    socket_path: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Manage the AgentFS FUSE mount lifecycle
    #[command(subcommand)]
    Fuse(FuseCommand),
    /// Manage the AgentFS macOS interpose daemon lifecycle
    #[command(subcommand)]
    Interpose(InterposeCommand),
}

#[derive(Subcommand, Debug)]
enum FuseCommand {
    /// Mount the AgentFS FUSE filesystem via the daemon
    Mount(MountArgs),
    /// Unmount the AgentFS FUSE filesystem
    Unmount,
    /// Show the current mount status
    Status(StatusArgs),
}

#[derive(Subcommand, Debug)]
enum InterposeCommand {
    /// Mount the AgentFS interpose daemon via the supervisor
    Mount(InterposeMountArgs),
    /// Stop the AgentFS interpose daemon
    Unmount,
    /// Show the current interpose daemon status
    Status(InterposeStatusArgs),
}

#[derive(Parser, Debug)]
struct MountArgs {
    /// Target mount point
    #[arg(long, default_value = "/tmp/agentfs")]
    mount_point: PathBuf,

    /// UID that should own the mount point
    #[arg(long)]
    uid: Option<u32>,

    /// GID that should own the mount point
    #[arg(long)]
    gid: Option<u32>,

    /// Allow other users to access the mount
    #[arg(long)]
    allow_other: bool,

    /// Allow root to access the mount
    #[arg(long)]
    allow_root: bool,

    /// Ask the FUSE host to auto-unmount when it exits
    #[arg(long)]
    auto_unmount: bool,

    /// Enable kernel writeback cache
    #[arg(long)]
    writeback_cache: bool,

    /// Timeout (milliseconds) to wait for .agentfs/control
    #[arg(long, default_value_t = 15_000)]
    mount_timeout_ms: u32,

    /// Which backstore implementation to use
    #[arg(long, value_enum, default_value_t = BackstoreKind::InMemory)]
    backstore: BackstoreKind,

    /// Root directory for HostFs backstore
    #[arg(long)]
    hostfs_root: Option<PathBuf>,

    /// Prefer native snapshots when using HostFs
    #[arg(long)]
    hostfs_prefer_native_snapshots: bool,

    /// Size of the RAM disk backstore (MiB)
    #[arg(long, default_value_t = 1024)]
    ramdisk_size_mb: u32,

    /// Print JSON status output
    #[arg(long)]
    json: bool,
}

#[derive(Parser, Debug)]
struct StatusArgs {
    /// Print JSON status output
    #[arg(long)]
    json: bool,

    /// Do not exit with an error when the mount is not running
    #[arg(long)]
    allow_not_ready: bool,
}

#[derive(Parser, Debug)]
struct InterposeMountArgs {
    /// Repository root exposed through AgentFS
    #[arg(long, default_value = ".")]
    repo_root: PathBuf,

    /// Optional fully-qualified socket path for daemon-managed sessions
    #[arg(long)]
    socket_path: Option<PathBuf>,

    /// Runtime directory for pid/log/status metadata
    #[arg(long)]
    runtime_dir: Option<PathBuf>,

    /// Timeout (milliseconds) to wait for the daemon socket
    #[arg(long, default_value_t = 5_000)]
    mount_timeout_ms: u32,

    /// Print JSON status output
    #[arg(long)]
    json: bool,
}

#[derive(Parser, Debug)]
struct InterposeStatusArgs {
    /// Print JSON status output
    #[arg(long)]
    json: bool,

    /// Do not exit with an error when the daemon is not running
    #[arg(long)]
    allow_not_ready: bool,
}

#[derive(Copy, Clone, Debug, ValueEnum, Eq, PartialEq)]
enum BackstoreKind {
    #[value(name = "in-memory")]
    InMemory,
    #[value(name = "hostfs")]
    Hostfs,
    #[value(name = "ramdisk")]
    Ramdisk,
}

#[derive(Serialize)]
struct StatusPrint<'a> {
    state: &'a str,
    healthy: bool,
    mount_point: String,
    pid: u64,
    restart_count: u32,
    log_path: String,
    runtime_dir: String,
    backstore: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_error: Option<String>,
}

#[derive(Serialize)]
struct InterposeStatusPrint {
    state: &'static str,
    healthy: bool,
    socket_path: String,
    pid: u64,
    restart_count: u32,
    log_path: String,
    runtime_dir: String,
    repo_root: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_error: Option<String>,
}

fn main() {
    if let Err(err) = run() {
        let _ = writeln!(io::stderr(), "{err}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let client = DaemonClient::with_socket_path(&cli.socket_path);

    match cli.command {
        Command::Fuse(cmd) => match cmd {
            FuseCommand::Mount(args) => do_mount(&client, args)?,
            FuseCommand::Unmount => do_unmount(&client)?,
            FuseCommand::Status(args) => do_status(&client, args.json, args.allow_not_ready)?,
        },
        Command::Interpose(cmd) => match cmd {
            InterposeCommand::Mount(args) => do_interpose_mount(&client, args)?,
            InterposeCommand::Unmount => do_interpose_unmount(&client)?,
            InterposeCommand::Status(args) => {
                do_interpose_status(&client, args.json, args.allow_not_ready)?
            }
        },
    }

    Ok(())
}

fn do_mount(client: &DaemonClient, args: MountArgs) -> Result<()> {
    let request = to_mount_request(&args)?;
    let status = client
        .mount_agentfs_fuse(request)
        .map_err(|e| anyhow::anyhow!("daemon mount failed: {e}"))?;
    print_fuse_status(&status, args.json)?;
    Ok(())
}

fn do_unmount(client: &DaemonClient) -> Result<()> {
    client
        .unmount_agentfs_fuse()
        .map_err(|e| anyhow::anyhow!("daemon unmount failed: {e}"))?;
    writeln!(io::stdout(), "AgentFS FUSE mount unmounted")?;
    Ok(())
}

fn do_status(client: &DaemonClient, json: bool, allow_not_ready: bool) -> Result<()> {
    let status = client
        .status_agentfs_fuse()
        .map_err(|e| anyhow::anyhow!("daemon status failed: {e}"))?;
    let state = print_fuse_status(&status, json)?;
    if state != AgentfsFuseState::Running && !allow_not_ready {
        anyhow::bail!(
            "AgentFS FUSE mount is not running (state: {})",
            state_name(state)
        );
    }
    Ok(())
}

fn do_interpose_mount(client: &DaemonClient, args: InterposeMountArgs) -> Result<()> {
    let request = to_interpose_request(&args)?;
    let hints = to_interpose_hints(&args);
    let status = match hints {
        Some(hints) => client
            .mount_agentfs_interpose_with_hints(request, hints)
            .map_err(|e| anyhow::anyhow!("daemon interpose mount failed: {e}"))?,
        None => client
            .mount_agentfs_interpose(request)
            .map_err(|e| anyhow::anyhow!("daemon interpose mount failed: {e}"))?,
    };
    print_interpose_status(&status, args.json)?;
    Ok(())
}

fn do_interpose_unmount(client: &DaemonClient) -> Result<()> {
    client
        .unmount_agentfs_interpose()
        .map_err(|e| anyhow::anyhow!("daemon interpose unmount failed: {e}"))?;
    writeln!(io::stdout(), "AgentFS interpose daemon stopped")?;
    Ok(())
}

fn do_interpose_status(client: &DaemonClient, json: bool, allow_not_ready: bool) -> Result<()> {
    let status = client
        .status_agentfs_interpose()
        .map_err(|e| anyhow::anyhow!("daemon interpose status failed: {e}"))?;
    let state = print_interpose_status(&status, json)?;
    if state != AgentfsFuseState::Running && !allow_not_ready {
        anyhow::bail!(
            "AgentFS interpose daemon is not running (state: {})",
            state_name(state)
        );
    }
    Ok(())
}

fn to_mount_request(args: &MountArgs) -> Result<AgentfsFuseMountRequest> {
    let mount_point = args.mount_point.to_string_lossy().into_owned().into_bytes();
    let uid = args.uid.unwrap_or_else(default_uid);
    let gid = args.gid.unwrap_or_else(default_gid);
    let backstore = build_backstore(args)?;

    Ok(AgentfsFuseMountRequest {
        mount_point,
        uid,
        gid,
        allow_other: args.allow_other,
        allow_root: args.allow_root,
        auto_unmount: args.auto_unmount,
        writeback_cache: args.writeback_cache,
        mount_timeout_ms: args.mount_timeout_ms,
        backstore,
    })
}

fn to_interpose_request(args: &InterposeMountArgs) -> Result<AgentfsInterposeMountRequest> {
    Ok(AgentfsInterposeMountRequest {
        repo_root: args.repo_root.to_string_lossy().into_owned().into_bytes(),
        uid: current_uid(),
        gid: current_gid(),
        mount_timeout_ms: args.mount_timeout_ms,
    })
}

fn to_interpose_hints(args: &InterposeMountArgs) -> Option<AgentfsInterposeMountHints> {
    let socket_path = args
        .socket_path
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned().into_bytes())
        .unwrap_or_default();
    let runtime_dir = args
        .runtime_dir
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned().into_bytes())
        .unwrap_or_default();

    if socket_path.is_empty() && runtime_dir.is_empty() {
        None
    } else {
        Some(AgentfsInterposeMountHints {
            socket_path,
            runtime_dir,
        })
    }
}

fn build_backstore(args: &MountArgs) -> Result<AgentfsFuseBackstore> {
    let backstore = match args.backstore {
        BackstoreKind::InMemory => AgentfsFuseBackstore::InMemory(Vec::new()),
        BackstoreKind::Hostfs => {
            let root = args
                .hostfs_root
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("--hostfs-root is required for hostfs backstore"))?
                .to_string_lossy()
                .into_owned()
                .into_bytes();
            AgentfsFuseBackstore::HostFs(AgentfsHostFsBackstore {
                root,
                prefer_native_snapshots: args.hostfs_prefer_native_snapshots,
            })
        }
        BackstoreKind::Ramdisk => AgentfsFuseBackstore::RamDisk(AgentfsRamDiskBackstore {
            size_mb: args.ramdisk_size_mb,
        }),
    };

    Ok(backstore)
}

fn default_uid() -> u32 {
    sudo_override("SUDO_UID").unwrap_or_else(|| unsafe { libc::geteuid() })
}

fn default_gid() -> u32 {
    sudo_override("SUDO_GID").unwrap_or_else(|| unsafe { libc::getegid() })
}

fn current_uid() -> u32 {
    unsafe { libc::geteuid() }
}

fn current_gid() -> u32 {
    unsafe { libc::getegid() }
}

fn sudo_override(var: &str) -> Option<u32> {
    env::var(var).ok().and_then(|value| value.parse::<u32>().ok())
}

fn print_fuse_status(
    status: &AgentfsFuseStatusData,
    json: bool,
) -> anyhow::Result<AgentfsFuseState> {
    let state = AgentfsFuseState::from_code(status.state);
    let output = StatusPrint {
        state: state_name(state),
        healthy: state == AgentfsFuseState::Running,
        mount_point: String::from_utf8_lossy(&status.mount_point).into_owned(),
        pid: status.pid,
        restart_count: status.restart_count,
        log_path: String::from_utf8_lossy(&status.log_path).into_owned(),
        runtime_dir: String::from_utf8_lossy(&status.runtime_dir).into_owned(),
        backstore: describe_backstore(&status.backstore),
        last_error: if status.last_error.is_empty() {
            None
        } else {
            Some(String::from_utf8_lossy(&status.last_error).into_owned())
        },
    };

    if json {
        writeln!(
            io::stdout(),
            "{}",
            serde_json::to_string_pretty(&output).unwrap()
        )?;
    } else {
        let mut stdout = io::stdout();
        writeln!(stdout, "state: {}", output.state)?;
        writeln!(stdout, "healthy: {}", output.healthy)?;
        writeln!(stdout, "mount_point: {}", output.mount_point)?;
        writeln!(stdout, "pid: {}", output.pid)?;
        writeln!(stdout, "restart_count: {}", output.restart_count)?;
        writeln!(stdout, "log_path: {}", output.log_path)?;
        writeln!(stdout, "runtime_dir: {}", output.runtime_dir)?;
        writeln!(stdout, "backstore: {}", output.backstore)?;
        if let Some(err) = output.last_error.as_deref() {
            writeln!(stdout, "last_error: {}", err)?;
        }
    }

    Ok(state)
}

fn print_interpose_status(
    status: &AgentfsInterposeStatusData,
    json: bool,
) -> anyhow::Result<AgentfsFuseState> {
    let state = AgentfsFuseState::from_code(status.state);
    let output = InterposeStatusPrint {
        state: state_name(state),
        healthy: state == AgentfsFuseState::Running,
        socket_path: String::from_utf8_lossy(&status.socket_path).into_owned(),
        pid: status.pid,
        restart_count: status.restart_count,
        log_path: String::from_utf8_lossy(&status.log_path).into_owned(),
        runtime_dir: String::from_utf8_lossy(&status.runtime_dir).into_owned(),
        repo_root: String::from_utf8_lossy(&status.repo_root).into_owned(),
        last_error: if status.last_error.is_empty() {
            None
        } else {
            Some(String::from_utf8_lossy(&status.last_error).into_owned())
        },
    };

    if json {
        writeln!(io::stdout(), "{}", serde_json::to_string_pretty(&output)?)?;
    } else {
        writeln!(io::stdout(), "AgentFS interpose daemon status:")?;
        writeln!(io::stdout(), "  state       : {}", output.state)?;
        writeln!(io::stdout(), "  socket_path : {}", output.socket_path)?;
        writeln!(io::stdout(), "  pid         : {}", output.pid)?;
        writeln!(io::stdout(), "  restart_cnt : {}", output.restart_count)?;
        writeln!(io::stdout(), "  log_path    : {}", output.log_path)?;
        writeln!(io::stdout(), "  runtime_dir : {}", output.runtime_dir)?;
        writeln!(io::stdout(), "  repo_root   : {}", output.repo_root)?;
        if let Some(err) = &output.last_error {
            writeln!(io::stdout(), "  last_error  : {}", err)?;
        }
    }

    Ok(state)
}

fn state_name(state: AgentfsFuseState) -> &'static str {
    match state {
        AgentfsFuseState::Unknown => "unknown",
        AgentfsFuseState::Starting => "starting",
        AgentfsFuseState::Running => "running",
        AgentfsFuseState::BackingOff => "backing_off",
        AgentfsFuseState::Unmounted => "unmounted",
        AgentfsFuseState::Failed => "failed",
    }
}

fn describe_backstore(backstore: &AgentfsFuseBackstore) -> String {
    match backstore {
        AgentfsFuseBackstore::InMemory(_) => "InMemory".to_string(),
        AgentfsFuseBackstore::HostFs(opts) => format!(
            "HostFs(root={}, prefer_native_snapshots={})",
            String::from_utf8_lossy(&opts.root),
            opts.prefer_native_snapshots
        ),
        AgentfsFuseBackstore::RamDisk(opts) => format!("RamDisk({}MiB)", opts.size_mb),
    }
}
