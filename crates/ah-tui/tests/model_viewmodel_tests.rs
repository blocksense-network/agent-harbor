// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Comprehensive ViewModel tests for PRD compliance
//!
//! These tests validate PRD-compliant behavior using the ViewModel architecture,
//! testing draft editing, navigation, task creation, and auto-save functionality.

use std::io::Write;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ah_core::{
    BranchesEnumerator, DefaultWorkspaceTermsEnumerator, RepositoriesEnumerator, TaskManager,
    WorkspaceFilesEnumerator, WorkspaceTermsEnumerator, task_manager::SaveDraftResult,
};
use ah_domain_types::{AgentChoice, AgentSoftware, AgentSoftwareBuild};
use ah_repo::VcsRepo;
use ah_rest_mock_client::MockRestClient;
use ah_tui::settings::KeyboardOperation;
use ah_tui::view_model::agents_selector_model::FilteredOption;
use ah_tui::view_model::task_entry::CardFocusElement;
use ah_tui::view_model::{
    DashboardFocusState, DraftSaveState, ModalState, ModalType, Msg, ViewModel,
};
use ah_workflows::{WorkflowConfig, WorkflowProcessor, WorkspaceWorkflowsEnumerator};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Helper function to create a test ViewModel with mock dependencies
fn create_test_view_model() -> ViewModel {
    let (vm, _rx) = create_test_view_model_with_channel();
    vm
}

fn create_test_view_model_with_channel() -> (ViewModel, crossbeam_channel::Receiver<Msg>) {
    let workspace_files: Arc<dyn WorkspaceFilesEnumerator> =
        Arc::new(VcsRepo::new(std::path::PathBuf::from(".")).unwrap());
    let workspace_workflows: Arc<dyn WorkspaceWorkflowsEnumerator> =
        Arc::new(WorkflowProcessor::new(WorkflowConfig::default()));
    let workspace_terms: Arc<dyn WorkspaceTermsEnumerator> = Arc::new(
        DefaultWorkspaceTermsEnumerator::new(Arc::clone(&workspace_files)),
    );
    let task_manager: Arc<dyn TaskManager> = Arc::new(MockRestClient::new());
    let mock_client = MockRestClient::new();
    let repositories_enumerator: Arc<dyn RepositoriesEnumerator> = Arc::new(
        ah_core::RemoteRepositoriesEnumerator::new(mock_client.clone(), "http://test".to_string()),
    );
    let branches_enumerator: Arc<dyn BranchesEnumerator> = Arc::new(
        ah_core::RemoteBranchesEnumerator::new(mock_client, "http://test".to_string()),
    );
    let agents_enumerator: Arc<dyn ah_core::AgentsEnumerator> =
        Arc::new(ah_core::agent_catalog::MockAgentsEnumerator::new(
            ah_core::agent_catalog::RemoteAgentCatalog::default_catalog(),
        ));
    let settings = ah_tui::settings::Settings::from_config()
        .unwrap_or_else(|_| ah_tui::settings::Settings::default());
    let (ui_tx, ui_rx) = crossbeam_channel::unbounded();

    (
        ViewModel::new(
            workspace_files,
            workspace_workflows,
            workspace_terms,
            task_manager,
            repositories_enumerator,
            branches_enumerator,
            agents_enumerator,
            settings,
            ui_tx,
        ),
        ui_rx,
    )
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
        assert_eq!(vm.focus_element, DashboardFocusState::DraftTask(0));
    }

    #[test]
    fn viewmodel_tab_navigation_cycles_through_draft_card_controls() {
        // Test PRD requirement: "TAB navigation between controls" for draft cards
        let mut vm = create_test_view_model();

        // Start with focus on draft task (initial state)
        assert_eq!(vm.focus_element, DashboardFocusState::DraftTask(0));

        // Check that the draft card's internal focus starts on TaskDescription
        if let Some(card) = vm.draft_cards.first() {
            assert_eq!(card.focus_element, CardFocusElement::TaskDescription);
        }

        // Tab from draft task should cycle through the card's internal controls
        assert!(vm.focus_next_control());
        // Global focus should remain on the draft task
        assert_eq!(vm.focus_element, DashboardFocusState::DraftTask(0));
        // But internal focus should move to RepositorySelector
        if let Some(card) = vm.draft_cards.first() {
            assert_eq!(card.focus_element, CardFocusElement::RepositorySelector);
        }

        assert!(vm.focus_next_control());
        assert_eq!(vm.focus_element, DashboardFocusState::DraftTask(0));
        if let Some(card) = vm.draft_cards.first() {
            assert_eq!(card.focus_element, CardFocusElement::BranchSelector);
        }

        assert!(vm.focus_next_control());
        assert_eq!(vm.focus_element, DashboardFocusState::DraftTask(0));
        if let Some(card) = vm.draft_cards.first() {
            assert_eq!(card.focus_element, CardFocusElement::ModelSelector);
        }

        assert!(vm.focus_next_control());
        assert_eq!(vm.focus_element, DashboardFocusState::DraftTask(0));
        if let Some(card) = vm.draft_cards.first() {
            assert_eq!(card.focus_element, CardFocusElement::GoButton);
        }

        assert!(vm.focus_next_control());
        assert_eq!(vm.focus_element, DashboardFocusState::DraftTask(0));
        if let Some(card) = vm.draft_cards.first() {
            assert_eq!(card.focus_element, CardFocusElement::AdvancedOptionsButton);
        }

        // Next tab should cycle back to TaskDescription
        assert!(vm.focus_next_control());
        assert_eq!(vm.focus_element, DashboardFocusState::DraftTask(0));
        if let Some(card) = vm.draft_cards.first() {
            assert_eq!(card.focus_element, CardFocusElement::TaskDescription);
        }
    }

    #[test]
    fn viewmodel_shift_tab_navigation_cycles_backward_through_card_controls() {
        // Test PRD requirement: "TAB navigation between controls" (reverse direction)
        let mut vm = create_test_view_model();

        // Start with draft task focused (initial state)
        vm.focus_element = DashboardFocusState::DraftTask(0);

        // Shift+Tab backward through card internal controls
        assert!(vm.focus_previous_control());
        // Global focus should remain on the draft task
        assert_eq!(vm.focus_element, DashboardFocusState::DraftTask(0));
        // But internal focus should move to AdvancedOptionsButton
        if let Some(card) = vm.draft_cards.first() {
            assert_eq!(card.focus_element, CardFocusElement::AdvancedOptionsButton);
        }

        assert!(vm.focus_previous_control());
        assert_eq!(vm.focus_element, DashboardFocusState::DraftTask(0));
        if let Some(card) = vm.draft_cards.first() {
            assert_eq!(card.focus_element, CardFocusElement::GoButton);
        }

        assert!(vm.focus_previous_control());
        assert_eq!(vm.focus_element, DashboardFocusState::DraftTask(0));
        if let Some(card) = vm.draft_cards.first() {
            assert_eq!(card.focus_element, CardFocusElement::ModelSelector);
        }

        assert!(vm.focus_previous_control());
        assert_eq!(vm.focus_element, DashboardFocusState::DraftTask(0));
        if let Some(card) = vm.draft_cards.first() {
            assert_eq!(card.focus_element, CardFocusElement::BranchSelector);
        }

        assert!(vm.focus_previous_control());
        assert_eq!(vm.focus_element, DashboardFocusState::DraftTask(0));
        if let Some(card) = vm.draft_cards.first() {
            assert_eq!(card.focus_element, CardFocusElement::RepositorySelector);
        }

        // Next shift+tab should cycle back to TaskDescription
        assert!(vm.focus_previous_control());
        assert_eq!(vm.focus_element, DashboardFocusState::DraftTask(0));
        if let Some(card) = vm.draft_cards.first() {
            assert_eq!(card.focus_element, CardFocusElement::TaskDescription);
        }
    }

    #[test]
    fn viewmodel_text_input_updates_draft_description() {
        // Test PRD requirement: text input in draft description
        let mut vm = create_test_view_model();

        // Start with draft task focused (global focus)
        vm.focus_element = DashboardFocusState::DraftTask(0);

        // Verify the card's internal focus is on TaskDescription
        if let Some(card) = vm.draft_cards.first() {
            assert_eq!(card.focus_element, CardFocusElement::TaskDescription);
        }

        // Type some text - should work because card internal focus is TaskDescription
        assert!(vm.handle_char_input('H'));
        assert!(vm.handle_char_input('e'));
        assert!(vm.handle_char_input('l'));
        assert!(vm.handle_char_input('l'));
        assert!(vm.handle_char_input('o'));

        // Check that the draft description was updated
        if let Some(card) = vm.draft_cards.first() {
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
        vm.focus_element = DashboardFocusState::DraftTask(0);
        vm.handle_char_input('H');
        vm.handle_char_input('e');
        vm.handle_char_input('l');
        vm.handle_char_input('l');
        vm.handle_char_input('o');

        // Check initial state
        if let Some(card) = vm.draft_cards.first() {
            assert_eq!(card.description.lines().join("\n"), "Hello");
        }

        // Backspace should remove characters
        assert!(vm.handle_backspace());
        if let Some(card) = vm.draft_cards.first() {
            assert_eq!(card.description.lines().join("\n"), "Hell");
        }

        assert!(vm.handle_backspace());
        if let Some(card) = vm.draft_cards.first() {
            assert_eq!(card.description.lines().join("\n"), "Hel");
        }
    }

    #[test]
    fn viewmodel_shift_enter_adds_newline() {
        // Test PRD requirement: "Shift+Enter creates a new line in the text area"
        let mut vm = create_test_view_model();

        // Start with draft task focused and add some text
        vm.focus_element = DashboardFocusState::DraftTask(0);
        vm.handle_char_input('L');
        vm.handle_char_input('i');
        vm.handle_char_input('n');
        vm.handle_char_input('e');
        vm.handle_char_input(' ');

        // Check initial state
        if let Some(card) = vm.draft_cards.first() {
            assert_eq!(card.description.lines().join("\n"), "Line ");
        }

        // Shift+Enter should add a newline
        assert!(vm.handle_enter(true)); // true = shift modifier

        if let Some(card) = vm.draft_cards.first() {
            assert_eq!(card.description.lines().join("\n"), "Line \n");
        } else {
            panic!("Expected draft card");
        }
    }

    #[test]
    fn viewmodel_enter_opens_modal_for_selectors() {
        // Test PRD requirement: Enter key activates modal dialogs for selectors
        let mut vm = create_test_view_model();

        // Test repository selector (card internal focus)
        vm.focus_element = DashboardFocusState::DraftTask(0);
        vm.draft_cards[0].focus_element = CardFocusElement::RepositorySelector;
        assert!(vm.handle_enter(false));
        assert_eq!(vm.modal_state, ModalState::RepositorySearch);

        // Reset modal state
        vm.modal_state = ModalState::None;

        // Test branch selector (card internal focus)
        vm.draft_cards[0].focus_element = CardFocusElement::BranchSelector;
        assert!(vm.handle_enter(false));
        assert_eq!(vm.modal_state, ModalState::BranchSearch);

        // Reset modal state
        vm.modal_state = ModalState::None;

        // Test model selector (card internal focus)
        vm.draft_cards[0].focus_element = CardFocusElement::ModelSelector;
        assert!(vm.handle_enter(false));
        assert_eq!(vm.modal_state, ModalState::ModelSearch);

        // Reset modal state
        vm.modal_state = ModalState::None;

        // Test settings button (global focus)
        vm.focus_element = DashboardFocusState::SettingsButton;
        assert!(vm.handle_enter(false));
        assert_eq!(vm.modal_state, ModalState::Settings);
    }

    #[test]
    fn viewmodel_enter_on_draft_card_activates_based_on_internal_focus() {
        // Test PRD requirement: Enter key on draft card activates based on internal focus
        let mut vm = create_test_view_model();

        // Start with draft task focused
        vm.focus_element = DashboardFocusState::DraftTask(0);

        // Initially, card internal focus should be on TaskDescription
        if let Some(card) = vm.draft_cards.first() {
            assert_eq!(card.focus_element, CardFocusElement::TaskDescription);
        }

        // Enter with TaskDescription focused should launch task (same as Go button)
        // This will fail validation since description is empty, but that's expected
        assert!(!vm.handle_enter(false)); // Should return false due to validation failure
        assert!(vm.status_bar.error_message.is_some());

        // Now test with repository selector focused internally
        if let Some(card) = vm.draft_cards.get_mut(0) {
            card.focus_element = CardFocusElement::RepositorySelector;
        }
        vm.modal_state = ModalState::None; // Reset modal state
        vm.status_bar.error_message = None; // Reset error

        assert!(vm.handle_enter(false));
        assert_eq!(vm.modal_state, ModalState::RepositorySearch);

        // Test branch selector
        if let Some(card) = vm.draft_cards.get_mut(0) {
            card.focus_element = CardFocusElement::BranchSelector;
        }
        vm.modal_state = ModalState::None;

        assert!(vm.handle_enter(false));
        assert_eq!(vm.modal_state, ModalState::BranchSearch);

        // Test model selector
        if let Some(card) = vm.draft_cards.get_mut(0) {
            card.focus_element = CardFocusElement::ModelSelector;
        }
        vm.modal_state = ModalState::None;

        assert!(vm.handle_enter(false));
        assert_eq!(vm.modal_state, ModalState::ModelSearch);

        // Test go button
        if let Some(card) = vm.draft_cards.get_mut(0) {
            card.focus_element = CardFocusElement::GoButton;
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
        vm.focus_element = DashboardFocusState::DraftTask(0);

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
            vm.handle_key_event(key_event(KeyCode::Tab, KeyModifiers::empty())),
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
        vm.focus_element = DashboardFocusState::DraftTask(0);

        // Should fail validation
        assert!(!vm.launch_task(0, ah_core::SplitMode::None, false, None, None, None)); // Should return false for validation failure
        assert!(vm.status_bar.error_message.is_some());
        assert!(vm.status_bar.error_message.as_ref().unwrap().contains("required"));
    }

    #[test]
    fn viewmodel_task_launch_validation_requires_models() {
        // Test PRD requirement: Task launch validation
        let mut vm = create_test_view_model();

        // Add description and clear models
        vm.focus_element = DashboardFocusState::DraftTask(0);
        vm.handle_char_input('T');
        vm.handle_char_input('e');
        vm.handle_char_input('s');
        vm.handle_char_input('t');

        // Clear the models to make validation fail
        if let Some(card) = vm.draft_cards.get_mut(0) {
            card.selected_agents.clear();
        }

        vm.focus_element = DashboardFocusState::DraftTask(0);

        // Should fail validation
        assert!(!vm.launch_task(0, ah_core::SplitMode::None, false, None, None, None)); // Should return false for validation failure
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
        let result = vm.handle_key_event(ctrl_n_event);
        assert!(result);

        // Should have created a new draft task
        assert_eq!(vm.draft_cards.len(), initial_draft_count + 1);
        // Global focus should be on the new draft task
        assert_eq!(
            vm.focus_element,
            DashboardFocusState::DraftTask(initial_draft_count)
        );
        // The new card's internal focus should be on TaskDescription
        if let Some(card) = vm.draft_cards.get(initial_draft_count) {
            assert_eq!(card.focus_element, CardFocusElement::TaskDescription);
        }
    }

    #[test]
    fn viewmodel_auto_save_timer_gets_set_on_text_input() {
        // Test PRD requirement: Auto-save functionality
        let mut vm = create_test_view_model();

        // Start with draft task focused (global focus)
        vm.focus_element = DashboardFocusState::DraftTask(0);

        // Type some text (should work since card internal focus is on TaskDescription)
        vm.handle_char_input('T');
        vm.handle_char_input('e');
        vm.handle_char_input('x');
        vm.handle_char_input('t');

        // Auto-save timer should be set on the draft card
        if let Some(card) = vm.draft_cards.first() {
            assert!(card.auto_save_timer.is_some());
            assert_eq!(card.save_state, DraftSaveState::Unsaved);
            assert_eq!(card.dirty_generation, 4);
            assert_eq!(card.last_saved_generation, 0);
            assert!(card.pending_save_request_id.is_none());
        } else {
            panic!("Expected draft card");
        }
    }

    #[test]
    fn viewmodel_auto_save_timer_expires_after_delay() {
        // Test PRD requirement: Auto-save timer expiration
        let (mut vm, ui_rx) = create_test_view_model_with_channel();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        let _guard = runtime.enter();

        // Start with draft task focused and type something to set the timer
        vm.focus_element = DashboardFocusState::DraftTask(0);
        vm.handle_char_input('T');

        // Get the first draft card and manually set its timer to expired state
        if let Some(card) = vm.draft_cards.get_mut(0) {
            card.auto_save_timer =
                Some(std::time::Instant::now() - std::time::Duration::from_millis(600));
        }

        // Handle tick should mark as saved
        assert!(vm.handle_tick());

        // Allow the spawned auto-save task to run and deliver its completion
        runtime.block_on(async { tokio::task::yield_now().await });
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        loop {
            runtime.block_on(async { tokio::task::yield_now().await });
            if let Ok(msg) = ui_rx.try_recv() {
                vm.update(msg).expect("apply auto-save completion");
                break;
            }
            if std::time::Instant::now() >= deadline {
                panic!("Expected auto-save completion message");
            }
        }

        // Check the first draft card
        if let Some(card) = vm.draft_cards.first() {
            assert_eq!(card.save_state, DraftSaveState::Saved);
            assert!(card.auto_save_timer.is_none());
            assert_eq!(card.dirty_generation, card.last_saved_generation);
            assert!(card.pending_save_request_id.is_none());
        }
    }

    #[test]
    fn auto_save_completion_updates_saved_state_for_matching_generation() {
        let (mut vm, _rx) = create_test_view_model_with_channel();
        vm.focus_element = DashboardFocusState::DraftTask(0);

        if let Some(card) = vm.draft_cards.get_mut(0) {
            card.dirty_generation = 3;
            card.last_saved_generation = 1;
            card.pending_save_request_id = Some(7);
            card.save_state = DraftSaveState::Saving;
            card.pending_save_invalidated = true;
        }

        let msg = Msg::DraftSaveCompleted {
            draft_id: vm.draft_cards[0].id.clone(),
            request_id: 7,
            generation: 3,
            result: SaveDraftResult::Success,
        };
        vm.needs_redraw = false;
        vm.update(msg).expect("update succeeds");

        let card = &vm.draft_cards[0];
        assert_eq!(card.save_state, DraftSaveState::Saved);
        assert_eq!(card.last_saved_generation, 3);
        assert!(card.pending_save_request_id.is_none());
        assert!(!card.pending_save_invalidated);
        assert!(vm.needs_redraw);
    }

    #[test]
    fn auto_save_completion_is_ignored_when_request_id_mismatch() {
        let (mut vm, _rx) = create_test_view_model_with_channel();

        if let Some(card) = vm.draft_cards.get_mut(0) {
            card.dirty_generation = 4;
            card.last_saved_generation = 2;
            card.pending_save_request_id = Some(9);
            card.save_state = DraftSaveState::Saving;
        }

        let msg = Msg::DraftSaveCompleted {
            draft_id: vm.draft_cards[0].id.clone(),
            request_id: 8,
            generation: 4,
            result: SaveDraftResult::Success,
        };
        vm.needs_redraw = false;
        vm.update(msg).expect("update succeeds");

        let card = &vm.draft_cards[0];
        assert_eq!(card.save_state, DraftSaveState::Saving);
        assert_eq!(card.pending_save_request_id, Some(9));
        assert_eq!(card.last_saved_generation, 2);
        // No redraw should be scheduled when we ignore the completion
        assert!(!vm.needs_redraw);
    }

    #[test]
    fn auto_save_completion_sets_error_state_on_failure() {
        let (mut vm, _rx) = create_test_view_model_with_channel();
        vm.focus_element = DashboardFocusState::DraftTask(0);

        if let Some(card) = vm.draft_cards.get_mut(0) {
            card.dirty_generation = 5;
            card.last_saved_generation = 3;
            card.pending_save_request_id = Some(11);
            card.save_state = DraftSaveState::Saving;
        }

        let msg = Msg::DraftSaveCompleted {
            draft_id: vm.draft_cards[0].id.clone(),
            request_id: 11,
            generation: 5,
            result: SaveDraftResult::Failure {
                error: "network timeout".to_string(),
            },
        };
        vm.needs_redraw = false;
        vm.update(msg).expect("update succeeds");

        let card = &vm.draft_cards[0];
        assert_eq!(card.save_state, DraftSaveState::Error);
        assert!(card.pending_save_request_id.is_none());
        assert!(
            vm.status_bar
                .error_message
                .as_ref()
                .map(|msg| msg.contains("network timeout"))
                .unwrap_or(false)
        );
        assert!(vm.needs_redraw);
    }

    #[test]
    fn auto_save_completion_leaves_unsaved_when_generation_has_advanced() {
        let (mut vm, _rx) = create_test_view_model_with_channel();

        if let Some(card) = vm.draft_cards.get_mut(0) {
            card.dirty_generation = 6;
            card.last_saved_generation = 3;
            card.pending_save_request_id = Some(12);
            card.save_state = DraftSaveState::Saving;
        }

        let msg = Msg::DraftSaveCompleted {
            draft_id: vm.draft_cards[0].id.clone(),
            request_id: 12,
            generation: 4,
            result: SaveDraftResult::Success,
        };
        vm.needs_redraw = false;
        vm.update(msg).expect("update succeeds");

        let card = &vm.draft_cards[0];
        assert_eq!(card.save_state, DraftSaveState::Unsaved);
        assert!(card.pending_save_request_id.is_none());
        assert_eq!(card.last_saved_generation, 4);
        assert!(vm.needs_redraw);
    }

    #[test]
    fn viewmodel_enter_selects_from_modal_and_returns_focus() {
        // Test PRD requirement: Enter key selects entry from modal and returns focus to task entry
        let mut vm = create_test_view_model();

        // Start with focus on draft task (initial state)
        vm.focus_element = DashboardFocusState::DraftTask(0);
        vm.draft_cards[0].focus_element = CardFocusElement::ModelSelector;

        // Manually populate available models for test (since background loading is async)
        let catalog = ah_core::agent_catalog::RemoteAgentCatalog::default_catalog();
        vm.available_models =
            catalog.agents.into_iter().map(|metadata| metadata.to_agent_choice()).collect();

        // Open model selection modal
        assert!(vm.handle_enter(false));
        assert_eq!(vm.modal_state, ModalState::ModelSearch);
        assert!(vm.active_modal.is_some());

        // Verify initial modal state
        if let Some(modal) = &vm.active_modal {
            // Should have model options available
            assert!(!modal.filtered_options.is_empty());
            // First option should be selected by default
            assert_eq!(modal.selected_index, 0);
            let first_option_selected = modal
                .filtered_options
                .iter()
                .any(|opt| matches!(opt, FilteredOption::Option { selected: true, .. }));
            assert!(first_option_selected);
        }

        // Initially, draft card buttons should not be pressed (no focus on them)
        if let Some(card) = vm.draft_cards.first() {
            assert_eq!(card.focus_element, CardFocusElement::ModelSelector);
            // Buttons should not be focused/pressed
            assert_ne!(card.focus_element, CardFocusElement::GoButton);
        }

        // Press Enter to select from modal
        let enter_key = key_event(KeyCode::Enter, KeyModifiers::empty());
        let handled = vm.handle_key_event(enter_key);

        // The modal should be closed and selection should be applied
        assert!(handled);
        assert_eq!(vm.modal_state, ModalState::None);
        assert!(vm.active_modal.is_none());

        // Focus should return to the model selector button
        assert_eq!(vm.focus_element, DashboardFocusState::DraftTask(0));
        if let Some(card) = vm.draft_cards.first() {
            assert_eq!(card.focus_element, CardFocusElement::ModelSelector);
        }

        // Verify that the selected model was applied to the card
        if let Some(card) = vm.draft_cards.first() {
            // Should have at least one model selected
            assert!(!card.selected_agents.is_empty());
            // The first model should be the one that was selected
            if let Some(selected_model) = card.selected_agents.first() {
                // Should be one of the available models
                let model_exists = vm
                    .available_models
                    .iter()
                    .any(|model| model.display_name() == selected_model.display_name());
                assert!(model_exists);
            }
        }
    }

    #[test]
    fn test_modal_input_height() {
        let mut vm = create_test_view_model();

        // Open model selection modal
        vm.open_modal(ModalState::ModelSearch);
        assert_eq!(vm.modal_state, ModalState::ModelSearch);
        assert!(vm.active_modal.is_some());

        // Check that the modal is created with AgentSelection type
        // This is tested implicitly through the rendering, but we can verify the modal type
        if let Some(modal) = &vm.active_modal {
            match &modal.modal_type {
                ModalType::AgentSelection { .. } => {} // Correct type
                _ => panic!("Expected AgentSelection modal type"),
            }
        }
    }

    #[test]
    fn test_modal_text_editing_operations() {
        let mut vm = create_test_view_model();

        // Open repository search modal (which uses Search modal type)
        vm.open_modal(ModalState::RepositorySearch);
        vm.handle_char_input('t');
        vm.handle_char_input('e');
        vm.handle_char_input('s');
        vm.handle_char_input('t');

        // Verify input was added
        assert_eq!(vm.active_modal.as_ref().unwrap().input_value, "test");

        // Test delete character backward
        let backspace_event = key_event(KeyCode::Backspace, KeyModifiers::empty());
        let _ = vm.handle_keyboard_operation(
            KeyboardOperation::DeleteCharacterBackward,
            &backspace_event,
        );

        // Verify character was deleted
        assert_eq!(vm.active_modal.as_ref().unwrap().input_value, "tes");

        // Test delete to end of line
        let ctrl_k_event = key_event(KeyCode::Char('k'), KeyModifiers::CONTROL); // Ctrl+K
        let _ = vm.handle_keyboard_operation(KeyboardOperation::DeleteToEndOfLine, &ctrl_k_event);

        // Verify line was cleared
        assert_eq!(vm.active_modal.as_ref().unwrap().input_value, "");
    }

    #[test]
    fn test_draft_button_navigation_focus() {
        let mut vm = create_test_view_model();

        // Create a draft task
        vm.create_draft_task(
            "Test task".to_string(),
            "test-repo".to_string(),
            "main".to_string(),
            vec![],
            CardFocusElement::TaskDescription,
        );

        // Focus on repository selector
        vm.focus_element = DashboardFocusState::DraftTask(0);
        if let Some(card) = vm.draft_cards.get_mut(0) {
            card.focus_element = CardFocusElement::RepositorySelector;
        }

        // Verify the focus was set correctly
        assert_eq!(vm.focus_element, DashboardFocusState::DraftTask(0));
        assert_eq!(
            vm.draft_cards[0].focus_element,
            CardFocusElement::RepositorySelector
        );
    }

    #[test]
    fn test_model_selection_modal_creation() {
        let mut vm = create_test_view_model();

        // Create a draft task with a model
        vm.create_draft_task(
            "Test task".to_string(),
            "test-repo".to_string(),
            "main".to_string(),
            vec![AgentChoice {
                agent: AgentSoftwareBuild {
                    software: AgentSoftware::Claude,
                    version: "latest".to_string(),
                },
                model: "sonnet".to_string(),
                count: 1,
                settings: std::collections::HashMap::new(),
                display_name: Some("Claude Latest".to_string()),
            }],
            CardFocusElement::TaskDescription,
        );

        // Manually populate available models for test (since background loading is async)
        let catalog = ah_core::agent_catalog::RemoteAgentCatalog::default_catalog();
        vm.available_models =
            catalog.agents.into_iter().map(|metadata| metadata.to_agent_choice()).collect();

        // Open model selection modal
        vm.open_modal(ModalState::ModelSearch);

        // Verify modal was created with the correct type
        assert!(vm.active_modal.is_some());
        if let Some(modal) = &vm.active_modal {
            match &modal.modal_type {
                ModalType::AgentSelection { options } => {
                    // Should have all available models (6 default models)
                    assert_eq!(options.len(), 6);
                    // Find the "Claude Code" that was selected
                    let test_model = options.iter().find(|opt| opt.name == "Claude Code").unwrap();
                    assert_eq!(test_model.count, 1);
                    assert!(test_model.is_selected);
                }
                _ => panic!("Expected AgentSelection modal type"),
            }
        }
    }
}
