//! Dialog/Modal rendering functions for the TUI
//!
//! This module contains all dialog and modal rendering functions
//! extracted from the main application for better organization.

use ratatui::{
    prelude::*,
    widgets::*,
};

use super::Theme;

/// Modal state enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalState {
    None,
    RepositorySearch,
    BranchSearch,
    ModelSearch,
    ModelSelection,
    Settings,
    GoToLine,
    FindReplace,
    ShortcutHelp,
}

/// Search mode enumeration
#[derive(Debug, Clone, PartialEq)]
pub enum SearchMode {
    None,
    IncrementalForward,
    IncrementalBackward,
}

/// Find/replace stage enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindReplaceStage {
    EnterSearch,
    EnterReplacement,
}

/// Shortcut help modal
#[derive(Debug, Clone)]
pub struct ShortcutHelpModal {
    pub entries: Vec<ShortcutDisplay>,
    pub scroll: usize,
}

/// Model selection modal
#[derive(Debug, Clone)]
pub struct ModelSelectionModal {
    pub available_models: Vec<String>,
    pub selected_models: Vec<ah_domain_types::SelectedModel>,
    pub selected_index: usize, // Index in available_models for adding new models
    pub editing_count: bool,   // Whether we're editing the count of a selected model
    pub editing_index: usize,  // Index in selected_models when editing count
}

/// Fuzzy search modal
#[derive(Debug, Clone)]
pub struct FuzzySearchModal {
    pub input: String,
    pub options: Vec<String>,
    pub selected_index: usize,
}

/// Go to line modal
#[derive(Debug, Clone)]
pub struct GoToLineModal {
    pub input: String,
    pub max_line: usize,
    pub error: Option<String>,
}

/// Find replace modal
#[derive(Debug, Clone)]
pub struct FindReplaceModal {
    pub search_input: String,
    pub replace_input: String,
    pub is_regex: bool,
    pub stage: FindReplaceStage,
    pub error: Option<String>,
}

/// Shortcut display structure
#[derive(Debug, Clone)]
pub struct ShortcutDisplay {
    pub key: String,
    pub description: String,
    pub category: String,
}

/// Render settings dialog
pub fn render_settings_dialog(frame: &mut Frame, modal_area: Rect, theme: &Theme) {
    // Calculate modal dimensions
    let modal_width = 70.min(modal_area.width - 4);
    let modal_height = 20.min(modal_area.height - 4);

    let area = Rect {
        x: (modal_area.width - modal_width) / 2,
        y: (modal_area.height - modal_height) / 2,
        width: modal_width,
        height: modal_height,
    };

    // Shadow effect
    let mut shadow_area = area;
    shadow_area.x += 1;
    shadow_area.y += 1;
    let shadow = Block::default().style(Style::default().bg(Color::Rgb(10, 10, 15)));
    frame.render_widget(Clear, shadow_area);
    frame.render_widget(shadow, shadow_area);

    // Main modal
    let title_line = Line::from(vec![
        Span::raw("").fg(theme.primary),
        Span::raw(" Settings ").style(Style::default().fg(theme.text).add_modifier(Modifier::BOLD)),
        Span::raw("").fg(theme.primary),
    ]);

    let dialog_block = Block::default()
        .title(title_line)
        .title_alignment(Alignment::Left)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_focused))
        .padding(Padding::new(2, 2, 1, 1))
        .style(Style::default().bg(theme.surface));

    frame.render_widget(Clear, area);
    let inner_area = dialog_block.inner(area);
    frame.render_widget(dialog_block, area);

    // Settings content placeholder - would be filled with actual settings UI
    let content = Paragraph::new("Settings dialog content would go here...")
        .style(Style::default().fg(theme.text));
    frame.render_widget(content, inner_area);
}

/// Render fuzzy search modal
pub fn render_fuzzy_modal(frame: &mut Frame, modal: &FuzzySearchModal, area: Rect, theme: &Theme) {
    // Calculate modal dimensions
    let modal_width = 60.min(area.width - 4);
    let modal_height = 15.min(area.height - 4);

    let modal_area = Rect {
        x: (area.width - modal_width) / 2,
        y: (area.height - modal_height) / 2,
        width: modal_width,
        height: modal_height,
    };

    // Shadow effect (offset darker rectangle)
    let mut shadow_area = modal_area;
    shadow_area.x += 1;
    shadow_area.y += 1;
    let shadow = Block::default().style(Style::default().bg(Color::Rgb(10, 10, 15)));
    frame.render_widget(Clear, shadow_area);
    frame.render_widget(shadow, shadow_area);

    // Main modal with Charm styling
    let title_line = Line::from(vec![
        Span::raw("").fg(theme.primary),
        Span::raw(" Select ").style(Style::default().fg(theme.text).add_modifier(Modifier::BOLD)),
        Span::raw("").fg(theme.primary),
    ]);

    let modal_block = Block::default()
        .title(title_line)
        .title_alignment(Alignment::Left)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_focused))
        .padding(Padding::new(1, 1, 0, 0))
        .style(Style::default().bg(theme.surface));

    frame.render_widget(Clear, modal_area);
    let inner_area = modal_block.inner(modal_area);
    frame.render_widget(modal_block, modal_area);

    // Split into input and options areas
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Input area
            Constraint::Min(0),    // Options area
        ])
        .split(inner_area);

    // Input section
    let input_block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(theme.border));
    frame.render_widget(input_block, layout[0]);

    // Input area (smaller)
    let input_area = Rect {
        x: layout[0].x + 1,
        y: layout[0].y + 1,
        width: layout[0].width.saturating_sub(2),
        height: 1,
    };

    let input_paragraph = Paragraph::new(modal.input.as_str())
        .style(Style::default().fg(theme.text));
    frame.render_widget(input_paragraph, input_area);

    // Options section
    let options_area = layout[1];
    let start_index = modal.selected_index.saturating_sub(5);
    let visible_options = modal.options.iter()
        .enumerate()
        .skip(start_index)
        .take(options_area.height as usize)
        .collect::<Vec<_>>();

    for (i, (global_idx, option)) in visible_options.into_iter().enumerate() {
        let y = options_area.y + i as u16;
        let style = if global_idx == modal.selected_index {
            Style::default().fg(theme.bg).bg(theme.primary).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text)
        };

        let line = Line::from(vec![Span::styled(option.clone(), style)]);
        let rect = Rect {
            x: options_area.x,
            y,
            width: options_area.width,
            height: 1,
        };
        frame.render_widget(Paragraph::new(line), rect);
    }
}

/// Render model selection modal
pub fn render_model_selection_modal(frame: &mut Frame, modal: &ModelSelectionModal, area: Rect, theme: &Theme) {
    // Calculate modal dimensions
    let modal_width = 50.min(area.width - 4);
    let modal_height = 15.min(area.height - 4);

    let modal_area = Rect {
        x: (area.width - modal_width) / 2,
        y: (area.height - modal_height) / 2,
        width: modal_width,
        height: modal_height,
    };

    // Shadow effect
    let mut shadow_area = modal_area;
    shadow_area.x += 1;
    shadow_area.y += 1;
    let shadow = Block::default().style(Style::default().bg(Color::Rgb(10, 10, 15)));
    frame.render_widget(Clear, shadow_area);
    frame.render_widget(shadow, shadow_area);

    // Main modal
    let title_line = Line::from(vec![
        Span::raw("").fg(theme.primary),
        Span::raw(" Model Selection ").style(Style::default().fg(theme.text).add_modifier(Modifier::BOLD)),
        Span::raw("").fg(theme.primary),
    ]);

    let modal_block = Block::default()
        .title(title_line)
        .title_alignment(Alignment::Left)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_focused))
        .padding(Padding::new(1, 1, 0, 0))
        .style(Style::default().bg(theme.surface));

    frame.render_widget(Clear, modal_area);
    let inner_area = modal_block.inner(modal_area);
    frame.render_widget(modal_block, modal_area);

    // Content placeholder - would show available models
    let content = Paragraph::new("Model selection dialog content would go here...")
        .style(Style::default().fg(theme.text));
    frame.render_widget(content, inner_area);
}

/// Render go to line modal
pub fn render_go_to_line_modal(frame: &mut Frame, modal: &GoToLineModal, area: Rect, theme: &Theme) {
    // Calculate modal dimensions
    let modal_width = 40.min(area.width - 4);
    let modal_height = 6.min(area.height - 4);

    let modal_area = Rect {
        x: (area.width - modal_width) / 2,
        y: (area.height - modal_height) / 2,
        width: modal_width,
        height: modal_height,
    };

    // Shadow effect
    let mut shadow_area = modal_area;
    shadow_area.x += 1;
    shadow_area.y += 1;
    let shadow = Block::default().style(Style::default().bg(Color::Rgb(10, 10, 15)));
    frame.render_widget(Clear, shadow_area);
    frame.render_widget(shadow, shadow_area);

    // Main modal
    let title_line = Line::from(vec![
        Span::raw("").fg(theme.primary),
        Span::raw(" Go to Line ").style(Style::default().fg(theme.text).add_modifier(Modifier::BOLD)),
        Span::raw("").fg(theme.primary),
    ]);

    let modal_block = Block::default()
        .title(title_line)
        .title_alignment(Alignment::Left)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_focused))
        .padding(Padding::new(2, 2, 1, 1))
        .style(Style::default().bg(theme.surface));

    frame.render_widget(Clear, modal_area);
    let inner_area = modal_block.inner(modal_area);
    frame.render_widget(modal_block, modal_area);

    // Input content
    let mut content_lines = vec![Line::from(vec![
        Span::styled("Line number: ", Style::default().fg(theme.muted)),
        Span::styled(&modal.input, Style::default().fg(theme.text)),
    ])];

    if let Some(error) = &modal.error {
        content_lines.push(Line::from(vec![
            Span::styled(error, Style::default().fg(theme.error)),
        ]));
    }

    let content = Paragraph::new(content_lines)
        .wrap(Wrap { trim: true });
    frame.render_widget(content, inner_area);
}

/// Render find replace modal
pub fn render_find_replace_modal(frame: &mut Frame, modal: &FindReplaceModal, area: Rect, theme: &Theme) {
    // Calculate modal dimensions
    let modal_width = 60.min(area.width - 4);
    let modal_height = 10.min(area.height - 4);

    let modal_area = Rect {
        x: (area.width - modal_width) / 2,
        y: (area.height - modal_height) / 2,
        width: modal_width,
        height: modal_height,
    };

    // Shadow effect
    let mut shadow_area = modal_area;
    shadow_area.x += 1;
    shadow_area.y += 1;
    let shadow = Block::default().style(Style::default().bg(Color::Rgb(10, 10, 15)));
    frame.render_widget(Clear, shadow_area);
    frame.render_widget(shadow, shadow_area);

    // Main modal
    let title_text = match modal.stage {
        FindReplaceStage::EnterSearch => "Find",
        FindReplaceStage::EnterReplacement => "Replace",
    };

    let title_line = Line::from(vec![
        Span::raw("").fg(theme.primary),
        Span::styled(format!(" {} ", title_text), Style::default().fg(theme.text).add_modifier(Modifier::BOLD)),
        Span::raw("").fg(theme.primary),
    ]);

    let modal_block = Block::default()
        .title(title_line)
        .title_alignment(Alignment::Left)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_focused))
        .padding(Padding::new(2, 2, 1, 1))
        .style(Style::default().bg(theme.surface));

    frame.render_widget(Clear, modal_area);
    let inner_area = modal_block.inner(modal_area);
    frame.render_widget(modal_block, modal_area);

    // Content
    let mut content_lines = Vec::new();

    if modal.is_regex {
        content_lines.push(Line::from(vec![
            Span::styled("Regex mode", Style::default().fg(theme.accent)),
        ]));
    }

    content_lines.push(Line::from(vec![
        Span::styled("Find: ", Style::default().fg(theme.muted)),
        Span::styled(&modal.search_input, Style::default().fg(theme.text)),
    ]));

    if modal.stage == FindReplaceStage::EnterReplacement {
        content_lines.push(Line::from(vec![
            Span::styled("Replace: ", Style::default().fg(theme.muted)),
            Span::styled(&modal.replace_input, Style::default().fg(theme.text)),
        ]));
    }

    if let Some(error) = &modal.error {
        content_lines.push(Line::from(vec![
            Span::styled(error, Style::default().fg(theme.error)),
        ]));
    }

    let content = Paragraph::new(content_lines)
        .wrap(Wrap { trim: true });
    frame.render_widget(content, inner_area);
}

/// Render shortcut help modal
pub fn render_shortcut_help_modal(frame: &mut Frame, modal: &ShortcutHelpModal, area: Rect, theme: &Theme) {
    // Calculate modal dimensions
    let modal_width = 80.min(area.width - 4);
    let modal_height = 20.min(area.height - 4);

    let modal_area = Rect {
        x: (area.width - modal_width) / 2,
        y: (area.height - modal_height) / 2,
        width: modal_width,
        height: modal_height,
    };

    // Shadow effect
    let mut shadow_area = modal_area;
    shadow_area.x += 1;
    shadow_area.y += 1;
    let shadow = Block::default().style(Style::default().bg(Color::Rgb(10, 10, 15)));
    frame.render_widget(Clear, shadow_area);
    frame.render_widget(shadow, shadow_area);

    // Main modal
    let title_line = Line::from(vec![
        Span::raw("").fg(theme.primary),
        Span::raw(" Keyboard Shortcuts ").style(Style::default().fg(theme.text).add_modifier(Modifier::BOLD)),
        Span::raw("").fg(theme.primary),
    ]);

    let modal_block = Block::default()
        .title(title_line)
        .title_alignment(Alignment::Left)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_focused))
        .padding(Padding::new(1, 1, 0, 0))
        .style(Style::default().bg(theme.surface));

    frame.render_widget(Clear, modal_area);
    let inner_area = modal_block.inner(modal_area);
    frame.render_widget(modal_block, modal_area);

    // Content placeholder - would show keyboard shortcuts
    let content = Paragraph::new("Keyboard shortcuts would be listed here...")
        .style(Style::default().fg(theme.text));
    frame.render_widget(content, inner_area);
}

/// Render modal input line helper
fn render_modal_input_line<'a>(
    label: &'a str,
    value: &'a str,
    active: bool,
    theme: &'a Theme,
) -> Vec<Span<'a>> {
    let mut spans = Vec::new();

    spans.push(Span::styled(
        format!("{}: ", label),
        if active {
            Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.muted)
        }
    ));

    spans.push(Span::styled(
        value,
        if active {
            Style::default().fg(theme.text)
        } else {
            Style::default().fg(theme.muted)
        }
    ));

    spans
}
