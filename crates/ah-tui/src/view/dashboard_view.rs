// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! View Layer - Pure Rendering and Presentation
//!
//! This module contains the Ratatui rendering code that transforms
//! ViewModel state into terminal widgets. The View layer is the final
//! step in the MVVM pipeline and should contain zero business logic.
//!
//! ## What Belongs Here:
//!
//! ✅ **Rendering Logic**: Ratatui widget creation and layout
//! ✅ **Visual Styling**: Colors, borders, spacing, typography
//! ✅ **Widget Composition**: Combining ViewModel data into UI layouts
//! ✅ **Terminal Drawing**: Converting widgets to terminal output
//! ✅ **Pure Functions**: ViewModel → Ratatui widgets transformations
//!
//! ## What Does NOT Belong Here:
//!
//! ❌ **Business Logic**: Any application behavior or state changes
//! ❌ **UI Events**: Key handling, mouse processing, input validation
//! ❌ **UI State**: Selection management, focus tracking, modal states
//! ❌ **Domain Logic**: Task operations, business rules, calculations
//!
//! ## Architecture Role:
//!
//! The View is the final, pure transformation layer:
//! 1. **Receives ViewModel** - Already prepared presentation data
//! 2. **Creates Ratatui widgets** - Terminal UI components
//! 3. **Handles layout** - Positioning, sizing, responsive design
//! 4. **Applies styling** - Colors, borders, visual hierarchy
//! 5. **Renders to terminal** - Final pixel output
//!
//! ## Design Principles:
//!
//! - **Pure Functions Only**: View functions should be deterministic and side-effect free
//! - **No State Mutations**: View never modifies ViewModel or Model state
//! - **Presentation Only**: Focus on visual appearance and user experience
//! - **Testable**: Rendering logic can be tested independently

use crate::view::autocomplete::{render_autocomplete, render_autocomplete_ghost};
use crate::view::draft_card;
use crate::view::{HitTestRegistry, Theme, ViewCache};
use crate::view_model::AgentActivityRow;
use crate::view_model::{DashboardFocusState, TaskCardType};
use crate::view_model::{
    DraftSaveState, TaskEntryViewModel, TaskExecutionFocusState, TaskExecutionViewModel,
};
use crate::view_model::{MouseAction, TaskCardTypeEnum, ViewModel};
// (no ah_core items needed at module scope)
use ah_domain_types::TaskState;
use ah_domain_types::task::ToolStatus;
use ratatui::{prelude::*, widgets::*};
use ratatui_image::StatefulImage;
use tracing::{debug, warn};

/// Display item types (exact same as main.rs)
#[derive(Debug, Clone)]
enum DisplayItem {
    Task(String), // Task ID
    FilterBar,
    Spacer,
}

fn render_header(
    frame: &mut Frame<'_>,
    area: Rect,
    theme: &Theme,
    view_model: &mut ViewModel,
    cache: &mut ViewCache,
    hit_registry: &mut HitTestRegistry<MouseAction>,
) {
    // Create padded content area within the header
    let content_area = if area.width >= 6 && area.height >= 4 {
        // Add padding: 1 line top/bottom, 2 columns left/right
        let vertical_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Top padding
                Constraint::Min(1),    // Content area
                Constraint::Length(1), // Bottom padding
            ])
            .split(area);

        let middle_area = vertical_chunks[1];

        let horizontal_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(2), // Left padding
                Constraint::Min(1),    // Content area
                Constraint::Length(2), // Right padding
            ])
            .split(middle_area);

        horizontal_chunks[1]
    } else {
        // If area is too small, use the full area (no padding)
        area
    };

    // Render settings button in upper right corner (before logo to ensure it's always visible)
    if area.width > 15 && area.height > 2 {
        let button_text = "⚙ Settings";
        let button_width = button_text.len() as u16 + 2; // +2 for padding
        let button_x = area.x.saturating_add(area.width.saturating_sub(button_width + 2));
        let button_area = Rect {
            x: button_x,   // 2 units from right edge
            y: area.y + 1, // Just below top padding
            width: button_width,
            height: 1,
        };

        let button_style = if matches!(
            view_model.focus_element,
            DashboardFocusState::SettingsButton
        ) {
            Style::default().fg(theme.bg).bg(theme.primary).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(theme.primary)
                .bg(theme.surface)
                .add_modifier(Modifier::BOLD)
        };

        let button_line = Line::from(vec![
            Span::styled(" ", button_style),
            Span::styled(button_text, button_style),
            Span::styled(" ", button_style),
        ]);

        let button_paragraph = Paragraph::new(button_line);
        frame.render_widget(button_paragraph, button_area);

        hit_registry.register(button_area, MouseAction::OpenSettings);
    }

    // Try to render the logo as an image first using persisted protocol
    if let Some(protocol) = cache.logo_protocol.as_mut() {
        // Render the logo image using StatefulImage widget in the padded area
        let image_widget = StatefulImage::default();
        frame.render_stateful_widget(image_widget, content_area, protocol);

        // Check for encoding errors and log them (don't fail the whole UI)
        if let Some(Err(e)) = protocol.last_encoding_result() {
            // If image rendering fails, fall through to ASCII
            tracing::warn!("Image logo rendering failed: {}", e);
        } else {
            // Image rendered successfully, we're done
            return;
        }
    }

    // Fallback to ASCII logo
    render_ascii_logo(frame, content_area);
}

/// Render the ASCII logo as fallback
fn render_ascii_logo(frame: &mut Frame<'_>, area: Rect) {
    // Try to read the ASCII logo from assets
    let logo_content = include_str!("../../../../assets/agent-harbor-logo-80.ansi");

    // Create a paragraph with the logo, preserving ANSI escape codes
    let header = Paragraph::new(logo_content)
        .style(Style::default())
        .alignment(Alignment::Center);
    frame.render_widget(header, area);
}

/// Main rendering function - transforms ViewModel to Ratatui widgets
#[allow(clippy::needless_borrow)]
pub fn render(
    frame: &mut Frame<'_>,
    view_model: &mut ViewModel,
    cache: &mut ViewCache,
    hit_registry: &mut HitTestRegistry<MouseAction>,
    theme: &Theme,
) {
    let size = frame.area();

    // Clear interactive areas before rendering
    hit_registry.clear();

    // Sync cursor style from ViewModel to terminal
    cache.sync_cursor_style(view_model.cursor_style).unwrap_or_else(|e| {
        warn!("Failed to sync cursor style: {}", e);
    });

    // Background fill with theme color
    let bg = Paragraph::new("").style(Style::default().bg(theme.bg));
    frame.render_widget(bg, size);

    // Main layout (adaptive to terminal size)
    let min_header_height = 9;
    let min_tasks_height = 5;
    let footer_height = 1;
    let padding_height = 1;
    let min_total_height = min_header_height + min_tasks_height + footer_height + padding_height;

    let (header_height, tasks_height, footer_y, padding_y) = if size.height >= min_total_height {
        // Enough space for full layout
        (
            min_header_height,
            size.height - min_header_height - footer_height - padding_height,
            size.height - footer_height - padding_height,
            size.height - padding_height,
        )
    } else if size.height >= 10 {
        // Minimum viable layout
        let available = size.height - footer_height - padding_height;
        let header_actual = (available * 3 / 5).max(3); // 60% for header minimum 3
        let tasks_actual = available - header_actual;
        (
            header_actual,
            tasks_actual,
            size.height - footer_height - padding_height,
            size.height - padding_height,
        )
    } else {
        // Emergency layout for very small terminals
        (
            size.height.saturating_sub(2),
            0,
            size.height.saturating_sub(1),
            size.height,
        )
    };

    let main_layout = if size.height >= 3 {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(header_height),
                Constraint::Length(tasks_height),
                Constraint::Length(footer_height),
                Constraint::Length(padding_height),
            ])
            .split(size)
    } else {
        // Fallback for extremely small terminals
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),
                Constraint::Length(0),
                Constraint::Length(0),
                Constraint::Length(0),
            ])
            .split(size)
    };

    // Render header
    render_header(
        frame,
        main_layout[0],
        &theme,
        view_model,
        cache,
        hit_registry,
    );

    // Render tasks with screen edge padding (exact same as main.rs)
    let tasks_area_unpadded = main_layout[1];
    let tasks_area = if tasks_area_unpadded.width >= 6 {
        let horizontal_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(2), // Left padding
                Constraint::Min(1),    // Content area
                Constraint::Length(2), // Right padding
            ])
            .split(tasks_area_unpadded);
        horizontal_chunks[1]
    } else {
        tasks_area_unpadded
    };

    // Create display items (exact same logic as main.rs)
    let mut display_items = Vec::new();

    // Add draft cards first
    for card in &view_model.draft_cards {
        display_items.push(DisplayItem::Task(card.id.clone()));
        display_items.push(DisplayItem::Spacer);
    }

    display_items.push(DisplayItem::FilterBar);
    display_items.push(DisplayItem::Spacer);

    // Add task cards
    for card in &view_model.task_cards {
        if let Ok(card_guard) = card.lock() {
            display_items.push(DisplayItem::Task(card_guard.task.id.clone()));
            display_items.push(DisplayItem::Spacer);
        }
    }

    // Remove trailing spacer if present
    if matches!(display_items.last(), Some(DisplayItem::Spacer)) {
        display_items.pop();
    }

    // Simple layout for MVVM testing - just stack items without scrolling
    let mut item_rects: Vec<(DisplayItem, Rect)> = Vec::new();
    let mut screen_y = tasks_area.y;

    for item in display_items {
        let item_height = match &item {
            DisplayItem::Spacer => 1,
            DisplayItem::FilterBar => 1,
            DisplayItem::Task(id) => {
                // Calculate height based on card content and presentation needs
                if let Some(card_info) = view_model.find_task_card_info(id.as_str()) {
                    match card_info.card_type {
                        TaskCardTypeEnum::Draft => view_model.draft_cards[card_info.index].height,
                        TaskCardTypeEnum::Task => {
                            if let Ok(card_guard) = view_model.task_cards[card_info.index].lock() {
                                calculate_task_card_height(&card_guard)
                            } else {
                                1
                            }
                        }
                    }
                } else {
                    1
                }
            }
        };

        // Only add items that fit within the screen bounds
        let screen = frame.area();
        if screen_y + item_height <= screen.height {
            let rect = Rect {
                x: tasks_area.x,
                y: screen_y,
                width: tasks_area.width,
                height: item_height,
            };
            item_rects.push((item, rect));
        }
        screen_y = screen_y.saturating_add(item_height);
    }

    // Render display items
    for (item, rect) in item_rects {
        match item {
            DisplayItem::Spacer => {
                frame.render_widget(
                    Paragraph::new("").style(Style::default().bg(theme.bg)),
                    rect,
                );
            }
            DisplayItem::FilterBar => {
                render_filter_bar(frame, rect, view_model, &theme, cache, hit_registry);
            }
            DisplayItem::Task(id) => {
                // Find and render the card using fast lookup
                if let Some(card_info) = view_model.find_task_card_info(id.as_str()) {
                    let (card_index, draft_layout) = match card_info.card_type {
                        TaskCardTypeEnum::Draft => {
                            let card = &view_model.draft_cards[card_info.index];
                            let is_selected = matches!(view_model.focus_element, DashboardFocusState::DraftTask(idx) if idx == card_info.index);
                            let layout = draft_card::render_draft_card(
                                frame,
                                rect,
                                card,
                                &theme,
                                is_selected,
                            );

                            // Update focused textarea rect for cursor positioning
                            if is_selected && card.focus_element == crate::view_model::task_entry::CardFocusElement::TaskDescription {
                                cache.update_focused_textarea_rect(layout.textarea);
                            }

                            // Store textarea area for autocomplete positioning
                            view_model.last_textarea_area = Some(layout.textarea);
                            (0, Some(layout)) // Draft card is always at index 0
                        }
                        TaskCardTypeEnum::Task => {
                            if let Ok(card_guard) = view_model.task_cards[card_info.index].lock() {
                                let is_selected = matches!(view_model.focus_element, DashboardFocusState::ExistingTask(idx) if idx == card_info.index);
                                render_task_card(frame, rect, &card_guard, &theme, is_selected);
                                (card_info.index + 1, None) // Task cards start at index 1 (after draft)
                            } else {
                                (card_info.index + 1, None)
                            }
                        }
                    };

                    // Add interactive area for the card (registered first so textarea can override)
                    hit_registry.register(rect, MouseAction::SelectCard(card_index));

                    // For draft cards, register textarea after card selection so it takes precedence
                    if let Some(layout) = draft_layout {
                        hit_registry.register(
                            layout.textarea,
                            MouseAction::FocusDraftTextarea(card_info.index),
                        );

                        // Register button hit regions after card and textarea so they take precedence
                        hit_registry.register(
                            layout.repository_button,
                            MouseAction::ActivateRepositoryModal,
                        );
                        hit_registry
                            .register(layout.branch_button, MouseAction::ActivateBranchModal);
                        hit_registry.register(layout.model_button, MouseAction::ActivateModelModal);
                        hit_registry.register(layout.go_button, MouseAction::LaunchTask);
                        hit_registry.register(
                            layout.advanced_options_button,
                            MouseAction::ActivateAdvancedOptionsModal,
                        );
                        // Debug: log registered hit regions
                        debug!("Registered button hit regions:");
                        debug!("  Repository: {:?}", layout.repository_button);
                        debug!("  Branch: {:?}", layout.branch_button);
                        debug!("  Model: {:?}", layout.model_button);
                        debug!("  Go: {:?}", layout.go_button);
                        debug!("  Advanced Options: {:?}", layout.advanced_options_button);
                    }
                }
            }
        }
    }

    // Render footer
    if footer_y < size.height {
        let footer_area = Rect {
            x: 0,
            y: footer_y,
            width: size.width,
            height: 1,
        };
        render_footer(frame, footer_area, view_model, &theme);
    }

    // Render bottom padding
    if padding_y < size.height {
        let padding_area = Rect {
            x: 0,
            y: padding_y,
            width: size.width,
            height: size.height - padding_y,
        };
        let padding = Paragraph::new("").style(Style::default().bg(theme.bg));
        frame.render_widget(padding, padding_area);
    }

    // Render autocomplete menu if textarea area is available
    if let (Some(area), Some(card)) = (
        view_model.last_textarea_area,
        view_model.draft_cards.first(),
    ) {
        if let Some(ghost) = view_model.autocomplete.ghost_state() {
            render_autocomplete_ghost(frame, area, &card.description, ghost, &theme);
        }
        render_autocomplete(
            view_model.autocomplete.menu_state(),
            frame,
            area,
            &card.description,
            &theme,
            theme.surface,
            hit_registry,
        );
    }

    // Set cursor position for focused textarea (Ratatui hides cursor by default each frame)
    use crate::view_model::DashboardFocusState;
    use crate::view_model::task_entry::CardFocusElement;

    // Check if a textarea is currently focused
    let textarea_focused = if let DashboardFocusState::DraftTask(idx) = view_model.focus_element {
        view_model
            .draft_cards
            .get(idx)
            .is_some_and(|card| card.focus_element == CardFocusElement::TaskDescription)
    } else {
        false
    };

    if textarea_focused {
        if let (Some(textarea_rect), Some(card)) = (
            cache.focused_textarea_rect,
            view_model.get_focused_draft_card(),
        ) {
            let (cursor_row, cursor_col) = card.description.cursor();

            // Calculate absolute screen position within the textarea
            // Account for any block borders/padding
            let inner_rect = card
                .description
                .block()
                .map(|block| block.inner(textarea_rect))
                .unwrap_or(textarea_rect);

            let cursor_x = inner_rect.x + cursor_col as u16;
            let cursor_y = inner_rect.y + cursor_row as u16;

            // Tell Ratatui to show the cursor at this position
            frame.set_cursor_position((cursor_x, cursor_y));
        }
    }
    // If no textarea is focused, Ratatui will hide the cursor by default
}

// Helper function to find the textarea area for a given card (needed for cursor positioning).
// Currently unused in production rendering; kept for forthcoming precise caret tracking work.
#[allow(dead_code)]
fn find_textarea_area_for_card(
    _view_model: &ViewModel,
    _card: &TaskEntryViewModel,
    tasks_area: Rect,
) -> Option<Rect> {
    // For draft cards, the textarea is positioned with left/right padding of 1
    // and starts after the top border + top padding
    // This is a simplified calculation - in a full implementation you'd track exact positions
    Some(Rect::new(
        tasks_area.x + 1,                   // Left padding
        tasks_area.y + 1,                   // Top border + top padding
        tasks_area.width.saturating_sub(2), // Left + right padding
        5, // Approximate visible lines - should match actual calculation
    ))
}

/// Render a task card (exact same as main.rs TaskCard::render for Active/Completed/Merged)
fn render_task_card(
    frame: &mut Frame<'_>,
    area: Rect,
    card: &TaskExecutionViewModel,
    theme: &Theme,
    is_selected: bool,
) {
    let display_title = match card.card_type {
        TaskCardType::Active { .. } => {
            // Active cards just show the title
            if card.title.len() > 40 {
                format!("{}...", &card.title[..37])
            } else {
                card.title.clone()
            }
        }
        TaskCardType::Completed { .. } | TaskCardType::Merged { .. } => {
            // Completed/merged cards show checkmark + title
            let title = if card.title.len() > 35 {
                // Leave space for checkmark
                format!("{}...", &card.title[..32])
            } else {
                card.title.clone()
            };
            format!("✓ {}", title)
        }
    };

    let card_block = theme.card_block(&display_title);

    // Apply selection highlighting
    let final_card_block = if is_selected {
        card_block.border_style(Style::default().fg(theme.primary).add_modifier(Modifier::BOLD))
    } else {
        card_block
    };

    let inner_area = final_card_block.inner(area);
    frame.render_widget(final_card_block, area);

    // Use the same logic as main.rs for different task states
    match card.task.state {
        // Map running/intermediate states to active card rendering
        TaskState::Queued
        | TaskState::Provisioning
        | TaskState::Running
        | TaskState::Pausing
        | TaskState::Paused
        | TaskState::Resuming
        | TaskState::Stopping
        | TaskState::Stopped => render_active_task_card(frame, inner_area, card, theme),
        // Map final states to completed card rendering
        TaskState::Completed | TaskState::Failed | TaskState::Cancelled => {
            render_completed_task_card(frame, inner_area, card, theme)
        }
        TaskState::Merged => render_completed_task_card(frame, inner_area, card, theme), // Same rendering as completed
        TaskState::Draft => {} // Should not happen for task cards
    }
}

/// Render active task card (exact same as main.rs TaskCard::render_active_card)
fn render_active_task_card(
    frame: &mut Frame<'_>,
    area: Rect,
    card: &TaskExecutionViewModel,
    theme: &Theme,
) {
    // First line: metadata on left, Stop button on right
    let agents_text = if card.metadata.models.is_empty() {
        "No agents".to_string()
    } else if card.metadata.models.len() == 1 {
        format!(
            "{:?} (x{})",
            card.metadata.models[0].agent.software, card.metadata.models[0].count
        )
    } else {
        let agent_strings: Vec<String> = card
            .metadata
            .models
            .iter()
            .map(|model| format!("{:?} (x{})", model.agent.software, model.count))
            .collect();
        agent_strings.join(", ")
    };

    let metadata_part = vec![
        Span::styled(
            "● ",
            Style::default().fg(theme.warning).add_modifier(Modifier::BOLD),
        ),
        Span::styled(&card.metadata.repository, Style::default().fg(theme.muted)),
        Span::raw(" • "),
        Span::styled(&card.metadata.branch, Style::default().fg(theme.muted)),
        Span::raw(" • "),
        Span::styled(&agents_text, Style::default().fg(theme.muted)),
        Span::raw(" • "),
        Span::styled(&card.metadata.timestamp, Style::default().fg(theme.muted)),
    ];

    // Calculate how much space we need for the right-aligned Stop button
    let metadata_text = format!(
        "● {} • {} • {} • {}",
        card.metadata.repository, card.metadata.branch, agents_text, card.metadata.timestamp
    );
    let stop_button_text = " Stop ";
    let total_width = area.width as usize;

    // Create the full line with metadata left-aligned and Stop right-aligned
    let mut line_spans = metadata_part;

    // Add spacer to push Stop button to the right
    let used_width = metadata_text.len() + stop_button_text.len();
    if total_width > used_width {
        let spacer_width = total_width - used_width;
        line_spans.push(Span::raw(" ".repeat(spacer_width)));
    }

    // Add the Stop button with focus styling
    let stop_style = if matches!(card.focus_element, TaskExecutionFocusState::StopButton) {
        Style::default().fg(theme.bg).bg(theme.error).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.error).bg(theme.surface).add_modifier(Modifier::BOLD)
    };
    line_spans.push(Span::styled(stop_button_text, stop_style));

    let title_line = Line::from(line_spans);

    // Activity lines - display activity entries
    let activity_lines: Vec<Line> = if let TaskCardType::Active {
        activity_entries, ..
    } = &card.card_type
    {
        // Convert the activity entries to display lines (may be multiple lines per entry)
        let mut all_lines = Vec::new();
        for entry in activity_entries.iter().rev().take(3).rev() {
            match entry {
                AgentActivityRow::AgentThought { thought } => {
                    all_lines.push(Line::from(vec![
                        Span::styled("💭", Style::default().fg(theme.muted)),
                        Span::raw(" "),
                        Span::styled(thought.clone(), Style::default().fg(theme.text)),
                    ]));
                }
                AgentActivityRow::AgentEdit {
                    file_path,
                    lines_added,
                    lines_removed,
                    description,
                } => {
                    let desc = if let Some(desc) = description.as_ref() {
                        desc.clone()
                    } else {
                        format!(
                            "Modified {} (+{}, -{})",
                            file_path, lines_added, lines_removed
                        )
                    };
                    all_lines.push(Line::from(vec![
                        Span::styled("📝", Style::default().fg(theme.accent)),
                        Span::raw(" "),
                        Span::styled(desc, Style::default().fg(theme.text)),
                    ]));
                }
                AgentActivityRow::ToolUse {
                    tool_name,
                    last_line,
                    completed,
                    status,
                    ..
                } => {
                    if *completed {
                        // Completed tool: show final result
                        let status_icon = match status {
                            ToolStatus::Completed => "✅",
                            ToolStatus::Failed => "❌",
                            ToolStatus::Started => "⚠", // Shouldn't happen for completed tools
                        };
                        let result_text = if let Some(line) = last_line.as_ref() {
                            line.clone()
                        } else {
                            "Completed".to_string()
                        };
                        all_lines.push(Line::from(vec![
                            Span::styled(
                                status_icon,
                                Style::default().fg(match status {
                                    ToolStatus::Completed => theme.success,
                                    ToolStatus::Failed => theme.error,
                                    ToolStatus::Started => theme.warning,
                                }),
                            ),
                            Span::raw(" "),
                            Span::styled(
                                format!("{}: {}", tool_name, result_text),
                                Style::default().fg(theme.text),
                            ),
                        ]));
                    } else if let Some(line) = last_line {
                        // Tool with output: show tool name + indented output (two lines)
                        all_lines.push(Line::from(vec![
                            Span::styled("🔧", Style::default().fg(theme.primary)),
                            Span::raw(" "),
                            Span::styled(
                                format!("Tool usage: {}", tool_name),
                                Style::default().fg(theme.text),
                            ),
                        ]));
                        all_lines.push(Line::from(vec![
                            Span::raw("  "),
                            Span::styled(line.clone(), Style::default().fg(theme.text)),
                        ]));
                    } else {
                        // Tool just started: show tool name only
                        all_lines.push(Line::from(vec![
                            Span::styled("🔧", Style::default().fg(theme.primary)),
                            Span::raw(" "),
                            Span::styled(
                                format!("Tool usage: {}", tool_name),
                                Style::default().fg(theme.text),
                            ),
                        ]));
                    }
                }
                AgentActivityRow::AgentRead { file_path, range } => {
                    let desc = range
                        .as_ref()
                        .map(|r| format!("Read {} ({})", file_path, r))
                        .unwrap_or_else(|| format!("Read {}", file_path));
                    all_lines.push(Line::from(vec![
                        Span::styled("📖", Style::default().fg(theme.accent)),
                        Span::raw(" "),
                        Span::styled(desc, Style::default().fg(theme.text)),
                    ]));
                }
                AgentActivityRow::AgentDeleted {
                    file_path,
                    lines_removed,
                } => {
                    let desc = format!("Deleted {} (-{})", file_path, lines_removed);
                    all_lines.push(Line::from(vec![
                        Span::styled("🗑", Style::default().fg(theme.error)),
                        Span::raw(" "),
                        Span::styled(desc, Style::default().fg(theme.text)),
                    ]));
                }
                AgentActivityRow::UserInput {
                    author, content, ..
                } => {
                    all_lines.push(Line::from(vec![
                        Span::styled("👤", Style::default().fg(theme.primary)),
                        Span::raw(" "),
                        Span::styled(
                            format!("{}: {}", author, content),
                            Style::default().fg(theme.text),
                        ),
                    ]));
                }
            }
        }
        all_lines
    } else {
        // Fallback for non-active cards (shouldn't happen)
        vec![Line::from(vec![
            Span::styled("❓", Style::default().fg(theme.muted)),
            Span::raw(" "),
            Span::styled("No activity data", Style::default().fg(theme.text)),
        ])]
    };

    // Build all_lines: title + separator + activity lines + spacing + padding to fill inner area
    let mut all_lines = vec![title_line, Line::from("")]; // Title + empty separator line

    // Add activity lines (up to 3)
    let activity_count = activity_lines.len().min(3);
    for line in activity_lines.iter().take(activity_count) {
        all_lines.push(line.clone());
    }

    // Add spacing line
    all_lines.push(Line::from(""));

    // Pad with empty lines to fill the inner area (9 - 2 = 7 lines total)
    while all_lines.len() < 7 {
        all_lines.push(Line::from(""));
    }

    // Render all lines (7 lines to fill inner area)
    for (i, line) in all_lines.iter().enumerate() {
        let line_area = Rect {
            x: area.x, // ACTIVE_TASK_LEFT_PADDING = 0
            y: area.y + i as u16,
            width: area.width,
            height: 1,
        };
        let para = Paragraph::new(line.clone());
        frame.render_widget(para, line_area);
    }
}

/// Calculate the summary of changes from activity entries
fn calculate_change_summary(activity: &[String]) -> String {
    // Simulate realistic file change summaries based on activity count
    match activity.len() {
        0 => "0 files changed".to_string(),
        1 => "1 file changed (+5 -2)".to_string(),
        2 => "2 files changed (+12 -3)".to_string(),
        3 => "3 files changed (+25 -8)".to_string(),
        4 => "4 files changed (+42 -15)".to_string(),
        5.. => "6 files changed (+78 -23)".to_string(),
    }
}

/// Calculate the appropriate height for a task card based on its content
fn calculate_task_card_height(card: &TaskExecutionViewModel) -> u16 {
    match card.card_type {
        TaskCardType::Active { .. } => 9, // Title + separator + 3 activity lines + 1 spacing + borders/padding
        TaskCardType::Completed { .. } => 5, // Title + metadata + borders/padding
        TaskCardType::Merged { .. } => 5, // Title + metadata + borders/padding
    }
}

/// Render completed/merged task card (exact same as main.rs TaskCard::render_completed_card)
fn render_completed_task_card(
    frame: &mut Frame<'_>,
    area: Rect,
    card: &TaskExecutionViewModel,
    theme: &Theme,
) {
    // Parse delivery indicators and apply proper colors
    let delivery_spans = if card.metadata.delivery_indicators.is_empty() {
        vec![Span::styled("⎇ br", Style::default().fg(theme.primary))]
    } else {
        card.metadata
            .delivery_indicators
            .split_whitespace()
            .flat_map(|indicator| match indicator {
                "⎇" => vec![
                    Span::styled("⎇", Style::default().fg(Color::Cyan)),
                    Span::raw(" "),
                ],
                "⇄" => vec![
                    Span::styled("⇄", Style::default().fg(Color::Yellow)),
                    Span::raw(" "),
                ],
                "✓" => vec![
                    Span::styled("✓", Style::default().fg(Color::Green)),
                    Span::raw(" "),
                ],
                _ => vec![Span::raw(indicator), Span::raw(" ")],
            })
            .collect::<Vec<_>>()
    };

    let agents_text = if card.metadata.models.is_empty() {
        "No agents".to_string()
    } else if card.metadata.models.len() == 1 {
        format!(
            "{:?} (x{})",
            card.metadata.models[0].agent.software, card.metadata.models[0].count
        )
    } else {
        let agent_strings: Vec<String> = card
            .metadata
            .models
            .iter()
            .map(|model| format!("{:?} (x{})", model.agent.software, model.count))
            .collect();
        agent_strings.join(", ")
    };

    // Calculate summary of changes
    let change_summary = calculate_change_summary(&card.task.activity);

    let metadata_line = Line::from(vec![
        Span::styled(&card.metadata.repository, Style::default().fg(theme.muted)),
        Span::raw(" • "),
        Span::styled(&card.metadata.branch, Style::default().fg(theme.muted)),
        Span::raw(" • "),
        Span::styled(&agents_text, Style::default().fg(theme.muted)),
        Span::raw(" • "),
        Span::styled(&card.metadata.timestamp, Style::default().fg(theme.muted)),
        Span::raw(" • "),
    ]);

    // Add delivery indicators
    let mut metadata_spans = metadata_line.spans;
    if !delivery_spans.is_empty() {
        metadata_spans.push(Span::raw(" • "));
        metadata_spans.extend(delivery_spans);
    }

    // Add summary of changes
    metadata_spans.push(Span::raw(" • "));
    metadata_spans.push(Span::styled(
        change_summary,
        Style::default().fg(theme.muted),
    ));

    let metadata_line = Line::from(metadata_spans);

    let paragraph = Paragraph::new(vec![metadata_line]).wrap(Wrap { trim: true });

    frame.render_widget(paragraph, area);
}

fn render_filter_bar(
    frame: &mut Frame<'_>,
    area: Rect,
    view_model: &ViewModel,
    theme: &Theme,
    cache: &mut ViewCache,
    hit_registry: &mut HitTestRegistry<MouseAction>,
) {
    let repo_label = "All".to_string(); // TODO: Get from view_model
    let status_label = "All".to_string(); // TODO: Get from view_model
    let creator_label = "All".to_string(); // TODO: Get from view_model

    let is_separator_focused = matches!(
        view_model.focus_element,
        DashboardFocusState::FilterBarSeparator
    );
    let border_style = if is_separator_focused {
        Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.border)
    };
    let header_style = Style::default().fg(theme.muted);

    fn push_span(spans: &mut Vec<Span>, consumed: &mut usize, text: &str, style: Style) {
        *consumed += text.len();
        spans.push(Span::styled(text.to_string(), style));
    }

    let mut spans: Vec<Span> = Vec::new();
    let mut consumed = 0usize;
    let _start_x = area.x as usize;

    push_span(&mut spans, &mut consumed, "─ ", border_style);
    push_span(
        &mut spans,
        &mut consumed,
        "Existing tasks",
        header_style.add_modifier(Modifier::BOLD),
    );
    push_span(&mut spans, &mut consumed, "  ", Style::default());

    let repo_style = if matches!(view_model.focus_element, DashboardFocusState::Filter(_)) {
        theme.focused_style()
    } else {
        Style::default().fg(theme.text)
    };
    push_span(&mut spans, &mut consumed, "Repo ", header_style);
    push_span(
        &mut spans,
        &mut consumed,
        &format!("[{}]", repo_label),
        repo_style,
    );

    push_span(&mut spans, &mut consumed, "  ", Style::default());

    let status_style = Style::default().fg(theme.text); // TODO: match focus
    push_span(&mut spans, &mut consumed, "Status ", header_style);
    push_span(
        &mut spans,
        &mut consumed,
        &format!("[{}]", status_label),
        status_style,
    );

    push_span(&mut spans, &mut consumed, "  ", Style::default());

    let creator_style = Style::default().fg(theme.text); // TODO: match focus
    push_span(&mut spans, &mut consumed, "Creator ", header_style);
    push_span(
        &mut spans,
        &mut consumed,
        &format!("[{}]", creator_label),
        creator_style,
    );

    let line_width = area.width as usize;
    if consumed < line_width {
        let remaining = line_width - consumed + 2;
        push_span(
            &mut spans,
            &mut consumed,
            cache.get_separator(remaining as u16),
            border_style,
        );
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);

    hit_registry.register(area, MouseAction::SelectFilterBarLine);
}

fn render_footer(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel, theme: &Theme) {
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

    // Get shortcuts from view_model and render them
    for (index, shortcut) in view_model.footer.shortcuts.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled(
                bullet.to_string(),
                Style::default().fg(theme.muted),
            ));
        }

        // Get display strings for the shortcut
        let display_strings = shortcut.display_strings();
        let key_display = if display_strings.is_empty() {
            "?".to_string()
        } else {
            display_strings[0].clone()
        };

        // Style based on operation type
        let style = match shortcut.operation {
            crate::settings::KeyboardOperation::MoveToNextLine
            | crate::settings::KeyboardOperation::MoveToPreviousLine => theme.text_style(),
            crate::settings::KeyboardOperation::IndentOrComplete
            | crate::settings::KeyboardOperation::OpenNewLine => theme.success_style(),
            crate::settings::KeyboardOperation::DeleteCharacterBackward
            | crate::settings::KeyboardOperation::DeleteToBeginningOfLine => theme.error_style(),
            _ => theme.warning_style(),
        };

        spans.push(Span::styled(key_display, theme.primary_style()));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            shortcut.operation.english_description().to_string(),
            style,
        ));
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(theme.bg)),
        footer_area,
    );
}

/// Format a DraftSaveState for display.
/// Currently unused; retained for future status footer enhancements.
#[allow(dead_code)]
fn format_save_state(state: &DraftSaveState) -> String {
    match state {
        DraftSaveState::Unsaved => "Unsaved".to_string(),
        DraftSaveState::Saving => "Saving...".to_string(),
        DraftSaveState::Saved => "Saved".to_string(),
        DraftSaveState::Error => "Error".to_string(),
    }
}
