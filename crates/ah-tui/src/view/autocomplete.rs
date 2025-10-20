use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState},
};
use tui_textarea::TextArea;
use unicode_segmentation::UnicodeSegmentation as _;

use crate::view::Theme;
use crate::view_model::autocomplete::{
    InlineAutocomplete, ScoredMatch, Trigger, MAX_MENU_HEIGHT, MENU_WIDTH,
};

#[derive(Debug, Clone, Copy)]
pub struct CaretMetrics {
    pub caret_x: u16,
    pub caret_y: u16,
    pub popup_x: u16,
    pub popup_y: u16,
}

pub fn render_autocomplete(
    autocomplete: &InlineAutocomplete,
    frame: &mut Frame<'_>,
    textarea_area: Rect,
    textarea: &TextArea<'_>,
    theme: &Theme,
    background: Color,
) {
    let Some(menu_state) = autocomplete.menu_state() else {
        return;
    };

    let caret = compute_caret_metrics(textarea, textarea_area);
    let screen = frame.size();
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

    let items: Vec<ListItem> = visible_results.iter().map(|m| make_list_item(m, theme)).collect();

    if items.is_empty() {
        return;
    }

    let mut block = Block::default().style(Style::default().bg(background));
    if menu_state.show_border {
        block = block
            .title(Span::styled(
                format!("{} suggestions", menu_state.trigger.display_label()),
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

fn compute_caret_metrics(textarea: &TextArea<'_>, area: Rect) -> CaretMetrics {
    let (cursor_row, cursor_col) = textarea.cursor();
    let (top_row, left_col) = textarea.viewport_origin();
    let gutter = textarea.gutter_width();

    let mut visible_row = cursor_row.saturating_sub(top_row as usize);
    let mut visible_col = cursor_col.saturating_sub(left_col as usize);

    if textarea.word_wrap() {
        let available_width = area.width.saturating_sub(gutter);
        let content_width = available_width.max(1) as usize;

        if content_width > 0 {
            let start_row = top_row as usize;
            let mut additional_rows = 0usize;

            for line_idx in start_row..cursor_row {
                let width = textarea.display_width_of_line(line_idx);
                if width > 0 {
                    let wraps = (width + content_width - 1) / content_width;
                    if wraps > 0 {
                        additional_rows = additional_rows.saturating_add(wraps.saturating_sub(1));
                    }
                }
            }

            let cursor_width = textarea.display_width_until(cursor_row, cursor_col);
            let wraps = cursor_width / content_width;
            visible_col = cursor_width % content_width;
            if cursor_width > 0 && visible_col == 0 {
                visible_col = 0;
            }
            additional_rows = additional_rows.saturating_add(wraps);
            visible_row = cursor_row.saturating_sub(start_row) + additional_rows;
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
        match_.item.trigger,
        theme,
    );

    let mut spans = Vec::new();
    spans.push(trigger_span);
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
    trigger: Trigger,
    theme: &Theme,
) -> (Span<'static>, Vec<Span<'static>>) {
    let trigger_char = trigger.as_char();
    let trigger_span = Span::styled(trigger_char.to_string(), theme.muted_style());

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
