// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use agentfs_proto::{Request, Response};
use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand};
use ssz::{Decode, Encode};
use std::fs::{self, OpenOptions};
use std::io::{self, Read};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

const AGENTFS_IOCTL_CMD: libc::c_ulong = 0xD000_4146;

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Send AgentFS control-plane requests via the FUSE mount"
)]
struct Cli {
    /// Path to the mounted AgentFS filesystem (e.g. /tmp/agentfs)
    #[arg(long)]
    mount: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Create a snapshot and print the resulting ID
    SnapshotCreate {
        /// Optional human-readable name for the snapshot
        #[arg(long)]
        name: Option<String>,
    },
    /// List all snapshots (one per line)
    SnapshotList,
    /// Create a branch from an existing snapshot and print the branch ID
    BranchCreate {
        /// Snapshot ID to fork from
        #[arg(long)]
        snapshot: String,
        /// Optional branch name
        #[arg(long)]
        name: Option<String>,
    },
    /// Bind a process ID to a branch
    BranchBind {
        /// Branch ID to bind to
        #[arg(long)]
        branch: String,
        /// Target process ID
        #[arg(long)]
        pid: u32,
    },
    /// Apply a new fault-injection policy from a JSON file (use '-' for stdin)
    FaultPolicySet {
        #[arg(long)]
        file: PathBuf,
    },
    /// Clear the currently installed fault policy
    FaultPolicyClear,
}

#[allow(clippy::disallowed_methods)]
fn main() -> Result<()> {
    let cli = Cli::parse();
    let mount = cli.mount.canonicalize().context("invalid mount path")?;
    match cli.command {
        Command::SnapshotCreate { name } => {
            let response = send_request(&mount, Request::snapshot_create(name))?;
            match response {
                Response::SnapshotCreate(info) => {
                    let id = String::from_utf8(info.snapshot.id)
                        .map_err(|e| anyhow!("Invalid UTF-8 snapshot id: {e}"))?;
                    let name = info
                        .snapshot
                        .name
                        .and_then(|n| String::from_utf8(n).ok())
                        .unwrap_or_else(|| "".to_string());
                    if name.is_empty() {
                        println!("SNAPSHOT_ID={id}");
                    } else {
                        println!("SNAPSHOT_ID={id}\tNAME={name}");
                    }
                }
                Response::Error(err) => {
                    return Err(anyhow!(
                        "snapshot_create failed: {} (errno={})",
                        String::from_utf8_lossy(&err.error),
                        err.code.unwrap_or_default()
                    ));
                }
                other => {
                    return Err(anyhow!("unexpected response: {:?}", other));
                }
            }
        }
        Command::SnapshotList => {
            let response = send_request(&mount, Request::snapshot_list())?;
            match response {
                Response::SnapshotList(list) => {
                    for entry in list.snapshots {
                        let id =
                            String::from_utf8(entry.id).unwrap_or_else(|_| "<invalid>".to_string());
                        let name = entry
                            .name
                            .map(|n| String::from_utf8(n).unwrap_or_else(|_| "<invalid>".into()))
                            .unwrap_or_else(|| "-".into());
                        println!("SNAPSHOT\t{id}\t{name}");
                    }
                }
                Response::Error(err) => {
                    return Err(anyhow!(
                        "snapshot_list failed: {} (errno={})",
                        String::from_utf8_lossy(&err.error),
                        err.code.unwrap_or_default()
                    ));
                }
                other => return Err(anyhow!("unexpected response: {:?}", other)),
            }
        }
        Command::BranchCreate { snapshot, name } => {
            let response = send_request(&mount, Request::branch_create(snapshot, name))?;
            match response {
                Response::BranchCreate(info) => {
                    let id = String::from_utf8(info.branch.id)
                        .map_err(|e| anyhow!("Invalid UTF-8 branch id: {e}"))?;
                    let name = info
                        .branch
                        .name
                        .and_then(|n| String::from_utf8(n).ok())
                        .unwrap_or_else(|| "".to_string());
                    if name.is_empty() {
                        println!("BRANCH_ID={id}");
                    } else {
                        println!("BRANCH_ID={id}\tNAME={name}");
                    }
                }
                Response::Error(err) => {
                    return Err(anyhow!(
                        "branch_create failed: {} (errno={})",
                        String::from_utf8_lossy(&err.error),
                        err.code.unwrap_or_default()
                    ));
                }
                other => return Err(anyhow!("unexpected response: {:?}", other)),
            }
        }
        Command::BranchBind { branch, pid } => {
            let response = send_request(&mount, Request::branch_bind(branch, Some(pid)))?;
            match response {
                Response::BranchBind(_) => {
                    println!("BRANCH_BIND_OK");
                }
                Response::Error(err) => {
                    return Err(anyhow!(
                        "branch_bind failed: {} (errno={})",
                        String::from_utf8_lossy(&err.error),
                        err.code.unwrap_or_default()
                    ));
                }
                other => return Err(anyhow!("unexpected response: {:?}", other)),
            }
        }
        Command::FaultPolicySet { file } => {
            let bytes = read_policy_spec(&file)?;
            let response = send_request(&mount, Request::fault_policy_set(bytes))?;
            match response {
                Response::FaultPolicyStatus(status) => {
                    println!(
                        "FAULT_POLICY enabled={} active={} rules={}",
                        status.enabled, status.active, status.rule_count
                    );
                }
                Response::Error(err) => {
                    return Err(anyhow!(
                        "fault_policy_set failed: {} (errno={})",
                        String::from_utf8_lossy(&err.error),
                        err.code.unwrap_or_default()
                    ));
                }
                other => return Err(anyhow!("unexpected response: {:?}", other)),
            }
        }
        Command::FaultPolicyClear => {
            let response = send_request(&mount, Request::fault_policy_clear())?;
            match response {
                Response::FaultPolicyStatus(status) => {
                    println!(
                        "FAULT_POLICY enabled={} active={} rules={}",
                        status.enabled, status.active, status.rule_count
                    );
                }
                Response::Error(err) => {
                    return Err(anyhow!(
                        "fault_policy_clear failed: {} (errno={})",
                        String::from_utf8_lossy(&err.error),
                        err.code.unwrap_or_default()
                    ));
                }
                other => return Err(anyhow!("unexpected response: {:?}", other)),
            }
        }
    }

    Ok(())
}

fn read_policy_spec(path: &Path) -> Result<Vec<u8>> {
    if path == Path::new("-") {
        let mut buf = Vec::new();
        io::stdin().read_to_end(&mut buf)?;
        Ok(buf)
    } else {
        fs::read(path).with_context(|| format!("failed to read policy file {:?}", path))
    }
}

fn send_request(mount: &Path, request: Request) -> Result<Response> {
    let control_path = mount.join(".agentfs").join("control");
    if !control_path.exists() {
        return Err(anyhow!("control file {:?} not found", control_path));
    }

    let request_bytes = request.as_ssz_bytes();
    if request_bytes.len() + 4 > 4096 {
        return Err(anyhow!("request too large ({} bytes)", request_bytes.len()));
    }
    let mut buffer = Vec::with_capacity(4096);
    buffer.extend_from_slice(&(request_bytes.len() as u32).to_le_bytes());
    buffer.extend_from_slice(&request_bytes);
    buffer.resize(4096, 0);

    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&control_path)
        .with_context(|| format!("failed to open control file {:?}", control_path))?;
    let fd = file.as_raw_fd();

    let result = unsafe {
        libc::ioctl(
            fd,
            AGENTFS_IOCTL_CMD,
            buffer.as_mut_ptr() as *mut libc::c_void,
        )
    };
    drop(file);

    if result < 0 {
        return Err(io::Error::last_os_error()).context("ioctl failed");
    }

    if buffer.len() < 4 {
        return Err(anyhow!("response truncated"));
    }
    let mut len_bytes = [0u8; 4];
    len_bytes.copy_from_slice(&buffer[..4]);
    let response_len = u32::from_le_bytes(len_bytes) as usize;
    if response_len == 0 || 4 + response_len > buffer.len() {
        return Err(anyhow!("invalid response length {}", response_len));
    }
    Response::from_ssz_bytes(&buffer[4..4 + response_len])
        .map_err(|e| anyhow!("failed to decode SSZ response: {:?}", e))
}
