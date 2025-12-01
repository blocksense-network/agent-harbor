// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

mod support;
use ah_core::TaskEvent;
use chrono::Utc;
use support::vm_with_events;

fn make_settings() -> insta::Settings {
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_suffix("");
    settings.set_snapshot_path("{snapshot}.snap.svg");
    settings
}

#[test]
fn renders_fuzzy_matched_user_input() {
    let _guard = make_settings().bind_to_scope();

    let initial_events = vec![TaskEvent::Thought {
        thought: "Initial thought".into(),
        reasoning: None,
        ts: Utc::now(),
    }];

    let mut vm = vm_with_events("fuzzy-match-session", initial_events, 10);

    // Simulate Controller action: Add unconfirmed row
    // Use a long enough string so that 1 char difference is within 5% tolerance (>= 20 chars)
    let client_content = "Please analyze this code for me";
    vm.push_row(
        ah_tui::view_model::task_execution::AgentActivityRow::UserInput {
            author: "You".into(),
            content: client_content.into(),
            confirmed: false,
            timestamp: std::time::Instant::now(),
        },
    );

    // Verify unconfirmed state
    if let Some(ah_tui::view_model::task_execution::AgentActivityRow::UserInput {
        confirmed,
        content,
        ..
    }) = vm.activity().last()
    {
        assert!(!confirmed, "User input should be unconfirmed initially");
        assert_eq!(content, client_content);
    } else {
        panic!("Last row should be UserInput");
    }

    // Simulate incoming event with slightly different content and different author
    // "Please analyze this code for me" (31 chars)
    // "Please analyze this code for me." (32 chars)
    // Distance = 1. Similarity = 1 - 1/32 = 0.968 > 0.95.
    let server_content = "Please analyze this code for me.";
    let user_input_event = TaskEvent::UserInput {
        author: "User".into(),          // Different author
        content: server_content.into(), // Slightly different content
        ts: Utc::now(),
    };
    vm.handle_task_event(&user_input_event);

    // Verify confirmation and update
    if let Some(ah_tui::view_model::task_execution::AgentActivityRow::UserInput {
        confirmed,
        content,
        author,
        ..
    }) = vm.activity().last()
    {
        assert!(
            *confirmed,
            "User input should be confirmed after fuzzy match"
        );
        assert_eq!(
            content, server_content,
            "Content should be updated to match server"
        );
        assert_eq!(author, "User", "Author should be updated to match server");
    } else {
        panic!("Last row should be UserInput");
    }
}
