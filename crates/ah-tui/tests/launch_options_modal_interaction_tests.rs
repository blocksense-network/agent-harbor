// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Tests for Launch Options modal interaction handling (keyboard and mouse)
//!
//! This comprehensive test suite verifies the Launch Options modal's interaction behavior:
//!
//! ## Keyboard Interactions
//! - **'A' key**: Saves changes and closes the modal, restoring focus to TaskDescription
//! - **'Esc' key**: Discards changes and restores the original configuration before closing
//!
//! ## Mouse Interactions
//! - **Click "A Apply"**: Same behavior as pressing 'A' key
//! - **Click "Esc Cancel"**: Same behavior as pressing 'Esc' key
//!
//! ## Test Coverage
//! 1. Basic save functionality (A key and mouse click)
//! 2. Basic cancel functionality (Esc key and mouse click)
//! 3. Config restoration when no prior config exists
//! 4. Persistence of saved changes across modal reopenings
//! 5. State integrity with multiple ESC presses
//! 6. Focus restoration after applying changes
//! 7. Interchangeability of keyboard and mouse inputs
//!
//! All tests follow the project's testing guidelines:
//! - Each test creates a unique log file for full output capture
//! - Minimal console output on success to preserve AI context windows
//! - Log path printed on completion for debugging when needed

mod common;

use ah_tui::view_model::agents_selector_model::{AdvancedLaunchOptions, MouseAction};
use ah_tui::view_model::task_entry::CardFocusElement;
use ah_tui::view_model::{DashboardFocusState, ModalState};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::io::Write;

#[test]
#[allow(clippy::disallowed_methods)]
fn test_a_key_saves_changes_and_closes_modal() {
    let (mut log, log_path) = common::create_test_log("launch_options_a_key_saves");
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
            // Change some values
            view_model.config.allow_egress = true;
            view_model.config.allow_containers = true;
            view_model.config.timeout = "300s".to_string();

            writeln!(
                log,
                "Modified config: allow_egress={}, allow_containers={}, timeout={}",
                view_model.config.allow_egress,
                view_model.config.allow_containers,
                view_model.config.timeout
            )
            .expect("write log");
        }
    }

    // Press 'A' key to apply changes
    let a_key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::empty());
    let handled = vm.handle_key_event(a_key);

    writeln!(
        log,
        "A key handled: {}, modal_state: {:?}",
        handled, vm.modal_state
    )
    .expect("write log");

    assert!(handled, "A key should be handled (log: {log_hint})");
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
    assert!(
        saved_config.allow_containers,
        "allow_containers should be saved (log: {log_hint})"
    );
    assert_eq!(
        saved_config.timeout, "300s",
        "timeout should be saved (log: {log_hint})"
    );

    writeln!(
        log,
        "Verified saved config: allow_egress={}, allow_containers={}, timeout={}",
        saved_config.allow_egress, saved_config.allow_containers, saved_config.timeout
    )
    .expect("write log");

    // Success message
    println!("✓ Test passed. See log for details (if needed): {log_hint}");
}

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
fn test_a_key_followed_by_reopening_modal_shows_saved_changes() {
    let (mut log, log_path) = common::create_test_log("launch_options_a_key_persists");
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

    // Press 'A' to save
    let a_key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::empty());
    vm.handle_key_event(a_key);

    writeln!(log, "Saved changes with A key").expect("write log");

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
fn test_focus_restoration_after_a_key() {
    let (mut log, log_path) = common::create_test_log("launch_options_focus_after_a");
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

    // Press 'A' to apply
    let a_key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::empty());
    vm.handle_key_event(a_key);

    writeln!(log, "Pressed A key").expect("write log");

    // Verify focus is restored to TaskDescription (as per PRD)
    assert_eq!(
        vm.focus_element,
        DashboardFocusState::DraftTask(0),
        "Global focus should remain on draft task (log: {log_hint})"
    );
    assert_eq!(
        vm.draft_cards[0].focus_element,
        CardFocusElement::TaskDescription,
        "Card focus should return to TaskDescription after A key (log: {log_hint})"
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
    let a_key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::empty());
    vm.handle_key_event(a_key);
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
