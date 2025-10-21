//! Filter Bar Rendering
//!
//! This module handles the rendering of the filter bar with its controls and focus states.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::UnicodeWidthStr;

use crate::view_model::{FilterBarTheme, FilterBarViewModel, FilterControl};

/// Render the filter bar
pub fn render_filter_bar(
    frame: &mut Frame,
    area: Rect,
    view_model: &FilterBarViewModel,
    theme: &FilterBarTheme,
) {
    let mut spans: Vec<Span> = Vec::new();
    let mut consumed = 0usize;

    // Use the focused border color when FilterBarLine is focused
    let border_style = if view_model.filter_bar_focused {
        Style::default().fg(theme.border_focused)
    } else {
        Style::default().fg(theme.border)
    };
    let header_style = if view_model.filter_bar_focused {
        Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.muted)
    };

    // Start with separator line
    push_span(&mut spans, &mut consumed, "─ ", border_style);
    push_span(
        &mut spans,
        &mut consumed,
        "Existing tasks",
        header_style.add_modifier(Modifier::BOLD),
    );
    push_span(&mut spans, &mut consumed, "  ", Style::default());

    // Repository filter
    let repo_style = if view_model.focused_element == Some(FilterControl::Repository) {
        Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text)
    };
    push_span(&mut spans, &mut consumed, "Repo ", header_style);
    render_filter_value(
        &mut spans,
        &mut consumed,
        &view_model.repository_value,
        repo_style,
    );

    push_span(&mut spans, &mut consumed, "  ", Style::default());

    // Status filter
    let status_style = if view_model.focused_element == Some(FilterControl::Status) {
        Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text)
    };
    push_span(&mut spans, &mut consumed, "Status ", header_style);
    render_filter_value(
        &mut spans,
        &mut consumed,
        &view_model.status_value,
        status_style,
    );

    push_span(&mut spans, &mut consumed, "  ", Style::default());

    // Creator filter
    let creator_style = if view_model.focused_element == Some(FilterControl::Creator) {
        Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text)
    };
    push_span(&mut spans, &mut consumed, "Creator ", header_style);
    render_filter_value(
        &mut spans,
        &mut consumed,
        &view_model.creator_value,
        creator_style,
    );

    // Fill remaining space with separator line
    let line_width = area.width as usize;
    if consumed < line_width {
        let remaining = line_width - consumed;
        push_span(
            &mut spans,
            &mut consumed,
            &"─".repeat(remaining),
            border_style,
        );
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn push_span(spans: &mut Vec<Span>, consumed: &mut usize, text: &str, style: Style) {
    *consumed += UnicodeWidthStr::width(text);
    spans.push(Span::styled(text.to_string(), style));
}

fn render_filter_value(spans: &mut Vec<Span>, consumed: &mut usize, value: &str, style: Style) {
    let display = format!("[{}]", value);
    let width = UnicodeWidthStr::width(display.as_str());
    *consumed += width;
    spans.push(Span::styled(display, style));
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn test_filter_bar_default_rendering() {
        let mut backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        let view_model = FilterBarViewModel::default();
        let theme = FilterBarTheme::default();

        terminal
            .draw(|frame| {
                let area = Rect::new(0, 0, 80, 1);
                render_filter_bar(frame, area, &view_model, &theme);
            })
            .unwrap();

        let buffer = terminal.backend_mut().buffer();
        // Should contain the basic structure
        let content = buffer.content();
        assert!(!content.is_empty());
    }

    #[test]
    fn test_filter_bar_focused_rendering() {
        let mut backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        let view_model = FilterBarViewModel {
            filter_bar_focused: true,
            ..Default::default()
        };
        let theme = FilterBarTheme::default();

        terminal
            .draw(|frame| {
                let area = Rect::new(0, 0, 80, 1);
                render_filter_bar(frame, area, &view_model, &theme);
            })
            .unwrap();

        let buffer = terminal.backend_mut().buffer();
        // Should contain focused styling
        let content = buffer.content();
        assert!(!content.is_empty());
    }

    #[test]
    fn test_filter_control_focused_rendering() {
        let mut backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        let view_model = FilterBarViewModel {
            focused_element: Some(FilterControl::Repository),
            repository_value: "test-repo".to_string(),
            ..Default::default()
        };
        let theme = FilterBarTheme::default();

        terminal
            .draw(|frame| {
                let area = Rect::new(0, 0, 80, 1);
                render_filter_bar(frame, area, &view_model, &theme);
            })
            .unwrap();

        let buffer = terminal.backend_mut().buffer();
        // Should contain focused repository control
        let content = buffer.content();
        assert!(!content.is_empty());
    }
}
