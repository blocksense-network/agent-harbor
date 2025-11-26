// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

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
async fn acp_session_list_respects_pagination() {
    let (acp_url, handle) = spawn_acp_server_basic().await;
    let acp_url = format!("{}?api_key=secret", acp_url);

    let (mut socket, _) = tokio_tungstenite::connect_async(&acp_url)
        .await
        .expect("connect");

    // initialize
    socket
        .send(WsMessage::Text(
            json!({"id":1,"method":"initialize","params":{"protocolVersion":"1.0"}})
                .to_string(),
        ))
        .await
        .unwrap();
    let _ = read_response(&mut socket, 1).await;

    // create three sessions so pagination has data to slice
    for i in 0..3 {
        socket
            .send(WsMessage::Text(
                json!({
                    "id": 10 + i,
                    "method": "session/new",
                    "params": {"prompt": format!("session-{i}"), "agent": "sonnet"}
                })
                .to_string(),
            ))
            .await
            .unwrap();
        let _ = read_response(&mut socket, 10 + i as i64).await;
    }

    // list first two
    socket
        .send(WsMessage::Text(
            json!({"id":20,"method":"session/list","params":{"offset":0,"limit":2}})
                .to_string(),
        ))
        .await
        .unwrap();
    let first_page = read_response(&mut socket, 20).await;
    let first_items = first_page
        .pointer("/result/items")
        .and_then(|v| v.as_array())
        .expect("items array");
    assert_eq!(first_items.len(), 2);
    assert_eq!(first_page.pointer("/result/offset").and_then(|v| v.as_u64()), Some(0));
    assert_eq!(first_page.pointer("/result/limit").and_then(|v| v.as_u64()), Some(2));
    assert_eq!(first_page.pointer("/result/total").and_then(|v| v.as_u64()), Some(3));

    // list last item via offset
    socket
        .send(WsMessage::Text(
            json!({"id":21,"method":"session/list","params":{"offset":2,"limit":2}})
                .to_string(),
        ))
        .await
        .unwrap();
    let second_page = read_response(&mut socket, 21).await;
    let second_items = second_page
        .pointer("/result/items")
        .and_then(|v| v.as_array())
        .expect("items array");
    assert_eq!(second_items.len(), 1);
    assert_eq!(second_page.pointer("/result/offset").and_then(|v| v.as_u64()), Some(2));
    assert_eq!(second_page.pointer("/result/limit").and_then(|v| v.as_u64()), Some(2));

    handle.abort();
}
