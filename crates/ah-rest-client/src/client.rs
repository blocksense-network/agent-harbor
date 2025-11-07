// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Main REST API client implementation

use ah_rest_api_contract::*;
use reqwest::{Client as HttpClient, Method, Response};
use serde::{Deserialize, de::DeserializeOwned};
use url::Url;

use crate::auth::AuthConfig;
use crate::error::{RestClientError, RestClientResult};
use crate::sse::SessionEventStream;

/// REST API client for agent-harbor service
#[derive(Debug, Clone)]
pub struct RestClient {
    http_client: HttpClient,
    base_url: Url,
    auth: AuthConfig,
}

impl RestClient {
    /// Create a new REST client
    pub fn new(base_url: Url, auth: AuthConfig) -> Self {
        let http_client = HttpClient::builder()
            .user_agent("ah-tui/1.0")
            .build()
            .expect("Failed to create HTTP client");

        Self {
            http_client,
            base_url,
            auth,
        }
    }

    /// Create a client from a base URL string
    pub fn from_url(base_url: &str, auth: AuthConfig) -> RestClientResult<Self> {
        let base_url = Url::parse(base_url)?;
        Ok(Self::new(base_url, auth))
    }

    /// Get the base URL
    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    /// Get the authentication config
    pub fn auth(&self) -> &AuthConfig {
        &self.auth
    }

    /// Create a task/session
    pub async fn create_task(
        &self,
        request: &CreateTaskRequest,
    ) -> RestClientResult<CreateTaskResponse> {
        self.post("/api/v1/tasks", request).await
    }

    /// List sessions with optional filtering
    pub async fn list_sessions(
        &self,
        filters: Option<&FilterQuery>,
    ) -> RestClientResult<SessionListResponse> {
        let mut url = self.base_url.join("/api/v1/sessions")?;

        if let Some(filters) = filters {
            let query_params = self.build_query_params(filters);
            url.set_query(Some(&query_params));
        }

        self.get(url.as_ref()).await
    }

    /// Get a specific session
    pub async fn get_session(&self, session_id: &str) -> RestClientResult<Session> {
        let url = format!("/api/v1/sessions/{}", session_id);
        self.get(&url).await
    }

    /// Stop a session
    pub async fn stop_session(
        &self,
        session_id: &str,
        reason: Option<&str>,
    ) -> RestClientResult<()> {
        let url = format!("/api/v1/sessions/{}/stop", session_id);
        let body = reason.map(|r| SessionControlRequest {
            reason: Some(r.to_string()),
        });
        self.post(&url, &body).await
    }

    /// Cancel a session (force terminate)
    pub async fn cancel_session(&self, session_id: &str) -> RestClientResult<()> {
        let url = format!("/api/v1/sessions/{}", session_id);
        self.delete(&url).await
    }

    /// Pause a session
    pub async fn pause_session(&self, session_id: &str) -> RestClientResult<()> {
        let url = format!("/api/v1/sessions/{}/pause", session_id);
        self.post_empty(&url).await
    }

    /// Resume a session
    pub async fn resume_session(&self, session_id: &str) -> RestClientResult<()> {
        let url = format!("/api/v1/sessions/{}/resume", session_id);
        self.post_empty(&url).await
    }

    /// Get session logs
    pub async fn get_session_logs(
        &self,
        session_id: &str,
        query: Option<&LogQuery>,
    ) -> RestClientResult<SessionLogsResponse> {
        let mut url = format!("/api/v1/sessions/{}/logs", session_id);

        if let Some(query) = query {
            let query_params = self.build_query_params(query);
            url.push('?');
            url.push_str(&query_params);
        }

        self.get(&url).await
    }

    /// Stream session events via SSE
    pub async fn stream_session_events(
        &self,
        session_id: &str,
    ) -> RestClientResult<SessionEventStream> {
        SessionEventStream::connect(&self.base_url, session_id, &self.auth).await
    }

    /// Get session info (fleet and endpoints)
    pub async fn get_session_info(
        &self,
        session_id: &str,
    ) -> RestClientResult<SessionInfoResponse> {
        let url = format!("/api/v1/sessions/{}/info", session_id);
        self.get(&url).await
    }

    /// List available agents
    pub async fn list_agents(&self) -> RestClientResult<Vec<AgentCapability>> {
        #[derive(Deserialize)]
        struct AgentListResponse {
            items: Vec<AgentCapability>,
        }
        let response: AgentListResponse = self.get("/api/v1/agents").await?;
        Ok(response.items)
    }

    /// List available runtimes
    pub async fn list_runtimes(&self) -> RestClientResult<Vec<RuntimeCapability>> {
        #[derive(Deserialize)]
        struct RuntimeListResponse {
            items: Vec<RuntimeCapability>,
        }
        let response: RuntimeListResponse = self.get("/api/v1/runtimes").await?;
        Ok(response.items)
    }

    /// List executors
    pub async fn list_executors(&self) -> RestClientResult<Vec<Executor>> {
        #[derive(Deserialize)]
        struct ExecutorListResponse {
            items: Vec<Executor>,
        }
        let response: ExecutorListResponse = self.get("/api/v1/executors").await?;
        Ok(response.items)
    }

    /// List projects
    pub async fn list_projects(&self, tenant_id: Option<&str>) -> RestClientResult<Vec<Project>> {
        let mut url = "/api/v1/projects".to_string();
        if let Some(tenant_id) = tenant_id {
            url.push('?');
            url.push_str(&format!("tenantId={}", tenant_id));
        }
        self.get(&url).await
    }

    /// List repositories
    pub async fn list_repositories(
        &self,
        tenant_id: Option<&str>,
        project_id: Option<&str>,
    ) -> RestClientResult<Vec<ah_rest_api_contract::Repository>> {
        let mut url = "/api/v1/repositories".to_string();
        let mut params = Vec::new();

        if let Some(tenant_id) = tenant_id {
            params.push(format!("tenantId={}", tenant_id));
        }
        if let Some(project_id) = project_id {
            params.push(format!("projectId={}", project_id));
        }

        if !params.is_empty() {
            url.push('?');
            url.push_str(&params.join("&"));
        }

        self.get(&url).await
    }

    /// Get branches for a repository
    pub async fn get_repository_branches(
        &self,
        repository_id: &str,
    ) -> RestClientResult<Vec<BranchInfo>> {
        let url = format!("/api/v1/repositories/{}/branches", repository_id);
        let response: RepositoryBranchesResponse = self.get(&url).await?;
        Ok(response.branches)
    }

    pub async fn get_repository_files(
        &self,
        repository_id: &str,
    ) -> RestClientResult<Vec<RepositoryFile>> {
        let url = format!("/api/v1/repositories/{}/files", repository_id);
        let response: RepositoryFilesResponse = self.get(&url).await?;
        Ok(response.files)
    }

    /// List workspaces
    pub async fn list_workspaces(&self, status: Option<&str>) -> RestClientResult<Vec<Workspace>> {
        let mut url = "/api/v1/workspaces".to_string();
        if let Some(status) = status {
            url.push('?');
            url.push_str(&format!("status={}", status));
        }
        self.get(&url).await
    }

    /// Get workspace details
    pub async fn get_workspace(&self, workspace_id: &str) -> RestClientResult<Workspace> {
        let url = format!("/api/v1/workspaces/{}", workspace_id);
        self.get(&url).await
    }

    /// Save a draft task
    pub async fn save_draft_task(
        &self,
        draft_id: &str,
        description: &str,
        repository: &str,
        branch: &str,
        models: &[ah_domain_types::SelectedModel],
    ) -> RestClientResult<()> {
        // Convert selected models to agent config
        let agent = if let Some(first_model) = models.first() {
            AgentConfig {
                agent_type: first_model.name.clone(),
                version: "latest".to_string(),
                settings: std::collections::HashMap::new(),
            }
        } else {
            return Err(RestClientError::UnexpectedResponse(
                "No model selected".to_string(),
            ));
        };

        // Create repo config
        let repo = RepoConfig {
            mode: RepoMode::Git,
            url: Some(url::Url::parse(repository).map_err(|e| {
                RestClientError::UnexpectedResponse(format!("Invalid repository URL: {}", e))
            })?),
            branch: Some(branch.to_string()),
            commit: None,
        };

        // Create the draft request
        let draft_request = CreateTaskRequest {
            tenant_id: None,
            project_id: None,
            prompt: description.to_string(),
            repo,
            agent,
            runtime: RuntimeConfig {
                runtime_type: RuntimeType::Local,
                devcontainer_path: None,
                resources: None,
            },
            workspace: None,
            delivery: None,
            labels: std::collections::HashMap::new(),
            webhooks: vec![],
        };

        // If draft_id is "draft-123" (placeholder), this is a new draft, so POST to create
        // Otherwise, it's an update, so PUT to the specific draft ID
        if draft_id == "draft-123" || draft_id.is_empty() {
            // Create new draft
            let _: serde_json::Value = self.post("/api/v1/drafts", &draft_request).await?;
        } else {
            // Update existing draft
            let url = format!("/api/v1/drafts/{}", draft_id);
            let _: serde_json::Value = self.put(&url, &draft_request).await?;
        }

        Ok(())
    }

    // Private helper methods

    async fn get<T: DeserializeOwned>(&self, path: &str) -> RestClientResult<T> {
        self.request(Method::GET, path, None::<&()>).await
    }

    async fn post<T: DeserializeOwned, B: serde::Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> RestClientResult<T> {
        self.request(Method::POST, path, Some(body)).await
    }

    async fn put<T: DeserializeOwned, B: serde::Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> RestClientResult<T> {
        self.request(Method::PUT, path, Some(body)).await
    }

    async fn post_empty(&self, path: &str) -> RestClientResult<()> {
        self.request(Method::POST, path, Some(&())).await
    }

    async fn delete<T: DeserializeOwned>(&self, path: &str) -> RestClientResult<T> {
        self.request(Method::DELETE, path, None::<&()>).await
    }

    async fn request<T: DeserializeOwned, B: serde::Serialize>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
    ) -> RestClientResult<T> {
        let url = if path.starts_with("http") {
            path.to_string()
        } else {
            self.base_url.join(path)?.to_string()
        };

        let mut request = self.http_client.request(method, &url);

        // Add authentication headers
        let auth_headers = self.auth.headers().map_err(|e| RestClientError::Auth(e.to_string()))?;
        request = request.headers(auth_headers);

        // Add body if provided
        if let Some(body) = body {
            request = request.json(body);
        }

        let response = request.send().await?;
        self.handle_response(response).await
    }

    async fn handle_response<T: DeserializeOwned>(
        &self,
        response: Response,
    ) -> RestClientResult<T> {
        let status = response.status();

        if status.is_success() {
            let text = response.text().await?;
            serde_json::from_str(&text).map_err(RestClientError::from)
        } else {
            let text = response.text().await?;
            match serde_json::from_str::<ProblemDetails>(&text) {
                Ok(problem) => Err(RestClientError::ServerError {
                    status,
                    details: problem,
                }),
                Err(_) => Err(RestClientError::UnexpectedResponse(text)),
            }
        }
    }

    fn build_query_params<T: serde::Serialize>(&self, params: &T) -> String {
        let mut pairs = Vec::new();
        let value = serde_json::to_value(params).unwrap();

        if let serde_json::Value::Object(map) = value {
            for (key, val) in map {
                if !val.is_null() {
                    let val_str = match val {
                        serde_json::Value::String(s) => s,
                        serde_json::Value::Number(n) => n.to_string(),
                        serde_json::Value::Bool(b) => b.to_string(),
                        _ => val.to_string().trim_matches('"').to_string(),
                    };
                    pairs.push(format!("{}={}", key, val_str));
                }
            }
        }

        pairs.join("&")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ah_rest_api_contract::*;

    #[tokio::test]
    async fn test_client_creation() {
        let base_url = "http://localhost:3001";
        let auth = AuthConfig::default();
        let client = RestClient::from_url(base_url, auth).unwrap();

        assert_eq!(client.base_url().to_string(), format!("{}/", base_url));
    }

    #[test]
    fn test_query_params_building() {
        let client = RestClient::from_url("http://localhost:3001", AuthConfig::default()).unwrap();

        let filters = FilterQuery {
            status: Some("running".to_string()),
            agent: Some("claude-code".to_string()),
            project_id: None,
            tenant_id: None,
        };

        let params = client.build_query_params(&filters);
        assert!(params.contains("status=running"));
        assert!(params.contains("agent=claude-code"));
    }
}
