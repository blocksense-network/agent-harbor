// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared wire protocol between the task manager and recorder.
//!
//! The task manager socket is a bidirectional Unix domain socket used by
//! `ah agent record` to stream live session events back to the task manager
//! and to receive interactive control messages (e.g. injected user input)
//! while the agent is running. Messages are length‑prefixed SSZ payloads
//! using the union below.

use ah_rest_api_contract::types::SessionEvent;
use ssz::{Decode, Encode};
use ssz_derive::{Decode as SszDecode, Encode as SszEncode};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Bidirectional socket message
#[derive(Clone, Debug, PartialEq, Eq, SszEncode, SszDecode)]
#[ssz(enum_behaviour = "union")]
pub enum TaskManagerMessage {
    /// Recorder → task manager: live session event (status/log/tool/etc)
    SessionEvent(SessionEvent),
    /// Task manager → recorder: raw bytes to inject into the running PTY
    InjectInput(Vec<u8>),
    /// Recorder → task manager: raw PTY output bytes for follower/backlog
    PtyData(Vec<u8>),
    /// Recorder → task manager: terminal resize notification (cols, rows)
    PtyResize((u16, u16)),
}

impl TaskManagerMessage {
    /// Convenience helper to SSZ‑encode with a 4‑byte little‑endian length prefix.
    pub fn to_length_prefixed_bytes(&self) -> Vec<u8> {
        let payload = self.as_ssz_bytes();
        let mut framed = (payload.len() as u32).to_le_bytes().to_vec();
        framed.extend_from_slice(&payload);
        framed
    }

    /// Decode a single message from an async reader (expects length‑prefixed SSZ)
    pub async fn read_from<R>(reader: &mut R) -> std::io::Result<Self>
    where
        R: AsyncReadExt + Unpin,
    {
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf).await?;
        let len = u32::from_le_bytes(len_buf) as usize;

        let mut buf = vec![0u8; len];
        reader.read_exact(&mut buf).await?;

        <TaskManagerMessage as Decode>::from_ssz_bytes(&buf)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{:?}", e)))
    }

    /// Write a single length‑prefixed message to an async writer.
    pub async fn write_to<W>(&self, writer: &mut W) -> std::io::Result<()>
    where
        W: AsyncWriteExt + Unpin,
    {
        let framed = self.to_length_prefixed_bytes();
        writer.write_all(&framed).await
    }
}

/// Get the base directory for AH sockets following OS conventions
pub fn socket_dir() -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;

    // Prefer explicit override for tests or custom deployments
    let base_dir = if let Ok(dir) = std::env::var("AH_SOCKET_DIR") {
        std::path::PathBuf::from(dir)
    } else if cfg!(target_os = "linux") {
        // Linux: prefer XDG_RUNTIME_DIR, fallback to /tmp
        std::env::var("XDG_RUNTIME_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"))
            .join("ah")
    } else if cfg!(target_os = "macos") {
        // macOS: use TMPDIR
        std::env::var("TMPDIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"))
            .join("ah")
    } else {
        // Other Unix-like systems: /tmp
        std::path::PathBuf::from("/tmp").join("ah")
    };

    // Create directory with proper permissions if it doesn't exist
    if !base_dir.exists() {
        if let Err(e) = std::fs::create_dir_all(&base_dir) {
            tracing::warn!(
                "Failed to create socket directory {}: {}",
                base_dir.display(),
                e
            );
        } else if let Ok(metadata) = std::fs::metadata(&base_dir) {
            // Set permissions to 0700 (owner read/write/execute only)
            let mut perms = metadata.permissions();
            perms.set_mode(0o700);
            let _ = std::fs::set_permissions(&base_dir, perms);
        }
    }

    base_dir
}

/// Path for the shared task manager socket used by recorder ↔ task manager IPC
pub fn task_manager_socket_path() -> std::path::PathBuf {
    socket_dir().join("task-manager.sock")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn task_manager_message_round_trip() {
        use tokio::io::duplex;

        let event = SessionEvent::status(
            ah_rest_api_contract::SessionStatus::Running,
            chrono::Utc::now().timestamp_millis() as u64,
        );
        let msg = TaskManagerMessage::SessionEvent(event.clone());

        let (mut a, mut b) = duplex(1024);
        let writer = msg.write_to(&mut a);
        let reader = TaskManagerMessage::read_from(&mut b);

        writer.await.expect("write");
        let decoded = reader.await.expect("read");
        assert_eq!(decoded, msg);

        let inject = TaskManagerMessage::InjectInput(b"hi".to_vec());
        let (mut a, mut b) = duplex(1024);
        inject.write_to(&mut a).await.expect("write inject");
        let decoded = TaskManagerMessage::read_from(&mut b).await.expect("read inject");
        assert_eq!(decoded, inject);

        let pty = TaskManagerMessage::PtyData(b"bytes".to_vec());
        let (mut a, mut b) = duplex(1024);
        pty.write_to(&mut a).await.expect("write pty");
        let decoded = TaskManagerMessage::read_from(&mut b).await.expect("read pty");
        assert_eq!(decoded, pty);

        let resize = TaskManagerMessage::PtyResize((120, 40));
        let (mut a, mut b) = duplex(1024);
        resize.write_to(&mut a).await.expect("write resize");
        let decoded = TaskManagerMessage::read_from(&mut b).await.expect("read resize");
        assert_eq!(decoded, resize);
    }
}
