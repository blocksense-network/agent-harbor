// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use ah_domain_types::{AgentChoice, AgentSoftware, AgentSoftwareBuild};
use ah_local_db::Database;
use ah_rest_api_contract::{CreateTaskRequest, RepoConfig, RepoMode, RuntimeConfig, RuntimeType};
use ah_rest_server::{
    executor::TaskExecutor,
    models::{DatabaseSessionStore, SessionStore},
};
use std::sync::Arc;

fn make_request() -> CreateTaskRequest {
    CreateTaskRequest {
        tenant_id: None,
        project_id: None,
        prompt: "pause me".into(),
        repo: RepoConfig {
            mode: RepoMode::None,
            url: None,
            branch: None,
            commit: None,
        },
        runtime: RuntimeConfig {
            runtime_type: RuntimeType::Local,
            devcontainer_path: None,
            resources: None,
        },
        workspace: None,
        agents: vec![AgentChoice {
            agent: AgentSoftwareBuild {
                software: AgentSoftware::Claude,
                version: "latest".into(),
            },
            model: "sonnet".into(),
            count: 1,
            settings: std::collections::HashMap::new(),
            display_name: Some("sonnet".into()),
            acp_stdio_launch_command: None,
        }],
        delivery: None,
        labels: std::collections::HashMap::new(),
        webhooks: Vec::new(),
    }
}

#[tokio::test]
async fn task_controller_pause_resume_updates_status() {
    let db = Arc::new(Database::open_in_memory().expect("db"));
    let session_store = Arc::new(DatabaseSessionStore::new(Arc::clone(&db)));
    let executor = TaskExecutor::new(Arc::clone(&db), Arc::clone(&session_store), None);

    let request = make_request();
    let ids = session_store.create_session(&request).await.expect("session create");
    let session_id = ids.first().cloned().expect("id");

    executor.pause_task(&session_id).await.expect("pause");
    let paused = session_store.get_session(&session_id).await.unwrap().unwrap();
    assert_eq!(paused.session.status.to_string(), "paused");

    executor.resume_task(&session_id).await.expect("resume");
    let resumed = session_store.get_session(&session_id).await.unwrap().unwrap();
    assert_eq!(resumed.session.status.to_string(), "running");
}

#[tokio::test]
async fn task_controller_inject_message_stub_is_non_fatal() {
    let db = Arc::new(Database::open_in_memory().expect("db"));
    let session_store = Arc::new(DatabaseSessionStore::new(Arc::clone(&db)));
    let executor = TaskExecutor::new(Arc::clone(&db), Arc::clone(&session_store), None);

    let request = make_request();
    let ids = session_store.create_session(&request).await.expect("session create");
    let session_id = ids.first().cloned().expect("id");

    // Should not error even though backend injection is stubbed.
    executor.inject_message(&session_id, "hello world").await.expect("inject");

    // ensure the log was recorded as a session event
    // Database-backed event storage is a TODO; the call must still succeed.
}
