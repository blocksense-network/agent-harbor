//! Session management endpoints

use crate::error::ServerError;
use crate::state::AppState;
use crate::ServerResult;
use ah_rest_api_contract::*;
use axum::{
    extract::{Path, Query, State},
    response::sse::{Event, KeepAlive, Sse},
    Json,
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
    let sessions = state.active_sessions.read().await;

    // Apply filters (placeholder implementation)
    let filtered_sessions: Vec<Session> = sessions
        .values()
        .filter(|session| {
            if let Some(status_filter) = &filters.status {
                if &session.status.to_string().to_lowercase() != status_filter {
                    return false;
                }
            }
            if let Some(project_id) = &filters.project_id {
                if session.project_id.as_ref() != Some(project_id) {
                    return false;
                }
            }
            if let Some(tenant_id) = &filters.tenant_id {
                if session.tenant_id.as_ref() != Some(tenant_id) {
                    return false;
                }
            }
            true
        })
        .cloned()
        .collect();

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
    let sessions = state.active_sessions.read().await;

    if let Some(session) = sessions.get(&session_id) {
        Ok(Json(session.clone()))
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
    let mut sessions = state.active_sessions.write().await;

    if sessions.remove(&session_id).is_some() {
        Ok(())
    } else {
        Err(ServerError::SessionNotFound(session_id))
    }
}

/// Stop a session gracefully
pub async fn stop_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> ServerResult<()> {
    let mut sessions = state.active_sessions.write().await;

    if let Some(session) = sessions.get_mut(&session_id) {
        session.status = SessionStatus::Stopping;
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
    let mut sessions = state.active_sessions.write().await;

    if let Some(session) = sessions.get_mut(&session_id) {
        session.status = SessionStatus::Pausing;
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
    let mut sessions = state.active_sessions.write().await;

    if let Some(session) = sessions.get_mut(&session_id) {
        session.status = SessionStatus::Resuming;
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
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let sessions = state.active_sessions.read().await;

    // Create a simple heartbeat stream
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
    let sessions = state.active_sessions.read().await;

    if let Some(session) = sessions.get(&session_id) {
        let response = SessionInfoResponse {
            id: session.id.clone(),
            status: session.status,
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
