// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Simple test program to verify TaskEvent streaming works

use ah_core::task_manager::TaskManager;
use ah_rest_mock_client::MockRestClient;
use futures::StreamExt;
use tui_exploration::{SelectedModel, TaskLaunchParams};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tokio::runtime::Runtime::new()?.block_on(async_main())
}

async fn async_main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing TaskManager event streaming...");

    let manager = MockRestClient::new();
    let task_id = "test_task_123";

    // Launch a task first
    let params = TaskLaunchParams {
        repository: "test/repo".to_string(),
        branch: "main".to_string(),
        description: "Test streaming task".to_string(),
        models: vec![SelectedModel {
            name: "Claude".to_string(),
            count: 1,
        }],
        split_view: false,
        focus: false,
    };

    let result = manager.launch_task(params).await;
    println!("Launch result: {:?}", result);

    if let Some(task_id) = result.task_id() {
        println!("Task ID: {}", task_id);

        // Now stream events
        let mut stream = manager.task_events_stream(task_id);
        let mut event_count = 0;

        println!("Streaming events...");
        while let Some(event) = stream.next().await {
            event_count += 1;
            println!("Event {}: {:?}", event_count, event);

            // Limit output for testing
            if event_count >= 20 {
                println!("... (stopping after 20 events for testing)");
                break;
            }
        }

        println!("Total events received: {}", event_count);
    }

    println!("Test completed successfully!");
    Ok(())
}
