// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use std::net::TcpListener;

use ah_rest_server::{
    auth::Claims, Server, ServerConfig,
    mock_dependencies::MockServerDependencies,
};
use futures::{SinkExt, StreamExt};
use jsonwebtoken::{EncodingKey, Header};
use serde_json::json;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use url::form_urlencoded;

async fn read_response(
    socket: &mut tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    target_id: i64,
) -> serde_json::Value {
    while let Some(msg) = socket.next().await {
        let msg = msg.expect("ws frame");
        if let WsMessage::Text(text) = msg {
            let value: serde_json::Value = serde_json::from_str(&text).expect("json");
            if value.get("id").and_then(|v| v.as_i64()) == Some(target_id) {
                return value;
            }
        }
    }
    panic!("response with id {} not received", target_id);
}

async fn spawn_acp_server(configure: impl Fn(&mut ServerConfig)) -> (String, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let acp_listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let acp_addr = acp_listener.local_addr().unwrap();
    drop(acp_listener);

    let mut config = ServerConfig::default();
    config.bind_addr = addr;
    config.enable_cors = true;
    config.acp.enabled = true;
    config.acp.bind_addr = acp_addr;
    configure(&mut config);

    let deps = MockServerDependencies::new(config.clone()).await.expect("deps");
    let server = Server::with_state(config, deps.into_state()).await.expect("server");
    let handle = tokio::spawn(async move { server.run().await.expect("server run") });
    let acp_url = format!("ws://{}/acp/v1/connect", acp_addr);
    (acp_url, handle)
}

#[tokio::test]
async fn acp_session_list_scopes_to_jwt_tenant() {
    let jwt_secret = "scope-secret";
    let (acp_url, handle) = spawn_acp_server(|cfg| {
        cfg.jwt_secret = Some(jwt_secret.to_string());
        cfg.api_key = Some("secret".into());
    })
    .await;

    // helper to connect with optional bearer
    let connect_with_bearer = |token: Option<String>| async {
        if let Some(tok) = token {
            let encoded: String = form_urlencoded::byte_serialize(tok.as_bytes()).collect();
            let url = format!("{acp_url}?token={encoded}&api_key=secret");
            tokio_tungstenite::connect_async(url)
                .await
                .expect("connect")
                .0
        } else {
            let url = format!("{acp_url}?api_key=secret");
            tokio_tungstenite::connect_async(url)
                .await
                .expect("connect")
                .0
        }
    };

    // anonymous creates session with no tenant
    let mut socket1 = connect_with_bearer(None).await;
    socket1
        .send(WsMessage::Text(json!({"id":1,"method":"initialize","params":{}}).to_string()))
        .await
        .unwrap();
    let _ = socket1.next().await;
    socket1
        .send(WsMessage::Text(
            json!({"id":2,"method":"session/new","params":{"prompt":"anon","agent":"sonnet"}})
                .to_string(),
        ))
        .await
        .unwrap();
    let _ = socket1.next().await;
    socket1.close(None).await.unwrap();

    // bearer with tenant creates scoped session
    let claims = Claims {
        sub: "user-1".into(),
        exp: (chrono::Utc::now().timestamp() + 300) as usize,
        tenant_id: Some("tenant-1".into()),
        project_id: Some("proj-1".into()),
        roles: vec![],
    };
    let token = jsonwebtoken::encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(jwt_secret.as_ref()),
    )
    .unwrap();
    let mut socket2 = connect_with_bearer(Some(token)).await;
    socket2
        .send(WsMessage::Text(json!({"id":10,"method":"initialize","params":{}}).to_string()))
        .await
        .unwrap();
    let _ = socket2.next().await;
    socket2
        .send(WsMessage::Text(
            json!({"id":11,"method":"session/new","params":{"prompt":"tenant scoped","agent":"sonnet"}})
                .to_string(),
        ))
        .await
        .unwrap();
    let created = read_response(&mut socket2, 11).await;
    let tenant_session_id = created
        .pointer("/result/sessionId")
        .and_then(|v| v.as_str())
        .expect("session id")
        .to_string();

    // list with bearer should only return tenant-1 session
    socket2
        .send(WsMessage::Text(
            json!({"id":12,"method":"session/list","params":{}}).to_string(),
        ))
        .await
        .unwrap();
    let list = socket2.next().await.unwrap().unwrap();
    if let WsMessage::Text(text) = list {
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        let items = value
            .pointer("/result/items")
            .and_then(|v| v.as_array())
            .unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0].get("id").and_then(|v| v.as_str()),
            Some(tenant_session_id.as_str())
        );
        assert_eq!(
            items[0].get("tenantId").and_then(|v| v.as_str()),
            Some("tenant-1")
        );
    } else {
        panic!("unexpected frame");
    }

    handle.abort();
}
