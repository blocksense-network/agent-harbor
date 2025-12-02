// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Tests for Launch Options modal interaction handling (keyboard and mouse)
//!
//! This comprehensive test suite verifies the Launch Options modal's interaction behavior:
//!
//! ## Keyboard Interactions
//! - **Enter key**: Saves changes and closes the modal, restoring focus to TaskDescription
//! - **Space key**: Toggles boolean options and opens enum selection popups
//! - **Esc key**: Discards changes and restores the original configuration before closing
//!
//! ## Mouse Interactions
//! - **Click action buttons**: Launch with selected options
//! - **Click "Esc Cancel"**: Same behavior as pressing Esc key
//!
//! ## Test Coverage
//! 1. Basic save functionality (Enter key and mouse click)
//! 2. Basic cancel functionality (Esc key and mouse click)
//! 3. Config restoration when no prior config exists
//! 4. Persistence of saved changes across modal reopenings
//! 5. State integrity with multiple ESC presses
//! 6. Focus restoration after applying changes
//! 7. Interchangeability of keyboard and mouse inputs
//! 8. Space key toggling of boolean options
//! 9. Enter key behavior in enum popups
//!
//! All tests follow the project's testing guidelines:
//! - Each test creates a unique log file for full output capture
//! - Minimal console output on success to preserve AI context windows
//! - Log path printed on completion for debugging when needed

mod common;

use ah_tui::view_model::agents_selector_model::{
    AdvancedLaunchOptions, LaunchOptionsColumn, MouseAction,
};
use ah_tui::view_model::task_entry::CardFocusElement;
use ah_tui::view_model::{DashboardFocusState, ModalState};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::io::Write;

#[test]
#[allow(clippy::disallowed_methods)]
fn test_esc_key_discards_changes_and_restores_original() {
    let (mut log, log_path) = common::create_test_log("launch_options_esc_discards");
    let log_hint = log_path.display().to_string();

    let mut vm = common::build_view_model_with_repos();

    // Setup: Set initial config values
    let original_config = AdvancedLaunchOptions {
        allow_egress: true,
        allow_containers: false,
        timeout: "600s".to_string(),
        ..Default::default()
    };

    vm.focus_element = DashboardFocusState::DraftTask(0);
    if let Some(card) = vm.draft_cards.get_mut(0) {
        card.focus_element = CardFocusElement::AdvancedOptionsButton;
        card.advanced_options = Some(original_config.clone());
    }

    writeln!(
        log,
        "Original config: allow_egress={}, allow_containers={}, timeout={}",
        original_config.allow_egress, original_config.allow_containers, original_config.timeout
    )
    .expect("write log");

    // Open launch options modal
    let draft_id = vm.draft_cards[0].id.clone();
    vm.open_launch_options_modal(draft_id);
    assert_eq!(vm.modal_state, ModalState::LaunchOptions);

    writeln!(log, "Opened launch options modal").expect("write log");

    // Modify config values (these should be discarded)
    if let Some(modal) = &mut vm.active_modal {
        if let ah_tui::view_model::ModalType::LaunchOptions { view_model } = &mut modal.modal_type {
            view_model.config.allow_egress = false;
            view_model.config.allow_containers = true;
            view_model.config.timeout = "999s".to_string();

            writeln!(
                log,
                "Modified config (to be discarded): allow_egress={}, allow_containers={}, timeout={}",
                view_model.config.allow_egress, view_model.config.allow_containers, view_model.config.timeout
            )
            .expect("write log");
        }
    }

    // Press ESC to discard changes
    let esc_key = KeyEvent::new(KeyCode::Esc, KeyModifiers::empty());
    let handled = vm.handle_key_event(esc_key);

    writeln!(
        log,
        "ESC handled: {}, modal_state: {:?}",
        handled, vm.modal_state
    )
    .expect("write log");

    assert!(handled, "ESC should be handled (log: {log_hint})");
    assert_eq!(
        vm.modal_state,
        ModalState::None,
        "Modal should be closed (log: {log_hint})"
    );

    // Verify original config was restored
    let restored_config = vm.draft_cards[0]
        .advanced_options
        .as_ref()
        .expect("Advanced options should be set");

    assert!(
        restored_config.allow_egress,
        "allow_egress should be restored to original value (log: {log_hint})"
    );
    assert!(
        !restored_config.allow_containers,
        "allow_containers should be restored to original value (log: {log_hint})"
    );
    assert_eq!(
        restored_config.timeout, "600s",
        "timeout should be restored to original value (log: {log_hint})"
    );

    writeln!(
        log,
        "Verified restored config: allow_egress={}, allow_containers={}, timeout={}",
        restored_config.allow_egress, restored_config.allow_containers, restored_config.timeout
    )
    .expect("write log");

    // Success message
    println!("✓ Test passed. See log for details (if needed): {log_hint}");
}

#[test]
#[allow(clippy::disallowed_methods)]
fn test_esc_key_with_no_prior_config() {
    let (mut log, log_path) = common::create_test_log("launch_options_esc_no_prior_config");
    let log_hint = log_path.display().to_string();

    let mut vm = common::build_view_model_with_repos();

    // Setup: No initial config (advanced_options = None)
    vm.focus_element = DashboardFocusState::DraftTask(0);
    if let Some(card) = vm.draft_cards.get_mut(0) {
        card.focus_element = CardFocusElement::AdvancedOptionsButton;
        card.advanced_options = None; // Explicitly set to None
    }

    writeln!(log, "Initial config: None").expect("write log");

    // Open launch options modal
    let draft_id = vm.draft_cards[0].id.clone();
    vm.open_launch_options_modal(draft_id);
    assert_eq!(vm.modal_state, ModalState::LaunchOptions);

    writeln!(log, "Opened launch options modal").expect("write log");

    // Modify config values
    if let Some(modal) = &mut vm.active_modal {
        if let ah_tui::view_model::ModalType::LaunchOptions { view_model } = &mut modal.modal_type {
            view_model.config.allow_egress = true;
            view_model.config.timeout = "300s".to_string();

            writeln!(
                log,
                "Modified config: allow_egress={}, timeout={}",
                view_model.config.allow_egress, view_model.config.timeout
            )
            .expect("write log");
        }
    }

    // Press ESC to discard changes
    let esc_key = KeyEvent::new(KeyCode::Esc, KeyModifiers::empty());
    let handled = vm.handle_key_event(esc_key);

    writeln!(
        log,
        "ESC handled: {}, modal_state: {:?}",
        handled, vm.modal_state
    )
    .expect("write log");

    assert!(handled, "ESC should be handled (log: {log_hint})");
    assert_eq!(
        vm.modal_state,
        ModalState::None,
        "Modal should be closed (log: {log_hint})"
    );

    // Verify original config (default) was restored
    let restored_config = vm.draft_cards[0]
        .advanced_options
        .as_ref()
        .expect("Advanced options should be set to original/default");

    // The original config should be the default values, not the modified ones
    assert_eq!(
        restored_config.allow_egress,
        AdvancedLaunchOptions::default().allow_egress,
        "allow_egress should be restored to default (log: {log_hint})"
    );
    assert_eq!(
        restored_config.timeout,
        AdvancedLaunchOptions::default().timeout,
        "timeout should be restored to default (log: {log_hint})"
    );

    writeln!(
        log,
        "Verified restored config matches default: allow_egress={}, timeout={}",
        restored_config.allow_egress, restored_config.timeout
    )
    .expect("write log");

    // Success message
    println!("✓ Test passed. See log for details (if needed): {log_hint}");
}

#[test]
#[allow(clippy::disallowed_methods)]
fn test_enter_key_followed_by_reopening_modal_shows_saved_changes() {
    let (mut log, log_path) = common::create_test_log("launch_options_enter_key_persists");
    let log_hint = log_path.display().to_string();

    let mut vm = common::build_view_model_with_repos();

    // Setup
    vm.focus_element = DashboardFocusState::DraftTask(0);
    if let Some(card) = vm.draft_cards.get_mut(0) {
        card.focus_element = CardFocusElement::AdvancedOptionsButton;
    }

    let draft_id = vm.draft_cards[0].id.clone();

    // First: Open modal and make changes
    vm.open_launch_options_modal(draft_id.clone());
    if let Some(modal) = &mut vm.active_modal {
        if let ah_tui::view_model::ModalType::LaunchOptions { view_model } = &mut modal.modal_type {
            view_model.config.allow_egress = true;
            view_model.config.timeout = "42s".to_string();
        }
    }

    // Press Enter to save
    let enter_key = KeyEvent::new(KeyCode::Enter, KeyModifiers::empty());
    vm.handle_key_event(enter_key);

    writeln!(log, "Saved changes with Enter key").expect("write log");

    // Reopen the modal
    vm.focus_element = DashboardFocusState::DraftTask(0);
    if let Some(card) = vm.draft_cards.get_mut(0) {
        card.focus_element = CardFocusElement::AdvancedOptionsButton;
    }
    vm.open_launch_options_modal(draft_id);

    writeln!(log, "Reopened modal").expect("write log");

    // Verify the modal shows the saved changes
    if let Some(modal) = &vm.active_modal {
        if let ah_tui::view_model::ModalType::LaunchOptions { view_model } = &modal.modal_type {
            assert!(
                view_model.config.allow_egress,
                "Modal should show previously saved value (log: {log_hint})"
            );
            assert_eq!(
                view_model.config.timeout, "42s",
                "Modal should show previously saved timeout value (log: {log_hint})"
            );

            writeln!(
                log,
                "Verified modal config shows saved values: allow_egress={}, timeout={}",
                view_model.config.allow_egress, view_model.config.timeout
            )
            .expect("write log");
        }
    }

    // Success message
    println!("✓ Test passed. See log for details (if needed): {log_hint}");
}

#[test]
#[allow(clippy::disallowed_methods)]
fn test_multiple_esc_presses_dont_corrupt_state() {
    let (mut log, log_path) = common::create_test_log("launch_options_multiple_esc");
    let log_hint = log_path.display().to_string();

    let mut vm = common::build_view_model_with_repos();

    // Setup initial config
    let original_config = AdvancedLaunchOptions {
        allow_egress: true,
        timeout: "100s".to_string(),
        ..Default::default()
    };

    vm.focus_element = DashboardFocusState::DraftTask(0);
    if let Some(card) = vm.draft_cards.get_mut(0) {
        card.focus_element = CardFocusElement::AdvancedOptionsButton;
        card.advanced_options = Some(original_config.clone());
    }

    let draft_id = vm.draft_cards[0].id.clone();

    // Open modal and modify
    vm.open_launch_options_modal(draft_id);
    if let Some(modal) = &mut vm.active_modal {
        if let ah_tui::view_model::ModalType::LaunchOptions { view_model } = &mut modal.modal_type {
            view_model.config.allow_egress = false;
            view_model.config.timeout = "999s".to_string();
        }
    }

    writeln!(log, "Modified config to different values").expect("write log");

    // Press ESC once
    let esc_key = KeyEvent::new(KeyCode::Esc, KeyModifiers::empty());
    vm.handle_key_event(esc_key);

    writeln!(log, "Pressed ESC first time").expect("write log");

    // Press ESC again (should be a no-op since modal is already closed)
    let esc_key = KeyEvent::new(KeyCode::Esc, KeyModifiers::empty());
    vm.handle_key_event(esc_key);

    writeln!(log, "Pressed ESC second time").expect("write log");

    // Verify config is still the original value
    let config = vm.draft_cards[0].advanced_options.as_ref().expect("Config should exist");

    assert!(
        config.allow_egress,
        "allow_egress should remain at original value after multiple ESC presses (log: {log_hint})"
    );
    assert_eq!(
        config.timeout, "100s",
        "timeout should remain at original value after multiple ESC presses (log: {log_hint})"
    );

    writeln!(
        log,
        "Verified config is still original: allow_egress={}, timeout={}",
        config.allow_egress, config.timeout
    )
    .expect("write log");

    // Success message
    println!("✓ Test passed. See log for details (if needed): {log_hint}");
}

#[test]
#[allow(clippy::disallowed_methods)]
fn test_focus_restoration_after_enter_key() {
    let (mut log, log_path) = common::create_test_log("launch_options_focus_after_enter");
    let log_hint = log_path.display().to_string();

    let mut vm = common::build_view_model_with_repos();

    // Setup
    vm.focus_element = DashboardFocusState::DraftTask(0);
    if let Some(card) = vm.draft_cards.get_mut(0) {
        card.focus_element = CardFocusElement::AdvancedOptionsButton;
    }

    let draft_id = vm.draft_cards[0].id.clone();

    // Open modal
    vm.open_launch_options_modal(draft_id);

    writeln!(log, "Opened modal").expect("write log");

    // Press Enter to apply
    let enter_key = KeyEvent::new(KeyCode::Enter, KeyModifiers::empty());
    vm.handle_key_event(enter_key);

    writeln!(log, "Pressed Enter key").expect("write log");

    // Verify focus is restored to TaskDescription (as per PRD)
    assert_eq!(
        vm.focus_element,
        DashboardFocusState::DraftTask(0),
        "Global focus should remain on draft task (log: {log_hint})"
    );
    assert_eq!(
        vm.draft_cards[0].focus_element,
        CardFocusElement::TaskDescription,
        "Card focus should return to TaskDescription after Enter key (log: {log_hint})"
    );

    writeln!(
        log,
        "Verified focus: global={:?}, card={:?}",
        vm.focus_element, vm.draft_cards[0].focus_element
    )
    .expect("write log");

    // Success message
    println!("✓ Test passed. See log for details (if needed): {log_hint}");
}

#[test]
#[allow(clippy::disallowed_methods)]
fn test_mouse_click_apply_saves_changes() {
    let (mut log, log_path) = common::create_test_log("launch_options_mouse_apply");
    let log_hint = log_path.display().to_string();

    let mut vm = common::build_view_model_with_repos();

    // Setup: Open launch options modal
    vm.focus_element = DashboardFocusState::DraftTask(0);
    if let Some(card) = vm.draft_cards.get_mut(0) {
        card.focus_element = CardFocusElement::AdvancedOptionsButton;
    }

    let draft_id = vm.draft_cards[0].id.clone();
    vm.open_launch_options_modal(draft_id);
    assert_eq!(vm.modal_state, ModalState::LaunchOptions);

    writeln!(log, "Opened launch options modal").expect("write log");

    // Modify some config values
    if let Some(modal) = &mut vm.active_modal {
        if let ah_tui::view_model::ModalType::LaunchOptions { view_model } = &mut modal.modal_type {
            view_model.config.allow_egress = true;
            view_model.config.timeout = "300s".to_string();

            writeln!(
                log,
                "Modified config: allow_egress={}, timeout={}",
                view_model.config.allow_egress, view_model.config.timeout
            )
            .expect("write log");
        }
    }

    // Simulate mouse click on "Apply" area
    vm.perform_mouse_action(MouseAction::ModalApplyChanges);

    writeln!(log, "Clicked Apply with mouse").expect("write log");

    assert_eq!(
        vm.modal_state,
        ModalState::None,
        "Modal should be closed (log: {log_hint})"
    );

    // Verify changes were saved to the draft card
    let saved_config = vm.draft_cards[0]
        .advanced_options
        .as_ref()
        .expect("Advanced options should be set");

    assert!(
        saved_config.allow_egress,
        "allow_egress should be saved (log: {log_hint})"
    );
    assert_eq!(
        saved_config.timeout, "300s",
        "timeout should be saved (log: {log_hint})"
    );

    writeln!(
        log,
        "Verified saved config: allow_egress={}, timeout={}",
        saved_config.allow_egress, saved_config.timeout
    )
    .expect("write log");

    // Success message
    println!("✓ Test passed. See log for details (if needed): {log_hint}");
}

#[test]
#[allow(clippy::disallowed_methods)]
fn test_mouse_click_cancel_discards_changes() {
    let (mut log, log_path) = common::create_test_log("launch_options_mouse_cancel");
    let log_hint = log_path.display().to_string();

    let mut vm = common::build_view_model_with_repos();

    // Setup: Set initial config values
    let original_config = AdvancedLaunchOptions {
        allow_egress: true,
        timeout: "600s".to_string(),
        ..Default::default()
    };

    vm.focus_element = DashboardFocusState::DraftTask(0);
    if let Some(card) = vm.draft_cards.get_mut(0) {
        card.focus_element = CardFocusElement::AdvancedOptionsButton;
        card.advanced_options = Some(original_config.clone());
    }

    writeln!(
        log,
        "Original config: allow_egress={}, timeout={}",
        original_config.allow_egress, original_config.timeout
    )
    .expect("write log");

    // Open launch options modal
    let draft_id = vm.draft_cards[0].id.clone();
    vm.open_launch_options_modal(draft_id);
    assert_eq!(vm.modal_state, ModalState::LaunchOptions);

    writeln!(log, "Opened launch options modal").expect("write log");

    // Modify config values (these should be discarded)
    if let Some(modal) = &mut vm.active_modal {
        if let ah_tui::view_model::ModalType::LaunchOptions { view_model } = &mut modal.modal_type {
            view_model.config.allow_egress = false;
            view_model.config.timeout = "999s".to_string();

            writeln!(
                log,
                "Modified config (to be discarded): allow_egress={}, timeout={}",
                view_model.config.allow_egress, view_model.config.timeout
            )
            .expect("write log");
        }
    }

    // Simulate mouse click on "Cancel" area
    vm.perform_mouse_action(MouseAction::ModalCancelChanges);

    writeln!(log, "Clicked Cancel with mouse").expect("write log");

    assert_eq!(
        vm.modal_state,
        ModalState::None,
        "Modal should be closed (log: {log_hint})"
    );

    // Verify original config was restored
    let restored_config = vm.draft_cards[0]
        .advanced_options
        .as_ref()
        .expect("Advanced options should be set");

    assert!(
        restored_config.allow_egress,
        "allow_egress should be restored to original value (log: {log_hint})"
    );
    assert_eq!(
        restored_config.timeout, "600s",
        "timeout should be restored to original value (log: {log_hint})"
    );

    writeln!(
        log,
        "Verified restored config: allow_egress={}, timeout={}",
        restored_config.allow_egress, restored_config.timeout
    )
    .expect("write log");

    // Success message
    println!("✓ Test passed. See log for details (if needed): {log_hint}");
}

#[test]
#[allow(clippy::disallowed_methods)]
fn test_mouse_click_and_keyboard_interchangeable() {
    let (mut log, log_path) = common::create_test_log("launch_options_mouse_keyboard_mix");
    let log_hint = log_path.display().to_string();

    let mut vm = common::build_view_model_with_repos();

    // Setup
    vm.focus_element = DashboardFocusState::DraftTask(0);
    if let Some(card) = vm.draft_cards.get_mut(0) {
        card.focus_element = CardFocusElement::AdvancedOptionsButton;
    }

    let draft_id = vm.draft_cards[0].id.clone();

    // Test 1: Use mouse to apply
    vm.open_launch_options_modal(draft_id.clone());
    if let Some(modal) = &mut vm.active_modal {
        if let ah_tui::view_model::ModalType::LaunchOptions { view_model } = &mut modal.modal_type {
            view_model.config.allow_egress = true;
        }
    }
    vm.perform_mouse_action(MouseAction::ModalApplyChanges);
    assert!(
        vm.draft_cards[0].advanced_options.as_ref().unwrap().allow_egress,
        "Mouse apply should work (log: {log_hint})"
    );

    writeln!(log, "Test 1: Mouse apply works").expect("write log");

    // Test 2: Use keyboard to cancel
    vm.focus_element = DashboardFocusState::DraftTask(0);
    if let Some(card) = vm.draft_cards.get_mut(0) {
        card.focus_element = CardFocusElement::AdvancedOptionsButton;
    }
    vm.open_launch_options_modal(draft_id.clone());
    if let Some(modal) = &mut vm.active_modal {
        if let ah_tui::view_model::ModalType::LaunchOptions { view_model } = &mut modal.modal_type {
            view_model.config.allow_egress = false; // Try to change it back
        }
    }
    let esc_key = KeyEvent::new(KeyCode::Esc, KeyModifiers::empty());
    vm.handle_key_event(esc_key);
    assert!(
        vm.draft_cards[0].advanced_options.as_ref().unwrap().allow_egress,
        "Keyboard cancel should restore previous value (log: {log_hint})"
    );

    writeln!(log, "Test 2: Keyboard cancel works").expect("write log");

    // Test 3: Use keyboard to apply
    vm.focus_element = DashboardFocusState::DraftTask(0);
    if let Some(card) = vm.draft_cards.get_mut(0) {
        card.focus_element = CardFocusElement::AdvancedOptionsButton;
    }
    vm.open_launch_options_modal(draft_id.clone());
    if let Some(modal) = &mut vm.active_modal {
        if let ah_tui::view_model::ModalType::LaunchOptions { view_model } = &mut modal.modal_type {
            view_model.config.allow_containers = true;
        }
    }
    let enter_key = KeyEvent::new(KeyCode::Enter, KeyModifiers::empty());
    vm.handle_key_event(enter_key);
    assert!(
        vm.draft_cards[0].advanced_options.as_ref().unwrap().allow_containers,
        "Keyboard apply should work (log: {log_hint})"
    );

    writeln!(log, "Test 3: Keyboard apply works").expect("write log");

    // Test 4: Use mouse to cancel
    vm.focus_element = DashboardFocusState::DraftTask(0);
    if let Some(card) = vm.draft_cards.get_mut(0) {
        card.focus_element = CardFocusElement::AdvancedOptionsButton;
    }
    vm.open_launch_options_modal(draft_id);
    if let Some(modal) = &mut vm.active_modal {
        if let ah_tui::view_model::ModalType::LaunchOptions { view_model } = &mut modal.modal_type {
            view_model.config.allow_containers = false; // Try to change it back
        }
    }
    vm.perform_mouse_action(MouseAction::ModalCancelChanges);
    assert!(
        vm.draft_cards[0].advanced_options.as_ref().unwrap().allow_containers,
        "Mouse cancel should restore previous value (log: {log_hint})"
    );

    writeln!(log, "Test 4: Mouse cancel works").expect("write log");

    // Success message
    println!("✓ Test passed. See log for details (if needed): {log_hint}");
}

#[test]
#[allow(clippy::disallowed_methods)]
fn test_split_launch_shortcut_restores_focus_to_task_description() {
    let (mut log, log_path) =
        common::create_test_log("launch_options_split_shortcut_focus_restore");
    let log_hint = log_path.display().to_string();

    let mut vm = common::build_view_model_with_repos();

    // Setup: Set focus on advanced options button to open launch options modal
    vm.focus_element = DashboardFocusState::DraftTask(0);
    if let Some(card) = vm.draft_cards.get_mut(0) {
        card.focus_element = CardFocusElement::AdvancedOptionsButton;
        // Add some description text
        card.description.insert_str("Test task description");
    }

    writeln!(
        log,
        "Initial focus: {:?}, card focus: {:?}",
        vm.focus_element, vm.draft_cards[0].focus_element
    )
    .expect("write log");

    // Open launch options modal
    let draft_id = vm.draft_cards[0].id.clone();
    vm.open_launch_options_modal(draft_id);
    assert_eq!(vm.modal_state, ModalState::LaunchOptions);

    writeln!(log, "Opened launch options modal").expect("write log");

    // Test each split launch shortcut key
    let test_cases = vec![
        ('t', "Launch in new tab (t)"),
        ('s', "Launch in split view (s)"),
        ('h', "Launch in horizontal split (h)"),
        ('v', "Launch in vertical split (v)"),
        ('T', "Launch in new tab and focus (T)"),
        ('S', "Launch in split view and focus (S)"),
        ('H', "Launch in horizontal split and focus (H)"),
        ('V', "Launch in vertical split and focus (V)"),
    ];

    for (key_char, option_name) in test_cases {
        writeln!(
            log,
            "\nTesting shortcut key: '{}' ({})",
            key_char, option_name
        )
        .expect("write log");

        // Reset focus to advanced options button
        vm.focus_element = DashboardFocusState::DraftTask(0);
        if let Some(card) = vm.draft_cards.get_mut(0) {
            card.focus_element = CardFocusElement::AdvancedOptionsButton;
        }

        // Reopen launch options modal
        let draft_id = vm.draft_cards[0].id.clone();
        vm.open_launch_options_modal(draft_id);
        assert_eq!(
            vm.modal_state,
            ModalState::LaunchOptions,
            "Modal should be open for key '{}' (log: {log_hint})",
            key_char
        );

        // Press the shortcut key
        let key_event = KeyEvent::new(KeyCode::Char(key_char), KeyModifiers::empty());
        let handled = vm.handle_key_event(key_event);

        writeln!(
            log,
            "Key '{}' handled: {}, modal_state: {:?}, focus: {:?}, card_focus: {:?}",
            key_char,
            handled,
            vm.modal_state,
            vm.focus_element,
            vm.draft_cards.first().map(|c| c.focus_element)
        )
        .expect("write log");

        assert!(
            handled,
            "Key '{}' should be handled (log: {log_hint})",
            key_char
        );
        assert_eq!(
            vm.modal_state,
            ModalState::None,
            "Modal should be closed after key '{}' (log: {log_hint})",
            key_char
        );

        // Verify focus is restored to TaskDescription, not advanced options button
        assert_eq!(
            vm.focus_element,
            DashboardFocusState::DraftTask(0),
            "Global focus should be on draft task 0 after key '{}' (log: {log_hint})",
            key_char
        );
        assert_eq!(
            vm.draft_cards[0].focus_element,
            CardFocusElement::TaskDescription,
            "Card focus should return to TaskDescription after key '{}', not advanced options button (log: {log_hint})",
            key_char
        );

        writeln!(
            log,
            "✓ Focus correctly restored to TaskDescription for key '{}'",
            key_char
        )
        .expect("write log");
    }

    // Success message
    println!("✓ Test passed. See log for details (if needed): {log_hint}");
}

#[test]
#[allow(clippy::disallowed_methods)]
fn test_space_key_toggles_boolean_options() {
    let (mut log, log_path) = common::create_test_log("launch_options_space_toggle");
    let log_hint = log_path.display().to_string();

    let mut vm = common::build_view_model_with_repos();

    // Setup: Open launch options modal
    vm.focus_element = DashboardFocusState::DraftTask(0);
    if let Some(card) = vm.draft_cards.get_mut(0) {
        card.focus_element = CardFocusElement::AdvancedOptionsButton;
    }

    let draft_id = vm.draft_cards[0].id.clone();
    vm.open_launch_options_modal(draft_id);
    assert_eq!(vm.modal_state, ModalState::LaunchOptions);

    writeln!(log, "Opened launch options modal").expect("write log");

    // Get initial boolean value
    let initial_allow_egress = if let Some(modal) = &vm.active_modal {
        if let ah_tui::view_model::ModalType::LaunchOptions { view_model } = &modal.modal_type {
            view_model.config.allow_egress
        } else {
            false
        }
    } else {
        false
    };

    writeln!(log, "Initial allow_egress value: {}", initial_allow_egress).expect("write log");

    // Navigate to the allow_egress option (index 5)
    if let Some(modal) = &mut vm.active_modal {
        if let ah_tui::view_model::ModalType::LaunchOptions { view_model } = &mut modal.modal_type {
            view_model.selected_option_index = 5; // allow_egress option
            view_model.active_column = LaunchOptionsColumn::Options;
        }
    }

    writeln!(log, "Navigated to allow_egress option (index 5)").expect("write log");

    // Press Space key to toggle the boolean value
    let space_key = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::empty());
    let handled = vm.handle_key_event(space_key);

    writeln!(
        log,
        "Space key handled: {}, modal_state: {:?}",
        handled, vm.modal_state
    )
    .expect("write log");

    assert!(handled, "Space key should be handled (log: {log_hint})");
    assert_eq!(
        vm.modal_state,
        ModalState::LaunchOptions,
        "Modal should still be open after Space (log: {log_hint})"
    );

    // Verify the boolean value was toggled
    if let Some(modal) = &vm.active_modal {
        if let ah_tui::view_model::ModalType::LaunchOptions { view_model } = &modal.modal_type {
            let new_value = view_model.config.allow_egress;
            writeln!(
                log,
                "After Space key - allow_egress: {} (was {})",
                new_value, initial_allow_egress
            )
            .expect("write log");

            assert_eq!(
                new_value, !initial_allow_egress,
                "allow_egress should be toggled (log: {log_hint})"
            );
        }
    }

    // Toggle it back with Space
    let space_key2 = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::empty());
    let handled2 = vm.handle_key_event(space_key2);

    assert!(
        handled2,
        "Second Space key should be handled (log: {log_hint})"
    );

    // Verify it toggled back
    if let Some(modal) = &vm.active_modal {
        if let ah_tui::view_model::ModalType::LaunchOptions { view_model } = &modal.modal_type {
            let final_value = view_model.config.allow_egress;
            writeln!(
                log,
                "After second Space key - allow_egress: {}",
                final_value
            )
            .expect("write log");

            assert_eq!(
                final_value, initial_allow_egress,
                "allow_egress should be toggled back to original (log: {log_hint})"
            );
        }
    }

    writeln!(log, "✓ Space key successfully toggles boolean values").expect("write log");
    // Success message
    println!("✓ Test passed. See log for details (if needed): {log_hint}");
}

#[test]
#[allow(clippy::disallowed_methods)]
fn test_enter_key_saves_and_closes_like_a_key() {
    let (mut log, log_path) = common::create_test_log("launch_options_enter_saves");
    let log_hint = log_path.display().to_string();

    let mut vm = common::build_view_model_with_repos();

    // Setup: Open launch options modal
    vm.focus_element = DashboardFocusState::DraftTask(0);
    if let Some(card) = vm.draft_cards.get_mut(0) {
        card.focus_element = CardFocusElement::AdvancedOptionsButton;
    }

    let draft_id = vm.draft_cards[0].id.clone();
    vm.open_launch_options_modal(draft_id);
    assert_eq!(vm.modal_state, ModalState::LaunchOptions);

    writeln!(log, "Opened launch options modal").expect("write log");

    // Modify some config values using Space key to toggle
    if let Some(modal) = &mut vm.active_modal {
        if let ah_tui::view_model::ModalType::LaunchOptions { view_model } = &mut modal.modal_type {
            view_model.selected_option_index = 5; // allow_egress
            view_model.active_column = LaunchOptionsColumn::Options;
        }
    }

    // Press Space to toggle allow_egress to true
    let space_key = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::empty());
    vm.handle_key_event(space_key);

    writeln!(log, "Toggled allow_egress with Space key").expect("write log");

    // Verify the modal is still open after Space
    assert_eq!(
        vm.modal_state,
        ModalState::LaunchOptions,
        "Modal should still be open after Space key (log: {log_hint})"
    );

    // Now press Enter to save and close
    let enter_key = KeyEvent::new(KeyCode::Enter, KeyModifiers::empty());
    let handled = vm.handle_key_event(enter_key);

    writeln!(
        log,
        "Enter key handled: {}, modal_state: {:?}",
        handled, vm.modal_state
    )
    .expect("write log");

    assert!(handled, "Enter key should be handled (log: {log_hint})");
    assert_eq!(
        vm.modal_state,
        ModalState::None,
        "Modal should be closed after Enter key (log: {log_hint})"
    );

    // Verify changes were saved to the draft card
    let saved_config = vm.draft_cards[0]
        .advanced_options
        .as_ref()
        .expect("Advanced options should be set");

    assert!(
        saved_config.allow_egress,
        "allow_egress should be saved as true (log: {log_hint})"
    );

    writeln!(
        log,
        "Verified saved config: allow_egress={}",
        saved_config.allow_egress
    )
    .expect("write log");

    // Verify focus restored to TaskDescription
    assert_eq!(
        vm.focus_element,
        DashboardFocusState::DraftTask(0),
        "Focus should return to draft task (log: {log_hint})"
    );

    if let Some(card) = vm.draft_cards.first() {
        assert_eq!(
            card.focus_element,
            CardFocusElement::TaskDescription,
            "Card focus should return to TaskDescription (log: {log_hint})"
        );
    }

    writeln!(log, "✓ Enter key works like 'A' key - saves and closes").expect("write log");

    // Success message
    println!("✓ Test passed. See log for details (if needed): {log_hint}");
}

#[test]
#[allow(clippy::disallowed_methods)]
fn test_enter_in_enum_popup_selects_value() {
    let (mut log, log_path) = common::create_test_log("launch_options_enter_in_popup");
    let log_hint = log_path.display().to_string();

    let mut vm = common::build_view_model_with_repos();

    // Setup: Open launch options modal
    vm.focus_element = DashboardFocusState::DraftTask(0);
    if let Some(card) = vm.draft_cards.get_mut(0) {
        card.focus_element = CardFocusElement::AdvancedOptionsButton;
    }

    let draft_id = vm.draft_cards[0].id.clone();
    vm.open_launch_options_modal(draft_id);
    assert_eq!(vm.modal_state, ModalState::LaunchOptions);

    writeln!(log, "Opened launch options modal").expect("write log");

    // Navigate to sandbox_profile (enum option at index 1)
    if let Some(modal) = &mut vm.active_modal {
        if let ah_tui::view_model::ModalType::LaunchOptions { view_model } = &mut modal.modal_type {
            view_model.selected_option_index = 1; // sandbox_profile
            view_model.active_column = LaunchOptionsColumn::Options;
        }
    }

    // Press Space to open the enum popup
    let space_key = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::empty());
    vm.handle_key_event(space_key);

    writeln!(log, "Opened enum popup with Space key").expect("write log");

    // Verify popup is open
    let popup_open = if let Some(modal) = &vm.active_modal {
        if let ah_tui::view_model::ModalType::LaunchOptions { view_model } = &modal.modal_type {
            view_model.inline_enum_popup.is_some()
        } else {
            false
        }
    } else {
        false
    };

    assert!(
        popup_open,
        "Enum popup should be open after Space (log: {log_hint})"
    );

    // Press Enter to select the current value in the popup
    let enter_key = KeyEvent::new(KeyCode::Enter, KeyModifiers::empty());
    let handled = vm.handle_key_event(enter_key);

    writeln!(
        log,
        "Enter key in popup handled: {}, modal_state: {:?}",
        handled, vm.modal_state
    )
    .expect("write log");

    assert!(
        handled,
        "Enter key in popup should be handled (log: {log_hint})"
    );

    // Verify popup is closed but modal is still open
    assert_eq!(
        vm.modal_state,
        ModalState::LaunchOptions,
        "Modal should still be open after Enter in popup (log: {log_hint})"
    );

    let popup_still_open = if let Some(modal) = &vm.active_modal {
        if let ah_tui::view_model::ModalType::LaunchOptions { view_model } = &modal.modal_type {
            view_model.inline_enum_popup.is_some()
        } else {
            false
        }
    } else {
        false
    };

    assert!(
        !popup_still_open,
        "Enum popup should be closed after Enter (log: {log_hint})"
    );

    writeln!(
        log,
        "✓ Enter in enum popup selects value and closes popup only"
    )
    .expect("write log");

    // Success message
    println!("✓ Test passed. See log for details (if needed): {log_hint}");
}
