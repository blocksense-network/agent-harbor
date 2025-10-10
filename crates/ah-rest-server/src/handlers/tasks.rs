//! Task management endpoints

use crate::state::AppState;
use crate::ServerResult;
use ah_rest_api_contract::{CreateTaskRequest, CreateTaskResponse, SessionStatus};
use axum::{extract::State, Json};
use uuid::Uuid;
// use validator::Validate; // Temporarily disabled due to version mismatch

/// Create a new task/session
#[utoipa::path(
    post,
    path = "/api/v1/tasks",
    request_body = CreateTaskRequest,
    responses(
        (status = 201, description = "Task created successfully", body = CreateTaskResponse),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Authentication required"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn create_task(
    State(state): State<AppState>,
    Json(request): Json<CreateTaskRequest>,
) -> ServerResult<Json<CreateTaskResponse>> {
    // Validate the request (temporarily disabled)
    // request.validate()?;

    // Generate a unique session ID
    let session_id = Uuid::new_v4().to_string();

    // Create the session in memory (placeholder - in real implementation,
    // this would create database records and start the actual task)
    let mut sessions = state.active_sessions.write().await;
    let session = ah_rest_api_contract::Session {
        id: session_id.clone(),
        tenant_id: request.tenant_id.clone(),
        project_id: request.project_id.clone(),
        task: ah_rest_api_contract::TaskInfo {
            prompt: request.prompt.clone(),
            attachments: Default::default(),
            labels: request.labels.clone(),
        },
        agent: request.agent.clone(),
        runtime: request.runtime.clone(),
        workspace: ah_rest_api_contract::WorkspaceInfo {
            snapshot_provider: "git".to_string(),     // placeholder
            mount_path: "/tmp/workspace".to_string(), // placeholder
            host: None,
            devcontainer_details: None,
        },
        vcs: ah_rest_api_contract::VcsInfo {
            repo_url: request.repo.url.as_ref().map(|u| u.to_string()),
            branch: request.repo.branch.clone(),
            commit: request.repo.commit.clone(),
        },
        status: SessionStatus::Queued,
        started_at: None,
        ended_at: None,
        links: ah_rest_api_contract::SessionLinks {
            self_link: format!("/api/v1/sessions/{}", session_id),
            events: format!("/api/v1/sessions/{}/events", session_id),
            logs: format!("/api/v1/sessions/{}/logs", session_id),
        },
    };
    sessions.insert(session_id.clone(), session);

    let response = CreateTaskResponse {
        id: session_id.clone(),
        status: SessionStatus::Queued,
        links: ah_rest_api_contract::TaskLinks {
            self_link: format!("/api/v1/sessions/{}", session_id),
            events: format!("/api/v1/sessions/{}/events", session_id),
            logs: format!("/api/v1/sessions/{}/logs", session_id),
        },
    };

    Ok(Json(response))
}
