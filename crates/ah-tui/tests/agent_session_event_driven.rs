// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

mod support;
use ah_core::TaskEvent;

use chrono::Utc;
use support::{STANDARD_THEMES, STANDARD_VIEWPORTS, render_snapshot_with_theme, vm_with_events};

fn assert_snapshots_all_sizes_and_themes(
    base: &str,
    vm: &ah_tui::view_model::agent_session_model::AgentSessionViewModel,
) {
    for &(theme_name, theme_fn) in STANDARD_THEMES {
        let theme = theme_fn();
        for &(w, h) in STANDARD_VIEWPORTS {
            let name = format!("{base}__{theme_name}__{w}x{h}");
            let snapshot = render_snapshot_with_theme(&name, vm, w, h, &theme);
            insta::assert_snapshot!(name, snapshot);
        }
    }
}

fn make_settings() -> insta::Settings {
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_suffix("");
    settings.set_snapshot_path("{snapshot}.snap.svg");
    settings
}

#[test]
fn renders_interleaved_events_and_user_input() {
    let _guard = make_settings().bind_to_scope();

    // 1. Initial events
    let initial_events = vec![TaskEvent::Thought {
        thought: "Initial thought".into(),
        reasoning: None,
        ts: Utc::now(),
    }];

    let mut vm = vm_with_events("interleaved-session", initial_events, 10);

    // 2. Simulate user typing in the task entry
    // We need to focus the instructions area first
    vm.set_focus_area(ah_tui::view_model::agent_session_model::FocusArea::Instructions);

    // Simulate typing "Hello"
    // Note: In a real scenario, we would use apply_keys helper, but here we can simulate
    // the state change directly via the task entry view model if exposed,
    // OR we can use the public handle_key_with_minor_modes API.
    // Let's use handle_key_with_minor_modes to be true to the requirement.

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let keys = vec![
        KeyCode::Char('H'),
        KeyCode::Char('e'),
        KeyCode::Char('l'),
        KeyCode::Char('l'),
        KeyCode::Char('o'),
    ];

    for code in keys {
        vm.handle_key_with_minor_modes(KeyEvent::new(code, KeyModifiers::empty()));
    }

    // 3. Simulate "Enter" to submit (this would trigger an action in the real app)
    // In the real app, the Controller would take the input, create an optimistic row,
    // and then clear the input box.
    // We simulate this by manually adding the unconfirmed row and clearing the input.

    // Simulate Controller action:
    vm.push_row(
        ah_tui::view_model::task_execution::AgentActivityRow::UserInput {
            author: "You".into(),
            content: "Hello".into(),
            confirmed: false,
            timestamp: std::time::Instant::now(),
        },
    );
    // Clear input (simulated)
    // vm.task_entry.set_text(""); // If we had access to clear text

    // Verify that we have exactly 2 rows (initial thought + unconfirmed user input)
    assert_eq!(
        vm.activity().len(),
        2,
        "Should have 2 rows: initial thought + unconfirmed input"
    );

    // Verify the last row is unconfirmed
    if let Some(ah_tui::view_model::task_execution::AgentActivityRow::UserInput {
        confirmed, ..
    }) = vm.activity().last()
    {
        assert!(!confirmed, "User input should be unconfirmed initially");
    } else {
        panic!("Last row should be UserInput");
    }

    // Snapshot: Unconfirmed state (should show spinner/indicator)
    assert_snapshots_all_sizes_and_themes("agent_activity_interleaved_2_unconfirmed", &vm);

    // 4. Simulate the system processing the input and emitting a UserInput event
    // The handle_task_event logic should find the unconfirmed row and mark it confirmed.

    let user_input_event = TaskEvent::UserInput {
        author: "You".into(), // Must match for reconciliation
        content: "Hello".into(),
        ts: Utc::now(),
    };
    vm.handle_task_event(&user_input_event);

    // Verify that we STILL have exactly 2 rows (no new row added)
    assert_eq!(
        vm.activity().len(),
        2,
        "Should still have 2 rows after confirmation"
    );

    // Verify the last row is now confirmed
    if let Some(ah_tui::view_model::task_execution::AgentActivityRow::UserInput {
        confirmed, ..
    }) = vm.activity().last()
    {
        assert!(*confirmed, "User input should be confirmed after event");
    } else {
        panic!("Last row should be UserInput");
    }

    // Snapshot: Confirmed state (spinner should be gone)
    assert_snapshots_all_sizes_and_themes("agent_activity_interleaved_3_confirmed", &vm);

    // 5. More agent events follow
    let follow_up_events = vec![TaskEvent::Thought {
        thought: "I heard you say Hello".into(),
        reasoning: None,
        ts: Utc::now(),
    }];

    for event in follow_up_events {
        vm.handle_task_event(&event);
    }

    // Snapshots are stored in `crates/ah-tui/tests/{snapshot}.snap.svg/`
    assert_snapshots_all_sizes_and_themes("agent_activity_interleaved_4_response", &vm);
}
