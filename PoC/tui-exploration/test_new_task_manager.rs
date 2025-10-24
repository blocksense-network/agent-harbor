// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Test the new TaskManager async methods

use tui_exploration::{MockTaskManager, SaveDraftResult, SelectedModel, TaskManager};

#[tokio::test]
async fn test_get_initial_tasks() {
    let manager = MockTaskManager::new();
    let (drafts, tasks) = manager.get_initial_tasks().await;

    // Check drafts
    assert_eq!(drafts.len(), 1);
    assert_eq!(drafts[0].id, "draft_001");
    assert_eq!(drafts[0].title, "Implement user authentication");
    assert_eq!(drafts[0].status, "draft");

    // Check tasks
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0].id, "task_001");
    assert_eq!(tasks[0].title, "Add database migrations");
    assert_eq!(tasks[0].status, "completed");
    assert_eq!(tasks[1].id, "task_002");
    assert_eq!(tasks[1].status, "running");
}

#[tokio::test]
async fn test_save_draft_success() {
    let manager = MockTaskManager::new();
    let models = vec![tui_exploration::SelectedModel {
        name: "Claude".to_string(),
        count: 1,
    }];

    let result = manager
        .save_draft_task(
            "draft_001",
            "Test description",
            "test/repo",
            "main",
            &models,
        )
        .await;

    assert!(matches!(result, tui_exploration::SaveDraftResult::Success));
}

#[tokio::test]
async fn test_save_draft_failure() {
    let manager = MockTaskManager::with_failures(true);
    let models = vec![tui_exploration::SelectedModel {
        name: "Claude".to_string(),
        count: 1,
    }];

    let result = manager
        .save_draft_task("draft_001", "This will fail", "test/repo", "main", &models)
        .await;

    assert!(matches!(
        result,
        tui_exploration::SaveDraftResult::Failure { .. }
    ));
}

#[tokio::test]
async fn test_list_repositories() {
    let manager = MockTaskManager::new();
    let repos = manager.list_repositories().await;

    assert_eq!(repos.len(), 3);
    assert_eq!(repos[0].name, "myapp/backend");
    assert_eq!(repos[1].name, "myapp/frontend");
    assert_eq!(repos[2].name, "myapp/mobile");
}

#[tokio::test]
async fn test_list_branches() {
    let manager = MockTaskManager::new();

    let branches = manager.list_branches("repo_001").await;
    assert_eq!(branches.len(), 3);
    assert_eq!(branches[0].name, "main");
    assert!(branches[0].is_default);

    let branches = manager.list_branches("repo_002").await;
    assert_eq!(branches.len(), 2);

    let branches = manager.list_branches("unknown_repo").await;
    assert_eq!(branches.len(), 0);
}

#[tokio::test]
async fn test_time_simulation_with_accelerated_execution() {
    // This test demonstrates how to use Tokio's time utilities for accelerated testing
    // We can pause time, advance it manually, and control the execution order

    tokio::time::pause();

    let manager = MockTaskManager::with_delay(1000); // 1 second delay for operations

    // Start multiple async operations that would normally take time
    let initial_tasks_future = manager.get_initial_tasks();
    let repos_future = manager.list_repositories();

    // At this point, time is paused, so no operations have actually executed yet
    // We can advance time selectively to control execution order

    // Advance time by 500ms - initial_tasks_future should still be pending
    // repos_future should still be pending
    tokio::time::advance(std::time::Duration::from_millis(500)).await;

    // Both futures should still be pending since they require 1000ms delay

    // Advance time by another 600ms (total 1100ms)
    tokio::time::advance(std::time::Duration::from_millis(600)).await;

    // Now both operations should be able to complete
    let (drafts, tasks) = initial_tasks_future.await;
    let repos = repos_future.await;

    assert_eq!(drafts.len(), 1);
    assert_eq!(tasks.len(), 2);
    assert_eq!(repos.len(), 3);

    // Resume normal time
    tokio::time::resume();
}

fn main() {
    println!("This is a test binary for TaskManager functionality");
    println!("Run with: cargo test --bin test-new-task-manager");
}
