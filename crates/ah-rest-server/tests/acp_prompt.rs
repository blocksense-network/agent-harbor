// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use common::acp::spawn_acp_server_basic;
use futures::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message as WsMessage;

mod common;

async fn read_response(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    target_id: i64,
) -> Value {
    while let Some(msg) = socket.next().await {
        let msg = msg.expect("ws frame");
        if let WsMessage::Text(text) = msg {
            let value: Value = serde_json::from_str(&text).expect("json");
            if value.get("id").and_then(|v| v.as_i64()) == Some(target_id) {
                return value;
            }
        }
    }
    panic!("response with id {} not received", target_id);
}

#[tokio::test]
async fn acp_prompt_round_trip() {
    let (acp_url, handle) = spawn_acp_server_basic().await;
    let acp_url = format!("{}?api_key=secret", acp_url);

    let (mut socket, _) = tokio_tungstenite::connect_async(&acp_url).await.expect("connect");

    socket
        .send(WsMessage::Text(
            json!({"id":1,"method":"initialize","params":{"protocolVersion":"1.0"}}).to_string(),
        ))
        .await
        .expect("send initialize");
    read_response(&mut socket, 1).await;

    socket
        .send(WsMessage::Text(
            json!({
                "id":2,
                "method":"session/new",
                "params":{
                    "prompt":"wire prompt handling",
                    "agent":"sonnet"
                }
            })
            .to_string(),
        ))
        .await
        .expect("send session/new");
    let created = read_response(&mut socket, 2).await;
    let session_id = created
        .pointer("/result/sessionId")
        .and_then(|v| v.as_str())
        .expect("session id")
        .to_string();

    socket
        .send(WsMessage::Text(
            json!({
                "id":3,
                "method":"session/prompt",
                "params":{
                    "sessionId":session_id,
                    "message":"please run the tests"
                }
            })
            .to_string(),
        ))
        .await
        .expect("send session/prompt");

    let prompt_ack = read_response(&mut socket, 3).await;
    assert_eq!(
        prompt_ack.pointer("/result/accepted").and_then(|v| v.as_bool()),
        Some(true)
    );

    // Expect the log event to flow back via session/update
    let update = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if let Some(msg) = socket.next().await {
                let msg = msg.expect("frame");
                if let WsMessage::Text(text) = msg {
                    let value: Value = serde_json::from_str(&text).expect("json");
                    if value.get("method").and_then(|v| v.as_str()) == Some("session/update") {
                        if value.pointer("/params/event/type").and_then(|v| v.as_str())
                            == Some("log")
                        {
                            if let Some(message) =
                                value.pointer("/params/event/message").and_then(|v| v.as_str())
                            {
                                if message.contains("please run the tests") {
                                    return value;
                                }
                            }
                        }
                    }
                }
            }
        }
    })
    .await
    .expect("session/update timeout");

    assert_eq!(
        update.pointer("/params/event/type").and_then(|v| v.as_str()),
        Some("log")
    );

    handle.abort();
}

#[tokio::test]
async fn acp_prompt_rejects_on_context_limit() {
    let (acp_url, handle) = spawn_acp_server_basic().await;
    let acp_url = format!("{}?api_key=secret", acp_url);

    let (mut socket, _) = tokio_tungstenite::connect_async(&acp_url).await.expect("connect");

    socket
        .send(WsMessage::Text(
            json!({"id":1,"method":"initialize","params":{"protocolVersion":"1.0"}}).to_string(),
        ))
        .await
        .expect("send initialize");
    read_response(&mut socket, 1).await;

    let base_prompt = "A".repeat(15_000);
    socket
        .send(WsMessage::Text(
            json!({
                "id":2,
                "method":"session/new",
                "params":{
                    "prompt": base_prompt,
                    "agent":"sonnet"
                }
            })
            .to_string(),
        ))
        .await
        .expect("send session/new");
    let created = read_response(&mut socket, 2).await;
    let session_id = created
        .pointer("/result/sessionId")
        .and_then(|v| v.as_str())
        .expect("session id")
        .to_string();

    let second_turn = "second-turn-too-long".repeat(80); // ~1.9k chars
    socket
        .send(WsMessage::Text(
            json!({
                "id":3,
                "method":"session/prompt",
                "params":{
                    "sessionId": session_id,
                    "message": second_turn
                }
            })
            .to_string(),
        ))
        .await
        .expect("send session/prompt");

    let prompt_ack = read_response(&mut socket, 3).await;
    assert_eq!(
        prompt_ack.pointer("/result/accepted").and_then(|v| v.as_bool()),
        Some(false)
    );
    assert_eq!(
        prompt_ack.pointer("/result/stopReason").and_then(|v| v.as_str()),
        Some("context_limit")
    );
    let limit = prompt_ack
        .pointer("/result/limitChars")
        .and_then(|v| v.as_u64())
        .expect("limitChars");
    let used = prompt_ack
        .pointer("/result/usedChars")
        .and_then(|v| v.as_u64())
        .expect("usedChars");
    assert!(used > limit, "rejected prompt should exceed context budget");

    // ensure the rejected prompt did not get echoed back as a log event
    let echoed = tokio::time::timeout(Duration::from_millis(300), async {
        loop {
            if let Some(msg) = socket.next().await {
                let msg = msg.expect("frame");
                if let WsMessage::Text(text) = msg {
                    let value: Value = serde_json::from_str(&text).expect("json");
                    if value.get("method").and_then(|v| v.as_str()) == Some("session/update") {
                        if value.pointer("/params/event/type").and_then(|v| v.as_str())
                            == Some("log")
                        {
                            if let Some(message) =
                                value.pointer("/params/event/message").and_then(|v| v.as_str())
                            {
                                if message.contains("second-turn-too-long") {
                                    return Some(value);
                                }
                            }
                        }
                    }
                }
            } else {
                return None;
            }
        }
    })
    .await;
    assert!(
        echoed.is_err(),
        "rejected prompt should not be recorded as a session/update log"
    );

    handle.abort();
}
