// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use std::{collections::HashMap, sync::Arc};

use ah_core::task_manager_wire::TaskManagerMessage;
use tokio::sync::{Mutex, broadcast};

/// In-memory buffer + broadcaster for PTY streams coming from the task-manager socket.
pub struct TaskSocketHub {
    buffers: Arc<Mutex<HashMap<String, Vec<TaskManagerMessage>>>>,
    senders: Arc<Mutex<HashMap<String, broadcast::Sender<TaskManagerMessage>>>>,
}

impl TaskSocketHub {
    pub fn new() -> Self {
        Self {
            buffers: Arc::new(Mutex::new(HashMap::new())),
            senders: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Record an incoming PTY message and broadcast to followers.
    pub async fn record(&self, session: &str, msg: TaskManagerMessage) {
        {
            let mut buffers = self.buffers.lock().await;
            buffers.entry(session.to_string()).or_default().push(msg.clone());
        }

        if let Some(tx) = self.senders.lock().await.get(session) {
            let _ = tx.send(msg);
        }
    }

    /// Subscribe to live stream and get current backlog snapshot.
    pub async fn subscribe(
        &self,
        session: &str,
    ) -> (
        Vec<TaskManagerMessage>,
        broadcast::Receiver<TaskManagerMessage>,
    ) {
        let backlog = {
            let buffers = self.buffers.lock().await;
            buffers.get(session).cloned().unwrap_or_default()
        };

        let tx = {
            let mut senders = self.senders.lock().await;
            senders
                .entry(session.to_string())
                .or_insert_with(|| broadcast::channel(256).0)
                .clone()
        };

        (backlog, tx.subscribe())
    }
}

impl Default for TaskSocketHub {
    fn default() -> Self {
        Self::new()
    }
}
