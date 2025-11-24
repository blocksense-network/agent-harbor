// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use std::{
    collections::HashMap,
    net::TcpListener,
    path::PathBuf,
    time::{Duration, Instant},
};

use ah_core::{
    GenericRestTaskManager, SplitMode, TaskLaunchParams, TaskManager,
    task_manager::TaskLaunchResult,
};
use ah_domain_types::{AgentChoice, AgentSoftware, AgentSoftwareBuild};
use ah_rest_api_contract::{
    CreateTaskRequest, RepoConfig, RepoMode, RuntimeConfig, RuntimeType, SessionEvent,
    SessionStatus,
};
use ah_rest_client::{AuthConfig, RestClient};
use ah_rest_server::{
    Server, ServerConfig,
    mock_dependencies::{MockServerDependencies, ScenarioPlaybackOptions},
};
use futures::StreamExt;
use reqwest::Client;
use tokio::task::JoinHandle;
use tokio::time::timeout;
use url::Url;

async fn spawn_mock_server() -> (String, JoinHandle<()>) {
    spawn_mock_server_with_options(None).await
}

async fn spawn_mock_server_with_options(
    playback: Option<ScenarioPlaybackOptions>,
) -> (String, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind to ephemeral port");
    let addr = listener.local_addr().expect("port");
    drop(listener);

    let config = ServerConfig {
        bind_addr: addr,
        enable_cors: true,
        ..Default::default()
    };

    let deps = if let Some(opts) = playback {
        MockServerDependencies::with_options(config.clone(), opts)
            .await
            .expect("mock deps")
    } else {
        MockServerDependencies::new(config.clone()).await.expect("mock deps")
    };
    let server = Server::with_state(config, deps.into_state()).expect("server");
    let bind = format!("http://{}", addr);

    let handle = tokio::spawn(async move {
        server.run().await.expect("server run");
    });

    wait_for_health(&bind).await;

    (bind, handle)
}

async fn wait_for_health(base_url: &str) {
    let client = reqwest::Client::new();
    let healthz = format!("{}/api/v1/healthz", base_url);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(response) = client.get(&healthz).send().await {
            if response.status().is_success() {
                return;
            }
        }
        if tokio::time::Instant::now() > deadline {
            panic!("mock server did not become healthy at {}", healthz);
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

#[tokio::test]
async fn rest_client_lists_mock_sessions() {
    let (base_url, handle) = spawn_mock_server().await;
    let client = RestClient::from_url(&base_url, AuthConfig::default()).expect("client");

    let request = sample_task_request("List sessions smoke check");
    client.create_task(&request).await.expect("create mock task");

    let response = client.list_sessions(None).await.expect("list sessions");

    assert!(
        !response.items.is_empty(),
        "expected mock server to return sessions"
    );

    handle.abort();
}

#[tokio::test]
async fn remote_task_manager_launches_task_against_mock_server() {
    let (base_url, handle) = spawn_mock_server().await;

    let rest_client = RestClient::from_url(&base_url, AuthConfig::default()).expect("client");
    let task_manager = GenericRestTaskManager::new(rest_client.clone());

    let agent_choice = AgentChoice {
        agent: AgentSoftwareBuild {
            software: AgentSoftware::Claude,
            version: "latest".to_string(),
        },
        model: "sonnet".to_string(),
        count: 1,
        settings: std::collections::HashMap::new(),
        display_name: Some("Claude 3.5 Sonnet".to_string()),
    };

    let params = TaskLaunchParams::builder()
        .repository("https://github.com/agent-harbor/mock.git".to_string())
        .branch("main".to_string())
        .description("Verify mock task launch".to_string())
        .agents(vec![agent_choice])
        .split_mode(SplitMode::None)
        .focus(false)
        .agent_type(AgentSoftware::Claude)
        .record(true)
        .task_id("mock-task".to_string())
        .build()
        .expect("valid params");

    let launch_result = task_manager.launch_task(params).await;
    assert!(
        matches!(launch_result, TaskLaunchResult::Success { .. }),
        "expected launch success but got {:?}",
        launch_result
    );

    let (_drafts, tasks) = task_manager.get_initial_tasks().await;
    assert!(
        !tasks.is_empty(),
        "expected mock remote TaskManager to return tasks"
    );

    // Clean up server
    handle.abort();
}

#[tokio::test]
async fn rest_client_streams_scenario_events() {
    let playback = ScenarioPlaybackOptions {
        scenario_files: vec![scenario_fixture("simulation_smoke.yaml")],
        speed_multiplier: 0.25,
    };
    let (base_url, handle) = spawn_mock_server_with_options(Some(playback)).await;
    let client = RestClient::from_url(&base_url, AuthConfig::default()).expect("client");

    let request = sample_task_request("Investigate intermittent CI test failure");
    let response = client.create_task(&request).await.expect("create task");
    let session_id = response.session_ids.first().cloned().expect("session id");

    let events = collect_sse_events(&base_url, &session_id, 4).await;
    assert!(
        events.iter().any(|event| matches!(
            event,
            SessionEvent::Status(status) if status.status == SessionStatus::Running
        )),
        "expected at least one running status event"
    );
    assert!(
        events.iter().any(|event| matches!(event, SessionEvent::Log(_))),
        "expected scenario log output to be streamed"
    );

    handle.abort();
}

#[tokio::test]
async fn remote_task_manager_replays_scenario_fast() {
    let playback = ScenarioPlaybackOptions {
        scenario_files: vec![scenario_fixture("long_running_demo.yaml")],
        speed_multiplier: 0.02,
    };
    let (base_url, handle) = spawn_mock_server_with_options(Some(playback)).await;

    let rest_client = RestClient::from_url(&base_url, AuthConfig::default()).expect("client");
    let task_manager = GenericRestTaskManager::new(rest_client.clone());

    let agent_choice = sample_agent_choice();
    let params = TaskLaunchParams::builder()
        .repository("https://github.com/agent-harbor/demo.git".to_string())
        .branch("main".to_string())
        .description("Telemetry refactor validation".to_string())
        .agents(vec![agent_choice])
        .split_mode(SplitMode::None)
        .focus(false)
        .agent_type(AgentSoftware::Claude)
        .record(false)
        .task_id("telemetry-refactor".to_string())
        .build()
        .expect("valid params");

    let start = Instant::now();
    let launch_result = task_manager.launch_task(params).await;
    assert!(
        matches!(launch_result, TaskLaunchResult::Success { .. }),
        "expected launch success but got {:?}",
        launch_result
    );

    let sessions = rest_client.list_sessions(None).await.expect("sessions");
    let session_id = sessions.items.first().expect("session").id.clone();

    let final_status = timeout(Duration::from_secs(3), async {
        loop {
            let session = rest_client.get_session(&session_id).await.expect("session");
            if matches!(
                session.status,
                SessionStatus::Completed | SessionStatus::Failed
            ) {
                return session.status;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("scenario completion timeout");

    assert_eq!(final_status, SessionStatus::Completed);
    assert!(
        start.elapsed() < Duration::from_secs(3),
        "speed multiplier should complete scenario quickly"
    );

    handle.abort();
}

fn scenario_fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test_scenarios")
        .join(name)
}

fn sample_agent_choice() -> AgentChoice {
    AgentChoice {
        agent: AgentSoftwareBuild {
            software: AgentSoftware::Claude,
            version: "latest".to_string(),
        },
        model: "sonnet".to_string(),
        count: 1,
        settings: HashMap::new(),
        display_name: Some("Claude 3.5 Sonnet".to_string()),
    }
}

fn sample_task_request(prompt: &str) -> CreateTaskRequest {
    CreateTaskRequest {
        tenant_id: Some("tenant-demo".into()),
        project_id: Some("project-demo".into()),
        prompt: prompt.to_string(),
        repo: RepoConfig {
            mode: RepoMode::Git,
            url: Some(Url::parse("https://github.com/agent-harbor/demo-repo.git").expect("url")),
            branch: Some("main".into()),
            commit: None,
        },
        runtime: RuntimeConfig {
            runtime_type: RuntimeType::Devcontainer,
            devcontainer_path: Some(".devcontainer/devcontainer.json".into()),
            resources: None,
        },
        workspace: None,
        agents: vec![sample_agent_choice()],
        delivery: None,
        labels: HashMap::new(),
        webhooks: Vec::new(),
    }
}

async fn collect_sse_events(
    base_url: &str,
    session_id: &str,
    expected: usize,
) -> Vec<SessionEvent> {
    let client = Client::new();
    let url = format!("{}/api/v1/sessions/{}/events", base_url, session_id);
    let response = client.get(&url).send().await.expect("SSE response from server");

    let mut stream = response.bytes_stream();
    let mut buffer = Vec::new();
    let mut events = Vec::new();

    let _ = timeout(Duration::from_secs(5), async {
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.expect("chunk");
            buffer.extend_from_slice(&chunk);
            while let Some(pos) = find_frame_boundary(&buffer) {
                let frame: Vec<u8> = buffer.drain(..pos + 2).collect();
                if let Some(event) = parse_sse_frame(&frame) {
                    events.push(event);
                    if events.len() >= expected {
                        return;
                    }
                }
            }
        }
    })
    .await;

    events
}

fn find_frame_boundary(buffer: &[u8]) -> Option<usize> {
    buffer.windows(2).position(|window| window == b"\n\n")
}

fn parse_sse_frame(frame: &[u8]) -> Option<SessionEvent> {
    let text = String::from_utf8_lossy(frame);
    let mut data_lines = Vec::new();
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("data:") {
            data_lines.push(value.trim_start());
        }
    }
    if data_lines.is_empty() {
        return None;
    }
    let payload = data_lines.join("\n");
    serde_json::from_str(&payload).ok()
}
