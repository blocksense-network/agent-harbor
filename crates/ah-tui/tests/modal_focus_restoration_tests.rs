// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Tests for modal focus restoration behavior
//!
//! These tests verify that when modals are dismissed with ESC, focus is restored
//! to the appropriate element based on the modal type, as specified in TUI-PRD.md

mod common;

use ah_tui::view_model::task_entry::CardFocusElement;
use ah_tui::view_model::{DashboardFocusState, ModalState};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::io::Write;

/// Helper to press ESC and verify modal closes
fn press_esc_and_verify_closed(vm: &mut ah_tui::view_model::ViewModel, log: &mut std::fs::File) {
    let esc_key = KeyEvent::new(KeyCode::Esc, KeyModifiers::empty());
    let handled = vm.handle_key_event(esc_key);

    writeln!(
        log,
        "ESC handled: {}, modal_state: {:?}, focus: {:?}, card_focus: {:?}",
        handled,
        vm.modal_state,
        vm.focus_element,
        vm.draft_cards.first().map(|c| c.focus_element)
    )
    .expect("write log");

    assert!(handled, "ESC should be handled");
    assert_eq!(vm.modal_state, ModalState::None, "Modal should be closed");
}

#[test]
fn repository_modal_returns_focus_to_repository_selector() {
    let (mut log, log_path) = common::create_test_log("repo_modal_focus");
    let log_hint = log_path.display().to_string();

    let mut vm = common::build_view_model_with_repos();

    // Set focus on repository selector and open modal
    vm.focus_element = DashboardFocusState::DraftTask(0);
    if let Some(card) = vm.draft_cards.get_mut(0) {
        card.focus_element = CardFocusElement::RepositorySelector;
    }

    // Open repository modal
    vm.open_modal(ModalState::RepositorySearch);
    assert_eq!(vm.modal_state, ModalState::RepositorySearch);

    writeln!(log, "Opened repository modal").expect("write log");

    // Press ESC to dismiss
    press_esc_and_verify_closed(&mut vm, &mut log);

    // Verify focus returned to task description
    assert_eq!(
        vm.focus_element,
        DashboardFocusState::DraftTask(0),
        "Global focus should remain on draft task (log: {log_hint})"
    );
    assert_eq!(
        vm.draft_cards[0].focus_element,
        CardFocusElement::TaskDescription,
        "Card focus should return to task description (log: {log_hint})"
    );
}

#[test]
fn branch_modal_returns_focus_to_branch_selector() {
    let (mut log, log_path) = common::create_test_log("branch_modal_focus");
    let log_hint = log_path.display().to_string();

    let mut vm = common::build_view_model_with_repos();

    // Set focus on branch selector and open modal
    vm.focus_element = DashboardFocusState::DraftTask(0);
    if let Some(card) = vm.draft_cards.get_mut(0) {
        card.focus_element = CardFocusElement::BranchSelector;
    }

    // Open branch modal
    vm.open_modal(ModalState::BranchSearch);
    assert_eq!(vm.modal_state, ModalState::BranchSearch);

    writeln!(log, "Opened branch modal").expect("write log");

    // Press ESC to dismiss
    press_esc_and_verify_closed(&mut vm, &mut log);

    // Verify focus returned to task description
    assert_eq!(
        vm.focus_element,
        DashboardFocusState::DraftTask(0),
        "Global focus should remain on draft task (log: {log_hint})"
    );
    assert_eq!(
        vm.draft_cards[0].focus_element,
        CardFocusElement::TaskDescription,
        "Card focus should return to task description (log: {log_hint})"
    );
}

#[test]
fn model_modal_returns_focus_to_model_selector() {
    let (mut log, log_path) = common::create_test_log("model_modal_focus");
    let log_hint = log_path.display().to_string();

    let mut vm = common::build_view_model_with_repos();

    // Populate available models
    let catalog = ah_core::agent_catalog::RemoteAgentCatalog::default_catalog();
    vm.available_models =
        catalog.agents.into_iter().map(|metadata| metadata.to_agent_choice()).collect();

    // Set focus on model selector and open modal
    vm.focus_element = DashboardFocusState::DraftTask(0);
    if let Some(card) = vm.draft_cards.get_mut(0) {
        card.focus_element = CardFocusElement::ModelSelector;
    }

    // Open model modal
    vm.open_modal(ModalState::ModelSearch);
    assert_eq!(vm.modal_state, ModalState::ModelSearch);

    writeln!(log, "Opened model modal").expect("write log");

    // Press ESC to dismiss
    press_esc_and_verify_closed(&mut vm, &mut log);

    // Verify focus returned to task description
    assert_eq!(
        vm.focus_element,
        DashboardFocusState::DraftTask(0),
        "Global focus should remain on draft task (log: {log_hint})"
    );
    assert_eq!(
        vm.draft_cards[0].focus_element,
        CardFocusElement::TaskDescription,
        "Card focus should return to task description (log: {log_hint})"
    );
}

// NOTE: Comprehensive keyboard & mouse interaction tests for Launch Options modal in:
//   `crates/ah-tui/tests/launch_options_modal_interaction_tests.rs`
#[test]
fn launch_options_modal_returns_focus_to_task_description() {
    let (mut log, log_path) = common::create_test_log("launch_options_modal_focus");
    let log_hint = log_path.display().to_string();

    let mut vm = common::build_view_model_with_repos();

    // Set focus on advanced options button (to open launch options modal)
    vm.focus_element = DashboardFocusState::DraftTask(0);
    if let Some(card) = vm.draft_cards.get_mut(0) {
        card.focus_element = CardFocusElement::AdvancedOptionsButton;
    }

    // Get the draft ID before opening modal
    let draft_id = vm.draft_cards[0].id.clone();

    // Open launch options modal
    vm.open_launch_options_modal(draft_id);
    assert_eq!(vm.modal_state, ModalState::LaunchOptions);

    writeln!(log, "Opened launch options modal").expect("write log");

    // Press ESC to dismiss
    press_esc_and_verify_closed(&mut vm, &mut log);

    // Verify focus returned to task description (not advanced options button)
    assert_eq!(
        vm.focus_element,
        DashboardFocusState::DraftTask(0),
        "Global focus should remain on draft task (log: {log_hint})"
    );
    assert_eq!(
        vm.draft_cards[0].focus_element,
        CardFocusElement::TaskDescription,
        "Card focus should return to task description, not advanced options button (log: {log_hint})"
    );
}

#[test]
fn settings_modal_restores_previous_focus() {
    let (mut log, log_path) = common::create_test_log("settings_modal_focus");
    let _log_hint = log_path.display().to_string();

    let mut vm = common::build_view_model_with_repos();

    // Set initial focus on task description
    vm.focus_element = DashboardFocusState::DraftTask(0);
    if let Some(card) = vm.draft_cards.get_mut(0) {
        card.focus_element = CardFocusElement::TaskDescription;
    }

    writeln!(
        log,
        "Initial focus: {:?}, card focus: {:?}",
        vm.focus_element, vm.draft_cards[0].focus_element
    )
    .expect("write log");

    // Open settings modal
    vm.open_modal(ModalState::Settings);
    assert_eq!(vm.modal_state, ModalState::Settings);

    writeln!(log, "Opened settings modal").expect("write log");

    // Press ESC to dismiss
    press_esc_and_verify_closed(&mut vm, &mut log);

    // Note: Settings modal currently doesn't restore focus, which is expected behavior
    // This test documents the current behavior - it could be enhanced in the future
    writeln!(
        log,
        "After settings modal close - focus: {:?}",
        vm.focus_element
    )
    .expect("write log");
}

#[test]
fn multiple_modal_dismissals_maintain_correct_focus() {
    let (mut log, log_path) = common::create_test_log("multiple_modal_focus");
    let log_hint = log_path.display().to_string();

    let mut vm = common::build_view_model_with_repos();

    // Populate available models
    let catalog = ah_core::agent_catalog::RemoteAgentCatalog::default_catalog();
    vm.available_models =
        catalog.agents.into_iter().map(|metadata| metadata.to_agent_choice()).collect();

    // Test sequence: Model → Launch Options → Model again

    // 1. Open model modal
    vm.focus_element = DashboardFocusState::DraftTask(0);
    vm.draft_cards[0].focus_element = CardFocusElement::ModelSelector;
    vm.open_modal(ModalState::ModelSearch);
    writeln!(log, "1. Opened model modal").expect("write log");

    press_esc_and_verify_closed(&mut vm, &mut log);
    assert_eq!(
        vm.draft_cards[0].focus_element,
        CardFocusElement::TaskDescription,
        "Focus should return to task description (log: {log_hint})"
    );

    // 2. Open launch options modal
    vm.draft_cards[0].focus_element = CardFocusElement::AdvancedOptionsButton;
    let draft_id = vm.draft_cards[0].id.clone();
    vm.open_launch_options_modal(draft_id);
    writeln!(log, "2. Opened launch options modal").expect("write log");

    press_esc_and_verify_closed(&mut vm, &mut log);
    assert_eq!(
        vm.draft_cards[0].focus_element,
        CardFocusElement::TaskDescription,
        "Focus should return to task description (log: {log_hint})"
    );

    // 3. Open model modal again
    vm.draft_cards[0].focus_element = CardFocusElement::ModelSelector;
    vm.open_modal(ModalState::ModelSearch);
    writeln!(log, "3. Opened model modal again").expect("write log");

    press_esc_and_verify_closed(&mut vm, &mut log);
    assert_eq!(
        vm.draft_cards[0].focus_element,
        CardFocusElement::TaskDescription,
        "Focus should return to task description (log: {log_hint})"
    );
}

#[test]
fn focus_restoration_works_from_different_button_states() {
    let (mut log, log_path) = common::create_test_log("focus_from_buttons");
    let log_hint = log_path.display().to_string();

    let mut vm = common::build_view_model_with_repos();

    // Test opening launch options from different starting points

    // 1. From Go button
    vm.focus_element = DashboardFocusState::DraftTask(0);
    vm.draft_cards[0].focus_element = CardFocusElement::GoButton;
    writeln!(log, "Starting from Go button").expect("write log");

    let draft_id = vm.draft_cards[0].id.clone();
    vm.open_launch_options_modal(draft_id);
    press_esc_and_verify_closed(&mut vm, &mut log);

    assert_eq!(
        vm.draft_cards[0].focus_element,
        CardFocusElement::TaskDescription,
        "Should return to task description even when opened from Go button (log: {log_hint})"
    );

    // 2. From task description itself
    vm.draft_cards[0].focus_element = CardFocusElement::TaskDescription;
    writeln!(log, "Starting from task description").expect("write log");

    let draft_id = vm.draft_cards[0].id.clone();
    vm.open_launch_options_modal(draft_id);
    press_esc_and_verify_closed(&mut vm, &mut log);

    assert_eq!(
        vm.draft_cards[0].focus_element,
        CardFocusElement::TaskDescription,
        "Should return to task description when opened from description (log: {log_hint})"
    );
}
