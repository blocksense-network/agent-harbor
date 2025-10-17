//! agent-harbor REST API contract types and validation
//!
//! This crate defines the schema types and validation for the REST API
//! as specified in REST-Service.md. These types are shared between
//! the mock server, production server, and REST client implementations.

pub mod error;
pub mod types;
pub mod validation;

pub use error::*;
pub use types::*;

/// Generate OpenAPI schema for the API contract types
#[cfg(feature = "utoipa")]
pub fn openapi_schema() -> utoipa::openapi::OpenApi {
    use utoipa::OpenApi;
    #[derive(OpenApi)]
    #[openapi(
        info(title = "Agent Harbor REST API"),
        paths(),
        components(schemas(
            SessionStatus,
            RepoMode,
            RuntimeType,
            DeliveryMode,
            EventType,
            LogLevel,
            RepoConfig,
            RuntimeConfig,
            ResourceLimits,
            WorkspaceConfig,
            AgentConfig,
            DeliveryConfig,
            CreateTaskRequest,
            WebhookConfig,
            CreateTaskResponse,
            TaskLinks,
            Session,
            TaskInfo,
            WorkspaceInfo,
            DevcontainerInfo,
            VcsInfo,
            SessionLinks,
            SessionListResponse,
            SessionEvent,
            HostResult,
            DeliveryInfo,
            LogEntry,
            SessionLogsResponse,
            AgentCapability,
            RuntimeCapability,
            Executor,
            OverlayInfo,
            Project,
            Repository,
            Workspace,
            SessionInfoResponse,
            FleetInfo,
            FollowerInfo,
            SessionEndpoints,
            SessionControlRequest,
            PaginationQuery,
            FilterQuery,
            LogQuery,
            IdempotencyKey,
            ProblemDetails
        ))
    )]
    struct ApiDoc;
    ApiDoc::openapi()
}
