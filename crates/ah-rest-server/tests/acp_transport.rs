// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use common::acp::spawn_acp_server_with_limit;
use hyper::StatusCode;
use serde_json::json;
use tokio_tungstenite::tungstenite::Error as WsError;

mod common;

#[tokio::test]
async fn acp_transport_smoke() {
    let (acp_url, handle) = spawn_acp_server_with_limit(4).await;
    let acp_url = format!("{}?api_key=secret", acp_url);

    let request = json!({
        "id": 1,
        "method": "ping",
        "params": { "echo": "hello" }
    });
    let response = ah_rest_server::acp::transport::ws_echo(&acp_url, request.clone())
        .await
        .expect("ws round trip");

    assert_eq!(response.get("id").and_then(|v| v.as_i64()), Some(1));
    assert_eq!(
        response.get("result").and_then(|v| v.get("echo")).and_then(|v| v.as_str()),
        Some("hello")
    );

    handle.abort();
}

#[tokio::test]
async fn acp_rate_limit() {
    let (acp_url, handle) = spawn_acp_server_with_limit(1).await;
    let acp_url = format!("{}?api_key=secret", acp_url);

    // First connection succeeds and holds the permit.
    let (mut socket1, _) = tokio_tungstenite::connect_async(&acp_url).await.expect("first connect");

    // Second connection should hit rate limit.
    let err = tokio_tungstenite::connect_async(&acp_url).await.unwrap_err();
    match err {
        WsError::Http(response) => {
            assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        }
        other => panic!("expected HTTP 429, got {other:?}"),
    }

    let _ = socket1.close(None).await;
    handle.abort();
}
