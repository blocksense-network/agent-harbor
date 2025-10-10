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

use ratatui::{prelude::*, widgets::*};
use crate::view_model::{ViewModel, TaskCardViewModel, TaskCardType, FooterViewModel};

/// Main rendering function - transforms ViewModel to Ratatui widgets
pub fn render(frame: &mut Frame<'_>, view_model: &ViewModel) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Min(1),     // Main content
            Constraint::Length(1),  // Footer
        ])
        .split(area);

    render_header(frame, chunks[0], view_model);
    render_main_content(frame, chunks[1], view_model);
    render_footer(frame, chunks[2], view_model);

    // Render modal if active
    if let Some(modal) = &view_model.active_modal {
        render_modal(frame, area, modal);
    }
}

fn render_header(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel) {
    let header = Paragraph::new(view_model.title.clone())
        .block(Block::bordered().title("Agent Harbor"))
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Cyan));

    frame.render_widget(header, area);
}

fn render_main_content(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),     // Task cards
            Constraint::Length(1),  // Filter bar
        ])
        .split(area);

    render_task_cards(frame, chunks[0], view_model);
    render_filter_bar(frame, chunks[1], view_model);
}

fn render_task_cards(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel) {
    if view_model.task_cards.is_empty() {
        let empty = Paragraph::new("No tasks")
            .block(Block::bordered())
            .alignment(Alignment::Center);
        frame.render_widget(empty, area);
        return;
    }

    // Simple vertical layout for task cards
    let card_constraints: Vec<Constraint> = view_model.task_cards.iter()
        .map(|card| Constraint::Length(card.height))
        .collect();

    if card_constraints.is_empty() {
        return;
    }

    let card_areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints(card_constraints)
        .split(area);

    for (i, card) in view_model.task_cards.iter().enumerate() {
        if i < card_areas.len() {
            render_task_card(frame, card_areas[i], card);
        }
    }
}

fn render_task_card(frame: &mut Frame<'_>, area: Rect, card: &TaskCardViewModel) {
    let block = Block::bordered()
        .title(card.title.clone())
        .border_style(if card.is_selected {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::Gray)
        });

    match &card.card_type {
        TaskCardType::Draft { description, controls, auto_save_indicator, .. } => {
            let content = if description.is_empty() {
                vec![
                    Line::from("Describe what you want the agent to do...").style(Style::default().fg(Color::DarkGray)),
                    Line::from(""),
                    Line::from(format!(
                        "[{}] [{}] [{}] [{}] - {}",
                        controls.repository_button.text,
                        controls.branch_button.text,
                        controls.model_button.text,
                        controls.go_button.text,
                        auto_save_indicator
                    ))
                ]
            } else {
                vec![
                    Line::from(description.clone()),
                    Line::from(""),
                    Line::from(format!(
                        "[{}] [{}] [{}] [{}] - {}",
                        controls.repository_button.text,
                        controls.branch_button.text,
                        controls.model_button.text,
                        controls.go_button.text,
                        auto_save_indicator
                    ))
                ]
            };

            let paragraph = Paragraph::new(content)
                .block(block)
                .wrap(Wrap { trim: true });
            frame.render_widget(paragraph, area);
        },
        TaskCardType::Active { activity_lines, .. } => {
            let mut content = vec![Line::from(card.metadata_line.clone())];
            content.push(Line::from(""));
            for activity in activity_lines {
                content.push(Line::from(activity.clone()));
            }

            let paragraph = Paragraph::new(content).block(block);
            frame.render_widget(paragraph, area);
        },
        TaskCardType::Completed { delivery_indicators } => {
            let content = vec![
                Line::from(format!("✓ {}", card.title)),
                Line::from(format!("{} • {}", card.metadata_line, delivery_indicators)),
            ];

            let paragraph = Paragraph::new(content).block(block);
            frame.render_widget(paragraph, area);
        },
        TaskCardType::Merged { delivery_indicators } => {
            let content = vec![
                Line::from(format!("✓ {}", card.title)),
                Line::from(format!("{} • {}", card.metadata_line, delivery_indicators)),
            ];

            let paragraph = Paragraph::new(content).block(block);
            frame.render_widget(paragraph, area);
        }
    }
}

fn render_filter_bar(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel) {
    let filter_text = format!(
        "Filter: [{}] [{}] Search: [{}]",
        view_model.filter_bar.status_filter.current_value,
        view_model.filter_bar.time_filter.current_value,
        view_model.filter_bar.search_box.value
    );

    let filter_bar = Paragraph::new(filter_text)
        .style(Style::default().fg(Color::Gray));
    frame.render_widget(filter_bar, area);
}

fn render_footer(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel) {
    let shortcuts_text = view_model.footer.shortcuts.iter()
        .map(|s| format!("{} {}", s.key, s.description))
        .collect::<Vec<_>>()
        .join(" • ");

    let footer = Paragraph::new(shortcuts_text)
        .style(Style::default().fg(Color::White));
    frame.render_widget(footer, area);
}

fn render_modal(frame: &mut Frame<'_>, _area: Rect, modal: &crate::view_model::ModalViewModel) {
    // Center the modal
    let modal_area = Rect {
        x: 10,
        y: 5,
        width: 50,
        height: 15,
    };

    // Clear the area
    frame.render_widget(Clear, modal_area);

    // Render modal content
    let content = vec![
        Line::from(modal.title.clone()),
        Line::from(""),
        Line::from(format!("Input: {}", modal.input_value)),
        Line::from(""),
        Line::from("Options:"),
    ];

    let modal_widget = Paragraph::new(content)
        .block(Block::bordered().title("Modal"))
        .alignment(Alignment::Left);
    frame.render_widget(modal_widget, modal_area);
}
