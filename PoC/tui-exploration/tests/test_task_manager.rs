// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Standalone test file for TaskManager functionality
//! This allows testing the task manager without the compilation issues
//! in the main library due to ongoing MVVM refactoring.

use chrono::Utc;
use futures::StreamExt;
use serde_json::json;

use ah_domain_types::TaskState;
use ah_domain_types::task::ToolStatus;
use ah_rest_mock_client::MockRestClient;
use tui_exploration::{SelectedModel, TaskEvent, TaskLaunchParams, TaskLaunchResult, TaskManager};

#[tokio::test]
async fn test_mock_rest_client_launches_successful_task() {
    let manager = MockRestClient::new();
    let params = TaskLaunchParams {
        repository: "test/repo".to_string(),
        branch: "main".to_string(),
        description: "Test task".to_string(),
        models: vec![SelectedModel {
            name: "Claude".to_string(),
            count: 1,
        }],
        split_view: false,
        focus: false,
    };

    let result = manager.launch_task(params).await;

    assert!(result.is_success());
    assert!(result.task_id().unwrap().starts_with("task_"));
}

#[tokio::test]
async fn test_mock_rest_client_validates_empty_description() {
    let manager = MockRestClient::new();
    let params = TaskLaunchParams {
        repository: "test/repo".to_string(),
        branch: "main".to_string(),
        description: "".to_string(),
        models: vec![SelectedModel {
            name: "Claude".to_string(),
            count: 1,
        }],
        split_view: false,
        focus: false,
    };

    let result = manager.launch_task(params).await;

    assert!(!result.is_success());
    assert_eq!(result.error().unwrap(), "Task description cannot be empty");
}

#[tokio::test]
async fn test_mock_rest_client_validates_empty_models() {
    let manager = MockRestClient::new();
    let params = TaskLaunchParams {
        repository: "test/repo".to_string(),
        branch: "main".to_string(),
        description: "Test task".to_string(),
        models: vec![],
        split_view: false,
        focus: false,
    };

    let result = manager.launch_task(params).await;

    assert!(!result.is_success());
    assert_eq!(
        result.error().unwrap(),
        "At least one model must be selected"
    );
}

#[tokio::test]
async fn test_mock_rest_client_handles_simulated_failures() {
    let manager = MockRestClient::with_failures(true);
    let params = TaskLaunchParams {
        repository: "test/repo".to_string(),
        branch: "main".to_string(),
        description: "This task will fail".to_string(),
        models: vec![SelectedModel {
            name: "Claude".to_string(),
            count: 1,
        }],
        split_view: false,
        focus: false,
    };

    let result = manager.launch_task(params).await;

    assert!(!result.is_success());
    assert_eq!(result.error().unwrap(), "Simulated task launch failure");
}

#[tokio::test]
async fn test_mock_task_manager_generates_deterministic_task_ids() {
    let manager = MockRestClient::new();
    let params1 = TaskLaunchParams {
        repository: "test/repo".to_string(),
        branch: "main".to_string(),
        description: "Test task".to_string(),
        models: vec![SelectedModel {
            name: "Claude".to_string(),
            count: 1,
        }],
        split_view: false,
        focus: false,
    };

    let params2 = params1.clone();

    let result1 = manager.launch_task(params1).await;
    let result2 = manager.launch_task(params2).await;

    // Same parameters should generate same session ID
    assert_eq!(
        result1.session_ids().unwrap()[0],
        result2.session_ids().unwrap()[0]
    );
}

#[tokio::test]
async fn test_task_launch_result_display_formats_correctly() {
    let success = TaskLaunchResult::Success {
        session_ids: vec!["task_123".to_string()],
    };
    let failure = TaskLaunchResult::Failure {
        error: "Something went wrong".to_string(),
    };

    assert_eq!(
        format!("{}", success),
        "Task launched successfully: task_123"
    );
    assert_eq!(
        format!("{}", failure),
        "Task launch failed: Something went wrong"
    );
}

#[tokio::test]
async fn test_mock_task_manager_event_stream() {
    let manager = MockRestClient::new();
    let task_id = "test_task_123";
    let mut stream = manager.task_events_stream(task_id);

    // Collect all events from the stream
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event);
    }

    // Verify we got the expected sequence of events
    assert!(!events.is_empty());

    // Check that the first event is status change to queued
    match &events[0] {
        TaskEvent::Status { status, .. } => assert_eq!(*status, TaskState::Queued),
        _ => panic!("First event should be status change to queued"),
    }

    // Check that we eventually get to completed status
    let has_completed = events.iter().any(|event| {
        matches!(event, TaskEvent::Status { status, .. } if *status == TaskState::Completed)
    });
    assert!(
        has_completed,
        "Stream should contain a completed status event"
    );

    // Check that we have various event types
    let has_thoughts = events.iter().any(|event| matches!(event, TaskEvent::Thought { .. }));
    let has_file_edits = events.iter().any(|event| matches!(event, TaskEvent::FileEdit { .. }));
    let has_tool_use = events.iter().any(|event| matches!(event, TaskEvent::ToolUse { .. }));
    let has_tool_result = events.iter().any(|event| matches!(event, TaskEvent::ToolResult { .. }));

    assert!(has_thoughts, "Stream should contain thought events");
    assert!(has_file_edits, "Stream should contain file edit events");
    assert!(has_tool_use, "Stream should contain tool use events");
    assert!(has_tool_result, "Stream should contain tool result events");
}

#[tokio::test]
async fn test_task_event_serialization() {
    let ts = Utc::now();

    // Test status event
    let status_event = TaskEvent::Status {
        status: TaskState::Running,
        ts,
    };
    let json = serde_json::to_string(&status_event).unwrap();
    let deserialized: TaskEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(status_event, deserialized);

    // Test thought event
    let thought_event = TaskEvent::Thought {
        thought: "Analyzing the code".to_string(),
        reasoning: Some("Need to understand the structure".to_string()),
        ts,
    };
    let json = serde_json::to_string(&thought_event).unwrap();
    let deserialized: TaskEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(thought_event, deserialized);

    // Test tool use event
    let tool_event = TaskEvent::ToolUse {
        tool_name: "cargo".to_string(),
        tool_args: json!(["build"]),
        tool_execution_id: "exec_123".to_string(),
        status: ToolStatus::Started,
        ts,
    };
    let json = serde_json::to_string(&tool_event).unwrap();
    let deserialized: TaskEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(tool_event, deserialized);

    // Test file edit event
    let file_event = TaskEvent::FileEdit {
        file_path: "src/main.rs".to_string(),
        lines_added: 10,
        lines_removed: 5,
        description: Some("Added new functionality".to_string()),
        ts,
    };
    let json = serde_json::to_string(&file_event).unwrap();
    let deserialized: TaskEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(file_event, deserialized);
}
