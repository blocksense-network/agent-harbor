//! Comprehensive ViewModel tests for PRD compliance
//!
//! These tests validate PRD-compliant behavior using the ViewModel architecture,
//! testing draft editing, navigation, task creation, and auto-save functionality.

use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

use ah_domain_types::{DeliveryStatus, DraftTask, SelectedModel, TaskExecution, TaskState};
use ah_rest_mock_client;
use ah_tui::view_model::{DraftSaveState, FocusElement, ModalState};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tui_exploration::settings::KeyboardOperation;
use tui_exploration::view_model::*;

/// Helper function to create a test ViewModel with mock dependencies
fn create_test_view_model() -> ViewModel {
    let workspace_files = Box::new(tui_exploration::workspace_files::GitWorkspaceFiles::new(
        std::path::PathBuf::from("."),
    ));
    let workspace_workflows = Box::new(ah_workflows::WorkflowProcessor::new(
        ah_workflows::WorkflowConfig::default(),
    ));
    let task_manager = Box::new(ah_rest_mock_client::MockRestClient::new());
    let settings = tui_exploration::settings::Settings::default();

    ViewModel::new(workspace_files, workspace_workflows, task_manager, settings)
}

/// Helper function to create a test key event
fn key_event(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent {
        code,
        modifiers,
        kind: crossterm::event::KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    }
}

fn create_viewmodel_test_log(test_name: &str) -> (std::fs::File, std::path::PathBuf) {
    let mut dir = std::env::temp_dir();
    dir.push("ah_tui_vm_logs");
    std::fs::create_dir_all(&dir).expect("create log directory");

    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).expect("valid time");
    let file_name = format!(
        "{}_{}_{}.log",
        test_name,
        std::process::id(),
        timestamp.as_nanos()
    );
    dir.push(file_name);
    let file = std::fs::File::create(&dir).expect("create log file");
    (file, dir)
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
    fn viewmodel_tab_navigation_cycles_through_draft_card_controls() {
        // Test PRD requirement: "TAB navigation between controls" for draft cards
        let mut vm = create_test_view_model();

        // Start with focus on draft task (initial state)
        assert_eq!(vm.focus_element, FocusElement::DraftTask(0));

        // Check that the draft card's internal focus starts on TaskDescription
        if let Some(card) = vm.draft_cards.get(0) {
            assert_eq!(card.focus_element, FocusElement::TaskDescription);
        }

        // Tab from draft task should cycle through the card's internal controls
        assert!(vm.focus_next_control());
        // Global focus should remain on the draft task
        assert_eq!(vm.focus_element, FocusElement::DraftTask(0));
        // But internal focus should move to RepositorySelector
        if let Some(card) = vm.draft_cards.get(0) {
            assert_eq!(card.focus_element, FocusElement::RepositorySelector);
        }

        assert!(vm.focus_next_control());
        assert_eq!(vm.focus_element, FocusElement::DraftTask(0));
        if let Some(card) = vm.draft_cards.get(0) {
            assert_eq!(card.focus_element, FocusElement::BranchSelector);
        }

        assert!(vm.focus_next_control());
        assert_eq!(vm.focus_element, FocusElement::DraftTask(0));
        if let Some(card) = vm.draft_cards.get(0) {
            assert_eq!(card.focus_element, FocusElement::ModelSelector);
        }

        assert!(vm.focus_next_control());
        assert_eq!(vm.focus_element, FocusElement::DraftTask(0));
        if let Some(card) = vm.draft_cards.get(0) {
            assert_eq!(card.focus_element, FocusElement::GoButton);
        }

        // Next tab should cycle back to TaskDescription
        assert!(vm.focus_next_control());
        assert_eq!(vm.focus_element, FocusElement::DraftTask(0));
        if let Some(card) = vm.draft_cards.get(0) {
            assert_eq!(card.focus_element, FocusElement::TaskDescription);
        }
    }

    #[test]
    fn viewmodel_shift_tab_navigation_cycles_backward_through_card_controls() {
        // Test PRD requirement: "TAB navigation between controls" (reverse direction)
        let mut vm = create_test_view_model();

        // Start with draft task focused (initial state)
        vm.focus_element = FocusElement::DraftTask(0);

        // Shift+Tab backward through card internal controls
        assert!(vm.focus_previous_control());
        // Global focus should remain on the draft task
        assert_eq!(vm.focus_element, FocusElement::DraftTask(0));
        // But internal focus should move to GoButton
        if let Some(card) = vm.draft_cards.get(0) {
            assert_eq!(card.focus_element, FocusElement::GoButton);
        }

        assert!(vm.focus_previous_control());
        assert_eq!(vm.focus_element, FocusElement::DraftTask(0));
        if let Some(card) = vm.draft_cards.get(0) {
            assert_eq!(card.focus_element, FocusElement::ModelSelector);
        }

        assert!(vm.focus_previous_control());
        assert_eq!(vm.focus_element, FocusElement::DraftTask(0));
        if let Some(card) = vm.draft_cards.get(0) {
            assert_eq!(card.focus_element, FocusElement::BranchSelector);
        }

        assert!(vm.focus_previous_control());
        assert_eq!(vm.focus_element, FocusElement::DraftTask(0));
        if let Some(card) = vm.draft_cards.get(0) {
            assert_eq!(card.focus_element, FocusElement::RepositorySelector);
        }

        // Next shift+tab should cycle back to TaskDescription
        assert!(vm.focus_previous_control());
        assert_eq!(vm.focus_element, FocusElement::DraftTask(0));
        if let Some(card) = vm.draft_cards.get(0) {
            assert_eq!(card.focus_element, FocusElement::TaskDescription);
        }
    }

    #[test]
    fn viewmodel_text_input_updates_draft_description() {
        // Test PRD requirement: text input in draft description
        let mut vm = create_test_view_model();

        // Start with draft task focused (global focus)
        vm.focus_element = FocusElement::DraftTask(0);

        // Verify the card's internal focus is on TaskDescription
        if let Some(card) = vm.draft_cards.get(0) {
            assert_eq!(card.focus_element, FocusElement::TaskDescription);
        }

        // Type some text - should work because card internal focus is TaskDescription
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

        // Start with draft task focused and add some text
        vm.focus_element = FocusElement::DraftTask(0);
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

        // Start with draft task focused and add some text
        vm.focus_element = FocusElement::DraftTask(0);
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

        // Test repository selector (global focus)
        vm.focus_element = FocusElement::RepositorySelector;
        assert!(vm.handle_enter(false));
        assert_eq!(vm.modal_state, ModalState::RepositorySearch);

        // Reset modal state
        vm.modal_state = ModalState::None;

        // Test branch selector (global focus)
        vm.focus_element = FocusElement::BranchSelector;
        assert!(vm.handle_enter(false));
        assert_eq!(vm.modal_state, ModalState::BranchSearch);

        // Reset modal state
        vm.modal_state = ModalState::None;

        // Test model selector (global focus)
        vm.focus_element = FocusElement::ModelSelector;
        assert!(vm.handle_enter(false));
        assert_eq!(vm.modal_state, ModalState::ModelSearch);

        // Reset modal state
        vm.modal_state = ModalState::None;

        // Test settings button (global focus)
        vm.focus_element = FocusElement::SettingsButton;
        assert!(vm.handle_enter(false));
        assert_eq!(vm.modal_state, ModalState::Settings);
    }

    #[test]
    fn viewmodel_enter_on_draft_card_activates_based_on_internal_focus() {
        // Test PRD requirement: Enter key on draft card activates based on internal focus
        let mut vm = create_test_view_model();

        // Start with draft task focused
        vm.focus_element = FocusElement::DraftTask(0);

        // Initially, card internal focus should be on TaskDescription
        if let Some(card) = vm.draft_cards.get(0) {
            assert_eq!(card.focus_element, FocusElement::TaskDescription);
        }

        // Enter with TaskDescription focused should launch task (same as Go button)
        // This will fail validation since description is empty, but that's expected
        assert!(!vm.handle_enter(false)); // Should return false due to validation failure
        assert!(vm.status_bar.error_message.is_some());

        // Now test with repository selector focused internally
        if let Some(card) = vm.draft_cards.get_mut(0) {
            card.focus_element = FocusElement::RepositorySelector;
        }
        vm.modal_state = ModalState::None; // Reset modal state
        vm.status_bar.error_message = None; // Reset error

        assert!(vm.handle_enter(false));
        assert_eq!(vm.modal_state, ModalState::RepositorySearch);

        // Test branch selector
        if let Some(card) = vm.draft_cards.get_mut(0) {
            card.focus_element = FocusElement::BranchSelector;
        }
        vm.modal_state = ModalState::None;

        assert!(vm.handle_enter(false));
        assert_eq!(vm.modal_state, ModalState::BranchSearch);

        // Test model selector
        if let Some(card) = vm.draft_cards.get_mut(0) {
            card.focus_element = FocusElement::ModelSelector;
        }
        vm.modal_state = ModalState::None;

        assert!(vm.handle_enter(false));
        assert_eq!(vm.modal_state, ModalState::ModelSearch);

        // Test go button
        if let Some(card) = vm.draft_cards.get_mut(0) {
            card.focus_element = FocusElement::GoButton;
        }
        vm.modal_state = ModalState::None;

        // Should try to launch task (will fail validation again)
        assert!(!vm.handle_enter(false));
        assert!(vm.status_bar.error_message.is_some());
    }

    #[test]
    fn viewmodel_escape_requires_double_press_to_request_exit() {
        let (mut log, log_path) = create_viewmodel_test_log("escape_double_press");
        let log_hint = log_path.display().to_string();

        let mut vm = create_test_view_model();
        vm.focus_element = FocusElement::TaskDescription;

        // First ESC should arm the exit confirmation state
        assert!(
            vm.handle_escape(),
            "first ESC should be handled (log: {log_hint})"
        );
        writeln!(
            log,
            "After first ESC -> armed {} requested {}",
            vm.exit_confirmation_armed, vm.exit_requested
        )
        .expect("write log");
        assert!(
            vm.exit_confirmation_armed,
            "first ESC arms exit (log: {log_hint})"
        );
        assert!(
            !vm.exit_requested,
            "no exit request after first ESC (log: {log_hint})"
        );

        // Any other key should discharge the confirmation state
        assert!(
            vm.handle_keyboard_operation(
                KeyboardOperation::MoveToNextField,
                &key_event(KeyCode::Tab, KeyModifiers::empty()),
            ),
            "Tab should remain handled (log: {log_hint})"
        );
        assert!(
            !vm.exit_confirmation_armed,
            "non-ESC key discharges confirmation (log: {log_hint})"
        );

        // Press ESC twice without other keys to request exit
        vm.handle_escape();
        assert!(vm.exit_confirmation_armed);
        vm.handle_escape();
        writeln!(
            log,
            "After second ESC -> armed {} requested {}",
            vm.exit_confirmation_armed, vm.exit_requested
        )
        .expect("write log");
        assert!(
            vm.exit_requested,
            "second ESC requests exit (log: {log_hint})"
        );
        assert!(
            vm.take_exit_request(),
            "take_exit_request returns true (log: {log_hint})"
        );
        assert!(
            !vm.exit_requested,
            "exit request clears after take_exit_request (log: {log_hint})"
        );
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
        // Global focus should be on the new draft task
        assert_eq!(
            vm.focus_element,
            FocusElement::DraftTask(initial_draft_count)
        );
        // The new card's internal focus should be on TaskDescription
        if let Some(card) = vm.draft_cards.get(initial_draft_count) {
            assert_eq!(card.focus_element, FocusElement::TaskDescription);
        }
    }

    #[test]
    fn viewmodel_auto_save_timer_gets_set_on_text_input() {
        // Test PRD requirement: Auto-save functionality
        let mut vm = create_test_view_model();

        // Start with draft task focused (global focus)
        vm.focus_element = FocusElement::DraftTask(0);

        // Type some text (should work since card internal focus is on TaskDescription)
        vm.handle_char_input('T');
        vm.handle_char_input('e');
        vm.handle_char_input('x');
        vm.handle_char_input('t');

        // Auto-save timer should be set on the draft card
        if let Some(card) = vm.draft_cards.get(0) {
            assert!(card.auto_save_timer.is_some());
            assert_eq!(card.save_state, DraftSaveState::Unsaved);
        } else {
            panic!("Expected draft card");
        }
    }

    #[test]
    fn viewmodel_auto_save_timer_expires_after_delay() {
        // Test PRD requirement: Auto-save timer expiration
        let mut vm = create_test_view_model();

        // Start with draft task focused and type something to set the timer
        vm.focus_element = FocusElement::DraftTask(0);
        vm.handle_char_input('T');

        // Get the first draft card and manually set its timer to expired state
        if let Some(card) = vm.draft_cards.get_mut(0) {
            card.auto_save_timer =
                Some(std::time::Instant::now() - std::time::Duration::from_millis(600));
        }

        // Handle tick should mark as saved
        assert!(vm.handle_tick());

        // Check the first draft card
        if let Some(card) = vm.draft_cards.get(0) {
            assert_eq!(card.save_state, DraftSaveState::Saved);
            assert!(card.auto_save_timer.is_none());
        }
    }
}
