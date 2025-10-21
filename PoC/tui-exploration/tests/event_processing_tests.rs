//! Tests for TUI functionality organized by topic
//!
//! This file contains tests organized by functional area:
//! - Task Events: TaskEvent processing and activity line generation

use ah_domain_types::{task::ToolStatus, TaskExecutionStatus};
use ah_rest_mock_client::MockRestClient;
use ah_tui::view_model::{AgentActivityRow, FocusElement, TaskCardType, TaskExecutionViewModel};
use ah_workflows::{WorkflowConfig, WorkflowProcessor};
use chrono::Utc;
use tui_exploration::{
    LogLevel, TaskEvent, settings::Settings, view_model::ViewModel,
    workspace_files::GitWorkspaceFiles,
};

#[cfg(test)]
mod event_processing_tests {
    use super::*;

    // Helper function to create a test ViewModel with a running task
    fn create_test_view_model_with_active_task() -> ViewModel {
        let workspace_files = Box::new(GitWorkspaceFiles::new(std::path::PathBuf::from(".")));
        let workspace_workflows = Box::new(WorkflowProcessor::new(WorkflowConfig::default()));
        let task_manager = Box::new(MockRestClient::new());
        let mut settings = Settings::default();
        settings.active_sessions_activity_rows = Some(3); // Set activity rows for testing

        let mut vm = ViewModel::new(workspace_files, workspace_workflows, task_manager, settings);

        // Add a test active task card manually
        use ah_domain_types::{TaskExecution, TaskState};
        let test_task = TaskExecution {
            id: "test_task_1".to_string(),
            repository: "test/repo".to_string(),
            branch: "main".to_string(),
            agents: vec![ah_domain_types::SelectedModel {
                name: "Claude".to_string(),
                count: 1,
            }],
            state: TaskState::Active,
            timestamp: "2024-01-01T12:00:00Z".to_string(),
            activity: vec![],
            delivery_status: vec![],
        };

        use ah_tui::view_model::{TaskCardType, TaskExecutionViewModel, TaskMetadataViewModel};
        let test_card = TaskExecutionViewModel {
            id: "test_task_1".to_string(),
            task: test_task,
            title: "Test Active Task".to_string(),
            metadata: TaskMetadataViewModel {
                repository: "test/repo".to_string(),
                branch: "main".to_string(),
                models: vec![ah_domain_types::SelectedModel {
                    name: "Claude".to_string(),
                    count: 1,
                }],
                state: ah_domain_types::TaskState::Active,
                timestamp: "2024-01-01T12:00:00Z".to_string(),
                delivery_indicators: "".to_string(),
            },
            height: 5,
            card_type: TaskCardType::Active {
                activity_entries: vec![],
                pause_delete_buttons: "Pause | Delete".to_string(),
            },
            focus_element: FocusElement::ExistingTask(0),
        };

        vm.task_cards.push(test_card);
        vm.rebuild_task_id_mapping();
        vm
    }

    #[test]
    fn agent_thought_events_create_activity_entries() {
        let mut vm = create_test_view_model_with_active_task();

        // Send a thought event
        let thought_event = TaskEvent::Thought {
            thought: "Analyzing the codebase structure".to_string(),
            reasoning: None,
            ts: Utc::now(),
        };

        vm.process_task_event("test_task_1", thought_event);

        // Check that the activity entry was created
        let task_card = vm.task_cards.first().unwrap();
        if let TaskCardType::Active {
            activity_entries, ..
        } = &task_card.card_type
        {
            assert_eq!(activity_entries.len(), 1);
            match &activity_entries[0] {
                AgentActivityRow::AgentThought { thought } => {
                    assert_eq!(thought, "Analyzing the codebase structure");
                }
                _ => panic!("Expected AgentThought activity entry"),
            }
        } else {
            panic!("Expected Active task card type");
        }
    }

    #[test]
    fn file_edit_events_create_activity_entries() {
        let mut vm = create_test_view_model_with_active_task();

        // Send a file edit event
        let file_edit_event = TaskEvent::FileEdit {
            file_path: "src/main.rs".to_string(),
            lines_added: 10,
            lines_removed: 5,
            description: Some("Added error handling".to_string()),
            ts: Utc::now(),
        };

        vm.process_task_event("test_task_1", file_edit_event);

        // Check that the activity entry was created
        let task_card = vm.task_cards.first().unwrap();
        if let TaskCardType::Active {
            activity_entries, ..
        } = &task_card.card_type
        {
            assert_eq!(activity_entries.len(), 1);
            match &activity_entries[0] {
                AgentActivityRow::AgentEdit {
                    file_path,
                    lines_added,
                    lines_removed,
                    description,
                } => {
                    assert_eq!(file_path, "src/main.rs");
                    assert_eq!(*lines_added, 10);
                    assert_eq!(*lines_removed, 5);
                    assert_eq!(description, &Some("Added error handling".to_string()));
                }
                _ => panic!("Expected AgentEdit activity entry"),
            }
        } else {
            panic!("Expected Active task card type");
        }
    }

    #[test]
    fn tool_use_events_create_initial_activity_entries() {
        let mut vm = create_test_view_model_with_active_task();

        // Send a tool use event
        let tool_use_event = TaskEvent::ToolUse {
            tool_name: "run_terminal_cmd".to_string(),
            tool_args: serde_json::json!({"command": "ls -la"}),
            tool_execution_id: "tool_exec_123".to_string(),
            status: ToolStatus::Started,
            ts: Utc::now(),
        };

        vm.process_task_event("test_task_1", tool_use_event);

        // Check that the activity entry was created
        let task_card = vm.task_cards.first().unwrap();
        if let TaskCardType::Active {
            activity_entries, ..
        } = &task_card.card_type
        {
            assert_eq!(activity_entries.len(), 1);
            match &activity_entries[0] {
                AgentActivityRow::ToolUse {
                    tool_name,
                    tool_execution_id,
                    last_line,
                    completed,
                    status,
                } => {
                    assert_eq!(tool_name, "run_terminal_cmd");
                    assert_eq!(tool_execution_id, "tool_exec_123");
                    assert_eq!(*last_line, None); // Initially no output
                    assert_eq!(*completed, false);
                    assert_eq!(*status, ToolStatus::Started);
                }
                _ => panic!("Expected ToolUse activity entry"),
            }
        } else {
            panic!("Expected Active task card type");
        }
    }

    #[test]
    fn log_events_update_existing_tool_use_entries() {
        let mut vm = create_test_view_model_with_active_task();

        // First, create a tool use entry
        let tool_use_event = TaskEvent::ToolUse {
            tool_name: "run_terminal_cmd".to_string(),
            tool_args: serde_json::json!({"command": "ls -la"}),
            tool_execution_id: "tool_exec_123".to_string(),
            status: ToolStatus::Started,
            ts: Utc::now(),
        };
        vm.process_task_event("test_task_1", tool_use_event);

        // Then send a log event for the same tool execution
        let log_event = TaskEvent::Log {
            level: LogLevel::Info,
            message: "Found 42 files in directory".to_string(),
            tool_execution_id: Some("tool_exec_123".to_string()),
            ts: Utc::now(),
        };
        vm.process_task_event("test_task_1", log_event);

        // Check that the tool use entry was updated with the log message
        let task_card = vm.task_cards.first().unwrap();
        if let TaskCardType::Active {
            activity_entries, ..
        } = &task_card.card_type
        {
            assert_eq!(activity_entries.len(), 1);
            match &activity_entries[0] {
                AgentActivityRow::ToolUse {
                    tool_name,
                    tool_execution_id,
                    last_line,
                    completed,
                    status,
                } => {
                    assert_eq!(tool_name, "run_terminal_cmd");
                    assert_eq!(tool_execution_id, "tool_exec_123");
                    assert_eq!(last_line, &Some("Found 42 files in directory".to_string()));
                    assert_eq!(*completed, false);
                    assert_eq!(*status, ToolStatus::Started);
                }
                _ => panic!("Expected ToolUse activity entry"),
            }
        }
    }

    #[test]
    fn tool_result_events_complete_tool_use_entries() {
        let mut vm = create_test_view_model_with_active_task();

        // First, create a tool use entry
        let tool_use_event = TaskEvent::ToolUse {
            tool_name: "run_terminal_cmd".to_string(),
            tool_args: serde_json::json!({"command": "ls -la"}),
            tool_execution_id: "tool_exec_123".to_string(),
            status: ToolStatus::Started,
            ts: Utc::now(),
        };
        vm.process_task_event("test_task_1", tool_use_event);

        // Send a log event to add some output
        let log_event = TaskEvent::Log {
            level: LogLevel::Info,
            message: "Command completed successfully".to_string(),
            tool_execution_id: Some("tool_exec_123".to_string()),
            ts: Utc::now(),
        };
        vm.process_task_event("test_task_1", log_event);

        // Then send the tool result
        let tool_result_event = TaskEvent::ToolResult {
            tool_name: "run_terminal_cmd".to_string(),
            tool_output: "total 42\ndrwxr-xr-x  5 user  staff   160 Jan  1 12:00 .\n..."
                .to_string(),
            tool_execution_id: "tool_exec_123".to_string(),
            status: ToolStatus::Completed,
            ts: Utc::now(),
        };
        vm.process_task_event("test_task_1", tool_result_event);

        // Check that the tool use entry was marked as completed
        let task_card = vm.task_cards.first().unwrap();
        if let TaskCardType::Active {
            activity_entries, ..
        } = &task_card.card_type
        {
            assert_eq!(activity_entries.len(), 1);
            match &activity_entries[0] {
                AgentActivityRow::ToolUse {
                    tool_name,
                    tool_execution_id,
                    last_line,
                    completed,
                    status,
                } => {
                    assert_eq!(tool_name, "run_terminal_cmd");
                    assert_eq!(tool_execution_id, "tool_exec_123");
                    // Should still have the log message as last_line
                    assert_eq!(
                        last_line,
                        &Some("Command completed successfully".to_string())
                    );
                    assert_eq!(*completed, true);
                    assert_eq!(*status, ToolStatus::Completed);
                }
                _ => panic!("Expected ToolUse activity entry"),
            }
        }
    }

    #[test]
    fn activity_lines_respect_max_rows_setting() {
        let mut vm = create_test_view_model_with_active_task();

        // Send multiple thought events (more than the 3-row limit)
        for i in 1..=5 {
            let thought_event = TaskEvent::Thought {
                thought: format!("Thought number {}", i),
                reasoning: None,
                ts: Utc::now(),
            };
            vm.process_task_event("test_task_1", thought_event);
        }

        // Check that only the most recent 3 activities are kept
        let task_card = vm.task_cards.first().unwrap();
        if let TaskCardType::Active {
            activity_entries, ..
        } = &task_card.card_type
        {
            assert_eq!(activity_entries.len(), 3);

            // Should have thoughts 3, 4, 5 (most recent)
            match &activity_entries[0] {
                AgentActivityRow::AgentThought { thought } => {
                    assert_eq!(thought, "Thought number 3")
                }
                _ => panic!("Expected AgentThought"),
            }
            match &activity_entries[1] {
                AgentActivityRow::AgentThought { thought } => {
                    assert_eq!(thought, "Thought number 4")
                }
                _ => panic!("Expected AgentThought"),
            }
            match &activity_entries[2] {
                AgentActivityRow::AgentThought { thought } => {
                    assert_eq!(thought, "Thought number 5")
                }
                _ => panic!("Expected AgentThought"),
            }
        }
    }

    #[test]
    fn log_events_for_unknown_tool_execution_are_ignored() {
        let mut vm = create_test_view_model_with_active_task();

        // Send a log event for a tool execution that doesn't exist
        let log_event = TaskEvent::Log {
            level: LogLevel::Info,
            message: "This should be ignored".to_string(),
            tool_execution_id: Some("nonexistent_tool_exec".to_string()),
            ts: Utc::now(),
        };

        vm.process_task_event("test_task_1", log_event);

        // Check that no activity entries were created
        let task_card = vm.task_cards.first().unwrap();
        if let TaskCardType::Active {
            activity_entries, ..
        } = &task_card.card_type
        {
            assert_eq!(activity_entries.len(), 0);
        }
    }

    #[test]
    fn tool_result_events_for_unknown_tool_execution_are_ignored() {
        let mut vm = create_test_view_model_with_active_task();

        // Send a tool result event for a tool execution that doesn't exist
        let tool_result_event = TaskEvent::ToolResult {
            tool_name: "unknown_tool".to_string(),
            tool_output: "Some output".to_string(),
            tool_execution_id: "nonexistent_tool_exec".to_string(),
            status: ToolStatus::Completed,
            ts: Utc::now(),
        };

        vm.process_task_event("test_task_1", tool_result_event);

        // Check that no activity entries were created
        let task_card = vm.task_cards.first().unwrap();
        if let TaskCardType::Active {
            activity_entries, ..
        } = &task_card.card_type
        {
            assert_eq!(activity_entries.len(), 0);
        }
    }

    #[test]
    fn log_events_without_tool_execution_id_are_ignored() {
        let mut vm = create_test_view_model_with_active_task();

        // Send a log event without tool_execution_id
        let log_event = TaskEvent::Log {
            level: LogLevel::Info,
            message: "General log message".to_string(),
            tool_execution_id: None,
            ts: Utc::now(),
        };

        vm.process_task_event("test_task_1", log_event);

        // Check that no activity entries were created
        let task_card = vm.task_cards.first().unwrap();
        if let TaskCardType::Active {
            activity_entries, ..
        } = &task_card.card_type
        {
            assert_eq!(activity_entries.len(), 0);
        }
    }

    #[test]
    fn events_for_unknown_task_ids_are_ignored() {
        let mut vm = create_test_view_model_with_active_task();

        // Send an event for a task that doesn't exist in the view model
        let thought_event = TaskEvent::Thought {
            thought: "This should be ignored".to_string(),
            reasoning: None,
            ts: Utc::now(),
        };

        vm.process_task_event("unknown_task_id", thought_event);

        // Check that no activity entries were created for our known task
        let task_card = vm.task_cards.first().unwrap();
        if let TaskCardType::Active {
            activity_entries, ..
        } = &task_card.card_type
        {
            assert_eq!(activity_entries.len(), 0);
        }
    }

    #[test]
    fn events_for_draft_tasks_are_ignored() {
        let workspace_files = Box::new(GitWorkspaceFiles::new(std::path::PathBuf::from(".")));
        let workspace_workflows = Box::new(WorkflowProcessor::new(WorkflowConfig::default()));
        let task_manager = Box::new(MockRestClient::new());
        let settings = Settings::default();

        let mut vm = ViewModel::new(workspace_files, workspace_workflows, task_manager, settings);

        // The MockTaskManager creates draft cards, so we should have at least one
        assert!(!vm.draft_cards.is_empty());

        // Send an event for the draft task
        let thought_event = TaskEvent::Thought {
            thought: "This should be ignored for draft tasks".to_string(),
            reasoning: None,
            ts: Utc::now(),
        };

        vm.process_task_event("draft_001", thought_event);

        // The event should be ignored (no activity processing for draft tasks)
        // This test just verifies that no panic occurs and processing continues
    }

    #[test]
    fn multiple_tool_executions_are_handled_correctly() {
        let mut vm = create_test_view_model_with_active_task();

        // Start two different tool executions
        let tool1_event = TaskEvent::ToolUse {
            tool_name: "run_terminal_cmd".to_string(),
            tool_args: serde_json::json!({"command": "ls"}),
            tool_execution_id: "tool_exec_1".to_string(),
            status: ToolStatus::Started,
            ts: Utc::now(),
        };
        vm.process_task_event("test_task_1", tool1_event);

        let tool2_event = TaskEvent::ToolUse {
            tool_name: "search_codebase".to_string(),
            tool_args: serde_json::json!({"query": "function"}),
            tool_execution_id: "tool_exec_2".to_string(),
            status: ToolStatus::Started,
            ts: Utc::now(),
        };
        vm.process_task_event("test_task_1", tool2_event);

        // Send log events for each tool
        let log1_event = TaskEvent::Log {
            level: LogLevel::Info,
            message: "Listing directory contents".to_string(),
            tool_execution_id: Some("tool_exec_1".to_string()),
            ts: Utc::now(),
        };
        vm.process_task_event("test_task_1", log1_event);

        let log2_event = TaskEvent::Log {
            level: LogLevel::Info,
            message: "Searching for functions".to_string(),
            tool_execution_id: Some("tool_exec_2".to_string()),
            ts: Utc::now(),
        };
        vm.process_task_event("test_task_1", log2_event);

        // Check that both tools have their respective log messages
        let task_card = vm.task_cards.first().unwrap();
        if let TaskCardType::Active {
            activity_entries, ..
        } = &task_card.card_type
        {
            assert_eq!(activity_entries.len(), 2);

            // Find the tool entries (order may vary)
            let tool1_entry = activity_entries.iter().find(|entry| {
                matches!(entry, AgentActivityRow::ToolUse { tool_execution_id, .. } if tool_execution_id == "tool_exec_1")
            });
            let tool2_entry = activity_entries.iter().find(|entry| {
                matches!(entry, AgentActivityRow::ToolUse { tool_execution_id, .. } if tool_execution_id == "tool_exec_2")
            });

            assert!(tool1_entry.is_some());
            assert!(tool2_entry.is_some());

            if let AgentActivityRow::ToolUse { last_line, .. } = tool1_entry.unwrap() {
                assert_eq!(last_line, &Some("Listing directory contents".to_string()));
            }
            if let AgentActivityRow::ToolUse { last_line, .. } = tool2_entry.unwrap() {
                assert_eq!(last_line, &Some("Searching for functions".to_string()));
            }
        }
    }

    #[test]
    fn tool_result_overwrites_log_message_when_no_log_received() {
        let mut vm = create_test_view_model_with_active_task();

        // Start a tool execution
        let tool_use_event = TaskEvent::ToolUse {
            tool_name: "run_terminal_cmd".to_string(),
            tool_args: serde_json::json!({"command": "ls -la"}),
            tool_execution_id: "tool_exec_123".to_string(),
            status: ToolStatus::Started,
            ts: Utc::now(),
        };
        vm.process_task_event("test_task_1", tool_use_event);

        // Send tool result directly without any log events
        let tool_result_event = TaskEvent::ToolResult {
            tool_name: "run_terminal_cmd".to_string(),
            tool_output: "total 42\ndrwxr-xr-x  5 user  staff   160 Jan  1 12:00 .".to_string(),
            tool_execution_id: "tool_exec_123".to_string(),
            status: ToolStatus::Completed,
            ts: Utc::now(),
        };
        vm.process_task_event("test_task_1", tool_result_event);

        // Check that the last_line contains the first line of tool_output
        let task_card = vm.task_cards.first().unwrap();
        if let TaskCardType::Active {
            activity_entries, ..
        } = &task_card.card_type
        {
            assert_eq!(activity_entries.len(), 1);
            match &activity_entries[0] {
                AgentActivityRow::ToolUse {
                    last_line,
                    completed,
                    ..
                } => {
                    assert_eq!(last_line, &Some("total 42".to_string()));
                    assert_eq!(*completed, true);
                }
                _ => panic!("Expected ToolUse activity entry"),
            }
        }
    }

    #[test]
    fn status_and_other_events_are_not_converted_to_activity_entries() {
        let mut vm = create_test_view_model_with_active_task();

        // Send various events that should NOT create activity entries
        let status_event = TaskEvent::Status {
            status: TaskExecutionStatus::Running,
            ts: Utc::now(),
        };
        vm.process_task_event("test_task_1", status_event);

        let log_without_tool_id = TaskEvent::Log {
            level: LogLevel::Info,
            message: "General status message".to_string(),
            tool_execution_id: None,
            ts: Utc::now(),
        };
        vm.process_task_event("test_task_1", log_without_tool_id);

        // Check that no activity entries were created
        let task_card = vm.task_cards.first().unwrap();
        if let TaskCardType::Active {
            activity_entries, ..
        } = &task_card.card_type
        {
            assert_eq!(activity_entries.len(), 0);
        }
    }

    #[test]
    fn failed_tool_result_updates_status_correctly() {
        let mut vm = create_test_view_model_with_active_task();

        // Start a tool execution
        let tool_use_event = TaskEvent::ToolUse {
            tool_name: "run_terminal_cmd".to_string(),
            tool_args: serde_json::json!({"command": "invalid_command"}),
            tool_execution_id: "tool_exec_456".to_string(),
            status: ToolStatus::Started,
            ts: Utc::now(),
        };
        vm.process_task_event("test_task_1", tool_use_event);

        // Send a failed tool result
        let tool_result_event = TaskEvent::ToolResult {
            tool_name: "run_terminal_cmd".to_string(),
            tool_output: "bash: invalid_command: command not found".to_string(),
            tool_execution_id: "tool_exec_456".to_string(),
            status: ToolStatus::Failed,
            ts: Utc::now(),
        };
        vm.process_task_event("test_task_1", tool_result_event);

        // Check that the tool entry shows failed status
        let task_card = vm.task_cards.first().unwrap();
        if let TaskCardType::Active {
            activity_entries, ..
        } = &task_card.card_type
        {
            assert_eq!(activity_entries.len(), 1);
            match &activity_entries[0] {
                AgentActivityRow::ToolUse {
                    completed,
                    status,
                    last_line,
                    ..
                } => {
                    assert_eq!(*completed, true);
                    assert_eq!(*status, ToolStatus::Failed);
                    assert_eq!(
                        last_line,
                        &Some("bash: invalid_command: command not found".to_string())
                    );
                }
                _ => panic!("Expected ToolUse activity entry"),
            }
        }
    }
}
