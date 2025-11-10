// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Session management endpoints

use crate::ServerResult;
use crate::error::ServerError;
use crate::models::SessionStore;
use crate::state::AppState;
use ah_rest_api_contract::*;
use axum::{
    Json,
    extract::{Path, Query, State},
    response::sse::{Event, KeepAlive, Sse},
};
use futures::stream::{self, Stream};
use std::convert::Infallible;
use std::time::Duration;
use tokio_stream::StreamExt;

/// List sessions with optional filtering
pub async fn list_sessions(
    State(state): State<AppState>,
    Query(filters): Query<FilterQuery>,
) -> ServerResult<Json<SessionListResponse>> {
    let sessions = state.session_store.list_sessions(&filters).await?;

    // Sessions are already filtered by the session store
    let filtered_sessions = sessions;

    let total = filtered_sessions.len() as u32;

    Ok(Json(SessionListResponse {
        items: filtered_sessions,
        next_page: None, // TODO: Implement pagination
        total: Some(total),
    }))
}

/// Get a specific session
pub async fn get_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> ServerResult<Json<Session>> {
    if let Some(internal_session) = state.session_store.get_session(&session_id).await? {
        Ok(Json(internal_session.session))
    } else {
        Err(ServerError::SessionNotFound(session_id))
    }
}

/// Update a session (placeholder)
pub async fn update_session(
    State(_state): State<AppState>,
    Path(_session_id): Path<String>,
) -> ServerResult<Json<Session>> {
    Err(ServerError::NotImplemented("Session updates".to_string()))
}

/// Delete/cancel a session
pub async fn delete_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> ServerResult<()> {
    // Stop the task if it's running
    let _ = state.task_executor.stop_task(&session_id).await;

    // Delete from database
    state.session_store.delete_session(&session_id).await?;
    Ok(())
}

/// Stop a session gracefully
pub async fn stop_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> ServerResult<()> {
    if let Some(mut internal_session) = state.session_store.get_session(&session_id).await? {
        internal_session.session.status = SessionStatus::Stopping;
        state.session_store.update_session(&session_id, &internal_session).await?;
        Ok(())
    } else {
        Err(ServerError::SessionNotFound(session_id))
    }
}

/// Pause a session
pub async fn pause_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> ServerResult<()> {
    if let Some(mut internal_session) = state.session_store.get_session(&session_id).await? {
        internal_session.session.status = SessionStatus::Pausing;
        state.session_store.update_session(&session_id, &internal_session).await?;
        Ok(())
    } else {
        Err(ServerError::SessionNotFound(session_id))
    }
}

/// Resume a session
pub async fn resume_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> ServerResult<()> {
    if let Some(mut internal_session) = state.session_store.get_session(&session_id).await? {
        internal_session.session.status = SessionStatus::Resuming;
        state.session_store.update_session(&session_id, &internal_session).await?;
        Ok(())
    } else {
        Err(ServerError::SessionNotFound(session_id))
    }
}

/// Get session logs
pub async fn get_session_logs(
    State(_state): State<AppState>,
    Path(_session_id): Path<String>,
    Query(_query): Query<LogQuery>,
) -> ServerResult<Json<SessionLogsResponse>> {
    // Placeholder implementation - return empty logs
    Ok(Json(SessionLogsResponse {
        items: vec![],
        next_page: None,
    }))
}

/// Stream session events via SSE
pub async fn stream_session_events(
    State(_state): State<AppState>,
    Path(_session_id): Path<String>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Create a simple heartbeat stream (placeholder - in real implementation,
    // this would stream actual session events)
    let stream = stream::unfold(0, |count| async move {
        tokio::time::sleep(Duration::from_secs(30)).await;
        Some((
            Ok(Event::default().event("heartbeat").data(format!("heartbeat-{}", count))),
            count + 1,
        ))
    });

    Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)).text("keep-alive"))
}

/// Get session info (fleet and endpoints)
pub async fn get_session_info(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> ServerResult<Json<SessionInfoResponse>> {
    if let Some(internal_session) = state.session_store.get_session(&session_id).await? {
        let session = &internal_session.session;
        let response = SessionInfoResponse {
            id: session.id.clone(),
            status: session.status.clone(),
            fleet: FleetInfo {
                leader: "localhost".to_string(), // placeholder
                followers: vec![],               // placeholder
            },
            endpoints: SessionEndpoints {
                events: format!("/api/v1/sessions/{}/events", session_id),
            },
        };
        Ok(Json(response))
    } else {
        Err(ServerError::SessionNotFound(session_id))
    }
}
