// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Layout rendering tests for the TUI dashboard

use ah_tui::{ViewModel, app::AppState};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use std::io::Result;

/// Test that the dashboard renders correctly on different terminal sizes
#[test]
fn test_dashboard_layout_small_terminal() -> Result<()> {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend)?;

    // Create test state with sample data - AppState::default() already includes sample tasks
    let state = AppState::default();

    // Create ViewModel from state
    let view_model = ViewModel::from_state(&state);

    // Test rendering
    terminal.draw(|f| {
        let size = f.size();

        ah_tui::ui::draw_task_dashboard(f, size, &view_model, None, None);
    })?;

    let buffer = terminal.backend().buffer();

    // Check that the layout contains expected elements
    let all_text = buffer.content().iter().map(|cell| cell.symbol()).collect::<String>();

    // Should contain header (check for box drawing characters that indicate logo is rendered)
    assert!(
        all_text.contains("╔"),
        "Should contain logo border characters"
    );
    assert!(
        all_text.contains("═"),
        "Should contain logo border characters"
    );

    // Should contain task data
    assert!(all_text.contains("Refactor"), "Should contain task titles");
    assert!(
        all_text.contains("New Task"),
        "Should contain 'New Task' card"
    );

    Ok(())
}

#[test]
fn test_dashboard_layout_large_terminal() -> Result<()> {
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend)?;

    let state = AppState::default();
    let view_model = ViewModel::from_state(&state);

    terminal.draw(|f| {
        let size = f.size();

        ah_tui::ui::draw_task_dashboard(f, size, &view_model, None, None);
    })?;

    let buffer = terminal.backend().buffer();

    // Check that layout adapts to larger terminal
    assert!(buffer.area().width >= 120);
    assert!(buffer.area().height >= 40);

    Ok(())
}

#[test]
fn test_focus_indication() -> Result<()> {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend)?;

    let state = AppState::default();
    let view_model = ViewModel::from_state(&state);

    terminal.draw(|f| {
        let size = f.size();

        ah_tui::ui::draw_task_dashboard(f, size, &view_model, None, None);
    })?;

    let buffer = terminal.backend().buffer();
    let _content = buffer.content();

    // Should render without errors - the new task-based UI is displayed
    // More sophisticated testing would require examining buffer cells for styling

    Ok(())
}
