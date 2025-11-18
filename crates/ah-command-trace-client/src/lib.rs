// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! High-level client for interacting with the command trace server.
//!
//! This crate wraps the low-level SSZ-based RPC protocol exposed by the
//! command trace server and provides a convenient API for other components
//! (such as the interpose shim) to establish a handshake and issue requests.

use ah_command_trace_proto::{
    CommandStart, HandshakeMessage, Request, Response, decode_ssz, encode_ssz,
};
use anyhow::{Context, Result, anyhow};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

/// Configuration describing how a client should identify itself to the server.
#[derive(Clone, Debug)]
pub struct ClientConfig {
    handshake_version: String,
    #[allow(dead_code)] // Reserved for future handshake extensions
    shim_name: String,
    #[allow(dead_code)] // Reserved for future handshake extensions
    crate_version: String,
    #[allow(dead_code)] // Reserved for future handshake extensions
    features: Vec<String>,
    process: ProcessConfig,
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
            handshake_version: Some("1.0".to_string()),
            shim_name: Some(shim_name.into()),
            crate_version: Some(crate_version.into()),
            features: Vec::new(),
            process: None,
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
    read_timeout: Option<Duration>,
    write_timeout: Option<Duration>,
}

impl ClientConfigBuilder {
    /// Override the handshake protocol version (defaults to `1.0`).
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
            read_timeout: self.read_timeout,
            write_timeout: self.write_timeout,
        })
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

        let exe_path =
            std::env::current_exe().context("failed to resolve current executable path")?;

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

/// Handshake connection to the command trace server.
pub struct CommandTraceClient {
    stream: UnixStream,
}

impl CommandTraceClient {
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
        let handshake_bytes = encode_ssz(&handshake);

        // Send length-prefixed message
        let handshake_len = handshake_bytes.len() as u32;
        stream
            .write_all(&handshake_len.to_le_bytes())
            .and_then(|_| stream.write_all(&handshake_bytes))
            .with_context(|| format!("failed to send handshake to {}", socket_display))?;

        // Read response (length + SSZ message)
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).with_context(|| {
            format!(
                "failed to read handshake acknowledgement from {}",
                socket_display
            )
        })?;
        let response_len = u32::from_le_bytes(len_buf) as usize;

        let mut response_buf = vec![0u8; response_len];
        stream.read_exact(&mut response_buf).with_context(|| {
            format!(
                "failed to read handshake response payload from {}",
                socket_display
            )
        })?;

        // Decode SSZ response
        let response_msg: Response = decode_ssz(&response_buf)
            .map_err(|e| anyhow!("Failed to decode SSZ response: {:?}", e))?;

        match response_msg {
            Response::Handshake(response) => {
                if !response.success {
                    return Err(anyhow!("Handshake failed"));
                }
            }
        }

        Ok(Self { stream })
    }

    /// Raw access to the underlying socket (consumes the client).
    pub fn into_stream(self) -> UnixStream {
        self.stream
    }

    /// Send a raw request and expect no response (one-way message).
    pub fn send_request(&mut self, request: Request) -> Result<()> {
        let request_bytes = encode_ssz(&request);
        let request_len = request_bytes.len() as u32;

        self.stream
            .write_all(&request_len.to_le_bytes())
            .and_then(|_| self.stream.write_all(&request_bytes))
            .context("failed to send command trace request")?;

        Ok(())
    }

    /// Send a CommandStart notification.
    pub fn send_command_start(&mut self, command_start: CommandStart) -> Result<()> {
        self.send_request(Request::CommandStart(command_start))
    }
}

fn build_handshake(config: &ClientConfig) -> Result<Request> {
    Ok(Request::Handshake(HandshakeMessage {
        version: config.handshake_version.as_bytes().to_vec(),
        pid: config.process.pid,
        platform: std::env::consts::OS.as_bytes().to_vec(),
    }))
}
