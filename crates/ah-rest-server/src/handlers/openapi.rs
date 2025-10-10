//! OpenAPI specification and Swagger UI endpoints

use crate::ServerResult;
use axum::response::{Html, Json};
use utoipa::OpenApi;

/// OpenAPI specification endpoint
pub async fn openapi_spec() -> ServerResult<Json<utoipa::openapi::OpenApi>> {
    // This is a placeholder - the actual OpenAPI spec will be generated
    // from the handler function annotations using the utoipa derive macros
    // For now, return a minimal spec
    let spec = utoipa::openapi::OpenApi::new(
        utoipa::openapi::Info::new("Agent Harbor API", "0.1.0"),
        utoipa::openapi::Paths::new(),
    );
    Ok(Json(spec))
}

/// Swagger UI endpoint
pub async fn swagger_ui() -> Html<&'static str> {
    // This is a placeholder - in a real implementation, this would serve
    // the Swagger UI HTML page with the OpenAPI spec
    Html(
        r#"<!DOCTYPE html>
<html>
<head>
    <title>Agent Harbor API Documentation</title>
</head>
<body>
    <h1>Agent Harbor API</h1>
    <p>OpenAPI documentation will be available here.</p>
</body>
</html>"#,
    )
}
