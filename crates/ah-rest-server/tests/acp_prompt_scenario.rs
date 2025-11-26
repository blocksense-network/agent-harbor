// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use std::{net::TcpListener, path::PathBuf, time::Duration};

use ah_rest_server::{
    Server, ServerConfig,
    mock_dependencies::{MockServerDependencies, ScenarioPlaybackOptions},
};
use futures::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message as WsMessage;

async fn spawn_acp_server_with_scenario(fixture: PathBuf) -> (String, JoinHandle<()>) {
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

    let deps = MockServerDependencies::with_options(
        config.clone(),
        ScenarioPlaybackOptions {
            scenario_files: vec![fixture],
            speed_multiplier: 0.1,
        },
    )
    .await
    .expect("deps");
    let server = Server::with_state(config, deps.into_state()).await.expect("server");
    let handle = tokio::spawn(async move { server.run().await.expect("server run") });
    let acp_url = format!("ws://{}/acp/v1/connect?api_key=secret", acp_addr);
    (acp_url, handle)
}

#[tokio::test]
async fn acp_prompt_scenario_streams_events() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/acp_bridge/scenarios/prompt_turn_basic.yaml");
    let (acp_url, handle) = spawn_acp_server_with_scenario(fixture).await;

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
                    "prompt":"prompt_turn_basic",
                    "agent":"sonnet"
                }
            })
            .to_string(),
        ))
        .await
        .expect("session/new");
    let created = socket.next().await.expect("created frame").expect("frame");
    let session_id = if let WsMessage::Text(text) = created {
        let value: Value = serde_json::from_str(&text).expect("json");
        value.pointer("/result/sessionId").and_then(|v| v.as_str()).unwrap().to_string()
    } else {
        panic!("unexpected frame");
    };

    // send a prompt to trigger log streaming even if playback finishes quickly
    socket
        .send(WsMessage::Text(
            json!({
                "id":3,
                "method":"session/prompt",
                "params": { "sessionId": session_id, "message": "hello scenario" }
            })
            .to_string(),
        ))
        .await
        .expect("prompt send");
    let _ = socket.next().await; // prompt ack

    let mut saw_running = false;
    let mut saw_log = false;
    let mut saw_completed = false;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        if let Some(msg) = socket.next().await {
            let msg = match msg {
                Ok(m) => m,
                Err(_) => break,
            };
            if let WsMessage::Text(text) = msg {
                let value: Value = serde_json::from_str(&text).expect("json");
                if value.get("method").and_then(|v| v.as_str()) == Some("session/update") {
                    if let Some(sid) = value.pointer("/params/sessionId").and_then(|v| v.as_str()) {
                        assert_eq!(sid, session_id);
                    }
                    if let Some(event_type) =
                        value.pointer("/params/event/type").and_then(|v| v.as_str())
                    {
                        match event_type {
                            "status" => {
                                if value.pointer("/params/event/status").and_then(|v| v.as_str())
                                    == Some("running")
                                {
                                    saw_running = true;
                                }
                                if value.pointer("/params/event/status").and_then(|v| v.as_str())
                                    == Some("completed")
                                {
                                    saw_completed = true;
                                }
                            }
                            "log" => saw_log = true,
                            _ => {}
                        }
                    }
                }
            }
        }
        if saw_running && saw_log && saw_completed {
            break;
        }
    }

    assert!(saw_running, "should stream running status");
    assert!(saw_log, "should stream log event");
    assert!(saw_completed, "should stream completed status");

    handle.abort();
}
