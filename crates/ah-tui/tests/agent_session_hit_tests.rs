// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

mod support;

use ah_tui::view_model::agent_session_model::AgentSessionMouseAction;
use support::{render_hits, render_snapshot, vm_with_events};

#[test]
fn registers_copy_and_expand_hit_zones() {
    let events = vec![
        ah_core::TaskEvent::Thought {
            thought: "first".into(),
            reasoning: None,
            ts: chrono::Utc::now(),
        },
        ah_core::TaskEvent::Thought {
            thought: "second".into(),
            reasoning: None,
            ts: chrono::Utc::now(),
        },
    ];
    let vm = vm_with_events("hit-session", events, 4);

    let hits = render_hits(&vm, 80, 24);
    let copy_hits: Vec<_> = hits
        .zones()
        .iter()
        .filter(|z| matches!(z.action, AgentSessionMouseAction::Copy(_)))
        .collect();
    let expand_hits: Vec<_> = hits
        .zones()
        .iter()
        .filter(|z| matches!(z.action, AgentSessionMouseAction::Expand(_)))
        .collect();

    assert!(!copy_hits.is_empty(), "expected copy hit zones");
    assert!(!expand_hits.is_empty(), "expected expand hit zones");
}

#[test]
fn fork_hit_zone_targets_last_visible_index() {
    let events = vec![
        ah_core::TaskEvent::Thought {
            thought: "top".into(),
            reasoning: None,
            ts: chrono::Utc::now(),
        },
        ah_core::TaskEvent::Thought {
            thought: "mid".into(),
            reasoning: None,
            ts: chrono::Utc::now(),
        },
    ];
    let mut vm = vm_with_events("fork", events, 2);
    vm.set_fork_tooltip(true);
    vm.set_fork_index(Some(1));

    let hits = render_hits(&vm, 80, 18);
    let fork_hit = hits
        .zones()
        .iter()
        .find(|z| matches!(z.action, AgentSessionMouseAction::ForkHere(_)))
        .expect("fork hit zone");

    if let AgentSessionMouseAction::ForkHere(idx) = fork_hit.action {
        assert_eq!(idx, 1);
    }
}

#[test]
fn fork_tooltip_renders_near_target_card() {
    let events = vec![
        ah_core::TaskEvent::Thought {
            thought: "top".into(),
            reasoning: None,
            ts: chrono::Utc::now(),
        },
        ah_core::TaskEvent::Thought {
            thought: "bottom".into(),
            reasoning: None,
            ts: chrono::Utc::now(),
        },
    ];
    let mut vm = vm_with_events("fork-tooltip", events, 2);
    vm.set_fork_tooltip(true);
    vm.set_fork_index(Some(0));

    let snapshot = render_snapshot("agent_session_hit_fork_tooltip", &vm, 80, 22);
    let lines: Vec<&str> = snapshot.lines().collect();
    let tooltip_row = lines
        .iter()
        .position(|line| line.contains("Click here to fork"))
        .expect("tooltip row");
    let first_card_row =
        lines.iter().position(|line| line.contains("THOUGHT")).expect("card header");
    assert!(
        tooltip_row <= first_card_row,
        "tooltip should sit above first visible card"
    );
}
