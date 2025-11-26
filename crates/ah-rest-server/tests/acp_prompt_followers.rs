// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use std::time::Duration;

use common::acp::spawn_acp_server_with_scenario;
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message as WsMessage;

mod common;

#[derive(Debug, Deserialize)]
struct Scenario {
    timeline: Vec<Step>,
}

#[derive(Debug, Deserialize)]
struct Step {
    #[serde(default)]
    client: Option<Frame>,
    #[serde(default)]
    server: Option<ExpectedFrame>,
}

#[derive(Debug, Deserialize, Clone)]
struct Frame {
    id: Option<i64>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Deserialize, Clone)]
struct ExpectedFrame {
    #[serde(default)]
    id: Option<i64>,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    params: Option<Value>,
    #[serde(default)]
    result: Option<Value>,
}

#[derive(Default)]
struct ScenarioContext {
    session_id: Option<String>,
}

#[tokio::test]
async fn scenario_terminal_follow_detach_replays_updates() {
    // Uses Scenario Format timeline in tests/acp_bridge/scenarios/terminal_follow_detach.yaml
    let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/acp_bridge/scenarios/terminal_follow_detach.yaml");
    let spec: Scenario =
        serde_yaml::from_reader(std::fs::File::open(&fixture).unwrap()).expect("parse scenario");

    let (acp_url, handle) = spawn_acp_server_with_scenario(fixture).await;
    let acp_url = format!("{}?api_key=secret", acp_url);
    let (mut socket, _) = tokio_tungstenite::connect_async(&acp_url).await.expect("connect");

    let mut ctx = ScenarioContext::default();

    for step in spec.timeline {
        if let Some(client) = step.client {
            let params = apply_placeholders(client.params, &ctx);
            let body = json!({"id": client.id, "method": client.method, "params": params});
            socket.send(WsMessage::Text(body.to_string())).await.expect("send frame");
        }

        if let Some(server) = step.server {
            let expected = apply_placeholders_expected(server.clone(), &ctx);
            eprintln!(
                "waiting for expected frame: {:?}",
                expected_summary(&expected)
            );
            let msg = recv_matching(&mut socket, &expected).await;
            update_context(&msg, &mut ctx);
        }
    }

    handle.abort();
}

fn apply_placeholders(mut value: Value, ctx: &ScenarioContext) -> Value {
    match value {
        Value::String(ref mut s) => {
            if s.contains("$SESSION_ID") {
                let replacement = ctx.session_id.clone().unwrap_or_default();
                *s = s.replace("$SESSION_ID", &replacement);
            }
            Value::String(s.clone())
        }
        Value::Array(arr) => {
            Value::Array(arr.into_iter().map(|v| apply_placeholders(v, ctx)).collect())
        }
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                out.insert(k, apply_placeholders(v, ctx));
            }
            Value::Object(out)
        }
        other => other,
    }
}

fn apply_placeholders_expected(frame: ExpectedFrame, ctx: &ScenarioContext) -> ExpectedFrame {
    ExpectedFrame {
        id: frame.id,
        method: frame.method,
        params: frame.params.map(|v| apply_placeholders(v, ctx)),
        result: frame.result.map(|v| apply_placeholders(v, ctx)),
    }
}

async fn recv_matching(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    expected: &ExpectedFrame,
) -> Value {
    timeout(Duration::from_secs(10), async {
        loop {
            let msg_opt = socket.next().await;
            let msg = msg_opt.expect("ws closed").expect("frame");
            if let WsMessage::Text(text) = msg {
                let value: Value = serde_json::from_str(&text).expect("json");
                if matches_expected(expected, &value) {
                    eprintln!("matched frame: {}", value);
                    return value;
                } else {
                    eprintln!(
                        "skipping frame: {}; match_debug={}",
                        value,
                        debug_match(expected, &value)
                    );
                }
            }
        }
    })
    .await
    .expect("timed out waiting for expected frame")
}

fn matches_expected(expected: &ExpectedFrame, actual: &Value) -> bool {
    if let Some(id) = expected.id {
        if actual.get("id").and_then(|v| v.as_i64()) != Some(id) {
            return false;
        }
    }
    if let Some(method) = &expected.method {
        if actual.get("method").and_then(|v| v.as_str()) != Some(method.as_str()) {
            return false;
        }
    }
    if let Some(params) = &expected.params {
        if !value_contains(actual.get("params"), Some(params)) {
            return false;
        }
    }
    if let Some(result) = &expected.result {
        if !value_contains(actual.get("result"), Some(result)) {
            return false;
        }
    }
    true
}

fn value_contains(haystack: Option<&Value>, needle: Option<&Value>) -> bool {
    match (haystack, needle) {
        (_, None) => true,
        (Some(Value::Object(h)), Some(Value::Object(n))) => n.iter().all(|(k, v)| {
            h.get(k).map(|actual| value_contains(Some(actual), Some(v))).unwrap_or(false)
        }),
        (Some(Value::Array(h)), Some(Value::Array(n))) => {
            if n.len() > h.len() {
                return false;
            }
            n.iter().zip(h.iter()).all(|(n_v, h_v)| value_contains(Some(h_v), Some(n_v)))
        }
        (Some(Value::String(h)), Some(Value::String(n))) => h == n,
        (Some(Value::Number(h)), Some(Value::Number(n))) => h == n,
        (Some(Value::Bool(h)), Some(Value::Bool(n))) => h == n,
        _ => false,
    }
}

fn update_context(msg: &Value, ctx: &mut ScenarioContext) {
    if let Some(id) = msg.get("id").and_then(|v| v.as_i64()) {
        if id == 2 {
            if let Some(session_id) = msg.pointer("/result/sessionId").and_then(|v| v.as_str()) {
                ctx.session_id = Some(session_id.to_string());
            }
        }
    }

    if let Some(session_id) = msg.pointer("/params/sessionId").and_then(|v| v.as_str()) {
        if ctx.session_id.is_none() {
            ctx.session_id = Some(session_id.to_string());
        }
    }
}

fn expected_summary(frame: &ExpectedFrame) -> String {
    let mut parts = Vec::new();
    if let Some(id) = frame.id {
        parts.push(format!("id={id}"));
    }
    if let Some(method) = &frame.method {
        parts.push(format!("method={method}"));
    }
    if let Some(params) = &frame.params {
        parts.push(format!("params={}", params));
    }
    if let Some(result) = &frame.result {
        parts.push(format!("result={result}"));
    }
    parts.join(" ")
}

fn debug_match(expected: &ExpectedFrame, actual: &Value) -> String {
    let id_ok = match (expected.id, actual.get("id").and_then(|v| v.as_i64())) {
        (Some(e), Some(a)) => e == a,
        (None, _) => true,
        _ => false,
    };
    let method_ok = match (
        &expected.method,
        actual.get("method").and_then(|v| v.as_str()),
    ) {
        (Some(e), Some(a)) => e == a,
        (None, _) => true,
        _ => false,
    };
    let params_ok = matches_optional(&expected.params, actual.get("params"));
    let result_ok = matches_optional(&expected.result, actual.get("result"));
    format!("id_ok={id_ok} method_ok={method_ok} params_ok={params_ok} result_ok={result_ok}")
}

fn matches_optional(expected: &Option<Value>, actual: Option<&Value>) -> bool {
    match (expected, actual) {
        (None, _) => true,
        (Some(exp), Some(act)) => value_contains(Some(act), Some(exp)),
        _ => false,
    }
}
