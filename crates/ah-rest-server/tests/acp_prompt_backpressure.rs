// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use std::net::TcpListener;

use ah_rest_server::{Server, ServerConfig, mock_dependencies::MockServerDependencies};
use futures::{SinkExt, StreamExt};
use serde_json::json;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message as WsMessage;

async fn spawn_acp_server() -> (String, JoinHandle<()>) {
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

    let deps = MockServerDependencies::new(config.clone()).await.expect("deps");
    let server = Server::with_state(config, deps.into_state()).await.expect("server");
    let handle = tokio::spawn(async move {
        server.run().await.expect("server run");
    });
    let acp_url = format!("ws://{}/acp/v1/connect?api_key=secret", acp_addr);
    (acp_url, handle)
}

#[tokio::test]
async fn acp_prompt_backpressure() {
    let (acp_url, handle) = spawn_acp_server().await;
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
    let session_new_resp = socket
        .next()
        .await
        .expect("session/new resp")
        .expect("frame");
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

    // send many prompts without reading updates immediately
    for i in 0..20u32 {
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

    // Now read a few messages to ensure the socket is still alive
    let mut received_updates = 0;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    while tokio::time::Instant::now() < deadline {
        if let Some(msg) = socket.next().await {
            if let WsMessage::Text(text) = msg.expect("frame") {
                let value: serde_json::Value = serde_json::from_str(&text).expect("json");
                if value.get("method").and_then(|v| v.as_str()) == Some("session/update") {
                    received_updates += 1;
                    if received_updates > 2 {
                        break;
                    }
                }
            }
        }
    }

    assert!(received_updates > 0, "expected some updates despite slow consumption");

    handle.abort();
}
