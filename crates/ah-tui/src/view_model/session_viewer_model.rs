// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! SessionViewerModel layer for SessionViewer UI presentation logic
//!
//! This module follows the same MVVM architecture as dashboard_model.rs,
//! providing UI presentation logic and state management for both live session
//! tracking mode and post-facto session examination modes.

use crate::settings::KeyboardOperation;

// Keyboard operations used in session viewer
use crate::view_model::autocomplete::{AutocompleteDependencies, InlineAutocomplete};
use crate::view_model::input::minor_modes;
use crate::view_model::task_entry::DRAFT_TEXT_EDITING_MODE;
use crate::view_model::task_entry::{
    AutocompleteManager, KeyboardOperationResult, TaskEntryControlsViewModel, TaskEntryViewModel,
};
use crate::view_model::{ButtonStyle, ButtonViewModel, DraftSaveState};
use ah_recorder::{LineIndex, Snapshot, TerminalState};
use chrono::Utc;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::style::{Color, Modifier, Style};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use tracing::{debug, trace};

// Minor mode for terminal navigation operations that work even when task entry is focused
pub static TERMINAL_NAVIGATION_MODE: crate::view_model::input::InputMinorMode =
    crate::view_model::input::InputMinorMode::new(&[
        KeyboardOperation::MoveToNextLine,
        KeyboardOperation::MoveToPreviousLine,
        KeyboardOperation::ScrollUpOneScreen,
        KeyboardOperation::ScrollDownOneScreen,
        KeyboardOperation::MoveToBeginningOfDocument,
        KeyboardOperation::MoveToEndOfDocument,
        KeyboardOperation::MoveToPreviousSnapshot,
        KeyboardOperation::MoveToNextSnapshot,
        KeyboardOperation::IncrementalSearchForward,
    ]);

// Minor mode for session viewer navigation (for viewing terminal sessions and navigating snapshots)
pub static SESSION_VIEWER_MODE: crate::view_model::input::InputMinorMode =
    crate::view_model::input::InputMinorMode::new(&[
        KeyboardOperation::MoveToNextField,
        KeyboardOperation::MoveToPreviousField,
        KeyboardOperation::DismissOverlay,
        KeyboardOperation::DraftNewTask,
    ]);

/// Mouse actions for the SessionViewer interface
#[derive(Debug, Clone)]
pub enum SessionViewerMouseAction {
    /// Click on a specific line in the terminal output
    ClickLine(usize),
    /// Click on the gutter area
    ClickGutter,
}

/// UI messages for the SessionViewer interface
#[derive(Debug, Clone)]
pub enum SessionViewerMsg {
    /// User keyboard input events
    Key(KeyEvent),
    /// Mouse click events
    MouseClick { column: u16, row: u16 },
    /// Mouse scroll up event
    MouseScrollUp,
    /// Mouse scroll down event
    MouseScrollDown,
    /// Periodic timer tick for animations/updates
    Tick,
    /// Application lifecycle events
    Quit,
}

/// Gutter position options
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GutterPosition {
    Left,
    Right,
    None,
}

/// Gutter display configuration
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GutterConfig {
    pub position: GutterPosition,
    pub show_line_numbers: bool,
}

impl GutterConfig {
    /// Width in columns required by the gutter configuration
    pub fn width(&self) -> usize {
        match self.position {
            GutterPosition::None => 0,
            GutterPosition::Left | GutterPosition::Right => 2, // Always 3 columns: 1 for left space, 1 for snapshot indicator, 1 for right space
        }
    }
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

/// Focus states specific to the SessionViewer interface
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SessionViewerFocusState {
    TaskEntry, // The task entry card is focused
    Terminal,  // The terminal output area is focused (for scrolling)
}

/// Display items for rendering the session viewer UI
#[derive(Debug, Clone, PartialEq)]
pub enum DisplayItem {
    /// A terminal line at the specified absolute line index
    TerminalLine(LineIndex),
    /// The task entry UI component
    TaskEntry,
}

/// A span of terminal output lines
#[derive(Debug, Clone, PartialEq)]
pub struct TerminalOutputSpan {
    /// The first line index (inclusive)
    pub first_line: LineIndex,
    /// The last line index (inclusive)
    pub last_line: LineIndex,
}

impl TerminalOutputSpan {
    /// Create an empty span
    pub fn empty() -> Self {
        Self {
            first_line: LineIndex(1),
            last_line: LineIndex(0),
        }
    }

    /// Check if the span is empty
    pub fn is_empty(&self) -> bool {
        self.first_line.as_usize() > self.last_line.as_usize()
    }

    /// Get the number of lines in this span
    pub fn len(&self) -> usize {
        if self.is_empty() {
            0
        } else {
            self.last_line.as_usize() - self.first_line.as_usize() + 1
        }
    }
}

/// Structure describing the display layout of the session viewer
#[derive(Debug, Clone, PartialEq)]
pub struct DisplayStructure {
    /// Terminal output span for all visible lines
    pub terminal_output: TerminalOutputSpan,
    /// Terminal lines before the task entry (empty if task entry not visible)
    pub before_task_entry: TerminalOutputSpan,
    /// Height of the task entry (0 if not visible)
    pub task_entry_height: usize,
    /// Terminal lines after the task entry (empty if task entry not visible)
    pub after_task_entry: TerminalOutputSpan,
}

/// Status bar view model for recorder interface
#[derive(Debug, Clone)]
pub struct StatusBarViewModel {
    pub recording_status: String, // "Recording", "Paused", "Stopped", etc.
    pub duration: String,         // Recording duration
    pub snapshot_count: usize,    // Number of snapshots taken
    pub error_message: Option<String>, // Error messages
    pub status_message: Option<String>, // Success/status messages
    pub exit_confirmation_message: Option<String>, // Exit confirmation message
}

/// Session mode indicating whether we're recording live or reviewing a completed session
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SessionViewerMode {
    /// Currently recording a live session
    LiveRecording,
    /// Reviewing a completed session from an AHR file
    SessionReview,
}

/// Main SessionViewerViewModel for the SessionViewer interface
pub struct SessionViewerViewModel {
    /// Current UI focus state
    pub focus_element: SessionViewerFocusState,

    /// Task entry card for creating recording tasks
    pub task_entry: TaskEntryViewModel,

    /// Status bar state
    pub status_bar: StatusBarViewModel,

    /// Gutter configuration
    pub gutter_config: GutterConfig,

    /// Terminal recording state (shared with viewer)
    pub recording_terminal_state: Rc<RefCell<TerminalState>>,

    /// Autocomplete state for task entry
    pub autocomplete: InlineAutocomplete,

    autocomplete_dependencies: Arc<AutocompleteDependencies>,

    /// Full terminal size available to the viewer
    pub terminal_cols: u16,
    pub terminal_rows: u16,

    /// UI state
    pub exit_confirmation_armed: bool,
    pub exit_requested: bool,

    /// Scroll state
    /// The LineIndex of the first visible line in the terminal viewport.
    /// This represents an absolute line position in the complete terminal output history,
    /// using the same coordinate system as snapshot positioning (LineIndex).
    ///
    /// When lines scroll out of memory, scroll_offset may need adjustment to stay within
    /// the currently available content range, but the LineIndex type ensures type safety
    /// and clear reasoning about absolute positioning.
    ///
    /// Valid range: LineIndex(0) <= scroll_offset <= LineIndex(max(0, total_rows_in_memory - viewport_height))
    /// When scroll_offset = LineIndex(0), the oldest available line is at the top of the viewport.
    /// When scroll_offset = LineIndex(total_rows_in_memory - viewport_height), the newest lines are visible.
    pub scroll_offset: LineIndex,
    pub auto_follow: bool,

    /// Session mode: live recording or session review
    pub session_mode: SessionViewerMode,

    /// Whether the task entry UI is currently shown
    pub task_entry_visible: bool,

    /// Incremental search state
    pub search_state: Option<SearchState>,

    /// Current snapshot index for task entry navigation (in live mode)
    pub current_snapshot_index: Option<usize>,
}

impl SessionViewerViewModel {
    /// Create a new SessionViewer view model
    pub fn new(
        task_entry: TaskEntryViewModel,
        recording_terminal_state: Rc<RefCell<TerminalState>>,
        gutter_config: GutterConfig,
        terminal_cols: u16,
        terminal_rows: u16,
        autocomplete_dependencies: std::sync::Arc<AutocompleteDependencies>,
        session_mode: SessionViewerMode,
    ) -> Self {
        let task_entry_visible = match session_mode {
            SessionViewerMode::LiveRecording => false, // Hidden by default in live mode
            SessionViewerMode::SessionReview => true,  // Shown by default in review mode
        };

        Self {
            focus_element: SessionViewerFocusState::TaskEntry,
            task_entry,
            status_bar: StatusBarViewModel {
                recording_status: "Ready".to_string(),
                duration: "00:00:00".to_string(),
                snapshot_count: 0,
                error_message: None,
                status_message: None,
                exit_confirmation_message: None,
            },
            gutter_config,
            recording_terminal_state,
            autocomplete: InlineAutocomplete::with_dependencies(autocomplete_dependencies.clone()),
            autocomplete_dependencies,
            terminal_cols,
            terminal_rows,
            exit_confirmation_armed: false,
            exit_requested: false,
            scroll_offset: LineIndex(0),
            auto_follow: true,
            session_mode,
            task_entry_visible,
            search_state: None,
            current_snapshot_index: None,
        }
    }

    /// Calculate the display area columns (terminal width minus horizontal padding and gutter)
    pub fn display_cols(&self) -> u16 {
        const LEFT_PADDING: u16 = 2; // Left padding
        const RIGHT_PADDING: u16 = 2; // Right padding (3 spaces as requested)
        self.terminal_cols
            .saturating_sub(LEFT_PADDING)
            .saturating_sub(RIGHT_PADDING)
            .saturating_sub(self.gutter_config.width() as u16)
    }

    /// Get the dimensions of the recorded terminal content
    pub fn recording_dims(&self) -> (u16, u16) {
        let state = self.recording_terminal_state.borrow();
        state.dimensions()
    }

    /// Calculate the display area rows (terminal height minus status bar, and task entry if in bottom positioning)
    pub fn display_rows(&self) -> u16 {
        let mut rows = self.terminal_rows.saturating_sub(1); // status bar

        // Subtract task entry height if visible in bottom positioning (when current_snapshot_index is None)
        if self.task_entry_visible && self.current_snapshot_index.is_none() {
            rows = rows.saturating_sub(self.task_entry.full_height());
        }

        rows
    }

    /// Show the task entry UI
    pub fn show_task_entry(&mut self) {
        self.task_entry_visible = true;
    }

    /// Hide the task entry UI
    pub fn hide_task_entry(&mut self) {
        self.task_entry_visible = false;
    }

    /// Returns the currently active task entry overlay if visible
    pub fn task_entry_overlay(&self) -> Option<&TaskEntryViewModel> {
        self.task_entry_visible.then_some(&self.task_entry)
    }

    /// Returns the currently active task entry overlay mutably if visible
    pub fn task_entry_overlay_mut(&mut self) -> Option<&mut TaskEntryViewModel> {
        if self.task_entry_visible {
            Some(&mut self.task_entry)
        } else {
            None
        }
    }

    /// Replace the current task entry overlay and mark it visible
    pub fn set_task_entry_overlay(&mut self, task_entry: TaskEntryViewModel) {
        self.task_entry = task_entry;
        self.task_entry_visible = true;
    }

    /// Clear the task entry overlay contents without modifying the underlying model
    pub fn clear_task_entry_overlay(&mut self) {
        self.task_entry_visible = false;
    }

    /// Check if the session is in live recording mode
    pub fn is_live_recording(&self) -> bool {
        matches!(self.session_mode, SessionViewerMode::LiveRecording)
    }

    /// Check if the session is in review mode
    pub fn is_session_review(&self) -> bool {
        matches!(self.session_mode, SessionViewerMode::SessionReview)
    }

    /// Handle UI messages and return any domain messages that need processing
    pub fn update(&mut self, msg: SessionViewerMsg) -> Vec<super::Msg> {
        match msg {
            SessionViewerMsg::Key(key_event) => self.handle_key_event(key_event),
            SessionViewerMsg::MouseClick { column, row } => self.handle_mouse_click(column, row),
            SessionViewerMsg::MouseScrollUp => self.handle_mouse_scroll_up(),
            SessionViewerMsg::MouseScrollDown => self.handle_mouse_scroll_down(),
            SessionViewerMsg::Tick => self.handle_tick(),
            SessionViewerMsg::Quit => {
                self.exit_requested = true;
                vec![super::Msg::Quit]
            }
        }
    }

    /// Handle keyboard events
    fn handle_key_event(&mut self, key: KeyEvent) -> Vec<super::Msg> {
        // Clear exit confirmation on any non-ESC key
        if !matches!(key.code, KeyCode::Esc) {
            self.exit_confirmation_armed = false;
            self.status_bar.exit_confirmation_message = None;
        }

        let settings = crate::Settings::default();

        // Try terminal navigation operations first (always available, even when task entry is focused)
        if let Some(operation) = TERMINAL_NAVIGATION_MODE.resolve_key_to_operation(&key, &settings)
        {
            tracing::debug!(
                "resolve_key_to_operation (TERMINAL_NAVIGATION): {:?} -> {:?}",
                key,
                operation
            );
            return self.handle_keyboard_operation(operation, &key);
        }

        // Try task entry operations (when task entry is focused)
        if self.focus_element == SessionViewerFocusState::TaskEntry {
            if let Some(operation) =
                DRAFT_TEXT_EDITING_MODE.resolve_key_to_operation(&key, &settings)
            {
                tracing::debug!(
                    "resolve_key_to_operation (TASK_ENTRY): {:?} -> {:?}",
                    key,
                    operation
                );
                return self.handle_keyboard_operation(operation, &key);
            }
        }

        // Try session viewer operations
        if let Some(operation) = SESSION_VIEWER_MODE.resolve_key_to_operation(&key, &settings) {
            tracing::debug!(
                "resolve_key_to_operation (SESSION_VIEWER): {:?} -> {:?}",
                key,
                operation
            );
            return self.handle_keyboard_operation(operation, &key);
        }

        tracing::debug!("resolve_key_to_operation: {:?} -> None", key);

        // Key not handled by any input mode
        vec![]
    }

    /// Handle a KeyboardOperation with the original KeyEvent context
    pub fn handle_keyboard_operation(
        &mut self,
        operation: KeyboardOperation,
        key: &KeyEvent,
    ) -> Vec<super::Msg> {
        // When the TaskEntry is focused, delegate handled keyboard operations to it
        if self.focus_element == SessionViewerFocusState::TaskEntry {
            // Check if this operation is handled by the task entry
            if DRAFT_TEXT_EDITING_MODE.handles_operation(&operation) {
                struct AutocompleteManagerImpl<'a> {
                    autocomplete: &'a mut InlineAutocomplete,
                    needs_redraw: &'a mut bool,
                }

                impl<'a> AutocompleteManager for AutocompleteManagerImpl<'a> {
                    fn show(&mut self, _prefix: &str) {
                        // The autocomplete shows itself automatically
                    }

                    fn hide(&mut self) {
                        self.autocomplete.close(&mut false);
                    }

                    fn after_textarea_change(&mut self, textarea: &tui_textarea::TextArea) {
                        self.autocomplete.after_textarea_change(textarea, self.needs_redraw);
                    }

                    fn set_needs_redraw(&mut self) {
                        *self.needs_redraw = true;
                    }
                }

                let mut autocomplete_manager = AutocompleteManagerImpl {
                    autocomplete: &mut self.autocomplete,
                    needs_redraw: &mut false, // Recorder doesn't need redraw signals
                };
                match self.task_entry.handle_keyboard_operation(operation, key, &mut false) {
                    KeyboardOperationResult::Handled => {
                        // Operation was handled by task entry
                        return vec![];
                    }
                    KeyboardOperationResult::Bubble { .. } => {
                        // Let the operation continue bubbling to session viewer handlers
                    }
                    KeyboardOperationResult::TaskLaunched {
                        split_mode,
                        focus,
                        starting_point,
                        working_copy_mode,
                    } => {
                        // Task was launched from session viewer
                        // If we have snapshot context, modify the launch parameters
                        let (final_starting_point, final_working_copy_mode) =
                            if let Some(snapshot_index) = self.current_snapshot_index {
                                // We're continuing from a snapshot, use FilesystemSnapshot starting point
                                let recording_state = self.recording_terminal_state.borrow();
                                let snapshots = recording_state.all_snapshots();
                                if let Some(snapshot) = snapshots.get(snapshot_index) {
                                    let snapshot_id =
                                        format!("snapshot_{}_{}", snapshot.line.0, snapshot.ts_ns);
                                    (
                                    Some(
                                        ah_core::task_manager::StartingPoint::FilesystemSnapshot {
                                            snapshot_id,
                                        },
                                    ),
                                    Some(ah_core::WorkingCopyMode::Snapshots),
                                )
                                } else {
                                    (starting_point, working_copy_mode)
                                }
                            } else {
                                (starting_point, working_copy_mode)
                            };

                        // For now, we don't have a direct way to launch tasks from session viewer
                        // This would need to be integrated with the dashboard or have its own task manager
                        // For now, just mark as handled
                        tracing::info!(
                            "Task launched from session viewer with starting_point: {:?}, working_copy_mode: {:?}",
                            final_starting_point,
                            final_working_copy_mode
                        );
                        return vec![];
                    }
                    KeyboardOperationResult::NotHandled => {
                        // Operation not handled by task entry, continue with other handling
                    }
                }
            }
        }

        // Handle session viewer specific operations
        match operation {
            KeyboardOperation::MoveToNextSnapshot | KeyboardOperation::MoveToNextLine => {
                debug!(operation = ?operation, "Resolved keyboard operation in session viewer");
                self.navigate_to_next_snapshot();
                return vec![];
            }
            KeyboardOperation::MoveToPreviousSnapshot => {
                debug!(operation = ?operation, "Resolved keyboard operation in session viewer");
                if matches!(self.session_mode, SessionViewerMode::LiveRecording)
                    && !self.task_entry_visible
                {
                    // First time: show task entry at latest snapshot
                    self.show_task_entry_at_latest_snapshot();
                } else {
                    // Subsequent times: navigate to previous snapshot
                    self.navigate_to_previous_snapshot();
                }
                return vec![];
            }
            KeyboardOperation::MoveToEndOfDocument => {
                debug!(operation = ?operation, "Resolved keyboard operation in session viewer");
                let viewport_height = self.display_rows() as usize;
                self.scroll_offset = LineIndex(self.total_rows().saturating_sub(viewport_height));
                self.auto_follow = true;
                return vec![];
            }
            KeyboardOperation::MoveToBeginningOfDocument => {
                debug!(operation = ?operation, "Resolved keyboard operation in session viewer");
                self.scroll_offset = LineIndex(0);
                self.auto_follow = false;
                return vec![];
            }
            KeyboardOperation::ScrollUpOneScreen => {
                let viewport_height = self.display_rows() as usize;
                self.scroll_offset =
                    LineIndex(self.scroll_offset.as_usize().saturating_sub(viewport_height));
                self.auto_follow = false;
                return vec![];
            }
            KeyboardOperation::ScrollDownOneScreen => {
                debug!(operation = ?operation, "Resolved keyboard operation in session viewer");
                let viewport_height = self.display_rows() as usize;
                self.scroll_offset =
                    LineIndex(self.scroll_offset.as_usize().saturating_add(viewport_height));
                let max_scroll = self.total_rows().saturating_sub(viewport_height);
                if self.scroll_offset.as_usize() >= max_scroll {
                    self.scroll_offset = LineIndex(max_scroll);
                    self.auto_follow = true;
                } else {
                    self.auto_follow = false;
                }
                return vec![];
            }
            KeyboardOperation::IncrementalSearchForward => {
                debug!(operation = ?operation, "Resolved keyboard operation in session viewer");
                self.start_search();
                return vec![];
            }
            KeyboardOperation::DismissOverlay => {
                debug!(operation = ?operation, "Resolved keyboard operation in session viewer");
                if self.task_entry_visible {
                    self.cancel_instruction_overlay();
                } else if self.exit_confirmation_armed {
                    // Second ESC - quit
                    self.exit_requested = true;
                    return vec![super::Msg::Quit];
                } else {
                    // First ESC - arm confirmation
                    self.exit_confirmation_armed = true;
                    self.status_bar.exit_confirmation_message =
                        Some("Press Esc again to quit".to_string());
                }
                return vec![];
            }
            KeyboardOperation::DraftNewTask => {
                debug!(operation = ?operation, "Resolved keyboard operation in session viewer");
                if !self.task_entry_visible {
                    // Show task entry at latest snapshot for new draft
                    self.show_task_entry_at_latest_snapshot();
                }
                return vec![];
            }
            _ => {}
        }

        // If we reach here, the operation wasn't handled
        vec![]
    }

    /// Handle mouse click events
    fn handle_mouse_click(&mut self, _column: u16, _row: u16) -> Vec<super::Msg> {
        // TODO: Implement mouse click handling for recorder interface
        vec![]
    }

    fn handle_mouse_scroll_up(&mut self) -> Vec<super::Msg> {
        // Scroll up by 3 lines (typical mouse wheel scroll amount)
        let viewport_height = self.display_rows() as usize;
        let total_lines = self.total_rows();

        // If currently auto-following, start from the bottom position
        let current_scroll = if self.auto_follow {
            total_lines.saturating_sub(viewport_height)
        } else {
            self.scroll_offset.as_usize()
        };

        let new_scroll = current_scroll.saturating_sub(3);
        debug!(
            "Mouse scroll up: total_lines={}, viewport_height={}, current_scroll={}, new_scroll={}",
            total_lines, viewport_height, current_scroll, new_scroll
        );

        self.scroll_offset = LineIndex(new_scroll);
        self.auto_follow = false;
        vec![]
    }

    fn handle_mouse_scroll_down(&mut self) -> Vec<super::Msg> {
        // Scroll down by 3 lines (typical mouse wheel scroll amount)
        let viewport_height = self.display_rows() as usize;

        // If currently auto-following, stay at bottom
        if self.auto_follow {
            return vec![];
        }

        self.scroll_offset = LineIndex(self.scroll_offset.as_usize().saturating_add(3));
        let max_scroll = self.total_rows().saturating_sub(viewport_height);
        if self.scroll_offset.as_usize() >= max_scroll {
            self.scroll_offset = LineIndex(max_scroll);
            self.auto_follow = true;
        }
        vec![]
    }

    /// Show task entry UI at the latest snapshot position (live mode)
    fn show_task_entry_at_latest_snapshot(&mut self) {
        let snapshot_info = {
            let recording_state = self.recording_terminal_state.borrow();
            let snapshots = recording_state.all_snapshots();
            snapshots.last().cloned().map(|snapshot| {
                let index = snapshots.len().saturating_sub(1);
                (snapshot, index)
            })
        };

        if let Some((latest_snapshot, latest_index)) = snapshot_info {
            // Set the current snapshot index to the latest
            self.current_snapshot_index = Some(latest_index);

            // Create task entry at the snapshot position
            let instruction_text = Some(format!(
                "Continue from snapshot at line {}",
                latest_snapshot.line.0 + 1
            ));
            self.start_instruction_overlay("latest", instruction_text);

            // Scroll to make the snapshot visible
            self.scroll_to_snapshot(&latest_snapshot);
        }
    }

    /// Show task entry at a specific snapshot index (for testing)
    pub fn show_task_entry_at_snapshot_index(&mut self, snapshot_index: usize) {
        let snapshot_info = {
            let recording_state = self.recording_terminal_state.borrow();
            let snapshots = recording_state.all_snapshots();
            snapshots
                .get(snapshot_index)
                .cloned()
                .map(|snapshot| (snapshot, snapshot_index))
        };

        if let Some((snapshot, index)) = snapshot_info {
            // Set the current snapshot index
            self.current_snapshot_index = Some(index);

            // Create task entry at the snapshot position
            let instruction_text = Some(format!(
                "Continue from snapshot at line {}",
                snapshot.line.0 + 1
            ));
            self.start_instruction_overlay("test", instruction_text);

            // Scroll to make the snapshot visible
            self.scroll_to_snapshot(&snapshot);
        }
    }

    /// Navigate to the next snapshot
    fn navigate_to_next_snapshot(&mut self) {
        if !self.task_entry_visible {
            return; // Only navigate if task entry is already shown
        }

        let all_snapshots = {
            let recording_state = self.recording_terminal_state.borrow();
            recording_state.all_snapshots().to_vec()
        };

        if all_snapshots.is_empty() {
            return;
        }

        // Initialize current_snapshot_index to the latest snapshot if not set
        let current_index = self.current_snapshot_index.unwrap_or_else(|| {
            let latest_index = all_snapshots.len().saturating_sub(1);
            self.current_snapshot_index = Some(latest_index);
            latest_index
        });

        let next_index = (current_index + 1).min(all_snapshots.len().saturating_sub(1));

        if next_index != current_index {
            self.current_snapshot_index = Some(next_index);
            debug!(next_index = next_index, "Navigating to next snapshot");
            let snapshot = &all_snapshots[next_index];
            self.update_task_entry_for_snapshot(snapshot);
            self.scroll_to_snapshot(snapshot);
        }
    }

    /// Navigate to the previous snapshot
    fn navigate_to_previous_snapshot(&mut self) {
        if !self.task_entry_visible {
            return; // Only navigate if task entry is already shown
        }

        let all_snapshots = {
            let recording_state = self.recording_terminal_state.borrow();
            recording_state.all_snapshots().to_vec()
        };

        if all_snapshots.is_empty() {
            return;
        }

        // Initialize current_snapshot_index to the latest snapshot if not set
        let current_index = self.current_snapshot_index.unwrap_or_else(|| {
            let latest_index = all_snapshots.len().saturating_sub(1);
            self.current_snapshot_index = Some(latest_index);
            latest_index
        });

        let prev_index = current_index.saturating_sub(1);

        if prev_index != current_index {
            self.current_snapshot_index = Some(prev_index);
            let snapshot = &all_snapshots[prev_index];
            self.update_task_entry_for_snapshot(snapshot);
            self.scroll_to_snapshot(snapshot);
        }
    }

    /// Update the task entry instruction text for a snapshot
    fn update_task_entry_for_snapshot(&mut self, snapshot: &ah_recorder::Snapshot) {
        let instruction_text = format!("Continue from snapshot at line {}", snapshot.line.0 + 1);
        if let Some(task_entry) = self.task_entry_overlay_mut() {
            // Clear existing text and set new instruction
            task_entry.description = tui_textarea::TextArea::new(vec![instruction_text]);
            task_entry.description.set_style(
                ratatui::style::Style::default().bg(ratatui::style::Color::Rgb(17, 17, 27)),
            );
        }
    }

    /// Scroll to make a snapshot visible in the viewport
    fn scroll_to_snapshot(&mut self, snapshot: &ah_recorder::Snapshot) {
        let viewport_height = self.display_rows() as usize;
        let target_line = snapshot.line.as_usize();

        // Check if target is already visible with enough room for task entry
        let current_start = self.scroll_offset.as_usize();
        let current_end = current_start + viewport_height;
        let task_entry_height = if self.task_entry_visible {
            self.task_entry.full_height() as usize
        } else {
            0
        };

        // Check if the snapshot is visible and there's enough room for the task entry
        let has_room_for_task_entry = if self.task_entry_visible {
            // The task entry starts at the snapshot line and needs task_entry_height lines
            target_line + task_entry_height <= current_start + viewport_height
        } else {
            true
        };

        if target_line >= current_start && target_line < current_end && has_room_for_task_entry {
            // Already visible with enough room, don't scroll
            self.auto_follow = false;
            return;
        }

        // Not visible or not enough room, center the snapshot in the viewport
        // Account for task entry height when centering
        let available_height_for_content = if self.task_entry_visible {
            viewport_height.saturating_sub(task_entry_height)
        } else {
            viewport_height
        };

        let target_scroll = if target_line < available_height_for_content / 2 {
            0 // Show at top if in first half of available content height
        } else {
            target_line.saturating_sub(available_height_for_content / 2)
        };

        self.scroll_offset =
            LineIndex(target_scroll.min(self.total_rows().saturating_sub(viewport_height)));
        self.auto_follow = false;
    }

    /// Handle periodic tick events
    fn handle_tick(&mut self) -> Vec<super::Msg> {
        // Update status bar with current recording duration
        // TODO: Implement duration tracking

        // Auto-follow if enabled
        if self.auto_follow {
            let viewport_height = self.display_rows() as usize;
            self.scroll_offset = LineIndex(self.total_rows().saturating_sub(viewport_height));
        }

        vec![]
    }

    /// Update recording status
    pub fn set_recording_status(&mut self, status: String) {
        self.status_bar.recording_status = status;
    }

    /// Update snapshot count
    pub fn set_snapshot_count(&mut self, count: usize) {
        self.status_bar.snapshot_count = count;
    }

    /// Set error message
    pub fn set_error_message(&mut self, message: Option<String>) {
        self.status_bar.error_message = message;
    }

    /// Set status message
    pub fn set_status_message(&mut self, message: Option<String>) {
        self.status_bar.status_message = message;
    }

    /// Begin an incremental search session
    pub fn start_search(&mut self) {
        self.search_state = Some(SearchState {
            query: String::new(),
            cursor_pos: 0,
            results: Vec::new(),
            current_result: 0,
        });
    }

    /// Update the search query
    pub fn update_search(&mut self, query: String) {
        if let Some(ref mut search) = self.search_state {
            search.cursor_pos = query.len();
            search.query = query;
            // TODO: populate search.results by scanning terminal contents
            search.results.clear();
            search.current_result = 0;
        }
    }

    /// Move to the next search result when available
    pub fn next_search_result(&mut self) {
        if let Some(ref mut search) = self.search_state {
            if !search.results.is_empty() {
                search.current_result = (search.current_result + 1) % search.results.len();
            }
        }
    }

    /// Move to the previous search result when available
    pub fn prev_search_result(&mut self) {
        if let Some(ref mut search) = self.search_state {
            if !search.results.is_empty() {
                if search.current_result == 0 {
                    search.current_result = search.results.len() - 1;
                } else {
                    search.current_result -= 1;
                }
            }
        }
    }

    /// Exit the incremental search mode
    pub fn exit_search(&mut self) {
        self.search_state = None;
    }

    /// Current number of terminal rows in scrollback
    pub fn total_rows(&self) -> usize {
        let lines_in_memory = self.recording_terminal_state.borrow().total_output_lines_in_memory();
        // For empty terminals, we conceptually have terminal_height worth of empty lines
        lines_in_memory.max(self.terminal_rows as usize)
    }

    /// Find nearest snapshot for provided row index
    pub fn find_nearest_snapshot(&self, row_index: usize) -> Option<Snapshot> {
        let recording_state = self.recording_terminal_state.borrow();
        let snapshots = recording_state.all_snapshots();

        if let Some(snapshot) = snapshots.iter().find(|s| s.line.0 == row_index) {
            return Some(snapshot.clone());
        }

        snapshots
            .iter()
            .min_by_key(|s| (s.line.0 as i64 - row_index as i64).abs())
            .cloned()
    }

    /// Start instruction overlay seeded with optional text
    pub fn start_instruction_overlay(&mut self, id_suffix: &str, text: Option<String>) {
        let task_entry =
            Self::build_task_entry_view_model(&self.autocomplete_dependencies, id_suffix, text);
        self.set_task_entry_overlay(task_entry);
    }

    /// Cancel the current instruction overlay
    pub fn cancel_instruction_overlay(&mut self) {
        self.clear_task_entry_overlay();
    }

    /// Fetch current instruction text if overlay is visible
    pub fn instruction_text(&self) -> Option<String> {
        self.task_entry_overlay()
            .map(|entry| entry.description.lines().join("\n"))
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty())
    }

    /// Get the display structure describing what should be shown.
    ///
    /// This method returns a DisplayStructure that describes the layout of the session viewer UI.
    /// The structure contains:
    /// - `terminal_output`: The complete span of visible terminal lines
    /// - `before_task_entry`: Terminal lines before the inline task entry (empty if task entry not visible)
    /// - `task_entry_height`: Height of the task entry widget (0 if not visible)
    /// - `after_task_entry`: Terminal lines after the inline task entry (empty if task entry not visible)
    ///
    /// The total height of all spans plus task_entry_height is less than or equal to the display height.
    /// When task_entry is not visible, after_task_entry is empty.
    /// When task_entry is visible but not in the visible range, before_task_entry and after_task_entry are empty.
    pub fn get_display_structure(&mut self) -> DisplayStructure {
        let viewport_height = self.display_rows() as usize;

        // Determine the effective scroll offset, applying auto-follow if enabled
        // For initial calculation, assume we have enough lines
        let effective_scroll_offset = if self.auto_follow && !self.task_entry_visible {
            // Auto-follow: show the last viewport_height lines
            // Assume we have at least viewport_height lines for auto-follow
            viewport_height.saturating_sub(viewport_height)
        } else {
            self.scroll_offset.as_usize()
        };

        let recording_state = self.recording_terminal_state.borrow();
        let total_lines_in_memory = recording_state.total_output_lines_in_memory();
        // For display purposes, we need to show at least enough lines to fill the viewport
        // even if they are conceptually empty
        let total_lines = (effective_scroll_offset + viewport_height).max(total_lines_in_memory);

        // Recalculate effective_scroll_offset now that we know total_lines
        let effective_scroll_offset = if self.auto_follow && !self.task_entry_visible {
            // Auto-follow: show the last viewport_height lines
            total_lines.saturating_sub(viewport_height)
        } else {
            self.scroll_offset.as_usize()
        };

        // Get the task entry line position if we're in inline mode
        let task_entry_line_position =
            if self.task_entry_visible && self.current_snapshot_index.is_some() {
                let snapshot_idx = self.current_snapshot_index.unwrap();
                Some(recording_state.snapshot_line_index(snapshot_idx).as_usize())
            } else {
                None
            };

        let task_entry_height = if self.task_entry_visible {
            self.task_entry.full_height() as usize
        } else {
            0
        };

        let start_line = effective_scroll_offset;

        let (before_task_entry, after_task_entry, task_entry_height_for_structure, end_line) =
            if let Some(task_entry_pos) = task_entry_line_position {
                if task_entry_pos >= start_line
                    && task_entry_pos < start_line + (viewport_height - task_entry_height)
                    && task_entry_height > 0
                {
                    // TaskEntry is visible, split terminal output around it
                    let terminal_lines_height = viewport_height - task_entry_height;
                    let end_line = (start_line + terminal_lines_height).min(total_lines);

                    let before_start = start_line;
                    let before_end = task_entry_pos;
                    let after_start = task_entry_pos;
                    let after_end = end_line;

                    let before_span = if before_start < before_end {
                        TerminalOutputSpan {
                            first_line: LineIndex(before_start),
                            last_line: LineIndex(before_end - 1),
                        }
                    } else {
                        TerminalOutputSpan::empty()
                    };

                    let after_span = if after_start < after_end {
                        TerminalOutputSpan {
                            first_line: LineIndex(after_start),
                            last_line: LineIndex(after_end - 1),
                        }
                    } else {
                        TerminalOutputSpan::empty()
                    };

                    (before_span, after_span, task_entry_height, end_line)
                } else {
                    // TaskEntry is not in visible range, treat all as before
                    let terminal_lines_height = viewport_height;
                    let end_line = (start_line + terminal_lines_height).min(total_lines);

                    let before_span = TerminalOutputSpan {
                        first_line: LineIndex(start_line),
                        last_line: LineIndex(end_line.saturating_sub(1)),
                    };

                    (before_span, TerminalOutputSpan::empty(), 0, end_line)
                }
            } else {
                // No task entry, treat all as before
                let terminal_lines_height = viewport_height;
                let end_line = (start_line + terminal_lines_height).min(total_lines);

                let before_span = TerminalOutputSpan {
                    first_line: LineIndex(start_line),
                    last_line: LineIndex(end_line.saturating_sub(1)),
                };

                (before_span, TerminalOutputSpan::empty(), 0, end_line)
            };

        let terminal_output = TerminalOutputSpan {
            first_line: LineIndex(start_line),
            last_line: LineIndex(end_line.saturating_sub(1)),
        };

        let structure = DisplayStructure {
            terminal_output,
            before_task_entry,
            task_entry_height: task_entry_height_for_structure,
            after_task_entry,
        };

        // Validate the structure
        self.validate_display_structure(&structure);

        structure
    }

    /// Validate that the display structure is correct
    fn validate_display_structure(&self, structure: &DisplayStructure) {
        let expected_viewport_height = self.display_rows() as usize;

        // Calculate total height from structure
        let before_height = structure.before_task_entry.len();
        let after_height = structure.after_task_entry.len();
        let total_height = before_height + structure.task_entry_height + after_height;

        // Validate that total height matches viewport height
        assert_eq!(
            total_height, expected_viewport_height,
            "Display structure height {} does not match viewport height {} - this would cause incorrect rendering",
            total_height, expected_viewport_height
        );

        // Validate that after_task_entry is empty when task entry is not visible
        if !self.task_entry_visible {
            assert!(
                structure.after_task_entry.is_empty(),
                "after_task_entry should be empty when task entry is not visible"
            );
        }
    }

    /// Handle keyboard input destined for the instruction overlay
    pub fn handle_instruction_key(&mut self, key: &KeyEvent) -> bool {
        if !self.task_entry_visible {
            return false;
        }

        struct NoOpAutocompleteManager;
        impl AutocompleteManager for NoOpAutocompleteManager {
            fn show(&mut self, _prefix: &str) {}
            fn hide(&mut self) {}
            fn after_textarea_change(&mut self, _textarea: &tui_textarea::TextArea) {}
            fn set_needs_redraw(&mut self) {}
        }

        let settings = crate::Settings::default();

        // Check for session viewer selection operations (includes DismissOverlay, NewDraft)
        if let Some(operation) =
            minor_modes::SESSION_VIEWER_SELECTION_MODE.resolve_key_to_operation(key, &settings)
        {
            if matches!(operation, KeyboardOperation::DismissOverlay) {
                self.clear_task_entry_overlay();
                return true;
            }
            // Other selection operations are handled by the task entry itself
        }

        // Check for navigation operations (includes MoveToPrevious/MoveToNextSnapshot)
        if let Some(operation) = SESSION_VIEWER_MODE.resolve_key_to_operation(key, &settings) {
            // Handle Previous Snapshot key
            if matches!(operation, KeyboardOperation::MoveToPreviousSnapshot) {
                self.navigate_to_previous_snapshot();
                return true;
            }

            // Handle Next Snapshot key
            if matches!(operation, KeyboardOperation::MoveToNextSnapshot) {
                self.navigate_to_next_snapshot();
                return true;
            }

            if let Some(task_entry) = self.task_entry_overlay_mut() {
                let mut autocomplete_manager = NoOpAutocompleteManager;
                let mut needs_redraw = false;
                match task_entry.handle_keyboard_operation(operation, key, &mut needs_redraw) {
                    KeyboardOperationResult::Handled => return true,
                    KeyboardOperationResult::Bubble { .. } => {}
                    KeyboardOperationResult::TaskLaunched { .. } => {
                        self.clear_task_entry_overlay();
                        return true;
                    }
                    KeyboardOperationResult::NotHandled => {}
                }
            }
        }

        if let Some(task_entry) = self.task_entry_overlay_mut() {
            if matches!(key.code, KeyCode::Char(_)) {
                task_entry.description.input(*key);
                return true;
            }
        }

        false
    }

    /// Scroll upwards by specified lines (show older content)
    pub fn scroll_up(&mut self, lines: usize, _viewport_height: usize) {
        self.scroll_offset = LineIndex(self.scroll_offset.as_usize().saturating_sub(lines));
        self.auto_follow = false;
    }

    /// Scroll downwards by specified lines (show newer content)
    pub fn scroll_down(&mut self, lines: usize, viewport_height: usize) {
        self.scroll_offset = LineIndex(self.scroll_offset.as_usize().saturating_add(lines));
        let max_scroll = self.total_rows().saturating_sub(viewport_height);
        if self.scroll_offset.as_usize() >= max_scroll {
            self.scroll_offset = LineIndex(max_scroll);
            self.auto_follow = true;
        } else {
            self.auto_follow = false;
        }
    }

    /// Scroll to the bottom of the buffer
    pub fn scroll_to_bottom(&mut self, viewport_height: usize) {
        self.scroll_offset = LineIndex(self.total_rows().saturating_sub(viewport_height));
        self.auto_follow = true;
    }

    pub fn build_task_entry_view_model(
        deps: &Arc<AutocompleteDependencies>,
        id_suffix: &str,
        existing_instruction: Option<String>,
    ) -> TaskEntryViewModel {
        use tui_textarea::{CursorMove, TextArea};

        let mut textarea = TextArea::new(vec![existing_instruction.unwrap_or_default()]);
        textarea.set_style(
            Style::default()
                .bg(Color::Rgb(17, 17, 27))
                .remove_modifier(Modifier::UNDERLINED),
        );
        textarea.set_cursor_line_style(Style::default());
        textarea.disable_cursor_rendering();
        textarea.move_cursor(CursorMove::End);

        let visible_lines = textarea.lines().len().max(5); // MIN_TEXTAREA_VISIBLE_LINES = 5
        let inner_height = visible_lines + 4; // padding + separator + buttons
        let height = inner_height as u16 + 2; // account for rounded border

        TaskEntryViewModel {
            id: format!("instruction-{}", id_suffix),
            repository: String::new(),
            branch: String::new(),
            selected_agents: vec![],
            created_at: Utc::now().to_rfc3339(),
            height,
            controls: TaskEntryControlsViewModel {
                repository_button: ButtonViewModel {
                    text: "Repository".to_string(),
                    is_focused: false,
                    style: ButtonStyle::Normal,
                },
                branch_button: ButtonViewModel {
                    text: "Branch".to_string(),
                    is_focused: false,
                    style: ButtonStyle::Normal,
                },
                model_button: ButtonViewModel {
                    text: "Model".to_string(),
                    is_focused: false,
                    style: ButtonStyle::Normal,
                },
                go_button: ButtonViewModel {
                    text: "Go".to_string(),
                    is_focused: false,
                    style: ButtonStyle::Normal,
                },
            },
            save_state: DraftSaveState::Unsaved,
            description: textarea,
            focus_element: crate::view_model::task_entry::CardFocusElement::TaskDescription,
            auto_save_timer: None,
            dirty_generation: 0,
            last_saved_generation: 0,
            pending_save_request_id: None,
            pending_save_invalidated: false,
            repositories_enumerator: None,
            branches_enumerator: None,
            autocomplete: InlineAutocomplete::with_dependencies(deps.clone()),
        }
    }
}

impl Default for GutterConfig {
    fn default() -> Self {
        Self {
            position: GutterPosition::Left,
            show_line_numbers: false,
        }
    }
}
