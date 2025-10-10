use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph, Wrap},
    Frame, Terminal,
};
use std::io;
use std::fs::OpenOptions;
use std::io::Write;
use std::time::{Duration, Instant};
use std::thread;
use rand::seq::SliceRandom;
use image::{DynamicImage, GenericImageView, ImageReader, Rgba, RgbaImage};
use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;
use tui_textarea::{TextArea, Input as TextAreaInput};
use tui_input::Input;
use crossterm::{queue, event::{Event, KeyCode, KeyEvent, KeyModifiers, PushKeyboardEnhancementFlags, PopKeyboardEnhancementFlags, KeyboardEnhancementFlags}};
use crossbeam_channel as chan;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use ctrlc;

// Logging function for debugging key events
fn log_key_event(key: &crossterm::event::KeyEvent, context: &str) {
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("key_log.txt")
    {
        let _ = writeln!(
            file,
            "[{}] Key: {:?}, Code: {:?}, Modifiers: {:?}, Ctrl: {}, Alt: {}, Shift: {}",
            context,
            key,
            key.code,
            key.modifiers,
            key.modifiers.intersects(crossterm::event::KeyModifiers::CONTROL),
            key.modifiers.intersects(crossterm::event::KeyModifiers::ALT),
            key.modifiers.intersects(crossterm::event::KeyModifiers::SHIFT)
        );

        // Special logging for arrow keys
        if matches!(key.code, KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down) {
            let _ = writeln!(file, "  ARROW KEY DETECTED: code={:?}, modifiers={:?}", key.code, key.modifiers);
        }
    }
}

// Padding constants for easy editing
const TEXTAREA_LEFT_PADDING: usize = 1;
const TEXTAREA_TOP_PADDING: usize = 1;
const TEXTAREA_BOTTOM_PADDING: usize = 1;
const TEXTAREA_RIGHT_PADDING: usize = 1;

const BUTTON_LEFT_PADDING: usize = 0;

const ACTIVE_TASK_LEFT_PADDING: usize = 0;

const MODAL_INNER_PADDING: usize = 1;

#[derive(Debug, Clone, PartialEq)]
enum TaskState {
    Draft,
    Active,
    Completed,
}

#[derive(Debug, Clone)]
struct ToolExecution {
    name: String,
    args: String,
    output_lines: Vec<String>,
    current_line_index: usize,
    is_complete: bool,
    success: bool,
    start_time: std::time::Instant,
}

#[derive(Debug, Clone)]
struct SelectedModel {
    name: String,
    count: usize,
}

struct TaskCard {
    title: String,
    repository: String,
    branch: String,
    agents: Vec<SelectedModel>, // Multiple agents with instance counts
    timestamp: String,
    state: TaskState,
    activity: Vec<String>, // For active tasks - live activity history
    delivery_indicators: Option<String>, // For completed tasks
    current_tool_execution: Option<ToolExecution>, // For tracking ongoing tool execution
}

#[derive(Debug, Clone, PartialEq)]
enum FocusElement {
    TaskCard(usize),
    TaskDescription,
    RepositoryButton,
    BranchButton,
    ModelButton,
    GoButton,
    StopButton(usize), // Stop button for specific card
    SettingsButton,
}

#[derive(Debug, Clone, PartialEq)]
enum ModalState {
    None,
    RepositorySearch,
    BranchSearch,
    ModelSearch,
    ModelSelection,
    Settings,
}

#[derive(Debug, Clone)]
struct ModelSelectionModal {
    available_models: Vec<String>,
    selected_models: Vec<SelectedModel>,
    selected_index: usize, // Index in available_models for adding new models
    editing_count: bool,   // Whether we're editing the count of a selected model
    editing_index: usize,  // Index in selected_models when editing count
}

#[derive(Debug, Clone)]
struct FuzzySearchModal {
    input: Input,
    options: Vec<String>,
    selected_index: usize,
}

/// Charm-inspired theme with cohesive colors and styling
#[derive(Debug, Clone)]
struct Theme {
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
    fn card_block_with_button(&self, title: &str, button_text: &str, button_focused: bool) -> Block {
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

    /// Style for focused elements
    fn focused_style(&self) -> Style {
        Style::default().bg(self.primary).fg(Color::Black).add_modifier(Modifier::BOLD)
    }

    /// Style for selected elements
    fn selected_style(&self) -> Style {
        Style::default().bg(self.primary).fg(Color::Black)
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

#[derive(Debug, Clone)]
struct AppState {
    selected_card: usize,
    focus_element: FocusElement,
    modal_state: ModalState,
    fuzzy_modal: Option<FuzzySearchModal>,
    model_selection_modal: Option<ModelSelectionModal>,
    task_description: TextArea<'static>,
    selected_repository: String,
    selected_branch: String,
    selected_models: Vec<SelectedModel>, // Multiple models with instance counts
    activity_timer: Instant,
    activity_lines_count: usize, // Configurable number of activity lines (1-3)
}

impl TaskCard {
    fn height(&self, activity_lines_count: usize) -> u16 {
        match self.state {
            TaskState::Completed => 3, // Title + metadata + padding (2 lines content)
            TaskState::Active => 2 + activity_lines_count as u16 + 3, // Title + empty line + N activity lines + 2 for borders
            TaskState::Draft => 6, // Description with padding + separator + buttons + borders
        }
    }

    fn add_activity(&mut self, activity: String) {
        if let TaskState::Active = self.state {
            self.activity.push(activity);
            // Keep only the last 10 activities for memory efficiency
            if self.activity.len() > 10 {
                self.activity.remove(0);
            }
        }
    }

    fn get_recent_activity(&self, count: usize) -> Vec<String> {
        if let TaskState::Active = self.state {
            // Return last N activities, formatted for display
            let recent = self.activity.iter().rev().take(count).cloned().collect::<Vec<_>>();
            let mut result = recent.into_iter().rev().collect::<Vec<_>>();

            // Always return exactly N lines, padding with empty strings
            while result.len() < count {
                result.push("".to_string());
            }

            result
        } else {
            vec!["".to_string(); count]
        }
    }

    fn format_agents(&self) -> String {
        if self.agents.is_empty() {
            "No agents".to_string()
        } else if self.agents.len() == 1 {
            format!("{} (x{})", self.agents[0].name, self.agents[0].count)
        } else {
            let agent_strings: Vec<String> = self.agents.iter()
                .map(|agent| format!("{} (x{})", agent.name, agent.count))
                .collect();
            agent_strings.join(", ")
        }
    }

    fn start_tool_execution(&mut self, name: &str, args: &str) {
        if let TaskState::Active = self.state {
            let tool_execution = ToolExecution {
                name: name.to_string(),
                args: args.to_string(),
                output_lines: self.generate_tool_output(name),
                current_line_index: 0,
                is_complete: false,
                success: true,
                start_time: std::time::Instant::now(),
            };

            self.current_tool_execution = Some(tool_execution);
            self.add_activity(format!("Tool usage: {}", name));
        }
    }

    fn update_tool_execution(&mut self) {
        if let Some(ref mut tool_exec) = self.current_tool_execution {
            if tool_exec.current_line_index < tool_exec.output_lines.len() {
                let line = &tool_exec.output_lines[tool_exec.current_line_index];
                // Update the last activity line (in-place update for last_line behavior)
                if let Some(last_activity) = self.activity.last_mut() {
                    if last_activity.starts_with("Tool usage: ") && !last_activity.contains("completed") {
                        *last_activity = format!("Tool usage: {}: {}", tool_exec.name, line);
                    }
                }
                tool_exec.current_line_index += 1;
            } else if !tool_exec.is_complete {
                // Mark as complete and add completion message
                tool_exec.is_complete = true;
                let status = if tool_exec.success { "completed successfully" } else { "failed" };
                if let Some(last_activity) = self.activity.last_mut() {
                    if last_activity.starts_with("Tool usage: ") && !last_activity.contains("completed") && !last_activity.contains("failed") {
                        *last_activity = format!("Tool usage: {}: {}", tool_exec.name, status);
                    }
                }
                self.current_tool_execution = None;
            }
        }
    }

    fn add_thought(&mut self, thought: &str) {
        if let TaskState::Active = self.state {
            self.add_activity(format!("Thoughts: {}", thought));
        }
    }

    fn add_file_edit(&mut self, file_path: &str, lines_added: usize, lines_removed: usize) {
        if let TaskState::Active = self.state {
            self.add_activity(format!("File edits: {} (+{} -{})", file_path, lines_added, lines_removed));
        }
    }

    fn generate_tool_output(&self, tool_name: &str) -> Vec<String> {
        match tool_name {
            "cargo build" => vec![
                "Compiling agent-harbor v0.1.0 (/home/user/agent-harbor)".to_string(),
                "Compiling serde v1.0.193".to_string(),
                "Compiling tokio v1.35.1".to_string(),
                "Compiling ratatui v0.26.0".to_string(),
                "Compiling crossterm v0.27.0".to_string(),
                "Compiling reqwest v0.11.22".to_string(),
                "Compiling sqlx v0.7.3".to_string(),
                "Compiling clap v4.4.18".to_string(),
                "Compiling tracing v0.1.40".to_string(),
                "Compiling thiserror v1.0.50".to_string(),
                "Compiling agent-harbor v0.1.0 (/home/user/agent-harbor)".to_string(),
                "Finished dev [unoptimized + debuginfo] target(s) in 45.23s".to_string(),
            ],
            "cargo check" => vec![
                "Checking agent-harbor v0.1.0 (/home/user/agent-harbor)".to_string(),
                "Finished dev [unoptimized + debuginfo] target(s) in 12.34s".to_string(),
            ],
            "cargo test" => vec![
                "running 12 tests".to_string(),
                "test auth::login::test_valid_credentials ... ok".to_string(),
                "test auth::login::test_invalid_credentials ... ok".to_string(),
                "test api::users::test_create_user ... ok".to_string(),
                "test api::users::test_get_user ... ok".to_string(),
                "test api::projects::test_create_project ... ok".to_string(),
                "test api::projects::test_list_projects ... ok".to_string(),
                "test db::migrations::test_migration_up ... ok".to_string(),
                "test db::migrations::test_migration_down ... ok".to_string(),
                "test utils::validation::test_email_validation ... ok".to_string(),
                "test utils::validation::test_password_strength ... ok".to_string(),
                "test utils::cache::test_cache_operations ... ok".to_string(),
                "test utils::cache::test_cache_expiration ... ok".to_string(),
                "".to_string(),
                "test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.34s".to_string(),
            ],
            "read_file" => vec![
                "Reading file: src/main.rs".to_string(),
                "File size: 1247 lines".to_string(),
                "Language: Rust".to_string(),
                "Found main function and imports".to_string(),
            ],
            _ => vec![
                format!("Starting {}...", tool_name),
                format!("Processing {} arguments...", tool_name),
                "Command completed successfully".to_string(),
            ],
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect, app_state: &AppState, theme: &Theme, is_selected: bool, card_index: usize) {
        match self.state {
            TaskState::Draft => {
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
                self.render_draft_card_content(frame, inner_area, app_state, theme);
            }
            TaskState::Active => {
                let display_title = if self.title.len() > 40 {
                    format!("{}...", &self.title[..37])
                } else {
                    self.title.clone()
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

                let is_stop_focused = matches!(app_state.focus_element, FocusElement::StopButton(idx) if idx == card_index);
                self.render_active_card(frame, inner_area, theme, is_stop_focused, app_state.activity_lines_count);
            }
            TaskState::Completed => {
                let display_title = if self.title.len() > 40 {
                    format!("{}...", &self.title[..37])
                } else {
                    self.title.clone()
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

                self.render_completed_card(frame, inner_area, theme);
            }
        }
    }

    fn render_completed_card(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // Parse delivery indicators and apply proper colors
        let delivery_spans = if let Some(indicators) = &self.delivery_indicators {
            indicators.split_whitespace()
                .flat_map(|indicator| {
                    match indicator {
                        "⎇" => vec![
                            Span::styled("⎇", Style::default().fg(Color::Cyan)),
                            Span::raw(" ")
                        ],
                        "⇄" => vec![
                            Span::styled("⇄", Style::default().fg(Color::Yellow)),
                            Span::raw(" ")
                        ],
                        "✓" => vec![
                            Span::styled("✓", Style::default().fg(Color::Green)),
                            Span::raw(" ")
                        ],
                        _ => vec![Span::raw(indicator), Span::raw(" ")],
                    }
                })
                .collect::<Vec<_>>()
        } else {
            vec![Span::styled("⎇ br", Style::default().fg(theme.primary))]
        };

        let mut title_spans = vec![
            Span::styled(
                "✓ ",
                theme.success_style().add_modifier(Modifier::BOLD),
            ),
            Span::styled(&self.title, Style::default().fg(theme.text)),
            Span::raw(" • "),
        ];
        title_spans.extend(delivery_spans);

        let title_line = Line::from(title_spans);

        let agents_text = self.format_agents();
        let metadata_line = Line::from(vec![
            Span::styled(&self.repository, Style::default().fg(theme.muted)),
            Span::raw(" • "),
            Span::styled(&self.branch, Style::default().fg(theme.muted)),
            Span::raw(" • "),
            Span::styled(&agents_text, Style::default().fg(theme.muted)),
            Span::raw(" • "),
            Span::styled(&self.timestamp, Style::default().fg(theme.muted)),
        ]);

        let paragraph = Paragraph::new(vec![title_line, metadata_line])
            .wrap(Wrap { trim: true });

        frame.render_widget(paragraph, area);
    }

    fn render_active_card(&self, frame: &mut Frame, area: Rect, theme: &Theme, is_stop_focused: bool, activity_lines_count: usize) {
        // First line: metadata on left, Stop button on right
        let agents_text = self.format_agents();
        let metadata_part = vec![
            Span::styled(
                "● ",
                Style::default().fg(theme.warning).add_modifier(Modifier::BOLD),
            ),
            Span::styled(&self.repository, Style::default().fg(theme.muted)),
            Span::raw(" • "),
            Span::styled(&self.branch, Style::default().fg(theme.muted)),
            Span::raw(" • "),
            Span::styled(&agents_text, Style::default().fg(theme.muted)),
            Span::raw(" • "),
            Span::styled(&self.timestamp, Style::default().fg(theme.muted)),
        ];

        // Calculate how much space we need for the right-aligned Stop button
        let metadata_text = format!("● {} • {} • {} • {}", self.repository, self.branch, agents_text, self.timestamp);
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
        let stop_style = if is_stop_focused {
            Style::default().fg(theme.bg).bg(theme.error).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.error).bg(theme.surface).add_modifier(Modifier::BOLD)
        };
        line_spans.push(Span::styled(stop_button_text, stop_style));

        let title_line = Line::from(line_spans);

        let activity_vec = self.get_recent_activity(activity_lines_count);
        let activity_lines: Vec<Line> = activity_vec.into_iter().enumerate().map(|(i, activity)| {
            if activity.trim().is_empty() {
                // Empty activity - show a subtle placeholder
                Line::from(vec![
                    Span::styled("  ", Style::default().fg(theme.muted)),
                    Span::styled("─", Style::default().fg(theme.border)),
                ])
            } else {
                let (prefix, content, color) = if activity.starts_with("Thoughts:") {
                    ("💭", activity.strip_prefix("Thoughts:").unwrap_or(&activity).trim().to_string(), theme.muted)
                } else if activity.starts_with("Tool usage:") {
                    let tool_content = activity.strip_prefix("Tool usage:").unwrap_or(&activity).trim();
                    let icon_color = if tool_content.contains("completed successfully") {
                        theme.success
                    } else if tool_content.contains("failed") {
                        theme.error
                    } else {
                        theme.primary
                    };
                    ("🔧", tool_content.to_string(), icon_color)
                } else if activity.starts_with("  ") {
                    ("  ", activity.strip_prefix("  ").unwrap_or(&activity).to_string(), theme.muted)
                } else if activity.starts_with("File edits:") {
                    ("📝", activity.strip_prefix("File edits:").unwrap_or(&activity).trim().to_string(), theme.warning)
                } else {
                    ("  ", activity, theme.text)
                };

                Line::from(vec![
                    Span::styled(prefix, Style::default().fg(color)),
                    Span::raw(" "),
                    Span::styled(content, Style::default().fg(theme.text)),
                ])
            }
        }).collect();

        // Build all_lines dynamically based on activity_lines_count
        let mut all_lines = vec![title_line, Line::from("")]; // Title + empty separator line
        for i in 0..activity_lines_count {
            all_lines.push(activity_lines.get(i).cloned().unwrap_or_else(|| Line::from("")));
        }

        // Render each line individually with left padding
        for (i, line) in all_lines.iter().enumerate() {
            if i < area.height as usize {
                let line_area = Rect::new(area.x + ACTIVE_TASK_LEFT_PADDING as u16, area.y + i as u16, area.width.saturating_sub(ACTIVE_TASK_LEFT_PADDING as u16), 1);
                let para = Paragraph::new(line.clone());
                frame.render_widget(para, line_area);
            }
        }
    }

    fn render_draft_card(&self, frame: &mut Frame, area: Rect, app_state: &AppState, theme: &Theme) {
        // Draft cards render directly without outer border like ah-tui
        self.render_draft_card_content(frame, area, app_state, theme);
    }

    fn render_draft_card_content(&self, frame: &mut Frame, area: Rect, app_state: &AppState, theme: &Theme) {
        let content_height = area.height as usize;

        // Split the available area between textarea and buttons
        let textarea_height: usize = 3; // Fixed height for textarea
        let button_height: usize = 1; // Single line for buttons
        let separator_height: usize = 1; // Empty line between

        // Add configurable left padding for textarea and buttons
        let textarea_area = Rect {
            x: area.x + TEXTAREA_LEFT_PADDING as u16,
            y: area.y + TEXTAREA_TOP_PADDING as u16,
            width: area.width.saturating_sub((TEXTAREA_LEFT_PADDING + TEXTAREA_RIGHT_PADDING) as u16),
            height: (textarea_height - TEXTAREA_TOP_PADDING - TEXTAREA_BOTTOM_PADDING) as u16,
        };

        let button_area = Rect {
            x: area.x + BUTTON_LEFT_PADDING as u16,
            y: area.y + textarea_height as u16 + separator_height as u16,
            width: area.width.saturating_sub(BUTTON_LEFT_PADDING as u16),
            height: button_height as u16,
        };

        // Render padding areas around textarea
        let top_padding_area = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: TEXTAREA_TOP_PADDING as u16,
        };
        let bottom_padding_area = Rect {
            x: area.x,
            y: area.y + (textarea_height - TEXTAREA_BOTTOM_PADDING) as u16,
            width: area.width,
            height: TEXTAREA_BOTTOM_PADDING as u16,
        };
        let left_padding_area = Rect {
            x: area.x,
            y: area.y + TEXTAREA_TOP_PADDING as u16,
            width: TEXTAREA_LEFT_PADDING as u16,
            height: (textarea_height - TEXTAREA_TOP_PADDING - TEXTAREA_BOTTOM_PADDING) as u16,
        };
        let right_padding_area = Rect {
            x: area.x + area.width.saturating_sub(TEXTAREA_RIGHT_PADDING as u16),
            y: area.y + TEXTAREA_TOP_PADDING as u16,
            width: TEXTAREA_RIGHT_PADDING as u16,
            height: (textarea_height - TEXTAREA_TOP_PADDING - TEXTAREA_BOTTOM_PADDING) as u16,
        };

        // Render padding with background color
        let padding_style = Style::default().bg(theme.bg);
        frame.render_widget(Paragraph::new("").style(padding_style), top_padding_area);
        frame.render_widget(Paragraph::new("").style(padding_style), bottom_padding_area);
        frame.render_widget(Paragraph::new("").style(padding_style), left_padding_area);
        frame.render_widget(Paragraph::new("").style(padding_style), right_padding_area);

        // Render left padding for buttons
        let button_left_padding = Rect {
            x: area.x,
            y: button_area.y,
            width: BUTTON_LEFT_PADDING as u16,
            height: button_area.height,
        };
        frame.render_widget(Paragraph::new("").style(padding_style), button_left_padding);

        // Render the textarea
        frame.render_widget(&app_state.task_description, textarea_area);

        // Render separator line
        if (textarea_height + separator_height) < content_height {
            let separator_area = Rect {
                x: area.x,
                y: area.y + textarea_height as u16,
                width: area.width,
                height: separator_height as u16,
            };
            let separator = Paragraph::new("").style(Style::default().bg(theme.bg));
            frame.render_widget(separator, separator_area);
        }

        // Render buttons
        let repo_button_text = if self.repository.is_empty() {
            "📁 Repository".to_string()
        } else {
            format!("📁 {}", self.repository)
        };

        let branch_button_text = if self.branch.is_empty() {
            "🌿 Branch".to_string()
        } else {
            format!("🌿 {}", self.branch)
        };

        let models_button_text = if app_state.selected_models.is_empty() {
            "🤖 Models".to_string()
        } else if app_state.selected_models.len() == 1 {
            format!("🤖 {} (x{})", app_state.selected_models[0].name, app_state.selected_models[0].count)
        } else {
            format!("🤖 {} models", app_state.selected_models.len())
        };

        let go_button_text = "⏎ Go".to_string();

        // Create button spans with focus styling using theme - exactly like ah-tui
        let repo_button = if matches!(app_state.focus_element, FocusElement::RepositoryButton) {
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

        let branch_button = if matches!(app_state.focus_element, FocusElement::BranchButton) {
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

        let models_button = if matches!(app_state.focus_element, FocusElement::ModelButton) {
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

        let go_button = if matches!(app_state.focus_element, FocusElement::GoButton) {
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

        let button_paragraph = Paragraph::new(button_line).style(Style::default().bg(theme.bg));
        frame.render_widget(button_paragraph, button_area);
    }
}

fn render_header(frame: &mut Frame, area: Rect, theme: &Theme, focus_element: &FocusElement, _image_picker: Option<&Picker>, logo_protocol: Option<&mut StatefulProtocol>) {
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
        let button_area = Rect {
            x: area.width - button_width - 2, // 2 units from right edge
            y: area.y + 1, // Just below top padding
            width: button_width,
            height: 1,
        };

        let button_style = if matches!(focus_element, FocusElement::SettingsButton) {
            Style::default().fg(theme.bg).bg(theme.primary).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.primary).bg(theme.surface).add_modifier(Modifier::BOLD)
        };

        let button_line = Line::from(vec![
            Span::styled(" ", button_style),
            Span::styled(button_text, button_style),
            Span::styled(" ", button_style),
        ]);

        let button_paragraph = Paragraph::new(button_line);
        frame.render_widget(button_paragraph, button_area);
    }

    // Try to render the logo as an image first
    if let Some(protocol) = logo_protocol {
        // Render the logo image using StatefulImage widget in the padded area
        let image_widget = ratatui_image::StatefulImage::default();
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

    frame.render_widget(paragraph, content_area);
}

fn render_settings_dialog(frame: &mut Frame, app_state: &AppState, area: Rect, theme: &Theme) {
    // Calculate dialog dimensions
    let dialog_width = 50.min(area.width - 4);
    let dialog_height = 12.min(area.height - 4);

    let dialog_area = Rect {
        x: (area.width - dialog_width) / 2,
        y: (area.height - dialog_height) / 2,
        width: dialog_width,
        height: dialog_height,
    };

    // Shadow effect
    let mut shadow_area = dialog_area;
    shadow_area.x += 1;
    shadow_area.y += 1;
    let shadow = Block::default().style(Style::default().bg(Color::Rgb(10, 10, 15)));
    frame.render_widget(Clear, shadow_area);
    frame.render_widget(shadow, shadow_area);

    // Main dialog with rounded border
    let title_line = Line::from(vec![
        Span::raw("┤").fg(theme.border),
        Span::raw(" Settings ").style(Style::default().fg(theme.text).add_modifier(Modifier::BOLD)),
        Span::raw("├").fg(theme.border),
    ]);

    let dialog_block = Block::default()
        .title(title_line)
        .title_alignment(ratatui::layout::Alignment::Left)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_focused))
        .style(Style::default().bg(theme.surface));

    frame.render_widget(Clear, dialog_area);
    let inner_area = dialog_block.inner(dialog_area);
    frame.render_widget(dialog_block, dialog_area);

    // Create horizontal line with text segment for section title
    let create_section_line = |title: &str| -> Line {
        let line_width = inner_area.width as usize;
        let title_with_spaces = format!(" {} ", title);
        let title_len = title_with_spaces.len();

        if title_len + 4 >= line_width {
            // If title is too long, just show a regular line
            Line::from(Span::styled("─".repeat(line_width), Style::default().fg(theme.border)))
        } else {
            let left_len = (line_width - title_len) / 2;
            let right_len = line_width - title_len - left_len;

            Line::from(vec![
                Span::styled("─".repeat(left_len), Style::default().fg(theme.border)),
                Span::styled(title_with_spaces, Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
                Span::styled("─".repeat(right_len), Style::default().fg(theme.border)),
            ])
        }
    };

    // Content
    let content_lines = vec![
        Line::from(""), // Empty line
        create_section_line("Activity Lines"),
        Line::from(""),
        Line::from(vec![
            Span::raw("  "),
            Span::styled("1", if app_state.activity_lines_count == 1 {
                Style::default().fg(theme.bg).bg(theme.primary).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)
            }),
            Span::raw(" - Show 1 activity line"),
        ]),
        Line::from(vec![
            Span::raw("  "),
            Span::styled("2", if app_state.activity_lines_count == 2 {
                Style::default().fg(theme.bg).bg(theme.primary).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)
            }),
            Span::raw(" - Show 2 activity lines"),
        ]),
        Line::from(vec![
            Span::raw("  "),
            Span::styled("3", if app_state.activity_lines_count == 3 {
                Style::default().fg(theme.bg).bg(theme.primary).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)
            }),
            Span::raw(" - Show 3 activity lines"),
        ]),
        Line::from(""),
        create_section_line("Controls"),
        Line::from(""),
        Line::from(vec![
            Span::raw("  "),
            Span::styled("1/2/3", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
            Span::raw(" - Change activity lines"),
        ]),
        Line::from(vec![
            Span::raw("  "),
            Span::styled("Esc/Enter", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
            Span::raw(" - Close settings"),
        ]),
    ];

    // Render content lines
    for (i, line) in content_lines.iter().enumerate() {
        if i < inner_area.height as usize {
            let line_area = Rect::new(inner_area.x, inner_area.y + i as u16, inner_area.width, 1);
            let para = Paragraph::new(line.clone());
            frame.render_widget(para, line_area);
        }
    }
}

fn render_footer(frame: &mut Frame, area: Rect, focus_element: &FocusElement, theme: &Theme) {
    let shortcuts = match focus_element {
        FocusElement::TaskCard(_) => vec![
            Span::styled("↑↓", theme.warning_style()),
            Span::raw(" Navigate • "),
            Span::styled("Tab/→", Style::default().fg(theme.primary)),
            Span::raw(" Stop Button • "),
            Span::styled("Ctrl+C x2", theme.error_style()),
            Span::raw(" Quit"),
        ],
        FocusElement::StopButton(_) => vec![
            Span::styled("Enter", theme.error_style()),
            Span::raw(" Stop Task • "),
            Span::styled("←", Style::default().fg(theme.primary)),
            Span::raw(" Back to Card • "),
            Span::styled("Ctrl+C x2", theme.error_style()),
            Span::raw(" Quit"),
        ],
        FocusElement::TaskDescription => vec![
            Span::styled("Enter", theme.success_style()),
            Span::raw(" Launch Agent(s) • "),
            Span::styled("Shift+Enter", theme.warning_style()),
            Span::raw(" New Line • "),
            Span::styled("Tab", Style::default().fg(theme.primary)),
            Span::raw(" Next Field"),
        ],
        FocusElement::RepositoryButton | FocusElement::BranchButton | FocusElement::ModelButton => vec![
            Span::styled("↑↓", theme.warning_style()),
            Span::raw(" Navigate • "),
            Span::styled("Enter", theme.success_style()),
            Span::raw(" Select • "),
            Span::styled("Esc", Style::default().fg(theme.muted)),
            Span::raw(" Back"),
        ],
        FocusElement::GoButton => vec![
            Span::styled("Enter", theme.success_style()),
            Span::raw(" Launch Task • "),
            Span::styled("Esc", Style::default().fg(theme.muted)),
            Span::raw(" Back"),
        ],
        FocusElement::SettingsButton => vec![
            Span::styled("Enter", theme.success_style()),
            Span::raw(" Open Settings • "),
            Span::styled("↓", Style::default().fg(theme.primary)),
            Span::raw(" Back to Tasks"),
        ],
    };

    // Add left padding to footer like other elements
    let footer_area = if area.width >= 4 {
        let horizontal_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(2), // Left padding
                Constraint::Min(1),    // Content area
            ])
            .split(area);
        horizontal_chunks[1]
    } else {
        area
    };

    let footer_line = Line::from(shortcuts);
    let footer = Paragraph::new(footer_line)
        .style(Style::default().bg(theme.bg))
        .alignment(Alignment::Left);

    frame.render_widget(footer, footer_area);
}

fn render_fuzzy_modal(frame: &mut Frame, modal: &FuzzySearchModal, area: Rect, theme: &Theme) {
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
        .title_alignment(ratatui::layout::Alignment::Left)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_focused))
        .padding(Padding::new(1, 1, 0, 0))
        .style(Style::default().bg(theme.surface));

    frame.render_widget(Clear, modal_area);
    let inner_area = modal_block.inner(modal_area);
    frame.render_widget(modal_block, modal_area);

    // Split the inner area: top for input, bottom for results, with a separator line
    let vertical_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Input line
            Constraint::Length(1), // Separator line
            Constraint::Min(1),    // Results area
        ])
        .split(inner_area);

    let input_area = vertical_chunks[0];
    let separator_area = vertical_chunks[1];
    let results_area = vertical_chunks[2];

    // Render the input field directly in the input area
    let input_value = modal.input.value();
    let display_value = if input_value.is_empty() {
        Span::styled("Type to search...", Style::default().fg(theme.muted))
    } else {
        Span::styled(input_value, Style::default().fg(theme.text))
    };

    let input_paragraph = Paragraph::new(Line::from(display_value))
        .wrap(Wrap { trim: true });
    frame.render_widget(input_paragraph, input_area);

    // Show cursor
    if !input_value.is_empty() {
        let visual_cursor = modal.input.visual_cursor();
        let cursor_x = input_area.x as u16 + visual_cursor as u16;
        let cursor_y = input_area.y as u16;
        if cursor_x < input_area.x as u16 + input_area.width as u16 && cursor_y < input_area.y as u16 + input_area.height as u16 {
            frame.set_cursor_position(ratatui::layout::Position::new(cursor_x, cursor_y));
        }
    }

    // Render separator line
    let separator_line = Line::from(vec![
        Span::styled("─".repeat(separator_area.width as usize), Style::default().fg(theme.border))
    ]);
    frame.render_widget(Paragraph::new(separator_line), separator_area);

    // Filter options based on input
    let query = modal.input.value();
    let filtered_options: Vec<&String> = if query.is_empty() {
        modal.options.iter().take(10).collect()
    } else {
        modal.options.iter()
            .filter(|opt| opt.to_lowercase().contains(&query.to_lowercase()))
            .take(10)
            .collect()
    };

    // Display filtered results directly in results area
    let result_lines: Vec<Line> = filtered_options.iter().enumerate().map(|(i, opt)| {
        let style = if i == modal.selected_index {
            theme.selected_style()
        } else {
            Style::default().fg(theme.text)
        };
        Line::from(opt.as_str()).style(style)
    }).collect();

    frame.render_widget(
        Paragraph::new(result_lines).wrap(Wrap { trim: true }),
        results_area
    );
}

fn render_model_selection_modal(frame: &mut Frame, modal: &ModelSelectionModal, area: Rect, theme: &Theme) {
    // Calculate modal dimensions
    let modal_width = 70.min(area.width - 4);
    let modal_height = 18.min(area.height - 4);

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
        Span::raw(" Select Models ").style(Style::default().fg(theme.text).add_modifier(Modifier::BOLD)),
        Span::raw("").fg(theme.primary),
    ]);

    let modal_block = Block::default()
        .title(title_line)
        .title_alignment(ratatui::layout::Alignment::Left)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_focused))
        .padding(Padding::new(1, 1, 0, 0))
        .style(Style::default().bg(theme.surface));

    frame.render_widget(Clear, modal_area);
    let inner_area = modal_block.inner(modal_area);
    frame.render_widget(modal_block, modal_area);

    // Split the inner area: top for selected models, middle separator, bottom for available models
    let vertical_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2 + modal.selected_models.len() as u16), // Selected models section
            Constraint::Length(1), // Separator
            Constraint::Min(1),    // Available models section
        ])
        .split(inner_area);

    let selected_area = vertical_chunks[0];
    let separator_area = vertical_chunks[1];
    let available_area = vertical_chunks[2];

    // Render selected models
    let mut selected_lines = vec![Line::from(vec![
        Span::styled("Selected Models:", Style::default().fg(theme.text).add_modifier(Modifier::BOLD))
    ])];

    for (i, model) in modal.selected_models.iter().enumerate() {
        let style = if modal.editing_count && i == modal.editing_index {
            theme.selected_style()
        } else {
            Style::default().fg(theme.text)
        };
        selected_lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(&model.name, style),
            Span::raw(" (x"),
            Span::styled(model.count.to_string(), style),
            Span::raw(")"),
            Span::styled(" [-]", Style::default().fg(theme.muted)),
        ]));
    }

    if modal.selected_models.is_empty() {
        selected_lines.push(Line::from(vec![
            Span::styled("  (none selected)", Style::default().fg(theme.muted))
        ]));
    }

    frame.render_widget(
        Paragraph::new(selected_lines).wrap(Wrap { trim: true }),
        selected_area
    );

    // Render separator
    let separator_line = Line::from(vec![
        Span::styled("─".repeat(separator_area.width as usize), Style::default().fg(theme.border))
    ]);
    frame.render_widget(Paragraph::new(separator_line), separator_area);

    // Render available models
    let mut available_lines = vec![Line::from(vec![
        Span::styled("Available Models (↑↓ to navigate, Enter to add):", Style::default().fg(theme.text).add_modifier(Modifier::BOLD))
    ])];

    for (i, model_name) in modal.available_models.iter().enumerate() {
        let is_selected = i == modal.selected_index && !modal.editing_count;
        let style = if is_selected {
            theme.selected_style()
        } else {
            Style::default().fg(theme.text)
        };
        available_lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(model_name, style),
            Span::styled(" [+]", Style::default().fg(theme.muted)),
        ]));
    }

    frame.render_widget(
        Paragraph::new(available_lines).wrap(Wrap { trim: true }),
        available_area
    );
}

fn create_sample_tasks() -> Vec<TaskCard> {
    vec![
        TaskCard {
            title: "".to_string(), // Will be filled by user input
            repository: "agent-harbor".to_string(),
            branch: "main".to_string(),
            agents: vec![SelectedModel { name: "claude-3-5-sonnet".to_string(), count: 1 }],
            timestamp: "now".to_string(),
            state: TaskState::Draft,
            activity: vec![], // Empty for draft
            delivery_indicators: None,
            current_tool_execution: None,
        },
        TaskCard {
            title: "Implement payment processing".to_string(),
            repository: "ecommerce-platform".to_string(),
            branch: "feature/payments".to_string(),
            agents: vec![SelectedModel { name: "claude-3-5-sonnet".to_string(), count: 1 }],
            timestamp: "5 min ago".to_string(),
            state: TaskState::Active,
            activity: vec![
                "Thoughts: Analyzing payment flow requirements".to_string(),
                "Tool usage: read_file".to_string(),
                "  Reading payment service contracts".to_string(),
            ],
            delivery_indicators: None,
            current_tool_execution: None,
        },
        TaskCard {
            title: "Optimize database queries for user dashboard performance".to_string(),
            repository: "analytics-platform".to_string(),
            branch: "perf/dashboard-queries".to_string(),
            agents: vec![SelectedModel { name: "gpt-4".to_string(), count: 1 }],
            timestamp: "25 min ago".to_string(),
            state: TaskState::Active,
            activity: vec![
                "Thoughts: Identifying N+1 query issues in dashboard components".to_string(),
                "Tool usage: read_file".to_string(),
                "  Examining dashboard query patterns".to_string(),
            ],
            delivery_indicators: None,
            current_tool_execution: None,
        },
        TaskCard {
            title: "Add user authentication and session management".to_string(),
            repository: "web-app".to_string(),
            branch: "feature/user-auth".to_string(),
            agents: vec![SelectedModel { name: "claude-3-5-sonnet".to_string(), count: 1 }],
            timestamp: "2 hours ago".to_string(),
            state: TaskState::Completed,
            activity: vec![],
            delivery_indicators: Some("⎇ ✓".to_string()), // Branch exists + PR merged
            current_tool_execution: None,
        },
        TaskCard {
            title: "Implement payment processing with Stripe integration".to_string(),
            repository: "ecommerce-platform".to_string(),
            branch: "feature/stripe-payment".to_string(),
            agents: vec![SelectedModel { name: "gpt-4".to_string(), count: 1 }],
            timestamp: "4 hours ago".to_string(),
            state: TaskState::Completed,
            activity: vec![],
            delivery_indicators: Some("⎇ ⇄ ✓".to_string()), // Branch exists + PR exists + PR merged
            current_tool_execution: None,
        },
        TaskCard {
            title: "Add comprehensive error logging and monitoring".to_string(),
            repository: "backend-api".to_string(),
            branch: "feature/error-monitoring".to_string(),
            agents: vec![SelectedModel { name: "claude-3-5-sonnet".to_string(), count: 1 }],
            timestamp: "6 hours ago".to_string(),
            state: TaskState::Completed,
            activity: vec![],
            delivery_indicators: Some("⎇".to_string()), // Branch exists only
            current_tool_execution: None,
        },
    ]
}

impl AppState {
    fn new() -> Self {
        let mut textarea = TextArea::default();
        textarea.set_placeholder_text("Describe what you want the agent to do...");
        textarea.set_cursor_line_style(Style::default());
        textarea.set_placeholder_style(Style::default().fg(Color::DarkGray));
        // Make cursor invisible in placeholder mode by using same style as placeholder

        Self {
            selected_card: 0,
            focus_element: FocusElement::TaskDescription,
            modal_state: ModalState::None,
            fuzzy_modal: None,
            model_selection_modal: None,
            task_description: textarea,
            selected_repository: "agent-harbor".to_string(),
            selected_branch: "main".to_string(),
            selected_models: vec![SelectedModel { name: "claude-3-5-sonnet".to_string(), count: 1 }],
            activity_timer: Instant::now(),
            activity_lines_count: 3, // Default to 3 activity lines
        }
    }


    fn handle_key(&mut self, key: crossterm::event::KeyEvent, tasks: &mut Vec<TaskCard>) -> bool {
        // Log all key events
        log_key_event(&key, "MAIN");

        match self.modal_state {
            ModalState::None => self.handle_main_key(key, tasks),
            ModalState::RepositorySearch | ModalState::BranchSearch | ModalState::ModelSearch => {
                self.handle_modal_key(key)
            }
            ModalState::ModelSelection => {
                self.handle_model_selection_key(key)
            }
            ModalState::Settings => {
                self.handle_settings_key(key)
            }
        }
    }

    fn handle_main_key(&mut self, key: crossterm::event::KeyEvent, tasks: &mut Vec<TaskCard>) -> bool {
        use crossterm::event::{KeyCode, KeyModifiers};

        let shift_pressed = key.modifiers.contains(KeyModifiers::SHIFT);
        let ctrl_pressed = key.modifiers.contains(KeyModifiers::CONTROL);

        // Handle activity lines count changes (Ctrl+1, Ctrl+2, Ctrl+3)
        if ctrl_pressed {
            match key.code {
                KeyCode::Char('1') => {
                    self.activity_lines_count = 1;
                    return false;
                }
                KeyCode::Char('2') => {
                    self.activity_lines_count = 2;
                    return false;
                }
                KeyCode::Char('3') => {
                    self.activity_lines_count = 3;
                    return false;
                }
                _ => {}
            }
        }

        match key.code {
            KeyCode::Esc => {
                // If focus is on any button in the draft card, move focus back to textarea
                match self.focus_element {
                    FocusElement::RepositoryButton |
                    FocusElement::BranchButton |
                    FocusElement::ModelButton |
                    FocusElement::GoButton => {
                        self.focus_element = FocusElement::TaskDescription;
                        return false; // Don't exit
                    }
                    _ => {
                        return true; // Exit for other focus elements
                    }
                }
            }
            KeyCode::Up => {
                match self.focus_element {
                    FocusElement::TaskDescription => {
                        // Move up from task description to settings button
                        self.focus_element = FocusElement::SettingsButton;
                    }
                    FocusElement::TaskCard(idx) => {
                        if idx > 0 {
                            self.selected_card = idx - 1;
                            self.focus_element = FocusElement::TaskCard(self.selected_card);
                            // If moving to draft card (index 0), automatically focus description
                            if self.selected_card == 0 && matches!(tasks[0].state, TaskState::Draft) {
                                self.focus_element = FocusElement::TaskDescription;
                            }
                        } else if idx == 0 {
                            // Move up from the first task card to the settings button
                            self.focus_element = FocusElement::SettingsButton;
                        }
                    }
                    _ => {}
                }
            }
            KeyCode::Down => {
                match self.focus_element {
                    FocusElement::SettingsButton => {
                        // Move down from settings button to first task card
                        self.selected_card = 0;
                        self.focus_element = FocusElement::TaskCard(0);
                        // If moving to draft card (index 0), automatically focus description
                        if matches!(tasks[0].state, TaskState::Draft) {
                            self.focus_element = FocusElement::TaskDescription;
                        }
                    }
                    FocusElement::TaskCard(idx) => {
                        if idx < tasks.len() - 1 {
                            self.selected_card = idx + 1;
                            self.focus_element = FocusElement::TaskCard(self.selected_card);
                            // If moving to draft card (index 0), automatically focus description
                            if self.selected_card == 0 && matches!(tasks[0].state, TaskState::Draft) {
                                self.focus_element = FocusElement::TaskDescription;
                            }
                        }
                    }
                    FocusElement::TaskDescription => {
                        // Move from description to first task card
                        self.selected_card = 0;
                        self.focus_element = FocusElement::TaskCard(0);
                    }
                    _ => {}
                }
            }
            KeyCode::Enter => {
                match self.focus_element {
                    FocusElement::TaskDescription => {
                        if shift_pressed {
                            // Shift+Enter: Let tui-textarea handle it (creates newline)
                            let input = tui_textarea::Input {
                                key: tui_textarea::Key::Enter,
                                ctrl: false,
                                alt: false,
                                shift: true,
                            };
                            self.task_description.input(input);
                        } else {
                            // Regular Enter: Launch task
                            let lines: Vec<String> = self.task_description.lines().into_iter().map(|s| s.to_string()).collect();
                            let description = lines.join("\n");
                            if !description.trim().is_empty() {
                                tasks[0].title = description;
                                tasks[0].state = TaskState::Active;
                                tasks[0].activity.push("Thoughts: Starting task execution".to_string());
                                self.focus_element = FocusElement::TaskCard(0);
                            }
                        }
                    }
                    FocusElement::TaskCard(idx) => {
                        if idx == 0 && matches!(tasks[0].state, TaskState::Draft) {
                            self.focus_element = FocusElement::TaskDescription;
                        }
                    }
                    FocusElement::SettingsButton => {
                        // Open settings dialog
                        self.modal_state = ModalState::Settings;
                    }
                    FocusElement::RepositoryButton => {
                        self.open_repository_modal();
                    }
                    FocusElement::BranchButton => {
                        self.open_branch_modal();
                    }
                    FocusElement::ModelButton => {
                        self.open_model_selection_modal();
                    }
                    FocusElement::GoButton => {
                        if !self.task_description.is_empty() {
                            tasks[0].title = self.task_description.lines().join("\n");
                            tasks[0].state = TaskState::Active;
                            tasks[0].activity.push("Thoughts: Starting task execution".to_string());
                            self.focus_element = FocusElement::TaskCard(0);
                        }
                    }
                    FocusElement::StopButton(idx) => {
                        // Stop the task - for now just change it to completed state
                        if idx < tasks.len() && matches!(tasks[idx].state, TaskState::Active) {
                            tasks[idx].state = TaskState::Completed;
                            tasks[idx].activity.clear();
                            tasks[idx].delivery_indicators = Some("⎇ ✓".to_string());
                            // Move focus back to the card
                            self.focus_element = FocusElement::TaskCard(idx);
                        }
                    }
                }
            }
            KeyCode::Tab => {
                match self.focus_element {
                    FocusElement::TaskCard(idx) => {
                        // Move to Stop button for this card
                        self.focus_element = FocusElement::StopButton(idx);
                    }
                    FocusElement::StopButton(idx) => {
                        // Stay on stop button for now
                    }
                    FocusElement::TaskDescription => {
                        self.focus_element = FocusElement::RepositoryButton;
                    }
                    FocusElement::RepositoryButton => {
                        self.focus_element = FocusElement::BranchButton;
                    }
                    FocusElement::BranchButton => {
                        self.focus_element = FocusElement::ModelButton;
                    }
                    FocusElement::ModelButton => {
                        self.focus_element = FocusElement::GoButton;
                    }
                    FocusElement::GoButton => {
                        self.focus_element = FocusElement::TaskDescription;
                    }
                    FocusElement::SettingsButton => {
                        // Settings button is not part of tab cycling within cards
                        // Stay on settings button
                    }
                }
            }
            KeyCode::Right => {
                match self.focus_element {
                    FocusElement::TaskDescription => {
                        // When task description is focused, let textarea handle Right arrow
                        let textarea_input = tui_textarea::Input {
                            key: tui_textarea::Key::Right,
                            ctrl: key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL),
                            alt: key.modifiers.contains(crossterm::event::KeyModifiers::ALT),
                            shift: key.modifiers.contains(crossterm::event::KeyModifiers::SHIFT),
                        };
                        self.task_description.input(textarea_input);
                        return false;
                    }
                    _ => {
                        // For other elements, treat Right as Tab
                        match self.focus_element {
                            FocusElement::TaskCard(idx) => {
                                self.focus_element = FocusElement::StopButton(idx);
                            }
                            FocusElement::StopButton(idx) => {
                                // Stay on stop button
                            }
                            FocusElement::RepositoryButton => {
                                self.focus_element = FocusElement::BranchButton;
                            }
                            FocusElement::BranchButton => {
                                self.focus_element = FocusElement::ModelButton;
                            }
                            FocusElement::ModelButton => {
                                self.focus_element = FocusElement::GoButton;
                            }
                            FocusElement::GoButton => {
                                self.focus_element = FocusElement::TaskDescription;
                            }
                            _ => {}
                        }
                    }
                }
            }
            KeyCode::Left => {
                match self.focus_element {
                    FocusElement::TaskDescription => {
                        // When task description is focused, let textarea handle Left arrow
                        let textarea_input = tui_textarea::Input {
                            key: tui_textarea::Key::Left,
                            ctrl: key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL),
                            alt: key.modifiers.contains(crossterm::event::KeyModifiers::ALT),
                            shift: key.modifiers.contains(crossterm::event::KeyModifiers::SHIFT),
                        };
                        self.task_description.input(textarea_input);
                        return false;
                    }
                    _ => {
                        // For other elements, treat Left as reverse Tab
                        match self.focus_element {
                            FocusElement::TaskCard(idx) => {
                                // Stay on card
                            }
                            FocusElement::StopButton(idx) => {
                                // Stay on stop button
                            }
                            FocusElement::TaskDescription => {
                                self.focus_element = FocusElement::GoButton;
                            }
                            FocusElement::RepositoryButton => {
                                // Can't go left from first button, stay
                            }
                            FocusElement::BranchButton => {
                                self.focus_element = FocusElement::RepositoryButton;
                            }
                            FocusElement::ModelButton => {
                                self.focus_element = FocusElement::BranchButton;
                            }
                            FocusElement::GoButton => {
                                self.focus_element = FocusElement::ModelButton;
                            }
                            FocusElement::SettingsButton => {
                                // Settings button is not part of tab cycling within cards
                                // Stay on settings button
                            }
                            _ => {}
                        }
                    }
                }
            }
            KeyCode::BackTab => {
                match self.focus_element {
                    FocusElement::TaskDescription => {
                        self.focus_element = FocusElement::GoButton;
                    }
                    FocusElement::RepositoryButton => {
                        self.focus_element = FocusElement::TaskDescription;
                    }
                    FocusElement::BranchButton => {
                        self.focus_element = FocusElement::RepositoryButton;
                    }
                    FocusElement::ModelButton => {
                        self.focus_element = FocusElement::BranchButton;
                    }
                    FocusElement::GoButton => {
                        self.focus_element = FocusElement::ModelButton;
                    }
                    FocusElement::SettingsButton => {
                        // Settings button is not part of tab cycling within cards
                        // Stay on settings button
                    }
                    _ => {}
                }
            }
            // Handle text input for description - let TextArea handle it
            _ if matches!(self.focus_element, FocusElement::TaskDescription) && !matches!(key.code, crossterm::event::KeyCode::Enter) => {
                use tui_textarea::{Key, CursorMove};

                // Log the key event for debugging
                log_key_event(&key, "TEXTAREA");

                // Check for CUA shortcuts first
                let ctrl = key.modifiers.intersects(crossterm::event::KeyModifiers::CONTROL);
                let alt = key.modifiers.intersects(crossterm::event::KeyModifiers::ALT);
                if ctrl || alt {
                    match key.code {
                        // Delete word backward: Ctrl+Backspace (CUA), Alt+Backspace (Emacs), Ctrl+H (terminal control code)
                        crossterm::event::KeyCode::Backspace | crossterm::event::KeyCode::Char('h') => {
                            self.task_description.delete_word();
                            return false;
                        }
                        // Delete word forward: Ctrl+Delete (CUA), Alt+Delete (Emacs), Alt+D (Emacs)
                        crossterm::event::KeyCode::Delete => {
                            self.task_description.delete_next_word();
                            return false;
                        }
                        // Move word backward: Ctrl+Left (CUA), Alt+Left, Alt+B (Emacs)
                        crossterm::event::KeyCode::Left => {
                            self.task_description.move_cursor(CursorMove::WordBack);
                            return false;
                        }
                        // Move word forward: Ctrl+Right (CUA), Alt+Right, Alt+F (Emacs)
                        crossterm::event::KeyCode::Right => {
                            self.task_description.move_cursor(CursorMove::WordForward);
                            return false;
                        }
                        // Additional Emacs bindings
                        crossterm::event::KeyCode::Char('b') if alt => {
                            self.task_description.move_cursor(CursorMove::WordBack);
                            return false;
                        }
                        crossterm::event::KeyCode::Char('f') if alt => {
                            self.task_description.move_cursor(CursorMove::WordForward);
                            return false;
                        }
                        crossterm::event::KeyCode::Char('d') if alt => {
                            self.task_description.delete_next_word();
                            return false;
                        }
                        crossterm::event::KeyCode::Char('w') if ctrl => {
                            self.task_description.delete_word();
                            return false;
                        }
                        _ => {}
                    }
                }

                // Fall back to default tui-textarea handling
                let textarea_key = match key.code {
                    crossterm::event::KeyCode::Char(c) => Key::Char(c),
                    crossterm::event::KeyCode::Backspace => Key::Backspace,
                    crossterm::event::KeyCode::Left => Key::Left,
                    crossterm::event::KeyCode::Right => Key::Right,
                    crossterm::event::KeyCode::Up => Key::Up,
                    crossterm::event::KeyCode::Down => Key::Down,
                    crossterm::event::KeyCode::Tab => Key::Tab,
                    crossterm::event::KeyCode::Delete => Key::Delete,
                    crossterm::event::KeyCode::Home => Key::Home,
                    crossterm::event::KeyCode::End => Key::End,
                    _ => Key::Null,
                };

                let textarea_input = tui_textarea::Input {
                    key: textarea_key,
                    ctrl: key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL),
                    alt: key.modifiers.contains(crossterm::event::KeyModifiers::ALT),
                    shift: key.modifiers.contains(crossterm::event::KeyModifiers::SHIFT),
                };

                self.task_description.input(textarea_input);
            }
            _ => {}
        }
        false
    }

    fn handle_model_selection_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crossterm::event::KeyCode;

        if let Some(modal) = &mut self.model_selection_modal {
            match key.code {
                KeyCode::Esc => {
                    // Close modal without saving changes
                    self.close_modal();
                }
                KeyCode::Enter => {
                    if modal.editing_count {
                        // Finish editing count
                        modal.editing_count = false;
                    } else if key.modifiers.intersects(crossterm::event::KeyModifiers::SHIFT) {
                        // Shift+Enter: Save and close modal
                        self.selected_models = modal.selected_models.clone();
                        self.close_modal();
                    } else {
                        // Regular Enter: Add the selected available model
                        if modal.selected_index < modal.available_models.len() {
                            let model_name = modal.available_models[modal.selected_index].clone();
                            // Check if already selected
                            if let Some(existing) = modal.selected_models.iter_mut().find(|m| m.name == model_name) {
                                existing.count += 1;
                            } else {
                                modal.selected_models.push(SelectedModel { name: model_name, count: 1 });
                            }
                        }
                    }
                }
                KeyCode::Tab => {
                    // Toggle between editing count mode and navigation mode
                    modal.editing_count = !modal.editing_count;
                    if modal.editing_count && !modal.selected_models.is_empty() {
                        modal.editing_index = modal.editing_index.min(modal.selected_models.len() - 1);
                    }
                }
                KeyCode::Up => {
                    if modal.editing_count {
                        // Navigate selected models
                        if modal.editing_index > 0 {
                            modal.editing_index -= 1;
                        }
                    } else {
                        // Navigate available models
                        if modal.selected_index > 0 {
                            modal.selected_index -= 1;
                        }
                    }
                }
                KeyCode::Down => {
                    if modal.editing_count {
                        // Navigate selected models
                        if modal.editing_index < modal.selected_models.len().saturating_sub(1) {
                            modal.editing_index = (modal.editing_index + 1).min(modal.selected_models.len() - 1);
                        }
                    } else {
                        // Navigate available models
                        if modal.selected_index < modal.available_models.len().saturating_sub(1) {
                            modal.selected_index = (modal.selected_index + 1).min(modal.available_models.len() - 1);
                        }
                    }
                }
                KeyCode::Left | KeyCode::Char('-') => {
                    if modal.editing_count && modal.editing_index < modal.selected_models.len() {
                        // Decrease count
                        if modal.selected_models[modal.editing_index].count > 1 {
                            modal.selected_models[modal.editing_index].count -= 1;
                        } else {
                            // Remove model if count reaches 0
                            modal.selected_models.remove(modal.editing_index);
                            if modal.editing_index >= modal.selected_models.len() && modal.editing_index > 0 {
                                modal.editing_index -= 1;
                            }
                            modal.editing_count = !modal.selected_models.is_empty();
                        }
                    }
                }
                KeyCode::Right | KeyCode::Char('+') | KeyCode::Char('=') => {
                    if modal.editing_count && modal.editing_index < modal.selected_models.len() {
                        // Increase count
                        modal.selected_models[modal.editing_index].count += 1;
                    }
                }
                _ => {}
            }
        }
        false
    }

    fn handle_settings_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc | KeyCode::Enter => {
                self.modal_state = ModalState::None;
            }
            KeyCode::Char('1') => {
                self.activity_lines_count = 1;
            }
            KeyCode::Char('2') => {
                self.activity_lines_count = 2;
            }
            KeyCode::Char('3') => {
                self.activity_lines_count = 3;
            }
            _ => {}
        }
        false
    }

    fn handle_modal_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        // Log modal key events
        log_key_event(&key, "MODAL");

        if let Some(modal) = &mut self.fuzzy_modal {
            match key.code {
                KeyCode::Esc => {
                    self.close_modal();
                }
                KeyCode::Enter => {
                    if let Some(selected) = modal.options.get(modal.selected_index) {
                        match self.modal_state {
                            ModalState::RepositorySearch => {
                                self.selected_repository = selected.clone();
                            }
                            ModalState::BranchSearch => {
                                self.selected_branch = selected.clone();
                            }
                            _ => {}
                        }
                    }
                    self.close_modal();
                }
                KeyCode::Up => {
                    if modal.selected_index > 0 {
                        modal.selected_index -= 1;
                    }
                }
                KeyCode::Down => {
                    if modal.selected_index < modal.options.len().saturating_sub(1) {
                        modal.selected_index = (modal.selected_index + 1).min(modal.options.len() - 1);
                    }
                }
                _ => {
                    // Check for CUA shortcuts first
                    let ctrl = key.modifiers.intersects(crossterm::event::KeyModifiers::CONTROL);
                    match key.code {
                        KeyCode::Backspace | KeyCode::Char('h') if ctrl => {
                            modal.input.handle(tui_input::InputRequest::DeletePrevWord);
                            modal.selected_index = 0;
                        }
                        KeyCode::Delete if ctrl => {
                            modal.input.handle(tui_input::InputRequest::DeleteNextWord);
                            modal.selected_index = 0;
                        }
                        KeyCode::Left if ctrl => {
                            modal.input.handle(tui_input::InputRequest::GoToPrevWord);
                        }
                        KeyCode::Right if ctrl => {
                            modal.input.handle(tui_input::InputRequest::GoToNextWord);
                        }
                        _ => {}
                    }

                    // Handle text input using tui-input
                    match key.code {
                        KeyCode::Char(c) => {
                            modal.input.handle(tui_input::InputRequest::InsertChar(c));
                            modal.selected_index = 0; // Reset selection when typing
                        }
                        KeyCode::Backspace => {
                            modal.input.handle(tui_input::InputRequest::DeletePrevChar);
                            modal.selected_index = 0;
                        }
                        KeyCode::Delete => {
                            modal.input.handle(tui_input::InputRequest::DeleteNextChar);
                            modal.selected_index = 0;
                        }
                        KeyCode::Left => {
                            modal.input.handle(tui_input::InputRequest::GoToPrevChar);
                        }
                        KeyCode::Right => {
                            modal.input.handle(tui_input::InputRequest::GoToNextChar);
                        }
                        KeyCode::Home => {
                            modal.input.handle(tui_input::InputRequest::GoToStart);
                        }
                        KeyCode::End => {
                            modal.input.handle(tui_input::InputRequest::GoToEnd);
                        }
                        _ => {}
                    }
                }
            }
        }
        false
    }

    fn open_repository_modal(&mut self) {
        self.modal_state = ModalState::RepositorySearch;
        self.fuzzy_modal = Some(FuzzySearchModal {
            input: Input::default(),
            options: vec![
                "agent-harbor".to_string(),
                "ecommerce-platform".to_string(),
                "backend-api".to_string(),
                "frontend-app".to_string(),
                "data-pipeline".to_string(),
            ],
            selected_index: 0,
        });
    }

    fn open_branch_modal(&mut self) {
        self.modal_state = ModalState::BranchSearch;
        self.fuzzy_modal = Some(FuzzySearchModal {
            input: Input::default(),
            options: vec![
                "main".to_string(),
                "develop".to_string(),
                "feature/payments".to_string(),
                "feature/auth".to_string(),
                "hotfix/db-connection".to_string(),
                "release/v1.2.0".to_string(),
            ],
            selected_index: 0,
        });
    }

    fn open_model_selection_modal(&mut self) {
        self.modal_state = ModalState::ModelSelection;
        self.model_selection_modal = Some(ModelSelectionModal {
            available_models: vec![
                "claude-3-5-sonnet".to_string(),
                "claude-3-opus".to_string(),
                "gpt-4".to_string(),
                "gpt-4-turbo".to_string(),
                "claude-3-haiku".to_string(),
            ],
            selected_models: self.selected_models.clone(),
            selected_index: 0,
            editing_count: false,
            editing_index: 0,
        });
    }

    fn close_modal(&mut self) {
        self.modal_state = ModalState::None;
        self.fuzzy_modal = None;
        self.model_selection_modal = None;
    }

    fn simulate_activity(&mut self, tasks: &mut Vec<TaskCard>) {
        // Update ongoing tool executions with high frequency (every 50-200ms)
        for task in tasks.iter_mut() {
            if let TaskState::Active = task.state {
                if task.current_tool_execution.is_some() {
                    // High-frequency updates for realistic progress
                    let fast_update_chance = rand::random::<u8>() % 5; // 20% chance per frame
                    if fast_update_chance == 0 {
                        task.update_tool_execution();
                    }
                }
            }
        }

        // Start new activities every 3-8 seconds (only when no tool is running)
        let activity_interval = Duration::from_secs(3 + (rand::random::<u64>() % 5));
        if self.activity_timer.elapsed() > activity_interval {
            for task in tasks.iter_mut() {
                if let TaskState::Active = task.state {
                    if task.current_tool_execution.is_none() {
                    // Choose activity type
                    let activity_type = rand::random::<u8>() % 4;

                    match activity_type {
                        0 => {
                            // Start thinking
                            let thoughts = vec![
                                "Analyzing codebase structure and dependencies",
                                "Considering edge cases and error handling",
                                "Planning the implementation strategy",
                                "Reviewing existing patterns and conventions",
                                "Evaluating performance implications",
                                "Checking for potential security issues",
                                "Assessing test coverage requirements",
                            ];
                            if let Some(thought) = thoughts.choose(&mut rand::thread_rng()) {
                                task.add_thought(thought);
                            }
                        }
                        1 => {
                            // Start file edit
                            let files = vec![
                                ("src/auth.rs", 5, 3),
                                ("src/api.rs", 12, 7),
                                ("src/models.rs", 8, 2),
                                ("tests/auth_test.rs", 15, 4),
                                ("src/lib.rs", 3, 1),
                                ("src/config.rs", 6, 8),
                                ("src/utils.rs", 9, 5),
                            ];
                            if let Some((file, added, removed)) = files.choose(&mut rand::thread_rng()) {
                                task.add_file_edit(file, *added, *removed);
                            }
                        }
                        2 => {
                            // Start tool execution
                            let tools = vec![
                                ("cargo build", ""),
                                ("cargo check", ""),
                                ("cargo test", ""),
                                ("read_file", "src/main.rs"),
                                ("grep", "TODO|FIXME"),
                            ];
                            if let Some((tool, args)) = tools.choose(&mut rand::thread_rng()) {
                                task.start_tool_execution(tool, args);
                            }
                        }
                        _ => {
                            // Another thought
                            let thoughts = vec![
                                "Optimizing database queries for better performance",
                                "Implementing proper error handling and logging",
                                "Adding comprehensive input validation",
                                "Creating unit tests for new functionality",
                                "Updating documentation and comments",
                            ];
                            if let Some(thought) = thoughts.choose(&mut rand::thread_rng()) {
                                task.add_thought(thought);
                            }
                        }
                    }
                    }
                }
            }
            self.activity_timer = Instant::now();
        }
    }
}

fn run_app() -> Result<(), Box<dyn std::error::Error>> {
    // This is a wrapper that will be replaced by run_app_with_interrupt
    run_app_internal(&Arc::new(AtomicBool::new(true)), true)
}

fn run_app_internal(running: &Arc<AtomicBool>, enable_raw_mode: bool) -> Result<(), Box<dyn std::error::Error>> {
    // Log run_app start
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("key_log.txt")
    {
        let _ = writeln!(file, "=== run_app started ===");
    }
    // Setup terminal with state tracking
    setup_terminal(enable_raw_mode)?;
    let mut stdout = io::stdout();
    queue!(stdout,
        PushKeyboardEnhancementFlags(
            KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
            | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
            | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
            | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
        )
    )?;
    KB_FLAGS_PUSHED.store(true, Ordering::SeqCst);
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;


    let theme = Theme::default();

    // Initialize image picker and logo protocol for logo rendering
    let (image_picker, mut logo_protocol) = initialize_logo_rendering(theme.bg);

    // Initialize app state
    let mut app_state = AppState::new();
    let mut tasks = create_sample_tasks();

    // Create channels for event handling
    let (tx_ev, rx_ev) = chan::unbounded::<Event>();
    let (tx_tick, rx_tick) = chan::unbounded::<()>();

    // Event reader thread (blocks, near-zero latency)
    thread::spawn(move || {
        loop {
            match crossterm::event::read() {
                Ok(ev) => {
                    let _ = tx_ev.send(ev);
                }
                Err(_) => break,
            }
        }
    });

    // Tick thread for periodic updates (~60 FPS)
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_millis(16)); // ~60 FPS
            let _ = tx_tick.send(());
        }
    });

    // Run the app
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        loop {
            // Check if we should exit due to interrupt signal
            if !running.load(Ordering::SeqCst) {
                break;
            }

            // Update task description in the draft card
            if let TaskState::Draft = tasks[0].state {
                tasks[0].title = app_state.task_description.lines().join("\n");
                tasks[0].repository = app_state.selected_repository.clone();
                tasks[0].branch = app_state.selected_branch.clone();
                tasks[0].agents = app_state.selected_models.clone();
            }

            // Simulate activity for active tasks
            app_state.simulate_activity(&mut tasks);

            terminal.draw(|frame| {
                let size = frame.area();

                // Background fill with theme color
                let bg = Paragraph::new("").style(Style::default().bg(theme.bg));
                frame.render_widget(bg, size);

                // Main layout
                let main_layout = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(9),  // Header with logo (larger for better visibility)
                        Constraint::Min(10),    // Tasks area
                        Constraint::Length(1),  // Footer
                        Constraint::Length(1),  // Bottom padding
                    ])
                    .split(size);

                // Render header
                render_header(frame, main_layout[0], &theme, &app_state.focus_element, image_picker.as_ref(), logo_protocol.as_mut());

                // Render tasks with screen edge padding
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
                let tasks_layout = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints(
                        tasks.iter().enumerate().map(|(i, task)| {
                            // All cards have fixed height, including spacing
                            Constraint::Length(task.height(app_state.activity_lines_count) + if i < tasks.len() - 1 { 1 } else { 0 }) // +1 for spacing between cards, but not after last
                        }).collect::<Vec<_>>()
                    )
                    .split(tasks_area);

                for (i, task) in tasks.iter().enumerate() {
                    // All cards now have fixed height
                    let task_area = tasks_layout[i];

                    let is_selected = matches!(app_state.focus_element, FocusElement::TaskCard(idx) if idx == i) ||
                        // For draft card (index 0), also highlight when any of its sub-elements are focused
                        (i == 0 && matches!(task.state, TaskState::Draft) && matches!(app_state.focus_element,
                            FocusElement::TaskDescription |
                            FocusElement::RepositoryButton |
                            FocusElement::BranchButton |
                            FocusElement::ModelButton |
                            FocusElement::GoButton
                        ));
                    task.render(frame, task_area, &app_state, &theme, is_selected, i);
                }

                // Render footer
                render_footer(frame, main_layout[2], &app_state.focus_element, &theme);

                // Render modal if active
                if let Some(modal) = &app_state.fuzzy_modal {
                    render_fuzzy_modal(frame, modal, size, &theme);
                }
                if let Some(modal) = &app_state.model_selection_modal {
                    render_model_selection_modal(frame, modal, size, &theme);
                }
                if matches!(app_state.modal_state, ModalState::Settings) {
                    render_settings_dialog(frame, &app_state, size, &theme);
                }
            })?;

            // Event-driven main loop
            chan::select! {
                recv(rx_ev) -> msg => {
                    let ev = match msg {
                        Ok(e) => e,
                        Err(_) => break,
                    };
                    // Handle input event
                    if let Event::Key(key) = ev {
                        // Handle key press and repeat events (for key repeating)
                        if key.kind == crossterm::event::KeyEventKind::Press || key.kind == crossterm::event::KeyEventKind::Repeat {
                            if app_state.handle_key(key, &mut tasks) {
                                break; // Exit
                            }
                        }
                    }
                }
                recv(rx_tick) -> _ => {
                    // Periodic tick - could be used for animations, but currently just continue
                    // This keeps the app responsive even when no input events occur
                }
            }
        }
        Ok(())
    })();

    result
}

/// Convert a Ratatui color into raw RGB components (default to black for non-RGB variants).
fn color_to_rgb_components(color: Color) -> (u8, u8, u8) {
    match color {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => (0, 0, 0),
    }
}

/// Blend the transparent regions of the logo onto the TUI background color before rendering.
fn precompose_on_background(image: DynamicImage, bg_color: Color) -> DynamicImage {
    let (r, g, b) = color_to_rgb_components(bg_color);
    let rgba_logo = image.to_rgba8();
    let (width, height) = rgba_logo.dimensions();
    let mut background = RgbaImage::from_pixel(width, height, Rgba([r, g, b, 255]));
    image::imageops::overlay(&mut background, &rgba_logo, 0, 0);
    DynamicImage::ImageRgba8(background)
}

/// Pad the image width so it fills complete terminal cells, avoiding partially transparent columns.
fn pad_to_cell_width(image: DynamicImage, bg_color: Color, cell_width: Option<u16>) -> DynamicImage {
    let cell_width = match cell_width {
        Some(width) if width > 0 => width as u32,
        _ => return image,
    };

    let (width, height) = image.dimensions();
    let remainder = width % cell_width;
    if remainder == 0 {
        return image;
    }

    let pad_width = cell_width - remainder;
    let (r, g, b) = color_to_rgb_components(bg_color);
    let mut canvas = RgbaImage::from_pixel(width + pad_width, height, Rgba([r, g, b, 255]));
    image::imageops::overlay(&mut canvas, &image.to_rgba8(), 0, 0);
    DynamicImage::ImageRgba8(canvas)
}

/// Initialize logo rendering components (Picker and StatefulProtocol)
fn initialize_logo_rendering(bg_color: Color) -> (Option<Picker>, Option<StatefulProtocol>) {
    // Try to create a picker that detects terminal graphics capabilities
    let picker = match Picker::from_query_stdio() {
        Ok(picker) => Some(picker),
        Err(_) => {
            // If we can't detect terminal capabilities, try with default font size
            // This allows for basic image processing
            Some(Picker::from_fontsize((8, 16)))
        }
    };

    // Try to load and encode the logo image
    let logo_protocol = if let Some(ref picker) = picker {
        let cell_width = Some(picker.font_size().0);
        // Try to load the PNG logo
        match ImageReader::open("../../assets/agent-harbor-logo.png") {
            Ok(reader) => match reader.decode() {
                Ok(img) => {
                    // Compose the transparent logo onto the themed background before encoding.
                    let composed = precompose_on_background(img, bg_color);
                    let prepared = pad_to_cell_width(composed, bg_color, cell_width);
                    Some(picker.new_resize_protocol(prepared) as StatefulProtocol)
                }
                Err(_) => None,
            },
            Err(_) => None,
        }
    } else {
        None
    };

    (picker, logo_protocol)
}

/// Generate ASCII logo for Agent Harbor
fn generate_ascii_logo() -> Vec<Line<'static>> {
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


// Global flag to ensure cleanup only happens once
static CLEANUP_DONE: AtomicBool = AtomicBool::new(false);

// Track what we modified so we can restore properly
static RAW_MODE_ENABLED: AtomicBool = AtomicBool::new(false);
static ALTERNATE_SCREEN_ACTIVE: AtomicBool = AtomicBool::new(false);
static KB_FLAGS_PUSHED: AtomicBool = AtomicBool::new(false);

fn setup_terminal(enable_raw_mode: bool) -> Result<(), Box<dyn std::error::Error>> {
    // Check current raw mode state
    let was_raw_mode = crossterm::terminal::is_raw_mode_enabled()?;

    if enable_raw_mode {
        // Enable raw mode and track that we did it
        crossterm::terminal::enable_raw_mode()?;
        RAW_MODE_ENABLED.store(!was_raw_mode, Ordering::SeqCst);
    }

    // Enter alternate screen and track it
    crossterm::execute!(std::io::stdout(), crossterm::terminal::EnterAlternateScreen)?;
    ALTERNATE_SCREEN_ACTIVE.store(true, Ordering::SeqCst);

    Ok(())
}

fn cleanup_terminal() {
    if CLEANUP_DONE.swap(true, Ordering::SeqCst) {
        return; // Already cleaned up
    }

    // Pop keyboard enhancement flags first (must be done while still in raw mode/alternate screen)
    if KB_FLAGS_PUSHED.load(Ordering::SeqCst) {
        let _ = crossterm::execute!(std::io::stdout(), PopKeyboardEnhancementFlags);
        KB_FLAGS_PUSHED.store(false, Ordering::SeqCst);
    }

    // Disable raw mode next
    if RAW_MODE_ENABLED.load(Ordering::SeqCst) {
        let _ = crossterm::terminal::disable_raw_mode();
        RAW_MODE_ENABLED.store(false, Ordering::SeqCst);
    }

    // Leave alternate screen last
    if ALTERNATE_SCREEN_ACTIVE.load(Ordering::SeqCst) {
        let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen);
        ALTERNATE_SCREEN_ACTIVE.store(false, Ordering::SeqCst);
    }
}

/// Cleanup terminal state and exit with the given code.
/// This should be used instead of process::exit() to ensure proper cleanup.
fn cleanup_and_exit(code: i32) -> ! {
    cleanup_terminal();
    std::process::exit(code);
}

/// Parse command line arguments
struct Args {
    enable_raw_mode: bool,
}

fn parse_args() -> Args {
    let args: Vec<String> = std::env::args().collect();
    let enable_raw_mode = !args.contains(&"--no-raw-mode".to_string());

    if args.contains(&"--help".to_string()) || args.contains(&"-h".to_string()) {
        println!("Usage: {} [OPTIONS]", args.get(0).unwrap_or(&"tui-exploration".to_string()));
        println!();
        println!("Options:");
        println!("  --no-raw-mode    Disable raw mode (useful for debugging, disables keyboard input)");
        println!("  --help, -h       Show this help message");
        std::process::exit(0);
    }

    Args { enable_raw_mode }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args();

    // Simple test logging
    println!("Main function reached");
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("key_log.txt")
    {
        let _ = writeln!(file, "=== Application started ===");
        println!("Log file created");
    } else {
        println!("Failed to create log file");
    }

    // Install signal handler for graceful shutdown
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        cleanup_terminal();
        r.store(false, Ordering::SeqCst);
        // Don't exit here - let the main thread handle it
    }).expect("Error setting Ctrl-C handler");

    // Install panic hook for cleanup on panic
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        cleanup_terminal();
        // Call the default panic handler
        default_panic(panic_info);
    }));

    // Run the app with panic-safe cleanup
    let result = std::panic::catch_unwind(|| {
        run_app_internal(&running, args.enable_raw_mode)
    });

    // Ensure cleanup happens (in case catch_unwind didn't catch something)
    cleanup_terminal();

    // Handle the result
    match result {
        Ok(inner_result) => inner_result,
        Err(_) => {
            eprintln!("Application panicked, but terminal has been restored.");
            cleanup_and_exit(1);
        }
    }
}
