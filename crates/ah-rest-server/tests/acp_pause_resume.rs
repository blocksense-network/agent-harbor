// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use std::{net::TcpListener, path::PathBuf, time::Duration};

use ah_rest_server::{
    Server, ServerConfig,
    mock_dependencies::{MockServerDependencies, ScenarioPlaybackOptions},
};
use futures::{SinkExt, StreamExt};
use serde_json::json;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message as WsMessage;

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
            speed_multiplier: 0.2,
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
async fn acp_pause_resume_status_streams() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/acp_bridge/scenarios/pause_resume.yaml");
    let (acp_url, handle) = spawn_acp_server_with_scenario(fixture).await;

    let (mut socket, _) = tokio_tungstenite::connect_async(&acp_url).await.expect("connect");

    socket
        .send(WsMessage::Text(
            json!({"id":1,"method":"initialize","params":{}}).to_string(),
        ))
        .await
        .expect("init");
    let _ = socket.next().await;

    socket
        .send(WsMessage::Text(
            json!({
                "id":2,
                "method":"session/new",
                "params":{
                    "prompt":"pause_resume",
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
        panic!("unexpected frame");
    };

    let mut saw_paused = false;
    let mut saw_completed = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
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
                        match status {
                            "paused" => saw_paused = true,
                            "completed" => {
                                saw_completed = true;
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    assert!(saw_paused, "should stream paused status during timeline");
    assert!(saw_completed, "should reach completed status after resume");

    handle.abort();
}

#[tokio::test]
async fn acp_pause_and_resume_rpcs_emit_status() {
    let (acp_url, handle) = spawn_acp_server_with_scenario(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/acp_bridge/scenarios/prompt_turn_basic.yaml"),
    )
    .await;

    let (mut socket, _) = tokio_tungstenite::connect_async(&acp_url).await.expect("connect");

    socket
        .send(WsMessage::Text(
            serde_json::to_string(&json!({"id":1,"method":"initialize","params":{}})).unwrap(),
        ))
        .await
        .expect("init");
    let _ = socket.next().await;

    socket
        .send(WsMessage::Text(
            serde_json::to_string(&json!({
                "id":2,
                "method":"session/new",
                "params":{"prompt":"pause rpc","agent":"sonnet"}
            }))
            .unwrap(),
        ))
        .await
        .expect("session/new");
    let created = socket.next().await.expect("created").expect("frame");
    let session_id = if let WsMessage::Text(text) = created {
        let value: serde_json::Value = serde_json::from_str(&text).expect("json");
        value.pointer("/result/sessionId").and_then(|v| v.as_str()).unwrap().to_string()
    } else {
        panic!("unexpected frame");
    };

    socket
        .send(WsMessage::Text(
            serde_json::to_string(&json!({
                "id":3,
                "method":"session/pause",
                "params":{"sessionId":session_id}
            }))
            .unwrap(),
        ))
        .await
        .expect("pause");

    // drain frames until we see the pause response
    loop {
        if let Some(msg) = socket.next().await {
            let msg = msg.expect("frame");
            if let WsMessage::Text(text) = msg {
                let value: serde_json::Value = serde_json::from_str(&text).expect("json");
                if value.get("id").and_then(|v| v.as_i64()) == Some(3) {
                    assert_eq!(
                        value.pointer("/result/status").and_then(|v| v.as_str()),
                        Some("paused")
                    );
                    break;
                }
            }
        }
    }

    // Expect a paused status event
    let mut saw_paused = false;
    let mut saw_running = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while tokio::time::Instant::now() < deadline {
        if let Some(msg) = socket.next().await {
            let msg = msg.expect("frame");
            if let WsMessage::Text(text) = msg {
                let value: serde_json::Value = serde_json::from_str(&text).expect("json");
                if value.get("method").and_then(|v| v.as_str()) == Some("session/update") {
                    if let Some(status) =
                        value.pointer("/params/event/status").and_then(|v| v.as_str())
                    {
                        if status == "paused" {
                            saw_paused = true;
                            break;
                        }
                    }
                }
            }
        }
    }
    assert!(saw_paused, "session/update should include paused status");

    socket
        .send(WsMessage::Text(
            serde_json::to_string(&json!({
                "id":4,
                "method":"session/resume",
                "params":{"sessionId":session_id}
            }))
            .unwrap(),
        ))
        .await
        .expect("resume");

    loop {
        if let Some(msg) = socket.next().await {
            let msg = msg.expect("frame");
            if let WsMessage::Text(text) = msg {
                let value: serde_json::Value = serde_json::from_str(&text).expect("json");
                if value.get("id").and_then(|v| v.as_i64()) == Some(4) {
                    assert_eq!(
                        value.pointer("/result/status").and_then(|v| v.as_str()),
                        Some("running")
                    );
                    break;
                }
            }
        }
    }

    // Expect running status again
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while tokio::time::Instant::now() < deadline {
        if let Some(msg) = socket.next().await {
            let msg = msg.expect("frame");
            if let WsMessage::Text(text) = msg {
                let value: serde_json::Value = serde_json::from_str(&text).expect("json");
                if value.get("method").and_then(|v| v.as_str()) == Some("session/update") {
                    if let Some(status) =
                        value.pointer("/params/event/status").and_then(|v| v.as_str())
                    {
                        if status == "running" {
                            saw_running = true;
                            break;
                        }
                    }
                }
            }
        }
    }

    assert!(
        saw_running,
        "session/update should include running status after resume"
    );

    handle.abort();
}
