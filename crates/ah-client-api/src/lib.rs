//! Client API trait for AH TUI

use ah_rest_api_contract::*;
use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ClientApiError {
    #[error("server error: {0}")]
    Server(String),
    #[error("unexpected: {0}")]
    Unexpected(String),
}

pub type ClientApiResult<T> = Result<T, ClientApiError>;

#[async_trait]
pub trait ClientApi: Send + Sync {
    async fn list_projects(&self, tenant_id: Option<&str>) -> ClientApiResult<Vec<Project>>;
    async fn list_repositories(
        &self,
        tenant_id: Option<&str>,
        project_id: Option<&str>,
    ) -> ClientApiResult<Vec<Repository>>;
    async fn list_agents(&self) -> ClientApiResult<Vec<AgentCapability>>;

    async fn create_task(&self, request: &CreateTaskRequest)
        -> ClientApiResult<CreateTaskResponse>;
}
