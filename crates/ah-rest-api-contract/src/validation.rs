// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Validation helpers for API contract types

use crate::error::ApiContractError;
use crate::types::*;
use ah_domain_types::AgentChoice;
use validator::Validate;

/// Validate a create task request
pub fn validate_create_task_request(request: &CreateTaskRequest) -> Result<(), ApiContractError> {
    request.validate()?;
    Ok(())
}

/// Validate agent configuration
pub fn validate_agent_config(config: &AgentChoice) -> Result<(), ApiContractError> {
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

/// Validate ULID format for idempotency keys
pub fn validate_ulid(ulid_str: &str) -> Result<(), ApiContractError> {
    // ULID is 26 characters, base32 encoded
    if ulid_str.len() != 26 {
        return Err(ApiContractError::InvalidUlid(format!(
            "ULID must be 26 characters, got {}",
            ulid_str.len()
        )));
    }
    // Basic check for valid base32 characters (ULIDs use Crockford base32)
    // ULIDs allow 0-9 and A-Z (uppercase)
    for c in ulid_str.chars() {
        if !c.is_ascii_uppercase() && !c.is_ascii_digit() {
            return Err(ApiContractError::InvalidUlid(format!(
                "ULID contains invalid character '{}': {}",
                c, ulid_str
            )));
        }
    }
    Ok(())
}

/// Validate idempotency key
pub fn validate_idempotency_key(key: &IdempotencyKey) -> Result<(), ApiContractError> {
    validate_ulid(&key.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProblemDetails;
    use crate::types::*;
    use ah_domain_types::{AgentSoftware, AgentSoftwareBuild};
    use serde_json;

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
            agents: vec![AgentChoice {
                agent: AgentSoftwareBuild {
                    software: AgentSoftware::Claude,
                    version: "latest".to_string(),
                },
                model: "sonnet".to_string(),
                count: 1,
                settings: Default::default(),
                display_name: None,
            }],
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
            agents: vec![AgentChoice {
                agent: AgentSoftwareBuild {
                    software: AgentSoftware::Claude,
                    version: "latest".to_string(),
                },
                model: "sonnet".to_string(),
                count: 1,
                settings: Default::default(),
                display_name: None,
            }],
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

    #[test]
    fn test_validate_ulid_valid() {
        let valid_ulid = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
        let result = validate_ulid(valid_ulid);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_ulid_invalid_length() {
        let invalid_ulid = "01HVZ6K9T1N8S6M3V3Q3F0X5B78"; // 27 chars
        assert!(validate_ulid(invalid_ulid).is_err());
    }

    #[test]
    fn test_validate_ulid_invalid_chars() {
        let invalid_ulid = "01HVZ6K9T1N8S6M3V3Q3F0X5@"; // Contains @
        assert!(validate_ulid(invalid_ulid).is_err());
    }

    #[test]
    fn test_validate_idempotency_key_valid() {
        let key = IdempotencyKey("01ARZ3NDEKTSV4RRFFQ69G5FAV".to_string());
        assert!(validate_idempotency_key(&key).is_ok());
    }

    #[test]
    fn test_validate_idempotency_key_invalid() {
        let key = IdempotencyKey("invalid-key".to_string());
        assert!(validate_idempotency_key(&key).is_err());
    }

    #[test]
    fn test_serialization_roundtrip_create_task_request() {
        let original = CreateTaskRequest {
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
            agents: vec![AgentChoice {
                agent: AgentSoftwareBuild {
                    software: AgentSoftware::Claude,
                    version: "latest".to_string(),
                },
                model: "sonnet".to_string(),
                count: 1,
                settings: Default::default(),
                display_name: None,
            }],
            delivery: None,
            labels: Default::default(),
            webhooks: vec![],
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: CreateTaskRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_serialization_roundtrip_session_event() {
        let original = SessionEvent::thought(
            "Analyzing the code".to_string(),
            Some("Need to understand the structure".to_string()),
            chrono::Utc::now().timestamp() as u64,
        );

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: SessionEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_problem_details_serialization() {
        let problem = ProblemDetails {
            problem_type: "https://docs.example.com/errors/validation".to_string(),
            title: "Invalid request".to_string(),
            status: Some(400),
            detail: "repo.url must be provided when repo.mode=git".to_string(),
            errors: std::collections::HashMap::from([(
                "repo.url".to_string(),
                vec!["is required".to_string()],
            )]),
        };

        let json = serde_json::to_string(&problem).unwrap();
        let deserialized: ProblemDetails = serde_json::from_str(&json).unwrap();
        assert_eq!(problem, deserialized);
    }

    #[test]
    fn test_pagination_query_edge_cases() {
        // Test empty pagination (should use defaults)
        let query = PaginationQuery {
            page: None,
            per_page: None,
        };
        let json = serde_json::to_string(&query).unwrap();
        let deserialized: PaginationQuery = serde_json::from_str(&json).unwrap();
        assert_eq!(query, deserialized);

        // Test with values
        let query_with_values = PaginationQuery {
            page: Some(2),
            per_page: Some(50),
        };
        let json = serde_json::to_string(&query_with_values).unwrap();
        let deserialized: PaginationQuery = serde_json::from_str(&json).unwrap();
        assert_eq!(query_with_values, deserialized);
    }
}
