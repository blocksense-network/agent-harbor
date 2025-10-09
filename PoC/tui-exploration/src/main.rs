use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph, Wrap},
    Frame, Terminal,
};
use std::io;
use std::time::{Duration, Instant};
use rand::seq::SliceRandom;
use image::ImageReader;
use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;

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

struct TaskCard {
    title: String,
    repository: String,
    branch: String,
    agent: String,
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
}

#[derive(Debug, Clone, PartialEq)]
enum ModalState {
    None,
    RepositorySearch,
    BranchSearch,
    ModelSearch,
}

#[derive(Debug, Clone)]
struct FuzzySearchModal {
    query: String,
    cursor_position: usize,
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
            .padding(Padding::new(2, 2, 1, 1))
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
    task_description: String,
    description_cursor: usize,
    selected_repository: String,
    selected_branch: String,
    selected_model: String,
    activity_timer: Instant,
}

impl TaskCard {
    fn height(&self) -> u16 {
        match self.state {
            TaskState::Completed => 3, // Title + metadata + padding (2 lines content)
            TaskState::Active => 9, // Title with metadata + 3 activity lines + padding
            TaskState::Draft => 6, // Variable height for draft (no card_block)
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

    fn get_recent_activity(&self) -> Vec<String> {
        if let TaskState::Active = self.state {
            // Return last 3 activities, formatted for display
            let recent = self.activity.iter().rev().take(3).cloned().collect::<Vec<_>>();
            let mut result = recent.into_iter().rev().collect::<Vec<_>>();

            // Always return exactly 3 lines
            while result.len() < 3 {
                result.push("".to_string());
            }

            result
        } else {
            vec!["".to_string(), "".to_string(), "".to_string()]
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
                // Draft cards don't use card_block - render directly like ah-tui
                // Show selection border only when card is selected but not in description edit mode
                if is_selected && !matches!(app_state.focus_element, FocusElement::TaskDescription) {
                    let border_block = Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(theme.primary).add_modifier(Modifier::BOLD))
                        .title("┤ New Task ├")
                        .title_alignment(ratatui::layout::Alignment::Left)
                        .title_style(Style::default().fg(theme.primary).add_modifier(Modifier::BOLD));

                    let inner_area = border_block.inner(area);
                    frame.render_widget(border_block, area);
                    self.render_draft_card_content(frame, inner_area, app_state, theme);
                } else {
                    self.render_draft_card(frame, area, app_state, theme);
                }
            }
            _ => {
                let display_title = match self.state {
                    TaskState::Active => {
                        // Show actual task title, truncated if too long
                        if self.title.len() > 40 {
                            format!("{}...", &self.title[..37])
                        } else {
                            self.title.clone()
                        }
                    }
                    TaskState::Completed => {
                        // Show actual task title, truncated if too long
                        if self.title.len() > 40 {
                            format!("{}...", &self.title[..37])
                        } else {
                            self.title.clone()
                        }
                    }
                    _ => unreachable!(),
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

                match self.state {
                    TaskState::Completed => self.render_completed_card(frame, inner_area, theme),
                    TaskState::Active => {
                        let is_stop_focused = matches!(app_state.focus_element, FocusElement::StopButton(idx) if idx == card_index);
                        self.render_active_card(frame, inner_area, theme, is_stop_focused)
                    },
                    TaskState::Draft => unreachable!(),
                }
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

        let metadata_line = Line::from(vec![
            Span::styled(&self.repository, Style::default().fg(theme.muted)),
            Span::raw(" • "),
            Span::styled(&self.branch, Style::default().fg(theme.muted)),
            Span::raw(" • "),
            Span::styled(&self.agent, Style::default().fg(theme.muted)),
            Span::raw(" • "),
            Span::styled(&self.timestamp, Style::default().fg(theme.muted)),
        ]);

        let paragraph = Paragraph::new(vec![title_line, metadata_line])
            .wrap(Wrap { trim: true });

        frame.render_widget(paragraph, area);
    }

    fn render_active_card(&self, frame: &mut Frame, area: Rect, theme: &Theme, is_stop_focused: bool) {
        // First line: metadata on left, Stop button on right
        let metadata_part = vec![
            Span::styled(
                "● ",
                Style::default().fg(theme.warning).add_modifier(Modifier::BOLD),
            ),
            Span::styled(&self.repository, Style::default().fg(theme.muted)),
            Span::raw(" • "),
            Span::styled(&self.branch, Style::default().fg(theme.muted)),
            Span::raw(" • "),
            Span::styled(&self.agent, Style::default().fg(theme.muted)),
            Span::raw(" • "),
            Span::styled(&self.timestamp, Style::default().fg(theme.muted)),
        ];

        // Calculate how much space we need for the right-aligned Stop button
        let metadata_text = format!("● {} • {} • {} • {}", self.repository, self.branch, self.agent, self.timestamp);
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

        let activity_vec = self.get_recent_activity();
        let activity_lines: Vec<Line> = activity_vec.into_iter().enumerate().map(|(i, activity)| {
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
        }).collect();

        let all_lines = vec![
            title_line,
            Line::from(""),
            activity_lines.get(0).cloned().unwrap_or_else(|| Line::from("")),
            activity_lines.get(1).cloned().unwrap_or_else(|| Line::from("")),
            activity_lines.get(2).cloned().unwrap_or_else(|| Line::from("")),
        ];

        // Render each line individually to ensure proper display
        for (i, line) in all_lines.iter().enumerate() {
            if i < area.height as usize {
                let line_area = Rect::new(area.x, area.y + i as u16, area.width, 1);
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
        let mut all_lines = Vec::new();

        // Description area - exact style from ah-tui
        all_lines.push(Line::from(vec![
            Span::styled("┌─ Description ", Style::default().fg(theme.border)),
            Span::styled("─".repeat(65), Style::default().fg(theme.border)),
            Span::raw("─┐"),
        ]));

        // Description content area with background highlighting
        let is_description_focused = matches!(app_state.focus_element, FocusElement::TaskDescription);
        let description_bg = if is_description_focused {
            theme.primary // Highlight when focused
        } else {
            theme.surface
        };

        if self.title.is_empty() {
            all_lines.push(Line::from(vec![
                Span::raw("│ "),
                Span::styled(
                    "Enter task description...".to_string() + &" ".repeat(45),
                    Style::default().bg(description_bg).fg(if is_description_focused { theme.bg } else { theme.muted }),
                ),
                Span::raw(" │"),
            ]));
        } else {
            // Split description into lines if it contains newlines
            let lines: Vec<&str> = self.title.split('\n').collect();
            for (i, line) in lines.iter().enumerate() {
                if i >= 2 { break; } // Max 2 lines for display
                let padded_line = format!("│ {:<69} │", line);
                all_lines.push(Line::from(vec![Span::styled(
                    padded_line,
                    Style::default().bg(description_bg).fg(if is_description_focused { theme.bg } else { theme.text }),
                )]));
            }
            // Add empty line if description is short
            if lines.len() < 2 {
                all_lines.push(Line::from(vec![Span::styled(
                    "│                                                                     │",
                    Style::default().bg(description_bg),
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

        // Button row at the bottom with Charm styling - exactly like ah-tui
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

        let models_button_text = if self.agent.is_empty() {
            "🤖 Models".to_string()
        } else {
            format!("🤖 {}", self.agent)
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

        all_lines.push(button_line);

        // Render each line of content within the area
        for (i, line) in all_lines.iter().enumerate() {
            if i < area.height as usize {
                let line_area = Rect::new(area.x, area.y + i as u16, area.width, 1);
                let para = Paragraph::new(line.clone());
                frame.render_widget(para, line_area);
            }
        }

        // Set cursor position when description is focused
        if matches!(app_state.focus_element, FocusElement::TaskDescription) {
            // When editing description, cursor is positioned in the description area
            let cursor_x = area.x + 2 + app_state.description_cursor.min(self.title.len()) as u16;
            let cursor_y = area.y + 1; // Second line of description area
            if cursor_x < area.x + area.width - 2 && cursor_y < area.y + area.height {
                frame.set_cursor_position(ratatui::layout::Position::new(cursor_x, cursor_y));
            }
        }
    }
}

fn render_header(frame: &mut Frame, area: Rect, _theme: &Theme, _image_picker: Option<&Picker>, logo_protocol: Option<&mut StatefulProtocol>) {
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

    // Search input
    let input_area = Rect {
        x: inner_area.x,
        y: inner_area.y,
        width: inner_area.width,
        height: 3,
    };

    let input_block = Block::default()
        .borders(Borders::ALL)
        .title("Query");

    let input_inner = input_block.inner(input_area);
    frame.render_widget(input_block, input_area);

    let query_display = if modal.query.is_empty() {
        Span::styled("Type to search...", Style::default().fg(theme.muted))
    } else {
        Span::styled(&modal.query, Style::default().fg(theme.text))
    };

    frame.render_widget(
        Paragraph::new(Line::from(query_display)).wrap(Wrap { trim: true }),
        input_inner
    );

    // Show cursor position
    if !modal.query.is_empty() && modal.cursor_position <= modal.query.len() {
        let cursor_x = input_inner.x + modal.cursor_position as u16;
        let cursor_y = input_inner.y;
        if cursor_x < input_inner.x + input_inner.width && cursor_y < input_inner.y + input_inner.height {
            frame.set_cursor_position(ratatui::layout::Position::new(cursor_x, cursor_y));
        }
    }

    // Results list
    let results_area = Rect {
        x: inner_area.x,
        y: inner_area.y + 3,
        width: inner_area.width,
        height: inner_area.height - 3,
    };

    let results_block = Block::default()
        .borders(Borders::ALL)
        .title(format!("Results ({})", modal.options.len()));

    let results_inner = results_block.inner(results_area);
    frame.render_widget(results_block, results_area);

    // Filter options based on query
    let filtered_options: Vec<&String> = if modal.query.is_empty() {
        modal.options.iter().take(10).collect()
    } else {
        modal.options.iter()
            .filter(|opt| opt.to_lowercase().contains(&modal.query.to_lowercase()))
            .take(10)
            .collect()
    };

    // Display filtered results
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
        results_inner
    );
}

fn create_sample_tasks() -> Vec<TaskCard> {
    vec![
        TaskCard {
            title: "".to_string(), // Will be filled by user input
            repository: "agent-harbor".to_string(),
            branch: "main".to_string(),
            agent: "claude-3-5-sonnet".to_string(),
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
            agent: "claude-3-5-sonnet".to_string(),
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
            agent: "gpt-4".to_string(),
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
            agent: "claude-3-5-sonnet".to_string(),
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
            agent: "gpt-4".to_string(),
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
            agent: "claude-3-5-sonnet".to_string(),
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
        Self {
            selected_card: 0,
            focus_element: FocusElement::TaskDescription,
            modal_state: ModalState::None,
            fuzzy_modal: None,
            task_description: String::new(),
            description_cursor: 0,
            selected_repository: "agent-harbor".to_string(),
            selected_branch: "main".to_string(),
            selected_model: "claude-3-5-sonnet".to_string(),
            activity_timer: Instant::now(),
        }
    }

    fn update_task_description(&mut self, description: String) {
        self.task_description = description;
    }

    fn handle_key(&mut self, key: crossterm::event::KeyEvent, tasks: &mut Vec<TaskCard>) -> bool {

        match self.modal_state {
            ModalState::None => self.handle_main_key(key, tasks),
            ModalState::RepositorySearch | ModalState::BranchSearch | ModalState::ModelSearch => {
                self.handle_modal_key(key)
            }
        }
    }

    fn handle_main_key(&mut self, key: crossterm::event::KeyEvent, tasks: &mut Vec<TaskCard>) -> bool {
        use crossterm::event::{KeyCode, KeyModifiers};

        let shift_pressed = key.modifiers.contains(KeyModifiers::SHIFT);

        match key.code {
            KeyCode::Esc => {
                return true; // Exit
            }
            KeyCode::Up => {
                match self.focus_element {
                    FocusElement::TaskCard(idx) => {
                        if idx > 0 {
                            self.selected_card = idx - 1;
                            self.focus_element = FocusElement::TaskCard(self.selected_card);
                        }
                    }
                    _ => {}
                }
            }
            KeyCode::Down => {
                match self.focus_element {
                    FocusElement::TaskCard(idx) => {
                        if idx < tasks.len() - 1 {
                            self.selected_card = idx + 1;
                            self.focus_element = FocusElement::TaskCard(self.selected_card);
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
                    FocusElement::TaskCard(idx) => {
                        if idx == 0 && matches!(tasks[0].state, TaskState::Draft) {
                            self.focus_element = FocusElement::TaskDescription;
                        }
                    }
                    FocusElement::TaskDescription => {
                        if !self.task_description.is_empty() {
                            tasks[0].title = self.task_description.clone();
                            tasks[0].state = TaskState::Active;
                            tasks[0].activity.push("Thoughts: Starting task execution".to_string());
                            self.focus_element = FocusElement::TaskCard(0);
                        }
                    }
                    FocusElement::RepositoryButton => {
                        self.open_repository_modal();
                    }
                    FocusElement::BranchButton => {
                        self.open_branch_modal();
                    }
                    FocusElement::ModelButton => {
                        self.open_model_modal();
                    }
                    FocusElement::GoButton => {
                        if !self.task_description.is_empty() {
                            tasks[0].title = self.task_description.clone();
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
            KeyCode::Tab | KeyCode::Right => {
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
                    _ => {}
                }
            }
            KeyCode::Left => {
                match self.focus_element {
                    FocusElement::StopButton(idx) => {
                        // Move back to the card
                        self.focus_element = FocusElement::TaskCard(idx);
                    }
                    _ => {}
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
                    _ => {}
                }
            }
            KeyCode::Char(c) if matches!(self.focus_element, FocusElement::TaskDescription) => {
                if c == '\n' && shift_pressed {
                    // Shift+Enter for new line
                    self.task_description.insert(self.description_cursor, '\n');
                    self.description_cursor += 1;
                } else if c != '\n' {
                    // Regular character - direct typing
                    self.task_description.insert(self.description_cursor, c);
                    self.description_cursor += 1;
                }
            }
            KeyCode::Backspace if matches!(self.focus_element, FocusElement::TaskDescription) => {
                if self.description_cursor > 0 {
                    self.description_cursor -= 1;
                    self.task_description.remove(self.description_cursor);
                }
            }
            KeyCode::Left if matches!(self.focus_element, FocusElement::TaskDescription) => {
                if self.description_cursor > 0 {
                    self.description_cursor -= 1;
                }
            }
            KeyCode::Right if matches!(self.focus_element, FocusElement::TaskDescription) => {
                if self.description_cursor < self.task_description.len() {
                    self.description_cursor += 1;
                }
            }
            _ => {}
        }
        false
    }

    fn handle_modal_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crossterm::event::KeyCode;

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
                            ModalState::ModelSearch => {
                                self.selected_model = selected.clone();
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
                KeyCode::Char(c) => {
                    modal.query.insert(modal.cursor_position, c);
                    modal.cursor_position += 1;
                    modal.selected_index = 0;
                }
                KeyCode::Backspace => {
                    if modal.cursor_position > 0 {
                        modal.cursor_position -= 1;
                        modal.query.remove(modal.cursor_position);
                        modal.selected_index = 0;
                    }
                }
                _ => {}
            }
        }
        false
    }

    fn open_repository_modal(&mut self) {
        self.modal_state = ModalState::RepositorySearch;
        self.fuzzy_modal = Some(FuzzySearchModal {
            query: String::new(),
            cursor_position: 0,
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
            query: String::new(),
            cursor_position: 0,
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

    fn open_model_modal(&mut self) {
        self.modal_state = ModalState::ModelSearch;
        self.fuzzy_modal = Some(FuzzySearchModal {
            query: String::new(),
            cursor_position: 0,
            options: vec![
                "claude-3-5-sonnet".to_string(),
                "claude-3-opus".to_string(),
                "gpt-4".to_string(),
                "gpt-4-turbo".to_string(),
                "claude-3-haiku".to_string(),
            ],
            selected_index: 0,
        });
    }

    fn close_modal(&mut self) {
        self.modal_state = ModalState::None;
        self.fuzzy_modal = None;
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
    // Setup terminal
    let mut stdout = io::stdout();
    crossterm::terminal::enable_raw_mode()?;
    crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    // Initialize image picker and logo protocol for logo rendering
    let (image_picker, mut logo_protocol) = initialize_logo_rendering();

    // Initialize app state
    let mut app_state = AppState::new();
    let mut tasks = create_sample_tasks();
    let theme = Theme::default();

    // Run the app
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        loop {
            // Update task description in the draft card
            if let TaskState::Draft = tasks[0].state {
                tasks[0].title = if app_state.task_description.is_empty() {
                    String::new()
                } else {
                    app_state.task_description.clone()
                };
                tasks[0].repository = app_state.selected_repository.clone();
                tasks[0].branch = app_state.selected_branch.clone();
                tasks[0].agent = app_state.selected_model.clone();
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
                render_header(frame, main_layout[0], &theme, image_picker.as_ref(), logo_protocol.as_mut());

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
                            Constraint::Length(task.height() + if i < tasks.len() - 1 { 1 } else { 0 }) // +1 for spacing between cards, but not after last
                        }).collect::<Vec<_>>()
                    )
                    .split(tasks_area);

                for (i, task) in tasks.iter().enumerate() {
                    // All cards now have fixed height
                    let task_area = tasks_layout[i];

                    let is_selected = matches!(app_state.focus_element, FocusElement::TaskCard(idx) if idx == i);
                    task.render(frame, task_area, &app_state, &theme, is_selected, i);
                }

                // Render footer
                render_footer(frame, main_layout[2], &app_state.focus_element, &theme);

                // Render modal if active
                if let Some(modal) = &app_state.fuzzy_modal {
                    render_fuzzy_modal(frame, modal, size, &theme);
                }
            })?;

            // Handle input
            if crossterm::event::poll(std::time::Duration::from_millis(100))? {
                if let crossterm::event::Event::Key(key) = crossterm::event::read()? {
                    if app_state.handle_key(key, &mut tasks) {
                        break; // Exit
                    }
                }
            }
        }
        Ok(())
    })();

    // Restore terminal
    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen
    )?;

    result
}

/// Initialize logo rendering components (Picker and StatefulProtocol)
fn initialize_logo_rendering() -> (Option<Picker>, Option<StatefulProtocol>) {
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
        // Try to load the PNG logo
        match ImageReader::open("../../assets/agent-harbor-logo.png") {
            Ok(reader) => match reader.decode() {
                Ok(img) => {
                    // Create a resize protocol that fits the image appropriately
                    Some(picker.new_resize_protocol(img) as StatefulProtocol)
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    run_app()
}
