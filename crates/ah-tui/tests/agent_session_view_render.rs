// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

mod support;
use ah_tui::theme::Theme;
use ah_tui::view_model::task_execution::AgentActivityRow;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use support::{
    STANDARD_THEMES, STANDARD_VIEWPORTS, apply_keys, find_text, key, key_with, render_buffer,
    render_snapshot_with_theme, style_at,
};

fn make_settings() -> insta::Settings {
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_suffix("");
    settings.set_snapshot_path("{snapshot}.snap.svg");
    settings
}

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

#[test]
fn renders_basic_cards_snapshot() {
    let _guard = make_settings().bind_to_scope();

    let events = vec![
        ah_core::TaskEvent::Thought {
            thought: "Thinking about fix".into(),
            reasoning: None,
            ts: chrono::Utc::now(),
        },
        ah_core::TaskEvent::ToolUse {
            tool_name: "runCmd".into(),
            tool_execution_id: "abc".into(),
            tool_args: "{}".into(),
            status: ah_domain_types::task::ToolStatus::Started,
            ts: chrono::Utc::now(),
        },
        ah_core::TaskEvent::Log {
            message: "running tests".into(),
            tool_execution_id: Some("abc".into()),
            level: ah_domain_types::task::LogLevel::Info,
            ts: chrono::Utc::now(),
        },
    ];

    let vm = support::vm_with_events("test-session", events, 5);

    assert_snapshots_all_sizes_and_themes("agent_activity_view_basic", &vm);
}

#[test]
fn renders_hero_and_timeline_separation() {
    let _guard = make_settings().bind_to_scope();

    let events = vec![
        ah_core::TaskEvent::Thought {
            thought: "Earlier thought".into(),
            reasoning: None,
            ts: chrono::Utc::now(),
        },
        ah_core::TaskEvent::FileEdit {
            file_path: "src/lib.rs".into(),
            lines_added: 10,
            lines_removed: 2,
            description: Some("Refactor module".into()),
            ts: chrono::Utc::now(),
        },
        ah_core::TaskEvent::ToolUse {
            tool_name: "cargo test".into(),
            tool_execution_id: "t1".into(),
            tool_args: "{}".into(),
            status: ah_domain_types::task::ToolStatus::Started,
            ts: chrono::Utc::now(),
        },
        ah_core::TaskEvent::ToolResult {
            tool_name: "cargo test".into(),
            tool_execution_id: "t1".into(),
            tool_output: "ok".into(),
            status: ah_domain_types::task::ToolStatus::Completed,
            ts: chrono::Utc::now(),
        },
    ];

    let vm = support::vm_with_events("hero-session", events, 6);

    assert_snapshots_all_sizes_and_themes("agent_activity_view_hero", &vm);
}

#[test]
fn renders_dimmed_when_scrolled_up() {
    let _guard = make_settings().bind_to_scope();

    let events = vec![
        ah_core::TaskEvent::Thought {
            thought: "Top".into(),
            reasoning: None,
            ts: chrono::Utc::now(),
        },
        ah_core::TaskEvent::Thought {
            thought: "Middle".into(),
            reasoning: None,
            ts: chrono::Utc::now(),
        },
        ah_core::TaskEvent::Thought {
            thought: "Bottom".into(),
            reasoning: None,
            ts: chrono::Utc::now(),
        },
    ];

    let mut vm = support::vm_with_events("dim-session", events, 2);
    apply_keys(&mut vm, &[key(KeyCode::PageUp)]); // scroll up via keyboard to disable auto-follow

    assert_snapshots_all_sizes_and_themes("agent_activity_view_dimmed", &vm);
}

#[test]
fn test_render_empty_state() {
    let _guard = make_settings().bind_to_scope();
    let vm = support::vm_with_events("empty-session", vec![], 4);
    assert_snapshots_all_sizes_and_themes("agent_activity_view_empty", &vm);
}

#[test]
fn test_render_card_selected() {
    let _guard = make_settings().bind_to_scope();

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

    let mut vm = support::vm_with_events("selected-session", events, 4);
    apply_keys(
        &mut vm,
        &[key_with(KeyCode::Home, KeyModifiers::CONTROL)], // move to beginning/select first card
    );
    assert_snapshots_all_sizes_and_themes("agent_activity_view_selected", &vm);
}

#[test]
fn test_render_control_box_focused() {
    let _guard = make_settings().bind_to_scope();

    let events = vec![
        ah_core::TaskEvent::ToolUse {
            tool_name: "cargo test".into(),
            tool_execution_id: "t1".into(),
            tool_args: "{}".into(),
            status: ah_domain_types::task::ToolStatus::Started,
            ts: chrono::Utc::now(),
        },
        ah_core::TaskEvent::Log {
            message: "running".into(),
            tool_execution_id: Some("t1".into()),
            level: ah_domain_types::task::LogLevel::Info,
            ts: chrono::Utc::now(),
        },
    ];

    let mut vm = support::vm_with_events("focus-session", events, 4);
    apply_keys(
        &mut vm,
        &[
            key_with(KeyCode::End, KeyModifiers::CONTROL),
            key(KeyCode::Tab),
        ],
    );
    assert_snapshots_all_sizes_and_themes("agent_activity_view_control_focus", &vm);
}

#[test]
fn test_render_control_box_expand_focused() {
    let _guard = make_settings().bind_to_scope();

    let events = vec![
        ah_core::TaskEvent::ToolUse {
            tool_name: "cargo build".into(),
            tool_execution_id: "build1".into(),
            tool_args: "{}".into(),
            status: ah_domain_types::task::ToolStatus::Started,
            ts: chrono::Utc::now(),
        },
        ah_core::TaskEvent::Log {
            message: "Compiling...".into(),
            tool_execution_id: Some("build1".into()),
            level: ah_domain_types::task::LogLevel::Info,
            ts: chrono::Utc::now(),
        },
    ];

    let mut vm = support::vm_with_events("expand-focus-session", events, 4);
    apply_keys(
        &mut vm,
        &[
            key_with(KeyCode::End, KeyModifiers::CONTROL),
            key(KeyCode::Tab),
            key(KeyCode::Tab),
        ],
    );
    assert_snapshots_all_sizes_and_themes("agent_activity_view_control_expand_focus", &vm);
}

#[test]
fn test_render_thought_markdown() {
    let _guard = make_settings().bind_to_scope();
    // Test markdown rendering in thought cards
    let events = vec![
        ah_core::TaskEvent::Thought {
            thought: "**Analysis**: I've identified the issue in the code. The problem is with the null pointer dereference in `src/main.rs` at line 42. Here's what I found:\n\n- The `user_data` pointer can be NULL\n- No null check before dereference\n- This causes a segfault when user data is missing\n\n```rust\nif user_data.is_null() {\n    return Err(\"No user data\");\n}\n```\n\n**Solution**: Add proper null checking before accessing the user data structure.".into(),
            reasoning: None,
            ts: chrono::Utc::now(),
        },
    ];

    let vm = support::vm_with_events("markdown-thought-session", events, 4);

    assert_snapshots_all_sizes_and_themes("agent_activity_view_thought_markdown", &vm);
}

#[test]
fn test_render_user_multiline() {
    let _guard = make_settings().bind_to_scope();
    // Test multiline user message
    let events = vec![
        ah_core::TaskEvent::UserInput {
            author: "you".into(),
            content: "I've been working on this Rust project and I'm running into some issues with async code.\n\nThe problem is that my futures are not being polled correctly, and I'm getting errors about futures never being completed.\n\nCan you help me debug this async issue?\n\nHere's a code snippet:\n\n```rust\nasync fn process_data(data: Vec<u8>) -> Result<String, Error> {\n    // Some async processing\n    Ok(\"processed\".to_string())\n}\n```\n\nThanks!".into(),
            ts: chrono::Utc::now(),
        },
    ];

    let vm = support::vm_with_events("multiline-user-session", events, 4);

    assert_snapshots_all_sizes_and_themes("agent_activity_view_user_multiline", &vm);
}

#[test]
fn test_render_command_stop_button() {
    let _guard = make_settings().bind_to_scope();
    // Test stop button on running command
    let events = vec![
        ah_core::TaskEvent::ToolUse {
            tool_name: "cargo build --release".into(),
            tool_execution_id: "build1".into(),
            tool_args: "{}".into(),
            status: ah_domain_types::task::ToolStatus::Started,
            ts: chrono::Utc::now(),
        },
        ah_core::TaskEvent::Log {
            message: "Compiling... 50% complete".into(),
            tool_execution_id: Some("build1".into()),
            level: ah_domain_types::task::LogLevel::Info,
            ts: chrono::Utc::now(),
        },
    ];

    let vm = support::vm_with_events("stop-button-session", events, 4);

    assert_snapshots_all_sizes_and_themes("agent_activity_view_command_stop_button", &vm);
}

#[test]
fn test_render_output_size_indicator() {
    let _guard = make_settings().bind_to_scope();
    // Test output size indicator display
    use ah_tui::view_model::task_execution::{PipelineMeta, PipelineSegment, PipelineStatus};

    let pipeline = PipelineMeta {
        segments: vec![PipelineSegment::new(
            Some(PipelineStatus::Success),
            Some("2.3KB".into()),
        )],
    };

    let mut vm = support::vm_with_events("output-size-session", vec![], 4);

    // Manually push row with pipeline since TaskEvent doesn't support it yet
    vm.push_row(AgentActivityRow::ToolUse {
        tool_name: "cat large_file.txt".into(),
        tool_execution_id: "cat1".into(),
        last_line: Some("File displayed successfully".into()),
        completed: true,
        status: ah_domain_types::task::ToolStatus::Completed,
        pipeline: Some(pipeline),
    });

    assert_snapshots_all_sizes_and_themes("agent_activity_view_output_size_indicator", &vm);
}

#[test]
fn test_render_mixed_card_sequence() {
    let _guard = make_settings().bind_to_scope();

    let events = vec![
        ah_core::TaskEvent::UserInput {
            author: "you".into(),
            content: "Please help me fix this bug".into(),
            ts: chrono::Utc::now(),
        },
        ah_core::TaskEvent::Thought {
            thought: "The user is asking for help with a bug. I need to understand what they're trying to do and identify the issue.".into(),
            reasoning: None,
            ts: chrono::Utc::now(),
        },
        ah_core::TaskEvent::ToolUse {
            tool_name: "grep".into(),
            tool_execution_id: "search1".into(),
            tool_args: "{}".into(),
            status: ah_domain_types::task::ToolStatus::Started,
            ts: chrono::Utc::now(),
        },
        ah_core::TaskEvent::ToolResult {
            tool_name: "grep".into(),
            tool_execution_id: "search1".into(),
            tool_output: "Found 3 matches in src/main.rs".into(),
            status: ah_domain_types::task::ToolStatus::Completed,
            ts: chrono::Utc::now(),
        },
    ];

    let mut vm = support::vm_with_events("mixed-cards", events, 6);

    // Manually push rows for unsupported events
    vm.push_row(AgentActivityRow::AgentRead {
        file_path: "src/main.rs".into(),
        range: Some("10-20".into()),
    });

    vm.push_row(AgentActivityRow::AgentEdit {
        file_path: "src/main.rs".into(),
        lines_added: 5,
        lines_removed: 2,
        description: Some("Fixed the null pointer issue".into()),
    });

    vm.push_row(AgentActivityRow::AgentDeleted {
        file_path: "old_file.rs".into(),
        lines_removed: 150,
    });

    assert_snapshots_all_sizes_and_themes("agent_activity_view_mixed_sequence", &vm);
}

#[test]
fn test_render_viewport_overflow() {
    let _guard = make_settings().bind_to_scope();
    // Create many cards to trigger scrolling
    let mut events = Vec::new();
    for i in 0..15 {
        events.push(ah_core::TaskEvent::Thought {
            thought: format!(
                "This is card number {} with some content to make it longer",
                i
            ),
            reasoning: None,
            ts: chrono::Utc::now(),
        });
    }

    let vm = support::vm_with_events("overflow-session", events, 8);

    assert_snapshots_all_sizes_and_themes("agent_activity_view_viewport_overflow", &vm);
}

#[test]
fn test_render_hero_thinking() {
    let _guard = make_settings().bind_to_scope();
    // Hero card in thinking state - this should show when the last activity is active thinking
    let events = vec![
        ah_core::TaskEvent::UserInput {
            author: "you".into(),
            content: "Help me debug this issue".into(),
            ts: chrono::Utc::now(),
        },
        ah_core::TaskEvent::Thought {
            thought: "I need to analyze the code to understand the issue. Let me look at the relevant files first.".into(),
            reasoning: None,
            ts: chrono::Utc::now(),
        },
    ];

    let vm = support::vm_with_events("thinking-session", events, 4);

    assert_snapshots_all_sizes_and_themes("agent_activity_view_hero_thinking", &vm);
}

#[test]
fn test_render_hero_tool_running() {
    let _guard = make_settings().bind_to_scope();
    // Hero card showing active tool execution
    let events = vec![
        ah_core::TaskEvent::UserInput {
            author: "you".into(),
            content: "Run the tests".into(),
            ts: chrono::Utc::now(),
        },
        ah_core::TaskEvent::Thought {
            thought: "The user wants to run tests. I should execute the test command.".into(),
            reasoning: None,
            ts: chrono::Utc::now(),
        },
        ah_core::TaskEvent::ToolUse {
            tool_name: "cargo test".into(),
            tool_execution_id: "test1".into(),
            tool_args: "{}".into(),
            status: ah_domain_types::task::ToolStatus::Started,
            ts: chrono::Utc::now(),
        },
        ah_core::TaskEvent::Log {
            message: "Compiling...".into(),
            tool_execution_id: Some("test1".into()),
            level: ah_domain_types::task::LogLevel::Info,
            ts: chrono::Utc::now(),
        },
    ];

    let vm = support::vm_with_events("tool-running-session", events, 4);

    assert_snapshots_all_sizes_and_themes("agent_activity_view_hero_tool_running", &vm);
}

#[test]
fn test_render_pipeline_success() {
    let _guard = make_settings().bind_to_scope();
    use ah_tui::view_model::task_execution::{PipelineMeta, PipelineSegment, PipelineStatus};

    let pipeline = PipelineMeta {
        segments: vec![
            PipelineSegment::new(Some(PipelineStatus::Success), Some("1.2KB".into())),
            PipelineSegment::new(Some(PipelineStatus::Success), Some("850B".into())),
        ],
    };

    let events = vec![ah_core::TaskEvent::UserInput {
        author: "you".into(),
        content: "Process the data and sort it".into(),
        ts: chrono::Utc::now(),
    }];

    let mut vm = support::vm_with_events("pipeline-session", events, 4);

    vm.push_row(AgentActivityRow::ToolUse {
        tool_name: "cat data.txt | grep important | sort".into(),
        tool_execution_id: "pipe1".into(),
        last_line: Some("Processing complete".into()),
        completed: true,
        status: ah_domain_types::task::ToolStatus::Completed,
        pipeline: Some(pipeline),
    });

    assert_snapshots_all_sizes_and_themes("agent_activity_view_pipeline_success", &vm);
}

#[test]
fn test_render_pipeline_partial_failure() {
    let _guard = make_settings().bind_to_scope();
    use ah_tui::view_model::task_execution::{PipelineMeta, PipelineSegment, PipelineStatus};

    let pipeline = PipelineMeta {
        segments: vec![
            PipelineSegment::new(Some(PipelineStatus::Success), Some("1.0KB".into())),
            PipelineSegment::new(Some(PipelineStatus::Failed), None),
            PipelineSegment::new(Some(PipelineStatus::Skipped), Some("0B".into())),
        ],
    };

    let mut vm = support::vm_with_events("pipeline-partial-session", vec![], 4);

    vm.push_row(AgentActivityRow::ToolUse {
        tool_name: "cat data.txt | grep foo | sort".into(),
        tool_execution_id: "pipe2".into(),
        last_line: Some("grep: pattern not found".into()),
        completed: true,
        status: ah_domain_types::task::ToolStatus::Failed,
        pipeline: Some(pipeline),
    });

    assert_snapshots_all_sizes_and_themes("agent_activity_view_pipeline_partial_failure", &vm);
}

#[test]
fn test_render_command_wrapping() {
    let _guard = make_settings().bind_to_scope();

    let events = vec![
        ah_core::TaskEvent::ToolUse {
            tool_name: "find . -type f -name '*.rs' -print0 | xargs -0 -n1 sed -n '1,120p'".into(),
            tool_execution_id: "wrap1".into(),
            tool_args: "{}".into(),
            status: ah_domain_types::task::ToolStatus::Started,
            ts: chrono::Utc::now(),
        },
        ah_core::TaskEvent::ToolResult {
            tool_name: "find . -type f -name '*.rs' -print0 | xargs -0 -n1 sed -n '1,120p'".into(),
            tool_execution_id: "wrap1".into(),
            tool_output: "processed 42 files".into(),
            status: ah_domain_types::task::ToolStatus::Completed,
            ts: chrono::Utc::now(),
        },
    ];

    let vm = support::vm_with_events("wrap-session", events, 4);

    assert_snapshots_all_sizes_and_themes("agent_activity_view_command_wrapping", &vm);
}

#[test]
fn test_render_collaborative_user() {
    let _guard = make_settings().bind_to_scope();

    let events = vec![ah_core::TaskEvent::UserInput {
        author: "alice".into(),
        content: "Please sync with my changes and re-run tests.".into(),
        ts: chrono::Utc::now(),
    }];

    let vm = support::vm_with_events("collab-session", events, 6);

    assert_snapshots_all_sizes_and_themes("agent_activity_view_collaborative_user", &vm);
}

#[test]
fn test_render_hero_pinned_scrolled() {
    let _guard = make_settings().bind_to_scope();
    let mut events = Vec::new();
    for i in 0..8 {
        events.push(ah_core::TaskEvent::Thought {
            thought: format!("History {}", i),
            reasoning: None,
            ts: chrono::Utc::now(),
        });
    }
    events.push(ah_core::TaskEvent::ToolUse {
        tool_name: "cargo test".into(),
        tool_execution_id: "hero".into(),
        tool_args: "{}".into(),
        status: ah_domain_types::task::ToolStatus::Started,
        ts: chrono::Utc::now(),
    });

    let mut vm = support::vm_with_events("hero-pinned", events, 4);
    apply_keys(&mut vm, &[key(KeyCode::Up), key(KeyCode::Up)]); // scroll via keyboard to disable auto-follow

    assert_snapshots_all_sizes_and_themes("agent_activity_view_hero_pinned_scrolled", &vm);
}

#[test]
fn test_background_fills_entire_area() {
    let theme = Theme::default();
    let events = vec![ah_core::TaskEvent::Thought {
        thought: "just a row".into(),
        reasoning: None,
        ts: chrono::Utc::now(),
    }];
    let vm = support::vm_with_events("bg-fill", events, 3);
    let buffer = render_buffer(&vm, 40, 12, &theme);
    let mut base_seen = false;
    for y in 0..buffer.area().height {
        for x in 0..buffer.area().width {
            let style = style_at(&buffer, x, y);
            assert!(
                style.bg.is_some(),
                "cell ({},{}) should be painted (no transparent bg)",
                x,
                y
            );
            if style.bg == Some(theme.bg) {
                base_seen = true;
            }
        }
    }
    assert!(base_seen, "base color should be present in the frame");
}

#[test]
fn test_render_footer_context_critical() {
    let theme = Theme::default();
    let mut vm = support::vm_with_events("footer-critical", vec![], 3);
    vm.set_context_percent(96);
    let buffer = render_buffer(&vm, 80, 12, &theme);
    let (rx, ry) = find_text(&buffer, "Context: 96%").expect("critical context text visible");
    let style = style_at(&buffer, rx, ry);
    assert_eq!(style.fg, Some(theme.error));
}

#[test]
fn test_render_hero_below_fork() {
    let theme = Theme::default();
    let events = vec![
        ah_core::TaskEvent::Thought {
            thought: "History".into(),
            reasoning: None,
            ts: chrono::Utc::now(),
        },
        ah_core::TaskEvent::Thought {
            thought: "More".into(),
            reasoning: None,
            ts: chrono::Utc::now(),
        },
        ah_core::TaskEvent::ToolUse {
            tool_name: "cargo test".into(),
            tool_execution_id: "hero".into(),
            tool_args: "{}".into(),
            status: ah_domain_types::task::ToolStatus::Started,
            ts: chrono::Utc::now(),
        },
    ];
    let mut vm = support::vm_with_events("hero-fork", events, 4);
    vm.set_fork_index(Some(2));
    vm.set_fork_tooltip(true);

    let buffer = render_buffer(&vm, 80, 24, &theme);
    let (_, tooltip_y) = find_text(&buffer, "Click here to fork").expect("tooltip present");
    let (_, hero_y) = find_text(&buffer, "cargo test").expect("hero text present");
    assert!(
        hero_y > tooltip_y,
        "hero card should render below the fork tooltip"
    );
}

#[test]
fn test_render_instructions_card_moved_up_when_space_is_tight() {
    let theme = Theme::default();
    let events = vec![ah_core::TaskEvent::Thought {
        thought: "long history".into(),
        reasoning: None,
        ts: chrono::Utc::now(),
    }];
    let vm = support::vm_with_events("instr-move", events, 3);

    let buffer_tall = render_buffer(&vm, 80, 26, &theme);
    let (_, y_tall) = find_text(&buffer_tall, "MODELS").expect("instructions present");

    let buffer_short = render_buffer(&vm, 80, 14, &theme);
    let (_, y_short) =
        find_text(&buffer_short, "MODELS").expect("instructions present even when compressed");

    assert!(
        y_short < y_tall,
        "instructions card should move up when vertical space shrinks"
    );
}

#[test]
fn test_render_output_modal_text() {
    let theme = Theme::default();
    let events = vec![ah_core::TaskEvent::Thought {
        thought: "row".into(),
        reasoning: None,
        ts: chrono::Utc::now(),
    }];
    let mut vm = support::vm_with_events("modal-text", events, 3);
    vm.open_output_modal(
        ah_tui::view_model::agent_session_model::OutputModalKind::Text,
        "Log",
        "First line\nSecond line",
    );
    let buffer = render_buffer(&vm, 80, 20, &theme);
    let (hx, hy) = find_text(&buffer, "OUTPUT").expect("modal header present");
    let header_style = style_at(&buffer, hx, hy);
    assert_eq!(header_style.fg, Some(theme.primary));
    let scrim_style = style_at(&buffer, 0, 0);
    assert_eq!(scrim_style.bg, Some(theme.bg));
}

#[test]
fn test_render_output_modal_stderr() {
    let theme = Theme::default();
    let events = vec![ah_core::TaskEvent::Thought {
        thought: "row".into(),
        reasoning: None,
        ts: chrono::Utc::now(),
    }];
    let mut vm = support::vm_with_events("modal-stderr", events, 3);
    vm.open_output_modal(
        ah_tui::view_model::agent_session_model::OutputModalKind::Stderr,
        "Error log",
        "failed to compile",
    );
    let buffer = render_buffer(&vm, 80, 20, &theme);
    let (hx, hy) = find_text(&buffer, "STDERR").expect("stderr header present");
    let header_style = style_at(&buffer, hx, hy);
    assert_eq!(header_style.bg, Some(theme.bg));
}

#[test]
fn test_render_output_modal_binary() {
    let theme = Theme::default();
    let events = vec![ah_core::TaskEvent::Thought {
        thought: "row".into(),
        reasoning: None,
        ts: chrono::Utc::now(),
    }];
    let mut vm = support::vm_with_events("modal-bin", events, 3);
    vm.open_output_modal(
        ah_tui::view_model::agent_session_model::OutputModalKind::Binary,
        "Binary",
        "00 ff ee dd",
    );
    let buffer = render_buffer(&vm, 80, 20, &theme);
    let (bx, by) = find_text(&buffer, "BINARY").expect("binary header present");
    let style = style_at(&buffer, bx, by);
    assert_eq!(style.fg, Some(theme.primary));
}

#[test]
fn edit_diff_lines_use_semantic_colors() {
    let theme = Theme::default();
    let events = vec![ah_core::TaskEvent::FileEdit {
        file_path: "src/main.rs".into(),
        lines_added: 3,
        lines_removed: 1,
        description: Some("+ added\n- removed".into()),
        ts: chrono::Utc::now(),
    }];
    let vm = support::vm_with_events("edit-diff", events, 5);
    let buffer = render_buffer(&vm, 80, 20, &theme);
    let (ax, ay) = find_text(&buffer, "+ added").expect("added line visible");
    let add_style = style_at(&buffer, ax, ay);
    assert_eq!(
        add_style.fg,
        Some(theme.accent),
        "added lines should use accent color"
    );

    let (dx, dy) = find_text(&buffer, "- removed").expect("deleted line visible");
    let del_style = style_at(&buffer, dx, dy);
    assert_eq!(
        del_style.fg,
        Some(theme.error),
        "removed lines should use error color"
    );
}

#[test]
fn fork_tooltip_matches_theme_colors() {
    let theme = Theme::default();
    let events = vec![ah_core::TaskEvent::Thought {
        thought: "top".into(),
        reasoning: None,
        ts: chrono::Utc::now(),
    }];
    let mut vm = support::vm_with_events("fork-style", events, 3);
    vm.set_fork_index(Some(0));
    vm.set_fork_tooltip(true);

    let buffer = render_buffer(&vm, 80, 18, &theme);
    let (x, y) = find_text(&buffer, "Click here to fork").expect("tooltip visible");
    let style = style_at(&buffer, x, y);
    assert_eq!(style.fg, Some(theme.tooltip_text));
    assert_eq!(style.bg, Some(theme.bg));
}

#[test]
fn hero_pinned_above_instructions() {
    let theme = Theme::default();
    let events = vec![
        ah_core::TaskEvent::Thought {
            thought: "history".into(),
            reasoning: None,
            ts: chrono::Utc::now(),
        },
        ah_core::TaskEvent::ToolUse {
            tool_name: "cargo test".into(),
            tool_execution_id: "hero".into(),
            tool_args: "{}".into(),
            status: ah_domain_types::task::ToolStatus::Started,
            ts: chrono::Utc::now(),
        },
    ];
    let vm = support::vm_with_events("hero-instructions", events, 4);
    let buffer = render_buffer(&vm, 80, 20, &theme);
    let (_, hero_y) = find_text(&buffer, "$ cargo test").expect("hero command visible");
    let (_, instr_y) = find_text(&buffer, "MODELS").expect("instructions visible");
    assert!(
        hero_y < instr_y,
        "hero card should render above the instructions card"
    );
}
