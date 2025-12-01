// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: Apache-2.0

//! Modal rendering functions
//!
//! This module contains functions for rendering modal dialogs and overlays.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::prelude::Stylize;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph};

use super::Theme;
use super::dialogs::{
    FuzzySearchModal, render_fuzzy_modal, render_model_selection_modal_with_hit_regions,
    render_settings_dialog,
};
use crate::view_model::agents_selector_model::FilteredOption;
use crate::view_model::{ModalState, ViewModel};

/// Render active modal dialogs
pub fn render_modals(
    frame: &mut Frame,
    view_model: &ViewModel,
    area: Rect,
    theme: &Theme,
    hit_registry: &mut crate::view::HitTestRegistry<crate::view_model::MouseAction>,
) {
    match view_model.modal_state {
        ModalState::None => {
            // No modal to render
        }
        ModalState::RepositorySearch => {
            // Render repository search modal with mouse click support
            if let Some(modal) = &view_model.active_modal {
                render_fuzzy_modal_with_mouse_support(frame, modal, area, theme, hit_registry, 3);
            }
        }
        ModalState::BranchSearch => {
            // Render branch search modal with mouse click support
            if let Some(modal) = &view_model.active_modal {
                render_fuzzy_modal_with_mouse_support(frame, modal, area, theme, hit_registry, 3);
            }
        }
        ModalState::ModelSearch => {
            // Use the actual modal data from view_model
            if let Some(modal) = &view_model.active_modal {
                if let crate::view_model::ModalType::AgentSelection { options } = &modal.modal_type
                {
                    // For AgentSelection, render with +/- controls and register hit regions
                    render_model_selection_modal_with_hit_regions(
                        frame,
                        modal,
                        options,
                        area,
                        theme,
                        hit_registry,
                    );
                } else {
                    // For other modal types, use fuzzy search rendering
                    let fuzzy_modal = FuzzySearchModal {
                        input: modal.input_value.clone(),
                        options: modal
                            .filtered_options
                            .iter()
                            .filter_map(|opt| match opt {
                                FilteredOption::Option { text, .. } => Some(text.clone()),
                                FilteredOption::Separator { .. } => None,
                            })
                            .collect(),
                        selected_index: modal.selected_index,
                    };
                    render_fuzzy_modal(frame, &fuzzy_modal, area, theme, 3);
                }
            }
        }
        ModalState::Settings => {
            render_settings_dialog(frame, area, theme);
        }
        ModalState::LaunchOptions => {
            // Render launch options modal
            if let Some(modal) = &view_model.active_modal {
                match &modal.modal_type {
                    crate::view_model::ModalType::LaunchOptions {
                        view_model: launch_vm,
                    } => {
                        super::launch_options_modal::render_advanced_launch_options_modal(
                            frame,
                            launch_vm,
                            area,
                            theme,
                            hit_registry,
                        );
                    }
                    crate::view_model::ModalType::EnumSelection {
                        title,
                        options,
                        selected_index,
                        ..
                    } => {
                        render_enum_selection_modal(
                            frame,
                            title,
                            options,
                            *selected_index,
                            area,
                            theme,
                        );
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Render fuzzy search modal with mouse click support
fn render_fuzzy_modal_with_mouse_support(
    frame: &mut Frame,
    modal: &crate::view_model::ModalViewModel,
    area: Rect,
    theme: &Theme,
    hit_registry: &mut crate::view::HitTestRegistry<crate::view_model::MouseAction>,
    input_height: u16,
) {
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
    let shadow = Block::default().style(Style::default().bg(theme.shadow));
    frame.render_widget(Clear, shadow_area);
    frame.render_widget(shadow, shadow_area);

    // Main modal with Charm styling
    let title_line = Line::from(vec![
        Span::raw("┤").fg(theme.primary),
        Span::raw(" Select ").style(Style::default().fg(theme.text).add_modifier(Modifier::BOLD)),
        Span::raw("├").fg(theme.primary),
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
            Constraint::Length(input_height), // Input area
            Constraint::Min(0),               // Options area
        ])
        .split(inner_area);

    // Input section
    let input_block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(theme.border));
    frame.render_widget(input_block, layout[0]);

    // Input area (inside the input block, leaving space for the bottom border)
    let input_area = Rect {
        x: layout[0].x,
        y: layout[0].y + 1,
        width: layout[0].width.saturating_sub(2),
        height: 1,
    };

    let input_paragraph =
        Paragraph::new(modal.input_value.as_str()).style(Style::default().fg(theme.text));
    frame.render_widget(input_paragraph, input_area);

    // Options section
    let options_area = layout[1];
    let start_index = modal.selected_index.saturating_sub(5);
    let visible_options = modal
        .filtered_options
        .iter()
        .enumerate()
        .skip(start_index)
        .take(options_area.height as usize)
        .collect::<Vec<_>>();

    for (i, (global_idx, option)) in visible_options.into_iter().enumerate() {
        let y = options_area.y + i as u16;

        // Render option text
        let style = if global_idx == modal.selected_index {
            Style::default().fg(theme.bg).bg(theme.primary).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text)
        };

        match option {
            FilteredOption::Option { text, .. } => {
                let line = Line::from(vec![Span::styled(text.clone(), style)]);
                let rect = Rect {
                    x: options_area.x,
                    y,
                    width: options_area.width,
                    height: 1,
                };
                frame.render_widget(Paragraph::new(line), rect);

                // Register hit region for mouse click
                hit_registry.register(
                    rect,
                    crate::view_model::MouseAction::ModalSelectOption(global_idx),
                );
            }
            FilteredOption::Separator { .. } => {
                // Skip separators for mouse clicks
            }
        }
    }
}

/// Render enum selection modal (simple submenu without search box)
fn render_enum_selection_modal(
    frame: &mut Frame,
    title: &str,
    options: &[String],
    selected_index: usize,
    area: Rect,
    theme: &Theme,
) {
    // Calculate modal dimensions
    let modal_width = 40.min(area.width - 4);
    let modal_height = 10.min(area.height - 4);

    let modal_area = Rect {
        x: (area.width - modal_width) / 2,
        y: (area.height - modal_height) / 2,
        width: modal_width,
        height: modal_height,
    };

    // Shadow
    let mut shadow_area = modal_area;
    shadow_area.x += 1;
    shadow_area.y += 1;
    let shadow = Block::default().style(Style::default().bg(theme.shadow));
    frame.render_widget(Clear, shadow_area);
    frame.render_widget(shadow, shadow_area);

    // Main modal
    let title_line = Line::from(vec![
        Span::raw("┤").fg(theme.primary),
        Span::raw(format!(" {} ", title))
            .style(Style::default().fg(theme.text).add_modifier(Modifier::BOLD)),
        Span::raw("├").fg(theme.primary),
    ]);

    let modal_block = Block::default()
        .title(title_line)
        .title_alignment(Alignment::Left)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_focused))
        .padding(Padding::new(1, 1, 1, 1))
        .style(Style::default().bg(theme.surface));

    frame.render_widget(Clear, modal_area);
    let inner_area = modal_block.inner(modal_area);
    frame.render_widget(modal_block, modal_area);

    // Render options
    for (i, option) in options.iter().enumerate() {
        let is_selected = i == selected_index;

        let (bg, fg) = if is_selected {
            (theme.primary, theme.surface)
        } else {
            (theme.surface, theme.text)
        };
        let style = Style::default().bg(bg).fg(fg);

        let row_area = Rect {
            x: inner_area.x,
            y: inner_area.y + i as u16,
            width: inner_area.width,
            height: 1,
        };

        frame.render_widget(Paragraph::new(option.clone()).style(style), row_area);
    }
}
