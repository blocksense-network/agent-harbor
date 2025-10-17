//! Renders draft task cards with textarea and control buttons.

use ratatui::{
    prelude::*,
    widgets::*,
};

use super::Theme;
use crate::view_model::{FocusElement, TaskEntryViewModel};

/// Render a draft card (exact same as main.rs TaskCard::render with state == Draft)
pub fn render_draft_card(frame: &mut Frame<'_>, area: Rect, card: &TaskEntryViewModel, theme: &Theme, is_selected: bool) {
    // Draft cards have outer border with "New Task" title
    let border_style = if is_selected {
        Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.border)
    };

    let title_style = if is_selected {
        Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.border).add_modifier(Modifier::BOLD)
    };

    let border_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .title("┤ New Task ├")
        .title_alignment(ratatui::layout::Alignment::Left)
        .title_style(title_style);

    let inner_area = border_block.inner(area);
    frame.render_widget(border_block, area);
    render_draft_card_content(frame, inner_area, card, theme);
}

/// Render draft card content (exact same as main.rs TaskCard::render_draft_card_content)
pub fn render_draft_card_content(frame: &mut Frame<'_>, area: Rect, card: &TaskEntryViewModel, theme: &Theme) {
    let content_height = area.height as usize;

    // Split the available area between textarea and buttons (exact same as main.rs)
    let button_height: usize = 1; // Single line for buttons
    let separator_height: usize = 1; // Empty line between
    let padding_total = 2; // TEXTAREA_TOP_PADDING + TEXTAREA_BOTTOM_PADDING
    let available_content = content_height.saturating_sub(button_height + separator_height);
    let available_inner = available_content.saturating_sub(padding_total).max(1);
    let desired_lines = card.description.lines().len().max(5); // MIN_TEXTAREA_VISIBLE_LINES = 5
    let visible_lines = desired_lines.min(available_inner).max(1);

    let textarea_inner_height = visible_lines as u16;
    let textarea_total_height = (visible_lines + padding_total) as u16;

    // Add configurable left padding for textarea and buttons
    let textarea_area = Rect {
        x: area.x + 1, // TEXTAREA_LEFT_PADDING
        y: area.y + 1, // TEXTAREA_TOP_PADDING
        width: area.width.saturating_sub(2), // Left + right padding
        height: textarea_inner_height,
    };

    let button_area = Rect {
        x: area.x, // BUTTON_LEFT_PADDING = 0
        y: area.y + textarea_total_height + separator_height as u16,
        width: area.width,
        height: button_height as u16,
    };

    // Render padding areas around textarea
    let padding_style = Style::default().bg(theme.bg);

    // Top padding
    let top_padding_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: 1, // TEXTAREA_TOP_PADDING
    };
    frame.render_widget(Paragraph::new("").style(padding_style), top_padding_area);

    // Bottom padding
    let bottom_padding_area = Rect {
        x: area.x,
        y: area.y + 1 + textarea_inner_height, // After top padding + textarea
        width: area.width,
        height: 1, // TEXTAREA_BOTTOM_PADDING
    };
    frame.render_widget(Paragraph::new("").style(padding_style), bottom_padding_area);

    // Left padding
    let left_padding_area = Rect {
        x: area.x,
        y: area.y + 1,
        width: 1, // TEXTAREA_LEFT_PADDING
        height: textarea_inner_height,
    };
    frame.render_widget(Paragraph::new("").style(padding_style), left_padding_area);

    // Right padding
    let right_padding_area = Rect {
        x: area.x + area.width.saturating_sub(1),
        y: area.y + 1,
        width: 1, // TEXTAREA_RIGHT_PADDING
        height: textarea_inner_height,
    };
    frame.render_widget(Paragraph::new("").style(padding_style), right_padding_area);

    // Render the textarea
    frame.render_widget(&card.description, textarea_area);

    // Store textarea area for caret positioning on mouse clicks
    // Temporarily disabled to avoid borrowing issues - will be re-enabled later
    // view_model.last_textarea_area = Some(textarea_area);

    // Render separator line
    if (textarea_total_height as usize + separator_height) < content_height {
        let separator_area = Rect {
            x: area.x,
            y: area.y + textarea_total_height,
            width: area.width,
            height: separator_height as u16,
        };
        let separator = Paragraph::new("").style(Style::default().bg(theme.bg));
        frame.render_widget(separator, separator_area);
    }

    // Render buttons
    let repo_button_text = if card.repository.is_empty() {
        "📁 Repository".to_string()
    } else {
        format!("📁 {}", card.repository)
    };

    let branch_button_text = if card.branch.is_empty() {
        "🌿 Branch".to_string()
    } else {
        format!("🌿 {}", card.branch)
    };

    let models_button_text = if card.models.is_empty() {
        "🤖 Models".to_string()
    } else {
        format!("🤖 {} model(s)", card.models.len())
    };

    let go_button_text = "⏎ Go".to_string();

    // Create button spans with focus styling using theme - exactly like main.rs
    let repo_button = if matches!(card.focus_element, FocusElement::RepositoryButton) {
        Span::styled(format!(" {} ", repo_button_text), theme.focused_style())
    } else {
        Span::styled(
            format!(" {} ", repo_button_text),
            Style::default()
                .fg(theme.primary)
                .bg(theme.surface)
                .add_modifier(Modifier::BOLD),
        )
    };

    let branch_button = if matches!(card.focus_element, FocusElement::BranchButton) {
        Span::styled(format!(" {} ", branch_button_text), theme.focused_style())
    } else {
        Span::styled(
            format!(" {} ", branch_button_text),
            Style::default()
                .fg(theme.primary)
                .bg(theme.surface)
                .add_modifier(Modifier::BOLD),
        )
    };

    let models_button = if matches!(card.focus_element, FocusElement::ModelButton) {
        Span::styled(format!(" {} ", models_button_text), theme.focused_style())
    } else {
        Span::styled(
            format!(" {} ", models_button_text),
            Style::default()
                .fg(theme.primary)
                .bg(theme.surface)
                .add_modifier(Modifier::BOLD),
        )
    };

    let go_button = if matches!(card.focus_element, FocusElement::GoButton) {
        Span::styled(
            format!(" {} ", go_button_text),
            Style::default().fg(Color::Black).bg(theme.accent).add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            format!(" {} ", go_button_text),
            Style::default().fg(theme.accent).bg(theme.surface).add_modifier(Modifier::BOLD),
        )
    };

    let button_line = Line::from(vec![
        repo_button,
        Span::raw(" "),
        branch_button,
        Span::raw(" "),
        models_button,
        Span::raw(" "),
        go_button,
    ]);

    let button_paragraph = Paragraph::new(button_line).style(Style::default().bg(theme.bg));
    frame.render_widget(button_paragraph, button_area);

    // Register interactive areas for draft card buttons
    // Temporarily disabled to avoid borrowing issues - will be re-enabled later when ViewModel is moved to ah-tui
    // register_draft_card_button_areas(view_model, button_area, &repo_button_text, &branch_button_text, &models_button_text, &go_button_text);
}
