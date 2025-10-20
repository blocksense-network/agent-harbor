//! UI components for the TUI application

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Padding, Paragraph},
};
use ratatui_image::{StatefulImage, picker::Picker, protocol::StatefulProtocol};

use crate::task::{Task, TaskState};
use crate::viewmodel::ViewModel;

/// Charm-inspired theme with cohesive colors and styling
#[derive(Debug, Clone)]
pub struct Theme {
    pub bg: Color,
    pub surface: Color,
    pub text: Color,
    pub muted: Color,
    pub primary: Color,
    pub accent: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub border: Color,
    pub border_focused: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            // Dark theme inspired by Catppuccin Mocha with Charm aesthetics
            bg: Color::Rgb(17, 17, 27),                // Base background
            surface: Color::Rgb(24, 24, 37),           // Card/surface background
            text: Color::Rgb(205, 214, 244),           // Main text
            muted: Color::Rgb(127, 132, 156),          // Secondary text
            primary: Color::Rgb(137, 180, 250),        // Blue for primary actions
            accent: Color::Rgb(166, 218, 149),         // Green for success/accent
            success: Color::Rgb(166, 218, 149),        // Green
            warning: Color::Rgb(250, 179, 135),        // Orange/yellow
            error: Color::Rgb(243, 139, 168),          // Red/pink
            border: Color::Rgb(49, 50, 68),            // Subtle borders
            border_focused: Color::Rgb(137, 180, 250), // Blue for focus
        }
    }
}

impl Theme {
    /// High contrast theme for accessibility
    pub fn high_contrast() -> Self {
        Self {
            bg: Color::Rgb(0, 0, 0),
            surface: Color::Rgb(20, 20, 20),
            text: Color::Rgb(255, 255, 255),
            muted: Color::Rgb(180, 180, 180),
            primary: Color::Rgb(100, 200, 255),
            accent: Color::Rgb(100, 255, 100),
            success: Color::Rgb(100, 255, 100),
            warning: Color::Rgb(255, 200, 100),
            error: Color::Rgb(255, 100, 100),
            border: Color::Rgb(100, 100, 100),
            border_focused: Color::Rgb(150, 200, 255),
        }
    }

    /// Create a card block with Charm-style rounded borders and padding
    pub fn card_block(&self, title: &str) -> Block {
        let title_line = Line::from(vec![
            Span::raw("┤").fg(self.border),
            Span::raw(format!(" {} ", title))
                .style(Style::default().fg(self.text).add_modifier(Modifier::BOLD)),
            Span::raw("├").fg(self.border),
        ]);

        Block::default()
            .title(title_line)
            .title_alignment(ratatui::layout::Alignment::Left)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.border))
            .padding(Padding::new(2, 2, 1, 1))
            .style(Style::default().bg(self.bg))
    }

    /// Style for focused elements
    pub fn focused_style(&self) -> Style {
        Style::default().bg(self.primary).fg(Color::Black).add_modifier(Modifier::BOLD)
    }

    /// Style for selected elements
    pub fn selected_style(&self) -> Style {
        Style::default().bg(self.primary).fg(Color::Black)
    }

    /// Style for success elements
    pub fn success_style(&self) -> Style {
        Style::default().fg(self.success)
    }

    /// Style for warning elements
    pub fn warning_style(&self) -> Style {
        Style::default().fg(self.warning)
    }

    /// Style for error elements
    pub fn error_style(&self) -> Style {
        Style::default().fg(self.error)
    }
}

// Old dashboard function removed - replaced with draw_task_dashboard

/// Draw a selector widget with filtering
fn draw_selector_with_filter(
    f: &mut ratatui::Frame,
    area: Rect,
    title: &str,
    items: &[&str],
    filter: &str,
    is_focused: bool,
    is_loading: bool,
    state: &mut ListState,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Filter input
            Constraint::Min(1),    // List
        ])
        .split(area);

    // Filter input
    let filter_text = if filter.is_empty() {
        format!("Filter {}...", title.to_lowercase())
    } else {
        filter.to_string()
    };

    let filter_style = if is_focused && filter.is_empty() {
        Style::default().fg(Color::DarkGray)
    } else if is_focused {
        Style::default().fg(Color::White).bg(Color::Blue)
    } else {
        Style::default().fg(Color::White)
    };

    let filter_paragraph = Paragraph::new(filter_text).style(filter_style).block(
        Block::default()
            .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
            .title(format!("{} Filter", title)),
    );

    f.render_widget(filter_paragraph, chunks[0]);

    // Filtered items
    let filtered_items: Vec<&str> = if filter.is_empty() {
        items.to_vec()
    } else {
        items
            .iter()
            .filter(|item| item.to_lowercase().contains(&filter.to_lowercase()))
            .copied()
            .collect()
    };

    let list_items: Vec<ListItem> = if is_loading {
        vec![ListItem::new("Loading...")]
    } else if filtered_items.is_empty() && !filter.is_empty() {
        vec![ListItem::new("No matches found")]
    } else {
        filtered_items.iter().map(|item| ListItem::new(*item)).collect()
    };

    let block_style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };

    let list = List::new(list_items)
        .block(
            Block::default()
                .borders(Borders::BOTTOM | Borders::LEFT | Borders::RIGHT)
                .style(block_style)
                .title(title),
        )
        .highlight_style(
            Style::default().fg(Color::Black).bg(Color::White).add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ");

    f.render_stateful_widget(list, chunks[1], state);
}

/// Draw the task description editor
fn draw_task_editor(f: &mut ratatui::Frame, area: Rect, description: &str, is_focused: bool) {
    let block_style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };

    let paragraph = Paragraph::new(description)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .style(block_style)
                .title("Task Description (Ctrl+Up/Down to resize)"),
        )
        .wrap(ratatui::widgets::Wrap { trim: true });

    f.render_widget(paragraph, area);
}

/// Draw the new task-centric dashboard with Charm-inspired theming
pub fn draw_task_dashboard(
    f: &mut ratatui::Frame,
    area: Rect,
    view_model: &ViewModel,
    image_picker: Option<&mut Picker>,
    logo_protocol: Option<&mut StatefulProtocol>,
) {
    let theme = Theme::default();

    // Background fill with theme color
    let bg = Paragraph::new("").style(Style::default().bg(theme.bg));
    f.render_widget(bg, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(9), // Header with logo (10% larger for better visibility)
            Constraint::Min(1),    // Previous tasks (takes remaining space)
            Constraint::Length(6), // New task entry area at bottom
            Constraint::Length(1), // Footer with shortcuts (single line, no borders)
        ])
        .split(area);

    // Draw header (smaller, left-aligned)
    draw_header(f, chunks[0], &theme, image_picker, logo_protocol);

    // Add left/right padding to content areas (2 columns each side)
    let previous_tasks_padded = if chunks[1].width >= 6 {
        let horizontal_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(2), // Left padding
                Constraint::Min(1),    // Content area
                Constraint::Length(2), // Right padding
            ])
            .split(chunks[1]);
        horizontal_chunks[1]
    } else {
        chunks[1]
    };

    let new_task_padded = if chunks[2].width >= 6 {
        let horizontal_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(2), // Left padding
                Constraint::Min(1),    // Content area
                Constraint::Length(2), // Right padding
            ])
            .split(chunks[2]);
        horizontal_chunks[1]
    } else {
        chunks[2]
    };

    let footer_padded = if chunks[3].width >= 6 {
        let horizontal_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(2), // Left padding
                Constraint::Min(1),    // Content area
                Constraint::Length(2), // Right padding
            ])
            .split(chunks[3]);
        horizontal_chunks[1]
    } else {
        chunks[3]
    };

    // Draw previous tasks (excluding new task)
    draw_previous_tasks(f, previous_tasks_padded, view_model, &theme);

    // Draw new task entry at bottom
    draw_new_task_entry(f, new_task_padded, view_model, &theme);

    // Draw footer with shortcuts (single line like Lazygit)
    draw_task_footer(f, footer_padded, view_model, &theme);

    // Draw modal if active
    if let Some(selected_task) = view_model.selected_task() {
        if let crate::task::TaskState::New {
            modal_state: Some(modal),
            ..
        } = &selected_task.state
        {
            draw_modal(f, area, modal, &theme);
        }
    }
}

/// Draw the header (smaller, left-aligned)
fn draw_header(
    f: &mut ratatui::Frame,
    area: Rect,
    theme: &Theme,
    image_picker: Option<&mut Picker>,
    logo_protocol: Option<&mut StatefulProtocol>,
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

    // Try to render the logo as an image first
    if let Some(protocol) = logo_protocol {
        // Render the logo image using StatefulImage widget in the padded area
        let image_widget = StatefulImage::default();
        f.render_stateful_widget(image_widget, content_area, protocol);

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
    let ascii_logo = generate_ascii_logo();

    // Limit to available content area height
    let mut lines = Vec::new();
    for (i, line) in ascii_logo.iter().enumerate() {
        if i >= content_area.height as usize {
            break;
        }
        lines.push(line.clone());
    }

    let paragraph = Paragraph::new(lines).alignment(ratatui::layout::Alignment::Left);

    f.render_widget(paragraph, content_area);
}

/// Draw modal dialogs with Charm styling
fn draw_modal(
    f: &mut ratatui::Frame,
    area: Rect,
    modal_state: &crate::task::ModalState,
    theme: &Theme,
) {
    let modal_width = 60;
    let modal_height = 12;
    let modal_x = (area.width.saturating_sub(modal_width)) / 2;
    let modal_y = (area.height.saturating_sub(modal_height)) / 2;
    let modal_area = Rect::new(modal_x, modal_y, modal_width, modal_height);

    // Shadow effect (offset darker rectangle)
    let mut shadow_area = modal_area;
    shadow_area.x += 1;
    shadow_area.y += 1;
    let shadow = Block::default().style(Style::default().bg(Color::Rgb(10, 10, 15)));
    f.render_widget(Clear, shadow_area);
    f.render_widget(shadow, shadow_area);

    // Main modal with Charm styling
    let title_line = Line::from(vec![
        Span::raw("").fg(theme.primary),
        Span::raw(" Select ").style(Style::default().fg(theme.text).add_modifier(Modifier::BOLD)),
        Span::raw("").fg(theme.primary),
    ]);

    let block = Block::default()
        .title(title_line)
        .title_alignment(ratatui::layout::Alignment::Left)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_focused))
        .padding(Padding::new(1, 1, 0, 0))
        .style(Style::default().bg(theme.surface));

    f.render_widget(Clear, modal_area);
    f.render_widget(&block, modal_area);

    let inner_area = block.inner(modal_area);

    match modal_state {
        crate::task::ModalState::RepositorySelect {
            query,
            options,
            selected_index,
        } => {
            let title_area = Rect::new(inner_area.x, inner_area.y, inner_area.width, 1);
            let query_area = Rect::new(inner_area.x, inner_area.y + 1, inner_area.width, 1);
            let options_area = Rect::new(
                inner_area.x,
                inner_area.y + 3,
                inner_area.width,
                inner_area.height - 3,
            );

            // Title (using theme)
            f.render_widget(
                Paragraph::new("Repository Selection")
                    .style(Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
                title_area,
            );

            // Query input
            let query_text = if query.is_empty() {
                Span::styled(
                    "Type to search repositories...",
                    Style::default().fg(theme.muted),
                )
            } else {
                Span::styled(query.clone(), Style::default().fg(theme.text))
            };
            f.render_widget(Paragraph::new(query_text), query_area);

            // Options list
            let items: Vec<ListItem> = options
                .iter()
                .enumerate()
                .map(|(idx, repo)| {
                    let is_selected = idx == *selected_index;
                    let style = if is_selected {
                        theme.selected_style()
                    } else {
                        Style::default().fg(theme.text)
                    };
                    ListItem::new(repo.clone()).style(style)
                })
                .collect();

            let list = List::new(items).block(Block::default().borders(Borders::NONE));
            f.render_widget(list, options_area);
        }
        crate::task::ModalState::BranchSelect {
            query,
            options,
            selected_index,
        } => {
            let title_area = Rect::new(inner_area.x, inner_area.y, inner_area.width, 1);
            let query_area = Rect::new(inner_area.x, inner_area.y + 1, inner_area.width, 1);
            let options_area = Rect::new(
                inner_area.x,
                inner_area.y + 3,
                inner_area.width,
                inner_area.height - 3,
            );

            // Title (using theme)
            f.render_widget(
                Paragraph::new("Branch Selection")
                    .style(Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
                title_area,
            );

            // Query input
            let query_text = if query.is_empty() {
                Span::styled(
                    "Type to search branches...",
                    Style::default().fg(theme.muted),
                )
            } else {
                Span::styled(query.clone(), Style::default().fg(theme.text))
            };
            f.render_widget(Paragraph::new(query_text), query_area);

            // Options list
            let items: Vec<ListItem> = options
                .iter()
                .enumerate()
                .map(|(idx, branch)| {
                    let is_selected = idx == *selected_index;
                    let style = if is_selected {
                        theme.selected_style()
                    } else {
                        Style::default().fg(theme.text)
                    };
                    ListItem::new(branch.clone()).style(style)
                })
                .collect();

            let list = List::new(items).block(Block::default().borders(Borders::NONE));
            f.render_widget(list, options_area);
        }
        crate::task::ModalState::ModelSelect {
            query,
            options,
            selected_index,
        } => {
            let title_area = Rect::new(inner_area.x, inner_area.y, inner_area.width, 1);
            let query_area = Rect::new(inner_area.x, inner_area.y + 1, inner_area.width, 1);
            let options_area = Rect::new(
                inner_area.x,
                inner_area.y + 3,
                inner_area.width,
                inner_area.height - 3,
            );

            // Title (using theme)
            f.render_widget(
                Paragraph::new("Model Selection")
                    .style(Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
                title_area,
            );

            // Query input
            let query_text = if query.is_empty() {
                Span::styled("Type to search models...", Style::default().fg(theme.muted))
            } else {
                Span::styled(query.clone(), Style::default().fg(theme.text))
            };
            f.render_widget(Paragraph::new(query_text), query_area);

            // Options list with instance counts
            let items: Vec<ListItem> = options
                .iter()
                .enumerate()
                .map(|(idx, model)| {
                    let is_selected = idx == *selected_index;
                    let display_text = if model.instance_count > 0 {
                        format!("{} (x{})", model.model_name, model.instance_count)
                    } else {
                        model.model_name.clone()
                    };
                    let style = if is_selected {
                        theme.selected_style()
                    } else {
                        Style::default().fg(theme.text)
                    };
                    ListItem::new(display_text).style(style)
                })
                .collect();

            let list = List::new(items).block(Block::default().borders(Borders::NONE));
            f.render_widget(list, options_area);
        }
    }
}

/// Draw previous tasks as bordered cards (excluding the new task entry)
fn draw_previous_tasks(f: &mut ratatui::Frame, area: Rect, view_model: &ViewModel, theme: &Theme) {
    // Filter out the new task - only show previous tasks
    let previous_tasks: Vec<_> = view_model
        .tasks
        .iter()
        .filter(|task| !matches!(task.state, crate::task::TaskState::New { .. }))
        .collect();

    if previous_tasks.is_empty() {
        // Show a message when no previous tasks
        let empty_message = Paragraph::new("No previous tasks")
            .alignment(ratatui::layout::Alignment::Center)
            .style(Style::default().fg(theme.muted));
        f.render_widget(empty_message, area);
        return;
    }

    // Calculate height per task card (each gets equal space)
    let card_height = 4; // Fixed height for each task card
    let available_height = area.height as usize;
    let visible_cards = available_height / card_height;

    // Create layout for task cards
    let card_constraints: Vec<Constraint> = (0..visible_cards.min(previous_tasks.len()))
        .map(|_| Constraint::Length(card_height as u16))
        .collect();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(card_constraints)
        .split(area);

    // Render each task as a bordered card
    for (i, task) in previous_tasks.iter().take(visible_cards).enumerate() {
        let card_area = chunks[i];
        draw_task_card(f, card_area, task, theme);
    }
}

/// Draw a single task as a bordered card
fn draw_task_card(f: &mut ratatui::Frame, area: Rect, task: &crate::task::Task, theme: &Theme) {
    let is_selected = false; // For now, no selection in previous tasks

    // Get the card title from the task
    let card_title = task.display_title(70); // Max width for card title

    let content = match &task.state {
        crate::task::TaskState::Merged { title, merged_at } => {
            let time_ago = task.time_ago();
            let title = task.display_title(50); // Shorter for card format
            vec![
                Line::from(vec![
                    Span::styled("● ", Style::default().fg(theme.muted)),
                    Span::styled(title, Style::default().fg(theme.text)),
                ]),
                Line::from(vec![Span::styled(
                    format!("Merged {}", time_ago),
                    Style::default().fg(theme.muted),
                )]),
            ]
        }
        crate::task::TaskState::Completed { title, status, .. } => {
            let time_ago = task.time_ago();
            let title = task.display_title(50);
            let status_short = if status.len() > 30 {
                format!("{}...", &status[..27])
            } else {
                status.clone()
            };
            vec![
                Line::from(vec![
                    Span::styled("✓ ", theme.success_style()),
                    Span::styled(title, Style::default().fg(theme.text)),
                ]),
                Line::from(vec![Span::styled(
                    format!("{} • {}", status_short, time_ago),
                    Style::default().fg(theme.muted),
                )]),
            ]
        }
        crate::task::TaskState::Active {
            title,
            current_action,
            action_detail,
            progress,
            ..
        } => {
            let title = task.display_title(50);
            let progress_info = progress.as_ref().map(|p| format!(" [{}]", p)).unwrap_or_default();
            vec![
                Line::from(vec![
                    Span::styled("● ", Style::default().fg(theme.warning)),
                    Span::styled(title, Style::default().fg(theme.text)),
                ]),
                Line::from(vec![Span::styled(
                    format!("{}{}", current_action, progress_info),
                    Style::default().fg(theme.primary),
                )]),
            ]
        }
        crate::task::TaskState::Draft { description, .. } => {
            let title = task.display_title(50);
            let time_ago = task.time_ago();
            let desc_preview = if description.len() > 35 {
                format!("{}...", &description[..32])
            } else {
                description.clone()
            };
            vec![
                Line::from(vec![
                    Span::styled("📝 ", Style::default().fg(theme.warning)),
                    Span::styled(title, Style::default().fg(theme.text)),
                ]),
                Line::from(vec![Span::styled(
                    format!("{} • {}", desc_preview, time_ago),
                    Style::default().fg(theme.muted),
                )]),
            ]
        }
        crate::task::TaskState::New { .. } => {
            // Should not happen since we filter these out
            vec![Line::from("New Task")]
        }
    };

    // Render the card border first
    let card_block = theme.card_block(&card_title);
    let inner_area = card_block.inner(area);
    f.render_widget(card_block, area);

    // Render content directly (temporarily ignore padding for debugging)
    for (i, line) in content.iter().enumerate() {
        if i < 2 {
            // Just render first 2 lines for debugging
            let line_area = Rect::new(area.x + 2, area.y + 1 + i as u16, area.width - 4, 1);
            let para = Paragraph::new(line.clone());
            f.render_widget(para, line_area);
        }
    }
}

/// Draw the new task entry area at the bottom of the screen
fn draw_new_task_entry(f: &mut ratatui::Frame, area: Rect, view_model: &ViewModel, theme: &Theme) {
    // Find the new task
    if let Some(new_task) = view_model.tasks.last() {
        if let crate::task::TaskState::New {
            description,
            selected_repo,
            selected_branch,
            selected_models,
            focused_button,
            modal_state,
        } = &new_task.state
        {
            let is_selected = view_model.selected_task_index == view_model.tasks.len() - 1;

            let mut all_lines = Vec::new();

            // Description area - highlighted input field
            all_lines.push(Line::from(vec![
                Span::styled("┌─ Description ", Style::default().fg(theme.border)),
                Span::styled("─".repeat(65), Style::default().fg(theme.border)),
                Span::raw("─┐"),
            ]));

            // Description content area with background highlighting
            if description.is_empty() {
                all_lines.push(Line::from(vec![
                    Span::raw("│ "),
                    Span::styled(
                        "Enter task description...".to_string() + &" ".repeat(45),
                        Style::default().bg(theme.surface).fg(theme.muted),
                    ),
                    Span::raw(" │"),
                ]));
            } else {
                // Split description into lines if it contains newlines
                for line in description.lines() {
                    let padded_line = format!("│ {:<69} │", line);
                    all_lines.push(Line::from(vec![Span::styled(
                        padded_line,
                        Style::default().bg(theme.surface),
                    )]));
                }
                // Add empty line if description is short
                if description.lines().count() < 2 {
                    all_lines.push(Line::from(vec![Span::styled(
                        "│                                                                     │",
                        Style::default().bg(theme.surface),
                    )]));
                }
            }

            all_lines.push(Line::from(vec![
                Span::styled("└", Style::default().fg(theme.border)),
                Span::styled("─".repeat(77), Style::default().fg(theme.border)),
                Span::raw("─┘"),
            ]));

            // Empty line as separator
            all_lines.push(Line::from(""));

            // Button row at the bottom with Charm styling
            let repo_button_text = if selected_repo.is_empty() {
                "📁 Repository".to_string()
            } else {
                format!("📁 {}", selected_repo)
            };

            let branch_button_text = if selected_branch.is_empty() {
                "🌿 Branch".to_string()
            } else {
                format!("🌿 {}", selected_branch)
            };

            let models_button_text = if selected_models.is_empty() {
                "🤖 Models".to_string()
            } else {
                let total_instances: u32 = selected_models.iter().map(|m| m.instance_count).sum();
                format!("🤖 Models ({})", total_instances)
            };

            let go_button_text = "⏎ Go".to_string();

            // Create button spans with focus styling using theme
            let repo_button = if matches!(focused_button, crate::task::ButtonFocus::Repository) {
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

            let branch_button = if matches!(focused_button, crate::task::ButtonFocus::Branch) {
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

            let models_button = if matches!(focused_button, crate::task::ButtonFocus::Models) {
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

            let go_button = if matches!(focused_button, crate::task::ButtonFocus::Go) {
                Span::styled(
                    format!(" {} ", go_button_text),
                    Style::default().fg(Color::Black).bg(theme.accent).add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled(
                    format!(" {} ", go_button_text),
                    Style::default()
                        .fg(theme.accent)
                        .bg(theme.surface)
                        .add_modifier(Modifier::BOLD),
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

            all_lines.push(button_line);

            // Render the card border first
            let new_task_block = theme.card_block("New Task");
            let inner_area = new_task_block.inner(area);
            f.render_widget(new_task_block, area);

            // Render each line of content within the inner area
            for (i, line) in all_lines.iter().enumerate() {
                if i < inner_area.height as usize {
                    let line_area =
                        Rect::new(inner_area.x, inner_area.y + i as u16, inner_area.width, 1);
                    let para = Paragraph::new(line.clone());
                    f.render_widget(para, line_area);
                }
            }
        }
    }
}

fn draw_task_list(f: &mut ratatui::Frame, area: Rect, view_model: &ViewModel, theme: &Theme) {
    // Create the main task list area with Charm styling
    let title_line = Line::from(vec![
        Span::raw("").fg(theme.primary),
        Span::raw(" Recent Tasks ")
            .style(Style::default().fg(theme.text).add_modifier(Modifier::BOLD)),
        Span::raw("").fg(theme.primary),
    ]);

    let block = Block::default()
        .title(title_line)
        .title_alignment(ratatui::layout::Alignment::Left)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .padding(Padding::new(1, 1, 0, 0))
        .style(Style::default().bg(theme.surface));

    let inner_area = block.inner(area);

    f.render_widget(block, area);

    // Calculate available width for task titles (accounting for time indicators)
    let available_width = inner_area.width.saturating_sub(10) as usize; // Reserve space for time

    let task_items: Vec<ListItem> = view_model
        .tasks
        .iter()
        .enumerate()
        .map(|(idx, task)| {
            let is_selected = idx == view_model.selected_task_index;
            let content = match &task.state {
                TaskState::Merged { title, merged_at } => {
                    let time_ago = task.time_ago();
                    let title = task.display_title(available_width);
                    let line = Line::from(vec![
                        Span::styled("● ", Style::default().fg(theme.muted)),
                        Span::styled(title, Style::default().fg(theme.text)),
                        Span::raw(" "),
                        Span::styled(format!("({})", time_ago), Style::default().fg(theme.muted)),
                    ]);
                    if is_selected {
                        ListItem::new(line).style(theme.selected_style())
                    } else {
                        ListItem::new(line)
                    }
                }
                TaskState::Completed { title, status, .. } => {
                    let time_ago = task.time_ago();
                    let title = task.display_title(available_width);
                    let line = Line::from(vec![
                        Span::styled("✓ ", theme.success_style()),
                        Span::styled(title, Style::default().fg(theme.text)),
                        Span::raw(" "),
                        Span::styled(format!("({})", time_ago), Style::default().fg(theme.muted)),
                    ]);
                    if is_selected {
                        ListItem::new(line).style(theme.selected_style())
                    } else {
                        ListItem::new(line)
                    }
                }
                TaskState::Active {
                    title,
                    current_action,
                    action_detail,
                    progress,
                    ..
                } => {
                    let title = task.display_title(available_width);
                    let mut spans = vec![
                        Span::styled("● ", Style::default().fg(theme.warning)),
                        Span::styled(title, Style::default().fg(theme.text)),
                    ];

                    // Add progress info if available
                    if let Some(progress) = progress {
                        spans.push(Span::raw(" "));
                        spans.push(Span::styled(
                            format!("[{}]", progress),
                            Style::default().fg(theme.primary),
                        ));
                    }

                    let line1 = Line::from(spans);

                    // Second line with current action details
                    let line2 = Line::from(vec![Span::styled(
                        format!("  {}: {}", current_action, action_detail),
                        Style::default().fg(theme.muted),
                    )]);

                    let text = ratatui::text::Text::from(vec![line1, line2]);

                    if is_selected {
                        ListItem::new(text).style(theme.selected_style())
                    } else {
                        ListItem::new(text)
                    }
                }
                TaskState::Draft { description, .. } => {
                    let title = task.display_title(available_width);
                    let time_ago = task.time_ago();
                    let line = Line::from(vec![
                        Span::styled("📝 ", Style::default().fg(theme.warning)),
                        Span::styled(title, Style::default().fg(theme.muted)),
                        Span::raw(" "),
                        Span::styled(
                            format!("(draft {})", time_ago),
                            Style::default().fg(theme.muted),
                        ),
                    ]);
                    if is_selected {
                        ListItem::new(line).style(theme.selected_style())
                    } else {
                        ListItem::new(line)
                    }
                }
                TaskState::New {
                    description,
                    selected_repo,
                    selected_branch,
                    selected_models,
                    focused_button,
                    modal_state,
                } => {
                    let mut all_lines = Vec::new();

                    // Description area (auto-expandable text area)
                    if description.is_empty() {
                        all_lines.push(Line::from(vec![Span::styled(
                            "Enter task description...",
                            Style::default().fg(theme.muted),
                        )]));
                    } else {
                        // Split description into lines if it contains newlines
                        for line in description.lines() {
                            all_lines.push(Line::from(vec![Span::styled(
                                line,
                                Style::default().fg(theme.text),
                            )]));
                        }
                    }

                    // Empty line as separator
                    all_lines.push(Line::from(""));

                    // Button row at the bottom with Charm styling
                    let repo_button_text = if selected_repo.is_empty() {
                        "📁 Repository".to_string()
                    } else {
                        format!("📁 {}", selected_repo)
                    };

                    let branch_button_text = if selected_branch.is_empty() {
                        "🌿 Branch".to_string()
                    } else {
                        format!("🌿 {}", selected_branch)
                    };

                    let models_button_text = if selected_models.is_empty() {
                        "🤖 Models".to_string()
                    } else {
                        let total_instances: u32 =
                            selected_models.iter().map(|m| m.instance_count).sum();
                        format!("🤖 Models ({})", total_instances)
                    };

                    let go_button_text = "⏎ Go".to_string();

                    // Create button spans with focus styling using theme
                    let repo_button =
                        if matches!(focused_button, crate::task::ButtonFocus::Repository) {
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

                    let branch_button =
                        if matches!(focused_button, crate::task::ButtonFocus::Branch) {
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

                    let models_button =
                        if matches!(focused_button, crate::task::ButtonFocus::Models) {
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

                    let go_button = if matches!(focused_button, crate::task::ButtonFocus::Go) {
                        Span::styled(
                            format!(" {} ", go_button_text),
                            Style::default()
                                .fg(Color::Black)
                                .bg(theme.accent)
                                .add_modifier(Modifier::BOLD),
                        )
                    } else {
                        Span::styled(
                            format!(" {} ", go_button_text),
                            Style::default()
                                .fg(theme.accent)
                                .bg(theme.surface)
                                .add_modifier(Modifier::BOLD),
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

                    all_lines.push(button_line);

                    let text = ratatui::text::Text::from(all_lines);

                    if is_selected {
                        ListItem::new(text).style(Style::default().bg(Color::Blue))
                    } else {
                        ListItem::new(text)
                    }
                }
            };
            content
        })
        .collect();

    let list =
        List::new(task_items).highlight_style(Style::default().bg(Color::Blue).fg(Color::White));

    f.render_widget(list, inner_area);
}

/// Draw the footer with contextual shortcuts (single line like Lazygit)
fn draw_task_footer(f: &mut ratatui::Frame, area: Rect, view_model: &ViewModel, theme: &Theme) {
    let shortcuts = if let Some(selected_task) = view_model.selected_task() {
        if let crate::task::TaskState::New {
            modal_state: Some(_),
            ..
        } = &selected_task.state
        {
            // Modal is active - show modal navigation shortcuts
            vec![
                Span::styled("↑↓", theme.warning_style()),
                Span::raw(" Navigate • "),
                Span::styled("Enter", theme.success_style()),
                Span::raw(" Select • "),
                Span::styled("Esc", Style::default().fg(theme.muted)),
                Span::raw(" Back • "),
                Span::styled("Ctrl+C", theme.error_style()),
                Span::raw(" Abort"),
            ]
        } else if matches!(selected_task.state, crate::task::TaskState::New { .. }) {
            // Task creation interface (no modal) shortcuts
            vec![
                Span::styled("Tab", Style::default().fg(theme.primary)),
                Span::raw(" Cycle Buttons • "),
                Span::styled("Enter", theme.success_style()),
                Span::raw(" Activate Button • "),
                Span::styled("Esc", Style::default().fg(theme.muted)),
                Span::raw(" Back • "),
                Span::styled("Ctrl+C x2", theme.error_style()),
                Span::raw(" Quit"),
            ]
        } else {
            // Regular task navigation shortcuts
            vec![
                Span::styled("↑↓", theme.warning_style()),
                Span::raw(" Navigate • "),
                Span::styled("Enter", theme.success_style()),
                Span::raw(" Select Task • "),
                Span::styled("Ctrl+C x2", theme.error_style()),
                Span::raw(" Quit"),
            ]
        }
    } else {
        vec![
            Span::styled("↑↓", theme.warning_style()),
            Span::raw(" Navigate • "),
            Span::styled("Ctrl+C x2", theme.error_style()),
            Span::raw(" Quit"),
        ]
    };

    // Render as a simple line without borders (like Lazygit)
    let line = Line::from(shortcuts);
    let paragraph = Paragraph::new(line).style(Style::default().bg(theme.bg));

    f.render_widget(paragraph, area);
}

/// Draw the footer with contextual shortcuts
fn draw_footer(f: &mut ratatui::Frame, area: Rect, current_section: usize) {
    let shortcuts = match current_section {
        0 => vec![
            Span::styled("Tab", Style::default().fg(Color::Cyan)),
            Span::raw(" Next • "),
            Span::styled("↑↓", Style::default().fg(Color::Yellow)),
            Span::raw(" Navigate • "),
            Span::styled("Type", Style::default().fg(Color::Green)),
            Span::raw(" Filter • "),
            Span::styled("Ctrl+C", Style::default().fg(Color::Red)),
            Span::raw(" Quit"),
        ],
        1 => vec![
            Span::styled("Tab", Style::default().fg(Color::Cyan)),
            Span::raw(" Next • "),
            Span::styled("↑↓", Style::default().fg(Color::Yellow)),
            Span::raw(" Navigate • "),
            Span::styled("Type", Style::default().fg(Color::Green)),
            Span::raw(" Filter • "),
            Span::styled("Ctrl+C", Style::default().fg(Color::Red)),
            Span::raw(" Quit"),
        ],
        2 => vec![
            Span::styled("Tab", Style::default().fg(Color::Cyan)),
            Span::raw(" Next • "),
            Span::styled("↑↓", Style::default().fg(Color::Yellow)),
            Span::raw(" Navigate • "),
            Span::styled("Type", Style::default().fg(Color::Green)),
            Span::raw(" Filter • "),
            Span::styled("Ctrl+C", Style::default().fg(Color::Red)),
            Span::raw(" Quit"),
        ],
        3 => vec![
            Span::styled("Tab", Style::default().fg(Color::Cyan)),
            Span::raw(" Next • "),
            Span::styled("Ctrl+↑↓", Style::default().fg(Color::Yellow)),
            Span::raw(" Resize • "),
            Span::styled("Enter", Style::default().fg(Color::Green)),
            Span::raw(" Create Task • "),
            Span::styled("Ctrl+C", Style::default().fg(Color::Red)),
            Span::raw(" Quit"),
        ],
        _ => vec![
            Span::styled("Ctrl+C", Style::default().fg(Color::Red)),
            Span::raw(" Quit"),
        ],
    };

    let line = Line::from(shortcuts);
    let paragraph = Paragraph::new(line).block(Block::default().borders(Borders::ALL));

    f.render_widget(paragraph, area);
}

/// Generate ASCII logo for Agent Harbor
pub fn generate_ascii_logo() -> Vec<Line<'static>> {
    vec![
        Line::from(
            "╔══════════════════════════════════════════════════════════════════════════════╗",
        ),
        Line::from(
            "║                                                                              ║",
        ),
        Line::from(
            "║                           █████╗  ██████╗ ███████╗███╗   ██╗████████╗         ║",
        ),
        Line::from(
            "║                          ██╔══██╗██╔════╝ ██╔════╝████╗  ██║╚══██╔══╝         ║",
        ),
        Line::from(
            "║                          ███████║██║  ███╗█████╗  ██╔██╗ ██║   ██║            ║",
        ),
        Line::from(
            "║                          ██╔══██║██║   ██║██╔══╝  ██║╚██╗██║   ██║            ║",
        ),
        Line::from(
            "║                          ██║  ██║╚██████╔╝███████╗██║ ╚████║   ██║            ║",
        ),
        Line::from(
            "║                          ╚═╝  ╚═╝ ╚═════╝ ╚══════╝╚═╝  ╚═══╝   ╚═╝            ║",
        ),
        Line::from(
            "║                                                                              ║",
        ),
        Line::from(
            "║                              ██╗  ██╗ █████╗ ██████╗ ██████╗  ██████╗ ██████╗ ║",
        ),
        Line::from(
            "║                              ██║  ██║██╔══██╗██╔══██╗██╔══██╗██╔═══██╗██╔══██╗║",
        ),
        Line::from(
            "║                              ███████║███████║██████╔╝██████╔╝██║   ██║██████╔╝║",
        ),
        Line::from(
            "║                              ██╔══██║██╔══██║██╔══██╗██╔══██╗██║   ██║██╔══██╗║",
        ),
        Line::from(
            "║                              ██║  ██║██║  ██║██║  ██║██████╔╝╚██████╔╝██║  ██║║",
        ),
        Line::from(
            "║                              ╚═╝  ╚═╝╚═╝  ╚═╝╚═╝  ╚═╝╚═════╝  ╚═════╝ ╚═╝  ╚═╝║",
        ),
        Line::from(
            "║                                                                              ║",
        ),
        Line::from(
            "╚══════════════════════════════════════════════════════════════════════════════╝",
        ),
    ]
}

/// Draw a loading overlay
pub fn draw_loading(f: &mut ratatui::Frame, area: Rect, message: &str) {
    let popup_area = centered_rect(60, 20, area);

    f.render_widget(Clear, popup_area);

    let paragraph = Paragraph::new(message)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Loading")
                .style(Style::default().fg(Color::Yellow)),
        )
        .wrap(ratatui::widgets::Wrap { trim: true });

    f.render_widget(paragraph, popup_area);
}

/// Draw an error overlay
pub fn draw_error(f: &mut ratatui::Frame, area: Rect, error: &str) {
    let popup_area = centered_rect(80, 30, area);

    f.render_widget(Clear, popup_area);

    let paragraph = Paragraph::new(error)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Error")
                .style(Style::default().fg(Color::Red)),
        )
        .wrap(ratatui::widgets::Wrap { trim: true });

    f.render_widget(paragraph, popup_area);
}

/// Helper function to create a centered rectangle
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
