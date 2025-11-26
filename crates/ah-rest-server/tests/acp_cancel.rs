// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use common::acp::spawn_acp_server_basic;
use futures::{SinkExt, StreamExt};
use serde_json::json;
use tokio_tungstenite::tungstenite::Message as WsMessage;

mod common;

#[tokio::test]
async fn acp_session_cancel_streams_update() {
    let (acp_url, handle) = spawn_acp_server_basic().await;
    let acp_url = format!("{}?api_key=secret", acp_url);
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
                    "prompt":"cancel me",
                    "agent":"sonnet"
                }
            })
            .to_string(),
        ))
        .await
        .expect("session/new");

    let mut session_id = String::new();
    while let Some(frame) = socket.next().await {
        let frame = frame.expect("frame");
        if let WsMessage::Text(text) = frame {
            let value: serde_json::Value = serde_json::from_str(&text).expect("json");
            if value.get("id").and_then(|v| v.as_i64()) == Some(2) {
                if let Some(err) = value.get("error") {
                    panic!("session/new error: {err}");
                }
                session_id = value
                    .pointer("/result/sessionId")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                break;
            }
        }
    }
    assert!(!session_id.is_empty(), "session id must be present");

    socket
        .send(WsMessage::Text(
            json!({
                "id":3,
                "method":"session/cancel",
                "params":{
                    "sessionId": session_id
                }
            })
            .to_string(),
        ))
        .await
        .expect("cancel");

    // ack
    let mut cancel_ok = false;
    while let Some(frame) = socket.next().await {
        let frame = frame.expect("frame");
        if let WsMessage::Text(text) = frame {
            let value: serde_json::Value = serde_json::from_str(&text).expect("json");
            if value.get("id").and_then(|v| v.as_i64()) == Some(3) {
                if let Some(err) = value.get("error") {
                    panic!("cancel error: {err}");
                }
                cancel_ok =
                    value.pointer("/result/cancelled").and_then(|v| v.as_bool()).unwrap_or(false);
                break;
            } else if value.get("method").and_then(|v| v.as_str()) == Some("session/update") {
                // consume stray updates before ack
                continue;
            }
        }
    }
    assert!(cancel_ok, "cancel ack missing");

    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    let mut saw_cancel = false;
    let _ = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while let Some(msg) = socket.next().await {
            if let WsMessage::Text(text) = msg.expect("frame") {
                let value: serde_json::Value = serde_json::from_str(&text).expect("json");
                if value.get("method").and_then(|v| v.as_str()) == Some("session/update") {
                    if value.pointer("/params/event/status").and_then(|v| v.as_str())
                        == Some("cancelled")
                    {
                        saw_cancel = true;
                        break;
                    }
                }
            }
        }
    })
    .await;

    assert!(saw_cancel, "should emit cancelled status");
    handle.abort();
}
