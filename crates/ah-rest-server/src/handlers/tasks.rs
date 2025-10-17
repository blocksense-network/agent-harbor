//! Task management endpoints

use crate::models::SessionStore;
use crate::state::AppState;
use crate::ServerResult;
use ah_rest_api_contract::{CreateTaskRequest, CreateTaskResponse, SessionStatus};
use axum::{extract::State, Json};
use std::sync::Arc;
// use validator::Validate; // Temporarily disabled due to version mismatch

/// Create a new task/session
#[utoipa::path(
    post,
    path = "/api/v1/tasks",
    request_body = CreateTaskRequest,
    responses(
        (status = 201, description = "Task created successfully", body = CreateTaskResponse),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Authentication required"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn create_task(
    State(state): State<AppState>,
    Json(request): Json<CreateTaskRequest>,
) -> ServerResult<Json<CreateTaskResponse>> {
    // Validate the request (temporarily disabled)
    // request.validate()?;

    // Create the session in database
    let session_service = crate::services::SessionService::new(Arc::clone(&state.session_store));
    let response = session_service.create_session(&request).await?;
    drop(session_service); // Release the service

    // Verify the session was created by fetching it
    let session = state.session_store.get_session(&response.id).await?
        .ok_or_else(|| crate::ServerError::BadRequest("Failed to retrieve created session".to_string()))?;

    Ok(Json(response))
}
