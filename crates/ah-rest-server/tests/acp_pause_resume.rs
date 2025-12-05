// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use std::{path::PathBuf, time::Duration};

use common::acp::{set_unique_socket_dir, spawn_acp_server_with_scenario};
use futures::{SinkExt, StreamExt};
use serde_json::json;
use serial_test::serial;
use tokio_tungstenite::tungstenite::Message as WsMessage;

mod common;

#[tokio::test]
#[serial(acp_socket)]
async fn acp_pause_resume_status_streams() {
    let _socket_dir = set_unique_socket_dir();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/acp_bridge/scenarios/pause_resume.yaml");
    let (acp_url, handle) = spawn_acp_server_with_scenario(fixture).await;
    let acp_url = format!("{}?api_key=secret", acp_url);

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
    // Give macOS/CI plenty of slack; the mock server occasionally pauses longer under load.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
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
#[serial(acp_socket)]
async fn acp_pause_and_resume_rpcs_emit_status() {
    let _socket_dir = set_unique_socket_dir();
    let (acp_url, handle) = spawn_acp_server_with_scenario(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/acp_bridge/scenarios/prompt_turn_basic.yaml"),
    )
    .await;

    let acp_url = format!("{}?api_key=secret", acp_url);

    let (mut socket, _) = tokio_tungstenite::connect_async(&acp_url).await.expect("connect");

    socket
        .send(WsMessage::Text(
            serde_json::to_string(
                &json!({"id":1,"method":"initialize","params":{"protocolVersion":"1.0"}}),
            )
            .unwrap(),
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
    // Extra slack for slower macOS CI VMs and heavier concurrent test loads.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
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
    // Allow slower macOS event delivery after resume.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
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
