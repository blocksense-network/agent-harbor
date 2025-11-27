// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only
//! Minimal wrapper that routes outbound ACP notifications through the SDK
//! `AgentSideConnection::notify`, but in a Send-safe way by hosting the SDK on
//! a single-threaded runtime behind a channel.

use agent_client_protocol::{
    AgentNotification, AgentResponse, AgentSide, AgentSideConnection, ClientNotification,
    ClientRequest, Error, Id, MessageHandler, StreamMessage, StreamMessageContent,
    StreamMessageDirection,
};
use futures::io::{empty, sink};
use serde_json::{Value, json};
use tokio::sync::{broadcast, mpsc};
use tokio::task::LocalSet;

#[derive(Clone)]
pub struct Notifier {
    tx: Option<mpsc::Sender<NotifyMsg>>,
    outgoing: Option<broadcast::Sender<Value>>,
}

#[derive(Clone)]
struct NotifyMsg {
    method: String,
    params: Option<AgentNotification>,
}

impl Notifier {
    /// Create a Send-safe notifier backed by a dedicated current-thread runtime.
    pub fn new_threaded() -> Self {
        let (tx, mut rx) = mpsc::channel::<NotifyMsg>(64);
        let (outgoing_tx, _) = broadcast::channel::<Value>(64);
        let outgoing_tx_main = outgoing_tx.clone();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("build notifier runtime");
            rt.block_on(async move {
                let local = LocalSet::new();
                local
                    .run_until(async move {
                        // Incoming stream never yields; we only use the notify path.
                        let pending = empty();
                        let sink = sink();
                        let (conn, io_task) =
                            AgentSideConnection::new(NullAgent, sink, pending, |fut| {
                                tokio::task::spawn_local(fut);
                            });

                        // Forward outgoing JSON-RPC messages to the broadcast channel so
                        // transports can write them to their sockets/stdout streams.
                        let mut stream = conn.subscribe();
                        let outgoing_loop = {
                            let outgoing_tx = outgoing_tx.clone();
                            tokio::task::spawn_local(async move {
                                while let Ok(msg) = stream.recv().await {
                                    if let Some(value) = stream_message_to_value(&msg) {
                                        let _ = outgoing_tx.send(value);
                                    }
                                }
                            })
                        };

                        // Drive IO loop
                        tokio::task::spawn_local(async move {
                            let _ = io_task.await;
                        });

                        // Drain notification channel
                        while let Some(msg) = rx.recv().await {
                            let _ = conn.notify(msg.method.clone(), msg.params.clone());
                        }

                        // Drop the subscription task once the channel closes.
                        outgoing_loop.abort();
                    })
                    .await;
            });
        });

        Self {
            tx: Some(tx),
            outgoing: Some(outgoing_tx_main),
        }
    }

    /// No-op notifier (used in tests that don't care about notify path).
    pub fn noop() -> Self {
        Self {
            tx: None,
            outgoing: None,
        }
    }

    pub async fn notify(&self, method: &str, params: Option<AgentNotification>) -> Result<(), ()> {
        match &self.tx {
            Some(tx) => tx
                .send(NotifyMsg {
                    method: method.to_string(),
                    params,
                })
                .await
                .map_err(|_| ()),
            None => Err(()),
        }
    }

    pub fn subscribe(&self) -> Option<broadcast::Receiver<Value>> {
        self.outgoing.as_ref().map(|tx| tx.subscribe())
    }

    /// Push a pre-serialized JSON-RPC payload directly onto the outgoing
    /// broadcast channel. Useful for legacy compatibility frames that the SDK
    /// encoder does not produce (e.g., `session/update` with raw events).
    pub fn push_raw(&self, payload: Value) {
        if let Some(tx) = &self.outgoing {
            let _ = tx.send(payload);
        }
    }

    /// Convenience helper to wrap params in a JSON-RPC notification envelope
    /// and broadcast it.
    pub fn push_raw_notification(&self, method: &str, params: Value) {
        self.push_raw(json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }));
    }
}

impl Default for Notifier {
    fn default() -> Self {
        Self::new_threaded()
    }
}

fn stream_message_to_value(msg: &StreamMessage) -> Option<Value> {
    if msg.direction != StreamMessageDirection::Outgoing {
        return None;
    }

    match &msg.message {
        StreamMessageContent::Notification { method, params } => Some(json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params.clone().unwrap_or(Value::Null),
        })),
        StreamMessageContent::Request { id, method, params } => Some(json!({
            "jsonrpc": "2.0",
            "id": id_to_value(id),
            "method": method,
            "params": params.clone().unwrap_or(Value::Null),
        })),
        StreamMessageContent::Response { id, result } => match result {
            Ok(value) => Some(json!({
                "jsonrpc": "2.0",
                "id": id_to_value(id),
                "result": value.clone().unwrap_or(Value::Null),
            })),
            Err(err) => Some(json!({
                "jsonrpc": "2.0",
                "id": id_to_value(id),
                "error": err,
            })),
        },
    }
}

fn id_to_value(id: &Id) -> Value {
    match id {
        Id::Null => Value::Null,
        Id::Number(n) => json!(n),
        Id::Str(s) => Value::String(s.clone()),
    }
}

struct NullAgent;

impl MessageHandler<AgentSide> for NullAgent {
    fn handle_request(
        &self,
        _request: ClientRequest,
    ) -> impl std::future::Future<Output = Result<AgentResponse, Error>> {
        async { Err(Error::method_not_found()) }
    }

    fn handle_notification(
        &self,
        _notification: ClientNotification,
    ) -> impl std::future::Future<Output = Result<(), Error>> {
        async { Ok(()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::{
        ContentBlock, SessionId, SessionNotification, SessionUpdate, TextContent,
    };
    use tokio::time::{Duration, timeout};

    #[tokio::test]
    async fn notifier_forwards_jsonrpc_notifications() {
        let notifier = Notifier::new_threaded();
        let mut rx = notifier.subscribe().expect("subscription");

        notifier
            .notify(
                "sessionUpdate",
                Some(AgentNotification::SessionNotification(
                    SessionNotification {
                        session_id: SessionId("s1".into()),
                        update: SessionUpdate::AgentMessageChunk {
                            content: ContentBlock::Text(TextContent {
                                annotations: None,
                                text: "hello".to_string(),
                                meta: None,
                            }),
                        },
                        meta: None,
                    },
                )),
            )
            .await
            .expect("notify should enqueue");

        let payload = timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("payload available")
            .expect("value present");

        assert_eq!(payload.get("jsonrpc").and_then(|v| v.as_str()), Some("2.0"));
        assert_eq!(
            payload.get("method").and_then(|v| v.as_str()),
            Some("sessionUpdate")
        );
        assert_eq!(
            payload.get("params").and_then(|p| p.get("sessionId")).and_then(|v| v.as_str()),
            Some("s1")
        );
        assert_eq!(
            payload
                .get("params")
                .and_then(|p| p.get("update"))
                .and_then(|u| u.get("sessionUpdate"))
                .and_then(|v| v.as_str()),
            Some("agent_message_chunk")
        );
        assert_eq!(
            payload
                .get("params")
                .and_then(|p| p.get("update"))
                .and_then(|u| u.get("content"))
                .and_then(|c| c.get("text"))
                .and_then(|v| v.as_str()),
            Some("hello")
        );
    }

    #[tokio::test]
    async fn raw_notifications_reach_subscribers() {
        let notifier = Notifier::new_threaded();
        let mut rx = notifier.subscribe().expect("subscription");

        notifier.push_raw_notification("session/update", json!({"hello": "world"}));

        let payload = timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("payload available")
            .expect("value present");

        assert_eq!(
            payload.get("method").and_then(|v| v.as_str()),
            Some("session/update")
        );
        assert_eq!(
            payload.get("params").and_then(|p| p.get("hello")).and_then(|v| v.as_str()),
            Some("world")
        );
    }
}
