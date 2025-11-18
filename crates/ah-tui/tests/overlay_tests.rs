// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Modal navigation and ESC dismissal behaviour tests for the ViewModel

mod common;

use ah_tui::settings::KeyboardOperation;
use ah_tui::view_model::task_entry::CardFocusElement;
use ah_tui::view_model::{DashboardFocusState, ModalState};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::io::Write;

#[test]
fn modal_navigation_wraps_with_keyboard_operations() {
    let (mut log, log_path) = common::create_test_log("modal_navigation");
    let log_hint = log_path.display().to_string();

    let mut vm = common::build_view_model_with_repos();
    vm.open_modal(ModalState::RepositorySearch);

    let next_key = KeyEvent::new(KeyCode::Down, KeyModifiers::empty());

    let total_options = vm.active_modal.as_ref().expect("modal available").filtered_options.len();

    for step in 0..(total_options.saturating_sub(1)) {
        assert!(
            vm.handle_keyboard_operation(KeyboardOperation::MoveToNextLine, &next_key),
            "MoveToNextLine should be handled inside modal (log: {log_hint})"
        );
        let modal = vm.active_modal.as_ref().expect("modal still open");
        writeln!(log, "Step {} -> selected {}", step, modal.selected_index).expect("write log");
    }

    let modal = vm.active_modal.as_ref().expect("modal still open");
    assert_eq!(
        modal.selected_index,
        total_options.saturating_sub(1),
        "selection should reach last option before wrapping (log: {log_hint})"
    );

    assert!(
        vm.handle_keyboard_operation(KeyboardOperation::MoveToNextLine, &next_key),
        "additional MoveToNextLine should wrap (log: {log_hint})"
    );
    let modal = vm.active_modal.as_ref().expect("modal still open");
    assert_eq!(
        modal.selected_index, 0,
        "selection should wrap to start (log: {log_hint})"
    );

    assert!(
        vm.handle_keyboard_operation(KeyboardOperation::MoveToPreviousField, &next_key),
        "MoveToPreviousField should wrap backwards (log: {log_hint})"
    );
    let modal = vm.active_modal.as_ref().expect("modal still open");
    let len = modal.filtered_options.len();
    assert_eq!(
        modal.selected_index,
        len - 1,
        "previous field wraps to last option (log: {log_hint})"
    );
}

#[tokio::test]
async fn dismiss_overlay_behaviour_follows_priority_and_exit_rules() {
    let (mut log, log_path) = common::create_test_log("dismiss_overlay");
    let log_hint = log_path.display().to_string();

    let mut vm = common::build_view_model_with_repos();
    vm.focus_element = DashboardFocusState::DraftTask(0);
    vm.open_modal(ModalState::Settings);

    let esc_key = KeyEvent::new(KeyCode::Esc, KeyModifiers::empty());
    assert!(
        vm.handle_key_event(esc_key),
        "ESC should dismiss settings modal (log: {log_hint})"
    );
    writeln!(
        log,
        "Modal after dismiss: {:?}, exit armed: {}",
        vm.modal_state, vm.exit_confirmation_armed
    )
    .expect("write log");

    assert_eq!(
        vm.modal_state,
        ModalState::None,
        "modal should close (log: {log_hint})"
    );
    assert!(
        !vm.exit_confirmation_armed,
        "closing modal should not arm exit (log: {log_hint})"
    );

    // Set up autocomplete
    vm.draft_cards[0].focus_element = CardFocusElement::TaskDescription;
    let card = vm.draft_cards.get_mut(0).expect("draft card");
    card.description = tui_textarea::TextArea::default();

    // For testing, populate the cache synchronously
    {
        let mut cache_state = vm.autocomplete.cache_state.lock().unwrap();
        if cache_state.workflows.is_none() {
            let workflows = vec!["test-workflow".to_string()];
            cache_state.workflows = Some(workflows);
        }
    }

    // Input trigger character and some text
    let key_event = KeyEvent::new(KeyCode::Char('/'), KeyModifiers::empty());
    vm.autocomplete.notify_text_input();
    card.description.input(key_event);
    vm.autocomplete.after_textarea_change(&card.description, &mut vm.needs_redraw);

    let key_event = KeyEvent::new(KeyCode::Char('t'), KeyModifiers::empty());
    vm.autocomplete.notify_text_input();
    card.description.input(key_event);
    vm.autocomplete.after_textarea_change(&card.description, &mut vm.needs_redraw);

    assert!(
        vm.autocomplete.is_open(),
        "menu should be open before ESC (log: {log_hint})"
    );
    assert!(
        vm.handle_key_event(esc_key),
        "ESC should close autocomplete first (log: {log_hint})"
    );
    assert!(
        !vm.autocomplete.is_open(),
        "autocomplete should close on ESC (log: {log_hint})"
    );
    assert!(
        !vm.exit_confirmation_armed,
        "closing autocomplete should not arm exit (log: {log_hint})"
    );

    // First ESC should arm exit state
    assert!(
        vm.handle_key_event(esc_key),
        "ESC should arm exit when nothing else is open (log: {log_hint})"
    );
    assert!(
        vm.exit_confirmation_armed,
        "exit should be armed after first ESC (log: {log_hint})"
    );
    assert!(
        !vm.exit_requested,
        "no exit yet after first ESC (log: {log_hint})"
    );

    // Any other key should discharge the confirmation
    assert!(
        vm.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::empty())),
        "Tab should still be handled (log: {log_hint})"
    );
    assert!(
        !vm.exit_confirmation_armed,
        "non-ESC key should clear exit confirmation (log: {log_hint})"
    );

    // Arm again and confirm second ESC requests exit
    vm.handle_key_event(esc_key);
    assert!(
        vm.exit_confirmation_armed,
        "exit re-armed (log: {log_hint})"
    );
    vm.handle_key_event(esc_key);
    writeln!(
        log,
        "After second ESC -> armed {} requested {}",
        vm.exit_confirmation_armed, vm.exit_requested
    )
    .expect("write log");
    assert!(
        vm.exit_requested,
        "second ESC should request exit (log: {log_hint})"
    );
    assert!(
        vm.take_exit_request(),
        "take_exit_request returns true (log: {log_hint})"
    );
    assert!(
        !vm.exit_requested,
        "exit flag cleared after take_exit_request (log: {log_hint})"
    );
}
