// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Renders draft task cards with textarea and control buttons.

use ratatui::{
    layout::{Constraint, Direction, Layout},
    prelude::*,
    widgets::*,
};

use super::Theme;
use crate::view_model::TaskEntryViewModel;
use crate::view_model::task_entry::CardFocusElement;

/// Geometry describing interactive regions inside a rendered draft card.
#[derive(Debug, Clone, Copy)]
pub struct DraftCardLayout {
    pub textarea: Rect,
    pub repository_button: Rect,
    pub branch_button: Rect,
    pub model_button: Rect,
    pub go_button: Rect,
    pub advanced_options_button: Rect,
}

/// Render a draft card (exact same as main.rs TaskCard::render with state == Draft)
/// Returns the textarea area for autocomplete positioning
pub fn render_draft_card(
    frame: &mut Frame<'_>,
    area: Rect,
    card: &TaskEntryViewModel,
    theme: &Theme,
    is_selected: bool,
) -> DraftCardLayout {
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
    render_draft_card_content(frame, inner_area, card, theme, is_selected)
}

/// Render draft card content (exact same as main.rs TaskCard::render_draft_card_content)
pub fn render_draft_card_content(
    frame: &mut Frame<'_>,
    area: Rect,
    card: &TaskEntryViewModel,
    theme: &Theme,
    is_selected: bool,
) -> DraftCardLayout {
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
        x: area.x + 1,                       // TEXTAREA_LEFT_PADDING
        y: area.y + 1,                       // TEXTAREA_TOP_PADDING
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

    // Render buttons - align repo/branch/models to left, Go to right
    let has_enumerators =
        card.repositories_enumerator.is_some() && card.branches_enumerator.is_some();

    let repo_button_text = if has_enumerators {
        if card.repository.is_empty() {
            Some("📁 Repository".to_string())
        } else {
            Some(format!("📁 {}", card.repository))
        }
    } else {
        None
    };

    let branch_button_text = if has_enumerators {
        if card.branch.is_empty() {
            Some("🌿 Branch".to_string())
        } else {
            Some(format!("🌿 {}", card.branch))
        }
    } else {
        None
    };

    let models_button_text = if card.selected_agents.is_empty() {
        "🤖 Models".to_string()
    } else {
        let model_parts: Vec<String> = card
            .selected_agents
            .iter()
            .map(|model| {
                if model.count > 1 {
                    format!("{:?} x{}", model.agent.software, model.count)
                } else {
                    format!("{:?}", model.agent.software)
                }
            })
            .collect();
        format!("🤖 {}", model_parts.join(", "))
    };

    // Calculate button positions: left buttons take available space, Go and Advanced Options buttons to the right
    let go_button_width = 6; // "⏎ Go" width
    let advanced_options_button_width = 4; // "⚙️" width
    let right_buttons_width = go_button_width + advanced_options_button_width;
    let left_buttons_width = button_area.width.saturating_sub(right_buttons_width);

    let left_buttons_area = Rect {
        x: button_area.x,
        y: button_area.y,
        width: left_buttons_width,
        height: button_area.height,
    };

    let go_button_rect = Rect {
        x: button_area.x + left_buttons_width,
        y: button_area.y,
        width: go_button_width,
        height: button_area.height,
    };

    let advanced_options_button_rect = Rect {
        x: button_area.x + left_buttons_width + go_button_width,
        y: button_area.y,
        width: advanced_options_button_width,
        height: button_area.height,
    };

    // Create layout for left buttons (repo, branch, models)
    let left_button_constraints = if has_enumerators {
        vec![
            Constraint::Ratio(1, 3), // Repository
            Constraint::Ratio(1, 3), // Branch
            Constraint::Ratio(1, 3), // Models
        ]
    } else {
        vec![Constraint::Ratio(1, 1)] // Models only
    };
    let left_button_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(left_button_constraints)
        .split(left_buttons_area);

    let repo_style =
        if is_selected && matches!(card.focus_element, CardFocusElement::RepositorySelector) {
            theme.focused_style()
        } else {
            Style::default()
                .fg(theme.primary)
                .bg(theme.surface)
                .add_modifier(Modifier::BOLD)
        };

    let branch_style =
        if is_selected && matches!(card.focus_element, CardFocusElement::BranchSelector) {
            theme.focused_style()
        } else {
            Style::default()
                .fg(theme.primary)
                .bg(theme.surface)
                .add_modifier(Modifier::BOLD)
        };

    let model_style =
        if is_selected && matches!(card.focus_element, CardFocusElement::ModelSelector) {
            theme.focused_style()
        } else {
            Style::default()
                .fg(theme.primary)
                .bg(theme.surface)
                .add_modifier(Modifier::BOLD)
        };

    let go_style = if is_selected && matches!(card.focus_element, CardFocusElement::GoButton) {
        Style::default().fg(Color::Black).bg(theme.accent).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.accent).bg(theme.surface).add_modifier(Modifier::BOLD)
    };

    let advanced_options_style =
        if is_selected && matches!(card.focus_element, CardFocusElement::AdvancedOptionsButton) {
            theme.focused_style()
        } else {
            Style::default()
                .fg(theme.primary)
                .bg(theme.surface)
                .add_modifier(Modifier::BOLD)
        };

    let mut render_button = |chunk: Rect, label: &str, style: Style, alignment: Alignment| {
        frame.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                format!(" {} ", label),
                style,
            )]))
            .alignment(alignment)
            .style(Style::default().bg(theme.bg)),
            chunk,
        );
    };

    // Render left buttons
    let repository_button_rect = if let Some(ref repo_text) = repo_button_text {
        render_button(
            left_button_chunks[0],
            repo_text,
            repo_style,
            Alignment::Left,
        );
        left_button_chunks[0]
    } else {
        Rect::default()
    };

    let branch_button_rect = if let Some(ref branch_text) = branch_button_text {
        render_button(
            left_button_chunks[1],
            branch_text,
            branch_style,
            Alignment::Left,
        );
        left_button_chunks[1]
    } else {
        Rect::default()
    };

    let model_button_rect = {
        let model_chunk_index = if has_enumerators { 2 } else { 0 };
        render_button(
            left_button_chunks[model_chunk_index],
            &models_button_text,
            model_style,
            Alignment::Left,
        );
        left_button_chunks[model_chunk_index]
    };

    // Render Go button to the right
    render_button(go_button_rect, "⏎ Go", go_style, Alignment::Center);

    // Render Advanced Options button next to Go button
    render_button(
        advanced_options_button_rect,
        "⚙️",
        advanced_options_style,
        Alignment::Center,
    );

    DraftCardLayout {
        textarea: textarea_area,
        repository_button: repository_button_rect,
        branch_button: branch_button_rect,
        model_button: model_button_rect,
        go_button: go_button_rect,
        advanced_options_button: advanced_options_button_rect,
    }
}
