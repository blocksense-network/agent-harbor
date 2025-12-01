// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use std::{cell::RefCell, rc::Rc, sync::Arc};

use ah_recorder::TerminalState;
use ah_rest_mock_client::MockRestClient;
use ah_tui::{
    agent_session_loop::build_agent_activity_view_model,
    session_viewer_deps::{AgentSessionDependencies, AgentSessionUiMode},
    theme::Theme,
    view_model::session_viewer_model::{GutterConfig, GutterPosition},
    view_model::task_execution::AgentActivityRow,
    viewer::ViewerConfig,
};

#[test]
fn agent_activity_dependencies_capture_activity_entries() {
    let recording = Rc::new(RefCell::new(TerminalState::new(24, 80)));
    let viewer_config = ViewerConfig {
        terminal_cols: 80,
        terminal_rows: 16,
        scrollback: 1000,
        gutter: GutterConfig {
            position: GutterPosition::Left,
            show_line_numbers: false,
        },
        is_replay_mode: false,
    };
    let activity = vec![
        (
            0,
            AgentActivityRow::AgentThought {
                thought: "a".into(),
            },
        ),
        (
            5,
            AgentActivityRow::AgentThought {
                thought: "b".into(),
            },
        ),
    ];

    let deps = AgentSessionDependencies::agent_activity(
        recording,
        viewer_config.clone(),
        Arc::new(MockRestClient::new()),
        ah_tui::settings::Settings::default(),
        Theme::default(),
        ah_tui::terminal::TerminalConfig::minimal(),
        activity.clone(),
    );

    assert_eq!(deps.ui_mode, AgentSessionUiMode::AgentActivity);
    assert_eq!(deps.activity_entries.len(), activity.len());
    assert_eq!(deps.viewer_config.terminal_rows, 16);
}

#[test]
fn build_activity_view_model_respects_viewport_rows() {
    let viewer_config = ViewerConfig {
        terminal_cols: 100,
        terminal_rows: 18,
        scrollback: 2000,
        gutter: GutterConfig {
            position: GutterPosition::Left,
            show_line_numbers: true,
        },
        is_replay_mode: false,
    };
    let vm = build_agent_activity_view_model(
        &viewer_config,
        ah_tui::settings::Settings::default(),
        None,
        Theme::default(),
    );
    assert_eq!(
        vm.viewport_rows(),
        viewer_config.terminal_rows.saturating_sub(4) as usize
    );
}
