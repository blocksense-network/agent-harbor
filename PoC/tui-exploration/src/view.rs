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
use crate::view_model::{ViewModel, DraftCardViewModel, TaskCardViewModel, DraftSaveState, FocusElement, InteractiveArea, MouseAction, TaskCardTypeEnum, TaskCardType, ActivityEntry};
use crate::task_manager::{TaskEvent, TaskStatus, LogLevel, ToolStatus};
use ah_domain_types::TaskState;
use ratatui_image::protocol::StatefulProtocol;
use ratatui_image::StatefulImage;

/// Cache for view-related computations and state
pub struct ViewCache {
    // Image rendering state
    pub picker: Option<ratatui_image::picker::Picker>,
    pub logo_protocol: Option<ratatui_image::protocol::StatefulProtocol>,

    // Cached computed strings - only recompute if inputs changed
    last_separator_width: Option<u16>,
    cached_separator: Option<String>,
}

impl ViewCache {
    pub fn new() -> Self {
        ViewCache {
            picker: None,
            logo_protocol: None,
            last_separator_width: None,
            cached_separator: None,
        }
    }

    /// Get a cached separator string - only recomputes if width changed
    pub fn get_separator(&mut self, width: u16) -> &str {
        if self.last_separator_width != Some(width) {
            self.cached_separator = Some("─".repeat(width as usize));
            self.last_separator_width = Some(width);
        }
        self.cached_separator.as_ref().unwrap()
    }
}

/// Display item types (exact same as main.rs)
#[derive(Debug, Clone)]
enum DisplayItem {
    Task(String), // Task ID
    FilterBar,
    Spacer,
}

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
            border: Color::Rgb(69, 71, 90),            // Border color
            border_focused: Color::Rgb(137, 180, 250), // Focused border color
        }
    }
}

impl Theme {
    /// Create a card block with Charm-style rounded borders and padding (exact ah-tui style)
    fn card_block(&self, title: &str) -> Block {
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
            .padding(Padding::new(1, 1, 1, 1))
            .style(Style::default().bg(self.bg))
    }

    /// Create a card block with a right-aligned button in the title area
    fn card_block_with_button(
        &self,
        title: &str,
        button_text: &str,
        button_focused: bool,
    ) -> Block {
        let button_style = if button_focused {
            Style::default().fg(self.bg).bg(self.error).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(self.error).bg(self.surface).add_modifier(Modifier::BOLD)
        };

        let title_line = Line::from(vec![
            Span::raw("┤").fg(self.border),
            Span::raw(format!(" {} ", title))
                .style(Style::default().fg(self.text).add_modifier(Modifier::BOLD)),
            Span::raw("├").fg(self.border),
            Span::raw(" ".repeat(15)), // Spacer to push button to right
            Span::styled(format!(" {} ", button_text), button_style),
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

    /// Style for primary elements
    fn primary_style(&self) -> Style {
        Style::default().fg(self.primary).add_modifier(Modifier::BOLD)
    }

    /// Style for focused elements
    fn focused_style(&self) -> Style {
        Style::default().fg(self.bg).bg(self.primary).add_modifier(Modifier::BOLD)
    }

    /// Style for text elements
    fn text_style(&self) -> Style {
        Style::default().fg(self.text)
    }

    /// Style for success elements
    fn success_style(&self) -> Style {
        Style::default().fg(self.success)
    }

    /// Style for warning elements
    fn warning_style(&self) -> Style {
        Style::default().fg(self.warning)
    }

    /// Style for error elements
    fn error_style(&self) -> Style {
        Style::default().fg(self.error)
    }
}


/// Main rendering function - transforms ViewModel to Ratatui widgets (exact same as main.rs)
pub fn render(frame: &mut Frame<'_>, view_model: &mut ViewModel, view_cache: &mut ViewCache) {
    let theme = Theme::default();
    let size = frame.area();

    // Clear interactive areas before rendering (exact same as main.rs)
    view_model.interactive_areas.clear();

    // Background fill with theme color (exact same as main.rs)
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
        (min_header_height, size.height - min_header_height - footer_height - padding_height, size.height - footer_height - padding_height, size.height - padding_height)
    } else if size.height >= 10 {
        // Minimum viable layout
        let available = size.height - footer_height - padding_height;
        let header_actual = (available * 3 / 5).max(3); // 60% for header minimum 3
        let tasks_actual = available - header_actual;
        (header_actual, tasks_actual, size.height - footer_height - padding_height, size.height - padding_height)
    } else {
        // Emergency layout for very small terminals
        (size.height.saturating_sub(2), 0, size.height.saturating_sub(1), size.height)
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
    render_header(frame, main_layout[0], &theme, view_model, view_cache);

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
        display_items.push(DisplayItem::Task(card.task.id.clone()));
        display_items.push(DisplayItem::Spacer);
    }

    // Remove trailing spacer if present
    if matches!(display_items.last(), Some(DisplayItem::Spacer)) {
        display_items.pop();
    }

    // Calculate item rectangles with scrolling (exact same logic as main.rs)
    let mut item_rects: Vec<(DisplayItem, Rect)> = Vec::new();
    let mut virtual_y: u16 = 0;
    let mut screen_y = tasks_area.y;
    let area_bottom = tasks_area.y.saturating_add(tasks_area.height);
    let scroll_offset = view_model.scroll_offset;

    for item in display_items {
        let item_height = match &item {
            DisplayItem::Spacer => 1,
            DisplayItem::FilterBar => 1,
            DisplayItem::Task(id) => {
                // Find the card height using fast lookup
                if let Some(card_info) = view_model.task_id_to_card_info.get(id.as_str()) {
                    match card_info.card_type {
                        TaskCardTypeEnum::Draft => {
                            view_model.draft_cards[card_info.index].height
                        }
                        TaskCardTypeEnum::Task => {
                            view_model.task_cards[card_info.index].height
                        }
                    }
                } else {
                    1
                }
            }
        };

        let item_bottom = virtual_y.saturating_add(item_height);

        if item_bottom <= scroll_offset {
            virtual_y = item_bottom;
            continue;
        }

        let visible_top_offset = if virtual_y < scroll_offset {
            scroll_offset.saturating_sub(virtual_y)
        } else {
            0
        };

        let visible_height = item_height.saturating_sub(visible_top_offset);

        if screen_y >= area_bottom {
            break;
        }

        let remaining_screen = area_bottom.saturating_sub(screen_y);
        let final_height = visible_height.min(remaining_screen);

        if final_height > 0 {
            let rect = Rect {
                x: tasks_area.x,
                y: screen_y,
                width: tasks_area.width,
                height: final_height,
            };
            item_rects.push((item, rect));
            screen_y = screen_y.saturating_add(final_height);
        }

        virtual_y = item_bottom;
    }

    // Render display items
    for (item, rect) in item_rects {
        match item {
            DisplayItem::Spacer => {
                frame.render_widget(Paragraph::new("").style(Style::default().bg(theme.bg)), rect);
            }
            DisplayItem::FilterBar => {
                render_filter_bar(frame, rect, view_model, &theme, view_cache);
            }
            DisplayItem::Task(id) => {
                // Find and render the card using fast lookup
                if let Some(card_info) = view_model.task_id_to_card_info.get(id.as_str()) {
                    let card_index = match card_info.card_type {
                        TaskCardTypeEnum::Draft => {
                            let card = &view_model.draft_cards[card_info.index];
                            let is_selected = matches!(view_model.focus_element, FocusElement::DraftTask(idx) if idx == card_info.index);
                            render_draft_card(frame, rect, card, &theme, is_selected);
                            0 // Draft card is always at index 0
                        }
                        TaskCardTypeEnum::Task => {
                            let card = &view_model.task_cards[card_info.index];
                            let is_selected = matches!(view_model.focus_element, FocusElement::ExistingTask(idx) if idx == card_info.index);
                            render_task_card(frame, rect, card, &theme, is_selected);
                            card_info.index + 1 // Task cards start at index 1 (after draft)
                        }
                    };

                    // Add interactive area for the card
                    view_model.interactive_areas.push(InteractiveArea {
                        rect,
                        action: MouseAction::SelectCard(card_index),
                    });
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

    // Handle cursor positioning for focused text areas (exact same as main.rs)
    if matches!(view_model.focus_element, FocusElement::TaskDescription) {
        // Find the focused draft card
        if let Some(card) = view_model.draft_cards.first() {
            if let Some(textarea_area) = find_textarea_area_for_card(view_model, card, tasks_area) {
                // Use simplified cursor positioning logic
                let (cursor_row, cursor_col) = card.textarea.cursor();
                let caret_x = textarea_area.x.saturating_add(cursor_col as u16).min(textarea_area.x + textarea_area.width - 1);
                let caret_y = textarea_area.y.saturating_add(cursor_row as u16).min(textarea_area.y + textarea_area.height - 1);
                frame.set_cursor_position(ratatui::layout::Position::new(caret_x, caret_y));
            }
        }
    }
}
}

// Helper function to find the textarea area for a given card (needed for cursor positioning)
fn find_textarea_area_for_card(_view_model: &ViewModel, _card: &DraftCardViewModel, tasks_area: Rect) -> Option<Rect> {
    // For draft cards, the textarea is positioned with left/right padding of 1
    // and starts after the top border + top padding
    // This is a simplified calculation - in a full implementation you'd track exact positions
    Some(Rect::new(
        tasks_area.x + 1, // Left padding
        tasks_area.y + 1, // Top border + top padding
        tasks_area.width.saturating_sub(2), // Left + right padding
        5, // Approximate visible lines - should match actual calculation
    ))
}

fn render_header(frame: &mut Frame<'_>, area: Rect, theme: &Theme, view_model: &mut ViewModel, view_cache: &mut ViewCache) {
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

        let button_style = if matches!(view_model.focus_element, FocusElement::SettingsButton) {
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

        view_model.interactive_areas.push(InteractiveArea {
            rect: button_area,
            action: MouseAction::OpenSettings,
        });
    }

    // Try to render the logo as an image first using persisted protocol
    if let Some(protocol) = view_cache.logo_protocol.as_mut() {
        // Render the logo image using StatefulImage widget in the padded area
        let image_widget = StatefulImage::default();
        frame.render_stateful_widget(image_widget, content_area, protocol);

        // Check for encoding errors and log them (don't fail the whole UI)
        if let Some(Err(e)) = protocol.last_encoding_result() {
            // If image rendering fails, fall through to ASCII
            eprintln!("Image logo rendering failed: {}", e);
        } else {
            // Image rendered successfully, we're done
            return;
        }
    }

    // Fallback to ASCII logo
    render_ascii_logo(frame, content_area);
}

/// Render a draft card (exact same as main.rs TaskCard::render with state == Draft)
fn render_draft_card(frame: &mut Frame<'_>, area: Rect, card: &DraftCardViewModel, theme: &Theme, is_selected: bool) {
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

/// Render a task card (exact same as main.rs TaskCard::render for Active/Completed/Merged)
fn render_task_card(frame: &mut Frame<'_>, area: Rect, card: &TaskCardViewModel, theme: &Theme, is_selected: bool) {
    let display_title = if card.title.len() > 40 {
        format!("{}...", &card.title[..37])
    } else {
        card.title.clone()
    };

    let card_block = theme.card_block(&display_title);

    // Apply selection highlighting
    let final_card_block = if is_selected {
        card_block.border_style(
            Style::default().fg(theme.primary).add_modifier(Modifier::BOLD),
        )
    } else {
        card_block
    };

    let inner_area = final_card_block.inner(area);
    frame.render_widget(final_card_block, area);

    // Use the same logic as main.rs for different task states
    match card.task.state {
        TaskState::Active => render_active_task_card(frame, inner_area, card, theme),
        TaskState::Completed => render_completed_task_card(frame, inner_area, card, theme),
        TaskState::Merged => render_completed_task_card(frame, inner_area, card, theme), // Same rendering as completed
        TaskState::Draft => {} // Should not happen for task cards
    }
}

/// Render draft card content (exact same as main.rs TaskCard::render_draft_card_content)
fn render_draft_card_content(frame: &mut Frame<'_>, area: Rect, card: &DraftCardViewModel, theme: &Theme) {
    let content_height = area.height as usize;

    // Split the available area between textarea and buttons (exact same as main.rs)
    let button_height: usize = 1; // Single line for buttons
    let separator_height: usize = 1; // Empty line between
    let padding_total = 2; // TEXTAREA_TOP_PADDING + TEXTAREA_BOTTOM_PADDING
    let available_content = content_height.saturating_sub(button_height + separator_height);
    let available_inner = available_content.saturating_sub(padding_total).max(1);
    let desired_lines = card.textarea.lines().len().max(5); // MIN_TEXTAREA_VISIBLE_LINES = 5
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
    frame.render_widget(&card.textarea, textarea_area);

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
    let repo_button_text = if card.task.repository.is_empty() {
        "📁 Repository".to_string()
    } else {
        format!("📁 {}", card.task.repository)
    };

    let branch_button_text = if card.task.branch.is_empty() {
        "🌿 Branch".to_string()
    } else {
        format!("🌿 {}", card.task.branch)
    };

    let models_button_text = if card.task.models.is_empty() {
        "🤖 Models".to_string()
    } else {
        format!("🤖 {} model(s)", card.task.models.len())
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
    // Temporarily disabled to avoid borrowing issues - will be re-enabled later
    // register_draft_card_button_areas(view_model, button_area, &repo_button_text, &branch_button_text, &models_button_text, &go_button_text);
}

/// Register interactive areas for draft card buttons
fn register_draft_card_button_areas(
    view_model: &mut ViewModel,
    button_area: Rect,
    repo_text: &str,
    branch_text: &str,
    models_text: &str,
    go_text: &str,
) {
    use crate::view_model::MouseAction;

    let mut current_x = button_area.x;

    // Repository button: "📁 Repository" -> add 2 for emoji + space + space
    let repo_width = repo_text.chars().count() as u16 + 2;
    let repo_rect = Rect {
        x: current_x,
        y: button_area.y,
        width: repo_width,
        height: button_area.height,
    };
    view_model.interactive_areas.push(InteractiveArea {
        rect: repo_rect,
        action: MouseAction::ActivateRepositoryModal,
    });
    current_x += repo_width + 1; // +1 for space

    // Branch button: "🌿 Branch" -> add 2 for emoji + space + space
    let branch_width = branch_text.chars().count() as u16 + 2;
    let branch_rect = Rect {
        x: current_x,
        y: button_area.y,
        width: branch_width,
        height: button_area.height,
    };
    view_model.interactive_areas.push(InteractiveArea {
        rect: branch_rect,
        action: MouseAction::ActivateBranchModal,
    });
    current_x += branch_width + 1; // +1 for space

    // Models button: "🤖 Models" -> add 2 for emoji + space + space
    let models_width = models_text.chars().count() as u16 + 2;
    let models_rect = Rect {
        x: current_x,
        y: button_area.y,
        width: models_width,
        height: button_area.height,
    };
    view_model.interactive_areas.push(InteractiveArea {
        rect: models_rect,
        action: MouseAction::ActivateModelModal,
    });
    current_x += models_width + 1; // +1 for space

    // Go button: "⏎ Go" -> add 2 for emoji + space + space
    let go_width = go_text.chars().count() as u16 + 2;
    let go_rect = Rect {
        x: current_x,
        y: button_area.y,
        width: go_width,
        height: button_area.height,
    };
    view_model.interactive_areas.push(InteractiveArea {
        rect: go_rect,
        action: MouseAction::LaunchTask,
    });
}

/// Render active task card (exact same as main.rs TaskCard::render_active_card)
fn render_active_task_card(frame: &mut Frame<'_>, area: Rect, card: &TaskCardViewModel, theme: &Theme) {
    // First line: metadata on left, Stop button on right
    let agents_text = if card.metadata.models.is_empty() {
        "No agents".to_string()
    } else if card.metadata.models.len() == 1 {
        format!("{} (x{})", card.metadata.models[0].name, card.metadata.models[0].count)
    } else {
        let agent_strings: Vec<String> = card
            .metadata.models.iter()
            .map(|model| format!("{} (x{})", model.name, model.count))
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
    let stop_style = if matches!(card.focus_element, FocusElement::StopButton(_)) {
        Style::default().fg(theme.bg).bg(theme.error).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.error).bg(theme.surface).add_modifier(Modifier::BOLD)
    };
    line_spans.push(Span::styled(stop_button_text, stop_style));

    let title_line = Line::from(line_spans);

    // Activity lines - display activity entries
    let activity_lines: Vec<Line> = if let TaskCardType::Active { activity_entries, .. } = &card.card_type {
        // Convert the activity entries to display lines (may be multiple lines per entry)
        let mut all_lines = Vec::new();
        for entry in activity_entries.iter().rev().take(3).rev() {
            match entry {
                ActivityEntry::AgentThought { thought } => {
                    all_lines.push(Line::from(vec![
                        Span::styled("💭", Style::default().fg(theme.muted)),
                        Span::raw(" "),
                        Span::styled(thought.clone(), Style::default().fg(theme.text)),
                    ]));
                }
                ActivityEntry::AgentEdit { file_path, lines_added, lines_removed, description } => {
                    let desc = if let Some(desc) = description.as_ref() {
                        desc.clone()
                    } else {
                        format!("Modified {} (+{}, -{})", file_path, lines_added, lines_removed)
                    };
                    all_lines.push(Line::from(vec![
                        Span::styled("📝", Style::default().fg(theme.accent)),
                        Span::raw(" "),
                        Span::styled(desc, Style::default().fg(theme.text)),
                    ]));
                }
                ActivityEntry::ToolUse { tool_name, last_line, completed, status, .. } => {
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
                            Span::styled(status_icon, Style::default().fg(match status {
                                ToolStatus::Completed => theme.success,
                                ToolStatus::Failed => theme.error,
                                ToolStatus::Started => theme.warning,
                            })),
                            Span::raw(" "),
                            Span::styled(format!("{}: {}", tool_name, result_text), Style::default().fg(theme.text)),
                        ]));
                    } else if let Some(line) = last_line {
                        // Tool with output: show tool name + indented output (two lines)
                        all_lines.push(Line::from(vec![
                            Span::styled("🔧", Style::default().fg(theme.primary)),
                            Span::raw(" "),
                            Span::styled(format!("Tool usage: {}", tool_name), Style::default().fg(theme.text)),
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
                            Span::styled(format!("Tool usage: {}", tool_name), Style::default().fg(theme.text)),
                        ]));
                    }
                }
            }
        }
        all_lines
    } else {
        // Fallback for non-active cards (shouldn't happen)
        vec![
            Line::from(vec![
                Span::styled("❓", Style::default().fg(theme.muted)),
                Span::raw(" "),
                Span::styled("No activity data", Style::default().fg(theme.text)),
            ]),
        ]
    };

    // Build all_lines dynamically based on activity_lines_count
    let mut all_lines = vec![title_line, Line::from("")]; // Title + empty separator line
    for activity_line in activity_lines {
        all_lines.push(activity_line);
    }

    // Render each line individually with left padding
    for (i, line) in all_lines.iter().enumerate() {
        if i < area.height as usize {
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
}

/// Render completed/merged task card (exact same as main.rs TaskCard::render_completed_card)
fn render_completed_task_card(frame: &mut Frame<'_>, area: Rect, card: &TaskCardViewModel, theme: &Theme) {
    // Parse delivery indicators and apply proper colors
    let delivery_spans = if card.metadata.delivery_indicators.is_empty() {
        vec![Span::styled("⎇ br", Style::default().fg(theme.primary))]
    } else {
        card.metadata.delivery_indicators
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

    let mut title_spans = vec![
        Span::styled("✓ ", theme.success_style().add_modifier(Modifier::BOLD)),
        Span::styled(&card.title, Style::default().fg(theme.text)),
        Span::raw(" • "),
    ];
    title_spans.extend(delivery_spans);

    let title_line = Line::from(title_spans);

    let agents_text = if card.metadata.models.is_empty() {
        "No agents".to_string()
    } else if card.metadata.models.len() == 1 {
        format!("{} (x{})", card.metadata.models[0].name, card.metadata.models[0].count)
    } else {
        let agent_strings: Vec<String> = card.metadata.models.iter()
            .map(|model| format!("{} (x{})", model.name, model.count))
            .collect();
        agent_strings.join(", ")
    };

    let metadata_line = Line::from(vec![
        Span::styled(&card.metadata.repository, Style::default().fg(theme.muted)),
        Span::raw(" • "),
        Span::styled(&card.metadata.branch, Style::default().fg(theme.muted)),
        Span::raw(" • "),
        Span::styled(&agents_text, Style::default().fg(theme.muted)),
        Span::raw(" • "),
        Span::styled(&card.metadata.timestamp, Style::default().fg(theme.muted)),
    ]);

    let paragraph = Paragraph::new(vec![title_line, metadata_line]).wrap(Wrap { trim: true });

    frame.render_widget(paragraph, area);
}

/// Render the ASCII logo as fallback
fn render_ascii_logo(frame: &mut Frame<'_>, area: Rect) {
    // Try to read the ASCII logo from assets
    let logo_content = include_str!("../../../assets/agent-harbor-logo-80.ansi");

    // Create a paragraph with the logo, preserving ANSI escape codes
    let header = Paragraph::new(logo_content)
        .style(Style::default())
        .alignment(Alignment::Center);
    frame.render_widget(header, area);
}

fn render_filter_bar(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel, theme: &Theme, view_cache: &mut ViewCache) {
    let repo_label = "All".to_string(); // TODO: Get from view_model
    let status_label = "All".to_string(); // TODO: Get from view_model
    let creator_label = "All".to_string(); // TODO: Get from view_model

    let is_separator_focused = matches!(view_model.focus_element, FocusElement::FilterBarSeparator);
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

    let repo_style = if matches!(view_model.focus_element, FocusElement::Filter(_)) {
        theme.focused_style()
    } else {
        Style::default().fg(theme.text)
    };
    push_span(&mut spans, &mut consumed, "Repo ", header_style);
    push_span(&mut spans, &mut consumed, &format!("[{}]", repo_label), repo_style);

    push_span(&mut spans, &mut consumed, "  ", Style::default());

    let status_style = Style::default().fg(theme.text); // TODO: match focus
    push_span(&mut spans, &mut consumed, "Status ", header_style);
    push_span(&mut spans, &mut consumed, &format!("[{}]", status_label), status_style);

    push_span(&mut spans, &mut consumed, "  ", Style::default());

    let creator_style = Style::default().fg(theme.text); // TODO: match focus
    push_span(&mut spans, &mut consumed, "Creator ", header_style);
    push_span(&mut spans, &mut consumed, &format!("[{}]", creator_label), creator_style);

    let line_width = area.width as usize;
    if consumed < line_width {
        let remaining = line_width - consumed + 2;
        push_span(
            &mut spans,
            &mut consumed,
            view_cache.get_separator(remaining as u16),
            border_style,
        );
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
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
        spans.push(Span::styled(shortcut.operation.english_description().to_string(), style));
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(theme.bg)),
        footer_area,
    );
}


/// Format a DraftSaveState for display
fn format_save_state(state: &DraftSaveState) -> String {
    match state {
        DraftSaveState::Unsaved => "Unsaved".to_string(),
        DraftSaveState::Saving => "Saving...".to_string(),
        DraftSaveState::Saved => "Saved".to_string(),
        DraftSaveState::Error => "Error".to_string(),
    }
}
