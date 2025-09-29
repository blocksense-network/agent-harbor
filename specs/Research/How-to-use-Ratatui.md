# Ratatui API Guide for AW TUI Implementation

## Overview

This guide provides a comprehensive overview of Ratatui APIs that can be used to implement the Agents Workflow (AW) Terminal User Interface (TUI) as specified in the TUI PRD and [CLI.md](../Public/CLI.md). Ratatui's modular architecture and rich widget ecosystem make it perfectly suited for building complex terminal dashboards.

## Core Architecture Mapping

### TUI Requirements → Ratatui Components

| AW TUI Component               | Ratatui API                               | Key Features Used                             |
| ------------------------------ | ----------------------------------------- | --------------------------------------------- |
| Dashboard Layout               | `Layout`, `Constraint`                    | Vertical/horizontal splits, responsive design |
| Project/Branch/Agent Selectors | `List`, `ListState`                       | Filtering, navigation, highlighting           |
| Task Description Editor        | `Paragraph`, `Frame::set_cursor_position` | Multi-line input, cursor management           |
| Status/Error Display           | `Paragraph`, `Block`                      | Styled text, borders                          |
| Hotkey System                  | Event handling                            | Key combinations, modifiers                   |
| Multiplexer Integration        | Backend selection                         | Crossterm for terminal control                |

## 1. Application Structure

### Basic App Template

```rust
use color_eyre::Result;
use ratatui::{DefaultTerminal, Frame};

struct App {
    // State management for all UI components
    project_selector: ListState,
    branch_selector: ListState,
    agent_selector: ListState,
    task_input: String,
    cursor_position: usize,
    should_exit: bool,
}

impl App {
    fn new() -> Self {
        Self {
            project_selector: ListState::default(),
            branch_selector: ListState::default(),
            agent_selector: ListState::default(),
            task_input: String::new(),
            cursor_position: 0,
            should_exit: false,
        }
    }

    fn run(mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        while !self.should_exit {
            terminal.draw(|frame| self.render(frame))?;
            self.handle_events()?;
        }
        Ok(())
    }

    fn render(&mut self, frame: &mut Frame) {
        // Layout and rendering logic
    }

    fn handle_events(&mut self) -> Result<()> {
        // Event handling logic
        Ok(())
    }
}

fn main() -> Result<()> {
    color_eyre::install()?;
    let mut terminal = ratatui::init();
    let app = App::new();
    let result = app.run(&mut terminal);
    ratatui::restore();
    result
}
```

## 2. Layout System

### Dashboard Layout Structure

```rust
use ratatui::layout::{Constraint, Layout, Rect};

impl App {
    fn render(&mut self, frame: &mut Frame) {
        // Main vertical layout: selectors + input + status
        let main_layout = Layout::vertical([
            Constraint::Length(3),  // Project selector
            Constraint::Length(3),  // Branch selector
            Constraint::Length(3),  // Agent selector
            Constraint::Fill(1),    // Task input area
            Constraint::Length(1),  // Status bar
        ]);

        let [project_area, branch_area, agent_area, input_area, status_area] =
            main_layout.areas(frame.area());

        // Render each component
        self.render_project_selector(frame, project_area);
        self.render_branch_selector(frame, branch_area);
        self.render_agent_selector(frame, agent_area);
        self.render_task_input(frame, input_area);
        self.render_status_bar(frame, status_area);
    }
}
```

### Nested Layouts for Complex Areas

```rust
impl App {
    fn render_task_input(&mut self, frame: &mut Frame, area: Rect) {
        // Split input area: label + editor
        let input_layout = Layout::vertical([
            Constraint::Length(1),  // Label
            Constraint::Fill(1),    // Editor
        ]);

        let [label_area, editor_area] = input_layout.areas(area);

        // Label
        let label = Paragraph::new("Task Description:")
            .style(Style::default().fg(Color::Blue));
        frame.render_widget(label, label_area);

        // Editor with cursor
        let editor = Paragraph::new(self.task_input.as_str())
            .block(Block::bordered().title("Editor"))
            .wrap(Wrap::default());
        frame.render_widget(editor, editor_area);

        // Set cursor position
        frame.set_cursor_position(Position::new(
            editor_area.x + self.cursor_position as u16 + 1,
            editor_area.y + 2, // Account for border and label
        ));
    }
}
```

## 3. List Widgets for Selectors

### Project/Branch/Agent Selector Implementation

```rust
use ratatui::widgets::{List, ListItem, ListState};
use ratatui::style::{Color, Style, Modifier};

struct Selector {
    items: Vec<String>,
    state: ListState,
    filter_text: String,
}

impl Selector {
    fn new(items: Vec<String>) -> Self {
        Self {
            items,
            state: ListState::default(),
            filter_text: String::new(),
        }
    }

    fn filtered_items(&self) -> Vec<&String> {
        if self.filter_text.is_empty() {
            self.items.iter().collect()
        } else {
            self.items.iter()
                .filter(|item| item.to_lowercase()
                    .contains(&self.filter_text.to_lowercase()))
                .collect()
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, title: &str) {
        let filtered = self.filtered_items();
        let items: Vec<ListItem> = filtered.iter()
            .enumerate()
            .map(|(i, item)| {
                let style = if Some(i) == self.state.selected() {
                    Style::default().bg(Color::Blue).fg(Color::White)
                } else {
                    Style::default()
                };
                ListItem::new(*item).style(style)
            })
            .collect();

        let list = List::new(items)
            .block(Block::bordered().title(title))
            .highlight_style(Style::default()
                .bg(Color::LightBlue)
                .add_modifier(Modifier::BOLD))
            .highlight_symbol("▶ ");

        frame.render_stateful_widget(list, area, &mut self.state);
    }

    fn next(&mut self) {
        let len = self.filtered_items().len();
        let i = match self.state.selected() {
            Some(i) if i >= len - 1 => 0,
            Some(i) => i + 1,
            None => 0,
        };
        self.state.select(Some(i));
    }

    fn previous(&mut self) {
        let len = self.filtered_items().len();
        let i = match self.state.selected() {
            Some(0) => len - 1,
            Some(i) => i - 1,
            None => 0,
        };
        self.state.select(Some(i));
    }
}
```

### Integration with App State

```rust
struct App {
    project_selector: Selector,
    branch_selector: Selector,
    agent_selector: Selector,
    active_selector: ActiveSelector,
}

enum ActiveSelector {
    Project,
    Branch,
    Agent,
    TaskInput,
}

impl App {
    fn render_selectors(&mut self, frame: &mut Frame, areas: [Rect; 3]) {
        self.project_selector.render(frame, areas[0], "Projects");
        self.branch_selector.render(frame, areas[1], "Branches");
        self.agent_selector.render(frame, areas[2], "Agents");
    }
}
```

## 4. Text Input and Cursor Management

### Multi-line Task Description Editor

```rust
use ratatui::layout::Position;

impl App {
    fn render_task_editor(&mut self, frame: &mut Frame, area: Rect) {
        let input = Paragraph::new(self.task_input.as_str())
            .block(Block::bordered().title("Task Description"))
            .style(match self.active_selector {
                ActiveSelector::TaskInput => Style::default().fg(Color::Yellow),
                _ => Style::default(),
            })
            .wrap(Wrap::default());

        frame.render_widget(input, area);

        // Set cursor position when active
        if matches!(self.active_selector, ActiveSelector::TaskInput) {
            let cursor_x = (self.cursor_position % area.width.saturating_sub(2)) as u16;
            let cursor_y = (self.cursor_position / area.width.saturating_sub(2)) as u16;
            frame.set_cursor_position(Position::new(
                area.x + cursor_x + 1, // +1 for border
                area.y + cursor_y + 1, // +1 for border and title
            ));
        }
    }

    fn insert_char(&mut self, c: char) {
        self.task_input.insert(self.cursor_position, c);
        self.cursor_position += 1;
    }

    fn delete_char(&mut self) {
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
            self.task_input.remove(self.cursor_position);
        }
    }

    fn move_cursor_left(&mut self) {
        self.cursor_position = self.cursor_position.saturating_sub(1);
    }

    fn move_cursor_right(&mut self) {
        if self.cursor_position < self.task_input.len() {
            self.cursor_position += 1;
        }
    }
}
```

## 5. Event Handling and Hotkeys

### Comprehensive Event System

```rust
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

impl App {
    fn handle_events(&mut self) -> Result<()> {
        if let Event::Key(key) = event::read()? {
            let shift_pressed = key.modifiers.contains(KeyModifiers::SHIFT);
            let ctrl_pressed = key.modifiers.contains(KeyModifiers::CONTROL);

            match key.code {
                // Global hotkeys
                KeyCode::Esc => self.should_exit = true,
                KeyCode::Tab => self.cycle_focus(),

                // Selector navigation
                KeyCode::Up | KeyCode::Char('k') => self.navigate_up(),
                KeyCode::Down | KeyCode::Char('j') => self.navigate_down(),

                // Selector-specific actions
                KeyCode::Enter if matches!(self.active_selector, ActiveSelector::Project | ActiveSelector::Branch | ActiveSelector::Agent) => {
                    self.select_current_item();
                }

                // Task input editing
                KeyCode::Char(c) if matches!(self.active_selector, ActiveSelector::TaskInput) => {
                    self.insert_char(c);
                }
                KeyCode::Backspace if matches!(self.active_selector, ActiveSelector::TaskInput) => {
                    self.delete_char();
                }
                KeyCode::Left if matches!(self.active_selector, ActiveSelector::TaskInput) => {
                    self.move_cursor_left();
                }
                KeyCode::Right if matches!(self.active_selector, ActiveSelector::TaskInput) => {
                    self.move_cursor_right();
                }

                // Task submission
                KeyCode::Char('s') if ctrl_pressed => {
                    self.submit_task();
                }

                // Filtering
                KeyCode::Char('/') => {
                    self.start_filtering();
                }

                _ => {}
            }
        }
        Ok(())
    }

    fn cycle_focus(&mut self) {
        self.active_selector = match self.active_selector {
            ActiveSelector::Project => ActiveSelector::Branch,
            ActiveSelector::Branch => ActiveSelector::Agent,
            ActiveSelector::Agent => ActiveSelector::TaskInput,
            ActiveSelector::TaskInput => ActiveSelector::Project,
        };
    }

    fn navigate_up(&mut self) {
        match self.active_selector {
            ActiveSelector::Project => self.project_selector.previous(),
            ActiveSelector::Branch => self.branch_selector.previous(),
            ActiveSelector::Agent => self.agent_selector.previous(),
            ActiveSelector::TaskInput => {} // Handle text navigation
        }
    }
}
```

## 6. Status Display and Error Handling

### Status Bar Implementation

```rust
use ratatui::text::{Line, Span};

impl App {
    fn render_status_bar(&self, frame: &mut Frame, area: Rect) {
        let status_parts = vec![
            Span::styled("AW TUI", Style::default().fg(Color::Cyan)),
            Span::raw(" | "),
            Span::styled(self.get_backend_status(), Style::default().fg(Color::Green)),
            Span::raw(" | "),
            Span::styled(self.get_multiplexer_status(), Style::default().fg(Color::Yellow)),
            Span::raw(" | "),
            Span::styled(self.get_help_text(), Style::default().fg(Color::Gray)),
        ];

        let status_line = Line::from(status_parts);
        let status = Paragraph::new(status_line)
            .style(Style::default().bg(Color::Black));

        frame.render_widget(status, area);
    }

    fn get_backend_status(&self) -> String {
        "Local Mode".to_string() // or "Remote: server-name"
    }

    fn get_multiplexer_status(&self) -> String {
        "tmux".to_string() // or "zellij", "screen"
    }

    fn get_help_text(&self) -> String {
        match self.active_selector {
            ActiveSelector::Project => "↑/↓/j/k: navigate | Enter: select | Tab: next",
            ActiveSelector::Branch => "↑/↓/j/k: navigate | /: filter | Tab: next",
            ActiveSelector::Agent => "↑/↓/j/k: navigate | Enter: select | Tab: next",
            ActiveSelector::TaskInput => "Type to edit | ←/→: move cursor | Ctrl+S: submit",
        }.to_string()
    }

    fn show_error(&mut self, message: &str) {
        self.status_message = Some(message.to_string());
        // Could also render an overlay or popup
    }
}
```

## 7. Advanced Features

### Popup Overlays for Confirmation

```rust
use ratatui::widgets::{Clear, BorderType};

impl App {
    fn render_popup(&self, frame: &mut Frame, message: &str) {
        let popup_area = centered_rect(60, 20, frame.area());

        // Clear the background
        frame.render_widget(Clear, popup_area);

        // Render the popup
        let popup = Paragraph::new(message)
            .block(Block::bordered()
                .title("Confirmation")
                .border_type(BorderType::Rounded))
            .style(Style::default().bg(Color::Black));

        frame.render_widget(popup, popup_area);
    }

    fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
        let popup_layout = Layout::vertical([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ]).split(r);

        Layout::horizontal([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ]).split(popup_layout[1])[1]
    }
}
```

### Scrollable Content Areas

```rust
use ratatui::widgets::Scrollbar;

impl App {
    fn render_scrollable_list(&mut self, frame: &mut Frame, area: Rect, items: &[String]) {
        let list_items: Vec<ListItem> = items.iter()
            .map(|item| ListItem::new(item.as_str()))
            .collect();

        let list = List::new(list_items)
            .block(Block::bordered().title("Scrollable List"))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

        // Render list
        frame.render_stateful_widget(list, area, &mut self.list_state);

        // Render scrollbar
        frame.render_stateful_widget(
            Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓")),
            area,
            &mut self.scroll_state,
        );
    }
}
```

## 8. State Persistence

### Configuration and State Management

```rust
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Serialize, Deserialize, Default)]
struct AppConfig {
    last_selected_project: Option<String>,
    last_selected_branch: Option<String>,
    last_selected_agent: Option<String>,
    theme: Theme,
    multiplexer: MultiplexerType,
}

#[derive(Serialize, Deserialize)]
enum Theme {
    Dark,
    Light,
}

#[derive(Serialize, Deserialize)]
enum MultiplexerType {
    Tmux,
    Zellij,
    Screen,
}

impl App {
    fn load_config() -> Result<AppConfig> {
        let config_path = dirs::config_dir()
            .ok_or_else(|| eyre::eyre!("Could not find config directory"))?
            .join("aw-tui")
            .join("config.toml");

        if config_path.exists() {
            let contents = fs::read_to_string(config_path)?;
            Ok(toml::from_str(&contents)?)
        } else {
            Ok(AppConfig::default())
        }
    }

    fn save_config(&self) -> Result<()> {
        let config = AppConfig {
            last_selected_project: self.get_selected_project(),
            last_selected_branch: self.get_selected_branch(),
            last_selected_agent: self.get_selected_agent(),
            theme: self.theme,
            multiplexer: self.multiplexer,
        };

        let config_path = dirs::config_dir()
            .ok_or_else(|| eyre::eyre!("Could not find config directory"))?
            .join("aw-tui");

        fs::create_dir_all(&config_path)?;
        let contents = toml::to_string(&config)?;
        fs::write(config_path.join("config.toml"), contents)?;

        Ok(())
    }
}
```

## 9. Performance Optimizations

### Efficient Rendering Patterns

```rust
impl App {
    fn should_render(&self, last_state: &App) -> bool {
        // Only re-render if state actually changed
        self.project_selector.state.selected() != last_state.project_selector.state.selected()
        || self.task_input != last_state.task_input
        || self.status_message != last_state.status_message
    }

    fn render_optimized(&mut self, frame: &mut Frame, last_state: &App) {
        if !self.should_render(last_state) {
            return;
        }

        // Only render changed areas
        if self.project_selector.state.selected() != last_state.project_selector.state.selected() {
            self.render_project_selector(frame, self.project_area);
        }

        if self.task_input != last_state.task_input {
            self.render_task_input(frame, self.input_area);
        }

        // Always render status as it might have changed
        self.render_status_bar(frame, self.status_area);
    }
}
```

### Memory Management for Large Lists

```rust
use std::collections::VecDeque;

struct VirtualList<T> {
    items: Vec<T>,
    visible_range: (usize, usize),
    cache: VecDeque<ListItem<'static>>,
    cache_size: usize,
}

impl<T> VirtualList<T> {
    fn new(items: Vec<T>, cache_size: usize) -> Self {
        Self {
            items,
            visible_range: (0, 0),
            cache: VecDeque::with_capacity(cache_size),
            cache_size,
        }
    }

    fn update_visible_range(&mut self, start: usize, end: usize) {
        self.visible_range = (start, end);
        self.update_cache();
    }

    fn update_cache(&mut self) {
        self.cache.clear();
        for i in self.visible_range.0..self.visible_range.1.min(self.items.len()) {
            // Create ListItem for visible items only
            self.cache.push_back(self.create_list_item(i));
        }
    }

    fn get_visible_items(&self) -> &VecDeque<ListItem> {
        &self.cache
    }
}
```

## 10. Integration with External Systems

### Multiplexer Session Detection

```rust
use std::env;

impl App {
    fn detect_multiplexer(&self) -> MultiplexerType {
        // Check environment variables set by multiplexers
        if env::var("TMUX").is_ok() {
            MultiplexerType::Tmux
        } else if env::var("ZELLIJ").is_ok() {
            MultiplexerType::Zellij
        } else if env::var("STY").is_ok() {
            MultiplexerType::Screen
        } else {
            MultiplexerType::Tmux // default
        }
    }

    fn create_task_window(&self) -> Result<()> {
        match self.multiplexer {
            MultiplexerType::Tmux => {
                // tmux new-window -n "aw-task-123" "aw agent record --task-id 123"
                std::process::Command::new("tmux")
                    .args(&["new-window", "-n", &format!("aw-task-{}", self.task_id),
                           &format!("aw agent record --task-id {}", self.task_id)])
                    .spawn()?;
            }
            MultiplexerType::Zellij => {
                // zellij run --name "aw-task-123" "aw agent record --task-id 123"
                std::process::Command::new("zellij")
                    .args(&["run", "--name", &format!("aw-task-{}", self.task_id),
                           &format!("aw agent record --task-id {}", self.task_id)])
                    .spawn()?;
            }
            _ => {}
        }
        Ok(())
    }
}
```

### REST API Integration

```rust
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct TaskRequest {
    project: String,
    branch: String,
    agent: String,
    prompt: String,
}

#[derive(Deserialize)]
struct TaskResponse {
    task_id: String,
    status: String,
}

impl App {
    async fn submit_task_remote(&self, server_url: &str) -> Result<String> {
        let client = Client::new();
        let request = TaskRequest {
            project: self.get_selected_project().unwrap_or_default(),
            branch: self.get_selected_branch().unwrap_or_default(),
            agent: self.get_selected_agent().unwrap_or_default(),
            prompt: self.task_input.clone(),
        };

        let response = client
            .post(format!("{}/api/v1/tasks", server_url))
            .json(&request)
            .send()
            .await?;

        let task_response: TaskResponse = response.json().await?;
        Ok(task_response.task_id)
    }

    async fn fetch_projects_remote(&self, server_url: &str) -> Result<Vec<String>> {
        let client = Client::new();
        let response = client
            .get(format!("{}/api/v1/projects", server_url))
            .send()
            .await?;

        let projects: Vec<String> = response.json().await?;
        Ok(projects)
    }
}
```

## 11. Testing and Debugging

### Unit Testing Components

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn test_selector_navigation() {
        let mut selector = Selector::new(vec![
            "item1".to_string(),
            "item2".to_string(),
            "item3".to_string(),
        ]);

        selector.next();
        assert_eq!(selector.state.selected(), Some(1));

        selector.previous();
        assert_eq!(selector.state.selected(), Some(0));
    }

    #[test]
    fn test_task_input_editing() {
        let mut app = App::new();

        app.insert_char('H');
        app.insert_char('i');
        assert_eq!(app.task_input, "Hi");
        assert_eq!(app.cursor_position, 2);

        app.move_cursor_left();
        app.delete_char();
        assert_eq!(app.task_input, "H");
        assert_eq!(app.cursor_position, 1);
    }

    #[test]
    fn test_layout_rendering() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut app = App::new();

        terminal.draw(|frame| app.render(frame)).unwrap();

        // Verify that the layout is correct
        let buffer = terminal.backend().buffer();
        // Add assertions about the rendered content
    }
}
```

### Debug Rendering

```rust
impl App {
    fn render_debug_info(&self, frame: &mut Frame, area: Rect) {
        let debug_info = format!(
            "Active: {:?} | Projects: {} | Branches: {} | Agents: {} | Input len: {}",
            self.active_selector,
            self.project_selector.items.len(),
            self.branch_selector.items.len(),
            self.agent_selector.items.len(),
            self.task_input.len()
        );

        let debug = Paragraph::new(debug_info)
            .block(Block::bordered().title("Debug"))
            .style(Style::default().fg(Color::Red));

        frame.render_widget(debug, area);
    }
}
```

## 12. Best Practices and Patterns

### Error Handling Patterns

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Terminal error: {0}")]
    Terminal(#[from] std::io::Error),

    #[error("Configuration error: {0}")]
    Config(#[from] std::fmt::Error),

    #[error("API error: {0}")]
    Api(#[from] reqwest::Error),

    #[error("Invalid selection")]
    InvalidSelection,
}

impl App {
    fn handle_error(&mut self, error: AppError) {
        match error {
            AppError::Terminal(_) => {
                self.show_error("Terminal communication failed");
            }
            AppError::Config(_) => {
                self.show_error("Configuration error");
            }
            AppError::Api(_) => {
                self.show_error("Server communication failed");
            }
            AppError::InvalidSelection => {
                self.show_error("Invalid selection made");
            }
        }
    }
}
```

### Accessibility Considerations

```rust
impl App {
    fn render_accessible(&mut self, frame: &mut Frame) {
        // Ensure sufficient color contrast
        let theme = if self.high_contrast_mode {
            Theme::high_contrast()
        } else {
            self.theme
        };

        // Add screen reader hints (when supported)
        // Ensure keyboard-only navigation works
        // Use semantic colors for status indication
    }

    fn announce_change(&self, message: &str) {
        // Could integrate with screen readers
        // For now, just log to terminal
        eprintln!("Accessibility: {}", message);
    }
}
```

## 13. Event loop, resizing, and redraw

Use a tick + input + resize loop for responsive UIs (status updates, SSE, etc.).

```rust
use std::time::{Duration, Instant};
use crossterm::event::{self, Event};

impl App {
    fn run(mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        let tick_rate = Duration::from_millis(100);
        let mut last_tick = Instant::now();
        while !self.should_exit {
            terminal.draw(|frame| self.render(frame))?;

            let timeout = tick_rate.saturating_sub(last_tick.elapsed());
            if event::poll(timeout)? {
                match event::read()? {
                    Event::Key(key) => self.handle_key(key)?,
                    Event::Mouse(m) => self.handle_mouse(m)?,
                    Event::Resize(_, _) => self.handle_resize()?,
                    _ => {}
                }
            }
            if last_tick.elapsed() >= tick_rate {
                self.on_tick();
                last_tick = Instant::now();
            }
        }
        Ok(())
    }
}
```

## 14. Scrolling and scrollbars for selectors

```rust
use ratatui::widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState};

struct Selector {
    items: Vec<String>,
    state: ListState,
    filter_text: String,
    scroll: ScrollbarState,
}

impl Selector {
    fn new(items: Vec<String>) -> Self {
        let len = items.len();
        Self {
            items,
            state: ListState::default(),
            filter_text: String::new(),
            scroll: ScrollbarState::new(len.saturating_sub(1)),
        }
    }

    fn update_scroll(&mut self) {
        if let Some(i) = self.state.selected() { self.scroll = self.scroll.position(i); }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, title: &str) {
        // build list ...
        frame.render_stateful_widget(list, area, &mut self.state);
        self.update_scroll();
        frame.render_stateful_widget(
            Scrollbar::default().orientation(ScrollbarOrientation::VerticalRight),
            area,
            &mut self.scroll,
        );
    }
}
```

## 15. Filter input mode (Ctrl+F)

```rust
enum UiMode { Normal, FilterProject, FilterBranch, FilterAgent, Editing }

impl App {
    fn start_filtering(&mut self) {
        self.ui_mode = match self.active_selector {
            ActiveSelector::Project => UiMode::FilterProject,
            ActiveSelector::Branch => UiMode::FilterBranch,
            ActiveSelector::Agent => UiMode::FilterAgent,
            ActiveSelector::TaskInput => UiMode::Editing,
        };
    }

    fn handle_filter_key(&mut self, key: KeyEvent) {
        use crossterm::event::KeyCode::*;
        match self.ui_mode {
            UiMode::FilterProject => match key.code {
                Esc => self.ui_mode = UiMode::Normal,
                Backspace => { self.project_selector.filter_text.pop(); }
                Char(c) => self.project_selector.filter_text.push(c),
                Enter => self.ui_mode = UiMode::Normal,
                _ => {}
            }
            _ => {}
        }
    }

    fn render_filter_bar(&self, frame: &mut Frame, area: Rect) {
        let text = match self.ui_mode {
            UiMode::FilterProject => format!("Project filter: {}", self.project_selector.filter_text),
            UiMode::FilterBranch => format!("Branch filter: {}", self.branch_selector.filter_text),
            UiMode::FilterAgent => format!("Agent filter: {}", self.agent_selector.filter_text),
            _ => String::new(),
        };
        if !text.is_empty() {
            frame.render_widget(Paragraph::new(text).block(Block::bordered().title("Filter")), area);
        }
    }
}
```

## 16. Resizable editor height (Ctrl+Up/Down)

```rust
impl App {
    fn render(&mut self, frame: &mut Frame) {
        let layout = Layout::vertical([
            Constraint::Length(3), // project
            Constraint::Length(3), // branch
            Constraint::Length(3), // agent
            Constraint::Length(self.editor_height), // editor
            Constraint::Fill(1),  // status/logs
        ]);
        let [p,b,a,editor,status] = layout.areas(frame.area());
        // ...
    }

    fn handle_key_resize(&mut self, key: KeyEvent) {
        use crossterm::event::{KeyCode, KeyModifiers};
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        match key.code {
            KeyCode::Up if shift => self.editor_height = self.editor_height.saturating_add(1),
            KeyCode::Down if shift => self.editor_height = self.editor_height.saturating_sub(1).max(3),
            _ => {}
        }
    }
}
```

## 17. Help overlay (F1)

```rust
impl App {
    fn render_help(&self, frame: &mut Frame) {
        if !self.show_help { return; }
        let area = Self::centered_rect(70, 60, frame.area());
        frame.render_widget(Clear, area);
        let help = Paragraph::new(
            "Tab: cycle | Ctrl+F: filter | Ctrl+Up/Down: resize editor | Ctrl+Enter: Start"
        ).block(Block::bordered().title("Help (F1)"));
        frame.render_widget(help, area);
    }
}
```

## 18. Async and SSE integration (remote monitoring)

Bridge async server events into the TUI loop using channels.

```rust
use tokio::sync::mpsc;

struct Channels { sse_rx: mpsc::UnboundedReceiver<ServerEvent> }

impl App {
    fn run(mut self, terminal: &mut DefaultTerminal, mut chans: Channels) -> Result<()> {
        let tick = Duration::from_millis(100);
        let mut last = Instant::now();
        while !self.should_exit {
            terminal.draw(|f| self.render(f))?;
            if event::poll(tick.saturating_sub(last.elapsed()))? {
                if let Event::Key(k) = event::read()? { self.handle_key(k)?; }
            }
            while let Ok(ev) = chans.sse_rx.try_recv() { self.apply_server_event(ev); }
            if last.elapsed() >= tick { self.on_tick(); last = Instant::now(); }
        }
        Ok(())
    }
}
```

## 19. Theming and high-contrast

```rust
use ratatui::style::{Color, Style};
use ratatui::style::palette::tailwind;

struct ThemeColors { bg: Color, fg: Color, accent: Color }

impl ThemeColors {
    fn high_contrast() -> Self {
        Self { bg: tailwind::SLATE.c950, fg: tailwind::SLATE.c100, accent: tailwind::BLUE.c400 }
    }
}

impl App {
    fn style_for_selected(&self) -> Style { Style::default().bg(self.theme.accent).fg(self.theme.bg) }
}
```

## 20. Implementation checklist vs PRD

- Auto-attach to multiplexer; create panes on new task (right: logs, left: workspace/editor).
- Dashboard layout: fixed-height selectors, resizable editor, status bar.
- Filtering: Ctrl+F opens filter bar; live filtering for project/branch/agent.
- Navigation: Arrow/PageUp/PageDown/Home/End; Enter selects; Tab/Shift+Tab cycles focus.
- Start action: Ctrl+Enter submits task and creates multiplexer window.
- Remote mode: async SSE listener updates task/session views.
- Accessibility: high-contrast theme; keyboard-only operation; predictable focus.
- Performance: tick/poll loop, avoid unnecessary work, show scrollbars.

This expanded guide now covers event loop patterns, resize handling, scrollbars, filtering, help overlays, async/SSE integration, theming, testing, and performance—answering the practical questions needed to implement the AW TUI per the TUI PRD and [CLI.md](../Public/CLI.md).
