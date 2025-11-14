// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph},
};
use tui_textarea::TextArea;
use unicode_segmentation::UnicodeSegmentation as _;
use unicode_width::UnicodeWidthStr;

use crate::view::{HitTestRegistry, Theme};
use crate::view_model::MouseAction;
use crate::view_model::autocomplete::{
    AutocompleteMenuState, GhostState, MAX_MENU_HEIGHT, MENU_WIDTH, MenuContext, ScoredMatch,
};

#[derive(Debug, Clone, Copy)]
pub struct CaretMetrics {
    pub caret_x: u16,
    pub caret_y: u16,
    pub popup_x: u16,
    pub popup_y: u16,
}

pub fn render_autocomplete(
    menu_state: Option<AutocompleteMenuState<'_>>,
    frame: &mut Frame<'_>,
    textarea_area: Rect,
    textarea: &TextArea<'_>,
    theme: &Theme,
    background: Color,
    hit_registry: &mut HitTestRegistry<MouseAction>,
) {
    let Some(menu_state) = menu_state else {
        return;
    };

    let caret = compute_caret_metrics(textarea, textarea_area);
    let screen = frame.area();
    let menu_width = MENU_WIDTH.min(screen.width);

    let total_results = menu_state.results.len();
    if total_results == 0 {
        return;
    }

    let visible_capacity = MAX_MENU_HEIGHT as usize;
    let selected = menu_state.selected_index.min(total_results - 1);

    let start = if total_results <= visible_capacity {
        0
    } else {
        let half = visible_capacity / 2;
        let max_start = total_results - visible_capacity;
        let mut proposed = selected.saturating_sub(half);
        if proposed > max_start {
            proposed = max_start;
        }
        proposed
    };

    let end = if total_results <= visible_capacity {
        total_results
    } else {
        (start + visible_capacity).min(total_results)
    };

    let visible_results = &menu_state.results[start..end];
    let menu_height = visible_results.len().max(1) as u16;

    let popup_y = if caret.popup_y + menu_height <= screen.height {
        caret.popup_y
    } else if caret.caret_y >= menu_height {
        caret.caret_y.saturating_sub(menu_height)
    } else {
        0
    };

    let popup = clip_popup(screen, caret.popup_x, popup_y, menu_width, menu_height);
    if popup.y + popup.height > screen.height || popup.x + popup.width > screen.width {
        return;
    }

    frame.render_widget(Clear, popup);

    for (offset, _) in visible_results.iter().enumerate() {
        let global_index = start + offset;
        let item_rect = Rect {
            x: popup.x,
            y: popup.y + offset as u16,
            width: popup.width,
            height: 1,
        };
        hit_registry.register(item_rect, MouseAction::AutocompleteSelect(global_index));
    }

    let items: Vec<ListItem> = visible_results.iter().map(|m| make_list_item(m, theme)).collect();

    if items.is_empty() {
        return;
    }

    let mut block = Block::default().style(Style::default().bg(background));
    if menu_state.show_border {
        block = block
            .title(Span::styled(
                format!("{} suggestions", menu_state.context.title()),
                Style::default().fg(theme.primary).add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border_focused));
    }

    let mut state = ListState::default();
    state.select(Some(selected.saturating_sub(start)));

    let list = List::new(items).block(block).highlight_style(
        Style::default().bg(background).fg(theme.primary).add_modifier(Modifier::BOLD),
    );

    frame.render_stateful_widget(list, popup, &mut state);
}

pub fn render_autocomplete_ghost(
    frame: &mut Frame<'_>,
    textarea_area: Rect,
    textarea: &TextArea<'_>,
    ghost: &GhostState,
    theme: &Theme,
) {
    if ghost.shared_extension().is_empty() && ghost.completion_extension().is_empty() {
        return;
    }

    let start_col = ghost.start_col() + ghost.typed_len();
    let metrics = compute_position_for(textarea, textarea_area, ghost.row(), start_col);

    if metrics.caret_y < textarea_area.y
        || metrics.caret_y >= textarea_area.y + textarea_area.height
    {
        return;
    }

    let line_end_x = textarea_area.x.saturating_add(textarea_area.width);
    if metrics.caret_x >= line_end_x {
        return;
    }

    let available_width = line_end_x.saturating_sub(metrics.caret_x);
    if available_width == 0 {
        return;
    }

    let mut remaining_width = available_width;
    let shared_segment = truncate_by_width(ghost.shared_extension(), remaining_width);
    let shared_width = UnicodeWidthStr::width(shared_segment.as_str()) as u16;
    remaining_width = remaining_width.saturating_sub(shared_width);

    let extra_segment = if remaining_width > 0 {
        truncate_by_width(ghost.extra_completion(), remaining_width)
    } else {
        String::new()
    };

    if shared_segment.is_empty() && extra_segment.is_empty() {
        return;
    }

    let ghost_rect = Rect {
        x: metrics.caret_x,
        y: metrics.caret_y,
        width: available_width,
        height: 1,
    };

    let shared_style = Style::default().fg(theme.muted).add_modifier(Modifier::DIM);
    let extra_style = Style::default().fg(theme.text).add_modifier(Modifier::DIM);

    let mut spans = Vec::new();
    if !shared_segment.is_empty() {
        spans.push(Span::styled(shared_segment, shared_style));
    }
    if !extra_segment.is_empty() {
        spans.push(Span::styled(extra_segment, extra_style));
    }

    let paragraph = Paragraph::new(Line::from(spans)).style(Style::default().bg(theme.bg));
    frame.render_widget(paragraph, ghost_rect);
}

fn compute_caret_metrics(textarea: &TextArea<'_>, area: Rect) -> CaretMetrics {
    let (cursor_row, cursor_col) = textarea.cursor();
    compute_position_for(textarea, area, cursor_row, cursor_col)
}

fn compute_position_for(
    textarea: &TextArea<'_>,
    area: Rect,
    target_row: usize,
    target_col: usize,
) -> CaretMetrics {
    let (top_row, left_col) = textarea.viewport_origin();
    let gutter = textarea.gutter_width();

    let mut visible_row = target_row.saturating_sub(top_row as usize);
    let mut visible_col = target_col.saturating_sub(left_col as usize);

    if textarea.word_wrap() {
        let available_width = area.width.saturating_sub(gutter);
        let content_width = available_width.max(1) as usize;

        if content_width > 0 {
            let start_row = top_row as usize;
            let mut additional_rows = 0usize;

            for line_idx in start_row..target_row {
                let width = textarea.display_width_of_line(line_idx);
                if width > 0 {
                    let wraps = (width + content_width - 1) / content_width;
                    if wraps > 0 {
                        additional_rows = additional_rows.saturating_add(wraps.saturating_sub(1));
                    }
                }
            }

            let cursor_width = textarea.display_width_until(target_row, target_col);
            let wraps = cursor_width / content_width;
            visible_col = cursor_width % content_width;
            if cursor_width > 0 && visible_col == 0 {
                visible_col = 0;
            }
            additional_rows = additional_rows.saturating_add(wraps);
            visible_row = target_row.saturating_sub(start_row) + additional_rows;
        } else {
            visible_col = 0;
        }
    }

    let text_start_x = area.x.saturating_add(gutter as u16);
    let max_x = area.x.saturating_add(area.width.saturating_sub(1));
    let max_y = area.y.saturating_add(area.height.saturating_sub(1));

    let caret_x = text_start_x.saturating_add(visible_col as u16).min(max_x);
    let caret_y = area.y.saturating_add(visible_row as u16).min(max_y);

    let popup_x = caret_x.saturating_add(1).min(max_x);
    let popup_y = caret_y.saturating_add(1).min(max_y);

    CaretMetrics {
        caret_x,
        caret_y,
        popup_x,
        popup_y,
    }
}

fn truncate_by_width(text: &str, max_width: u16) -> String {
    if max_width == 0 {
        return String::new();
    }

    let mut remaining = max_width as usize;
    let mut result = String::new();

    for grapheme in text.graphemes(true) {
        let width = UnicodeWidthStr::width(grapheme);
        if width == 0 {
            continue;
        }
        if width > remaining {
            break;
        }
        result.push_str(grapheme);
        remaining = remaining.saturating_sub(width);
        if remaining == 0 {
            break;
        }
    }

    result
}

fn clip_popup(area: Rect, x: u16, y: u16, w: u16, h: u16) -> Rect {
    let clamped_x = x.min(area.x + area.width.saturating_sub(1));
    let clamped_y = y.min(area.y + area.height.saturating_sub(1));

    let available_width = area.x + area.width - clamped_x;
    let available_height = area.y + area.height - clamped_y;

    Rect {
        x: clamped_x,
        y: clamped_y,
        width: w.min(available_width),
        height: h.min(available_height),
    }
}

fn make_list_item(match_: &ScoredMatch, theme: &Theme) -> ListItem<'static> {
    let display = match_.item.label.clone();
    let (trigger_span, label_spans) = styled_label(
        &display,
        match_.indices.as_slice(),
        match_.item.context,
        theme,
    );

    let mut spans = Vec::new();
    if let Some(trigger_span) = trigger_span {
        spans.push(trigger_span);
        if !display.is_empty() {
            spans.push(Span::raw(" "));
        }
    }
    if !display.is_empty() {
        spans.extend(label_spans);
    }

    if let Some(detail) = &match_.item.detail {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(detail.clone(), theme.muted_style()));
    }

    ListItem::new(Line::from(spans))
}

fn styled_label(
    display: &str,
    indices: &[usize],
    context: MenuContext,
    theme: &Theme,
) -> (Option<Span<'static>>, Vec<Span<'static>>) {
    let trigger_span = context
        .leading_symbol()
        .map(|ch| Span::styled(ch.to_string(), theme.muted_style()));

    let mut highlight_indices = indices.to_vec();
    highlight_indices.sort_unstable();
    highlight_indices.dedup();

    let mut spans = Vec::new();
    let mut current = String::new();
    let mut current_style = Style::default().fg(theme.text);
    let mut highlighted = false;

    for (idx, grapheme) in display.graphemes(true).enumerate() {
        let is_highlight = highlight_indices.binary_search(&idx).is_ok();
        if is_highlight != highlighted && !current.is_empty() {
            spans.push(Span::styled(current.clone(), current_style));
            current.clear();
        }

        highlighted = is_highlight;
        current_style = if highlighted {
            Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text)
        };
        current.push_str(grapheme);
    }

    if !current.is_empty() {
        spans.push(Span::styled(current, current_style));
    }

    (trigger_span, spans)
}
