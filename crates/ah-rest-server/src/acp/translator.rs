// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! ACP translation helpers built on the upstream SDK types.
//!
//! We currently only use the SDK data structures (e.g. `AgentCapabilities`) for
//! capability negotiation; the full JSON-RPC runtime swap will follow. Keeping
//! this layer thin makes it easier to graduate to the SDK transport without
//! rewriting downstream code.

use crate::config::{AcpConfig, AcpTransportMode};
use agent_client_protocol::{AgentCapabilities, McpCapabilities, PromptCapabilities};
use serde_json::{Value, json};

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

    pub fn negotiate_caps(config: &AcpConfig) -> AgentCapabilities {
        let mut transports = vec!["websocket".to_string()];
        if matches!(config.transport, AcpTransportMode::Stdio) {
            transports.push("stdio".to_string());
        }
        AgentCapabilities {
            load_session: true,
            prompt_capabilities: PromptCapabilities {
                image: false,
                audio: false,
                embedded_context: false,
                meta: None,
            },
            mcp_capabilities: McpCapabilities {
                http: true,
                sse: true,
                meta: None,
            },
            meta: Some(harbor_meta_caps(transports)),
        }
    }

    pub fn initialize_response(caps: &AgentCapabilities) -> Value {
        let transports = caps
            .meta
            .as_ref()
            .and_then(|m| m.pointer("/agent.harbor/transports").cloned())
            .unwrap_or_else(|| json!(["websocket"]));
        json!({
            "capabilities": {
                "loadSession": caps.load_session,
                "promptCapabilities": caps.prompt_capabilities,
                "mcp": caps.mcp_capabilities,
                "transports": transports,
                "_meta": caps.meta.clone().unwrap_or(json!({}))
            }
        })
    }

    pub fn ignore_unknown_caps(input: &Value) -> AgentCapabilities {
        let load_session = input
            .pointer("/capabilities/loadSession")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let prompt_caps = input
            .pointer("/capabilities/promptCapabilities")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let mcp_caps = input.pointer("/capabilities/mcp").cloned().unwrap_or_else(|| json!({}));

        let mut caps = AgentCapabilities::default();
        caps.load_session = load_session;
        caps.prompt_capabilities = serde_json::from_value(prompt_caps).unwrap_or_default();
        caps.mcp_capabilities = serde_json::from_value(mcp_caps).unwrap_or_default();
        caps.meta = input.pointer("/capabilities/_meta").cloned().or_else(|| Some(json!({})));
        caps
    }
}

fn harbor_meta_caps(transports: Vec<String>) -> Value {
    json!({
        "agent.harbor": {
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
            },
            "transports": transports
        }
    })
}
