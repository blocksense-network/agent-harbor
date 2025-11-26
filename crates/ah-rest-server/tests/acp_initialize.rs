// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use ah_rest_server::acp::translator::{AcpCapabilities, JsonRpcTranslator};
use ah_rest_server::config::{AcpConfig, AcpTransportMode};
use ah_rest_server::{Server, ServerConfig, mock_dependencies::{MockServerDependencies, ScenarioPlaybackOptions}};
use futures::{SinkExt, StreamExt};
use proptest::prelude::*;
use serde_json::json;
use std::net::TcpListener;
use std::path::PathBuf;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message as WsMessage;

#[test]
fn acp_initialize_caps_roundtrip() {
    let mut cfg = AcpConfig::default();
    cfg.transport = AcpTransportMode::WebSocket;
    let caps = JsonRpcTranslator::negotiate_caps(&cfg);
    assert_eq!(
        caps,
        AcpCapabilities {
            transports: vec!["websocket".into()],
            fs_read: false,
            fs_write: false,
            terminals: true
        }
    );

    let payload = JsonRpcTranslator::initialize_response(&caps);
    assert_eq!(payload["capabilities"]["transports"][0], "websocket");
    assert_eq!(payload["capabilities"]["filesystem"]["readTextFile"], false);

    // Unknown flags should be ignored
    let noisy = json!({
        "capabilities": {
            "filesystem": {
                "readTextFile": true,
                "unknownFlag": true
            },
            "terminal": true,
            "transports": ["websocket", "stdio"],
            "extra": {"foo":"bar"}
        }
    });
    let parsed = JsonRpcTranslator::ignore_unknown_caps(&noisy);
    assert_eq!(parsed.transports, vec!["websocket", "stdio"]);
    assert!(parsed.fs_read);
    assert!(parsed.terminals);
}

proptest! {
    #[test]
    fn unknown_capabilities_are_ignored_but_known_fields_respected(fs_read in proptest::bool::ANY, fs_write in proptest::bool::ANY) {
        let noisy = json!({
            "capabilities": {
                "filesystem": {
                    "readTextFile": fs_read,
                    "writeTextFile": fs_write,
                    "someFutureFlag": true
                },
                "terminal": true,
                "transports": ["websocket"],
                "experimental": {"foo": "bar"}
            }
        });
        let parsed = JsonRpcTranslator::ignore_unknown_caps(&noisy);
        prop_assert_eq!(parsed.fs_read, fs_read);
        prop_assert_eq!(parsed.fs_write, fs_write);
        prop_assert!(parsed.transports.contains(&"websocket".into()));
    }
}

async fn spawn_acp_server_with_scenario(fixture: PathBuf) -> (String, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let acp_listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let acp_addr = acp_listener.local_addr().unwrap();
    drop(acp_listener);

    let mut config = ServerConfig::default();
    config.bind_addr = addr;
    config.enable_cors = true;
    config.api_key = Some("secret".into());
    config.acp.enabled = true;
    config.acp.bind_addr = acp_addr;

    let deps = MockServerDependencies::with_options(
        config.clone(),
        ScenarioPlaybackOptions {
            scenario_files: vec![fixture],
            speed_multiplier: 0.1,
        },
    )
    .await
    .expect("deps");
    let server = Server::with_state(config, deps.into_state()).await.expect("server");
    let handle = tokio::spawn(async move { server.run().await.expect("server run") });
    let acp_url = format!("ws://{}/acp/v1/connect?api_key=secret", acp_addr);
    (acp_url, handle)
}

#[tokio::test]
async fn acp_initialize_and_auth_scenario_succeeds() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/acp_bridge/scenarios/initialize_and_auth.yaml");
    let (acp_url, handle) = spawn_acp_server_with_scenario(fixture).await;

    let (mut socket, _) = tokio_tungstenite::connect_async(&acp_url)
        .await
        .expect("connect");

    // initialize
    socket
        .send(WsMessage::Text(
            json!({"id":1,"method":"initialize","params":{}}).to_string(),
        ))
        .await
        .expect("init");
    let _ = socket.next().await;

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
                    if let Some(status) = value.pointer("/params/event/status").and_then(|v| v.as_str()) {
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
    assert!(saw_completed, "should observe completed status from scenario");

    handle.abort();
}
