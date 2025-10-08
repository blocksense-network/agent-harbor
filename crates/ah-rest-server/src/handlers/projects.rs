//! Project and repository management endpoints

use crate::ServerResult;
use ah_rest_api_contract::{Project, Repository};
use axum::{extract::Query, Json};

/// List projects
pub async fn list_projects(
    Query(_params): Query<std::collections::HashMap<String, String>>,
) -> ServerResult<Json<Vec<Project>>> {
    // Placeholder - return empty list
    Ok(Json(vec![]))
}

/// List repositories
pub async fn list_repositories(
    Query(_params): Query<std::collections::HashMap<String, String>>,
) -> ServerResult<Json<Vec<Repository>>> {
    // Placeholder - return empty list
    Ok(Json(vec![]))
}
