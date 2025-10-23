// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Draft task management endpoints

use crate::ServerResult;
use crate::error::ServerError;
use ah_rest_api_contract::CreateTaskRequest;
use axum::{Json, extract::Path};

/// List draft tasks
pub async fn list_drafts() -> ServerResult<Json<Vec<CreateTaskRequest>>> {
    // Placeholder - return empty list
    Ok(Json(vec![]))
}

/// Create a new draft task
pub async fn create_draft(Json(_draft): Json<CreateTaskRequest>) -> ServerResult<Json<String>> {
    // Placeholder - return draft ID
    Ok(Json("draft-123".to_string()))
}

/// Get a specific draft
pub async fn get_draft(Path(_draft_id): Path<String>) -> ServerResult<Json<CreateTaskRequest>> {
    Err(ServerError::BadRequest("Draft not found".to_string()))
}

/// Update a draft
pub async fn update_draft(
    Path(_draft_id): Path<String>,
    Json(_draft): Json<CreateTaskRequest>,
) -> ServerResult<Json<String>> {
    Err(ServerError::BadRequest("Draft not found".to_string()))
}

/// Delete a draft
pub async fn delete_draft(Path(_draft_id): Path<String>) -> ServerResult<()> {
    Err(ServerError::BadRequest("Draft not found".to_string()))
}
