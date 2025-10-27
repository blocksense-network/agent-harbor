// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

// Live Ratatui viewer for terminal recordings
//
// This module implements the live TUI viewer that renders directly from a vt100::Parser,
// providing real-time display of terminal sessions with scroll, navigation, and instruction
// overlay capabilities.
//
// See: specs/Public/ah-agent-record.md section 6 for complete specification

use crate::snapshots::Snapshot;
use ah_core::TaskManager;
use ah_tui::Theme;
use ah_tui::view::draft_card::render_draft_card;
use ah_tui::view_model::input::key_event_to_operation;
use ah_tui::view_model::{FocusElement, TaskEntryViewModel};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, MouseButton, MouseEvent};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use std::collections::HashMap;
use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use vt100::Parser;

/// Row metadata tracking last write byte offset for each terminal row
#[derive(Debug, Clone)]
pub struct RowMetadata {
    /// The largest byte_off that wrote to any cell in this row
    pub last_write_byte: u64,
    /// Row content hash for change detection
    pub content_hash: u64,
}

/// Position of the snapshot indicator gutter
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GutterPosition {
    Left,
    Right,
    None,
}

/// Viewer configuration
#[derive(Debug, Clone)]
pub struct ViewerConfig {
    /// Initial terminal size
    pub cols: u16,
    pub rows: u16,
    /// Scrollback buffer size (lines)
    pub scrollback: usize,
    /// Position of the snapshot indicator gutter
    pub gutter: GutterPosition,
}

/// Viewer state for the terminal display
pub struct TerminalViewer {
    /// Terminal state containing vt100 parser and metadata
    terminal_state: Arc<Mutex<crate::pty::TerminalState>>,
    /// Metadata for each row (last write byte, content hash)
    row_metadata: Arc<Mutex<HashMap<usize, RowMetadata>>>,
    /// Current scroll position (0 = bottom of screen)
    scroll_offset: usize,
    /// Total number of rows in scrollback
    total_rows: usize,
    /// Configuration
    config: ViewerConfig,
    /// Current task entry for instruction input (if any)
    instruction_entry: Option<TaskEntryViewModel>,
    /// Search mode state
    search_mode: Option<SearchState>,
}

/// Search mode state for incremental search
#[derive(Debug, Clone)]
pub struct SearchState {
    /// Current search query
    pub query: String,
    /// Current cursor position in query
    pub cursor_pos: usize,
    /// Search results (row indices)
    pub results: Vec<usize>,
    /// Current result index
    pub current_result: usize,
}

impl TerminalViewer {
    /// Create a new viewer with the given terminal state and configuration
    pub fn new(
        terminal_state: Arc<Mutex<crate::pty::TerminalState>>,
        config: ViewerConfig,
    ) -> Self {
        let mut viewer = Self {
            terminal_state,
            row_metadata: Arc::new(Mutex::new(HashMap::new())),
            scroll_offset: 0,
            total_rows: 0,
            config,
            instruction_entry: None,
            search_mode: None,
        };

        // Initialize with current parser state
        viewer.update_row_metadata();

        viewer
    }

    /// Update row metadata after terminal state changes
    ///
    /// This computes damage bands and updates last_write_byte for changed rows.
    pub fn update_row_metadata(&mut self) {
        let terminal_state = self.terminal_state.lock().unwrap();
        let screen = terminal_state.parser().screen();

        // Get screen contents and split into lines
        let contents = screen.contents();
        let lines: Vec<&str> = contents.lines().collect();

        let mut metadata = self.row_metadata.lock().unwrap();

        // Update total rows
        self.total_rows = lines.len();

        // Compute new hashes and update metadata for each line
        for (row_idx, line) in lines.iter().enumerate() {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            std::hash::Hash::hash(line, &mut hasher);
            let content_hash = std::hash::Hasher::finish(&hasher);

            // If hash changed, this row was recently written to
            let last_write_byte = if let Some(existing) = metadata.get(&row_idx) {
                if existing.content_hash != content_hash {
                    // Row changed - this would be set by the recorder with current byte offset
                    // For now, use a placeholder (will be updated when integrated with PTY reader)
                    existing.last_write_byte
                } else {
                    existing.last_write_byte
                }
            } else {
                // New row
                0 // Placeholder
            };

            metadata.insert(
                row_idx,
                RowMetadata {
                    last_write_byte,
                    content_hash,
                },
            );
        }
    }

    /// Find the nearest snapshot to a given row
    pub fn find_nearest_snapshot<'a>(
        &self,
        row_index: usize,
        snapshots: &'a [Snapshot],
    ) -> Option<&'a Snapshot> {
        let metadata = self.row_metadata.lock().unwrap();
        let row_last_write = metadata.get(&row_index)?.last_write_byte;

        snapshots
            .iter()
            .min_by_key(move |s| (s.anchor_byte as i64 - row_last_write as i64).abs())
    }

    /// Start instruction entry overlay at the given row
    pub fn start_instruction_overlay(
        &mut self,
        row_index: usize,
        existing_instruction: Option<String>,
    ) {
        use ah_tui::view_model::DraftSaveState;
        use tui_textarea::TextArea;

        // Create a new task entry for instruction input
        let mut textarea = TextArea::new(vec![existing_instruction.unwrap_or_default()]);
        textarea.move_cursor(tui_textarea::CursorMove::End);

        let mut task_entry = TaskEntryViewModel {
            id: format!("instruction-{}", row_index),
            repository: String::new(),
            branch: String::new(),
            models: vec![],
            created_at: chrono::Utc::now().to_rfc3339(),
            height: 5, // Fixed height for instruction entry
            controls: ah_tui::view_model::TaskEntryControlsViewModel {
                repository_button: ah_tui::view_model::ButtonViewModel {
                    text: "Repository".to_string(),
                    is_focused: false,
                    style: ah_tui::view_model::ButtonStyle::Normal,
                },
                branch_button: ah_tui::view_model::ButtonViewModel {
                    text: "Branch".to_string(),
                    is_focused: false,
                    style: ah_tui::view_model::ButtonStyle::Normal,
                },
                model_button: ah_tui::view_model::ButtonViewModel {
                    text: "Model".to_string(),
                    is_focused: false,
                    style: ah_tui::view_model::ButtonStyle::Normal,
                },
                go_button: ah_tui::view_model::ButtonViewModel {
                    text: "Go".to_string(),
                    is_focused: false,
                    style: ah_tui::view_model::ButtonStyle::Normal,
                },
            },
            save_state: DraftSaveState::Unsaved,
            description: textarea,
            focus_element: ah_tui::view_model::task_entry::CardFocusElement::TaskDescription,
            auto_save_timer: None,
            repositories_enumerator: None,
            branches_enumerator: None,
        };

        self.instruction_entry = Some(task_entry);
    }

    /// Cancel the current instruction entry
    pub fn cancel_instruction_overlay(&mut self) {
        self.instruction_entry = None;
    }

    /// Submit the current instruction entry
    pub fn submit_instruction_overlay(&mut self) -> Option<String> {
        if let Some(task_entry) = self.instruction_entry.take() {
            // Get the instruction text from the description field
            let instruction = task_entry.description.lines().join("\n").trim().to_string();
            if instruction.is_empty() {
                None
            } else {
                Some(instruction)
            }
        } else {
            None
        }
    }

    /// Handle keyboard input for the instruction entry
    pub fn handle_instruction_key(&mut self, key: KeyEvent) -> bool {
        if let Some(ref mut task_entry) = self.instruction_entry {
            // Try to map to keyboard operation
            let settings = ah_tui::Settings::default();
            if let Some(operation) = key_event_to_operation(&key, &settings) {
                // Create a dummy autocomplete manager for recorder
                struct RecorderAutocompleteManager;
                impl ah_tui::view_model::task_entry::AutocompleteManager for RecorderAutocompleteManager {
                    fn show(&mut self, _prefix: &str) {}
                    fn hide(&mut self) {}
                    fn after_textarea_change(&mut self, _textarea: &tui_textarea::TextArea) {
                        // Recorder doesn't need to handle redraw
                    }
                    fn set_needs_redraw(&mut self) {
                        // Recorder doesn't need redraw
                    }
                }

                let mut needs_redraw = false;
                let mut manager = RecorderAutocompleteManager;

                return matches!(
                    task_entry.handle_keyboard_operation(operation, &key, &mut manager,),
                    ah_tui::view_model::task_entry::KeyboardOperationResult::Handled
                        | ah_tui::view_model::task_entry::KeyboardOperationResult::TaskLaunched { .. }
                );
            }

            // Handle character input directly
            if let ratatui::crossterm::event::KeyCode::Char(_) = key.code {
                // Regular character input - use textarea's input method directly
                task_entry.description.input(key);
                return true;
            }
        }
        false
    }

    /// Start incremental search
    pub fn start_search(&mut self) {
        self.search_mode = Some(SearchState {
            query: String::new(),
            cursor_pos: 0,
            results: Vec::new(),
            current_result: 0,
        });
    }

    /// Update search query and find results
    pub fn update_search(&mut self, query: String) {
        if let Some(ref mut search) = self.search_mode {
            let query_len = query.len();
            search.query = query;
            search.cursor_pos = query_len;
            // TODO: Implement actual search through terminal content
            search.results = Vec::new(); // Placeholder
        }
    }

    /// Navigate to next search result
    pub fn next_search_result(&mut self) {
        if let Some(ref mut search) = self.search_mode {
            if !search.results.is_empty() {
                search.current_result = (search.current_result + 1) % search.results.len();
                // TODO: Scroll to result
            }
        }
    }

    /// Navigate to previous search result
    pub fn prev_search_result(&mut self) {
        if let Some(ref mut search) = self.search_mode {
            if !search.results.is_empty() {
                search.current_result = if search.current_result == 0 {
                    search.results.len() - 1
                } else {
                    search.current_result - 1
                };
                // TODO: Scroll to result
            }
        }
    }

    /// Exit search mode
    pub fn exit_search(&mut self) {
        self.search_mode = None;
    }

    /// Scroll up by the given number of lines
    pub fn scroll_up(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_add(lines);
        let max_scroll = self.total_rows.saturating_sub(self.config.rows as usize);
        if self.scroll_offset > max_scroll {
            self.scroll_offset = max_scroll;
        }
    }

    /// Scroll down by the given number of lines
    pub fn scroll_down(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
    }

    /// Scroll to bottom
    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
    }

    /// Handle a mouse click at the given position
    pub fn handle_mouse_click(&mut self, col: u16, row: u16, snapshots: &[Snapshot]) {
        // Calculate viewport layout to determine if click is in gutter
        let gutter_width = match self.config.gutter {
            GutterPosition::None => 0,
            _ => 3,
        };

        let recorded_cols = self.config.cols as usize;
        let available_width: usize = 80; // Approximate terminal width, could be passed as parameter
        let viewport_cols =
            recorded_cols.min(available_width.saturating_sub(gutter_width).saturating_sub(2));

        // Calculate gutter position
        let total_width = viewport_cols as u16 + 2 + gutter_width as u16;
        let x_offset = (80u16.saturating_sub(total_width)) / 2; // Approximate centering

        let is_in_gutter = match self.config.gutter {
            GutterPosition::Left => col >= x_offset && col < x_offset + gutter_width as u16,
            GutterPosition::Right => {
                col >= x_offset + viewport_cols as u16 + 2
                    && col < x_offset + viewport_cols as u16 + 2 + gutter_width as u16
            }
            GutterPosition::None => false,
        };

        // Convert screen coordinates to row index
        let visible_start = self.scroll_offset;
        let clicked_row = visible_start + row as usize;

        if clicked_row < self.total_rows {
            if is_in_gutter {
                // Gutter click - find snapshot for this row and insert instruction UI
                if let Some(snapshot) = self.find_nearest_snapshot(clicked_row, snapshots) {
                    tracing::debug!("Clicked gutter snapshot marker: {}", snapshot.id);
                    // Insert instruction overlay at the snapshot location
                    self.start_instruction_overlay(clicked_row, None); // TODO: Pre-fill with existing instruction if available
                }
            } else {
                // Terminal content click - find nearest snapshot or start instruction overlay
                if let Some(snapshot) = self.find_nearest_snapshot(clicked_row, snapshots) {
                    // TODO: Show snapshot info overlay or navigate to it
                    tracing::debug!("Clicked near snapshot: {}", snapshot.id);
                } else {
                    // Start instruction overlay at this row
                    self.start_instruction_overlay(clicked_row, None);
                }
            }
        }
    }

    /// Render the viewer to the terminal frame
    pub fn render(&mut self, frame: &mut Frame, snapshots: &[Snapshot]) {
        let area = frame.area();

        // Split the area for status bar and main content
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),    // Terminal content
                Constraint::Length(1), // Status bar
            ])
            .split(area);

        let terminal_area = chunks[0];
        let status_area = chunks[1];

        // Render terminal content
        self.render_terminal_content(frame, terminal_area, snapshots);

        // Render instruction entry if active
        if let Some(ref task_entry) = self.instruction_entry {
            self.render_instruction_overlay(frame, task_entry);
        }

        // Render search overlay if active
        if let Some(ref search) = self.search_mode {
            self.render_search_overlay(frame, search.clone());
        }

        // Render status bar
        self.render_status_bar(frame, status_area, snapshots);
    }

    /// Render the main terminal content
    fn render_terminal_content(&self, frame: &mut Frame, area: Rect, snapshots: &[Snapshot]) {
        let terminal_state = self.terminal_state.lock().unwrap();
        let screen = terminal_state.parser().screen();

        // Get screen contents and split into lines
        let contents = screen.contents();
        let all_lines: Vec<&str> = contents.lines().collect();

        // Use the original recorded terminal dimensions for the viewport
        let recorded_cols = self.config.cols as usize;
        let recorded_rows = self.config.rows as usize;

        // Calculate gutter width if enabled
        let gutter_width = match self.config.gutter {
            GutterPosition::None => 0,
            _ => 3, // 1 for marker + 2 for padding/borders
        };

        // Calculate the viewport size, constrained by available area
        let available_width = area.width.saturating_sub(gutter_width);
        let viewport_cols = recorded_cols.min(available_width.saturating_sub(2) as usize); // -2 for borders
        let viewport_rows = recorded_rows.min(area.height.saturating_sub(1) as usize); // -1 for status bar

        // Center the viewport in the available area
        let total_width = viewport_cols as u16 + 2 + gutter_width as u16; // terminal + borders + gutter
        let viewport_height = viewport_rows as u16;
        let x_offset = (area.width.saturating_sub(total_width)) / 2;
        let y_offset = (area.height.saturating_sub(viewport_height + 1)) / 2; // +1 for status bar

        // Split area based on gutter position
        let (gutter_area, terminal_area) = match self.config.gutter {
            GutterPosition::Left => {
                let gutter_rect = Rect {
                    x: area.x + x_offset,
                    y: area.y + y_offset,
                    width: gutter_width as u16,
                    height: viewport_height,
                };
                let terminal_rect = Rect {
                    x: area.x + x_offset + gutter_width as u16,
                    y: area.y + y_offset,
                    width: viewport_cols as u16 + 2, // +2 for borders
                    height: viewport_height,
                };
                (Some(gutter_rect), terminal_rect)
            }
            GutterPosition::Right => {
                let terminal_rect = Rect {
                    x: area.x + x_offset,
                    y: area.y + y_offset,
                    width: viewport_cols as u16 + 2, // +2 for borders
                    height: viewport_height,
                };
                let gutter_rect = Rect {
                    x: area.x + x_offset + viewport_cols as u16 + 2,
                    y: area.y + y_offset,
                    width: gutter_width as u16,
                    height: viewport_height,
                };
                (Some(gutter_rect), terminal_rect)
            }
            GutterPosition::None => {
                let terminal_rect = Rect {
                    x: area.x + x_offset,
                    y: area.y + y_offset,
                    width: viewport_cols as u16 + 2, // +2 for borders
                    height: viewport_height,
                };
                (None, terminal_rect)
            }
        };

        // Render gutter if enabled
        if let Some(gutter_rect) = gutter_area {
            self.render_gutter(frame, gutter_rect, snapshots, viewport_rows);
        }

        // Collect visible rows (accounting for scroll)
        let mut lines = Vec::new();
        let start_row = self.scroll_offset;

        for i in 0..viewport_rows {
            let row_idx = start_row + i;
            if let Some(line_text) = all_lines.get(row_idx) {
                let line = Line::from(*line_text);
                lines.push(line);
            } else {
                // Empty row
                lines.push(Line::from(""));
            }
        }

        let terminal_widget = Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!("Terminal ({}x{})", recorded_cols, recorded_rows)),
            )
            .wrap(Wrap { trim: false });

        frame.render_widget(terminal_widget, terminal_area);
    }

    /// Render the gutter with snapshot markers
    fn render_gutter(
        &self,
        frame: &mut Frame,
        area: Rect,
        snapshots: &[Snapshot],
        visible_rows: usize,
    ) {
        let mut lines = Vec::new();
        let start_row = self.scroll_offset;

        for i in 0..visible_rows {
            let row_idx = start_row + i;

            // Check if this row has any snapshots
            let has_snapshot = snapshots.iter().any(|snapshot| {
                // Find the nearest snapshot to this row based on anchor_byte proximity to row's last_write_byte
                if let Some(row_last_write) = self
                    .row_metadata
                    .lock()
                    .unwrap()
                    .get(&(row_idx as usize))
                    .map(|meta| meta.last_write_byte)
                {
                    // Consider it a match if the snapshot is within a reasonable range of this row
                    (snapshot.anchor_byte as i64 - row_last_write as i64).abs() < 1000
                // Within 1000 bytes
                } else {
                    false
                }
            });

            if has_snapshot {
                lines.push(Line::from(" • ").style(Style::default().fg(Color::Yellow)));
            } else {
                lines.push(Line::from("   "));
            }
        }

        let gutter_widget = Paragraph::new(lines)
            .block(Block::default().borders(Borders::LEFT).title("Snapshots"))
            .wrap(Wrap { trim: false });

        frame.render_widget(gutter_widget, area);
    }

    /// Render instruction overlay
    fn render_instruction_overlay(&self, frame: &mut Frame, task_entry: &TaskEntryViewModel) {
        let area = frame.area();
        let overlay_height = 8; // Height for the task entry
        let overlay_width = 80; // Width for the task entry
        let overlay_area = Rect {
            x: (area.width - overlay_width) / 2,
            y: (area.height - overlay_height) / 2,
            width: overlay_width,
            height: overlay_height,
        };

        // Create a simple theme for the instruction entry
        let theme = Theme {
            primary: Color::Cyan,
            bg: Color::Black,
            text: Color::White,
            border: Color::Gray,
            error: Color::Red,
            success: Color::Green,
            warning: Color::Yellow,
            ..Default::default()
        };

        // Render the task entry using the draft card renderer
        let _ = render_draft_card(frame, overlay_area, task_entry, &theme, true);
    }

    /// Render search overlay
    fn render_search_overlay(&self, frame: &mut Frame, search: SearchState) {
        let area = frame.area();
        let overlay_area = Rect {
            x: 0,
            y: area.height - 1,
            width: area.width,
            height: 1,
        };

        let search_text = format!("/{}", search.query);
        let search_widget =
            Paragraph::new(search_text).style(Style::default().bg(Color::Blue).fg(Color::White));

        frame.render_widget(search_widget, overlay_area);
    }

    /// Render status bar
    fn render_status_bar(&self, frame: &mut Frame, area: Rect, snapshots: &[Snapshot]) {
        let status_text = format!(
            "Scroll: {}/{} | Snapshots: {} | {}",
            self.scroll_offset,
            self.total_rows,
            snapshots.len(),
            if self.instruction_entry.is_active() {
                "EDITING"
            } else if self.search_mode.is_some() {
                "SEARCH"
            } else {
                "NORMAL"
            }
        );

        let status = Paragraph::new(status_text)
            .style(Style::default().bg(Color::DarkGray).fg(Color::White))
            .alignment(Alignment::Center);

        frame.render_widget(status, area);
    }
}

/// Extension trait for Option<TaskEntryViewModel>
trait InstructionEntryExt {
    fn is_active(&self) -> bool;
}

impl InstructionEntryExt for Option<TaskEntryViewModel> {
    fn is_active(&self) -> bool {
        self.is_some()
    }
}

/// Event loop for the terminal viewer
pub struct ViewerEventLoop {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
    viewer: TerminalViewer,
    snapshots: Vec<Snapshot>,
    task_manager: std::sync::Arc<dyn TaskManager>,
}

impl ViewerEventLoop {
    /// Create a new event loop
    pub fn new(
        viewer: TerminalViewer,
        snapshots: Vec<Snapshot>,
        task_manager: std::sync::Arc<dyn TaskManager>,
    ) -> io::Result<Self> {
        let backend = CrosstermBackend::new(io::stdout());
        let terminal = Terminal::new(backend)?;

        Ok(Self {
            terminal,
            viewer,
            snapshots,
            task_manager,
        })
    }

    /// Create a new task from an instruction submitted at the current viewer position
    async fn create_task_from_instruction(&self, instruction: String) {
        // Find the snapshot that corresponds to the current viewer position
        // For now, use the most recent snapshot as a placeholder
        let current_snapshot = self.snapshots.last();

        if let Some(snapshot) = current_snapshot {
            tracing::info!(
                "Creating task from instruction at snapshot {} (byte offset: {}): {}",
                snapshot.id,
                snapshot.anchor_byte,
                instruction
            );

            // TODO: TaskLaunchParams needs to be extended to support starting from a snapshot
            // For now, create a basic task with default parameters
            // In the future, this should include snapshot information so the agent
            // can resume from that point in the recorded session
            let params = match ah_core::TaskLaunchParams::new(
                ".".to_string(),    // Default repository (current directory)
                "main".to_string(), // Default branch
                instruction,
                vec![ah_domain_types::SelectedModel {
                    name: "Claude 3.5 Sonnet".to_string(),
                    count: 1,
                }],
            ) {
                Ok(params) => params,
                Err(e) => {
                    tracing::error!("Failed to create task launch params: {}", e);
                    return;
                }
            };

            match self.task_manager.launch_task(params).await {
                ah_core::TaskLaunchResult::Success { task_id } => {
                    tracing::info!("Successfully launched task {} from instruction", task_id);
                }
                ah_core::TaskLaunchResult::Failure { error } => {
                    tracing::error!("Failed to launch task from instruction: {}", error);
                }
            }
        } else {
            tracing::warn!("No snapshots available, cannot create task from instruction");
        }
    }

    /// Run the event loop until quit
    pub async fn run(&mut self) -> io::Result<()> {
        loop {
            // Update viewer state
            self.viewer.update_row_metadata();

            // Draw the UI
            self.terminal.draw(|f| {
                self.viewer.render(f, &self.snapshots);
            })?;

            // Handle input with timeout
            if event::poll(Duration::from_millis(100))? {
                match event::read()? {
                    Event::Key(key) => {
                        if self.handle_key(key).await? {
                            break; // Quit
                        }
                    }
                    Event::Mouse(mouse) => {
                        self.handle_mouse(mouse);
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }

    /// Handle keyboard input, returns true if should quit
    async fn handle_key(&mut self, key: KeyEvent) -> io::Result<bool> {
        // If instruction entry is active, delegate all input to it first
        if self.viewer.instruction_entry.is_active() {
            if self.viewer.handle_instruction_key(key) {
                // Key was handled by instruction entry
                return Ok(false);
            }
            // Special handling for Enter to submit
            if key.code == KeyCode::Enter {
                if let Some(instruction) = self.viewer.submit_instruction_overlay() {
                    // Create new task through TaskManager starting from current snapshot
                    // This will spawn a new agent session from the snapshot at the current cursor position
                    self.create_task_from_instruction(instruction).await;
                }
            }
            return Ok(false);
        }

        match key.code {
            KeyCode::Char('q') => {
                return Ok(true); // Quit
            }
            KeyCode::Char('i') if key.modifiers.is_empty() => {
                // Start instruction entry (will be at current cursor position)
                // For now, start at row 0
                self.viewer.start_instruction_overlay(0, None);
            }
            KeyCode::Char('/') => {
                self.viewer.start_search();
            }
            KeyCode::Esc => {
                if self.viewer.search_mode.is_some() {
                    self.viewer.exit_search();
                }
            }
            KeyCode::PageUp => {
                self.viewer.scroll_up(self.viewer.config.rows as usize);
            }
            KeyCode::PageDown => {
                self.viewer.scroll_down(self.viewer.config.rows as usize);
            }
            KeyCode::Char('[') => {
                self.viewer.prev_search_result();
            }
            KeyCode::Char(']') => {
                self.viewer.next_search_result();
            }
            _ => {}
        }

        Ok(false)
    }

    /// Handle mouse input
    fn handle_mouse(&mut self, mouse: MouseEvent) {
        match mouse.kind {
            event::MouseEventKind::Down(MouseButton::Left) => {
                self.viewer.handle_mouse_click(mouse.column, mouse.row, &self.snapshots);
            }
            _ => {}
        }
    }
}
