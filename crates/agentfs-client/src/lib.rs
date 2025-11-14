// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! High-level client for interacting with the AgentFS daemon.
//!
//! This crate wraps the low-level SSZ-based RPC protocol exposed by the
//! AgentFS daemon and provides a convenient API for other components (such as
//! the interpose shim or filesystem snapshot providers) to establish a
//! handshake and issue requests.

use agentfs_daemon::{
    AllowlistInfo, HandshakeData, HandshakeMessage, ProcessInfo, ShimInfo, decode_ssz_message,
    encode_ssz_message,
};
use agentfs_proto::{
    BranchCreateResponse, BranchInfo as ProtoBranchInfo, Request, Response, SnapshotCreateResponse,
    SnapshotExportReleaseResponse, SnapshotExportResponse, SnapshotInfo as ProtoSnapshotInfo,
    SnapshotListResponse,
};
use anyhow::{Context, Result, anyhow};
use libc;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Configuration describing how a client should identify itself to the daemon.
#[derive(Clone, Debug)]
pub struct ClientConfig {
    handshake_version: String,
    shim_name: String,
    crate_version: String,
    features: Vec<String>,
    process: ProcessConfig,
    allowlist: Option<AllowlistConfig>,
    read_timeout: Option<Duration>,
    write_timeout: Option<Duration>,
}

impl ClientConfig {
    /// Start building configuration for a client.
    pub fn builder(
        shim_name: impl Into<String>,
        crate_version: impl Into<String>,
    ) -> ClientConfigBuilder {
        ClientConfigBuilder {
            handshake_version: Some("1".to_string()),
            shim_name: Some(shim_name.into()),
            crate_version: Some(crate_version.into()),
            features: Vec::new(),
            process: None,
            allowlist: None,
            read_timeout: None,
            write_timeout: None,
        }
    }
}

/// Builder for [`ClientConfig`].
pub struct ClientConfigBuilder {
    handshake_version: Option<String>,
    shim_name: Option<String>,
    crate_version: Option<String>,
    features: Vec<String>,
    process: Option<ProcessConfig>,
    allowlist: Option<AllowlistConfig>,
    read_timeout: Option<Duration>,
    write_timeout: Option<Duration>,
}

impl ClientConfigBuilder {
    /// Override the handshake protocol version (defaults to `1`).
    pub fn handshake_version(mut self, version: impl Into<String>) -> Self {
        self.handshake_version = Some(version.into());
        self
    }

    /// Add a single feature string advertised in the handshake.
    pub fn feature(mut self, feature: impl Into<String>) -> Self {
        self.features.push(feature.into());
        self
    }

    /// Replace the feature list with the provided entries.
    pub fn features<I, S>(mut self, features: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.features = features.into_iter().map(Into::into).collect();
        self
    }

    /// Provide explicit process metadata used during the handshake.
    pub fn process(mut self, process: ProcessConfig) -> Self {
        self.process = Some(process);
        self
    }

    /// Provide allowlist metadata used during the handshake.
    pub fn allowlist(mut self, allowlist: AllowlistConfig) -> Self {
        self.allowlist = Some(allowlist);
        self
    }

    /// Set the read timeout applied to the underlying socket.
    pub fn read_timeout(mut self, timeout: Duration) -> Self {
        self.read_timeout = Some(timeout);
        self
    }

    /// Set the write timeout applied to the underlying socket.
    pub fn write_timeout(mut self, timeout: Duration) -> Self {
        self.write_timeout = Some(timeout);
        self
    }

    /// Finalise the configuration.
    pub fn build(self) -> Result<ClientConfig> {
        let handshake_version =
            self.handshake_version.ok_or_else(|| anyhow!("handshake version missing"))?;
        let shim_name = self.shim_name.ok_or_else(|| anyhow!("shim name missing"))?;
        let crate_version = self.crate_version.ok_or_else(|| anyhow!("crate version missing"))?;

        let process = match self.process {
            Some(process) => process,
            None => ProcessConfig::current_process()
                .context("failed to gather current process metadata")?,
        };

        Ok(ClientConfig {
            handshake_version,
            shim_name,
            crate_version,
            features: self.features,
            process,
            allowlist: self.allowlist,
            read_timeout: self.read_timeout,
            write_timeout: self.write_timeout,
        })
    }
}

/// Allowlist metadata advertised during handshake.
#[derive(Clone, Debug)]
pub struct AllowlistConfig {
    pub matched_entry: Option<String>,
    pub configured_entries: Option<Vec<String>>,
}

impl AllowlistConfig {
    pub fn new(matched_entry: Option<String>, configured_entries: Option<Vec<String>>) -> Self {
        Self {
            matched_entry,
            configured_entries,
        }
    }
}

/// Process metadata used during handshake.
#[derive(Clone, Debug)]
pub struct ProcessConfig {
    pub pid: u32,
    pub ppid: u32,
    pub uid: u32,
    pub gid: u32,
    pub exe_path: String,
    pub exe_name: String,
}

impl ProcessConfig {
    pub fn new(
        pid: u32,
        ppid: u32,
        uid: u32,
        gid: u32,
        exe_path: impl Into<String>,
        exe_name: impl Into<String>,
    ) -> Self {
        Self {
            pid,
            ppid,
            uid,
            gid,
            exe_path: exe_path.into(),
            exe_name: exe_name.into(),
        }
    }

    pub fn current_process() -> Result<Self> {
        let pid = std::process::id();
        let ppid = unsafe { libc::getppid() as u32 };
        let uid = unsafe { libc::geteuid() as u32 };
        let gid = unsafe { libc::getegid() as u32 };

        let exe_path = std::env::current_exe()
            .or_else(|_| {
                // Fallback to /proc/self/exe if available.
                let fallback = PathBuf::from("/proc/self/exe");
                if fallback.exists() {
                    fs::read_link(&fallback)
                } else {
                    Err(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "unable to determine current executable",
                    ))
                }
            })
            .context("failed to resolve current executable path")?;

        let exe_name = exe_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| anyhow!("current executable name not valid UTF-8"))?
            .to_string();

        Ok(Self {
            pid,
            ppid,
            uid,
            gid,
            exe_path: exe_path.display().to_string(),
            exe_name,
        })
    }
}

/// Handshake connection to the AgentFS daemon.
pub struct AgentFsClient {
    stream: UnixStream,
    handshake_ack: Vec<u8>,
}

impl AgentFsClient {
    /// Establish a handshake and return a ready-to-use client.
    pub fn connect(socket_path: &Path, config: &ClientConfig) -> Result<Self> {
        let socket_display = socket_path.display();
        let mut stream = UnixStream::connect(socket_path)
            .with_context(|| format!("failed to connect to {}", socket_display))?;

        if let Some(timeout) = config.read_timeout {
            stream
                .set_read_timeout(Some(timeout))
                .with_context(|| format!("failed to set read timeout on {}", socket_display))?;
        }
        if let Some(timeout) = config.write_timeout {
            stream
                .set_write_timeout(Some(timeout))
                .with_context(|| format!("failed to set write timeout on {}", socket_display))?;
        }

        let handshake = build_handshake(config)?;
        let handshake_bytes = encode_ssz_message(&handshake);
        let handshake_len = handshake_bytes.len() as u32;

        stream
            .write_all(&handshake_len.to_le_bytes())
            .and_then(|_| stream.write_all(&handshake_bytes))
            .with_context(|| format!("failed to send handshake to {}", socket_display))?;

        let mut ack_buf = vec![0u8; 1024];
        let ack_len = stream.read(&mut ack_buf).with_context(|| {
            format!(
                "failed to read handshake acknowledgement from {}",
                socket_display
            )
        })?;
        ack_buf.truncate(ack_len);

        Ok(Self {
            stream,
            handshake_ack: ack_buf,
        })
    }

    /// Raw access to the underlying socket (consumes the client).
    pub fn into_stream(self) -> UnixStream {
        self.stream
    }

    /// Returns the raw acknowledgement bytes received after the handshake.
    pub fn handshake_ack(&self) -> &[u8] {
        &self.handshake_ack
    }

    /// Send a raw request and decode the corresponding response.
    pub fn send_request(&mut self, request: Request) -> Result<Response> {
        let request_bytes = encode_ssz_message(&request);
        let request_len = request_bytes.len() as u32;

        let debug = std::env::var("AGENTFS_CLIENT_DEBUG").is_ok();
        if debug {
            println!(
                "AgentFsClient: sending request tag={:?} bytes={}",
                request,
                request_bytes.len()
            );
        }

        self.stream
            .write_all(&request_len.to_le_bytes())
            .and_then(|_| self.stream.write_all(&request_bytes))
            .context("failed to send AgentFS request")?;

        if debug {
            println!("AgentFsClient: request dispatched, waiting for response len");
        }

        let mut len_buf = [0u8; 4];
        self.stream
            .read_exact(&mut len_buf)
            .context("failed to read AgentFS response length")?;
        if debug {
            println!(
                "AgentFsClient: response length header ok, len={}",
                u32::from_le_bytes(len_buf)
            );
        }
        let response_len = u32::from_le_bytes(len_buf) as usize;
        let mut response_buf = vec![0u8; response_len];
        self.stream
            .read_exact(&mut response_buf)
            .context("failed to read AgentFS response payload")?;
        if debug {
            println!(
                "AgentFsClient: response payload read ({} bytes)",
                response_len
            );
        }

        decode_ssz_message(&response_buf)
            .map_err(|err| anyhow!("failed to decode AgentFS response: {:?}", err))
    }

    /// Create a snapshot via the daemon.
    pub fn snapshot_create(&mut self, name: Option<String>) -> Result<SnapshotRecord> {
        let response = self.send_request(Request::snapshot_create(name))?;
        match response {
            Response::SnapshotCreate(SnapshotCreateResponse { snapshot }) => {
                Ok(SnapshotRecord::from_proto(snapshot))
            }
            Response::Error(err) => Err(anyhow!(
                "AgentFS snapshot_create failed: {}",
                String::from_utf8_lossy(&err.error)
            )),
            other => Err(anyhow!(
                "unexpected response for snapshot_create: {:?}",
                other
            )),
        }
    }

    /// List snapshots from the daemon.
    pub fn snapshot_list(&mut self) -> Result<Vec<SnapshotRecord>> {
        let response = self.send_request(Request::snapshot_list())?;
        match response {
            Response::SnapshotList(SnapshotListResponse { snapshots }) => {
                Ok(snapshots.into_iter().map(SnapshotRecord::from_proto).collect())
            }
            Response::Error(err) => Err(anyhow!(
                "AgentFS snapshot_list failed: {}",
                String::from_utf8_lossy(&err.error)
            )),
            other => Err(anyhow!(
                "unexpected response for snapshot_list: {:?}",
                other
            )),
        }
    }

    /// Export an existing snapshot as a read-only filesystem view that the caller can mount.
    ///
    /// The AgentFS daemon materialises the snapshot into a temporary directory and returns both
    /// the export path and an opaque cleanup token. Call [`snapshot_export_release`] once the
    /// caller is finished with the exported view so the daemon can tear it down promptly.
    ///
    /// # Errors
    ///
    /// Returns an error if the daemon rejects the request or if the response cannot be decoded.
    pub fn snapshot_export(&mut self, snapshot_id: &str) -> Result<SnapshotExportRecord> {
        let response = self.send_request(Request::snapshot_export(snapshot_id.to_string()))?;
        match response {
            Response::SnapshotExport(SnapshotExportResponse {
                path,
                cleanup_token,
            }) => {
                let path_str = String::from_utf8(path)
                    .map_err(|err| anyhow!("invalid UTF-8 path from snapshot_export: {:?}", err))?;
                let token_str = String::from_utf8(cleanup_token).map_err(|err| {
                    anyhow!(
                        "invalid UTF-8 cleanup token from snapshot_export: {:?}",
                        err
                    )
                })?;
                Ok(SnapshotExportRecord {
                    path: PathBuf::from(path_str),
                    cleanup_token: token_str,
                })
            }
            Response::Error(err) => Err(anyhow!(
                "AgentFS snapshot_export failed: {}",
                String::from_utf8_lossy(&err.error)
            )),
            other => Err(anyhow!(
                "unexpected response for snapshot_export: {:?}",
                other
            )),
        }
    }

    /// Release a previously exported snapshot view.
    ///
    /// Passing the cleanup token returned by [`snapshot_export`] allows the daemon to delete the
    /// temporary export directory and associated resources. Calling this method more than once
    /// with the same token is harmless; the daemon returns an error if the token is unknown.
    ///
    /// # Errors
    ///
    /// Returns an error if the daemon reports a failure or if the returned token mismatches the
    /// provided one.
    pub fn snapshot_export_release(&mut self, cleanup_token: &str) -> Result<()> {
        let response =
            self.send_request(Request::snapshot_export_release(cleanup_token.to_string()))?;
        match response {
            Response::SnapshotExportRelease(SnapshotExportReleaseResponse {
                cleanup_token: returned,
            }) => {
                if let Ok(token) = String::from_utf8(returned.clone()) {
                    if token != cleanup_token {
                        return Err(anyhow!(
                            "AgentFS snapshot_export_release returned mismatched token (expected {}, got {})",
                            cleanup_token,
                            token
                        ));
                    }
                }
                Ok(())
            }
            Response::Error(err) => Err(anyhow!(
                "AgentFS snapshot_export_release failed: {}",
                String::from_utf8_lossy(&err.error)
            )),
            other => Err(anyhow!(
                "unexpected response for snapshot_export_release: {:?}",
                other
            )),
        }
    }

    /// Create a branch from an existing snapshot.
    pub fn branch_create(
        &mut self,
        from_snapshot: &str,
        name: Option<String>,
    ) -> Result<BranchRecord> {
        let response =
            self.send_request(Request::branch_create(from_snapshot.to_string(), name))?;
        match response {
            Response::BranchCreate(BranchCreateResponse { branch }) => {
                Ok(BranchRecord::from_proto(branch))
            }
            Response::Error(err) => Err(anyhow!(
                "AgentFS branch_create failed: {}",
                String::from_utf8_lossy(&err.error)
            )),
            other => Err(anyhow!(
                "unexpected response for branch_create: {:?}",
                other
            )),
        }
    }

    /// Bind a process to a branch.
    pub fn branch_bind(&mut self, branch: &str, pid: Option<u32>) -> Result<()> {
        let response = self.send_request(Request::branch_bind(branch.to_string(), pid))?;
        match response {
            Response::BranchBind(_) => Ok(()),
            Response::Error(err) => Err(anyhow!(
                "AgentFS branch_bind failed: {}",
                String::from_utf8_lossy(&err.error)
            )),
            other => Err(anyhow!("unexpected response for branch_bind: {:?}", other)),
        }
    }
}

/// Snapshot metadata returned by the client.
#[derive(Clone, Debug)]
pub struct SnapshotRecord {
    pub id: String,
    pub name: Option<String>,
}

/// Information about an exported snapshot mount.
///
/// Instances of this type are produced by [`AgentFsClient::snapshot_export`]. Use the
/// `cleanup_token` with [`AgentFsClient::snapshot_export_release`] when the read-only export is no
/// longer needed.
pub struct SnapshotExportRecord {
    /// Filesystem path containing the exported read-only snapshot view.
    pub path: PathBuf,
    /// Token that must be supplied to release the exported resources.
    pub cleanup_token: String,
}

impl SnapshotRecord {
    fn from_proto(proto: ProtoSnapshotInfo) -> Self {
        Self {
            id: bytes_to_string(proto.id),
            name: proto.name.map(bytes_to_string),
        }
    }
}

/// Branch metadata returned by the client.
#[derive(Clone, Debug)]
pub struct BranchRecord {
    pub id: String,
    pub parent: Option<String>,
    pub name: Option<String>,
}

impl BranchRecord {
    fn from_proto(proto: ProtoBranchInfo) -> Self {
        Self {
            id: bytes_to_string(proto.id),
            parent: Some(bytes_to_string(proto.parent)).filter(|s| !s.is_empty()),
            name: proto.name.map(bytes_to_string),
        }
    }
}

fn build_handshake(config: &ClientConfig) -> Result<HandshakeMessage> {
    let allowlist = config
        .allowlist
        .as_ref()
        .map(|allow| AllowlistInfo {
            matched_entry: allow.matched_entry.clone().map(|s| s.into_bytes()),
            configured_entries: allow
                .configured_entries
                .clone()
                .map(|entries| entries.into_iter().map(|entry| entry.into_bytes()).collect()),
        })
        .unwrap_or_else(AllowlistInfo::default);

    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
        .to_string()
        .into_bytes();

    Ok(HandshakeMessage::Handshake(HandshakeData {
        version: config.handshake_version.as_bytes().to_vec(),
        shim: ShimInfo {
            name: config.shim_name.as_bytes().to_vec(),
            crate_version: config.crate_version.as_bytes().to_vec(),
            features: config.features.iter().map(|feature| feature.as_bytes().to_vec()).collect(),
        },
        process: ProcessInfo {
            pid: config.process.pid,
            ppid: config.process.ppid,
            uid: config.process.uid,
            gid: config.process.gid,
            exe_path: config.process.exe_path.as_bytes().to_vec(),
            exe_name: config.process.exe_name.as_bytes().to_vec(),
        },
        allowlist,
        timestamp,
    }))
}

fn bytes_to_string(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes.clone())
        .unwrap_or_else(|_| bytes.into_iter().map(|b| format!("{:02x}", b)).collect::<String>())
}
