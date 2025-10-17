//! Simple test program to verify TaskEvent streaming works

use futures::StreamExt;
use tui_exploration::{TaskManager, TaskLaunchParams, SelectedModel};
use ah_rest_mock_client::MockRestClient;

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
