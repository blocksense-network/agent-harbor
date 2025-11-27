// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use ah_rest_server::acp::translator::JsonRpcTranslator;
use ah_rest_server::config::AcpAuthPolicy;
use ah_rest_server::config::{AcpConfig, AcpTransportMode};
use common::acp::spawn_acp_server_with_scenario;
use futures::{SinkExt, StreamExt};
use proptest::prelude::*;
use serde_json::json;
use std::path::PathBuf;
use tokio_tungstenite::tungstenite::Message as WsMessage;

mod common;

#[test]
fn acp_initialize_caps_roundtrip() {
    let cfg = AcpConfig {
        transport: AcpTransportMode::WebSocket,
        ..AcpConfig::default()
    };
    let caps = JsonRpcTranslator::negotiate_caps(&cfg);
    assert!(caps.load_session);
    assert!(caps.mcp_capabilities.http);

    let payload = JsonRpcTranslator::initialize_response(&caps);
    assert_eq!(
        payload
            .pointer("/capabilities/_meta/agent.harbor/transports/0")
            .and_then(|v| v.as_str()),
        Some("websocket")
    );
    assert!(payload["capabilities"]["_meta"]["agent.harbor"]["workspace"].is_object());

    // Unknown flags should be ignored
    let noisy = json!({
        "capabilities": {
            "loadSession": false,
            "promptCapabilities": { "image": true },
            "mcp": { "http": true },
            "_meta": { "agent.harbor": { "workspace": { "supportsDiffs": true } } },
            "extra": {"foo":"bar"}
        }
    });
    let parsed = JsonRpcTranslator::ignore_unknown_caps(&noisy);
    assert!(!parsed.load_session, "loadSession should parse known flag");
    assert!(parsed.mcp_capabilities.http);
}

proptest! {
    #[test]
    fn unknown_capabilities_are_ignored_but_known_fields_respected(load_session in proptest::bool::ANY) {
        let noisy = json!({
            "capabilities": {
                "loadSession": load_session,
                "promptCapabilities": { "image": true, "someFutureFlag": true },
                "mcp": { "http": true, "sse": true, "future": false },
                "_meta": { "agent.harbor": { "workspace": { "supportsDiffs": true } } },
                "experimental": {"foo": "bar"}
            }
        });
        let parsed = JsonRpcTranslator::ignore_unknown_caps(&noisy);
        prop_assert_eq!(parsed.load_session, load_session);
        prop_assert!(parsed.mcp_capabilities.http);
    }
}

#[tokio::test]
async fn acp_initialize_and_auth_scenario_succeeds() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/acp_bridge/scenarios/initialize_and_auth.yaml");
    let (acp_url, handle) = spawn_acp_server_with_scenario(fixture).await;
    let acp_url = format!("{}?api_key=secret", acp_url);

    let (mut socket, _) = tokio_tungstenite::connect_async(&acp_url).await.expect("connect");

    // initialize
    socket
        .send(WsMessage::Text(
            json!({"id":1,"method":"initialize","params":{"protocolVersion":"1.0"}}).to_string(),
        ))
        .await
        .expect("init");
    let init_resp = socket.next().await.expect("init response").expect("frame");
    if let WsMessage::Text(text) = init_resp {
        let value: serde_json::Value = serde_json::from_str(&text).expect("json");
        assert_eq!(value.get("id").and_then(|v| v.as_i64()), Some(1));
        assert!(
            value.pointer("/result/capabilities/_meta/agent.harbor").is_some(),
            "initialize should advertise _meta.agent.harbor"
        );
        let transports = value
            .pointer("/result/capabilities/transports")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        assert!(
            transports.iter().any(|t| t == "websocket"),
            "initialize should advertise websocket transport"
        );
    } else {
        panic!("unexpected init frame: {:?}", init_resp);
    }

    // create session to trigger scenario playback
    socket
        .send(WsMessage::Text(
            json!({
                "id":2,
                "method":"session/new",
                "params":{
                    "prompt":"initialize_and_auth",
                    "agent":"sonnet"
                }
            })
            .to_string(),
        ))
        .await
        .expect("session/new");
    let created = socket.next().await.expect("created").expect("frame");
    let session_id = if let WsMessage::Text(text) = created {
        let value: serde_json::Value = serde_json::from_str(&text).expect("json");
        value.pointer("/result/sessionId").and_then(|v| v.as_str()).unwrap().to_string()
    } else {
        panic!("unexpected frame")
    };

    // expect running and completed statuses from scenario playback
    let mut saw_running = false;
    let mut saw_completed = false;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    while tokio::time::Instant::now() < deadline {
        if let Some(msg) = socket.next().await {
            let msg = msg.expect("frame");
            if let WsMessage::Text(text) = msg {
                let value: serde_json::Value = serde_json::from_str(&text).expect("json");
                if value.get("method").and_then(|v| v.as_str()) == Some("session/update") {
                    assert_eq!(
                        value.pointer("/params/sessionId").and_then(|v| v.as_str()),
                        Some(session_id.as_str())
                    );
                    if let Some(status) =
                        value.pointer("/params/event/status").and_then(|v| v.as_str())
                    {
                        if status == "running" {
                            saw_running = true;
                        }
                        if status == "completed" {
                            saw_completed = true;
                            break;
                        }
                    }
                }
            }
        }
    }

    assert!(saw_running, "should observe running status from scenario");
    assert!(
        saw_completed,
        "should observe completed status from scenario"
    );

    handle.abort();
}

#[tokio::test]
async fn acp_authenticate_rpc_uses_payload_tokens() {
    use common::acp::spawn_acp_server;

    let (acp_url, handle) = spawn_acp_server(
        |cfg| {
            cfg.acp.auth_policy = AcpAuthPolicy::Anonymous;
            cfg.api_key = Some("secret".into());
        },
        None,
    )
    .await;

    let (mut socket, _) = tokio_tungstenite::connect_async(&acp_url).await.expect("connect");

    socket
        .send(WsMessage::Text(
            json!({"id":1,"method":"initialize","params":{"protocolVersion":"1.0"}}).to_string(),
        ))
        .await
        .expect("init");
    let _ = socket.next().await;

    socket
        .send(WsMessage::Text(
            json!({"id":2,"method":"authenticate","params":{"apiKey":"secret"}}).to_string(),
        ))
        .await
        .expect("auth");

    let mut authed = false;
    while let Some(msg) = socket.next().await {
        let msg = msg.expect("frame");
        if let WsMessage::Text(text) = msg {
            let value: serde_json::Value = serde_json::from_str(&text).expect("json");
            if value.get("id").and_then(|v| v.as_i64()) == Some(2) {
                authed = value
                    .pointer("/result/authenticated")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                break;
            }
        }
    }

    assert!(authed, "authenticate RPC should accept apiKey payload");

    handle.abort();
}
