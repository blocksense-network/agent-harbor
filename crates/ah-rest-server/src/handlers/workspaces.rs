// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Workspace management endpoints

use crate::ServerResult;
use crate::error::ServerError;
use ah_rest_api_contract::Workspace;
use axum::{Json, extract::Path};

/// List workspaces
pub async fn list_workspaces() -> ServerResult<Json<Vec<Workspace>>> {
    // Placeholder - return empty list
    Ok(Json(vec![]))
}

/// Get workspace details
pub async fn get_workspace(Path(_workspace_id): Path<String>) -> ServerResult<Json<Workspace>> {
    // Placeholder - return not found
    Err(ServerError::BadRequest("Workspace not found".to_string()))
}
