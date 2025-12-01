// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

mod support;

use ah_core::TaskEvent;
use ah_tui::theme::Theme;
use ah_tui::view_model::agent_session_model::OutputModalKind;
use ah_tui::view_model::agent_session_model::{
    AgentSessionMsg, AgentSessionViewModel, ControlAction, ControlFocus, FocusArea,
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn mk_key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::empty())
}

fn mk_key_mod(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers)
}

fn test_settings() -> ah_tui::settings::Settings {
    use ah_tui::settings::{KeyBinding, KeymapConfig};

    let mut keymap = KeymapConfig::default();
    let to_matcher = |s: &str| KeyBinding::from_string(s).unwrap().to_matcher().unwrap();
    keymap.move_to_next_field = Some(vec![to_matcher("Tab")]);
    keymap.move_to_previous_field = Some(vec![to_matcher("Shift+Tab")]);
    keymap.move_to_next_snapshot = Some(vec![to_matcher("Ctrl+Shift+Down")]);
    keymap.move_to_previous_snapshot = Some(vec![to_matcher("Ctrl+Shift+Up")]);
    keymap.scroll_up_one_screen = Some(vec![to_matcher("PageUp")]);
    keymap.scroll_down_one_screen = Some(vec![to_matcher("PageDown")]);
    keymap.move_to_end_of_document = Some(vec![to_matcher("End")]);
    keymap.move_to_beginning_of_document = Some(vec![to_matcher("Home")]);
    keymap.incremental_search_forward = Some(vec![to_matcher("/")]);
    keymap.incremental_search_backward = Some(vec![to_matcher("?")]);
    keymap.find_next = Some(vec![to_matcher("n")]);
    keymap.find_previous = Some(vec![to_matcher("Shift+N")]);
    keymap.dismiss_overlay = Some(vec![to_matcher("Esc")]);

    ah_tui::settings::Settings {
        keymap: Some(keymap),
        active_sessions_activity_rows: Some(1000),
        ..Default::default()
    }
}

fn vm_with_events_and_settings(
    title: &str,
    events: Vec<TaskEvent>,
    viewport_rows: usize,
) -> AgentSessionViewModel {
    let mut vm = AgentSessionViewModel::new(
        title.to_string(),
        Vec::new(),
        viewport_rows,
        test_settings(),
        None,
        Theme::default(),
    );
    for event in events {
        vm.handle_task_event(&event);
    }
    vm
}

#[test]
fn test_auto_follow_toggle() {
    let mut vm = vm_with_events_and_settings("test", vec![], 3);

    // Auto-follow should keep the newest rows in view.
    vm.handle_task_event(&TaskEvent::Thought {
        thought: "a".into(),
        reasoning: None,
        ts: chrono::Utc::now(),
    });
    vm.handle_task_event(&TaskEvent::Thought {
        thought: "b".into(),
        reasoning: None,
        ts: chrono::Utc::now(),
    });
    assert_eq!(vm.scroll(), 0);

    vm.handle_task_event(&TaskEvent::Thought {
        thought: "c".into(),
        reasoning: None,
        ts: chrono::Utc::now(),
    });
    assert_eq!(vm.scroll(), 0); // still auto-follow with small list

    vm.handle_task_event(&TaskEvent::Thought {
        thought: "d".into(),
        reasoning: None,
        ts: chrono::Utc::now(),
    });
    assert_eq!(vm.scroll(), 1); // followed to keep bottom visible

    // User scrolls up, auto-follow should disable.
    vm.handle_key_with_minor_modes(mk_key(KeyCode::Up));
    assert!(!vm.auto_follow());
    let before = vm.scroll();

    vm.handle_task_event(&TaskEvent::Thought {
        thought: "e".into(),
        reasoning: None,
        ts: chrono::Utc::now(),
    });
    assert_eq!(vm.scroll(), before); // no auto-follow while user scrolled up
}

#[test]
fn test_scroll_behavior() {
    let mut events = Vec::new();
    for i in 0..5 {
        events.push(TaskEvent::Thought {
            thought: format!("row-{i}"),
            reasoning: None,
            ts: chrono::Utc::now(),
        });
    }
    let mut vm = vm_with_events_and_settings("test", events, 2);

    // PageUp should move scroll and disable auto-follow
    vm.handle_key_with_minor_modes(mk_key(KeyCode::PageUp));
    assert!(!vm.auto_follow());
    assert!(vm.scroll() > 0);

    // Esc should signal quit via dismiss overlay binding
    let msg = vm.handle_key_with_minor_modes(mk_key(KeyCode::Esc));
    assert_eq!(msg, Some(AgentSessionMsg::QuitRequested));
}

#[test]
fn test_cycle_control_box() {
    let events = vec![
        TaskEvent::Thought {
            thought: "a".into(),
            reasoning: None,
            ts: chrono::Utc::now(),
        },
        TaskEvent::Thought {
            thought: "b".into(),
            reasoning: None,
            ts: chrono::Utc::now(),
        },
    ];
    let mut vm = vm_with_events_and_settings("test", events, 2);

    // Select bottom row
    vm.handle_key_with_minor_modes(mk_key(KeyCode::End));
    // Focus control box (Tab binding)
    vm.handle_key_with_minor_modes(mk_key(KeyCode::Tab));
    assert!(matches!(vm.focus(), FocusArea::Control(ControlFocus::Copy)));

    // Cycle to expand
    vm.handle_key_with_minor_modes(mk_key(KeyCode::Tab));
    assert!(matches!(
        vm.focus(),
        FocusArea::Control(ControlFocus::Expand)
    ));

    // Enter should emit activation
    let msg = vm.handle_key_with_minor_modes(mk_key(KeyCode::Enter));
    assert_eq!(
        msg,
        Some(AgentSessionMsg::ActivateControl {
            index: 1,
            action: ControlAction::Expand
        })
    );
}

#[test]
fn test_move_instruction_card() {
    let events = vec![
        TaskEvent::Thought {
            thought: "top".into(),
            reasoning: None,
            ts: chrono::Utc::now(),
        },
        TaskEvent::Thought {
            thought: "mid".into(),
            reasoning: None,
            ts: chrono::Utc::now(),
        },
        TaskEvent::Thought {
            thought: "bottom".into(),
            reasoning: None,
            ts: chrono::Utc::now(),
        },
    ];
    let mut vm = vm_with_events_and_settings("fork", events, 2);

    // Move fork down
    vm.handle_key_with_minor_modes(mk_key_mod(
        KeyCode::Down,
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    ));
    assert_eq!(vm.fork_index(), Some(1));

    // Move fork up and ensure it clamps at zero
    vm.handle_key_with_minor_modes(mk_key_mod(
        KeyCode::Up,
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    ));
    assert_eq!(vm.fork_index(), Some(0));
}

#[test]
fn test_navigate_cards_boundary() {
    let events = vec![
        TaskEvent::Thought {
            thought: "first".into(),
            reasoning: None,
            ts: chrono::Utc::now(),
        },
        TaskEvent::Thought {
            thought: "second".into(),
            reasoning: None,
            ts: chrono::Utc::now(),
        },
    ];
    let mut vm = vm_with_events_and_settings("bounds", events, 2);

    // Up at top should stay at zero
    vm.handle_key_with_minor_modes(mk_key(KeyCode::Home));
    vm.handle_key_with_minor_modes(mk_key(KeyCode::Up));
    assert_eq!(vm.selected(), Some(0));

    // Down past last should clamp
    vm.handle_key_with_minor_modes(mk_key(KeyCode::End));
    vm.handle_key_with_minor_modes(mk_key(KeyCode::Down));
    assert_eq!(vm.selected(), Some(1));
}

#[test]
fn test_leave_control_box() {
    let events = vec![TaskEvent::Thought {
        thought: "x".into(),
        reasoning: None,
        ts: chrono::Utc::now(),
    }];
    let mut vm = vm_with_events_and_settings("focus", events, 2);

    vm.handle_key_with_minor_modes(mk_key(KeyCode::End));
    vm.handle_key_with_minor_modes(mk_key(KeyCode::Tab));
    assert!(matches!(vm.focus(), FocusArea::Control(ControlFocus::Copy)));

    vm.handle_key_with_minor_modes(mk_key_mod(KeyCode::BackTab, KeyModifiers::SHIFT));
    assert!(matches!(vm.focus(), FocusArea::Timeline));
}

#[test]
fn test_navigate_cards_vertical() {
    let events = vec![
        TaskEvent::Thought {
            thought: "a".into(),
            reasoning: None,
            ts: chrono::Utc::now(),
        },
        TaskEvent::Thought {
            thought: "b".into(),
            reasoning: None,
            ts: chrono::Utc::now(),
        },
        TaskEvent::Thought {
            thought: "c".into(),
            reasoning: None,
            ts: chrono::Utc::now(),
        },
    ];
    let mut vm = vm_with_events_and_settings("vertical", events, 2);

    // First Up selects the prior row from the implicit last selection.
    vm.handle_key_with_minor_modes(mk_key(KeyCode::Up));
    assert_eq!(vm.selected(), Some(1));

    vm.handle_key_with_minor_modes(mk_key(KeyCode::Down));
    assert_eq!(vm.selected(), Some(2));
    assert!(vm.auto_follow());
}

#[test]
fn test_scroll_to_extremes() {
    let mut events = Vec::new();
    for i in 0..5 {
        events.push(TaskEvent::Thought {
            thought: format!("row-{i}"),
            reasoning: None,
            ts: chrono::Utc::now(),
        });
    }
    let mut vm = vm_with_events_and_settings("extremes", events, 3);

    vm.handle_key_with_minor_modes(mk_key(KeyCode::Home));
    assert_eq!(vm.selected(), Some(0));
    assert_eq!(vm.scroll(), 0);

    vm.handle_key_with_minor_modes(mk_key(KeyCode::End));
    assert_eq!(vm.selected(), Some(4));
    assert_eq!(vm.scroll(), 2); // enough to show last row in 3-line viewport
    assert!(vm.auto_follow());
}

#[test]
fn test_focus_control_box() {
    let events = vec![TaskEvent::Thought {
        thought: "only".into(),
        reasoning: None,
        ts: chrono::Utc::now(),
    }];
    let mut vm = vm_with_events_and_settings("focus-cycle", events, 2);

    // Timeline -> Copy
    vm.handle_key_with_minor_modes(mk_key(KeyCode::Tab));
    assert!(matches!(vm.focus(), FocusArea::Control(ControlFocus::Copy)));

    // Copy -> Expand
    vm.handle_key_with_minor_modes(mk_key(KeyCode::Tab));
    assert!(matches!(
        vm.focus(),
        FocusArea::Control(ControlFocus::Expand)
    ));

    // Expand -> Instructions
    vm.handle_key_with_minor_modes(mk_key(KeyCode::Tab));
    assert!(matches!(vm.focus(), FocusArea::Instructions));

    // Instructions -> Timeline (wrap)
    vm.handle_key_with_minor_modes(mk_key(KeyCode::Tab));
    assert!(matches!(vm.focus(), FocusArea::Timeline));
}

#[test]
fn test_draft_mode_entry() {
    let events = vec![TaskEvent::Thought {
        thought: "context".into(),
        reasoning: None,
        ts: chrono::Utc::now(),
    }];
    let mut vm = vm_with_events_and_settings("draft", events, 2);

    // Move focus into instructions card
    vm.handle_key_with_minor_modes(mk_key(KeyCode::Tab));
    vm.handle_key_with_minor_modes(mk_key(KeyCode::Tab));
    vm.handle_key_with_minor_modes(mk_key(KeyCode::Tab));
    assert!(matches!(vm.focus(), FocusArea::Instructions));

    // Typing should be routed to task entry (simulated by MoveToBeginningOfLine)
    let msg = vm.handle_key_with_minor_modes(mk_key(KeyCode::Home));
    assert_eq!(msg, Some(AgentSessionMsg::RedrawRequested));
}

#[test]
fn test_fork_point_selection() {
    let events = vec![
        TaskEvent::Thought {
            thought: "one".into(),
            reasoning: None,
            ts: chrono::Utc::now(),
        },
        TaskEvent::Thought {
            thought: "two".into(),
            reasoning: None,
            ts: chrono::Utc::now(),
        },
        TaskEvent::Thought {
            thought: "three".into(),
            reasoning: None,
            ts: chrono::Utc::now(),
        },
    ];
    let mut vm = vm_with_events_and_settings("forked", events, 2);

    // Move fork down twice
    vm.handle_key_with_minor_modes(mk_key_mod(
        KeyCode::Down,
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    ));
    vm.handle_key_with_minor_modes(mk_key_mod(
        KeyCode::Down,
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    ));
    assert_eq!(vm.fork_index(), Some(2));

    // Move back up
    vm.handle_key_with_minor_modes(mk_key_mod(
        KeyCode::Up,
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    ));
    assert_eq!(vm.fork_index(), Some(1));
}

#[test]
fn test_enter_search_mode() {
    let events = vec![
        TaskEvent::Thought {
            thought: "History one".into(),
            reasoning: None,
            ts: chrono::Utc::now(),
        },
        TaskEvent::Thought {
            thought: "Other".into(),
            reasoning: None,
            ts: chrono::Utc::now(),
        },
        TaskEvent::Thought {
            thought: "Second history".into(),
            reasoning: None,
            ts: chrono::Utc::now(),
        },
    ];
    let mut vm = vm_with_events_and_settings("search", events, 3);

    vm.set_search_query("history");
    let msg = vm.handle_key_with_minor_modes(mk_key(KeyCode::Char('/')));
    assert_eq!(msg, Some(AgentSessionMsg::RedrawRequested));
    assert_eq!(vm.selected(), Some(0));
    assert_eq!(vm.search_matches(), &[0, 2]);
}

#[test]
fn test_search_navigation() {
    let events = vec![
        TaskEvent::Thought {
            thought: "match one".into(),
            reasoning: None,
            ts: chrono::Utc::now(),
        },
        TaskEvent::Thought {
            thought: "nope".into(),
            reasoning: None,
            ts: chrono::Utc::now(),
        },
        TaskEvent::Thought {
            thought: "match two".into(),
            reasoning: None,
            ts: chrono::Utc::now(),
        },
    ];
    let mut vm = vm_with_events_and_settings("search-nav", events, 3);

    vm.set_search_query("match");
    vm.handle_key_with_minor_modes(mk_key(KeyCode::Char('/')));
    assert_eq!(vm.selected(), Some(0));

    vm.handle_key_with_minor_modes(mk_key(KeyCode::Char('n')));
    assert_eq!(vm.selected(), Some(2));

    vm.handle_key_with_minor_modes(mk_key_mod(KeyCode::Char('N'), KeyModifiers::SHIFT));
    assert_eq!(vm.selected(), Some(0));
}

#[test]
fn test_search_selection_moves_to_first_match() {
    let events = vec![
        TaskEvent::Thought {
            thought: "alpha match".into(),
            reasoning: None,
            ts: chrono::Utc::now(),
        },
        TaskEvent::Thought {
            thought: "beta other".into(),
            reasoning: None,
            ts: chrono::Utc::now(),
        },
        TaskEvent::Thought {
            thought: "alpha second match".into(),
            reasoning: None,
            ts: chrono::Utc::now(),
        },
    ];
    let mut vm = vm_with_events_and_settings("search-select", events, 3);

    // Pre-select a non-matching row to prove search moves selection to first hit.
    vm.handle_key_with_minor_modes(mk_key(KeyCode::End));
    assert_eq!(vm.selected(), Some(2));

    vm.set_search_query("alpha");
    let msg = vm.handle_key_with_minor_modes(mk_key(KeyCode::Char('/')));

    assert_eq!(msg, Some(AgentSessionMsg::RedrawRequested));
    assert_eq!(vm.search_matches(), &[0, 2]);
    assert_eq!(
        vm.selected(),
        Some(0),
        "selection should jump to first search hit"
    );
    assert!(
        !vm.auto_follow(),
        "search disables auto-follow while navigating results"
    );
}

#[test]
fn test_exit_search() {
    let events = vec![TaskEvent::Thought {
        thought: "search target".into(),
        reasoning: None,
        ts: chrono::Utc::now(),
    }];
    let mut vm = vm_with_events_and_settings("search-exit", events, 2);

    vm.set_search_query("search");
    vm.handle_key_with_minor_modes(mk_key(KeyCode::Char('/')));
    assert_eq!(vm.search_matches().len(), 1);

    let msg = vm.handle_key_with_minor_modes(mk_key(KeyCode::Esc));
    assert_eq!(msg, Some(AgentSessionMsg::RedrawRequested));
    assert!(vm.search_matches().is_empty());
}

#[test]
fn test_modal_open_and_close_via_escape() {
    let events = vec![TaskEvent::Thought {
        thought: "row".into(),
        reasoning: None,
        ts: chrono::Utc::now(),
    }];
    let mut vm = vm_with_events_and_settings("modal", events, 2);

    vm.open_output_modal(OutputModalKind::Text, "Log", "hello");
    assert!(vm.output_modal().is_some());

    let msg = vm.handle_key_with_minor_modes(mk_key(KeyCode::Esc));
    assert_eq!(msg, Some(AgentSessionMsg::RedrawRequested));
    assert!(vm.output_modal().is_none());
}

#[test]
fn test_modal_overlay_state_closes_before_quit() {
    let events = vec![TaskEvent::Thought {
        thought: "row".into(),
        reasoning: None,
        ts: chrono::Utc::now(),
    }];
    let mut vm = vm_with_events_and_settings("modal-state", events, 2);

    vm.open_output_modal(OutputModalKind::Text, "Log", "body");
    assert!(vm.output_modal().is_some(), "modal should open");

    // First ESC should close the modal, not request quit.
    let first = vm.handle_key_with_minor_modes(mk_key(KeyCode::Esc));
    assert_eq!(first, Some(AgentSessionMsg::RedrawRequested));
    assert!(vm.output_modal().is_none(), "modal should be closed");

    // Second ESC without modal should request quit.
    let second = vm.handle_key_with_minor_modes(mk_key(KeyCode::Esc));
    assert_eq!(second, Some(AgentSessionMsg::QuitRequested));
}

#[test]
fn test_open_output_modal_sets_overlay() {
    let events = vec![TaskEvent::Thought {
        thought: "context".into(),
        reasoning: None,
        ts: chrono::Utc::now(),
    }];
    let mut vm = vm_with_events_and_settings("modal-open", events, 2);

    vm.open_output_modal(OutputModalKind::Stderr, "Compile error", "failed");
    let modal = vm.output_modal().expect("modal should be stored");
    assert_eq!(modal.title, "Compile error");
    assert!(modal.body.contains("failed"));
}
