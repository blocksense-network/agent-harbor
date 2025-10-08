//! Health check endpoints

use crate::ServerResult;
use axum::Json;
use serde::Serialize;

/// Health check response
#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub timestamp: String,
}

/// Version response
#[derive(Serialize)]
pub struct VersionResponse {
    pub version: String,
    pub build_info: BuildInfo,
}

/// Build information
#[derive(Serialize)]
pub struct BuildInfo {
    pub git_commit: Option<String>,
    pub build_date: Option<String>,
}

/// Health check endpoint
pub async fn health_check() -> ServerResult<Json<HealthResponse>> {
    let response = HealthResponse {
        status: "ok".to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    };
    Ok(Json(response))
}

/// Readiness check endpoint
pub async fn readiness_check() -> ServerResult<Json<HealthResponse>> {
    // In a real implementation, this would check database connectivity,
    // external service dependencies, etc.
    let response = HealthResponse {
        status: "ready".to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    };
    Ok(Json(response))
}

/// Version endpoint
pub async fn version() -> ServerResult<Json<VersionResponse>> {
    let response = VersionResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        build_info: BuildInfo {
            git_commit: option_env!("VERGEN_GIT_SHA").map(|s| s.to_string()),
            build_date: option_env!("VERGEN_BUILD_DATE").map(|s| s.to_string()),
        },
    };
    Ok(Json(response))
}
