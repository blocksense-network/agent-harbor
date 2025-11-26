// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only
//! Minimal wrapper that routes outbound ACP notifications through the SDK
//! `AgentSideConnection::notify`, so transports share a single serialization path.

use agent_client_protocol::{AgentNotification, AgentSideConnection};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Cheap wrapper because `AgentSideConnection` is not `Sync` on its own.
#[derive(Clone)]
pub struct Notifier {
    inner: Arc<Mutex<Option<AgentSideConnection>>>,
}

impl Notifier {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
        }
    }

    pub fn noop() -> Self {
        Self::new()
    }

    pub async fn set(&self, conn: AgentSideConnection) {
        let mut guard = self.inner.lock().await;
        *guard = Some(conn);
    }

    pub async fn clear(&self) {
        let mut guard = self.inner.lock().await;
        *guard = None;
    }

    pub async fn notify(&self, method: &str, params: Option<AgentNotification>) -> Result<(), ()> {
        let guard = self.inner.lock().await;
        if let Some(conn) = guard.as_ref() {
            conn.notify(method, params).map_err(|_e| ())?;
            Ok(())
        } else {
            Err(())
        }
    }
}

impl Default for Notifier {
    fn default() -> Self {
        Self::new()
    }
}
