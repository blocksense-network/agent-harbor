// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Server-Sent Events (SSE) streaming support

use ah_rest_api_contract::SessionEvent;
use eventsource_client::{Client, ClientBuilder, SSE};
use futures::StreamExt;
use futures::stream::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc;

use crate::auth::AuthConfig;
use crate::error::{RestClientError, RestClientResult};

/// SSE event stream for session events
pub struct SessionEventStream {
    receiver: mpsc::Receiver<Result<SessionEvent, RestClientError>>,
    _handle: tokio::task::JoinHandle<()>,
}

impl SessionEventStream {
    /// Create a new SSE stream for session events
    pub async fn connect(
        base_url: &url::Url,
        session_id: &str,
        auth: &AuthConfig,
    ) -> RestClientResult<Self> {
        let url = base_url
            .join(&format!("/api/v1/sessions/{}/events", session_id))
            .map_err(RestClientError::from)?;

        let mut builder = ClientBuilder::for_url(url.as_str())
            .map_err(|e| RestClientError::Sse(e.to_string()))?;

        let headers = auth.headers().map_err(|e| RestClientError::Auth(e.to_string()))?;
        for (name, value) in headers.iter() {
            if let Ok(val) = value.to_str() {
                builder = builder
                    .header(name.as_str(), val)
                    .map_err(|e| RestClientError::Sse(e.to_string()))?;
            }
        }

        let client = builder.build();

        let (tx, rx) = mpsc::channel(32);
        let handle = tokio::spawn(async move {
            let mut stream = client.stream();
            while let Some(event) = stream.next().await {
                match event {
                    Ok(SSE::Connected(_)) => {
                        // Keep the stream alive; no payload to decode.
                    }
                    Ok(SSE::Event(ev)) => {
                        // Default event type is "message"; server uses "session"
                        if ev.event_type != "session" && !ev.event_type.is_empty() {
                            continue;
                        }
                        match serde_json::from_str::<SessionEvent>(&ev.data) {
                            Ok(parsed) => {
                                if tx.send(Ok(parsed)).await.is_err() {
                                    break;
                                }
                            }
                            Err(err) => {
                                let _ = tx.send(Err(RestClientError::Sse(err.to_string()))).await;
                                break;
                            }
                        }
                    }
                    Ok(SSE::Comment(_)) => {}
                    Err(err) => {
                        let _ = tx.send(Err(RestClientError::Sse(err.to_string()))).await;
                        break;
                    }
                }
            }
        });

        Ok(SessionEventStream {
            receiver: rx,
            _handle: handle,
        })
    }
}

impl Stream for SessionEventStream {
    type Item = Result<SessionEvent, RestClientError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.receiver.poll_recv(cx)
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_session_event_parsing() {
        let event_json = r#"{
            "type": "status",
            "status": "running",
            "timestamp": 1733030400
        }"#;

        let event: ah_rest_api_contract::SessionEvent = serde_json::from_str(event_json).unwrap();
        match event {
            ah_rest_api_contract::SessionEvent::Status(event) => {
                assert_eq!(event.status, ah_rest_api_contract::SessionStatus::Running);
            }
            _ => panic!("Expected Status event"),
        }
    }
}
