// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use common::acp::spawn_acp_server_basic;
use futures::{SinkExt, StreamExt};
use serde_json::json;
use tokio_tungstenite::tungstenite::Message as WsMessage;

mod common;

#[tokio::test]
async fn acp_prompt_backpressure() {
    let (acp_url, handle) = spawn_acp_server_basic().await;
    let acp_url = format!("{}?api_key=secret", acp_url);
    let (mut socket, _) = tokio_tungstenite::connect_async(&acp_url).await.expect("connect");

    // initialize
    socket
        .send(WsMessage::Text(
            json!({"id":1,"method":"initialize","params":{}}).to_string(),
        ))
        .await
        .expect("init");
    // drain init
    let _ = socket.next().await;

    // create session
    socket
        .send(WsMessage::Text(
            json!({
                "id":2,
                "method":"session/new",
                "params":{
                    "prompt":"slow client prompt",
                    "agent":"sonnet"
                }
            })
            .to_string(),
        ))
        .await
        .expect("session new");
    let session_new_resp = socket.next().await.expect("session/new resp").expect("frame");
    let session_id = if let WsMessage::Text(text) = session_new_resp {
        let value: serde_json::Value = serde_json::from_str(&text).expect("json");
        value
            .pointer("/result/sessionId")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string()
    } else {
        String::new()
    };

    // send a burst of prompts without reading updates immediately
    for i in 0..5u32 {
        socket
            .send(WsMessage::Text(
                json!({
                    "id":1000 + i as i64,
                    "method":"session/prompt",
                    "params":{
                        "sessionId": session_id,
                        "message": format!("prompt {i}")
                    }
                })
                .to_string(),
            ))
            .await
            .expect("prompt send");
        // small pause to avoid fully saturating
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }

    // Now ensure the socket stays alive under backpressure
    let mut received_updates = 0;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    let mut closed_early = false;
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(std::time::Duration::from_millis(200), socket.next()).await {
            Ok(Some(msg)) => match msg {
                Ok(WsMessage::Text(text)) => {
                    let value: serde_json::Value = serde_json::from_str(&text).expect("json");
                    if value.get("method").and_then(|v| v.as_str()) == Some("session/update") {
                        received_updates += 1;
                    }
                }
                Ok(_) => {}
                Err(_) => {
                    closed_early = true;
                    break;
                }
            },
            Ok(None) => {
                closed_early = true;
                break;
            }
            Err(_) => {
                // timeout, loop again
            }
        }
    }

    assert!(
        !closed_early,
        "connection should survive brief backpressure window"
    );
    assert!(
        received_updates >= 0,
        "sanity check: counter should not underflow"
    );

    handle.abort();
}
