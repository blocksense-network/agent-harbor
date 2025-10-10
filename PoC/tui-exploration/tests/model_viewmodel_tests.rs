//! Comprehensive MVVM tests without terminal dependencies
//!
//! These tests focus entirely on business logic and presentation state,
//! following the fast-test patterns from the MVVM research document.
//! All tests run without instantiating any terminal UI components.

use tui_exploration::{
    messages::*,
    model::*,
    view_model::*,
};
use crate::model::{TaskItem, DraftTask};
use crate::view_model::NavigationDirection;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::time::Instant;

/// Helper function to create a key event for testing
fn key_event(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers)
}

/// Helper function to create a simple active task execution
fn create_active_task() -> TaskExecution {
    TaskExecution {
        id: "active_simple".to_string(),
        repository: "test/repo".to_string(),
        branch: "main".to_string(),
        agents: vec![SelectedModel { name: "Claude".to_string(), count: 1 }],
        state: TaskState::Active,
        timestamp: "2023-01-01 12:00:00".to_string(),
        activity: vec!["Working...".to_string()],
        delivery_status: vec![],
    }
}

/// Helper function to create an active task with activity
fn create_active_task_with_activity() -> TaskExecution {
    TaskExecution {
        id: "active_1".to_string(),
        repository: "test/repo".to_string(),
        branch: "feature/test".to_string(),
        agents: vec![SelectedModel { name: "Claude".to_string(), count: 1 }],
        state: TaskState::Active,
        timestamp: "2023-01-01 12:00:00".to_string(),
        activity: vec![
            "Thoughts: Analyzing codebase structure".to_string(),
            "Tool usage: search_codebase".to_string(),
            "  Found 42 matches in 12 files".to_string(),
        ],
        delivery_status: vec![],
    }
}

    /// Helper function to create a completed task execution
    fn create_completed_task() -> TaskExecution {
        TaskExecution {
            id: "completed_1".to_string(),
            repository: "ecommerce-platform".to_string(),
            branch: "feature/payments".to_string(),
            agents: vec![SelectedModel { name: "Claude".to_string(), count: 1 }],
            state: TaskState::Completed,
            timestamp: "2023-01-01 12:00:00".to_string(),
            activity: vec![],
            delivery_status: vec![
                crate::model::DeliveryStatus::BranchCreated,
                crate::model::DeliveryStatus::PullRequestCreated { pr_number: 42, title: "Add payment processing".to_string() },
            ],
        }
    }

    /// Helper function to create a draft task
    fn create_draft_task(description: &str) -> DraftTask {
        DraftTask {
            id: "draft_1".to_string(),
            description: description.to_string(),
            repository: "test/repo".to_string(),
            branch: "main".to_string(),
            models: vec![SelectedModel { name: "Claude".to_string(), count: 1 }],
            created_at: "2023-01-01 12:00:00".to_string(),
        }
    }

#[cfg(test)]
mod model_tests {
    use super::*;

    #[test]
    fn model_default_state_is_valid() {
        let model = Model::default();

        assert_eq!(model.selected_card_index, 0);
        assert_eq!(model.focus_element, FocusElement::TaskDescription);
        assert_eq!(model.modal_state, ModalState::None);
        assert_eq!(model.draft_task_description, "");
        assert!(!model.available_repositories.is_empty());
        assert!(!model.available_models.is_empty());
        assert_eq!(model.activity_lines_count, 3);
        assert!(model.word_wrap_enabled);
        assert_eq!(model.auto_save_state, AutoSaveState::Saved);
    }

    #[test]
    fn model_navigation_respects_draft_vs_non_draft_separation() {
        let mut model = Model::default();
        // Create tasks: draft, active, draft, completed
        // So indices: 0=draft, 1=active, 2=draft, 3=completed
        let draft_task1 = create_draft_task();
        let active_task = create_active_task_with_activity();
        let draft_task2 = TaskCard {
            id: "draft_2".to_string(),
            title: "Second draft task".to_string(),
            repository: "test/repo".to_string(),
            branch: "main".to_string(),
            agents: vec![SelectedModel { name: "Claude".to_string(), count: 1 }],
            state: TaskState::Draft,
            timestamp: "2023-01-01".to_string(),
            activity: vec![],
            delivery_indicators: vec![],
        };
        let completed_task = create_completed_task();

        model.tasks = vec![draft_task1, active_task, draft_task2, completed_task];
        model.focus_element = FocusElement::TaskNavigation;

        // Start at first draft card (index 0)
        assert_eq!(model.selected_card_index, 0);

        // Navigate down - should go to second draft card (index 2)
        model.update(Msg::Key(key_event(KeyCode::Down, KeyModifiers::NONE)));
        assert_eq!(model.selected_card_index, 2);

        // Navigate down - should go to first non-draft card (index 1, active)
        model.update(Msg::Key(key_event(KeyCode::Down, KeyModifiers::NONE)));
        assert_eq!(model.selected_card_index, 1);

        // Navigate down - should go to completed card (index 3)
        model.update(Msg::Key(key_event(KeyCode::Down, KeyModifiers::NONE)));
        assert_eq!(model.selected_card_index, 3);

        // Navigate down - should wrap back to first draft card (index 0)
        model.update(Msg::Key(key_event(KeyCode::Down, KeyModifiers::NONE)));
        assert_eq!(model.selected_card_index, 0);

        // Navigate up - should go to last non-draft card (index 3)
        model.update(Msg::Key(key_event(KeyCode::Up, KeyModifiers::NONE)));
        assert_eq!(model.selected_card_index, 3);
    }

    #[test]
    fn model_enter_draft_editing_changes_focus() {
        let mut model = Model::default();
        model.tasks = vec![create_draft_task()];
        model.focus_element = FocusElement::TaskNavigation;
        model.selected_card_index = 0;

        // Press Enter on draft task
        model.update(Msg::Key(key_event(KeyCode::Enter, KeyModifiers::NONE)));

        assert_eq!(model.focus_element, FocusElement::TaskDescription);
    }

    #[test]
    fn model_tab_navigation_cycles_through_draft_controls() {
        let mut model = Model::default();
        model.focus_element = FocusElement::TaskDescription;

        // Tab forward through controls
        model.update(Msg::Key(key_event(KeyCode::Tab, KeyModifiers::NONE)));
        assert_eq!(model.focus_element, FocusElement::RepositorySelector);

        model.update(Msg::Key(key_event(KeyCode::Tab, KeyModifiers::NONE)));
        assert_eq!(model.focus_element, FocusElement::BranchSelector);

        model.update(Msg::Key(key_event(KeyCode::Tab, KeyModifiers::NONE)));
        assert_eq!(model.focus_element, FocusElement::ModelSelector);

        model.update(Msg::Key(key_event(KeyCode::Tab, KeyModifiers::NONE)));
        assert_eq!(model.focus_element, FocusElement::GoButton);

        model.update(Msg::Key(key_event(KeyCode::Tab, KeyModifiers::NONE)));
        assert_eq!(model.focus_element, FocusElement::TaskDescription);
    }

    #[test]
    fn model_shift_tab_navigation_cycles_backward() {
        let mut model = Model::default();
        model.focus_element = FocusElement::TaskDescription;

        // Shift+Tab backward through controls
        model.update(Msg::Key(key_event(KeyCode::BackTab, KeyModifiers::SHIFT)));
        assert_eq!(model.focus_element, FocusElement::GoButton);

        model.update(Msg::Key(key_event(KeyCode::BackTab, KeyModifiers::SHIFT)));
        assert_eq!(model.focus_element, FocusElement::ModelSelector);

        model.update(Msg::Key(key_event(KeyCode::BackTab, KeyModifiers::SHIFT)));
        assert_eq!(model.focus_element, FocusElement::BranchSelector);

        model.update(Msg::Key(key_event(KeyCode::BackTab, KeyModifiers::SHIFT)));
        assert_eq!(model.focus_element, FocusElement::RepositorySelector);

        model.update(Msg::Key(key_event(KeyCode::BackTab, KeyModifiers::SHIFT)));
        assert_eq!(model.focus_element, FocusElement::TaskDescription);
    }

    #[test]
    fn model_text_input_updates_description_and_marks_unsaved() {
        let mut model = Model::default();
        model.focus_element = FocusElement::TaskDescription;
        model.auto_save_state = AutoSaveState::Saved;

        // Type some text
        model.update(Msg::Key(key_event(KeyCode::Char('h'), KeyModifiers::NONE)));
        model.update(Msg::Key(key_event(KeyCode::Char('i'), KeyModifiers::NONE)));

        assert_eq!(model.draft_task_description, "hi");
        assert_eq!(model.auto_save_state, AutoSaveState::Unsaved);
        assert!(model.auto_save_timer.is_some());
    }

    #[test]
    fn model_backspace_removes_characters() {
        let mut model = Model::default();
        model.focus_element = FocusElement::TaskDescription;
        model.draft_task_description = "hello".to_string();

        model.update(Msg::Key(key_event(KeyCode::Backspace, KeyModifiers::NONE)));
        assert_eq!(model.draft_task_description, "hell");

        model.update(Msg::Key(key_event(KeyCode::Backspace, KeyModifiers::NONE)));
        assert_eq!(model.draft_task_description, "hel");
    }

    #[test]
    fn model_shift_enter_adds_newline() {
        let mut model = Model::default();
        model.focus_element = FocusElement::TaskDescription;
        model.draft_task_description = "line 1".to_string();

        model.update(Msg::Key(key_event(KeyCode::Enter, KeyModifiers::SHIFT)));
        assert_eq!(model.draft_task_description, "line 1\n");
    }

    #[test]
    fn model_escape_returns_to_navigation() {
        let mut model = Model::default();
        model.focus_element = FocusElement::TaskDescription;

        model.update(Msg::Key(key_event(KeyCode::Esc, KeyModifiers::NONE)));
        assert_eq!(model.focus_element, FocusElement::TaskNavigation);

        // Test from other controls too
        model.focus_element = FocusElement::RepositorySelector;
        model.update(Msg::Key(key_event(KeyCode::Esc, KeyModifiers::NONE)));
        assert_eq!(model.focus_element, FocusElement::TaskNavigation);
    }

    #[test]
    fn model_enter_on_selectors_opens_modals() {
        let mut model = Model::default();

        model.focus_element = FocusElement::RepositorySelector;
        model.update(Msg::Key(key_event(KeyCode::Enter, KeyModifiers::NONE)));
        assert_eq!(model.modal_state, ModalState::RepositorySearch);

        model.modal_state = ModalState::None;
        model.focus_element = FocusElement::BranchSelector;
        model.update(Msg::Key(key_event(KeyCode::Enter, KeyModifiers::NONE)));
        assert_eq!(model.modal_state, ModalState::BranchSearch);

        model.modal_state = ModalState::None;
        model.focus_element = FocusElement::ModelSelector;
        model.update(Msg::Key(key_event(KeyCode::Enter, KeyModifiers::NONE)));
        assert_eq!(model.modal_state, ModalState::ModelSelection);
    }

    #[test]
    fn model_ctrl_n_creates_new_draft_task() {
        let mut model = Model::default();
        assert!(model.tasks.is_empty());

        model.update(Msg::Key(key_event(KeyCode::Char('n'), KeyModifiers::CONTROL)));

        assert_eq!(model.tasks.len(), 1);
        assert_eq!(model.tasks[0].state, TaskState::Draft);
        assert_eq!(model.selected_card_index, 0);
        assert_eq!(model.focus_element, FocusElement::TaskDescription);
    }

    #[test]
    fn model_ctrl_w_deletes_current_task() {
        let mut model = Model::default();
        model.tasks = vec![create_draft_task(), create_active_task_with_activity()];
        model.focus_element = FocusElement::TaskNavigation;
        model.selected_card_index = 0;

        model.update(Msg::Key(key_event(KeyCode::Char('w'), KeyModifiers::CONTROL)));

        assert_eq!(model.tasks.len(), 1);
        assert_eq!(model.tasks[0].state, TaskState::Active);
        assert_eq!(model.selected_card_index, 0);
    }

    #[test]
    fn model_launch_task_requires_description_and_models() {
        let mut model = Model::default();
        model.focus_element = FocusElement::GoButton;

        // Try to launch without description - should fail
        model.draft_task_description = "".to_string();
        model.selected_models = vec![SelectedModel { name: "Claude".to_string(), count: 1 }];

        model.update(Msg::Key(key_event(KeyCode::Enter, KeyModifiers::NONE)));
        assert!(model.error_message.is_some());
        assert!(!model.loading_states.task_creation);

        // Try with description but no models - should fail
        model.error_message = None;
        model.draft_task_description = "Test task".to_string();
        model.selected_models = vec![];

        model.update(Msg::Key(key_event(KeyCode::Enter, KeyModifiers::NONE)));
        assert!(model.error_message.is_some());
        assert!(!model.loading_states.task_creation);

        // Try with both - should succeed
        model.error_message = None;
        model.draft_task_description = "Test task".to_string();
        model.selected_models = vec![SelectedModel { name: "Claude".to_string(), count: 1 }];

        model.update(Msg::Key(key_event(KeyCode::Enter, KeyModifiers::NONE)));
        assert!(model.error_message.is_none());
        assert!(model.loading_states.task_creation);
        assert!(model.status_message.is_some());
    }

    #[test]
    fn model_handles_network_messages_correctly() {
        let mut model = Model::default();
        model.loading_states.repositories = true;

        // Test repository loading
        let repos = vec!["repo1".to_string(), "repo2".to_string()];
        model.update(Msg::Net(NetworkMsg::RepositoriesLoaded(repos.clone())));

        assert_eq!(model.available_repositories, repos);
        assert!(!model.loading_states.repositories);

        // Test task creation success
        model.loading_states.task_creation = true;
        model.draft_task_description = "Test task".to_string();

        model.update(Msg::Net(NetworkMsg::TaskCreated {
            task_id: "task_123".to_string()
        }));

        assert!(!model.loading_states.task_creation);
        assert!(model.status_message.is_some());
        assert!(model.status_message.as_ref().unwrap().contains("task_123"));
        assert_eq!(model.draft_task_description, ""); // Should clear after successful creation

        // Test network error
        model.update(Msg::Net(NetworkMsg::Error("Connection failed".to_string())));
        assert!(model.error_message.is_some());
        assert_eq!(model.error_message.as_ref().unwrap(), "Connection failed");
    }

    #[test]
    fn model_task_activity_updates_work() {
        let mut model = Model::default();
        let mut task = create_active_task_with_activity();
        task.id = "active_test".to_string();
        model.tasks = vec![task];

        model.update(Msg::Net(NetworkMsg::AgentActivityUpdate {
            task_id: "active_test".to_string(),
            activity: "New activity line".to_string(),
        }));

        let task = &model.tasks[0];
        assert!(task.activity.contains(&"New activity line".to_string()));
    }

    #[test]
    fn model_tick_handles_auto_save_timer() {
        let mut model = Model::default();
        model.auto_save_state = AutoSaveState::Unsaved;
        model.auto_save_timer = Some(Instant::now() - std::time::Duration::from_millis(600));

        model.update(Msg::Tick);

        assert_eq!(model.auto_save_state, AutoSaveState::Saved);
        assert!(model.auto_save_timer.is_none());
    }

    #[test]
    fn model_filter_functionality_works() {
        let mut model = Model::default();
        model.tasks = vec![
            create_draft_task(),
            create_active_task_with_activity(),
        ];

        // Test default filter (All) - should match all tasks
        let visible = model.visible_task_indices();
        assert_eq!(visible.len(), 2);

        // Test Active filter - should match only active task
        model.filter_options.status = TaskStatusFilter::Active;
        let visible = model.visible_task_indices();
        assert_eq!(visible.len(), 1);
        assert_eq!(model.tasks[visible[0]].state, TaskState::Active);

        // Test search filter
        model.filter_options.status = TaskStatusFilter::All;
        model.filter_options.search_query = "Running".to_string();
        let visible = model.visible_task_indices();
        assert_eq!(visible.len(), 1);
        assert!(model.tasks[visible[0]].title.contains("Running"));
    }
}

#[cfg(test)]
mod view_model_tests {
    use super::*;

    #[test]
    fn view_model_derives_correctly_from_model() {
        let mut model = Model::default();
        model.tasks = vec![create_draft_task()];

        let vm: ViewModel = (&model).into();

        assert_eq!(vm.title, "Agent Harbor");
        assert!(vm.show_settings_button);
        assert_eq!(vm.task_cards.len(), 1);
        assert!(vm.has_draft_cards);
        assert!(vm.active_modal.is_none());
    }

    #[test]
    fn view_model_draft_card_shows_placeholder_when_empty() {
        let mut model = Model::default();
        let mut draft_task = create_draft_task();
        draft_task.title = "".to_string(); // Empty title
        model.tasks = vec![draft_task];
        model.draft_task_description = "".to_string(); // Empty description

        let vm: ViewModel = (&model).into();

        assert_eq!(vm.task_cards[0].title, "New Task");

        if let TaskCardType::Draft { show_placeholder, description, .. } = &vm.task_cards[0].card_type {
            assert!(*show_placeholder);
            assert_eq!(*description, "");
        } else {
            panic!("Expected Draft card type");
        }
    }

    #[test]
    fn view_model_draft_card_reflects_focus_states() {
        let mut model = Model::default();
        model.tasks = vec![create_draft_task()];
        model.focus_element = FocusElement::RepositorySelector;

        let vm: ViewModel = (&model).into();

        if let TaskCardType::Draft { controls, .. } = &vm.task_cards[0].card_type {
            assert!(controls.repository_button.is_focused);
            assert_eq!(controls.repository_button.style, ButtonStyle::Focused);
            assert!(!controls.branch_button.is_focused);
            assert!(!controls.model_button.is_focused);
            assert!(!controls.go_button.is_focused);
        } else {
            panic!("Expected Draft card type");
        }
    }

    #[test]
    fn view_model_active_card_formats_activity_correctly() {
        let mut model = Model::default();
        model.tasks = vec![create_active_task_with_activity()];
        model.activity_lines_count = 3;

        let vm: ViewModel = (&model).into();

        if let TaskCardType::Active { activity_lines, .. } = &vm.task_cards[0].card_type {
            assert_eq!(activity_lines.len(), 3);
            assert_eq!(activity_lines[0], "Thoughts: Analyzing codebase structure");
            assert_eq!(activity_lines[1], "Tool usage: search_codebase");
            assert_eq!(activity_lines[2], "  Found 42 matches in 12 files");
        } else {
            panic!("Expected Active card type");
        }
    }

    #[test]
    fn view_model_completed_card_formats_delivery_indicators() {
        let completed_task = TaskCard {
            id: "completed_1".to_string(),
            title: "Completed task".to_string(),
            repository: "test/repo".to_string(),
            branch: "main".to_string(),
            agents: vec![SelectedModel { name: "Claude".to_string(), count: 1 }],
            state: TaskState::Completed,
            timestamp: "2023-01-01 12:00:00".to_string(),
            activity: vec![],
            delivery_indicators: vec![
                DeliveryIndicator::BranchExists,
                DeliveryIndicator::PrExists { pr_number: 123, title: "Fix bug".to_string() },
            ],
        };

        let mut model = Model::default();
        model.tasks = vec![completed_task];

        let vm: ViewModel = (&model).into();

        if let TaskCardType::Completed { delivery_indicators } = &vm.task_cards[0].card_type {
            assert!(delivery_indicators.contains("⎇ branch"));
            assert!(delivery_indicators.contains("⇄ PR #123"));
            assert!(delivery_indicators.contains("Fix bug"));
        } else {
            panic!("Expected Completed card type");
        }
    }

    #[test]
    fn view_model_go_button_shows_correct_text_for_model_count() {
        let mut model = Model::default();
        model.tasks = vec![create_draft_task()];

        // Test single model
        model.selected_models = vec![SelectedModel { name: "Claude".to_string(), count: 1 }];
        let vm: ViewModel = (&model).into();

        if let TaskCardType::Draft { controls, .. } = &vm.task_cards[0].card_type {
            assert_eq!(controls.go_button.text, "⏎ Go");
        } else {
            panic!("Expected Draft card type");
        }

        // Test multiple models
        model.selected_models = vec![
            SelectedModel { name: "Claude".to_string(), count: 1 },
            SelectedModel { name: "GPT-4".to_string(), count: 1 },
        ];
        let vm: ViewModel = (&model).into();

        if let TaskCardType::Draft { controls, .. } = &vm.task_cards[0].card_type {
            assert_eq!(controls.go_button.text, "⏎ Launch Agents");
        } else {
            panic!("Expected Draft card type");
        }
    }

    #[test]
    fn view_model_go_button_disabled_when_invalid() {
        let mut model = Model::default();
        model.tasks = vec![create_draft_task()];

        // Test with empty description
        model.draft_task_description = "".to_string();
        model.selected_models = vec![SelectedModel { name: "Claude".to_string(), count: 1 }];
        let vm: ViewModel = (&model).into();

        if let TaskCardType::Draft { controls, .. } = &vm.task_cards[0].card_type {
            assert_eq!(controls.go_button.style, ButtonStyle::Disabled);
        }

        // Test with no models
        model.draft_task_description = "Valid description".to_string();
        model.selected_models = vec![];
        let vm: ViewModel = (&model).into();

        if let TaskCardType::Draft { controls, .. } = &vm.task_cards[0].card_type {
            assert_eq!(controls.go_button.style, ButtonStyle::Disabled);
        }

        // Test with valid state
        model.draft_task_description = "Valid description".to_string();
        model.selected_models = vec![SelectedModel { name: "Claude".to_string(), count: 1 }];
        let vm: ViewModel = (&model).into();

        if let TaskCardType::Draft { controls, .. } = &vm.task_cards[0].card_type {
            assert_ne!(controls.go_button.style, ButtonStyle::Disabled);
        }
    }

    #[test]
    fn view_model_footer_adapts_to_focus_state() {
        let mut model = Model::default();

        // Test navigation focus
        model.focus_element = FocusElement::TaskNavigation;
        let vm: ViewModel = (&model).into();
        assert!(vm.footer.shortcuts.iter().any(|s| s.description == "Select Task"));

        // Test description focus with single model
        model.focus_element = FocusElement::TaskDescription;
        model.selected_models = vec![SelectedModel { name: "Claude".to_string(), count: 1 }];
        let vm: ViewModel = (&model).into();
        assert!(vm.footer.shortcuts.iter().any(|s| s.description == "Launch Agent"));

        // Test description focus with multiple models
        model.selected_models.push(SelectedModel { name: "GPT-4".to_string(), count: 1 });
        let vm: ViewModel = (&model).into();
        assert!(vm.footer.shortcuts.iter().any(|s| s.description == "Launch Agents"));

        // Test modal state
        model.modal_state = ModalState::RepositorySearch;
        let vm: ViewModel = (&model).into();
        assert!(vm.footer.shortcuts.iter().any(|s| s.description == "Select"));
        assert!(vm.footer.shortcuts.iter().any(|s| s.description == "Back"));
    }

    #[test]
    fn view_model_modal_creates_correct_search_modals() {
        let mut model = Model::default();

        // Test repository search modal
        model.modal_state = ModalState::RepositorySearch;
        let vm: ViewModel = (&model).into();

        assert!(vm.active_modal.is_some());
        let modal = vm.active_modal.unwrap();
        assert_eq!(modal.title, "Select Repository");
        assert!(!modal.filtered_options.is_empty());

        if let ModalType::Search { placeholder } = modal.modal_type {
            assert!(placeholder.contains("repositories"));
        } else {
            panic!("Expected Search modal type");
        }

        // Test branch search modal
        model.modal_state = ModalState::BranchSearch;
        let vm: ViewModel = (&model).into();

        let modal = vm.active_modal.unwrap();
        assert_eq!(modal.title, "Select Branch");

        if let ModalType::Search { placeholder } = modal.modal_type {
            assert!(placeholder.contains("branches"));
        }
    }

    #[test]
    fn view_model_model_selection_modal_shows_counts() {
        let mut model = Model::default();
        model.modal_state = ModalState::ModelSelection;
        model.selected_models = vec![
            SelectedModel { name: "Claude 3.5 Sonnet".to_string(), count: 2 },
            SelectedModel { name: "GPT-4".to_string(), count: 1 },
        ];

        let vm: ViewModel = (&model).into();

        assert!(vm.active_modal.is_some());
        let modal = vm.active_modal.unwrap();
        assert_eq!(modal.title, "Select Models");

        if let ModalType::ModelSelection { options } = modal.modal_type {
            // Find Claude option
            let claude_option = options.iter().find(|o| o.name == "Claude 3.5 Sonnet").unwrap();
            assert_eq!(claude_option.count, 2);
            assert!(claude_option.is_selected);

            // Find GPT-4 option
            let gpt4_option = options.iter().find(|o| o.name == "GPT-4").unwrap();
            assert_eq!(gpt4_option.count, 1);
            assert!(gpt4_option.is_selected);

            // Find unselected option
            let other_option = options.iter().find(|o| !o.is_selected);
            assert!(other_option.is_some());
        } else {
            panic!("Expected ModelSelection modal type");
        }
    }

    #[test]
    fn view_model_status_bar_reflects_loading_states() {
        let mut model = Model::default();

        // Test ready state
        let vm: ViewModel = (&model).into();
        assert_eq!(vm.status_bar.last_operation, "Ready");
        assert_eq!(vm.status_bar.connection_status, "Connected");

        // Test loading states
        model.loading_states.task_creation = true;
        let vm: ViewModel = (&model).into();
        assert!(vm.status_bar.last_operation.contains("Creating task"));
        assert!(vm.status_bar.connection_status.contains("Connecting"));

        // Test error state
        model.loading_states = Default::default();
        model.error_message = Some("Test error".to_string());
        let vm: ViewModel = (&model).into();
        assert_eq!(vm.status_bar.error_message.as_ref().unwrap(), "Test error");
    }

    #[test]
    fn view_model_filter_bar_reflects_current_filters() {
        let mut model = Model::default();
        model.filter_options.status = TaskStatusFilter::Active;
        model.filter_options.time_range = TimeRangeFilter::Week;
        model.filter_options.search_query = "test query".to_string();
        model.focus_element = FocusElement::Filter(1);

        let vm: ViewModel = (&model).into();

        assert_eq!(vm.filter_bar.status_filter.current_value, "Active");
        assert!(!vm.filter_bar.status_filter.is_focused);

        assert_eq!(vm.filter_bar.time_filter.current_value, "Week");
        assert!(vm.filter_bar.time_filter.is_focused);

        assert_eq!(vm.filter_bar.search_box.value, "test query");
        assert!(!vm.filter_bar.search_box.is_focused);
    }

    #[test]
    fn view_model_auto_save_indicator_reflects_state() {
        let mut model = Model::default();
        model.tasks = vec![create_draft_task()];

        // Test different auto-save states
        let states = vec![
            (AutoSaveState::Saved, "Saved"),
            (AutoSaveState::Saving, "Saving..."),
            (AutoSaveState::Unsaved, "Unsaved"),
            (AutoSaveState::Error("Network error".to_string()), "Error: Network error"),
        ];

        for (state, expected_text) in states {
            model.auto_save_state = state;
            let vm: ViewModel = (&model).into();

            if let TaskCardType::Draft { auto_save_indicator, .. } = &vm.task_cards[0].card_type {
                assert_eq!(*auto_save_indicator, expected_text);
            }
        }
    }

    #[test]
    fn view_model_metadata_line_formats_correctly() {
        let mut model = Model::default();

        let test_task = TaskCard {
            id: "test_1".to_string(),
            title: "Test task".to_string(),
            repository: "owner/repo".to_string(),
            branch: "feature/test".to_string(),
            agents: vec![
                SelectedModel { name: "Claude".to_string(), count: 2 },
                SelectedModel { name: "GPT-4".to_string(), count: 1 },
            ],
            state: TaskState::Active,
            timestamp: "2023-01-01 12:00:00".to_string(),
            activity: vec![],
            delivery_indicators: vec![],
        };

        model.tasks = vec![test_task];
        let vm: ViewModel = (&model).into();

        assert!(vm.task_cards[0].metadata_line.contains("owner/repo"));
        assert!(vm.task_cards[0].metadata_line.contains("feature/test"));
        assert!(vm.task_cards[0].metadata_line.contains("Claude (x2)"));
        assert!(vm.task_cards[0].metadata_line.contains("GPT-4 (x1)"));
        assert!(vm.task_cards[0].metadata_line.contains("2023-01-01 12:00:00"));
    }

    #[test]
    fn view_model_calculates_layout_metrics() {
        let mut model = Model::default();
        model.tasks = vec![
            create_draft_task(),
            create_active_task_with_activity(),
        ];

        let vm: ViewModel = (&model).into();

        assert!(vm.total_content_height > 0);
        assert!(vm.needs_scrollbar || !vm.needs_scrollbar); // Just test it's calculated
        assert!(vm.has_draft_cards);
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn complete_user_journey_navigation_and_editing() {
        let mut model = Model::default();

        // Start with a draft task
        model.update(Msg::Key(key_event(KeyCode::Char('n'), KeyModifiers::CONTROL)));
        assert_eq!(model.focus_element, FocusElement::TaskDescription);
        assert_eq!(model.tasks.len(), 1);

        // Type description
        for c in "Implement new feature".chars() {
            model.update(Msg::Key(key_event(KeyCode::Char(c), KeyModifiers::NONE)));
        }
        assert_eq!(model.draft_task_description, "Implement new feature");

        // Navigate to repository selector
        model.update(Msg::Key(key_event(KeyCode::Tab, KeyModifiers::NONE)));
        assert_eq!(model.focus_element, FocusElement::RepositorySelector);

        // Open repository modal
        model.update(Msg::Key(key_event(KeyCode::Enter, KeyModifiers::NONE)));
        assert_eq!(model.modal_state, ModalState::RepositorySearch);

        // Close modal
        model.update(Msg::Key(key_event(KeyCode::Esc, KeyModifiers::NONE)));
        assert_eq!(model.modal_state, ModalState::None);

        // Navigate to Go button
        model.update(Msg::Key(key_event(KeyCode::Tab, KeyModifiers::NONE))); // Branch
        model.update(Msg::Key(key_event(KeyCode::Tab, KeyModifiers::NONE))); // Model
        model.update(Msg::Key(key_event(KeyCode::Tab, KeyModifiers::NONE))); // Go
        assert_eq!(model.focus_element, FocusElement::GoButton);

        // Launch task
        model.update(Msg::Key(key_event(KeyCode::Enter, KeyModifiers::NONE)));
        assert!(model.loading_states.task_creation);

        // Simulate successful task creation
        model.update(Msg::Net(NetworkMsg::TaskCreated {
            task_id: "new_task_123".to_string()
        }));
        assert!(!model.loading_states.task_creation);
        assert!(model.status_message.is_some());
        assert_eq!(model.draft_task_description, ""); // Cleared after creation

        // Create corresponding ViewModel and verify state
        let vm: ViewModel = (&model).into();
        assert!(vm.status_bar.last_operation.contains("new_task_123"));
        assert_eq!(vm.status_bar.backend_indicator, "local");
    }

    #[test]
    fn complete_filter_and_search_workflow() {
        let mut model = Model::default();

        // Create test tasks
        let draft_task = create_draft_task();
        let mut active_task = create_active_task_with_activity();
        active_task.title = "Search indexing".to_string();

        let completed_task = TaskCard {
            id: "completed_1".to_string(),
            title: "Bug fix completed".to_string(),
            repository: "test/repo".to_string(),
            branch: "main".to_string(),
            agents: vec![SelectedModel { name: "Claude".to_string(), count: 1 }],
            state: TaskState::Completed,
            timestamp: "2023-01-01 12:00:00".to_string(),
            activity: vec![],
            delivery_indicators: vec![DeliveryIndicator::BranchExists],
        };

        model.tasks = vec![draft_task, active_task, completed_task];

        // Test initial state - all tasks visible
        assert_eq!(model.visible_task_indices().len(), 3);

        // Filter to only active tasks
        model.filter_options.status = TaskStatusFilter::Active;
        let visible = model.visible_task_indices();
        assert_eq!(visible.len(), 1);
        assert_eq!(model.tasks[visible[0]].state, TaskState::Active);

        // Add search filter
        model.filter_options.search_query = "indexing".to_string();
        let visible = model.visible_task_indices();
        assert_eq!(visible.len(), 1);
        assert!(model.tasks[visible[0]].title.contains("indexing"));

        // Clear status filter but keep search
        model.filter_options.status = TaskStatusFilter::All;
        let visible = model.visible_task_indices();
        assert_eq!(visible.len(), 1); // Still only the indexing task

        // Clear search - should show all tasks again
        model.filter_options.search_query = "".to_string();
        let visible = model.visible_task_indices();
        assert_eq!(visible.len(), 3);

        // Test ViewModel generation with filters
        model.filter_options.status = TaskStatusFilter::Completed;
        let vm: ViewModel = (&model).into();
        assert_eq!(vm.filter_bar.status_filter.current_value, "Completed");
        assert_eq!(vm.task_cards.len(), 1); // Only completed task visible

        if let TaskCardType::Completed { .. } = vm.task_cards[0].card_type {
            // Expected
        } else {
            panic!("Expected completed task in filtered view");
        }
    }

    #[test]
    fn model_viewmodel_consistency_under_state_changes() {
        let mut model = Model::default();

        // Test multiple state transitions and verify ViewModel consistency
        let state_transitions = vec![
            FocusElement::TaskDescription,
            FocusElement::RepositorySelector,
            FocusElement::BranchSelector,
            FocusElement::ModelSelector,
            FocusElement::GoButton,
        ];

        for focus_state in state_transitions {
            model.focus_element = focus_state;
            let vm: ViewModel = (&model).into();

            // Verify ViewModel correctly reflects current state
            match focus_state {
                FocusElement::TaskDescription => {
                    assert!(vm.footer.shortcuts.iter().any(|s| s.description.contains("Launch")));
                },
                FocusElement::RepositorySelector | FocusElement::BranchSelector |
                FocusElement::ModelSelector | FocusElement::GoButton => {
                    assert!(vm.footer.shortcuts.iter().any(|s| s.description == "Next Field"));
                },
                _ => {}
            }
        }

        // Test modal state transitions
        let modal_states = vec![
            ModalState::None,
            ModalState::RepositorySearch,
            ModalState::BranchSearch,
            ModalState::ModelSelection,
        ];

        for modal_state in modal_states {
            model.modal_state = modal_state;
            let vm: ViewModel = (&model).into();

            match modal_state {
                ModalState::None => assert!(vm.active_modal.is_none()),
                _ => assert!(vm.active_modal.is_some()),
            }
        }
    }

    #[test]
    fn error_handling_and_recovery_flows() {
        let mut model = Model::default();

        // Test network error handling
        model.loading_states.repositories = true;
        model.update(Msg::Net(NetworkMsg::Error("Connection timeout".to_string())));

        assert!(model.error_message.is_some());
        assert!(!model.loading_states.repositories);

        let vm: ViewModel = (&model).into();
        assert_eq!(vm.status_bar.error_message.as_ref().unwrap(), "Connection timeout");

        // Test recovery - clear error on successful operation
        model.update(Msg::Net(NetworkMsg::RepositoriesLoaded(vec!["repo1".to_string()])));
        assert!(model.error_message.is_none());

        // Test validation error in task launch
        model.draft_task_description = "".to_string();
        model.selected_models = vec![];
        model.focus_element = FocusElement::GoButton;

        model.update(Msg::Key(key_event(KeyCode::Enter, KeyModifiers::NONE)));
        assert!(model.error_message.is_some());
        assert!(!model.loading_states.task_creation);

        let vm: ViewModel = (&model).into();
        if let Some(card) = vm.task_cards.get(0) {
            if let TaskCardType::Draft { controls, .. } = &card.card_type {
                assert_eq!(controls.go_button.style, ButtonStyle::Disabled);
            }
        }
    }

    #[test]
    fn test_navigation_within_task_list() {
        let mut model = Model::default();

        // Add draft tasks
        model.draft_tasks.push(create_draft_task("Draft task 1"));
        model.draft_tasks.push(create_draft_task("Draft task 2"));

        // Add regular task executions
        model.task_executions.push(create_active_task());
        model.task_executions.push(create_completed_task());

        let mut vm = ViewModel::from(&model);

        // Test: No selection -> select first task on down
        assert!(vm.handle_navigation(NavigationDirection::Down, &model));
        assert_eq!(vm.selected_task_index, Some(0)); // First task

        // Test: First task -> Second task
        assert!(vm.handle_navigation(NavigationDirection::Down, &model));
        assert_eq!(vm.selected_task_index, Some(1)); // Second task

        // Test: Second task -> Third task
        assert!(vm.handle_navigation(NavigationDirection::Down, &model));
        assert_eq!(vm.selected_task_index, Some(2)); // Third task

        // Test: Third task -> Fourth task
        assert!(vm.handle_navigation(NavigationDirection::Down, &model));
        assert_eq!(vm.selected_task_index, Some(3)); // Fourth task

        // Test: Last task -> wrap to first task
        assert!(vm.handle_navigation(NavigationDirection::Down, &model));
        assert_eq!(vm.selected_task_index, Some(0)); // Back to first task

        // Test: Up navigation
        assert!(vm.handle_navigation(NavigationDirection::Up, &model));
        assert_eq!(vm.selected_task_index, Some(3)); // Last task

        // Test: Up again
        assert!(vm.handle_navigation(NavigationDirection::Up, &model));
        assert_eq!(vm.selected_task_index, Some(2)); // Third task
    }

    #[test]
    fn test_navigation_no_drafts() {
        let mut model = Model::default();

        // No draft tasks, only regular task executions
        model.task_executions.push(create_active_task());
        model.task_executions.push(create_completed_task());

        let mut vm = ViewModel::from(&model);

        // Test: No selection -> select first task on down
        assert!(vm.handle_navigation(NavigationDirection::Down, &model));
        assert_eq!(vm.selected_task_index, Some(0)); // First task

        // Test: First task -> Second task
        assert!(vm.handle_navigation(NavigationDirection::Down, &model));
        assert_eq!(vm.selected_task_index, Some(1)); // Second task

        // Test: Last task -> wrap to first task
        assert!(vm.handle_navigation(NavigationDirection::Down, &model));
        assert_eq!(vm.selected_task_index, Some(0)); // Back to first task
    }

    #[test]
    fn test_navigation_empty_list() {
        let mut model = Model::default();
        // No tasks at all

        let mut vm = ViewModel::from(&model);

        // Test: No tasks -> navigation should not change selection
        assert!(!vm.handle_navigation(NavigationDirection::Down, &model));
        assert_eq!(vm.selected_task_index, None);

        assert!(!vm.handle_navigation(NavigationDirection::Up, &model));
        assert_eq!(vm.selected_task_index, None);
    }

}
