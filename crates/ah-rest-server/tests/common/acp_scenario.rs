use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, Result};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::Value;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message as WsMessage;

/// Minimal ACP scenario driver that replays client/server frames described in a YAML file.
///
/// The scenario format mirrors the existing `tests/acp_bridge/scenarios/*.yaml` fixtures:
/// ```
/// timeline:
///   - client: { id: 1, method: initialize, params: {} }
///   - server: { id: 1 }
///   - client: { id: 2, method: session/new, params: { prompt: "hi" } }
///   - server: { id: 2 }
/// ```
/// Placeholder strings like `$SESSION_ID` are replaced with values captured from
/// earlier responses (any JSON value assigned to a key is stored in the context).
pub async fn run_acp_scenario(acp_url: &str, fixture: std::path::PathBuf) -> Result<()> {
    let spec: Scenario = serde_yaml::from_reader(std::fs::File::open(&fixture)?)?;
    let (mut socket, _) = tokio_tungstenite::connect_async(acp_url).await?;

    let mut ctx = ContextVars::default();

    for step in spec.timeline {
        if let Some(client) = step.client {
            let params = apply_placeholders(client.params, &ctx);
            let mut body = serde_json::Map::new();
            if let Some(id) = client.id {
                body.insert("id".into(), Value::from(id));
            }
            body.insert("method".into(), Value::from(client.method));
            body.insert("params".into(), params);
            socket.send(WsMessage::Text(Value::Object(body).to_string())).await?;
        }

        if let Some(server) = step.server {
            let expected = apply_placeholders_expected(server, &ctx);
            let msg = recv_matching(&mut socket, &expected).await?;
            update_context(&msg, &mut ctx);
        }
    }

    Ok(())
}

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
    #[serde(default)]
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
struct ContextVars {
    values: HashMap<String, String>,
}

fn apply_placeholders(mut value: Value, ctx: &ContextVars) -> Value {
    match value {
        Value::String(ref mut s) => {
            for (k, v) in &ctx.values {
                let placeholder = format!("${}", k);
                if s.contains(&placeholder) {
                    *s = s.replace(&placeholder, v);
                }
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

fn apply_placeholders_expected(frame: ExpectedFrame, ctx: &ContextVars) -> ExpectedFrame {
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
) -> Result<Value> {
    let deadline = Duration::from_secs(10);
    let val = timeout(deadline, async {
        loop {
            let msg_opt = socket.next().await;
            let msg = msg_opt.context("websocket closed unexpectedly")??;
            if let WsMessage::Text(text) = msg {
                let value: Value = serde_json::from_str(&text)?;
                if matches_expected(expected, &value) {
                    return Ok::<Value, anyhow::Error>(value);
                }
            }
        }
    })
    .await
    .context("timed out waiting for expected frame")??;
    Ok(val)
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

fn update_context(msg: &Value, ctx: &mut ContextVars) {
    // Capture any string values under known keys
    if let Some(session_id) = msg.pointer("/result/sessionId").and_then(|v| v.as_str()) {
        ctx.values.insert("SESSION_ID".into(), session_id.to_string());
    }
    if let Some(session_id) = msg.pointer("/params/sessionId").and_then(|v| v.as_str()) {
        ctx.values.insert("SESSION_ID".into(), session_id.to_string());
    }
    if let Some(exec_id) = msg.pointer("/params/event/executionId").and_then(|v| v.as_str()) {
        ctx.values.insert("EXECUTION_ID".into(), exec_id.to_string());
    }
}
