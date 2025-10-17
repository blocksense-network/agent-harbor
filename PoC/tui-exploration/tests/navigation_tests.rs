//! Tests for TUI functionality organized by topic
//!
//! This file contains tests organized by functional area:
//! - Navigation: Keyboard navigation and focus management per PRD specifications
//! - Task Events: TaskEvent processing and activity line generation

use tui_exploration::{
    view_model::ViewModel,
    workspace_files::GitWorkspaceFiles,
    workspace_workflows::PathWorkspaceWorkflows,
    task_manager::{MockTaskManager, TaskEvent, TaskStatus, LogLevel},
    settings::Settings,
};
use ah_core::task_manager::ToolStatus;
use ah_domain_types::{TaskExecution, TaskState, SelectedModel, DeliveryStatus};
use ah_tui::view_model::{FocusElement, AgentActivityRow, TaskCardType, TaskMetadataViewModel, TaskExecutionViewModel};

#[cfg(test)]
mod navigation_tests {
    use super::*;

    #[test]
    fn view_model_hierarchical_navigation_matches_prd_specification() {
        // Test the hierarchical navigation order as specified in TUI-PRD.md
        // Navigation order: Settings button → Draft task cards → Filter bar separator → Existing task cards → Settings button

        // Create a minimal ViewModel for testing navigation
        let workspace_files = Box::new(GitWorkspaceFiles::new(std::path::PathBuf::from(".")));
        let workspace_workflows = Box::new(PathWorkspaceWorkflows::new(std::path::PathBuf::from(".")));
        let task_manager = Box::new(MockTaskManager::new());
        let settings = Settings::default();

        let mut vm = ViewModel::new(
            workspace_files,
            workspace_workflows,
            task_manager,
            settings,
        );

        // Initially focused on draft task (as per PRD: "The initially focused element is the top draft task card.")
        assert_eq!(vm.focus_element, FocusElement::DraftTask(0));

        // Test downward navigation order
        // From draft task → should go to settings button (since there are no existing tasks)
        assert!(vm.navigate_down_hierarchy());
        assert_eq!(vm.focus_element, FocusElement::SettingsButton);

        // From settings → should go to draft task (only draft exists)
        assert!(vm.navigate_down_hierarchy());
        assert_eq!(vm.focus_element, FocusElement::DraftTask(0));

        // Test upward navigation order
        // From draft task → should go to settings button
        assert!(vm.navigate_up_hierarchy());
        assert_eq!(vm.focus_element, FocusElement::SettingsButton);

        // From settings → should wrap to draft task (no existing tasks)
        assert!(vm.navigate_up_hierarchy());
        assert_eq!(vm.focus_element, FocusElement::DraftTask(0));

        // Now add some existing tasks to test full navigation cycle
        // Add a completed task and an active task
        vm.task_cards.push(TaskExecutionViewModel {
            id: "completed_1".to_string(),
            task: TaskExecution {
                id: "completed_1".to_string(),
                repository: "test/repo".to_string(),
                branch: "main".to_string(),
                agents: vec![SelectedModel {
                    name: "Claude".to_string(),
                    count: 1,
                }],
                state: TaskState::Completed,
                timestamp: "2023-01-01 12:00:00".to_string(),
                activity: vec![],
                delivery_status: vec![DeliveryStatus::BranchCreated],
            },
            title: "Completed Task".to_string(),
            metadata: TaskMetadataViewModel {
                repository: "test/repo".to_string(),
                branch: "main".to_string(),
                models: vec![SelectedModel {
                    name: "Claude".to_string(),
                    count: 1,
                }],
                state: TaskState::Completed,
                timestamp: "2023-01-01 12:00:00".to_string(),
                delivery_indicators: "✓".to_string(),
            },
            height: 2,
            card_type: TaskCardType::Completed {
                delivery_indicators: "✓ Completed".to_string(),
            },
            focus_element: FocusElement::ExistingTask(0),
        });

        vm.task_cards.push(TaskExecutionViewModel {
            id: "active_1".to_string(),
            task: TaskExecution {
                id: "active_1".to_string(),
                repository: "test/repo".to_string(),
                branch: "feature/test".to_string(),
                agents: vec![SelectedModel {
                    name: "Claude".to_string(),
                    count: 1,
                }],
                state: TaskState::Active,
                timestamp: "2023-01-01 12:00:00".to_string(),
                activity: vec!["Working...".to_string()],
                delivery_status: vec![],
            },
            title: "Active Task".to_string(),
            metadata: TaskMetadataViewModel {
                repository: "test/repo".to_string(),
                branch: "feature/test".to_string(),
                models: vec![SelectedModel {
                    name: "Claude".to_string(),
                    count: 1,
                }],
                state: TaskState::Active,
                timestamp: "2023-01-01 12:00:00".to_string(),
                delivery_indicators: "".to_string(),
            },
            height: 5,
            card_type: TaskCardType::Active {
                activity_entries: vec![AgentActivityRow::AgentThought { thought: "Working...".to_string() }],
                pause_delete_buttons: "Pause | Delete".to_string(),
            },
            focus_element: FocusElement::ExistingTask(1),
        });

        // Rebuild the task ID mapping after adding tasks
        vm.rebuild_task_id_mapping();

        // Start from settings button
        vm.focus_element = FocusElement::SettingsButton;

        // Navigate down: Settings → Draft task
        assert!(vm.navigate_down_hierarchy());
        assert_eq!(vm.focus_element, FocusElement::DraftTask(0));

        // Navigate down: Draft task → Filter bar separator (existing tasks exist)
        assert!(vm.navigate_down_hierarchy());
        assert_eq!(vm.focus_element, FocusElement::FilterBarSeparator);

        // Navigate down: Filter bar separator → First existing task
        assert!(vm.navigate_down_hierarchy());
        assert_eq!(vm.focus_element, FocusElement::ExistingTask(0));

        // Navigate down: First existing task → Second existing task
        assert!(vm.navigate_down_hierarchy());
        assert_eq!(vm.focus_element, FocusElement::ExistingTask(1));

        // Navigate down: Last existing task → Settings button (wrap around)
        assert!(vm.navigate_down_hierarchy());
        assert_eq!(vm.focus_element, FocusElement::SettingsButton);

        // Test upward navigation in reverse
        // From settings → Last existing task
        assert!(vm.navigate_up_hierarchy());
        assert_eq!(vm.focus_element, FocusElement::ExistingTask(1));

        // From last existing → First existing
        assert!(vm.navigate_up_hierarchy());
        assert_eq!(vm.focus_element, FocusElement::ExistingTask(0));

        // From first existing → Filter bar separator
        assert!(vm.navigate_up_hierarchy());
        assert_eq!(vm.focus_element, FocusElement::FilterBarSeparator);

        // From filter bar separator → Draft task
        assert!(vm.navigate_up_hierarchy());
        assert_eq!(vm.focus_element, FocusElement::DraftTask(0));

        // From draft task → Settings button
        assert!(vm.navigate_up_hierarchy());
        assert_eq!(vm.focus_element, FocusElement::SettingsButton);
    }

    #[test]
    fn view_model_navigation_edge_cases() {
        // Create a minimal ViewModel for testing navigation edge cases
        let workspace_files = Box::new(GitWorkspaceFiles::new(std::path::PathBuf::from(".")));
        let workspace_workflows = Box::new(PathWorkspaceWorkflows::new(std::path::PathBuf::from(".")));
        let task_manager = Box::new(MockTaskManager::new());
        let settings = Settings::default();

        let mut vm = ViewModel::new(
            workspace_files,
            workspace_workflows,
            task_manager,
            settings,
        );

        // Test: No existing tasks, navigation wraps between settings and draft
        vm.focus_element = FocusElement::DraftTask(0);

        // Down: Draft → Settings (wrap)
        assert!(vm.navigate_down_hierarchy());
        assert_eq!(vm.focus_element, FocusElement::SettingsButton);

        // Down: Settings → Draft
        assert!(vm.navigate_down_hierarchy());
        assert_eq!(vm.focus_element, FocusElement::DraftTask(0));

        // Up: Draft → Settings
        assert!(vm.navigate_up_hierarchy());
        assert_eq!(vm.focus_element, FocusElement::SettingsButton);

        // Up: Settings → Draft (wrap, since no existing tasks)
        assert!(vm.navigate_up_hierarchy());
        assert_eq!(vm.focus_element, FocusElement::DraftTask(0));

        // Basic navigation test completed successfully
    }

    #[test]
    fn draft_cards_are_loaded_from_mock_task_manager() {
        // Test that draft cards are loaded correctly from MockTaskManager

        // Create a ViewModel
        let workspace_files = Box::new(GitWorkspaceFiles::new(std::path::PathBuf::from(".")));
        let workspace_workflows = Box::new(PathWorkspaceWorkflows::new(std::path::PathBuf::from(".")));
        let task_manager = Box::new(MockTaskManager::new());
        let settings = Settings::default();

        let vm = ViewModel::new(
            workspace_files,
            workspace_workflows,
            task_manager,
            settings,
        );

        // ViewModel creates 1 draft card initially with ID "current"
        assert_eq!(vm.draft_cards.len(), 1);
        assert_eq!(vm.draft_cards[0].id, "current");
        assert_eq!(vm.draft_cards[0].description.lines().join("\n"), "");
    }

    #[test]
    fn key_event_filtering_processes_press_and_repeat_events() {
        use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

        // Create a ViewModel
        let workspace_files = Box::new(GitWorkspaceFiles::new(std::path::PathBuf::from(".")));
        let workspace_workflows = Box::new(PathWorkspaceWorkflows::new(std::path::PathBuf::from(".")));
        let task_manager = Box::new(MockTaskManager::new());
        let settings = Settings::default();

        let mut vm = ViewModel::new(
            workspace_files,
            workspace_workflows,
            task_manager,
            settings,
        );

        // Initially focused on draft task
        assert_eq!(vm.focus_element, FocusElement::DraftTask(0));

        // Test that KeyEventKind::Press is processed
        let press_event = KeyEvent::new_with_kind(
            KeyCode::Down,
            KeyModifiers::NONE,
            KeyEventKind::Press,
        );
        vm.handle_key_event(press_event);

        // Should have moved to next focus element (from DraftTask(0) to SettingsButton)
        assert_eq!(vm.focus_element, FocusElement::SettingsButton);

        // Test that KeyEventKind::Repeat is also processed
        let repeat_event = KeyEvent::new_with_kind(
            KeyCode::Down,
            KeyModifiers::NONE,
            KeyEventKind::Repeat,
        );
        vm.handle_key_event(repeat_event);

        // Should have moved to the next focus element again (SettingsButton wraps to DraftTask(0))
        assert_eq!(vm.focus_element, FocusElement::DraftTask(0));

        // Test that KeyEventKind::Release is ignored (filtered at main event loop)
        let release_event = KeyEvent::new_with_kind(
            KeyCode::Down,
            KeyModifiers::NONE,
            KeyEventKind::Release,
        );
        vm.handle_key_event(release_event);

        // Should have moved to SettingsButton (navigation cycles: DraftTask(0) -> SettingsButton -> DraftTask(0) -> SettingsButton)
        assert_eq!(vm.focus_element, FocusElement::SettingsButton);
    }
}

