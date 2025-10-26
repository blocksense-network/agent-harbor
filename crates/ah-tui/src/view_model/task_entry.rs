// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Task Entry ViewModel - for draft/editable task cards

use super::{ButtonViewModel, DraftSaveState, FocusElement};
use crate::settings::KeyboardOperation;
use ah_domain_types::SelectedModel;
use ratatui::crossterm::event::{KeyEvent, KeyModifiers};

#[derive(Debug, Clone, PartialEq)]
pub struct TaskEntryControlsViewModel {
    pub repository_button: ButtonViewModel,
    pub branch_button: ButtonViewModel,
    pub model_button: ButtonViewModel,
    pub go_button: ButtonViewModel,
}

/// ViewModel for draft/editable task entries
#[derive(Clone)] // Debug and PartialEq removed due to TextArea
pub struct TaskEntryViewModel {
    pub id: String,                 // Unique identifier for the task entry
    pub repository: String,         // Repository name
    pub branch: String,             // Branch name
    pub models: Vec<SelectedModel>, // Selected models
    pub created_at: String,         // Creation timestamp
    pub height: u16,
    pub controls: TaskEntryControlsViewModel,
    pub save_state: DraftSaveState,
    pub description: tui_textarea::TextArea<'static>, // TextArea stores content, cursor, and placeholder
    pub focus_element: FocusElement,                  // Current focus within this card
    pub auto_save_timer: Option<std::time::Instant>,  // Timer for auto-save functionality
}

impl TaskEntryViewModel {
    /// Handle a keyboard operation on the task entry's description textarea
    /// Returns true if the operation was handled
    pub fn handle_keyboard_operation<F>(
        &mut self,
        operation: KeyboardOperation,
        key: &KeyEvent,
        on_textarea_change: F,
    ) -> bool
    where
        F: FnOnce(&tui_textarea::TextArea, &mut bool),
    {
        // Only handle operations when focused on the task description
        if self.focus_element != FocusElement::TaskDescription {
            return false;
        }

        match operation {
            KeyboardOperation::MoveToBeginningOfLine => {
                // Home key: move cursor to beginning of line in text area
                use tui_textarea::CursorMove;
                let before = self.description.cursor();

                // Handle shift+home selection (CUA style)
                let shift_pressed = key.modifiers.contains(KeyModifiers::SHIFT);
                if shift_pressed {
                    // Start selection if not already active
                    if self.description.selection_range().is_none() {
                        self.description.start_selection();
                    }
                } else {
                    // Clear any existing selection when moving without shift
                    if self.description.selection_range().is_some() {
                        self.description.cancel_selection();
                    }
                }

                self.description.move_cursor(CursorMove::Head);
                if self.description.cursor() != before {
                    let mut needs_redraw = false;
                    on_textarea_change(&self.description, &mut needs_redraw);
                }
                true
            }
            KeyboardOperation::MoveToEndOfLine => {
                // End key: move cursor to end of line in text area
                use tui_textarea::CursorMove;
                let before = self.description.cursor();

                // Handle shift+end selection (CUA style)
                let shift_pressed = key.modifiers.contains(KeyModifiers::SHIFT);
                if shift_pressed {
                    // Start selection if not already active
                    if self.description.selection_range().is_none() {
                        self.description.start_selection();
                    }
                } else {
                    // Clear any existing selection when moving without shift
                    if self.description.selection_range().is_some() {
                        self.description.cancel_selection();
                    }
                }

                self.description.move_cursor(CursorMove::End);
                if self.description.cursor() != before {
                    let mut needs_redraw = false;
                    on_textarea_change(&self.description, &mut needs_redraw);
                }
                true
            }
            KeyboardOperation::MoveForwardOneCharacter => {
                // Right arrow: move cursor forward one character in text area
                use tui_textarea::CursorMove;
                let before = self.description.cursor();

                // Handle shift+arrow selection (CUA style)
                let shift_pressed = key.modifiers.contains(KeyModifiers::SHIFT);
                if shift_pressed {
                    // Start selection if not already active
                    if self.description.selection_range().is_none() {
                        self.description.start_selection();
                    }
                } else {
                    // Clear any existing selection when moving without shift
                    if self.description.selection_range().is_some() {
                        self.description.cancel_selection();
                    }
                }

                self.description.move_cursor(CursorMove::Forward);

                if self.description.cursor() != before {
                    let mut needs_redraw = false;
                    on_textarea_change(&self.description, &mut needs_redraw);
                }
                true
            }
            KeyboardOperation::MoveBackwardOneCharacter => {
                // Left arrow: move cursor backward one character in text area
                use tui_textarea::CursorMove;
                let before = self.description.cursor();

                // Handle shift+arrow selection (CUA style)
                let shift_pressed = key.modifiers.contains(KeyModifiers::SHIFT);
                if shift_pressed {
                    // Start selection if not already active
                    if self.description.selection_range().is_none() {
                        self.description.start_selection();
                    }
                } else {
                    // Clear any existing selection when moving without shift
                    if self.description.selection_range().is_some() {
                        self.description.cancel_selection();
                    }
                }

                self.description.move_cursor(CursorMove::Back);

                if self.description.cursor() != before {
                    let mut needs_redraw = false;
                    on_textarea_change(&self.description, &mut needs_redraw);
                }
                true
            }
            KeyboardOperation::MoveForwardOneWord => {
                // Ctrl+Right: move cursor forward one word in text area
                use tui_textarea::CursorMove;
                let before = self.description.cursor();

                // Handle shift+ctrl+arrow selection (CUA style)
                let shift_pressed = key.modifiers.contains(KeyModifiers::SHIFT);
                if shift_pressed {
                    // Start selection if not already active
                    if self.description.selection_range().is_none() {
                        self.description.start_selection();
                    }
                } else {
                    // Clear any existing selection when moving without shift
                    if self.description.selection_range().is_some() {
                        self.description.cancel_selection();
                    }
                }

                self.description.move_cursor(CursorMove::WordForward);

                if self.description.cursor() != before {
                    let mut needs_redraw = false;
                    on_textarea_change(&self.description, &mut needs_redraw);
                }
                true
            }
            KeyboardOperation::MoveBackwardOneWord => {
                // Ctrl+Left: move cursor backward one word in text area
                use tui_textarea::CursorMove;
                let before = self.description.cursor();

                // Handle shift+ctrl+arrow selection (CUA style)
                let shift_pressed = key.modifiers.contains(KeyModifiers::SHIFT);
                if shift_pressed {
                    // Start selection if not already active
                    if self.description.selection_range().is_none() {
                        self.description.start_selection();
                    }
                } else {
                    // Clear any existing selection when moving without shift
                    if self.description.selection_range().is_some() {
                        self.description.cancel_selection();
                    }
                }

                self.description.move_cursor(CursorMove::WordBack);

                if self.description.cursor() != before {
                    let mut needs_redraw = false;
                    on_textarea_change(&self.description, &mut needs_redraw);
                }
                true
            }
            KeyboardOperation::DeleteWordForward => {
                // Ctrl+Delete: delete word forward
                let before_text = self.description.lines().join("\\n");
                self.description.delete_next_word();
                let after_text = self.description.lines().join("\\n");
                if before_text != after_text {
                    let mut needs_redraw = false;
                    on_textarea_change(&self.description, &mut needs_redraw);
                }
                true
            }
            KeyboardOperation::DeleteWordBackward => {
                // Ctrl+Backspace: delete word backward
                let before_text = self.description.lines().join("\\n");
                self.description.delete_word();
                let after_text = self.description.lines().join("\\n");
                if before_text != after_text {
                    let mut needs_redraw = false;
                    on_textarea_change(&self.description, &mut needs_redraw);
                }
                true
            }
            KeyboardOperation::MoveToPreviousLine => {
                // Up arrow: move cursor up in the text area
                use tui_textarea::CursorMove;
                let old_cursor = self.description.cursor();

                // Handle shift+arrow selection (CUA style)
                let shift_pressed = key.modifiers.contains(KeyModifiers::SHIFT);
                if shift_pressed {
                    // Start selection if not already active
                    if self.description.selection_range().is_none() {
                        self.description.start_selection();
                    }
                } else {
                    // Clear any existing selection when moving without shift
                    if self.description.selection_range().is_some() {
                        self.description.cancel_selection();
                    }
                }

                self.description.move_cursor(CursorMove::Up);
                let new_cursor = self.description.cursor();
                if new_cursor != old_cursor {
                    // Cursor moved successfully within text area
                    let mut needs_redraw = false;
                    on_textarea_change(&self.description, &mut needs_redraw);
                    true
                } else {
                    false // Cursor couldn't move up, let caller handle navigation
                }
            }
            KeyboardOperation::MoveToNextLine => {
                // Down arrow: move cursor down in the text area
                use tui_textarea::CursorMove;
                let old_cursor = self.description.cursor();

                // Handle shift+arrow selection (CUA style)
                let shift_pressed = key.modifiers.contains(KeyModifiers::SHIFT);
                if shift_pressed {
                    // Start selection if not already active
                    if self.description.selection_range().is_none() {
                        self.description.start_selection();
                    }
                } else {
                    // Clear any existing selection when moving without shift
                    if self.description.selection_range().is_some() {
                        self.description.cancel_selection();
                    }
                }

                self.description.move_cursor(CursorMove::Down);
                let new_cursor = self.description.cursor();
                if new_cursor != old_cursor {
                    // Cursor moved successfully within text area
                    let mut needs_redraw = false;
                    on_textarea_change(&self.description, &mut needs_redraw);
                    true
                } else {
                    false // Cursor couldn't move down, let caller handle navigation
                }
            }
            KeyboardOperation::DeleteCharacterBackward => {
                // Backspace
                let before_text = self.description.lines().join("\\n");
                use ratatui::crossterm::event::{KeyCode, KeyEvent};
                let key_event = KeyEvent::new(KeyCode::Backspace, KeyModifiers::empty());
                self.description.input(key_event);
                let after_text = self.description.lines().join("\\n");
                if before_text != after_text {
                    let mut needs_redraw = false;
                    on_textarea_change(&self.description, &mut needs_redraw);
                }
                true
            }
            KeyboardOperation::DeleteCharacterForward => {
                // Delete key
                let before_text = self.description.lines().join("\\n");
                use ratatui::crossterm::event::{KeyCode, KeyEvent};
                let key_event = KeyEvent::new(KeyCode::Delete, KeyModifiers::empty());
                self.description.input(key_event);
                let after_text = self.description.lines().join("\\n");
                if before_text != after_text {
                    let mut needs_redraw = false;
                    on_textarea_change(&self.description, &mut needs_redraw);
                }
                true
            }
            KeyboardOperation::OpenNewLine => {
                // Shift+Enter: add newline to description
                use ratatui::crossterm::event::{KeyCode, KeyEvent};
                let key_event = KeyEvent::new(KeyCode::Enter, KeyModifiers::empty());
                self.description.input(key_event);
                let mut needs_redraw = false;
                on_textarea_change(&self.description, &mut needs_redraw);
                true
            }
            KeyboardOperation::Cut => {
                // Cut selected text
                let before_text = self.description.lines().join("\\n");
                self.description.cut();
                let after_text = self.description.lines().join("\\n");
                if before_text != after_text {
                    let mut needs_redraw = false;
                    on_textarea_change(&self.description, &mut needs_redraw);
                }
                true
            }
            KeyboardOperation::Copy => {
                // Copy selected text
                self.description.copy();
                true
            }
            KeyboardOperation::Paste => {
                // Paste from clipboard
                let before_text = self.description.lines().join("\\n");
                self.description.paste();
                let after_text = self.description.lines().join("\\n");
                if before_text != after_text {
                    let mut needs_redraw = false;
                    on_textarea_change(&self.description, &mut needs_redraw);
                }
                true
            }
            KeyboardOperation::Undo => {
                // Undo last operation
                let before_text = self.description.lines().join("\\n");
                self.description.undo();
                let after_text = self.description.lines().join("\\n");
                if before_text != after_text {
                    let mut needs_redraw = false;
                    on_textarea_change(&self.description, &mut needs_redraw);
                }
                true
            }
            KeyboardOperation::Redo => {
                // Redo last operation
                let before_text = self.description.lines().join("\\n");
                self.description.redo();
                let after_text = self.description.lines().join("\\n");
                if before_text != after_text {
                    let mut needs_redraw = false;
                    on_textarea_change(&self.description, &mut needs_redraw);
                }
                true
            }
            KeyboardOperation::DeleteToEndOfLine => {
                // Delete from cursor to end of line
                let before_text = self.description.lines().join("\\n");
                self.description.delete_line_by_end();
                let after_text = self.description.lines().join("\\n");
                if before_text != after_text {
                    let mut needs_redraw = false;
                    on_textarea_change(&self.description, &mut needs_redraw);
                }
                true
            }
            KeyboardOperation::DeleteToBeginningOfLine => {
                // Delete from cursor to beginning of line
                let before_text = self.description.lines().join("\\n");
                self.description.delete_line_by_head();
                let after_text = self.description.lines().join("\\n");
                if before_text != after_text {
                    let mut needs_redraw = false;
                    on_textarea_change(&self.description, &mut needs_redraw);
                }
                true
            }
            KeyboardOperation::SelectAll => {
                // Select all text
                self.description.select_all();
                let mut needs_redraw = false;
                on_textarea_change(&self.description, &mut needs_redraw);
                true
            }
            KeyboardOperation::MoveToBeginningOfSentence => {
                // Move to beginning of sentence (approximated as beginning of line)
                use tui_textarea::CursorMove;
                let before = self.description.cursor();

                // Handle shift+sentence selection (CUA style)
                let shift_pressed = key.modifiers.contains(KeyModifiers::SHIFT);
                if shift_pressed {
                    // Start selection if not already active
                    if self.description.selection_range().is_none() {
                        self.description.start_selection();
                    }
                } else {
                    // Clear any existing selection when moving without shift
                    if self.description.selection_range().is_some() {
                        self.description.cancel_selection();
                    }
                }

                self.description.move_cursor(CursorMove::Head);
                if self.description.cursor() != before {
                    let mut needs_redraw = false;
                    on_textarea_change(&self.description, &mut needs_redraw);
                }
                true
            }
            KeyboardOperation::MoveToEndOfSentence => {
                // Move to end of sentence (approximated as end of line)
                use tui_textarea::CursorMove;
                let before = self.description.cursor();

                // Handle shift+sentence selection (CUA style)
                let shift_pressed = key.modifiers.contains(KeyModifiers::SHIFT);
                if shift_pressed {
                    // Start selection if not already active
                    if self.description.selection_range().is_none() {
                        self.description.start_selection();
                    }
                } else {
                    // Clear any existing selection when moving without shift
                    if self.description.selection_range().is_some() {
                        self.description.cancel_selection();
                    }
                }

                self.description.move_cursor(CursorMove::End);
                if self.description.cursor() != before {
                    let mut needs_redraw = false;
                    on_textarea_change(&self.description, &mut needs_redraw);
                }
                true
            }
            KeyboardOperation::MoveToBeginningOfDocument => {
                // Move to beginning of document (first line, first character)
                use tui_textarea::CursorMove;
                let before = self.description.cursor();

                // Handle shift+document selection (CUA style)
                let shift_pressed = key.modifiers.contains(KeyModifiers::SHIFT);
                if shift_pressed {
                    // Start selection if not already active
                    if self.description.selection_range().is_none() {
                        self.description.start_selection();
                    }
                } else {
                    // Clear any existing selection when moving without shift
                    if self.description.selection_range().is_some() {
                        self.description.cancel_selection();
                    }
                }

                // Move to first line, then to beginning of that line
                let mut prev_cursor = self.description.cursor();
                loop {
                    self.description.move_cursor(CursorMove::Up);
                    let new_cursor = self.description.cursor();
                    if new_cursor == prev_cursor {
                        break; // Can't move further up
                    }
                    prev_cursor = new_cursor;
                }
                self.description.move_cursor(CursorMove::Head);

                if self.description.cursor() != before {
                    let mut needs_redraw = false;
                    on_textarea_change(&self.description, &mut needs_redraw);
                }
                true
            }
            KeyboardOperation::MoveToEndOfDocument => {
                // Move to end of document (last line, last character)
                use tui_textarea::CursorMove;
                let before = self.description.cursor();

                // Handle shift+document selection (CUA style)
                let shift_pressed = key.modifiers.contains(KeyModifiers::SHIFT);
                if shift_pressed {
                    // Start selection if not already active
                    if self.description.selection_range().is_none() {
                        self.description.start_selection();
                    }
                } else {
                    // Clear any existing selection when moving without shift
                    if self.description.selection_range().is_some() {
                        self.description.cancel_selection();
                    }
                }

                // Move to last line, then to end of that line
                let mut prev_cursor = self.description.cursor();
                loop {
                    self.description.move_cursor(CursorMove::Down);
                    let new_cursor = self.description.cursor();
                    if new_cursor == prev_cursor {
                        break; // Can't move further down
                    }
                    prev_cursor = new_cursor;
                }
                self.description.move_cursor(CursorMove::End);

                if self.description.cursor() != before {
                    let mut needs_redraw = false;
                    on_textarea_change(&self.description, &mut needs_redraw);
                }
                true
            }
            KeyboardOperation::MoveToBeginningOfParagraph => {
                // Move to beginning of paragraph (approximated as beginning of current line)
                use tui_textarea::CursorMove;
                let before = self.description.cursor();

                // Handle shift+paragraph selection (CUA style)
                let shift_pressed = key.modifiers.contains(KeyModifiers::SHIFT);
                if shift_pressed {
                    // Start selection if not already active
                    if self.description.selection_range().is_none() {
                        self.description.start_selection();
                    }
                } else {
                    // Clear any existing selection when moving without shift
                    if self.description.selection_range().is_some() {
                        self.description.cancel_selection();
                    }
                }

                self.description.move_cursor(CursorMove::Head);
                if self.description.cursor() != before {
                    let mut needs_redraw = false;
                    on_textarea_change(&self.description, &mut needs_redraw);
                }
                true
            }
            KeyboardOperation::MoveToEndOfParagraph => {
                // Move to end of paragraph (approximated as end of current line)
                use tui_textarea::CursorMove;
                let before = self.description.cursor();

                // Handle shift+paragraph selection (CUA style)
                let shift_pressed = key.modifiers.contains(KeyModifiers::SHIFT);
                if shift_pressed {
                    // Start selection if not already active
                    if self.description.selection_range().is_none() {
                        self.description.start_selection();
                    }
                } else {
                    // Clear any existing selection when moving without shift
                    if self.description.selection_range().is_some() {
                        self.description.cancel_selection();
                    }
                }

                self.description.move_cursor(CursorMove::End);
                if self.description.cursor() != before {
                    let mut needs_redraw = false;
                    on_textarea_change(&self.description, &mut needs_redraw);
                }
                true
            }
            KeyboardOperation::SelectWordUnderCursor => {
                // Select word under cursor
                // For now, just select all as a simple approximation
                // A more sophisticated implementation would find word boundaries
                self.description.select_all();
                let mut needs_redraw = false;
                on_textarea_change(&self.description, &mut needs_redraw);
                true
            }
            KeyboardOperation::SetMark => {
                // Set mark for selection (CUA style selection start)
                self.description.start_selection();
                true
            }
            KeyboardOperation::ScrollDownOneScreen => {
                // Scroll viewport down one screen (PageDown)
                use tui_textarea::Scrolling;
                self.description.scroll(Scrolling::PageDown);
                true
            }
            KeyboardOperation::ScrollUpOneScreen => {
                // Scroll viewport up one screen (PageUp)
                use tui_textarea::Scrolling;
                self.description.scroll(Scrolling::PageUp);
                true
            }
            KeyboardOperation::RecenterScreenOnCursor => {
                // Recenter cursor in middle of screen (Ctrl+L)
                // Get current cursor line and viewport height
                let cursor = self.description.cursor();
                let lines = self.description.lines();
                let viewport_height = self.description.viewport_origin().1 as usize; // Approximation

                // Calculate target top line to center cursor
                let target_top = cursor.0.saturating_sub(viewport_height / 2);

                // Scroll to center cursor
                self.description.scroll((target_top as i16, 0));
                true
            }
            KeyboardOperation::DuplicateLineSelection => {
                // Duplicate line/selection (Ctrl+D) - copy and paste below
                let cursor_row = self.description.cursor().0 as usize;
                let lines = self.description.lines();

                if cursor_row < lines.len() {
                    let current_line = lines[cursor_row].clone();

                    // Move to end of current line and insert newline + duplicated content
                    self.description.move_cursor(tui_textarea::CursorMove::End);
                    self.description.insert_char('\n');
                    self.description.insert_str(&current_line);
                }

                let mut needs_redraw = false;
                on_textarea_change(&self.description, &mut needs_redraw);
                true
            }
            KeyboardOperation::ToggleComment => {
                // Toggle comment (Ctrl+/) - add/remove comment markers from lines
                let lines = self.description.lines();
                let cursor_row = self.description.cursor().0 as usize;
                let cursor_col = self.description.cursor().1 as usize;

                // Determine lines to comment/uncomment
                let (_start_line, _end_line) =
                    if let Some(range) = self.description.selection_range() {
                        // Multi-line selection - range is ((start_row, start_col), (end_row, end_col))
                        (range.0.0, range.1.0)
                    } else {
                        // Single line at cursor
                        (cursor_row, cursor_row)
                    };

                // Use // as comment marker (could be made configurable)
                let comment_marker = "//";
                let mut lines_to_modify = Vec::new();

                // Check if we're adding or removing comments
                let should_add_comment = lines
                    .get(_start_line)
                    .map(|line: &String| !line.starts_with(comment_marker))
                    .unwrap_or(true);

                // Collect modified lines
                for i in _start_line..=_end_line {
                    if let Some(line) = lines.get(i) {
                        let modified_line = if should_add_comment {
                            format!("{}{}", comment_marker, line)
                        } else if line.starts_with(comment_marker) {
                            line.strip_prefix(comment_marker).unwrap_or(line).to_string()
                        } else {
                            line.clone()
                        };
                        lines_to_modify.push(modified_line);
                    }
                }

                // Replace the lines in textarea
                // This is a simplified approach - in practice you'd need to handle this more carefully
                // For now, we'll just implement a basic version
                let mut needs_redraw = false;
                on_textarea_change(&self.description, &mut needs_redraw);
                true // Placeholder - full implementation would modify textarea content
            }
            KeyboardOperation::MoveLineUp => {
                // Move line up (Alt+↑) - cut and reinsert above previous line
                let cursor_row = self.description.cursor().0 as usize;
                let lines = self.description.lines();

                // Can't move first line up
                if cursor_row == 0 {
                    return false;
                }

                // Select current line (simplified - would need proper line selection)
                self.description.move_cursor(tui_textarea::CursorMove::Head);
                self.description.start_selection();
                self.description.move_cursor(tui_textarea::CursorMove::End);
                // Note: This doesn't include the newline - simplified implementation

                // Cut the line
                self.description.cut();

                // Move cursor up to previous line
                self.description.move_cursor(tui_textarea::CursorMove::Up);
                self.description.move_cursor(tui_textarea::CursorMove::Head);

                // Paste above the current line
                self.description.paste();

                let mut needs_redraw = false;
                on_textarea_change(&self.description, &mut needs_redraw);
                true
            }
            KeyboardOperation::MoveLineDown => {
                // Move line down (Alt+↓) - cut and reinsert below next line
                let cursor_row = self.description.cursor().0 as usize;
                let lines = self.description.lines();

                // Can't move last line down
                if cursor_row >= lines.len().saturating_sub(1) {
                    return false;
                }

                // Select current line (simplified)
                self.description.move_cursor(tui_textarea::CursorMove::Head);
                self.description.start_selection();
                self.description.move_cursor(tui_textarea::CursorMove::End);

                // Cut the line
                self.description.cut();

                // Move cursor down to next line
                self.description.move_cursor(tui_textarea::CursorMove::Down);
                self.description.move_cursor(tui_textarea::CursorMove::End);

                // Insert newline and paste
                self.description.insert_newline();
                self.description.paste();

                let mut needs_redraw = false;
                on_textarea_change(&self.description, &mut needs_redraw);
                true
            }
            KeyboardOperation::IndentRegion => {
                // Indent region (Ctrl+]) - insert spaces at start of selected lines
                // Get selection range or current line
                let (_start_line, _end_line) =
                    if let Some(range) = self.description.selection_range() {
                        (range.0.0, range.1.0)
                    } else {
                        let cursor_row = self.description.cursor().0 as usize;
                        (cursor_row, cursor_row)
                    };

                // Insert 4 spaces (or tab) at start of each line
                // This is simplified - full implementation would need to modify textarea content directly
                let mut needs_redraw = false;
                on_textarea_change(&self.description, &mut needs_redraw);
                true // Placeholder
            }
            KeyboardOperation::DedentRegion => {
                // Dedent region (Ctrl+[) - remove spaces from start of selected lines
                // Get selection range or current line
                let (_start_line, _end_line) =
                    if let Some(range) = self.description.selection_range() {
                        (range.0.0, range.1.0)
                    } else {
                        let cursor_row = self.description.cursor().0 as usize;
                        (cursor_row, cursor_row)
                    };

                // Remove up to 4 spaces from start of each line
                // This is simplified - full implementation would need to modify textarea content directly
                let mut needs_redraw = false;
                on_textarea_change(&self.description, &mut needs_redraw);
                true // Placeholder
            }
            KeyboardOperation::UppercaseWord => {
                // Uppercase word (Alt+U) - transform word at/after cursor to uppercase
                // Get current line and cursor position
                let lines = self.description.lines();
                let (cursor_row, cursor_col) = self.description.cursor();

                if cursor_row < lines.len() {
                    let current_line = &lines[cursor_row];
                    let chars: Vec<char> = current_line.chars().collect();

                    if cursor_col < chars.len() {
                        // Find word boundaries around cursor
                        let mut word_start = cursor_col;
                        let mut word_end = cursor_col;

                        // Find start of word (move left until non-alphanumeric)
                        while word_start > 0 && chars[word_start - 1].is_alphanumeric() {
                            word_start -= 1;
                        }

                        // Find end of word (move right until non-alphanumeric)
                        while word_end < chars.len() && chars[word_end].is_alphanumeric() {
                            word_end += 1;
                        }

                        if word_start < word_end {
                            // Extract and uppercase the word
                            let word: String = chars[word_start..word_end].iter().collect();
                            let uppercased = word.to_uppercase();

                            // Replace the word in the line
                            let mut new_line = String::new();
                            new_line.extend(&chars[0..word_start]);
                            new_line.push_str(&uppercased);
                            new_line.extend(&chars[word_end..]);

                            // Replace the entire line
                            let mut all_lines: Vec<String> =
                                lines.into_iter().map(|s| s.clone()).collect();
                            all_lines[cursor_row] = new_line;
                            self.description = tui_textarea::TextArea::new(all_lines);

                            // Restore cursor position (after the uppercased word)
                            let new_cursor_col = word_start + uppercased.chars().count();
                            self.description.move_cursor(tui_textarea::CursorMove::Jump(
                                cursor_row as u16,
                                new_cursor_col as u16,
                            ));
                        }
                    }
                }

                let mut needs_redraw = false;
                on_textarea_change(&self.description, &mut needs_redraw);
                true
            }
            KeyboardOperation::LowercaseWord => {
                // Lowercase word (Alt+L) - transform word at/after cursor to lowercase
                // Get current line and cursor position
                let lines = self.description.lines();
                let (cursor_row, cursor_col) = self.description.cursor();

                if cursor_row < lines.len() {
                    let current_line = &lines[cursor_row];
                    let chars: Vec<char> = current_line.chars().collect();

                    if cursor_col < chars.len() {
                        // Find word boundaries around cursor
                        let mut word_start = cursor_col;
                        let mut word_end = cursor_col;

                        // Find start of word (move left until non-alphanumeric)
                        while word_start > 0 && chars[word_start - 1].is_alphanumeric() {
                            word_start -= 1;
                        }

                        // Find end of word (move right until non-alphanumeric)
                        while word_end < chars.len() && chars[word_end].is_alphanumeric() {
                            word_end += 1;
                        }

                        if word_start < word_end {
                            // Extract and lowercase the word
                            let word: String = chars[word_start..word_end].iter().collect();
                            let lowercased = word.to_lowercase();

                            // Replace the word in the line
                            let mut new_line = String::new();
                            new_line.extend(&chars[0..word_start]);
                            new_line.push_str(&lowercased);
                            new_line.extend(&chars[word_end..]);

                            // Replace the entire line
                            let mut all_lines: Vec<String> =
                                lines.into_iter().map(|s| s.clone()).collect();
                            all_lines[cursor_row] = new_line;
                            self.description = tui_textarea::TextArea::new(all_lines);

                            // Restore cursor position (after the lowercased word)
                            let new_cursor_col = word_start + lowercased.chars().count();
                            self.description.move_cursor(tui_textarea::CursorMove::Jump(
                                cursor_row as u16,
                                new_cursor_col as u16,
                            ));
                        }
                    }
                }

                let mut needs_redraw = false;
                on_textarea_change(&self.description, &mut needs_redraw);
                true
            }
            KeyboardOperation::CapitalizeWord => {
                // Capitalize word (Alt+C) - capitalize word at/after cursor
                if self.description.selection_range().is_some() {
                    // Select word at/after cursor
                    self.description.start_selection();
                    self.description.move_cursor(tui_textarea::CursorMove::WordForward);
                    self.description.copy();

                    // Get the copied word and capitalize it
                    let word = self.description.yank_text();
                    if !word.is_empty() {
                        let capitalized = word
                            .chars()
                            .enumerate()
                            .map(|(i, c)| {
                                if i == 0 {
                                    c.to_uppercase().to_string()
                                } else {
                                    c.to_lowercase().to_string()
                                }
                            })
                            .collect::<String>();
                        self.description.set_yank_text(capitalized);

                        // Replace the selection
                        self.description.paste();

                        let mut needs_redraw = false;
                        on_textarea_change(&self.description, &mut needs_redraw);
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            KeyboardOperation::JoinLines => {
                // Join lines (Alt+^) - delete newline between lines
                // Move cursor to end of line and delete newline
                self.description.move_cursor(tui_textarea::CursorMove::End);
                self.description.delete_next_char(); // This should delete the newline

                let mut needs_redraw = false;
                on_textarea_change(&self.description, &mut needs_redraw);
                true
            }
            KeyboardOperation::Bold => {
                // Bold (Ctrl+B) - wrap selection or next word with **
                if self.description.selection_range().is_some() {
                    // Copy selection to yank buffer
                    self.description.copy();
                    let selected_text = self.description.yank_text();
                    if !selected_text.is_empty() {
                        // Replace selection with wrapped text
                        self.description.insert_str(&format!("**{}**", selected_text));
                    }
                } else {
                    // Insert ** and position cursor between them
                    self.description.insert_char('*');
                    self.description.insert_char('*');
                    self.description.insert_char('*');
                    self.description.insert_char('*');
                    self.description.move_cursor(tui_textarea::CursorMove::Back);
                    self.description.move_cursor(tui_textarea::CursorMove::Back);
                }

                let mut needs_redraw = false;
                on_textarea_change(&self.description, &mut needs_redraw);
                true
            }
            KeyboardOperation::Italic => {
                // Italic (Ctrl+I) - wrap selection or next word with *
                if self.description.selection_range().is_some() {
                    // Copy selection to yank buffer
                    self.description.copy();
                    let selected_text = self.description.yank_text();
                    if !selected_text.is_empty() {
                        // Replace selection with wrapped text
                        self.description.insert_str(&format!("*{}*", selected_text));
                    }
                } else {
                    // Insert ** and position cursor between them
                    self.description.insert_str("**");
                    self.description.move_cursor(tui_textarea::CursorMove::Back);
                }

                let mut needs_redraw = false;
                on_textarea_change(&self.description, &mut needs_redraw);
                true
            }
            KeyboardOperation::Underline => {
                // Underline (Ctrl+U) - wrap selection with <u> tags
                if self.description.selection_range().is_some() {
                    // Copy selection to yank buffer
                    self.description.copy();
                    let selected_text = self.description.yank_text();
                    if !selected_text.is_empty() {
                        // Replace selection with wrapped text
                        self.description.insert_str(&format!("<u>{}</u>", selected_text));
                    }
                } else {
                    // Insert tags and position cursor
                    self.description.insert_str("<u></u>");
                    self.description.move_cursor(tui_textarea::CursorMove::Back);
                    self.description.move_cursor(tui_textarea::CursorMove::Back);
                    self.description.move_cursor(tui_textarea::CursorMove::Back);
                    self.description.move_cursor(tui_textarea::CursorMove::Back);
                }

                let mut needs_redraw = false;
                on_textarea_change(&self.description, &mut needs_redraw);
                true
            }
            KeyboardOperation::CycleThroughClipboard => {
                // Cycle through clipboard (Alt+Y) - cycle through yank ring
                // This would require implementing a yank ring - simplified for now
                let mut needs_redraw = false;
                on_textarea_change(&self.description, &mut needs_redraw);
                true // Placeholder
            }
            KeyboardOperation::TransposeCharacters => {
                // Transpose characters (Ctrl+T) - swap character before cursor with character after
                self.description.move_cursor(tui_textarea::CursorMove::Back);
                self.description.delete_next_char();
                self.description.move_cursor(tui_textarea::CursorMove::Forward);
                // Full implementation would need to save characters and swap them
                let mut needs_redraw = false;
                on_textarea_change(&self.description, &mut needs_redraw);
                true // Placeholder
            }
            KeyboardOperation::TransposeWords => {
                // Transpose words (Alt+T) - swap word before cursor with word after
                // Simplified implementation - would need complex word boundary detection
                let mut needs_redraw = false;
                on_textarea_change(&self.description, &mut needs_redraw);
                true // Placeholder
            }
            KeyboardOperation::IncrementalSearchForward => {
                // Incremental search forward (Ctrl+S) - start search mode
                // Set search pattern (would need search dialog/input in real implementation)
                let _ = self.description.set_search_pattern("search_term".to_string());
                let _ = self.description.search_forward(false);
                let mut needs_redraw = false;
                on_textarea_change(&self.description, &mut needs_redraw);
                true
            }
            KeyboardOperation::IncrementalSearchBackward => {
                // Incremental search backward (Ctrl+R) - start reverse search mode
                // Set search pattern and search backward
                let _ = self.description.set_search_pattern("search_term".to_string());
                let _ = self.description.search_back(false);
                let mut needs_redraw = false;
                on_textarea_change(&self.description, &mut needs_redraw);
                true
            }
            KeyboardOperation::FindNext => {
                // Find next (F3) - jump to next search match
                let _ = self.description.search_forward(false);
                let mut needs_redraw = false;
                on_textarea_change(&self.description, &mut needs_redraw);
                true
            }
            KeyboardOperation::FindPrevious => {
                // Find previous (Shift+F3) - jump to previous search match
                let _ = self.description.search_back(false);
                let mut needs_redraw = false;
                on_textarea_change(&self.description, &mut needs_redraw);
                true
            }
            // Operations that don't apply to text editing
            _ => false,
        }
    }
}
