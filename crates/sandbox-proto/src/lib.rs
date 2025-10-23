// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Protocol definitions for communication between sandbox helper and supervisor.

use serde::{Deserialize, Serialize};

/// Message types for helper-supervisor communication
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum Message {
    /// Filesystem access request
    #[serde(rename = "fs_request")]
    FilesystemRequest(FilesystemRequest),

    /// Response to filesystem access request
    #[serde(rename = "fs_response")]
    FilesystemResponse(FilesystemResponse),

    /// Audit log entry
    #[serde(rename = "audit")]
    Audit(AuditEntry),
}

/// Filesystem access request from sandbox
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesystemRequest {
    pub path: String,
    pub operation: String,
    pub pid: u32,
}

/// Response to filesystem access request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesystemResponse {
    pub allow: bool,
    pub reason: Option<String>,
}

/// Audit log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub timestamp: u64,
    pub event: String,
    pub details: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_serialization() {
        let request = Message::FilesystemRequest(FilesystemRequest {
            path: "/etc/passwd".to_string(),
            operation: "read".to_string(),
            pid: 1234,
        });

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: Message = serde_json::from_str(&json).unwrap();

        match deserialized {
            Message::FilesystemRequest(req) => {
                assert_eq!(req.path, "/etc/passwd");
                assert_eq!(req.operation, "read");
                assert_eq!(req.pid, 1234);
            }
            _ => panic!("Wrong message type"),
        }
    }
}
