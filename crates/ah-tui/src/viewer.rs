// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

// Live Ratatui viewer for terminal recordings
//
// This module implements the live TUI viewer that renders directly from a vt100::Parser,
// providing real-time display of terminal sessions with scroll, navigation, and instruction
// overlay capabilities.
//
// See: specs/Public/ah-agent-record.md section 6 for complete specification

use crate::settings::Settings;
use crate::terminal::{self, TerminalConfig};
use crate::view::HitTestRegistry;
use crate::view::Theme;
use crate::view::session_viewer::render_session_viewer;
use crate::view_model::autocomplete::AutocompleteDependencies;
use crate::view_model::session_viewer_model::{
    GutterConfig, GutterPosition, SESSION_VIEWER_MODE, SessionViewerMode, SessionViewerMouseAction,
    SessionViewerMsg, SessionViewerViewModel,
};
use ah_core::TaskManager;
use ah_domain_types::AgentSoftware;
use ah_recorder::TerminalState;
use ah_repo;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, MouseButton, MouseEvent};
use ratatui::{Frame, Terminal, backend::CrosstermBackend};
use std::cell::RefCell;
use std::io;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;
use tracing::debug;

/// Viewer configuration
#[derive(Debug, Clone)]
pub struct ViewerConfig {
    /// Full terminal size available to the viewer
    pub terminal_cols: u16,
    pub terminal_rows: u16,
    /// Scrollback buffer size (lines)
    pub scrollback: usize,
    /// Gutter display configuration
    pub gutter: GutterConfig,
    /// Whether this viewer is in replay mode (vs live recording)
    pub is_replay_mode: bool,
}

pub(crate) fn default_autocomplete_dependencies() -> Arc<AutocompleteDependencies> {
    let settings = Settings::from_config().unwrap_or_else(|_| Settings::default());

    let repo = ah_repo::VcsRepo::new(".")
        .or_else(|_| ah_repo::VcsRepo::new("/tmp"))
        .unwrap_or_else(|_| panic!("Failed to create fallback VcsRepo"));

    let deps = AutocompleteDependencies::from_vcs_repo(repo, settings)
        .unwrap_or_else(|err| panic!("Failed to create autocomplete dependencies: {err}"));

    Arc::new(deps)
}

pub(crate) fn build_session_viewer_view_model(
    recording_terminal_state: Rc<RefCell<TerminalState>>,
    config: &ViewerConfig,
    autocomplete_dependencies: Option<Arc<AutocompleteDependencies>>,
) -> SessionViewerViewModel {
    let deps = autocomplete_dependencies.unwrap_or_else(default_autocomplete_dependencies);

    let session_mode = if config.is_replay_mode {
        SessionViewerMode::SessionReview
    } else {
        SessionViewerMode::LiveRecording
    };

    let task_entry =
        SessionViewerViewModel::build_task_entry_view_model(&deps, "session-viewer", None);

    let mut view_model = SessionViewerViewModel::new(
        task_entry,
        recording_terminal_state,
        config.gutter,
        config.terminal_cols,
        config.terminal_rows,
        deps,
        session_mode,
    );

    if session_mode == SessionViewerMode::LiveRecording {
        view_model.hide_task_entry();
    }

    view_model
}

pub fn update_row_metadata_with_autofollow(
    view_model: &mut SessionViewerViewModel,
    _config: &ViewerConfig,
) {
    let previous_total = view_model.total_rows();

    // Auto-follow is suppressed when task entry is visible
    if view_model.auto_follow
        && !view_model.task_entry_visible
        && view_model.total_rows() > previous_total
    {
        view_model.scroll_to_bottom(view_model.display_rows() as usize);
    }
}

pub(crate) fn render_view_frame(
    frame: &mut Frame,
    view_model: &mut SessionViewerViewModel,
    _config: &ViewerConfig,
    _exit_confirmation_armed: bool,
    _recorded_dims: (u16, u16),
) {
    // Create hit test registry for session viewer mouse interactions
    let mut hit_registry = HitTestRegistry::<SessionViewerMouseAction>::new();
    let theme = Theme::default();

    render_session_viewer(frame, view_model, &mut hit_registry, &theme);
}

pub(crate) fn handle_mouse_click_for_view(
    view_model: &mut SessionViewerViewModel,
    config: &ViewerConfig,
    col: u16,
    row: u16,
) {
    let gutter_width = view_model.gutter_config.width();
    let recorded_cols = view_model.display_cols() as usize;
    let available_width: usize = view_model.terminal_cols as usize;
    let viewport_cols =
        recorded_cols.min(available_width.saturating_sub(gutter_width).saturating_sub(2));

    let total_width = viewport_cols as u16 + 2 + gutter_width as u16;
    let x_offset = (view_model.terminal_cols.saturating_sub(total_width)) / 2;

    let is_in_gutter = match config.gutter.position {
        GutterPosition::Left => col >= x_offset && col < x_offset + gutter_width as u16,
        GutterPosition::Right => {
            col >= x_offset + viewport_cols as u16 + 2
                && col < x_offset + viewport_cols as u16 + 2 + gutter_width as u16
        }
        GutterPosition::None => false,
    };

    let visible_start = view_model.scroll_offset.as_usize();
    let clicked_row = visible_start + row as usize;

    if clicked_row < view_model.total_rows() {
        if is_in_gutter {
            if let Some(snapshot) = view_model.find_nearest_snapshot(clicked_row) {
                tracing::debug!("Clicked gutter snapshot marker: {}", snapshot.anchor_byte);
                view_model.start_instruction_overlay(&clicked_row.to_string(), None);
            }
        } else if let Some(snapshot) = view_model.find_nearest_snapshot(clicked_row) {
            tracing::debug!("Clicked near snapshot: {}", snapshot.anchor_byte);
        } else {
            view_model.start_instruction_overlay(&clicked_row.to_string(), None);
        }
    }
}

pub(crate) async fn launch_task_from_instruction(
    recording_terminal_state: Rc<RefCell<TerminalState>>,
    task_manager: Arc<dyn TaskManager>,
    instruction: String,
    selected_agents: &[ah_domain_types::AgentChoice],
) {
    let current_snapshot = {
        let recording_state = recording_terminal_state.borrow();
        recording_state.all_snapshots().last().cloned()
    };

    if let Some(snapshot) = current_snapshot {
        tracing::info!(
            "Creating task from instruction at snapshot {} (byte offset: {}): {}",
            snapshot.anchor_byte,
            snapshot.anchor_byte,
            instruction
        );

        let task_id = uuid::Uuid::new_v4().to_string();
        let params = match ah_core::TaskLaunchParams::builder()
            .starting_point(ah_core::task_manager::StartingPoint::FilesystemSnapshot {
                snapshot_id: snapshot.anchor_byte.to_string(),
            })
            .working_copy_mode(ah_core::WorkingCopyMode::Snapshots)
            .description(instruction)
            .agents(selected_agents.to_vec())
            .agent_type(AgentSoftware::Codex) // Default agent type
            .task_id(task_id)
            .build()
        {
            Ok(params) => params,
            Err(e) => {
                tracing::error!("Failed to create task launch params: {}", e);
                return;
            }
        };

        match task_manager.launch_task(params).await {
            ah_core::TaskLaunchResult::Success { session_ids } => {
                if session_ids.len() == 1 {
                    tracing::info!(
                        "Successfully launched session {} from instruction",
                        session_ids[0]
                    );
                } else {
                    tracing::info!(
                        "Successfully launched {} sessions from instruction: {}",
                        session_ids.len(),
                        session_ids.join(", ")
                    );
                }
            }
            ah_core::TaskLaunchResult::Failure { error } => {
                tracing::error!("Failed to launch task from instruction: {}", error);
            }
        }
    } else {
        tracing::warn!("No snapshots available, cannot create task from instruction");
    }
}

// Viewer state for the terminal display

// Restore terminal state when the event loop drops
impl Drop for ViewerEventLoop {
    fn drop(&mut self) {
        // Ignore errors during teardown
        let _ = self.terminal.show_cursor();
        terminal::cleanup_terminal();
    }
}

/// Event loop for the terminal viewer
pub struct ViewerEventLoop {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
    view_model: SessionViewerViewModel,
    config: ViewerConfig,
    task_manager: std::sync::Arc<dyn TaskManager>,
}

impl ViewerEventLoop {
    /// Create a new event loop
    pub fn new(
        view_model: SessionViewerViewModel,
        config: ViewerConfig,
        task_manager: std::sync::Arc<dyn TaskManager>,
    ) -> io::Result<Self> {
        Self::new_with_config(view_model, config, task_manager, TerminalConfig::minimal())
    }

    /// Create a new event loop with custom terminal config
    pub fn new_with_config(
        view_model: SessionViewerViewModel,
        config: ViewerConfig,
        task_manager: std::sync::Arc<dyn TaskManager>,
        terminal_config: TerminalConfig,
    ) -> io::Result<Self> {
        // Enter alternate screen and enable raw mode before building Terminal
        if let Err(e) = terminal::setup_terminal(terminal_config) {
            return Err(io::Error::other(format!("{}", e)));
        }

        let backend = CrosstermBackend::new(std::io::stdout());
        let mut terminal = Terminal::new(backend)?;
        // Ensure a pristine first frame
        terminal.clear()?;

        Ok(Self {
            terminal,
            view_model,
            config,
            task_manager,
        })
    }

    /// Run the event loop until quit
    pub async fn run(&mut self) -> io::Result<()> {
        loop {
            // Update viewer state
            update_row_metadata_with_autofollow(&mut self.view_model, &self.config);

            // Draw the UI
            let config_ref = &self.config;
            let exit_confirmation = self.view_model.exit_confirmation_armed;
            let recorded_dims = self.view_model.recording_dims();
            let view_model_ptr = &mut self.view_model;
            self.terminal.draw(|f| {
                render_view_frame(
                    f,
                    view_model_ptr,
                    config_ref,
                    exit_confirmation,
                    recorded_dims,
                );
            })?;

            // Handle input with timeout
            if event::poll(Duration::from_millis(100))? {
                match event::read()? {
                    Event::Key(key) => {
                        // Debug logging for key events
                        debug!(
                            key_code = ?key.code,
                            modifiers = ?key.modifiers,
                            key_kind = ?key.kind,
                            focus_element = ?self.view_model.focus_element,
                            "Key event received in replay viewer"
                        );

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

    /// Handle ESC key dismissal logic (similar to dashboard's handle_dismiss_overlay)
    fn handle_dismiss_overlay(&mut self) -> bool {
        // First priority: dismiss instruction entry if it exists
        if self.view_model.task_entry_visible {
            self.view_model.cancel_instruction_overlay();
            // Don't return here - continue to check for exit confirmation
        }

        // Second priority: exit search mode if active
        if self.view_model.search_state.is_some() {
            self.view_model.exit_search();
            self.view_model.exit_confirmation_armed = false;
            return false;
        }

        // Third priority: handle exit confirmation (return expression style)
        if self.view_model.exit_confirmation_armed {
            // Second ESC - quit
            true
        } else {
            // First ESC - arm confirmation
            self.view_model.exit_confirmation_armed = true;
            false
        }
    }

    /// Handle keyboard input, returns true if should quit
    async fn handle_key(&mut self, key: KeyEvent) -> io::Result<bool> {
        // Clear exit confirmation on any non-ESC key
        if !matches!(key.code, ratatui::crossterm::event::KeyCode::Esc) {
            self.view_model.exit_confirmation_armed = false;
        }

        // Handle ESC key with dismiss overlay logic
        if key.code == KeyCode::Esc {
            return Ok(self.handle_dismiss_overlay());
        }

        // Check if this is a MoveToPreviousSnapshot or MoveToNextSnapshot operation that should be handled
        // by the session viewer model even when task entry is visible
        if let Some(operation) =
            SESSION_VIEWER_MODE.resolve_key_to_operation(&key, &Default::default())
        {
            if matches!(
                operation,
                crate::settings::KeyboardOperation::MoveToPreviousSnapshot
                    | crate::settings::KeyboardOperation::MoveToNextSnapshot
            ) {
                // These operations should be handled by the session viewer model
                // Convert to SessionViewerMsg and handle it
                let msgs = self.view_model.update(SessionViewerMsg::Key(key));
                // Process any returned messages (though these operations typically return empty vec)
                for msg in msgs {
                    if msg == super::Msg::Quit {
                        return Ok(true);
                    }
                }
                return Ok(false);
            }
        }

        // First handle draft card input if it exists
        if self.view_model.task_entry_visible {
            let handled = self.view_model.handle_instruction_key(&key);
            if handled {
                return Ok(false);
            }

            if key.code == KeyCode::Enter {
                if let Some(instruction) = self.view_model.instruction_text() {
                    let recording_state = self.view_model.recording_terminal_state.clone();
                    let task_manager = Arc::clone(&self.task_manager);
                    self.view_model.cancel_instruction_overlay();
                    let empty_agents = Vec::new();
                    let selected_agents = self
                        .view_model
                        .task_entry_overlay()
                        .map(|entry| &entry.selected_agents)
                        .unwrap_or(&empty_agents);
                    launch_task_from_instruction(
                        recording_state,
                        task_manager,
                        instruction,
                        selected_agents,
                    )
                    .await;
                }
                return Ok(false);
            }

            // If the overlay is active, consume all other keys
            return Ok(false);
        }

        // Handle keys that are not for the draft card

        match key.code {
            KeyCode::Char('q') => {
                return Ok(true); // Quit
            }
            KeyCode::Char('i') if key.modifiers.is_empty() => {
                let row_id = self.view_model.scroll_offset.as_usize().to_string();
                self.view_model.start_instruction_overlay(&row_id, None);
            }
            KeyCode::Char('/') => {
                self.view_model.start_search();
            }
            KeyCode::Up => {
                self.view_model.scroll_up(1, self.view_model.display_rows() as usize);
            }
            KeyCode::Down => {
                self.view_model.scroll_down(1, self.view_model.display_rows() as usize);
            }
            KeyCode::PageUp => {
                let viewport = self.view_model.display_rows() as usize;
                self.view_model.scroll_up(viewport, viewport);
            }
            KeyCode::PageDown => {
                let viewport = self.view_model.display_rows() as usize;
                self.view_model.scroll_down(viewport, viewport);
            }
            KeyCode::Char('[') => {
                self.view_model.prev_search_result();
            }
            KeyCode::Char(']') => {
                self.view_model.next_search_result();
            }
            _ => {}
        }

        Ok(false)
    }

    /// Handle mouse input
    fn handle_mouse(&mut self, mouse: MouseEvent) {
        if let event::MouseEventKind::Down(MouseButton::Left) = mouse.kind {
            handle_mouse_click_for_view(
                &mut self.view_model,
                &self.config,
                mouse.column,
                mouse.row,
            );
        }
    }
}
