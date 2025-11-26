// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Placeholder translation helpers for ACP JSON-RPC payloads.
//!
//! The ACP spec models messages in terms of thought/log/tool content blocks and
//! lifecycle methods (`initialize`, `session/new`, `session/update`, ...). The
//! real implementation will convert between the `agentclientprotocol/rust-sdk`
//! types and the REST/task-manager domain models that already power Harbor.

use crate::config::{AcpConfig, AcpTransportMode};
use serde_json::{Value, json};

/// Negotiated capability snapshot
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpCapabilities {
    pub transports: Vec<String>,
    pub fs_read: bool,
    pub fs_write: bool,
    pub terminals: bool,
}

impl Default for AcpCapabilities {
    fn default() -> Self {
        Self {
            transports: vec!["websocket".into()],
            fs_read: false,
            fs_write: false,
            terminals: true,
        }
    }
}

/// Translator utilities for JSON-RPC payloads.
#[derive(Debug, Default, Clone)]
pub struct JsonRpcTranslator;

impl JsonRpcTranslator {
    pub fn new() -> Self {
        Self
    }

    pub fn describe(&self) -> &'static str {
        "ACP translator scaffold (Milestone 2)"
    }

    pub fn negotiate_caps(config: &AcpConfig) -> AcpCapabilities {
        let mut transports = vec!["websocket".to_string()];
        if matches!(config.transport, AcpTransportMode::Stdio) {
            transports.push("stdio".to_string());
        }
        AcpCapabilities {
            transports,
            fs_read: false,
            fs_write: false,
            terminals: true,
        }
    }

    pub fn initialize_response(caps: &AcpCapabilities) -> Value {
        json!({
            "capabilities": {
                "transports": caps.transports,
                "filesystem": {
                    "readTextFile": caps.fs_read,
                    "writeTextFile": caps.fs_write
                },
                "terminal": caps.terminals,
                "_meta": {
                    "agent.harbor": harbor_meta_caps()
                }
            }
        })
    }

    pub fn ignore_unknown_caps(input: &Value) -> AcpCapabilities {
        let transports = input
            .get("capabilities")
            .and_then(|c| c.get("transports"))
            .and_then(|t| t.as_array())
            .map(|arr| {
                arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect::<Vec<_>>()
            })
            .unwrap_or_else(|| vec!["websocket".into()]);
        let fs_read = input
            .pointer("/capabilities/filesystem/readTextFile")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let fs_write = input
            .pointer("/capabilities/filesystem/writeTextFile")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let terminals = input
            .pointer("/capabilities/terminal")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        AcpCapabilities {
            transports,
            fs_read,
            fs_write,
            terminals,
        }
    }
}

fn harbor_meta_caps() -> Value {
    json!({
        "workspace": {
            "version": 1,
            "supportsDiffs": true
        },
        "snapshots": {
            "version": 1,
            "supportsTimelineSeek": true,
            "supportsBranching": true,
            "supportsFollowPlayback": true
        },
        "pipelineIntrospection": {
            "version": 1,
            "supportsStepStreaming": true
        }
    })
}
