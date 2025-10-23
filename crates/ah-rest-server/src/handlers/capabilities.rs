// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Capability discovery endpoints

use crate::ServerResult;
use ah_rest_api_contract::{AgentCapability, Executor, RuntimeCapability, RuntimeType};
use axum::Json;

/// List available agents
pub async fn list_agents() -> ServerResult<Json<Vec<AgentCapability>>> {
    let agents = vec![
        AgentCapability {
            agent_type: "claude-code".to_string(),
            versions: vec!["latest".to_string()],
            settings_schema_ref: Some("/api/v1/schemas/agents/claude-code.json".to_string()),
        },
        AgentCapability {
            agent_type: "openhands".to_string(),
            versions: vec!["latest".to_string()],
            settings_schema_ref: Some("/api/v1/schemas/agents/openhands.json".to_string()),
        },
    ];

    Ok(Json(agents))
}

/// List available runtimes
pub async fn list_runtimes() -> ServerResult<Json<Vec<RuntimeCapability>>> {
    let runtimes = vec![
        RuntimeCapability {
            runtime_type: RuntimeType::Devcontainer,
            images: vec!["ghcr.io/devcontainers/base:ubuntu".to_string()],
            paths: vec![".devcontainer/devcontainer.json".to_string()],
            sandbox_profiles: vec!["default".to_string(), "disabled".to_string()],
        },
        RuntimeCapability {
            runtime_type: RuntimeType::Local,
            images: vec![],
            paths: vec![],
            sandbox_profiles: vec!["default".to_string(), "disabled".to_string()],
        },
    ];

    Ok(Json(runtimes))
}

/// List available executors
pub async fn list_executors() -> ServerResult<Json<Vec<Executor>>> {
    let executors = vec![Executor {
        id: "local-linux".to_string(),
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        snapshot_capabilities: vec!["git".to_string(), "zfs".to_string(), "btrfs".to_string()],
        health: "healthy".to_string(),
        overlay: None,
    }];

    Ok(Json(executors))
}
