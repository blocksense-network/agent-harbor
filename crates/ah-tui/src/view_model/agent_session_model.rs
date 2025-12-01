// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! ViewModel for the Agent Activity TUI mock mode.
//!
//! The goal of milestone 0.5 is to allow the session viewer to be driven by
//! mock-agent transcripts without requiring a live VT stream. This lightweight
//! ViewModel focuses on scrollable presentation of agent activity rows
//! (thoughts, tool calls, file edits, etc.) that come from scenario playback.

use crate::settings::{KeyboardOperation, Settings};
use crate::view_model::TaskEntryViewModel;
use crate::view_model::input::{InputState, minor_modes};

use crate::view_model::task_execution::{AgentActivityRow, process_activity_event};
use ah_core::TaskEvent;

/// Messages emitted by the Agent Activity view model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentSessionMsg {
    QuitRequested,
    RedrawRequested,
    ActivateControl { index: usize, action: ControlAction },
    ForkAt(usize),
    TaskEntry(KeyboardOperation),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlAction {
    Copy,
    Expand,
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlFocus {
    Copy,
    Expand,
    Stop,
}

impl ControlFocus {
    pub fn from_index(index: usize) -> Self {
        match index {
            0 => ControlFocus::Stop,
            1 => ControlFocus::Copy,
            2 => ControlFocus::Expand,
            _ => ControlFocus::Copy, // fallback
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusArea {
    Timeline,
    Control(ControlFocus),
    Instructions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputModalKind {
    Text,
    Stderr,
    Binary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputModal {
    pub kind: OutputModalKind,
    pub title: String,
    pub body: String,
}

#[derive(Clone)]
pub struct AgentSessionViewModel {
    title: String,
    activity: Vec<AgentActivityRow>,
    scroll: usize,
    viewport_rows: usize,
    auto_follow: bool,
    fork_index: Option<usize>,
    show_fork_tooltip: bool,
    selected: Option<usize>,
    focus: FocusArea,
    input_state: InputState,
    settings: Settings,
    task_entry: TaskEntryViewModel,
    theme: crate::theme::Theme,
    context_percent: u8,
    search_query: Option<String>,
    search_matches: Vec<usize>,
    search_cursor: Option<usize>,
    output_modal: Option<OutputModal>,
}

/// Mouse actions produced by hit testing in the Agent Activity view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentSessionMouseAction {
    /// Copy button on a specific card (by index in visible order, hero uses `usize::MAX`)
    Copy(usize),
    /// Expand button on a specific card (by index in visible order, hero uses `usize::MAX`)
    Expand(usize),
    /// Stop button on a running tool card
    Stop(usize),
    /// Fork tooltip click between cards (target index represents insert position)
    ForkHere(usize),
}

static ACTIVITY_NAV_MODE: crate::view_model::input::InputMinorMode =
    crate::view_model::input::InputMinorMode::new(&[
        KeyboardOperation::MoveToNextLine,
        KeyboardOperation::MoveToPreviousLine,
        KeyboardOperation::MoveToNextField,
        KeyboardOperation::MoveToPreviousField,
        KeyboardOperation::MoveToNextSnapshot,
        KeyboardOperation::MoveToPreviousSnapshot,
        KeyboardOperation::ScrollUpOneScreen,
        KeyboardOperation::ScrollDownOneScreen,
        KeyboardOperation::MoveToBeginningOfDocument,
        KeyboardOperation::MoveToEndOfDocument,
        KeyboardOperation::DismissOverlay,
        KeyboardOperation::ActivateCurrentItem,
    ]);

impl AgentSessionViewModel {
    pub fn new<S: Into<String>>(
        title: S,
        activity: Vec<AgentActivityRow>,
        viewport_rows: usize,
        settings: Settings,
        autocomplete: Option<
            std::sync::Arc<crate::view_model::autocomplete::AutocompleteDependencies>,
        >,
        theme: crate::theme::Theme,
    ) -> Self {
        let task_entry = build_task_entry(autocomplete, &theme);
        Self {
            title: title.into(),
            activity,
            scroll: 0,
            viewport_rows,
            auto_follow: true,
            fork_index: None,
            show_fork_tooltip: false,
            selected: None,
            focus: FocusArea::Timeline,
            input_state: InputState::default(),
            settings,
            task_entry,
            theme,
            context_percent: 45,
            search_query: None,
            search_matches: Vec::new(),
            search_cursor: None,
            output_modal: None,
        }
    }

    /// Append a new activity row, respecting auto-follow state.
    pub fn push_row(&mut self, row: AgentActivityRow) {
        self.activity.push(row);
        let last_idx = self.activity.len().saturating_sub(1);
        if self.auto_follow {
            self.scroll_to_end();
            if self.selected.is_some() {
                self.selected = Some(last_idx);
            }
        }
    }

    /// Process a raw TaskEvent (from ACP/SSE) and update the view model.
    pub fn handle_task_event(&mut self, event: &TaskEvent) -> bool {
        if process_activity_event(&mut self.activity, event, self.settings.activity_rows()) {
            if self.auto_follow {
                self.scroll_to_end();
                // If we are auto-following, keep the selection at the end if it was there
                if let Some(selected) = self.selected {
                    if selected == self.activity.len().saturating_sub(2) {
                        self.selected = Some(self.activity.len().saturating_sub(1));
                    }
                }
            }
            return true;
        }
        false
    }

    pub fn scroll_up(&mut self, delta: usize) {
        self.scroll = self.scroll.saturating_sub(delta);
    }

    pub fn scroll_down(&mut self, delta: usize) {
        let max_scroll = self.activity.len().saturating_sub(self.viewport_rows);
        self.scroll = (self.scroll + delta).min(max_scroll);
        if self.scroll == max_scroll {
            self.auto_follow = true;
        }
    }

    pub fn scroll_to_end(&mut self) {
        self.scroll = self.activity.len().saturating_sub(self.viewport_rows);
        self.auto_follow = true;
    }

    pub fn set_fork_index(&mut self, idx: Option<usize>) {
        if let Some(i) = idx {
            let max = self.activity.len().saturating_sub(1);
            self.fork_index = Some(i.min(max));
        } else {
            self.fork_index = None;
        }
    }

    pub fn set_fork_tooltip(&mut self, show: bool) {
        self.show_fork_tooltip = show;
    }

    /// Explicitly select a card, clamping into valid range.
    pub fn select_card(&mut self, idx: usize) {
        if self.activity.is_empty() {
            self.selected = None;
            return;
        }
        let max = self.activity.len().saturating_sub(1);
        self.selected = Some(idx.min(max));
    }

    /// Set focus area; used by tests to reach states without mutating fields directly.
    pub fn set_focus_area(&mut self, focus: FocusArea) {
        self.focus = focus;
    }

    /// Force auto-follow on and snap scroll to end (valid state).
    pub fn ensure_auto_follow(&mut self) {
        self.scroll_to_end();
    }

    pub fn visible_indices(&self) -> Vec<usize> {
        let end = (self.scroll + self.viewport_rows).min(self.activity.len());
        (self.scroll..end).collect()
    }

    pub fn visible_rows(&self) -> impl Iterator<Item = &AgentActivityRow> {
        self.visible_indices().into_iter().filter_map(|idx| self.activity.get(idx))
    }

    // --- Read-only accessors for view layer and tests ---
    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn activity(&self) -> &[AgentActivityRow] {
        &self.activity
    }

    pub fn context_percent(&self) -> u8 {
        self.context_percent
    }

    pub fn set_context_percent(&mut self, pct: u8) {
        self.context_percent = pct;
    }

    pub fn scroll(&self) -> usize {
        self.scroll
    }

    pub fn viewport_rows(&self) -> usize {
        self.viewport_rows
    }

    pub fn auto_follow(&self) -> bool {
        self.auto_follow
    }

    pub fn fork_index(&self) -> Option<usize> {
        self.fork_index
    }

    pub fn show_fork_tooltip(&self) -> bool {
        self.show_fork_tooltip
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub fn focus(&self) -> FocusArea {
        self.focus
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn theme(&self) -> &crate::theme::Theme {
        &self.theme
    }

    pub fn set_search_query<S: Into<String>>(&mut self, query: S) {
        self.search_query = Some(query.into());
        self.search_matches.clear();
        self.search_cursor = None;
    }

    pub fn search_matches(&self) -> &[usize] {
        &self.search_matches
    }

    pub fn output_modal(&self) -> Option<&OutputModal> {
        self.output_modal.as_ref()
    }

    pub fn open_output_modal<S1: Into<String>, S2: Into<String>>(
        &mut self,
        kind: OutputModalKind,
        title: S1,
        body: S2,
    ) {
        self.output_modal = Some(OutputModal {
            kind,
            title: title.into(),
            body: body.into(),
        });
    }

    pub fn close_modal(&mut self) {
        self.output_modal = None;
    }

    pub fn task_entry(&self) -> &TaskEntryViewModel {
        &self.task_entry
    }

    pub fn handle_mouse_action(
        &mut self,
        action: AgentSessionMouseAction,
    ) -> Option<AgentSessionMsg> {
        use AgentSessionMouseAction::*;
        match action {
            Copy(idx) => {
                self.selected = Some(idx);
                self.focus = FocusArea::Control(ControlFocus::Copy);
                Some(AgentSessionMsg::ActivateControl {
                    index: idx,
                    action: ControlAction::Copy,
                })
            }
            Expand(idx) => {
                self.selected = Some(idx);
                self.focus = FocusArea::Control(ControlFocus::Expand);
                Some(AgentSessionMsg::ActivateControl {
                    index: idx,
                    action: ControlAction::Expand,
                })
            }
            Stop(idx) => {
                self.selected = Some(idx);
                self.focus = FocusArea::Control(ControlFocus::Stop);
                Some(AgentSessionMsg::ActivateControl {
                    index: idx,
                    action: ControlAction::Stop,
                })
            }
            ForkHere(idx) => {
                self.set_fork_index(Some(idx));
                self.show_fork_tooltip = true;
                Some(AgentSessionMsg::ForkAt(idx))
            }
        }
    }

    fn move_selection_up(&mut self, delta: usize) {
        if self.activity.is_empty() {
            return;
        }
        let current = self.selected.unwrap_or_else(|| self.activity.len().saturating_sub(1));
        let new = current.saturating_sub(delta);
        self.selected = Some(new);
        if new < self.scroll {
            self.scroll = new;
        }
        self.auto_follow = false;
    }

    fn move_selection_down(&mut self, delta: usize) {
        if self.activity.is_empty() {
            return;
        }
        let max_idx = self.activity.len().saturating_sub(1);
        let current = self.selected.unwrap_or(0);
        let new = (current + delta).min(max_idx);
        self.selected = Some(new);
        let bottom = self.scroll + self.viewport_rows;
        if new >= bottom {
            self.scroll = new.saturating_sub(self.viewport_rows).saturating_add(1);
        }
        if new == max_idx {
            self.auto_follow = true;
        }
    }

    fn move_fork_index(&mut self, delta: i32) {
        let len = self.activity.len();
        if len == 0 {
            return;
        }
        let current = self.fork_index.unwrap_or(0);
        let step = delta.unsigned_abs() as usize;
        let new = if delta.is_negative() {
            current.saturating_sub(step.min(current))
        } else {
            current.saturating_add(step).min(len.saturating_sub(1))
        };
        self.fork_index = Some(new);
        self.show_fork_tooltip = true;
        self.auto_follow = false;
    }

    pub fn handle_key_with_minor_modes(
        &mut self,
        key: ratatui::crossterm::event::KeyEvent,
    ) -> Option<AgentSessionMsg> {
        // Resolve via minor modes
        if let Some(operation) = self.resolve_operation(key) {
            return self.handle_operation(operation, key);
        }
        None
    }

    fn resolve_operation(
        &mut self,
        key: ratatui::crossterm::event::KeyEvent,
    ) -> Option<KeyboardOperation> {
        self.input_state.update(&key);
        let processed = self.input_state.preprocess_key_event(key);
        let modes: Vec<&'static crate::view_model::input::InputMinorMode> = match self.focus {
            FocusArea::Instructions => vec![
                &minor_modes::TEXT_EDITING_PROMINENT_MODE,
                &minor_modes::TEXT_EDITING_MODE,
                &minor_modes::SEARCH_MODE,
                &ACTIVITY_NAV_MODE,
            ],
            FocusArea::Control(_) | FocusArea::Timeline => {
                vec![&minor_modes::SEARCH_MODE, &ACTIVITY_NAV_MODE]
            }
        };
        modes
            .into_iter()
            .find_map(|mode| mode.resolve_key_to_operation(&processed, &self.settings))
    }

    fn handle_operation(
        &mut self,
        op: KeyboardOperation,
        key: ratatui::crossterm::event::KeyEvent,
    ) -> Option<AgentSessionMsg> {
        match self.focus {
            FocusArea::Instructions => {
                let mut needs_redraw = false;
                let result = self.task_entry.handle_keyboard_operation(op, &key, &mut needs_redraw);
                match result {
                    crate::view_model::task_entry::KeyboardOperationResult::Handled => {
                        Some(AgentSessionMsg::RedrawRequested)
                    }
                    crate::view_model::task_entry::KeyboardOperationResult::NotHandled => {
                        // allow fallthrough for navigation ops
                        self.handle_operation_timeline(op)
                    }
                    crate::view_model::task_entry::KeyboardOperationResult::Bubble {
                        operation,
                    } => Some(AgentSessionMsg::TaskEntry(operation)),
                    crate::view_model::task_entry::KeyboardOperationResult::TaskLaunched {
                        ..
                    } => Some(AgentSessionMsg::TaskEntry(op)),
                }
            }
            _ => self.handle_operation_timeline(op),
        }
    }

    fn handle_operation_timeline(&mut self, op: KeyboardOperation) -> Option<AgentSessionMsg> {
        match op {
            KeyboardOperation::MoveToNextLine => {
                self.move_selection_down(1);
                Some(AgentSessionMsg::RedrawRequested)
            }
            KeyboardOperation::MoveToPreviousLine => {
                self.auto_follow = false;
                self.move_selection_up(1);
                Some(AgentSessionMsg::RedrawRequested)
            }
            KeyboardOperation::MoveToEndOfDocument => {
                self.scroll_to_end();
                if !self.activity.is_empty() {
                    self.selected = Some(self.activity.len() - 1);
                }
                Some(AgentSessionMsg::RedrawRequested)
            }
            KeyboardOperation::MoveToBeginningOfDocument => {
                self.scroll = 0;
                self.selected = Some(0);
                self.auto_follow = false;
                Some(AgentSessionMsg::RedrawRequested)
            }
            KeyboardOperation::ScrollDownOneScreen => {
                self.scroll_down(self.viewport_rows.max(1));
                self.move_selection_down(self.viewport_rows.max(1));
                Some(AgentSessionMsg::RedrawRequested)
            }
            KeyboardOperation::ScrollUpOneScreen => {
                self.auto_follow = false;
                self.scroll_up(self.viewport_rows.max(1));
                self.move_selection_up(self.viewport_rows.max(1));
                Some(AgentSessionMsg::RedrawRequested)
            }
            KeyboardOperation::MoveToNextField => {
                self.focus = match self.focus {
                    FocusArea::Timeline => FocusArea::Control(ControlFocus::Copy),
                    FocusArea::Control(ControlFocus::Copy) => {
                        FocusArea::Control(ControlFocus::Expand)
                    }
                    FocusArea::Control(ControlFocus::Expand) => FocusArea::Instructions,
                    FocusArea::Control(ControlFocus::Stop) => {
                        FocusArea::Control(ControlFocus::Copy)
                    }
                    FocusArea::Instructions => FocusArea::Timeline,
                };
                Some(AgentSessionMsg::RedrawRequested)
            }
            KeyboardOperation::MoveToPreviousField => {
                self.focus = match self.focus {
                    FocusArea::Timeline => FocusArea::Instructions,
                    FocusArea::Instructions => FocusArea::Control(ControlFocus::Expand),
                    FocusArea::Control(ControlFocus::Expand) => {
                        FocusArea::Control(ControlFocus::Copy)
                    }
                    FocusArea::Control(ControlFocus::Copy)
                    | FocusArea::Control(ControlFocus::Stop) => FocusArea::Timeline,
                };
                Some(AgentSessionMsg::RedrawRequested)
            }
            KeyboardOperation::MoveToNextSnapshot => {
                self.move_fork_index(1);
                Some(AgentSessionMsg::RedrawRequested)
            }
            KeyboardOperation::MoveToPreviousSnapshot => {
                self.move_fork_index(-1);
                Some(AgentSessionMsg::RedrawRequested)
            }
            KeyboardOperation::ActivateCurrentItem => match self.focus {
                FocusArea::Control(focus) => self.build_control_activation(focus),
                FocusArea::Timeline => {
                    self.focus = FocusArea::Control(ControlFocus::Copy);
                    Some(AgentSessionMsg::RedrawRequested)
                }
                FocusArea::Instructions => Some(AgentSessionMsg::TaskEntry(
                    KeyboardOperation::ActivateCurrentItem,
                )),
            },
            KeyboardOperation::DismissOverlay => {
                if self.output_modal.is_some() {
                    self.close_modal();
                    return Some(AgentSessionMsg::RedrawRequested);
                }
                if self.search_query.is_some() {
                    self.search_query = None;
                    self.search_matches.clear();
                    self.search_cursor = None;
                    return Some(AgentSessionMsg::RedrawRequested);
                }
                Some(AgentSessionMsg::QuitRequested)
            }
            KeyboardOperation::IncrementalSearchForward => {
                self.activate_search(SearchDirection::Forward);
                Some(AgentSessionMsg::RedrawRequested)
            }
            KeyboardOperation::IncrementalSearchBackward => {
                self.activate_search(SearchDirection::Backward);
                Some(AgentSessionMsg::RedrawRequested)
            }
            KeyboardOperation::FindNext => {
                self.advance_search(SearchDirection::Forward);
                Some(AgentSessionMsg::RedrawRequested)
            }
            KeyboardOperation::FindPrevious => {
                self.advance_search(SearchDirection::Backward);
                Some(AgentSessionMsg::RedrawRequested)
            }
            _ => None,
        }
    }

    fn build_control_activation(&self, focus: ControlFocus) -> Option<AgentSessionMsg> {
        self.selected.map(|idx| AgentSessionMsg::ActivateControl {
            index: idx,
            action: match focus {
                ControlFocus::Copy => ControlAction::Copy,
                ControlFocus::Expand => ControlAction::Expand,
                ControlFocus::Stop => ControlAction::Stop,
            },
        })
    }

    fn row_text(row: &AgentActivityRow) -> String {
        match row {
            AgentActivityRow::AgentThought { thought } => thought.clone(),
            AgentActivityRow::ToolUse {
                tool_name,
                last_line,
                ..
            } => format!("{tool_name} {}", last_line.as_deref().unwrap_or("")),
            AgentActivityRow::AgentEdit {
                file_path,
                description,
                ..
            } => format!("edit {file_path} {}", description.as_deref().unwrap_or("")),
            AgentActivityRow::AgentRead { file_path, range } => {
                format!("read {file_path} {}", range.as_deref().unwrap_or(""))
            }
            AgentActivityRow::AgentDeleted { file_path, .. } => format!("deleted {file_path}"),
            AgentActivityRow::UserInput {
                author, content, ..
            } => format!("{author}: {content}"),
        }
    }

    fn compute_matches(&self) -> Vec<usize> {
        let query = match self.search_query.as_ref() {
            Some(q) if !q.is_empty() => q.to_lowercase(),
            _ => return Vec::new(),
        };
        self.activity
            .iter()
            .enumerate()
            .filter_map(|(idx, row)| {
                let hay = Self::row_text(row).to_lowercase();
                if hay.contains(&query) {
                    Some(idx)
                } else {
                    None
                }
            })
            .collect()
    }

    fn activate_search(&mut self, direction: SearchDirection) {
        self.search_matches = self.compute_matches();
        if self.search_matches.is_empty() {
            self.search_cursor = None;
            return;
        }

        let start_idx = match direction {
            SearchDirection::Forward => 0,
            SearchDirection::Backward => self.search_matches.len().saturating_sub(1),
        };
        self.search_cursor = Some(start_idx);
        let idx = self.search_matches[start_idx];
        self.selected = Some(idx);
        self.scroll = idx.saturating_sub(self.viewport_rows / 2);
        self.auto_follow = false;
    }

    fn advance_search(&mut self, direction: SearchDirection) {
        if self.search_matches.is_empty() {
            self.search_matches = self.compute_matches();
        }
        if self.search_matches.is_empty() {
            self.search_cursor = None;
            return;
        }
        let len = self.search_matches.len();
        let current = self.search_cursor.unwrap_or(0);
        let next = match direction {
            SearchDirection::Forward => (current + 1) % len,
            SearchDirection::Backward => current.checked_sub(1).unwrap_or(len - 1),
        };
        self.search_cursor = Some(next);
        let idx = self.search_matches[next];
        self.selected = Some(idx);
        self.scroll = idx.saturating_sub(self.viewport_rows / 2);
        self.auto_follow = false;
    }
}

#[derive(Clone, Copy)]
enum SearchDirection {
    Forward,
    Backward,
}

fn build_task_entry(
    autocomplete: Option<std::sync::Arc<crate::view_model::autocomplete::AutocompleteDependencies>>,
    theme: &crate::theme::Theme,
) -> TaskEntryViewModel {
    use crate::view_model::session_viewer_model::SessionViewerViewModel;
    let deps = autocomplete.unwrap_or_else(crate::viewer::default_autocomplete_dependencies);
    SessionViewerViewModel::build_task_entry_view_model(&deps, "agent-activity", None, theme)
}
