//! Comprehensive ViewModel tests for PRD compliance
//!
//! These tests validate PRD-compliant behavior using the ViewModel architecture,
//! testing draft editing, navigation, task creation, and auto-save functionality.

use tui_exploration::view_model::*;
use ah_domain_types::{DraftTask, SelectedModel, TaskState, DeliveryStatus, TaskExecution};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Helper function to create a test ViewModel with mock dependencies
fn create_test_view_model() -> ViewModel {
    let workspace_files = Box::new(tui_exploration::workspace_files::GitWorkspaceFiles::new(std::path::PathBuf::from(".")));
    let workspace_workflows = Box::new(tui_exploration::workspace_workflows::PathWorkspaceWorkflows::new(std::path::PathBuf::from(".")));
    let task_manager = Box::new(tui_exploration::task_manager::MockTaskManager::new());
    let settings = tui_exploration::settings::Settings::default();

    ViewModel::new(workspace_files, workspace_workflows, task_manager, settings)
}

/// Helper function to create a test key event
fn key_event(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent { code, modifiers, kind: crossterm::event::KeyEventKind::Press, state: crossterm::event::KeyEventState::empty() }
}

#[cfg(test)]
mod viewmodel_tests {
    use super::*;

    #[test]
    fn viewmodel_initial_focus_is_draft_task_description() {
        // Test PRD requirement: "The initially focused element is the top draft task card."
        let vm = create_test_view_model();
        assert_eq!(vm.focus_element, FocusElement::DraftTask(0));
    }

    #[test]
    fn viewmodel_tab_navigation_cycles_through_draft_controls() {
        // Test PRD requirement: "TAB navigation between controls" for draft cards
        let mut vm = create_test_view_model();

        // Start with focus on draft task (initial state)
        assert_eq!(vm.focus_element, FocusElement::DraftTask(0));

        // Tab from draft task should start cycling through controls at RepositorySelector
        assert!(vm.focus_next_control());
        assert_eq!(vm.focus_element, FocusElement::RepositorySelector);

        assert!(vm.focus_next_control());
        assert_eq!(vm.focus_element, FocusElement::BranchSelector);

        assert!(vm.focus_next_control());
        assert_eq!(vm.focus_element, FocusElement::ModelSelector);

        assert!(vm.focus_next_control());
        assert_eq!(vm.focus_element, FocusElement::GoButton);

        // Next tab should cycle back to RepositorySelector
        assert!(vm.focus_next_control());
        assert_eq!(vm.focus_element, FocusElement::RepositorySelector);
    }

    #[test]
    fn viewmodel_shift_tab_navigation_cycles_backward() {
        // Test PRD requirement: "TAB navigation between controls" (reverse direction)
        let mut vm = create_test_view_model();

        // Start in draft editing mode
        vm.focus_element = FocusElement::TaskDescription;

        // Shift+Tab backward through controls
        assert!(vm.focus_previous_control());
        assert_eq!(vm.focus_element, FocusElement::GoButton);

        assert!(vm.focus_previous_control());
        assert_eq!(vm.focus_element, FocusElement::ModelSelector);

        assert!(vm.focus_previous_control());
        assert_eq!(vm.focus_element, FocusElement::BranchSelector);

        assert!(vm.focus_previous_control());
        assert_eq!(vm.focus_element, FocusElement::RepositorySelector);

        // Next shift+tab should cycle back to TaskDescription
        assert!(vm.focus_previous_control());
        assert_eq!(vm.focus_element, FocusElement::TaskDescription);
    }

    #[test]
    fn viewmodel_text_input_updates_draft_description() {
        // Test PRD requirement: text input in draft description
        let mut vm = create_test_view_model();

        // Enter draft editing mode (focus on description)
        vm.focus_element = FocusElement::TaskDescription;

        // Type some text
        assert!(vm.handle_char_input('H'));
        assert!(vm.handle_char_input('e'));
        assert!(vm.handle_char_input('l'));
        assert!(vm.handle_char_input('l'));
        assert!(vm.handle_char_input('o'));

        // Check that the draft description was updated
        if let Some(card) = vm.draft_cards.get(0) {
            assert_eq!(card.description.lines().join("\n"), "Hello");
        } else {
            panic!("Expected draft card");
        }
    }

    #[test]
    fn viewmodel_backspace_removes_characters() {
        // Test PRD requirement: backspace functionality in text input
        let mut vm = create_test_view_model();

        // Focus on task description and add some text
        vm.focus_element = FocusElement::TaskDescription;
        vm.handle_char_input('H');
        vm.handle_char_input('e');
        vm.handle_char_input('l');
        vm.handle_char_input('l');
        vm.handle_char_input('o');

        // Check initial state
        if let Some(card) = vm.draft_cards.get(0) {
            assert_eq!(card.description.lines().join("\n"), "Hello");
        }

        // Backspace should remove characters
        assert!(vm.handle_backspace());
        if let Some(card) = vm.draft_cards.get(0) {
            assert_eq!(card.description.lines().join("\n"), "Hell");
        }

        assert!(vm.handle_backspace());
        if let Some(card) = vm.draft_cards.get(0) {
            assert_eq!(card.description.lines().join("\n"), "Hel");
        }
    }

    #[test]
    fn viewmodel_shift_enter_adds_newline() {
        // Test PRD requirement: "Shift+Enter creates a new line in the text area"
        let mut vm = create_test_view_model();

        // Focus on task description and add some text
        vm.focus_element = FocusElement::TaskDescription;
        vm.handle_char_input('L');
        vm.handle_char_input('i');
        vm.handle_char_input('n');
        vm.handle_char_input('e');
        vm.handle_char_input(' ');

        // Check initial state
        if let Some(card) = vm.draft_cards.get(0) {
            assert_eq!(card.description.lines().join("\n"), "Line ");
        }

        // Shift+Enter should add a newline
        assert!(vm.handle_enter(true)); // true = shift modifier

        if let Some(card) = vm.draft_cards.get(0) {
            assert_eq!(card.description.lines().join("\n"), "Line \n");
        } else {
            panic!("Expected draft card");
        }
    }

    #[test]
    fn viewmodel_enter_opens_modal_for_selectors() {
        // Test PRD requirement: Enter key activates modal dialogs for selectors
        let mut vm = create_test_view_model();

        // Test repository selector
        vm.focus_element = FocusElement::RepositorySelector;
        assert!(vm.handle_enter(false));
        assert_eq!(vm.modal_state, ModalState::RepositorySearch);

        // Reset modal state
        vm.modal_state = ModalState::None;

        // Test branch selector
        vm.focus_element = FocusElement::BranchSelector;
        assert!(vm.handle_enter(false));
        assert_eq!(vm.modal_state, ModalState::BranchSearch);

        // Reset modal state
        vm.modal_state = ModalState::None;

        // Test model selector
        vm.focus_element = FocusElement::ModelSelector;
        assert!(vm.handle_enter(false));
        assert_eq!(vm.modal_state, ModalState::ModelSearch);

        // Reset modal state
        vm.modal_state = ModalState::None;

        // Test settings button
        vm.focus_element = FocusElement::SettingsButton;
        assert!(vm.handle_enter(false));
        assert_eq!(vm.modal_state, ModalState::Settings);
    }

    #[test]
    fn viewmodel_escape_returns_to_navigation_mode() {
        // Test PRD requirement: Esc key returns from editing to navigation mode
        let mut vm = create_test_view_model();

        // Start in draft editing mode
        vm.focus_element = FocusElement::TaskDescription;

        // Escape should return to draft task navigation
        assert!(vm.handle_escape());
        assert_eq!(vm.focus_element, FocusElement::DraftTask(0));
    }

    #[test]
    fn viewmodel_task_launch_validation_requires_description() {
        // Test PRD requirement: Task launch validation
        let mut vm = create_test_view_model();

        // Try to launch without description
        vm.focus_element = FocusElement::GoButton;

        // Should fail validation
        assert!(!vm.handle_go_button()); // Should return false for validation failure
        assert!(vm.status_bar.error_message.is_some());
        assert!(vm.status_bar.error_message.as_ref().unwrap().contains("required"));
    }

    #[test]
    fn viewmodel_task_launch_validation_requires_models() {
        // Test PRD requirement: Task launch validation
        let mut vm = create_test_view_model();

        // Add description and clear models
        vm.focus_element = FocusElement::TaskDescription;
        vm.handle_char_input('T');
        vm.handle_char_input('e');
        vm.handle_char_input('s');
        vm.handle_char_input('t');

        // Clear the models to make validation fail
        if let Some(card) = vm.draft_cards.get_mut(0) {
            card.models.clear();
        }

        vm.focus_element = FocusElement::GoButton;

        // Should fail validation
        assert!(!vm.handle_go_button()); // Should return false for validation failure
        assert!(vm.status_bar.error_message.is_some());
        assert!(vm.status_bar.error_message.as_ref().unwrap().contains("model"));
    }

    #[test]
    fn viewmodel_ctrl_n_creates_new_draft_task() {
        // Test PRD requirement: Ctrl+N creates new draft task
        let mut vm = create_test_view_model();

        let initial_draft_count = vm.draft_cards.len();

        // Send Ctrl+N
        let ctrl_n_event = key_event(KeyCode::Char('n'), KeyModifiers::CONTROL);
        assert!(vm.handle_key_event(ctrl_n_event));

        // Should have created a new draft task
        assert_eq!(vm.draft_cards.len(), initial_draft_count + 1);
        assert_eq!(vm.focus_element, FocusElement::TaskDescription);
    }

    #[test]
    fn viewmodel_auto_save_timer_gets_set_on_text_input() {
        // Test PRD requirement: Auto-save functionality
        let mut vm = create_test_view_model();

        // Focus on task description for editing
        vm.focus_element = FocusElement::TaskDescription;

        // Type some text
        vm.handle_char_input('T');
        vm.handle_char_input('e');
        vm.handle_char_input('x');
        vm.handle_char_input('t');

        // Auto-save timer should be set on the draft card (even though focus is on description)
        if let Some(card) = vm.draft_cards.get(0) {
            assert!(card.auto_save_timer.is_some());
            assert_eq!(card.save_state, tui_exploration::view_model::DraftSaveState::Unsaved);
        } else {
            panic!("Expected draft card");
        }
    }

    #[test]
    fn viewmodel_auto_save_timer_expires_after_delay() {
        // Test PRD requirement: Auto-save timer expiration
        let mut vm = create_test_view_model();

        // Focus on task description and type something to set the timer
        vm.focus_element = FocusElement::TaskDescription;
        vm.handle_char_input('T');

        // Get the first draft card and manually set its timer to expired state
        if let Some(card) = vm.draft_cards.get_mut(0) {
            card.auto_save_timer = Some(std::time::Instant::now() - std::time::Duration::from_millis(600));
        }

        // Handle tick should mark as saved
        assert!(vm.handle_tick());

        // Check the first draft card
        if let Some(card) = vm.draft_cards.get(0) {
            assert_eq!(card.save_state, tui_exploration::view_model::DraftSaveState::Saved);
            assert!(card.auto_save_timer.is_none());
        }
    }
}
