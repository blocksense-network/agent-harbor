// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use base64::Engine;
use common::acp::spawn_acp_server_basic;
use futures::{SinkExt, StreamExt};
use serde_json::{Value, json};
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
async fn terminal_follow_and_write_roundtrip() {
    let (acp_url, handle) = spawn_acp_server_basic().await;
    let acp_url = format!("{}?api_key=secret", acp_url);

    let (mut socket, _) = tokio_tungstenite::connect_async(&acp_url).await.expect("connect");

    socket
        .send(WsMessage::Text(
            json!({"id":1,"method":"initialize","params":{"protocolVersion":"1.0"}}).to_string(),
        ))
        .await
        .expect("send init");
    let _ = socket.next().await;

    socket
        .send(WsMessage::Text(
            json!({"id":2,"method":"session/new","params":{"prompt":"tty","agent":"sonnet"}})
                .to_string(),
        ))
        .await
        .expect("send session/new");
    let created = read_response(&mut socket, 2).await;
    let session_id = created
        .pointer("/result/sessionId")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();

    // Issue follow request and capture response/update regardless of ordering
    socket
        .send(WsMessage::Text(
            json!({
                "id":3,
                "method":"_ah/terminal/follow",
                "params":{
                    "sessionId":session_id,
                    "executionId":"exec-1",
                    "command":"echo hi"
                }
            })
            .to_string(),
        ))
        .await
        .expect("send follow");

    let mut follow_res: Option<Value> = None;
    let mut follow_update: Option<Value> = None;
    while follow_res.is_none() || follow_update.is_none() {
        if let Some(msg) = socket.next().await {
            let msg = msg.expect("frame");
            if let WsMessage::Text(text) = msg {
                let v: Value = serde_json::from_str(&text).unwrap();
                if v.get("id").and_then(|v| v.as_i64()) == Some(3) {
                    follow_res = Some(v);
                    continue;
                }
                if v.pointer("/params/event/type").and_then(|v| v.as_str())
                    == Some("terminal_follow")
                {
                    follow_update = Some(v);
                    continue;
                }
            }
        }
    }
    let v = follow_res.unwrap();
    assert!(
        v.pointer("/result/command")
            .and_then(|v| v.as_str())
            .unwrap()
            .contains("show-sandbox-execution")
    );
    let follow_update = follow_update.unwrap();
    assert_eq!(
        follow_update.pointer("/params/event/executionId").and_then(|v| v.as_str()),
        Some("exec-1")
    );

    // Send a terminal write and expect immediate ack
    socket
        .send(WsMessage::Text(
            json!({
                "id":4,
                "method":"_ah/terminal/write",
                "params":{
                    "sessionId":session_id,
                    "data": base64::engine::general_purpose::STANDARD.encode("ok")
                }
            })
            .to_string(),
        ))
        .await
        .expect("send write");
    let v = read_response(&mut socket, 4).await;
    assert_eq!(
        v.pointer("/result/accepted").and_then(|v| v.as_bool()),
        Some(true)
    );

    // Detach and ensure both response and update are observed
    socket
        .send(WsMessage::Text(
            json!({
                "id":5,
                "method":"_ah/terminal/detach",
                "params":{
                    "sessionId":session_id,
                    "executionId":"exec-1"
                }
            })
            .to_string(),
        ))
        .await
        .expect("send detach");

    let mut detach_res: Option<Value> = None;
    let mut detach_update: Option<Value> = None;
    while detach_res.is_none() || detach_update.is_none() {
        if let Some(msg) = socket.next().await {
            let msg = msg.expect("frame");
            if let WsMessage::Text(text) = msg {
                let val: Value = serde_json::from_str(&text).unwrap();
                if val.get("id").and_then(|v| v.as_i64()) == Some(5) {
                    detach_res = Some(val);
                    continue;
                }
                if val.pointer("/params/event/type").and_then(|v| v.as_str())
                    == Some("terminal_detach")
                {
                    detach_update = Some(val);
                    continue;
                }
            }
        }
    }
    let v = detach_res.unwrap();
    assert_eq!(
        v.pointer("/result/detached").and_then(|v| v.as_bool()),
        Some(true)
    );
    let detach_update = detach_update.unwrap();
    assert_eq!(
        detach_update.pointer("/params/event/executionId").and_then(|v| v.as_str()),
        Some("exec-1")
    );

    handle.abort();
}
