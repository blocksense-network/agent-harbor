// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only
//! Minimal wrapper that routes outbound ACP notifications through the SDK
//! `AgentSideConnection::notify`, but in a Send-safe way by hosting the SDK on
//! a single-threaded runtime behind a channel.

use agent_client_protocol::{
    AgentNotification, AgentResponse, AgentSide, AgentSideConnection, ClientNotification,
    ClientRequest, Error, MessageHandler,
};
use futures::io::{empty, sink};
use tokio::{sync::mpsc, task::LocalSet};

#[derive(Clone)]
pub struct Notifier {
    tx: Option<mpsc::Sender<NotifyMsg>>,
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

                        // Drive IO loop
                        tokio::task::spawn_local(async move {
                            let _ = io_task.await;
                        });

                        // Drain notification channel
                        while let Some(msg) = rx.recv().await {
                            let _ = conn.notify(msg.method.clone(), msg.params.clone());
                        }
                    })
                    .await;
            });
        });

        Self { tx: Some(tx) }
    }

    /// No-op notifier (used in tests that don't care about notify path).
    pub fn noop() -> Self {
        Self { tx: None }
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
}

impl Default for Notifier {
    fn default() -> Self {
        Self::new_threaded()
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
