//! Mock REST client backed by scenarios

use async_trait::async_trait;
use ah_client_api::{ClientApi, ClientApiError, ClientApiResult};
use ah_rest_api_contract::*;

pub struct MockClient {
    _scenario_name: String,
}

impl MockClient {
    pub fn from_scenario_name(name: impl Into<String>) -> Self {
        Self {
            _scenario_name: name.into(),
        }
    }
}

#[async_trait]
impl ClientApi for MockClient {
    async fn list_projects(&self, _tenant_id: Option<&str>) -> ClientApiResult<Vec<Project>> {
        Ok(vec![
            Project {
                id: "p1".into(),
                display_name: "Demo Project 1".into(),
                last_used_at: None,
            },
            Project {
                id: "p2".into(),
                display_name: "Demo Project 2".into(),
                last_used_at: None,
            },
        ])
    }

    async fn list_repositories(
        &self,
        _tenant_id: Option<&str>,
        _project_id: Option<&str>,
    ) -> ClientApiResult<Vec<Repository>> {
        use url::Url;
        Ok(vec![
            Repository {
                id: "r1".into(),
                display_name: "demo/repo1".into(),
                scm_provider: "github".into(),
                remote_url: Url::parse("https://github.com/demo/repo1").unwrap(),
                default_branch: "main".into(),
                last_used_at: None,
            },
            Repository {
                id: "r2".into(),
                display_name: "demo/repo2".into(),
                scm_provider: "github".into(),
                remote_url: Url::parse("https://github.com/demo/repo2").unwrap(),
                default_branch: "main".into(),
                last_used_at: None,
            },
        ])
    }

    async fn list_agents(&self) -> ClientApiResult<Vec<AgentCapability>> {
        Ok(vec![
            AgentCapability {
                agent_type: "claude-code".into(),
                versions: vec!["latest".into()],
                settings_schema_ref: None,
            },
            AgentCapability {
                agent_type: "gpt-engineer".into(),
                versions: vec!["v1.0".into()],
                settings_schema_ref: None,
            },
        ])
    }

    async fn create_task(
        &self,
        _request: &CreateTaskRequest,
    ) -> ClientApiResult<CreateTaskResponse> {
        Err(ClientApiError::Unexpected("not implemented in mock".into()))
    }
}
