// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Project and repository management endpoints

use crate::{ServerResult, state::AppState};
use ah_core::DatabaseManager;
use ah_rest_api_contract::{Project, Repository};
use axum::{
    Json,
    extract::{Query, State},
};
use std::collections::HashMap;
use tracing::warn;
use url::Url;

/// List projects
pub async fn list_projects(
    State(_state): State<AppState>,
    Query(_params): Query<HashMap<String, String>>,
) -> ServerResult<Json<Vec<Project>>> {
    // TODO: Replace with real project catalog once server tracks projects.
    // For now we return a single pseudo-project so remote clients have a stable anchor.
    Ok(Json(vec![Project {
        id: "default".to_string(),
        display_name: "Local Git Repositories".to_string(),
        last_used_at: None,
    }]))
}

/// List repositories
pub async fn list_repositories(
    State(state): State<AppState>,
    Query(_params): Query<HashMap<String, String>>,
) -> ServerResult<Json<Vec<Repository>>> {
    let db_manager = DatabaseManager::with_database((*state.db).clone());

    let repo_records = db_manager.list_repositories().map_err(|err| {
        tracing::error!("Failed to list repositories from database: {}", err);
        crate::error::ServerError::Internal("failed to list repositories".into())
    })?;

    let repositories = repo_records
        .into_iter()
        .map(|record| {
            let display_name = record
                .remote_url
                .as_ref()
                .and_then(|raw| Url::parse(raw).ok())
                .and_then(|url| {
                    let trimmed = url.path().trim_matches('/');
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed.to_string())
                    }
                })
                .or_else(|| {
                    record.root_path.as_ref().map(|path| {
                        std::path::Path::new(path)
                            .file_name()
                            .map(|name| name.to_string_lossy().to_string())
                            .unwrap_or_else(|| path.to_string())
                    })
                })
                .unwrap_or_else(|| format!("repo-{}", record.id));

            let remote_url = record.remote_url.as_ref().and_then(|raw| Url::parse(raw).ok()).or_else(|| {
                record.root_path.as_ref().and_then(|path| {
                    std::path::Path::new(path)
                        .canonicalize()
                        .ok()
                        .and_then(|p| Url::from_file_path(&p).ok())
                })
            });

            let url = match remote_url {
                Some(url) => url,
                None => {
                    warn!(
                        "Repository {} missing usable remote URL; falling back to placeholder value",
                        record.id
                    );
                    // Use placeholder URL that still parses so clients can render.
                    Url::parse("https://example.invalid/local-repo").expect("static URL to parse")
                }
            };

            Repository {
                id: record.id.to_string(),
                display_name,
                scm_provider: record.vcs.clone(),
                remote_url: url,
                default_branch: record
                    .default_branch
                    .clone()
                    .unwrap_or_else(|| "main".to_string()),
                last_used_at: None,
            }
        })
        .collect::<Vec<_>>();

    Ok(Json(repositories))
}
