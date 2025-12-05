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
use serde::Deserialize;
use serde_json::{Map, Value, json};
use tracing::warn;

/// Translator utilities for JSON-RPC payloads.
#[derive(Debug, Default, Clone)]
pub struct JsonRpcTranslator;

/// Minimal Initialize request parser to avoid tight coupling to SDK defaults.
#[derive(Default, Clone, Debug, Deserialize)]
pub struct InitializeLite {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: Option<String>,
    #[serde(rename = "_meta", default)]
    pub meta: Option<Value>,
}

impl JsonRpcTranslator {
    pub fn new() -> Self {
        Self
    }

    pub fn describe(&self) -> &'static str {
        "ACP translator scaffold (Milestone 2)"
    }

    pub fn negotiate_caps(config: &AcpConfig) -> AgentCapabilities {
        let mut transports = config.transports();
        // Preserve legacy single-transport behavior if callers constructed an
        // AcpConfig without the helper fields.
        if transports.is_empty() {
            transports.push("websocket".to_string());
            if matches!(config.transport, AcpTransportMode::Stdio) {
                transports.push("stdio".to_string());
            }
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
        Self::initialize_response_typed(caps, &InitializeLite::default())
    }

    pub fn initialize_response_typed(caps: &AgentCapabilities, req: &InitializeLite) -> Value {
        let transports = caps
            .meta
            .as_ref()
            .and_then(|m| m.pointer("/agent.harbor/transports").cloned())
            .unwrap_or_else(|| json!(["websocket"]));
        let mut response_meta = caps.meta.clone().unwrap_or(json!({}));
        response_meta
            .as_object_mut()
            .and_then(|m| m.get_mut("agent.harbor"))
            .and_then(|m| m.as_object_mut())
            .map(|m| m.insert("transports".into(), transports.clone()));

        json!({
            "protocolVersion": req.protocol_version.clone().unwrap_or_else(|| "1.0".into()),
            "capabilities": {
                "loadSession": caps.load_session,
                "promptCapabilities": caps.prompt_capabilities,
                "mcp": caps.mcp_capabilities,
                "transports": transports,
                "_meta": response_meta
            },
            "authMethods": []
        })
    }

    pub fn ignore_unknown_caps(input: &Value) -> AgentCapabilities {
        if let Some(map) = input.pointer("/capabilities").and_then(|v| v.as_object()) {
            log_unknown_fields(
                map,
                &["loadSession", "promptCapabilities", "mcp", "_meta"],
                "capabilities",
            );
            if let Some(prompt) = map.get("promptCapabilities").and_then(|v| v.as_object()) {
                log_unknown_fields(
                    prompt,
                    &["image", "audio", "embeddedContext", "meta"],
                    "capabilities.promptCapabilities",
                );
            }
            if let Some(mcp) = map.get("mcp").and_then(|v| v.as_object()) {
                log_unknown_fields(mcp, &["http", "sse", "meta"], "capabilities.mcp");
            }
        }

        let load_session = input
            .pointer("/capabilities/loadSession")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let prompt_caps = input
            .pointer("/capabilities/promptCapabilities")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let mcp_caps = input.pointer("/capabilities/mcp").cloned().unwrap_or_else(|| json!({}));

        AgentCapabilities {
            load_session,
            prompt_capabilities: serde_json::from_value(prompt_caps).unwrap_or_default(),
            mcp_capabilities: serde_json::from_value(mcp_caps).unwrap_or_default(),
            meta: input.pointer("/capabilities/_meta").cloned().or_else(|| Some(json!({}))),
        }
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

fn log_unknown_fields(map: &Map<String, Value>, known: &[&str], context: &str) {
    for key in map.keys() {
        if !known.iter().any(|k| k == key) {
            warn!(%context, %key, "Unknown capability flag ignored: {key}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_test::traced_test;

    #[traced_test]
    #[test]
    fn unknown_capabilities_emit_warnings_and_preserve_known_fields() {
        let noisy = json!({
            "capabilities": {
                "loadSession": true,
                "promptCapabilities": { "image": true, "unexpectedPrompt": 1 },
                "mcp": { "http": true, "future": false },
                "unexpectedRoot": "foo"
            }
        });

        let caps = JsonRpcTranslator::ignore_unknown_caps(&noisy);
        assert!(caps.load_session, "known flags should round-trip");
        assert!(caps.mcp_capabilities.http);
        assert!(caps.prompt_capabilities.image);

        logs_assert(|lines: &[&str]| {
            lines
                .iter()
                .find(|line| {
                    line.contains("Unknown capability flag ignored")
                        && line.contains("unexpectedRoot")
                })
                .map(|_| Ok(()))
                .unwrap_or_else(|| Err("expected warning for unexpectedRoot".to_string()))
        });

        logs_assert(|lines: &[&str]| {
            lines
                .iter()
                .find(|line| {
                    line.contains("capabilities.promptCapabilities")
                        && line.contains("unexpectedPrompt")
                })
                .map(|_| Ok(()))
                .unwrap_or_else(|| Err("expected warning for prompt capability".to_string()))
        });

        logs_assert(|lines: &[&str]| {
            lines
                .iter()
                .find(|line| line.contains("capabilities.mcp") && line.contains("future"))
                .map(|_| Ok(()))
                .unwrap_or_else(|| Err("expected warning for mcp capability".to_string()))
        });
    }

    #[test]
    fn uds_transport_advertises_stdio() {
        let cfg = AcpConfig {
            enabled: true,
            uds_path: Some(std::path::PathBuf::from("/tmp/acp.sock")),
            ..AcpConfig::default()
        };
        let caps = JsonRpcTranslator::negotiate_caps(&cfg);
        let transports = caps
            .meta
            .as_ref()
            .and_then(|m| m.pointer("/agent.harbor/transports"))
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let values: Vec<String> = transports
            .into_iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
        assert!(values.contains(&"stdio".to_string()));
        assert!(values.contains(&"websocket".to_string()));
    }
}
