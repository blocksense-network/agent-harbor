//! OpenAPI specification and Swagger UI endpoints

use crate::ServerResult;
use axum::response::{Html, Json};
use utoipa::OpenApi;

/// Main OpenAPI specification for Agent Harbor REST API
#[derive(OpenApi)]
#[openapi(
    paths(
        crate::handlers::health::health_check,
        crate::handlers::health::readiness_check,
        crate::handlers::health::version,
    ),
    components(
        schemas(
            crate::handlers::health::HealthResponse,
            crate::handlers::health::VersionResponse,
            crate::handlers::health::BuildInfo,
        )
    ),
    info(
        title = "Agent Harbor REST API",
        version = "0.1.0",
        description = "REST API for Agent Harbor - AI-powered coding sessions and task orchestration",
        contact(
            name = "Agent Harbor Team"
        ),
        license(
            name = "MIT OR Apache-2.0"
        )
    ),
    servers(
        (url = "http://localhost:3001", description = "Local development server"),
        (url = "https://api.agent-harbor.dev", description = "Production server")
    )
)]
pub struct ApiDoc;

/// OpenAPI specification endpoint
#[utoipa::path(
    get,
    path = "/api/v1/openapi.json",
    responses(
        (status = 200, description = "OpenAPI specification", body = utoipa::openapi::OpenApi)
    )
)]
pub async fn openapi_spec() -> ServerResult<Json<utoipa::openapi::OpenApi>> {
    Ok(Json(ApiDoc::openapi()))
}

/// Swagger UI endpoint
#[utoipa::path(
    get,
    path = "/api/docs/",
    responses(
        (status = 200, description = "Swagger UI interface", content_type = "text/html")
    )
)]
pub async fn swagger_ui() -> Html<&'static str> {
    Html(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <meta name="description" content="Agent Harbor REST API Documentation" />
    <title>Agent Harbor REST API - Swagger UI</title>
    <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5.7.2/swagger-ui.css" />
</head>
<body>
<div id="swagger-ui"></div>
<script src="https://unpkg.com/swagger-ui-dist@5.7.2/swagger-ui-bundle.js" crossorigin></script>
<script src="https://unpkg.com/swagger-ui-dist@5.7.2/swagger-ui-standalone-preset.js" crossorigin></script>
<script>
    window.onload = () => {
        window.ui = SwaggerUIBundle({
            url: '/api/v1/openapi.json',
            dom_id: '#swagger-ui',
            deepLinking: true,
            presets: [
                SwaggerUIBundle.presets.apis,
                SwaggerUIStandalonePreset
            ],
            plugins: [
                SwaggerUIBundle.plugins.DownloadUrl
            ],
            layout: "StandaloneLayout"
        });
    };
</script>
</body>
</html>"#,
    )
}
