// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Session Viewer View Layer
//!
//! This module contains the Ratatui rendering functions for the session viewer
//! interface. It follows the same separation principles as the dashboard view,
//! keeping all presentation logic isolated from event handling and state
//! updates.

use crate::view::draft_card::render_draft_card;
use crate::view::{HitTestRegistry, Theme};
use crate::view_model::TaskEntryViewModel;
use crate::view_model::session_viewer_model::{
    GutterConfig, GutterPosition, SearchState, SessionViewerFocusState, SessionViewerMode,
    SessionViewerMouseAction, SessionViewerViewModel, TerminalOutputSpan,
};
use ah_recorder::{LineIndex, ScreenLineIndex, TerminalState};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
// tracing::debug not currently used in this module
use vt100;

/// Convert ANSI bytes to Ratatui spans using manual ANSI parsing
fn ansi_bytes_to_spans(bytes: &[u8]) -> Vec<Span<'static>> {
    if bytes.is_empty() {
        return vec![Span::raw("")];
    }

    // Convert the raw bytes to owned string (lossy is fine for arbitrary bytes)
    let s = String::from_utf8_lossy(bytes).into_owned();

    // Use the existing manual ANSI parsing
    ansi_content_to_spans(&s)
}

/// Convert VT100 ANSI content to Ratatui spans with proper styling
fn ansi_content_to_spans(content: &str) -> Vec<Span<'static>> {
    if content.is_empty() {
        return vec![Span::raw(String::new())];
    }

    // Parse the ANSI content to recreate VT100 styling
    let mut parser = vt100::Parser::new(1, 1000, 0); // 1 row, wide enough for content
    parser.process(content.as_bytes());

    let screen = parser.screen();
    let mut spans = Vec::new();
    let mut current_style = Style::default();
    let mut current_text = String::new();

    // Process each cell in the first row
    let (_rows, cols) = screen.size();
    for col in 0..cols {
        if let Some(cell) = screen.cell(0, col) {
            let ch = match cell.contents().chars().next() {
                Some(c) if !c.is_control() => c,
                _ => ' ',
            };

            // Build the style for this cell
            let mut cell_style = Style::default();

            // Foreground / background colors
            let fg = cell.fgcolor();
            cell_style = cell_style.fg(map_vt100_color(fg));

            let bg = cell.bgcolor();
            cell_style = cell_style.bg(map_vt100_color(bg));

            // Text modifiers
            if cell.bold() {
                cell_style = cell_style.add_modifier(Modifier::BOLD);
            }
            if cell.italic() {
                cell_style = cell_style.add_modifier(Modifier::ITALIC);
            }
            if cell.underline() {
                cell_style = cell_style.add_modifier(Modifier::UNDERLINED);
            }
            if cell.inverse() {
                cell_style = cell_style.add_modifier(Modifier::REVERSED);
            }

            // If style changed, flush current text as a span
            if cell_style != current_style && !current_text.is_empty() {
                spans.push(Span::styled(current_text, current_style));
                current_text = String::new();
            }

            current_style = cell_style;
            current_text.push(ch);

            // Handle wide characters
            let (_rows, cols) = screen.size();
            if cell.is_wide() && col + 1 < cols {
                // Skip the next cell as it's part of this wide character
                // The VT100 parser should handle this properly
            }
        }
    }

    // Flush remaining text
    if !current_text.is_empty() {
        spans.push(Span::styled(current_text, current_style));
    }

    if spans.is_empty() {
        vec![Span::raw(String::new())]
    } else {
        spans
    }
}

fn map_vt100_color(c: vt100::Color) -> Color {
    match c {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(i) => Color::Indexed(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

/// Render the session viewer interface
pub fn render_session_viewer(
    frame: &mut Frame<'_>,
    view_model: &mut SessionViewerViewModel,
    hit_registry: &mut HitTestRegistry<SessionViewerMouseAction>,
    theme: &Theme,
) {
    let area = frame.area();
    // Paint the full viewport with the app background so no terminal pixels bleed through.
    frame.render_widget(Block::default().style(Style::default().bg(theme.bg)), area);

    let status_height = 1u16;

    // Check if task entry should be positioned inline (at a specific line)
    let inline_task_entry =
        view_model.task_entry_visible && view_model.current_snapshot_index.is_some();

    if inline_task_entry {
        // Inline positioning: task entry is inserted between terminal lines
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(status_height)])
            .split(area);

        let (terminal_area, status_area) = (chunks[0], chunks[1]);

        render_terminal_content(frame, terminal_area, view_model, hit_registry, theme);

        if let Some(search) = &view_model.search_state {
            render_search_overlay(frame, search, theme);
        }

        render_status_bar(frame, status_area, view_model, theme);
    } else {
        // Bottom positioning: task entry at bottom (legacy behavior or replay mode)
        let overlay_height = if view_model.task_entry_visible {
            Some(view_model.task_entry.full_height())
        } else {
            None
        };
        let has_overlay = overlay_height.is_some();

        let chunks = if let Some(overlay) = overlay_height {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(1),
                    Constraint::Length(overlay),
                    Constraint::Length(status_height),
                ])
                .split(area)
        } else {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(status_height)])
                .split(area)
        };

        let (terminal_area, draft_card_area, status_area) = if has_overlay {
            (chunks[0], Some(chunks[1]), chunks[2])
        } else {
            (chunks[0], None, chunks[1])
        };

        render_terminal_content(frame, terminal_area, view_model, hit_registry, theme);

        if let Some(task_entry_area) = draft_card_area {
            if view_model.task_entry_visible {
                frame.render_widget(Clear, task_entry_area);
                render_instruction_entry(frame, task_entry_area, &view_model.task_entry, theme);
            }
        }

        if let Some(search) = &view_model.search_state {
            render_search_overlay(frame, search, theme);
        }

        render_status_bar(frame, status_area, view_model, theme);
    }
}

fn render_instruction_entry(
    frame: &mut Frame<'_>,
    area: Rect,
    task_entry: &TaskEntryViewModel,
    theme: &Theme,
) {
    let layout = render_draft_card(frame, area, task_entry, theme, true);

    // Position cursor in the textarea
    let (cursor_row, cursor_col) = task_entry.description.cursor();
    let inner_rect = task_entry
        .description
        .block()
        .map(|block| block.inner(layout.textarea))
        .unwrap_or(layout.textarea);
    let cursor_x = inner_rect.x + cursor_col as u16;
    let cursor_y = inner_rect.y + cursor_row as u16;
    frame.set_cursor_position((cursor_x, cursor_y));
}

/// Helper function to render a span of terminal lines with optional snapshot suppression
/// Renders all lines within the given TerminalOutputSpan, handling both gutter and terminal content
/// If suppress_snapshot_line is provided, the snapshot indicator will be suppressed for that line
/// If focus_element is Terminal, positions the cursor on the line where the terminal cursor is located
#[allow(clippy::too_many_arguments)] // High-arity kept: rendering needs parallel data slices & context objects; future refactor may introduce a struct param.
fn render_terminal_lines_span_with_snapshot_suppression(
    frame: &mut Frame<'_>,
    span: &TerminalOutputSpan,
    terminal_area: Rect,
    gutter_area: Option<Rect>,
    start_row: u16,
    gutter_config: &GutterConfig,
    recording_state: &TerminalState,
    suppress_snapshot_line: Option<LineIndex>,
    focus_element: SessionViewerFocusState,
    theme: &Theme,
) -> u16 {
    let mut current_row = start_row;
    let mut current_line_idx = span.first_line.as_usize();
    let parser = recording_state.parser();
    let (cursor_row, cursor_col) = parser.screen().cursor_position();
    let cursor_line_index =
        recording_state.get_visible_line_absolute_index(ScreenLineIndex(cursor_row as usize));

    for _ in 0..span.len() {
        // Calculate terminal chunk for this line
        let terminal_chunk = Rect {
            x: terminal_area.x,
            y: terminal_area.y + current_row,
            width: terminal_area.width,
            height: 1,
        };

        // Calculate gutter chunk for this line
        let gutter_chunk = gutter_area.map(|area| Rect {
            x: area.x,
            y: area.y + current_row,
            width: area.width,
            height: 1,
        });

        // Render gutter entry
        if let Some(gutter_chunk) = gutter_chunk {
            let suppress_snapshot = suppress_snapshot_line
                .map(|line_idx| line_idx == LineIndex(current_line_idx))
                .unwrap_or(false);

            render_gutter_item_for_line_with_suppression(
                frame,
                gutter_chunk,
                LineIndex(current_line_idx),
                gutter_config,
                recording_state,
                suppress_snapshot,
                theme,
            );
        }

        // Check if we should position the cursor on this line (only when terminal is focused)
        if matches!(focus_element, SessionViewerFocusState::Terminal)
            && cursor_line_index == LineIndex(current_line_idx)
        {
            // Cursor is on this line - position it
            let cursor_x = terminal_chunk.x + cursor_col;
            let cursor_y = terminal_chunk.y;
            frame.set_cursor_position((cursor_x, cursor_y));
        }

        // Render terminal content
        let line_content_formatted = recording_state
            .line_content_by_line_index_formatted(LineIndex(current_line_idx))
            .unwrap_or_default();

        // Convert ANSI bytes to spans using manual ANSI parsing
        let spans = ansi_bytes_to_spans(&line_content_formatted);
        let line = Line::from(spans);

        frame.render_widget(line, terminal_chunk);

        current_row += 1;
        current_line_idx += 1;
    }

    current_row
}

fn render_terminal_content(
    frame: &mut Frame<'_>,
    area: Rect,
    view_model: &mut SessionViewerViewModel,
    _hit_registry: &mut HitTestRegistry<SessionViewerMouseAction>,
    theme: &Theme,
) {
    let recorded_cols = view_model.display_cols();
    let recorded_rows = view_model.display_rows();
    let gutter_width = view_model.gutter_config.width();

    let show_frame = matches!(view_model.session_mode, SessionViewerMode::SessionReview)
        && (recorded_cols != area.width.saturating_sub(2 + gutter_width as u16)
            || recorded_rows != area.height.saturating_sub(2));

    // Apply frame around entire terminal area if needed
    let content_area = if show_frame {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!("Terminal ({}x{})", recorded_cols, recorded_rows));
        let inner_area = block.inner(area);
        frame.render_widget(block, area);
        inner_area
    } else {
        area
    };

    // Get display structure from view model (includes scrolling logic)
    let display_structure = view_model.get_display_structure();

    let (gutter_area, terminal_area) = match view_model.gutter_config.position {
        GutterPosition::Left => {
            let gutter_rect = Rect {
                x: content_area.x,
                y: content_area.y,
                width: gutter_width as u16,
                height: content_area.height,
            };
            let terminal_rect = Rect {
                x: content_area.x + gutter_width as u16,
                y: content_area.y,
                width: content_area.width - gutter_width as u16,
                height: content_area.height,
            };
            (Some(gutter_rect), terminal_rect)
        }
        GutterPosition::Right => {
            let terminal_rect = Rect {
                x: content_area.x,
                y: content_area.y,
                width: content_area.width - gutter_width as u16,
                height: content_area.height,
            };
            let gutter_rect = Rect {
                x: content_area.x + content_area.width - gutter_width as u16,
                y: content_area.y,
                width: gutter_width as u16,
                height: content_area.height,
            };
            (Some(gutter_rect), terminal_rect)
        }
        GutterPosition::None => (None, content_area),
    };

    // Render content starting from row 0
    let mut current_row = 0;
    let temp_focus_element_fix = if display_structure.task_entry_height == 0 {
        SessionViewerFocusState::Terminal
    } else {
        SessionViewerFocusState::TaskEntry
    };

    // Render before_task_entry lines
    current_row = render_terminal_lines_span_with_snapshot_suppression(
        frame,
        &display_structure.before_task_entry,
        terminal_area,
        gutter_area,
        current_row,
        &view_model.gutter_config,
        &view_model.recording_terminal_state.borrow(),
        None, // No snapshot suppression for before_task_entry
        temp_focus_element_fix,
        theme,
    );

    // Render task entry if visible
    if display_structure.task_entry_height > 0 {
        let task_entry_height = display_structure.task_entry_height as u16;

        // Calculate terminal chunk for task entry
        let terminal_chunk = Rect {
            x: terminal_area.x,
            y: terminal_area.y + current_row,
            width: terminal_area.width,
            height: task_entry_height,
        };

        // Calculate gutter chunk for task entry
        let gutter_chunk = gutter_area.map(|area| Rect {
            x: area.x,
            y: area.y + current_row,
            width: area.width,
            height: task_entry_height,
        });

        // Render gutter entry for task entry
        if let Some(gutter_chunk) = gutter_chunk {
            // Get the line index for the task entry if it's positioned at a snapshot
            let task_entry_line_idx = view_model
                .current_snapshot_index
                .map(|idx| view_model.recording_terminal_state.borrow().snapshot_line_index(idx));

            render_gutter_item_for_task_entry(
                frame,
                gutter_chunk,
                &view_model.gutter_config,
                &view_model.recording_terminal_state.borrow(),
                task_entry_line_idx,
                theme,
            );
        }

        // Render task entry
        frame.render_widget(Clear, terminal_chunk);
        render_instruction_entry(frame, terminal_chunk, &view_model.task_entry, theme);

        current_row += task_entry_height;
    }

    // Render after_task_entry lines
    // Suppress snapshot indicator on the first line after task entry since it's shown on the task entry itself
    let suppress_snapshot_line = view_model
        .current_snapshot_index
        .map(|idx| view_model.recording_terminal_state.borrow().snapshot_line_index(idx));

    let _ = render_terminal_lines_span_with_snapshot_suppression(
        frame,
        &display_structure.after_task_entry,
        terminal_area,
        gutter_area,
        current_row,
        &view_model.gutter_config,
        &view_model.recording_terminal_state.borrow(),
        suppress_snapshot_line,
        temp_focus_element_fix,
        theme,
    );
}

/// Render gutter content for a terminal line with optional snapshot suppression
///
/// The gutter displays snapshot indicators for each terminal line. We query the terminal
/// state by absolute line number to determine if a gutter indicator (like snapshot marker)
/// should be displayed at a particular line. This ensures snapshot positioning remains consistent
/// and meaningful regardless of scrolling.
fn render_gutter_item_for_line_with_suppression(
    frame: &mut Frame<'_>,
    area: Rect,
    line_idx: LineIndex,
    _gutter_config: &GutterConfig,
    recording_state: &TerminalState,
    suppress_snapshot: bool,
    theme: &Theme,
) {
    let has_snapshot = recording_state.has_snapshot_at_line(line_idx) && !suppress_snapshot;
    let mut spans = Vec::new();

    // Left space or snapshot indicator
    let snapshot_char = if has_snapshot { "▶" } else { " " };
    let style = if has_snapshot {
        Style::default().fg(theme.warning)
    } else {
        Style::default()
    };
    spans.push(Span::styled(snapshot_char, style));

    // Right space
    spans.push(Span::raw(" "));

    let line = Line::from(spans);
    let gutter_widget = Paragraph::new(line)
        .style(Style::default().bg(theme.surface))
        .wrap(Wrap { trim: false });

    frame.render_widget(gutter_widget, area);
}

/// Render gutter content for a terminal line
///
/// The gutter displays snapshot indicators for each terminal line. We query the terminal
/// state by absolute line number to determine if a gutter indicator (like snapshot marker)
/// should be displayed at a particular line. This ensures snapshot positioning remains consistent
/// and meaningful regardless of scrolling.
#[allow(dead_code)] // Simple wrapper retained for potential gutter customization without suppression flag.
fn render_gutter_item_for_line(
    frame: &mut Frame<'_>,
    area: Rect,
    line_idx: LineIndex,
    _gutter_config: &GutterConfig,
    recording_state: &TerminalState,
    theme: &Theme,
) {
    render_gutter_item_for_line_with_suppression(
        frame,
        area,
        line_idx,
        _gutter_config,
        recording_state,
        false,
        theme,
    );
}

/// Render gutter content for the task entry
fn render_gutter_item_for_task_entry(
    frame: &mut Frame<'_>,
    area: Rect,
    _gutter_config: &GutterConfig,
    recording_state: &TerminalState,
    task_entry_line_idx: Option<LineIndex>,
    theme: &Theme,
) {
    let has_snapshot = task_entry_line_idx
        .map(|line_idx| recording_state.has_snapshot_at_line(line_idx))
        .unwrap_or(false);
    let mut spans = Vec::new();

    // Left space or snapshot indicator
    let snapshot_char = if has_snapshot { "▶" } else { " " };
    let style = if has_snapshot {
        Style::default().fg(theme.warning)
    } else {
        Style::default()
    };
    spans.push(Span::styled(snapshot_char, style));

    // Right space
    spans.push(Span::raw(" "));

    let line = Line::from(spans);
    let gutter_widget = Paragraph::new(line)
        .style(Style::default().bg(theme.surface))
        .wrap(Wrap { trim: false });

    frame.render_widget(gutter_widget, area);
}

fn render_search_overlay(frame: &mut Frame<'_>, search: &SearchState, theme: &Theme) {
    let area = frame.area();
    let overlay_area = Rect {
        x: 0,
        y: area.height - 1,
        width: area.width,
        height: 1,
    };

    let search_text = format!("/{}", search.query);
    let search_widget =
        Paragraph::new(search_text).style(Style::default().bg(theme.primary).fg(theme.bg));

    frame.render_widget(search_widget, overlay_area);
}

fn render_status_bar(
    frame: &mut Frame<'_>,
    area: Rect,
    view_model: &SessionViewerViewModel,
    theme: &Theme,
) {
    let mut footer_area = area;
    if area.width >= 4 {
        let horizontal_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(2), Constraint::Min(1)])
            .split(area);
        footer_area = horizontal_chunks[1];
    }

    let mut spans: Vec<Span> = Vec::new();
    let bullet = " • ";

    let first_visible = view_model.scroll_offset.as_usize();
    let viewport_height = view_model.display_rows() as usize;
    let last_visible = (first_visible + viewport_height).saturating_sub(1);
    let total_rows = view_model.total_rows();

    // Use 1-based indexing for display (show last visible line number)
    let display_last_visible = last_visible.min(total_rows.saturating_sub(1)) + 1;
    let display_total = total_rows;

    spans.push(Span::styled(
        format!("Scroll: {}/{}", display_last_visible, display_total),
        Style::default().fg(theme.text),
    ));

    spans.push(Span::styled(
        bullet.to_string(),
        Style::default().fg(theme.border),
    ));

    let recording_state = view_model.recording_terminal_state.borrow();
    spans.push(Span::styled(
        format!("Snapshots: {}", recording_state.all_snapshots().len()),
        Style::default().fg(theme.primary),
    ));
    if let Some(last_snapshot) = recording_state.all_snapshots().last() {
        spans.push(Span::styled(
            bullet.to_string(),
            Style::default().fg(theme.border),
        ));
        spans.push(Span::styled(
            format!("Last snapshot at line {}", last_snapshot.line + 1),
            Style::default().fg(theme.warning),
        ));
    }
    drop(recording_state);

    spans.push(Span::styled(
        bullet.to_string(),
        Style::default().fg(theme.border),
    ));

    let mode = if view_model.task_entry_visible {
        ("EDITING", theme.warning)
    } else if view_model.search_state.is_some() {
        ("SEARCH", theme.accent)
    } else {
        ("NORMAL", theme.text)
    };

    spans.push(Span::styled(mode.0, Style::default().fg(mode.1)));

    if view_model.exit_confirmation_armed {
        spans.push(Span::styled(
            " • Please ESC again to exit".to_string(),
            Style::default().fg(theme.error),
        ));
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(theme.bg)),
        footer_area,
    );
}
