//! REST API client for agent-harbor service
//!
//! This crate provides a complete HTTP client for the agent-harbor REST API
//! as specified in REST-Service.md. It includes support for authentication,
//! request/response handling, and SSE streaming for real-time updates.

pub mod auth;
pub mod client;
pub mod error;
pub mod sse;

pub use auth::*;
pub use client::*;
pub use error::*;

use async_trait::async_trait;
use ah_client_api::{ClientApi, ClientApiError, ClientApiResult};
use ah_rest_api_contract::*;

#[async_trait]
impl ClientApi for client::RestClient {
    async fn list_projects(&self, tenant_id: Option<&str>) -> ClientApiResult<Vec<Project>> {
        self.list_projects(tenant_id)
            .await
            .map_err(|e| ClientApiError::Server(e.to_string()))
    }

    async fn list_repositories(
        &self,
        tenant_id: Option<&str>,
        project_id: Option<&str>,
    ) -> ClientApiResult<Vec<Repository>> {
        self.list_repositories(tenant_id, project_id)
            .await
            .map_err(|e| ClientApiError::Server(e.to_string()))
    }

    async fn list_agents(&self) -> ClientApiResult<Vec<AgentCapability>> {
        self.list_agents().await.map_err(|e| ClientApiError::Server(e.to_string()))
    }

    async fn create_task(
        &self,
        request: &CreateTaskRequest,
    ) -> ClientApiResult<CreateTaskResponse> {
        self.create_task(request)
            .await
            .map_err(|e| ClientApiError::Server(e.to_string()))
    }
}
