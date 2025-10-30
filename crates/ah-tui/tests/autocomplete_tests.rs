// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Autocomplete-specific ViewModel tests ensuring keyboard navigation and caret interactions

mod common;

use std::io::Write;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ah_tui::settings::KeyboardOperation;
use ah_tui::view_model::autocomplete::{InlineAutocomplete, Item, Trigger};
use ah_tui::view_model::task_entry::CardFocusElement;
use ah_tui::view_model::{DashboardFocusState, ViewModel};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

async fn prepare_autocomplete(vm: &mut ViewModel, trigger_char: char, query: &str) {
    vm.focus_element = DashboardFocusState::DraftTask(0);
    vm.draft_cards[0].focus_element = CardFocusElement::TaskDescription;

    let card = vm.draft_cards.get_mut(0).expect("draft card");
    card.description = tui_textarea::TextArea::default();

    // For testing, populate the cache synchronously
    {
        let mut cache_state = vm.autocomplete.cache_state.lock().unwrap();
        if trigger_char == '/' {
            let workflows = vec![
                "test-workflow".to_string(),
                "test-another".to_string(),
                "test-command".to_string(),
            ];
            cache_state.workflows = Some(workflows);
        } else if trigger_char == '@' {
            let files = vec!["src/main.rs".to_string(), "Cargo.toml".to_string()];
            cache_state.files = Some(files);
        }
        cache_state.refresh_in_progress = false;
        cache_state.last_update = Some(std::time::Instant::now());
    }

    // Input the trigger character and query
    let text = format!("{}{}", trigger_char, query);
    for ch in text.chars() {
        card.description.insert_char(ch);
        vm.autocomplete.after_textarea_change(&card.description, &mut vm.needs_redraw);
    }

    // Wait for autocomplete to populate (in real usage this would be async)
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
}

#[tokio::test]
#[ignore = "autocomplete API changed, tests need adaptation"]
async fn autocomplete_navigation_wraps_for_keyboard_operations() {
    let (mut log, log_path) = common::create_test_log("autocomplete_wrap");
    let log_hint = log_path.display().to_string();

    let mut vm = common::build_view_model();
    prepare_autocomplete(&mut vm, '/', "test").await;

    let state = vm.autocomplete.menu_state().expect("menu should be open after preparation");
    assert_eq!(
        state.selected_index, 0,
        "initial selection should be first item (log: {log_hint})"
    );

    let dummy_key = KeyEvent::new(KeyCode::Down, KeyModifiers::empty());
    assert!(
        vm.handle_keyboard_operation(KeyboardOperation::MoveToNextLine, &dummy_key),
        "MoveToNextLine should be handled by autocomplete (log: {log_hint})"
    );
    let state = vm.autocomplete.menu_state().expect("menu should remain open after moving next");
    assert_eq!(
        state.selected_index, 1,
        "selection should advance (log: {log_hint})"
    );

    assert!(
        vm.handle_keyboard_operation(KeyboardOperation::MoveToNextField, &dummy_key),
        "MoveToNextField should also advance selection (log: {log_hint})"
    );
    let state = vm.autocomplete.menu_state().expect("menu remains open after MoveToNextField");
    assert_eq!(
        state.selected_index, 2,
        "selection should move to third item (log: {log_hint})"
    );

    // Next move should wrap to first entry
    assert!(
        vm.handle_keyboard_operation(KeyboardOperation::MoveToNextLine, &dummy_key),
        "MoveToNextLine should wrap selection (log: {log_hint})"
    );
    let state = vm.autocomplete.menu_state().expect("menu remains open after wrapping");
    assert_eq!(
        state.selected_index, 0,
        "selection should wrap to first item (log: {log_hint})"
    );

    // Previous movement should also wrap backwards
    assert!(
        vm.handle_keyboard_operation(KeyboardOperation::MoveToPreviousField, &dummy_key),
        "MoveToPreviousField should move selection backwards (log: {log_hint})"
    );
    let state = vm.autocomplete.menu_state().expect("menu remains open after previous movement");
    assert_eq!(
        state.selected_index, 2,
        "selection should wrap to last item (log: {log_hint})"
    );
}

#[tokio::test]
#[ignore = "autocomplete API changed, tests need adaptation"]
async fn caret_movement_closes_autocomplete_menu() {
    let (mut log, log_path) = common::create_test_log("autocomplete_caret");
    let log_hint = log_path.display().to_string();

    let mut vm = common::build_view_model();
    prepare_autocomplete(&mut vm, '/', "test").await;

    // Verify autocomplete is open with some query
    assert!(vm.autocomplete.is_open());
    let initial_query = vm.autocomplete.get_query().to_string();
    assert_eq!(initial_query, "test"); // Should be "test" since we typed "/test"

    // Simulate moving caret to beginning of line (should close autocomplete)
    let home_key = KeyEvent::new(KeyCode::Home, KeyModifiers::empty());
    vm.handle_keyboard_operation(KeyboardOperation::MoveToBeginningOfLine, &home_key);

    // Autocomplete should be closed
    assert!(
        !vm.autocomplete.is_open(),
        "autocomplete should be closed after moving caret to start (log: {log_hint})"
    );
}
