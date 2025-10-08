//! Workspace management endpoints

use crate::error::ServerError;
use crate::ServerResult;
use ah_rest_api_contract::Workspace;
use axum::{extract::Path, Json};

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
