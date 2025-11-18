// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Repository-related handlers

use crate::error::{ServerError, ServerResult};
use crate::services::RepositoryService;
use crate::state::AppState;
use ah_rest_api_contract::{
    BranchInfo, RepositoryBranchesResponse, RepositoryFile, RepositoryFilesResponse,
};
use axum::{Json, extract::Path, extract::State};

/// Get branches for a repository
#[utoipa::path(
    get,
    path = "/api/v1/repositories/{id}/branches",
    responses(
        (status = 200, description = "List of branches for the repository", body = RepositoryBranchesResponse),
        (status = 404, description = "Repository not found"),
        (status = 500, description = "Internal server error")
    ),
    params(
        ("id" = String, Path, description = "Repository ID")
    )
)]
pub async fn get_repository_branches(
    State(state): State<AppState>,
    Path(repository_id): Path<String>,
) -> ServerResult<Json<RepositoryBranchesResponse>> {
    let repository_service = RepositoryService::new(state.db.clone());

    let branches =
        repository_service.get_repository_branches(&repository_id).await.map_err(|e| {
            tracing::error!(
                "Failed to get branches for repository {}: {}",
                repository_id,
                e
            );
            ServerError::Internal("Failed to get repository branches".to_string())
        })?;

    let response = RepositoryBranchesResponse {
        repository_id: repository_id.clone(),
        branches: branches
            .into_iter()
            .map(|branch| BranchInfo {
                name: branch.name,
                is_default: branch.is_default,
                last_commit: branch.last_commit,
            })
            .collect(),
    };

    Ok(Json(response))
}

/// Get files for a repository
#[utoipa::path(
    get,
    path = "/api/v1/repositories/{id}/files",
    responses(
        (status = 200, description = "List of files for the repository", body = RepositoryFilesResponse),
        (status = 404, description = "Repository not found"),
        (status = 500, description = "Internal server error")
    ),
    params(
        ("id" = String, Path, description = "Repository ID")
    )
)]
pub async fn get_repository_files(
    State(state): State<AppState>,
    Path(repository_id): Path<String>,
) -> ServerResult<Json<RepositoryFilesResponse>> {
    let repository_service = RepositoryService::new(state.db.clone());

    let files = repository_service.get_repository_files(&repository_id).await.map_err(|e| {
        tracing::error!(
            "Failed to get files for repository {}: {}",
            repository_id,
            e
        );
        ServerError::Internal("Failed to get repository files".to_string())
    })?;

    let response = RepositoryFilesResponse {
        repository_id: repository_id.clone(),
        files: files
            .into_iter()
            .map(|file| RepositoryFile {
                path: file.path,
                detail: file.detail,
            })
            .collect(),
    };

    Ok(Json(response))
}
