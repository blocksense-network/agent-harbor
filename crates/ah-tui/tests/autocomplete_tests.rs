// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Autocomplete-specific ViewModel tests ensuring keyboard navigation, ghost text,
//! and input interactions.

mod common;

use ah_tui::settings::{KeyboardOperation, Settings};
use ah_tui::view_model::autocomplete::MenuContext;
use ah_tui::view_model::task_entry::CardFocusElement;
use ah_tui::view_model::{DashboardFocusState, MouseAction, ViewModel};
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
async fn autocomplete_navigation_wraps_for_keyboard_operations() {
    let mut vm = common::build_view_model();
    prepare_autocomplete(&mut vm, '/', "test").await;

    let state = vm.autocomplete.menu_state().expect("menu should be open after preparation");
    assert_eq!(
        state.selected_index, 0,
        "initial selection should be first item"
    );

    let dummy_key = KeyEvent::new(KeyCode::Down, KeyModifiers::empty());
    assert!(
        vm.handle_keyboard_operation(KeyboardOperation::MoveToNextLine, &dummy_key),
        "MoveToNextLine should be handled by autocomplete"
    );
    let state = vm.autocomplete.menu_state().expect("menu should remain open after moving next");
    assert_eq!(state.selected_index, 1, "selection should advance");
    assert!(
        state.results.len() >= 3,
        "expected at least three autocomplete entries"
    );

    assert!(
        vm.handle_keyboard_operation(KeyboardOperation::MoveToNextField, &dummy_key),
        "MoveToNextField should also advance selection"
    );
    let state = vm.autocomplete.menu_state().expect("menu remains open after MoveToNextField");
    assert_eq!(
        state.selected_index, 2,
        "selection should move to third item"
    );

    // Next move should wrap to first entry
    assert!(
        vm.handle_keyboard_operation(KeyboardOperation::MoveToNextLine, &dummy_key),
        "MoveToNextLine should wrap selection"
    );
    let state = vm.autocomplete.menu_state().expect("menu remains open after wrapping");
    assert_eq!(
        state.selected_index, 0,
        "selection should wrap to first item"
    );

    // Previous movement should also wrap backwards
    assert!(
        vm.handle_keyboard_operation(KeyboardOperation::MoveToPreviousField, &dummy_key),
        "MoveToPreviousField should move selection backwards"
    );
    let state = vm.autocomplete.menu_state().expect("menu remains open after previous movement");
    assert_eq!(
        state.selected_index, 2,
        "selection should wrap to last item"
    );
}

#[tokio::test]
async fn caret_movement_closes_autocomplete_menu() {
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
        "autocomplete should be closed after moving caret to start"
    );
}

#[tokio::test]
async fn mouse_selection_commits_autocomplete_entry() {
    let mut vm = common::build_view_model();
    prepare_autocomplete(&mut vm, '/', "test").await;

    // Ensure menu is open and second item exists
    let state = vm.autocomplete.menu_state().expect("menu should be open");
    assert!(state.results.len() >= 2);

    vm.perform_mouse_action(MouseAction::AutocompleteSelect(1));

    assert!(
        !vm.autocomplete.is_open(),
        "autocomplete should close after mouse selection is committed"
    );
    let card = vm.draft_cards.first().expect("draft card exists");
    let text = card.description.lines().join("\n");
    assert!(
        text.starts_with("/test-"),
        "selection should insert the chosen item, found: {text}"
    );
}

#[test]
fn workspace_terms_menu_tracks_selected_item() {
    let mut vm = common::build_view_model_with_terms(vec![
        "helloWorld".to_string(),
        "helloWarehouse".to_string(),
    ]);
    vm.focus_element = DashboardFocusState::DraftTask(0);
    vm.draft_cards[0].focus_element = CardFocusElement::TaskDescription;

    for ch in ['h', 'e', 'l'] {
        vm.handle_char_input(ch);
    }

    let menu_state = vm.autocomplete.menu_state().expect("workspace terms menu should open");
    assert_eq!(menu_state.context, MenuContext::WorkspaceTerms);
    assert!(
        menu_state.results.len() >= 2,
        "terms menu should expose all available completions"
    );

    let ghost = vm
        .autocomplete
        .ghost_state()
        .expect("ghost text should reflect current selection");
    assert_eq!(ghost.completion_extension(), "loWorld");

    vm.autocomplete.select_next();
    let ghost = vm
        .autocomplete
        .ghost_state()
        .expect("ghost text should refresh after selection changes");
    assert_eq!(ghost.completion_extension(), "loWarehouse");
}

#[test]
fn workspace_terms_menu_can_be_disabled() {
    let settings = Settings {
        workspace_terms_menu: Some(false),
        ..Default::default()
    };
    let mut vm =
        common::build_view_model_with_terms_and_settings(vec!["helloWorld".to_string()], settings);
    vm.focus_element = DashboardFocusState::DraftTask(0);
    vm.draft_cards[0].focus_element = CardFocusElement::TaskDescription;

    for ch in ['h', 'e', 'l'] {
        vm.handle_char_input(ch);
    }

    assert!(
        vm.autocomplete.menu_state().is_none(),
        "terms menu should remain hidden when the preference is disabled"
    );
    let ghost = vm.autocomplete.ghost_state().expect("ghost text should still be available");
    assert_eq!(ghost.completion_extension(), "loWorld");
}
