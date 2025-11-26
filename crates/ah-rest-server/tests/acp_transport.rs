// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use std::net::TcpListener;

use ah_rest_server::{Server, ServerConfig, mock_dependencies::MockServerDependencies};
use hyper::StatusCode;
use serde_json::json;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Error as WsError;

async fn spawn_acp_server(connection_limit: usize) -> (String, JoinHandle<()>) {
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
    config.acp.connection_limit = connection_limit;

    let deps = MockServerDependencies::new(config.clone()).await.expect("deps");
    let server = Server::with_state(config, deps.into_state()).await.expect("server");
    let handle = tokio::spawn(async move {
        server.run().await.expect("server run");
    });
    let acp_url = format!("ws://{}/acp/v1/connect?api_key=secret", acp_addr);
    (acp_url, handle)
}

#[tokio::test]
async fn acp_transport_smoke() {
    let (acp_url, handle) = spawn_acp_server(4).await;

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
    let (acp_url, handle) = spawn_acp_server(1).await;

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
