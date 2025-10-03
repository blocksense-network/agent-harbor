//! Validation helpers for API contract types

use crate::error::ApiContractError;
use crate::types::*;
use validator::Validate;

/// Validate a create task request
pub fn validate_create_task_request(request: &CreateTaskRequest) -> Result<(), ApiContractError> {
    request.validate()?;
    Ok(())
}

/// Validate agent configuration
pub fn validate_agent_config(config: &AgentConfig) -> Result<(), ApiContractError> {
    config.validate()?;
    Ok(())
}

/// Validate runtime configuration
pub fn validate_runtime_config(config: &RuntimeConfig) -> Result<(), ApiContractError> {
    config.validate()?;
    Ok(())
}

/// Validate repository configuration
pub fn validate_repo_config(config: &RepoConfig) -> Result<(), ApiContractError> {
    config.validate()?;

    // Additional validation logic
    match config.mode {
        RepoMode::Git => {
            if config.url.is_none() {
                return Err(ApiContractError::Validation(
                    validator::ValidationErrors::new(),
                ));
            }
        }
        RepoMode::Upload | RepoMode::None => {
            // URL is optional for these modes
        }
    }

    Ok(())
}

/// Validate URL format
pub fn validate_url(url_str: &str) -> Result<(), ApiContractError> {
    url::Url::parse(url_str)?;
    Ok(())
}

/// Validate UUID format
pub fn validate_uuid(uuid_str: &str) -> Result<(), ApiContractError> {
    uuid::Uuid::parse_str(uuid_str)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;

    #[test]
    fn test_validate_create_task_request_valid() {
        let request = CreateTaskRequest {
            tenant_id: Some("acme".to_string()),
            project_id: Some("storefront".to_string()),
            prompt: "Fix the bug".to_string(),
            repo: RepoConfig {
                mode: RepoMode::Git,
                url: Some("https://github.com/acme/storefront.git".parse().unwrap()),
                branch: Some("main".to_string()),
                commit: None,
            },
            runtime: RuntimeConfig {
                runtime_type: RuntimeType::Devcontainer,
                devcontainer_path: Some(".devcontainer/devcontainer.json".to_string()),
                resources: None,
            },
            workspace: None,
            agent: AgentConfig {
                agent_type: "claude-code".to_string(),
                version: "latest".to_string(),
                settings: Default::default(),
            },
            delivery: None,
            labels: Default::default(),
            webhooks: vec![],
        };

        assert!(validate_create_task_request(&request).is_ok());
    }

    #[test]
    fn test_validate_create_task_request_empty_prompt() {
        let request = CreateTaskRequest {
            tenant_id: Some("acme".to_string()),
            project_id: Some("storefront".to_string()),
            prompt: "".to_string(), // Invalid: empty prompt
            repo: RepoConfig {
                mode: RepoMode::Git,
                url: Some("https://github.com/acme/storefront.git".parse().unwrap()),
                branch: Some("main".to_string()),
                commit: None,
            },
            runtime: RuntimeConfig {
                runtime_type: RuntimeType::Devcontainer,
                devcontainer_path: Some(".devcontainer/devcontainer.json".to_string()),
                resources: None,
            },
            workspace: None,
            agent: AgentConfig {
                agent_type: "claude-code".to_string(),
                version: "latest".to_string(),
                settings: Default::default(),
            },
            delivery: None,
            labels: Default::default(),
            webhooks: vec![],
        };

        assert!(validate_create_task_request(&request).is_err());
    }

    #[test]
    fn test_validate_repo_config_git_without_url() {
        let config = RepoConfig {
            mode: RepoMode::Git,
            url: None, // Invalid: Git mode requires URL
            branch: Some("main".to_string()),
            commit: None,
        };

        assert!(validate_repo_config(&config).is_err());
    }
}
