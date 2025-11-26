// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use std::net::TcpListener;

use ah_rest_server::{Server, ServerConfig, mock_dependencies::MockServerDependencies};
use serde_json::{Value, json};
use futures::{SinkExt, StreamExt};
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

async fn read_response(
    socket: &mut tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
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
    panic!("response with id {target_id} not received");
}

#[tokio::test]
async fn acp_session_catalog_end_to_end() {
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
        .expect("send initialize");
    let init = read_response(&mut socket, 1).await;
    assert_eq!(
        init.pointer("/result/capabilities/transports/0")
            .and_then(|v| v.as_str()),
        Some("websocket")
    );

    // create a session
    socket
        .send(WsMessage::Text(
            json!({
                "id":2,
                "method":"session/new",
                "params":{
                    "prompt":"Write unit tests for ACP gateway",
                    "agent":"sonnet",
                    "labels": {"suite":"acp"},
                    "repoUrl": "https://example.com/mock.git",
                    "branch":"main"
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
    assert_eq!(
        created.pointer("/result/status").and_then(|v| v.as_str()),
        Some("queued")
    );

    // Expect at least one session/update notification
    let update = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if let Some(msg) = socket.next().await {
                let msg = msg.expect("frame");
                if let WsMessage::Text(text) = msg {
                    let value: Value = serde_json::from_str(&text).expect("json");
                    if value.get("method").and_then(|v| v.as_str()) == Some("session/update") {
                        return value;
                    }
                }
            }
        }
    })
    .await
    .expect("session/update timeout");
    assert_eq!(
        update
            .pointer("/params/sessionId")
            .and_then(|v| v.as_str()),
        Some(session_id.as_str())
    );

    // list sessions
    socket
        .send(WsMessage::Text(
            json!({"id":3,"method":"session/list","params":{}}).to_string(),
        ))
        .await
        .expect("send session/list");
    let list = read_response(&mut socket, 3).await;
    let items = list
        .pointer("/result/items")
        .and_then(|v| v.as_array())
        .expect("items");
    assert!(
        items.iter()
            .any(|item| item.get("id").and_then(|v| v.as_str()) == Some(session_id.as_str())),
        "session list should contain created session"
    );

    // load session
    socket
        .send(WsMessage::Text(
            json!({"id":4,"method":"session/load","params":{"sessionId":session_id}})
                .to_string(),
        ))
        .await
        .expect("send session/load");
    let loaded = read_response(&mut socket, 4).await;
    assert_eq!(
        loaded
            .pointer("/result/session/id")
            .and_then(|v| v.as_str()),
        Some(items[0].get("id").and_then(|v| v.as_str()).unwrap())
    );

    handle.abort();
}

#[tokio::test]
async fn acp_session_new_infers_tenant_from_jwt() {
    use tungstenite::client::IntoClientRequest;
    use jsonwebtoken::{EncodingKey, Header};
    use ah_rest_server::auth::Claims;

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let acp_listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let acp_addr = acp_listener.local_addr().unwrap();
    drop(acp_listener);

    let mut config = ServerConfig::default();
    config.bind_addr = addr;
    config.enable_cors = true;
    config.jwt_secret = Some("secret".into());
    config.acp.enabled = true;
    config.acp.bind_addr = acp_addr;

    let deps = MockServerDependencies::new(config.clone()).await.expect("deps");
    let server = Server::with_state(config, deps.into_state()).await.expect("server");
    let handle = tokio::spawn(async move { server.run().await.expect("server run") });
    let acp_url = format!("ws://{}/acp/v1/connect", acp_addr);

    let claims = Claims {
        sub: "user-1".into(),
        exp: (chrono::Utc::now().timestamp() + 300) as usize,
        tenant_id: Some("tenant-xyz".into()),
        project_id: Some("proj-99".into()),
        roles: vec!["dev".into()],
    };
    let token = jsonwebtoken::encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret("secret".as_ref()),
    )
    .expect("jwt");

    let mut request = acp_url.into_client_request().unwrap();
    request
        .headers_mut()
        .insert("Authorization", format!("Bearer {}", token).parse().unwrap());

    let (mut socket, _) = tokio_tungstenite::connect_async(request)
        .await
        .expect("connect");

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
                    "prompt":"tenant inference",
                    "agent":"sonnet"
                }
            })
            .to_string(),
        ))
        .await
        .expect("session/new");
    let created = read_response(&mut socket, 2).await;
    assert_eq!(
        created.pointer("/result/tenantId").and_then(|v| v.as_str()),
        Some("tenant-xyz")
    );
    assert_eq!(
        created.pointer("/result/projectId").and_then(|v| v.as_str()),
        Some("proj-99")
    );

    handle.abort();
}

#[tokio::test]
async fn acp_session_new_respects_context_limit() {
    let (acp_url, handle) = spawn_acp_server().await;

    let (mut socket, _) = tokio_tungstenite::connect_async(&acp_url)
        .await
        .expect("connect");

    socket
        .send(WsMessage::Text(
            json!({"id":1,"method":"initialize","params":{}}).to_string(),
        ))
        .await
        .expect("send initialize");
    let _ = read_response(&mut socket, 1).await;

    let long_prompt = "Z".repeat(20_000);
    socket
        .send(WsMessage::Text(
            json!({
                "id":2,
                "method":"session/new",
                "params":{
                    "prompt": long_prompt,
                    "agent":"sonnet"
                }
            })
            .to_string(),
        ))
        .await
        .expect("send session/new");
    let response = read_response(&mut socket, 2).await;

    assert_eq!(
        response.pointer("/result/accepted").and_then(|v| v.as_bool()),
        Some(false)
    );
    assert_eq!(
        response.pointer("/result/stopReason").and_then(|v| v.as_str()),
        Some("context_limit")
    );
    assert!(
        response.pointer("/result/usedChars").and_then(|v| v.as_u64()).unwrap_or(0)
            > response.pointer("/result/limitChars").and_then(|v| v.as_u64()).unwrap_or(16_000),
        "usedChars should exceed limit when rejection occurs"
    );

    // ensure no session/update arrives for a rejected creation
    let maybe_update = tokio::time::timeout(std::time::Duration::from_millis(300), socket.next()).await;
    assert!(maybe_update.is_err(), "no session/update should be emitted for rejected session/new");

    handle.abort();
}
