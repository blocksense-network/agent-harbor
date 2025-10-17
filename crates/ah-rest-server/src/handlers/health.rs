//! Health check endpoints

use crate::ServerResult;
use axum::Json;
use serde::Serialize;
use utoipa::ToSchema;

/// Health check response
#[derive(Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: String,
    pub timestamp: String,
}

/// Version response
#[derive(Serialize, ToSchema)]
pub struct VersionResponse {
    pub version: String,
    pub build_info: BuildInfo,
}

/// Build information
#[derive(Serialize, ToSchema)]
pub struct BuildInfo {
    pub git_commit: Option<String>,
    pub build_date: Option<String>,
}

/// Health check endpoint
#[utoipa::path(
    get,
    path = "/api/v1/healthz",
    responses(
        (status = 200, description = "Server is healthy", body = HealthResponse)
    )
)]
pub async fn health_check() -> ServerResult<Json<HealthResponse>> {
    let response = HealthResponse {
        status: "ok".to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    };
    Ok(Json(response))
}

/// Readiness check endpoint
#[utoipa::path(
    get,
    path = "/api/v1/readyz",
    responses(
        (status = 200, description = "Server is ready to accept requests", body = HealthResponse),
        (status = 503, description = "Server is not ready", body = HealthResponse)
    )
)]
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
#[utoipa::path(
    get,
    path = "/api/v1/version",
    responses(
        (status = 200, description = "Server version information", body = VersionResponse)
    )
)]
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
