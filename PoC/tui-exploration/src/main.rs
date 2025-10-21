//! Main Application - MVVM Orchestration and TUI Runner
//!
//! This is the application entry point that orchestrates the MVVM components
//! and handles the terminal user interface lifecycle. It coordinates between
//! the Model, ViewModel, and View layers while managing the terminal state.
//!
//! ## Responsibilities:
//!
//! ✅ **Application Lifecycle**: Setup, event loop, cleanup, error handling
//! ✅ **Terminal Management**: Raw mode, alternate screen, event polling
//! ✅ **MVVM Coordination**: Routes messages between Model ↔ ViewModel ↔ View
//! ✅ **Event Processing**: Keyboard, mouse, resize, and timer events
//! ✅ **State Management**: Application state transitions and persistence
//!
//! ## Architecture Role:
//!
//! The main module acts as the "glue" that connects the MVVM layers:
//! 1. **Receives UI events** from the terminal
//! 2. **Routes to ViewModel** for UI-specific processing
//! 3. **Forwards domain messages** to the Model for business logic
//! 4. **Updates ViewModel** with new domain state
//! 5. **Renders View** with current ViewModel state
//!
//! ## Message Flow:
//!
//! ```text
//! Terminal Event → Main → ViewModel → DomainMsg → Model → ViewModel Update → View → Terminal
//! ```
//!
//! ## Design Principles:
//!
//! - **Thin Orchestration Layer**: Minimal logic, mostly routing and coordination
//! - **Error Handling**: Comprehensive error handling and graceful shutdowns
//! - **Performance**: Efficient event polling and rendering loops
//! - **Cross-Platform**: Works across different terminal environments

use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Position, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph, Wrap},
};
mod autocomplete;
mod shortcuts;
use arboard::Clipboard;
use crossbeam_channel as chan;
use crossterm::{
    event::{
        DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers,
        KeyboardEnhancementFlags, MouseButton, MouseEvent, MouseEventKind,
        PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    queue,
};
use ctrlc;
use image::{DynamicImage, GenericImageView, ImageReader, Rgba, RgbaImage};
use rand::seq::SliceRandom;
use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;
use regex::Regex;
use std::cell::Cell;
use std::collections::{BTreeSet, VecDeque};
use std::fmt;
use std::fs::OpenOptions;
use std::io;
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};
use tui_input::{Input, InputRequest};
use tui_textarea::TextArea;
use unicode_width::UnicodeWidthStr;

use crate::autocomplete::{AutocompleteKeyResult, InlineAutocomplete};
use crate::shortcuts::{
    InMemoryShortcutConfig, SHORTCUT_LAUNCH_TASK, SHORTCUT_NEW_LINE, SHORTCUT_NEXT_FIELD,
    SHORTCUT_OPEN_SETTINGS, SHORTCUT_PREV_FIELD, SHORTCUT_SHORTCUT_HELP, ShortcutConfigProvider,
    ShortcutDisplay,
};

// Comprehensive Command enum for all TUI keyboard shortcuts
#[derive(Debug, Clone, Copy, PartialEq)]
enum Command {
    // Cursor Movement
    MoveToBeginningOfLine,
    MoveToEndOfLine,
    MoveForwardOneCharacter,
    MoveBackwardOneCharacter,
    MoveToNextLine,
    MoveToPreviousLine,
    MoveForwardOneWord,
    MoveBackwardOneWord,
    MoveToBeginningOfSentence,
    MoveToEndOfSentence,
    ScrollDownOneScreen,
    ScrollUpOneScreen,
    RecenterScreenOnCursor,
    MoveToBeginningOfDocument,
    MoveToEndOfDocument,
    MoveToBeginningOfParagraph,
    MoveToEndOfParagraph,
    GoToLineNumber,
    MoveToMatchingParenthesis,

    // Editing and Deletion
    SelectAll,
    DeleteCharacterForward,
    DeleteCharacterBackward,
    DeleteWordForward,
    DeleteWordBackward,
    DeleteToEndOfLine,
    Cut,
    Copy,
    Paste,
    CycleThroughClipboard,
    TransposeCharacters,
    TransposeWords,
    Undo,
    Redo,
    OpenNewLine,
    IndentOrComplete,
    DeleteToBeginningOfLine,

    // Text Transformation
    UppercaseWord,
    LowercaseWord,
    CapitalizeWord,
    FillParagraph,
    JoinLines,

    // Formatting (Markdown Style)
    Bold,
    Italic,
    Underline,
    InsertHyperlink,

    // Code Editing
    ToggleComment,
    DuplicateLineSelection,
    MoveLineUp,
    MoveLineDown,
    IndentRegion,
    DedentRegion,

    // Search and Replace
    IncrementalSearchForward,
    IncrementalSearchBackward,
    FindAndReplace,
    FindAndReplaceWithRegex,
    FindNext,
    FindPrevious,

    // Mark and Region
    SetMark,
    ExtendSelection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommandAction {
    OpenGoToLineDialog,
    OpenFindReplace { is_regex: bool },
}

#[derive(Debug, Clone, Default)]
struct CommandEffect {
    text_changed: bool,
    caret_moved: bool,
    action: Option<CommandAction>,
    status_message: Option<String>,
}

#[derive(Debug)]
struct KillRing {
    entries: VecDeque<String>,
    max_entries: usize,
    current: Option<usize>,
}

impl KillRing {
    fn new(max_entries: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(max_entries),
            max_entries,
            current: None,
        }
    }

    fn push(&mut self, text: String) {
        if text.trim().is_empty() {
            return;
        }

        if self.entries.front().map(|existing| existing == &text).unwrap_or(false) {
            self.current = Some(0);
            return;
        }

        self.entries.push_front(text);
        if self.entries.len() > self.max_entries {
            self.entries.pop_back();
        }
        self.current = Some(0);
    }

    fn cycle_next(&mut self) -> Option<String> {
        if self.entries.is_empty() {
            return None;
        }

        let len = self.entries.len();
        let next_index = match self.current {
            Some(current) => (current + 1) % len,
            None => 0,
        };
        self.current = Some(next_index);
        self.entries.get(next_index).cloned()
    }
}

fn cycle_index(current: usize, len: usize, direction: i32) -> usize {
    if len == 0 {
        return 0;
    }
    let len_i32 = len as i32;
    let mut next = current as i32 + direction;
    if next < 0 {
        next = len_i32 - 1;
    } else if next >= len_i32 {
        next = 0;
    }
    next as usize
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CaretMetrics {
    caret_x: u16,
    caret_y: u16,
    popup_x: u16,
    popup_y: u16,
}

pub(crate) fn compute_caret_metrics(textarea: &TextArea<'_>, area: Rect) -> CaretMetrics {
    let (cursor_row, cursor_col) = textarea.cursor();
    let (top_row, left_col) = textarea.viewport_origin();
    let gutter = textarea.gutter_width();

    let mut visible_row = cursor_row.saturating_sub(top_row as usize);
    let mut visible_col = cursor_col.saturating_sub(left_col as usize);

    if textarea.word_wrap() {
        let available_width = area.width.saturating_sub(gutter);
        let content_width = available_width.max(1) as usize;

        if content_width > 0 {
            let start_row = top_row as usize;
            let mut additional_rows = 0usize;

            for line_idx in start_row..cursor_row {
                let width = textarea.display_width_of_line(line_idx);
                if width > 0 {
                    let wraps = (width + content_width - 1) / content_width;
                    if wraps > 0 {
                        additional_rows = additional_rows.saturating_add(wraps.saturating_sub(1));
                    }
                }
            }

            let cursor_width = textarea.display_width_until(cursor_row, cursor_col);
            let wraps = cursor_width / content_width;
            visible_col = cursor_width % content_width;
            if cursor_width > 0 && visible_col == 0 {
                visible_col = 0;
            }
            additional_rows = additional_rows.saturating_add(wraps);
            visible_row = cursor_row.saturating_sub(start_row) + additional_rows;
        } else {
            visible_col = 0;
        }
    }

    let text_start_x = area.x.saturating_add(gutter as u16);
    let max_x = area.x.saturating_add(area.width.saturating_sub(1));
    let max_y = area.y.saturating_add(area.height.saturating_sub(1));

    let caret_x = text_start_x.saturating_add(visible_col as u16).min(max_x);
    let caret_y = area.y.saturating_add(visible_row as u16).min(max_y);

    let popup_x = caret_x.saturating_add(1).min(max_x);
    let popup_y = caret_y.saturating_add(1).min(max_y);

    CaretMetrics {
        caret_x,
        caret_y,
        popup_x,
        popup_y,
    }
}

const COMMAND_SHORTCUTS: &[(Command, &str)] = &[
    // Selection and clipboard operations first to resolve conflicts
    (Command::SelectAll, crate::shortcuts::SHORTCUT_SELECT_ALL),
    (Command::Cut, crate::shortcuts::SHORTCUT_CUT),
    (Command::Copy, crate::shortcuts::SHORTCUT_COPY),
    (Command::Paste, crate::shortcuts::SHORTCUT_PASTE),
    (
        Command::CycleThroughClipboard,
        crate::shortcuts::SHORTCUT_CYCLE_CLIPBOARD,
    ),
    (Command::Undo, crate::shortcuts::SHORTCUT_UNDO),
    (Command::Redo, crate::shortcuts::SHORTCUT_REDO),
    (
        Command::DeleteToEndOfLine,
        crate::shortcuts::SHORTCUT_DELETE_TO_END_OF_LINE,
    ),
    (
        Command::DeleteToBeginningOfLine,
        crate::shortcuts::SHORTCUT_DELETE_TO_BEGINNING_OF_LINE,
    ),
    (
        Command::DeleteWordForward,
        crate::shortcuts::SHORTCUT_DELETE_WORD_FORWARD,
    ),
    (
        Command::DeleteWordBackward,
        crate::shortcuts::SHORTCUT_DELETE_WORD_BACKWARD,
    ),
    (
        Command::DeleteCharacterForward,
        crate::shortcuts::SHORTCUT_DELETE_CHARACTER_FORWARD,
    ),
    (
        Command::DeleteCharacterBackward,
        crate::shortcuts::SHORTCUT_DELETE_CHARACTER_BACKWARD,
    ),
    // Navigation
    (
        Command::MoveToBeginningOfLine,
        crate::shortcuts::SHORTCUT_MOVE_TO_BEGINNING_OF_LINE,
    ),
    (
        Command::MoveToEndOfLine,
        crate::shortcuts::SHORTCUT_MOVE_TO_END_OF_LINE,
    ),
    (
        Command::MoveForwardOneCharacter,
        crate::shortcuts::SHORTCUT_MOVE_FORWARD_ONE_CHARACTER,
    ),
    (
        Command::MoveBackwardOneCharacter,
        crate::shortcuts::SHORTCUT_MOVE_BACKWARD_ONE_CHARACTER,
    ),
    (
        Command::MoveToNextLine,
        crate::shortcuts::SHORTCUT_MOVE_TO_NEXT_LINE,
    ),
    (
        Command::MoveToPreviousLine,
        crate::shortcuts::SHORTCUT_MOVE_TO_PREVIOUS_LINE,
    ),
    (
        Command::MoveForwardOneWord,
        crate::shortcuts::SHORTCUT_MOVE_FORWARD_ONE_WORD,
    ),
    (
        Command::MoveBackwardOneWord,
        crate::shortcuts::SHORTCUT_MOVE_BACKWARD_ONE_WORD,
    ),
    (
        Command::MoveToBeginningOfSentence,
        crate::shortcuts::SHORTCUT_MOVE_TO_BEGINNING_OF_SENTENCE,
    ),
    (
        Command::MoveToEndOfSentence,
        crate::shortcuts::SHORTCUT_MOVE_TO_END_OF_SENTENCE,
    ),
    (
        Command::ScrollDownOneScreen,
        crate::shortcuts::SHORTCUT_SCROLL_DOWN_ONE_SCREEN,
    ),
    (
        Command::ScrollUpOneScreen,
        crate::shortcuts::SHORTCUT_SCROLL_UP_ONE_SCREEN,
    ),
    (
        Command::RecenterScreenOnCursor,
        crate::shortcuts::SHORTCUT_RECENTER_SCREEN,
    ),
    (
        Command::MoveToBeginningOfDocument,
        crate::shortcuts::SHORTCUT_MOVE_TO_BEGINNING_OF_DOCUMENT,
    ),
    (
        Command::MoveToEndOfDocument,
        crate::shortcuts::SHORTCUT_MOVE_TO_END_OF_DOCUMENT,
    ),
    (
        Command::MoveToBeginningOfParagraph,
        crate::shortcuts::SHORTCUT_MOVE_TO_BEGINNING_OF_PARAGRAPH,
    ),
    (
        Command::MoveToEndOfParagraph,
        crate::shortcuts::SHORTCUT_MOVE_TO_END_OF_PARAGRAPH,
    ),
    (
        Command::GoToLineNumber,
        crate::shortcuts::SHORTCUT_GO_TO_LINE_NUMBER,
    ),
    (
        Command::MoveToMatchingParenthesis,
        crate::shortcuts::SHORTCUT_MOVE_TO_MATCHING_PAREN,
    ),
    // Insert / transform
    (
        Command::OpenNewLine,
        crate::shortcuts::SHORTCUT_OPEN_NEW_LINE,
    ),
    (
        Command::IndentOrComplete,
        crate::shortcuts::SHORTCUT_INDENT_OR_COMPLETE,
    ),
    (
        Command::TransposeCharacters,
        crate::shortcuts::SHORTCUT_TRANSPOSE_CHARACTERS,
    ),
    (
        Command::TransposeWords,
        crate::shortcuts::SHORTCUT_TRANSPOSE_WORDS,
    ),
    (
        Command::UppercaseWord,
        crate::shortcuts::SHORTCUT_UPPERCASE_WORD,
    ),
    (
        Command::LowercaseWord,
        crate::shortcuts::SHORTCUT_LOWERCASE_WORD,
    ),
    (
        Command::CapitalizeWord,
        crate::shortcuts::SHORTCUT_CAPITALIZE_WORD,
    ),
    (
        Command::FillParagraph,
        crate::shortcuts::SHORTCUT_FILL_PARAGRAPH,
    ),
    (Command::JoinLines, crate::shortcuts::SHORTCUT_JOIN_LINES),
    // Formatting
    (Command::Bold, crate::shortcuts::SHORTCUT_BOLD),
    (Command::Italic, crate::shortcuts::SHORTCUT_ITALIC),
    (Command::Underline, crate::shortcuts::SHORTCUT_UNDERLINE),
    (
        Command::InsertHyperlink,
        crate::shortcuts::SHORTCUT_INSERT_HYPERLINK,
    ),
    // Code editing
    (
        Command::ToggleComment,
        crate::shortcuts::SHORTCUT_TOGGLE_COMMENT,
    ),
    (
        Command::DuplicateLineSelection,
        crate::shortcuts::SHORTCUT_DUPLICATE_LINE,
    ),
    (Command::MoveLineUp, crate::shortcuts::SHORTCUT_MOVE_LINE_UP),
    (
        Command::MoveLineDown,
        crate::shortcuts::SHORTCUT_MOVE_LINE_DOWN,
    ),
    (
        Command::IndentRegion,
        crate::shortcuts::SHORTCUT_INDENT_REGION,
    ),
    (
        Command::DedentRegion,
        crate::shortcuts::SHORTCUT_DEDENT_REGION,
    ),
    // Search
    (
        Command::IncrementalSearchForward,
        crate::shortcuts::SHORTCUT_INCREMENTAL_SEARCH_FORWARD,
    ),
    (
        Command::IncrementalSearchBackward,
        crate::shortcuts::SHORTCUT_INCREMENTAL_SEARCH_BACKWARD,
    ),
    (
        Command::FindAndReplace,
        crate::shortcuts::SHORTCUT_FIND_AND_REPLACE,
    ),
    (
        Command::FindAndReplaceWithRegex,
        crate::shortcuts::SHORTCUT_FIND_AND_REPLACE_REGEX,
    ),
    (Command::FindNext, crate::shortcuts::SHORTCUT_FIND_NEXT),
    (
        Command::FindPrevious,
        crate::shortcuts::SHORTCUT_FIND_PREVIOUS,
    ),
    // Mark & region
    (Command::SetMark, crate::shortcuts::SHORTCUT_SET_MARK),
];
struct SystemClipboard {
    inner: Option<Clipboard>,
    last_error: Option<String>,
}

impl SystemClipboard {
    fn new() -> Self {
        Self {
            inner: Clipboard::new().ok(),
            last_error: None,
        }
    }

    fn set_text(&mut self, text: &str) -> Result<(), String> {
        let result = self
            .inner
            .as_mut()
            .ok_or_else(|| "System clipboard unavailable".to_string())
            .and_then(|cb| cb.set_text(text.to_string()).map_err(|err| err.to_string()));

        match result {
            Ok(_) => {
                self.last_error = None;
                Ok(())
            }
            Err(err) => {
                self.last_error = Some(err.clone());
                Err(err)
            }
        }
    }

    fn get_text(&mut self) -> Result<String, String> {
        let result: Result<String, String> = self
            .inner
            .as_mut()
            .ok_or_else(|| "System clipboard unavailable".to_string())
            .and_then(|cb| cb.get_text().map_err(|err| err.to_string()));

        match result {
            Ok(text) => {
                self.last_error = None;
                Ok(text)
            }
            Err(err) => {
                self.last_error = Some(err.clone());
                Err(err)
            }
        }
    }

    fn last_error(&self) -> Option<&String> {
        self.last_error.as_ref()
    }
}

impl fmt::Debug for SystemClipboard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SystemClipboard")
            .field("available", &self.inner.is_some())
            .field("last_error", &self.last_error)
            .finish()
    }
}

fn flatten_text_with_positions(lines: &[String]) -> (Vec<char>, Vec<(usize, usize)>) {
    let mut chars = Vec::new();
    let mut positions = Vec::new();

    for (row, line) in lines.iter().enumerate() {
        let line_chars: Vec<char> = line.chars().collect();
        for (col, ch) in line_chars.iter().enumerate() {
            chars.push(*ch);
            positions.push((row, col));
        }
        if row + 1 < lines.len() {
            chars.push('\n');
            positions.push((row, line_chars.len()));
        }
    }

    (chars, positions)
}

fn cursor_index_from_positions(positions: &[(usize, usize)], cursor: (usize, usize)) -> usize {
    if positions.is_empty() {
        return 0;
    }

    let (target_row, target_col) = cursor;
    for (idx, &(row, col)) in positions.iter().enumerate() {
        if row > target_row || (row == target_row && col >= target_col) {
            return idx;
        }
    }

    positions.len() - 1
}

fn is_sentence_terminator(ch: char) -> bool {
    matches!(ch, '.' | '!' | '?' | '。' | '！' | '？')
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_' || ch == '-'
}

fn char_index_to_byte_index(s: &str, char_idx: usize) -> usize {
    if char_idx == 0 {
        return 0;
    }
    for (idx, (byte_idx, _)) in s.char_indices().enumerate() {
        if idx == char_idx {
            return byte_idx;
        }
    }
    s.len()
}

fn sentence_start_position(lines: &[String], cursor: (usize, usize)) -> Option<(usize, usize)> {
    let (chars, positions) = flatten_text_with_positions(lines);
    if chars.is_empty() {
        return Some((0, 0));
    }

    let mut idx = cursor_index_from_positions(&positions, cursor);
    if idx >= chars.len() {
        idx = chars.len() - 1;
    }
    if idx > 0 {
        idx -= 1;
    }

    while idx > 0 {
        if is_sentence_terminator(chars[idx]) {
            let mut candidate = idx + 1;
            while candidate < chars.len() && chars[candidate].is_whitespace() {
                candidate += 1;
            }
            return positions.get(candidate).copied().or_else(|| positions.last().copied());
        }
        idx = idx.saturating_sub(1);
    }

    let mut candidate = 0;
    while candidate < chars.len() && chars[candidate].is_whitespace() {
        candidate += 1;
    }

    positions.get(candidate).copied().or_else(|| positions.first().copied())
}

fn sentence_end_position(lines: &[String], cursor: (usize, usize)) -> Option<(usize, usize)> {
    let (chars, positions) = flatten_text_with_positions(lines);
    if chars.is_empty() {
        return Some((0, 0));
    }

    let mut idx = cursor_index_from_positions(&positions, cursor);
    if idx >= chars.len() {
        idx = chars.len() - 1;
    }

    while idx < chars.len() {
        if is_sentence_terminator(chars[idx]) {
            let mut last = idx;
            while last + 1 < chars.len() && matches!(chars[last + 1], ')' | ']' | '}' | '"' | '\'')
            {
                last += 1;
            }
            return positions.get(last).copied().or_else(|| positions.last().copied());
        }
        idx += 1;
    }

    positions.last().copied()
}

fn bracket_pair(ch: char) -> Option<(char, char)> {
    match ch {
        '(' | ')' => Some(('(', ')')),
        '[' | ']' => Some(('[', ']')),
        '{' | '}' => Some(('{', '}')),
        '<' | '>' => Some(('<', '>')),
        _ => None,
    }
}

fn matching_parenthesis_position(
    lines: &[String],
    cursor: (usize, usize),
) -> Option<(usize, usize)> {
    let (chars, positions) = flatten_text_with_positions(lines);
    if chars.is_empty() {
        return None;
    }

    let mut current_index =
        positions.iter().position(|&(row, col)| row == cursor.0 && col == cursor.1);

    let mut search_char = current_index.map(|idx| chars[idx]);

    if search_char.is_none() && cursor.1 > 0 {
        let target_col = cursor.1 - 1;
        current_index =
            positions.iter().position(|&(row, col)| row == cursor.0 && col == target_col);
        search_char = current_index.map(|idx| chars[idx]);
    }

    let idx = current_index?;
    let current_char = search_char?;
    let (open, close) = bracket_pair(current_char)?;

    if current_char == open {
        let mut depth = 0;
        for j in idx + 1..chars.len() {
            let ch = chars[j];
            if ch == open {
                depth += 1;
            } else if ch == close {
                if depth == 0 {
                    return positions.get(j).copied();
                }
                depth -= 1;
            }
        }
    } else {
        let mut depth = 0;
        for j in (0..idx).rev() {
            let ch = chars[j];
            if ch == close {
                depth += 1;
            } else if ch == open {
                if depth == 0 {
                    return positions.get(j).copied();
                }
                depth -= 1;
            }
        }
    }

    None
}

fn recenter_viewport(textarea: &mut TextArea<'static>) {
    textarea.move_cursor(tui_textarea::CursorMove::InViewport);
}

fn transpose_words(
    textarea: &mut TextArea<'static>,
    lines_snapshot: &[String],
    cursor: (usize, usize),
    original_yank: &String,
) {
    use tui_textarea::CursorMove;

    if cursor.0 >= lines_snapshot.len() {
        return;
    }

    let line = &lines_snapshot[cursor.0];
    let chars: Vec<char> = line.chars().collect();
    if chars.is_empty() {
        return;
    }

    let cursor_col = cursor.1.min(chars.len());

    let mut word1_end = cursor_col;
    while word1_end > 0 && chars[word1_end - 1].is_whitespace() {
        word1_end -= 1;
    }
    let mut word1_start = word1_end;
    while word1_start > 0 && is_word_char(chars[word1_start - 1]) {
        word1_start -= 1;
    }
    if word1_start == word1_end {
        return;
    }

    let mut word2_start = cursor_col;
    while word2_start < chars.len() && chars[word2_start].is_whitespace() {
        word2_start += 1;
    }
    let mut word2_end = word2_start;
    while word2_end < chars.len() && is_word_char(chars[word2_end]) {
        word2_end += 1;
    }
    if word2_start == word2_end {
        return;
    }

    let word1_start_byte = char_index_to_byte_index(line, word1_start);
    let word1_end_byte = char_index_to_byte_index(line, word1_end);
    let word2_start_byte = char_index_to_byte_index(line, word2_start);
    let word2_end_byte = char_index_to_byte_index(line, word2_end);

    let word1 = line[word1_start_byte..word1_end_byte].to_string();
    let between = line[word1_end_byte..word2_start_byte].to_string();
    let word2 = line[word2_start_byte..word2_end_byte].to_string();

    textarea.move_cursor(CursorMove::Jump(cursor.0 as u16, word1_start as u16));
    textarea.start_selection();
    textarea.move_cursor(CursorMove::Jump(cursor.0 as u16, word2_end as u16));

    if textarea.cut() {
        textarea.set_yank_text(original_yank.as_str());
        textarea.insert_str(&word2);
        if !between.is_empty() {
            textarea.insert_str(&between);
        }
        textarea.insert_str(&word1);

        textarea.move_cursor(CursorMove::Jump(cursor.0 as u16, word2_end as u16));
    } else {
        textarea.cancel_selection();
    }
}

fn paragraph_bounds(lines: &[String], row: usize) -> (usize, usize) {
    if lines.is_empty() {
        return (0, 0);
    }

    let mut start = row.min(lines.len() - 1);
    while start > 0 && !lines[start - 1].trim().is_empty() {
        start -= 1;
    }

    let mut end = row.min(lines.len() - 1);
    while end + 1 < lines.len() && !lines[end + 1].trim().is_empty() {
        end += 1;
    }

    (start, end)
}

fn wrap_text_to_width(text: &str, width: usize) -> Vec<String> {
    if text.trim().is_empty() {
        return vec![String::new()];
    }

    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_len = 0usize;

    for word in text.split_whitespace() {
        let word_len = word.chars().count();
        if current.is_empty() {
            current.push_str(word);
            current_len = word_len;
        } else if current_len + 1 + word_len <= width {
            current.push(' ');
            current.push_str(word);
            current_len += 1 + word_len;
        } else {
            lines.push(current);
            current = word.to_string();
            current_len = word_len;
        }
    }

    if !current.is_empty() {
        lines.push(current);
    }

    if lines.is_empty() {
        vec![String::new()]
    } else {
        lines
    }
}

fn offset_to_row_col(lines: &[String], offset: usize) -> (usize, usize) {
    if lines.is_empty() {
        return (0, 0);
    }

    let mut remaining = offset;
    for (row, line) in lines.iter().enumerate() {
        let len = line.chars().count();
        if remaining <= len {
            return (row, remaining.min(len));
        }

        if remaining == 0 {
            return (row, 0);
        }

        remaining = remaining.saturating_sub(len + 1);
    }

    let last_row = lines.len() - 1;
    (last_row, lines[last_row].chars().count())
}

fn total_text_chars(lines: &[String]) -> usize {
    if lines.is_empty() {
        return 0;
    }

    let mut total = 0;
    for (idx, line) in lines.iter().enumerate() {
        total += line.chars().count();
        if idx + 1 < lines.len() {
            total += 1;
        }
    }
    total
}

fn set_text_with_offset(
    textarea: &mut TextArea<'static>,
    new_text: &str,
    original_yank: &str,
    target_offset: usize,
) {
    use tui_textarea::CursorMove;

    textarea.move_cursor(CursorMove::Top);
    textarea.move_cursor(CursorMove::Head);
    textarea.select_all();
    if textarea.cut() {
        textarea.set_yank_text(original_yank);
        let lines: Vec<&str> = new_text.split('\n').collect();
        for (idx, line) in lines.iter().enumerate() {
            if idx > 0 {
                textarea.insert_newline();
            }
            if !line.is_empty() {
                textarea.insert_str(line);
            }
        }

        let new_lines: Vec<String> = lines.iter().map(|s| (*s).to_string()).collect();
        let total = total_text_chars(&new_lines);
        let clamped_offset = target_offset.min(total);
        let (row, col) = offset_to_row_col(&new_lines, clamped_offset);
        textarea.move_cursor(CursorMove::Jump(row as u16, col as u16));
    } else {
        textarea.cancel_selection();
    }
}

fn fill_paragraph(
    textarea: &mut TextArea<'static>,
    lines_snapshot: &[String],
    cursor: (usize, usize),
    original_yank: &String,
) {
    use tui_textarea::CursorMove;

    if lines_snapshot.is_empty() || cursor.0 >= lines_snapshot.len() {
        return;
    }

    let (start, end) = paragraph_bounds(lines_snapshot, cursor.0);
    let mut paragraph_text = String::new();

    for (idx, line) in lines_snapshot[start..=end].iter().enumerate() {
        if idx > 0 {
            paragraph_text.push(' ');
        }
        paragraph_text.push_str(line.trim());
    }

    let wrapped = wrap_text_to_width(&paragraph_text, 80);

    let mut relative_offset = 0usize;
    for row in start..cursor.0 {
        relative_offset += lines_snapshot[row].chars().count() + 1;
    }
    relative_offset += cursor.1.min(lines_snapshot[cursor.0].chars().count());

    textarea.move_cursor(CursorMove::Jump(start as u16, 0));
    textarea.start_selection();
    if end + 1 < lines_snapshot.len() {
        textarea.move_cursor(CursorMove::Jump((end + 1) as u16, 0));
    } else {
        let end_col = lines_snapshot[end].chars().count();
        textarea.move_cursor(CursorMove::Jump(end as u16, end_col as u16));
    }

    if textarea.cut() {
        textarea.set_yank_text(original_yank.as_str());
        let mut first = true;
        for line in &wrapped {
            if !first {
                textarea.insert_newline();
            }
            if !line.is_empty() {
                textarea.insert_str(line);
            }
            first = false;
        }

        let (rel_row, rel_col) = offset_to_row_col(&wrapped, relative_offset);
        textarea.move_cursor(CursorMove::Jump((start + rel_row) as u16, rel_col as u16));
    } else {
        textarea.cancel_selection();
    }
}

fn move_line_up(
    textarea: &mut TextArea<'static>,
    cursor: (usize, usize),
    lines_snapshot: &[String],
    original_yank: &String,
) {
    use tui_textarea::CursorMove;

    if cursor.0 == 0 || cursor.0 >= lines_snapshot.len() {
        return;
    }

    let current_row = cursor.0;
    let prev_row = current_row - 1;
    let current_line = lines_snapshot[current_row].clone();
    let prev_line = lines_snapshot[prev_row].clone();

    textarea.move_cursor(CursorMove::Jump(prev_row as u16, 0));
    textarea.start_selection();
    if current_row + 1 < lines_snapshot.len() {
        textarea.move_cursor(CursorMove::Jump((current_row + 1) as u16, 0));
    } else {
        let end_col = current_line.chars().count();
        textarea.move_cursor(CursorMove::Jump(current_row as u16, end_col as u16));
    }

    if textarea.cut() {
        textarea.set_yank_text(original_yank.as_str());
        textarea.insert_str(&current_line);
        textarea.insert_newline();
        textarea.insert_str(&prev_line);

        let new_col = cursor.1.min(current_line.chars().count());
        textarea.move_cursor(CursorMove::Jump(prev_row as u16, new_col as u16));
    } else {
        textarea.cancel_selection();
    }
}

fn move_line_down(
    textarea: &mut TextArea<'static>,
    cursor: (usize, usize),
    lines_snapshot: &[String],
    original_yank: &String,
) {
    use tui_textarea::CursorMove;

    if lines_snapshot.is_empty() || cursor.0 + 1 >= lines_snapshot.len() {
        return;
    }

    let current_row = cursor.0;
    let next_row = current_row + 1;
    let current_line = lines_snapshot[current_row].clone();
    let next_line = lines_snapshot[next_row].clone();

    textarea.move_cursor(CursorMove::Jump(current_row as u16, 0));
    textarea.start_selection();
    if next_row + 1 < lines_snapshot.len() {
        textarea.move_cursor(CursorMove::Jump((next_row + 1) as u16, 0));
    } else {
        let end_col = next_line.chars().count();
        textarea.move_cursor(CursorMove::Jump(next_row as u16, end_col as u16));
    }

    if textarea.cut() {
        textarea.set_yank_text(original_yank.as_str());
        textarea.insert_str(&next_line);
        textarea.insert_newline();
        textarea.insert_str(&current_line);

        let new_col = cursor.1.min(current_line.chars().count());
        textarea.move_cursor(CursorMove::Jump(next_row as u16, new_col as u16));
    } else {
        textarea.cancel_selection();
    }
}

fn indent_region(textarea: &mut TextArea<'static>) {
    use tui_textarea::CursorMove;

    let indent = textarea.indent().to_string();
    if indent.is_empty() {
        return;
    }

    if let Some(((start_row, _), (end_row, end_col))) = textarea.selection_range() {
        let mut last_row = end_row;
        if end_col == 0 && end_row > start_row {
            last_row -= 1;
        }
        for row in start_row..=last_row {
            textarea.move_cursor(CursorMove::Jump(row as u16, 0));
            textarea.insert_str(&indent);
        }
    } else {
        let (row, col) = textarea.cursor();
        textarea.move_cursor(CursorMove::Jump(row as u16, 0));
        textarea.insert_str(&indent);
        let new_col = col + indent.chars().count();
        textarea.move_cursor(CursorMove::Jump(row as u16, new_col as u16));
    }
}

// Keymap function to translate KeyEvent to Command
fn key_to_command(key: &crossterm::event::KeyEvent) -> Option<Command> {
    use crossterm::event::{KeyCode, KeyModifiers};

    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);

    match key.code {
        // Cursor Movement
        KeyCode::Home if !ctrl => Some(Command::MoveToBeginningOfLine),
        KeyCode::End if !ctrl => Some(Command::MoveToEndOfLine),
        KeyCode::Left if !ctrl && !alt => Some(Command::MoveBackwardOneCharacter),
        KeyCode::Right if !ctrl && !alt => Some(Command::MoveForwardOneCharacter),
        KeyCode::Up if !ctrl => Some(Command::MoveToPreviousLine),
        KeyCode::Down if !ctrl => Some(Command::MoveToNextLine),
        KeyCode::Left if ctrl => Some(Command::MoveBackwardOneWord),
        KeyCode::Right if ctrl => Some(Command::MoveForwardOneWord),
        KeyCode::Char('a') if ctrl => Some(Command::SelectAll),
        KeyCode::Char('e') if ctrl => Some(Command::MoveToEndOfLine),
        KeyCode::Char('f') if ctrl => Some(Command::MoveForwardOneCharacter),
        KeyCode::Char('b') if ctrl => Some(Command::MoveBackwardOneCharacter),
        KeyCode::Char('n') if ctrl => Some(Command::MoveToNextLine),
        KeyCode::Char('p') if ctrl => Some(Command::MoveToPreviousLine),
        KeyCode::Char('f') if alt => Some(Command::MoveForwardOneWord),
        KeyCode::Char('b') if alt => Some(Command::MoveBackwardOneWord),
        KeyCode::Char('a') if alt => Some(Command::MoveToBeginningOfSentence),
        KeyCode::Char('e') if alt => Some(Command::MoveToEndOfSentence),
        KeyCode::Char('v') if ctrl && alt => Some(Command::ScrollDownOneScreen),
        KeyCode::Char('v') if ctrl => Some(Command::Paste),
        KeyCode::Char('v') if alt => Some(Command::ScrollUpOneScreen),
        KeyCode::Char('l') if ctrl => Some(Command::RecenterScreenOnCursor),
        KeyCode::Home if ctrl => Some(Command::MoveToBeginningOfDocument),
        KeyCode::End if ctrl => Some(Command::MoveToEndOfDocument),
        KeyCode::Up if alt => Some(Command::MoveToBeginningOfParagraph),
        KeyCode::Down if alt => Some(Command::MoveToEndOfParagraph),
        KeyCode::Char('g') if alt && ctrl => Some(Command::GoToLineNumber),
        KeyCode::Char('f') if alt && ctrl => Some(Command::MoveToMatchingParenthesis),

        // Editing and Deletion
        KeyCode::Delete if !ctrl && !alt => Some(Command::DeleteCharacterForward),
        KeyCode::Backspace if !ctrl && !alt => Some(Command::DeleteCharacterBackward),
        KeyCode::Delete if ctrl => Some(Command::DeleteWordForward),
        KeyCode::Delete if alt => Some(Command::DeleteWordForward),
        KeyCode::Backspace if ctrl => Some(Command::DeleteWordBackward),
        KeyCode::Backspace if alt => Some(Command::DeleteWordBackward),
        KeyCode::Char('k') if ctrl => Some(Command::DeleteToEndOfLine),
        KeyCode::Char('w') if ctrl => Some(Command::Cut),
        KeyCode::Char('c') if ctrl => Some(Command::Copy),
        KeyCode::Char('y') if alt => Some(Command::CycleThroughClipboard),
        KeyCode::Char('t') if ctrl => Some(Command::TransposeCharacters),
        KeyCode::Char('t') if alt => Some(Command::TransposeWords),
        KeyCode::Char('z') if ctrl => Some(Command::Undo),
        KeyCode::Char('y') if ctrl && shift => Some(Command::Redo),
        KeyCode::Char('o') if ctrl => Some(Command::OpenNewLine),
        KeyCode::Char('j') if ctrl => Some(Command::OpenNewLine),
        KeyCode::Tab => Some(Command::IndentOrComplete),
        KeyCode::Backspace if ctrl && alt => Some(Command::DeleteToBeginningOfLine),
        KeyCode::Char('h') if ctrl => Some(Command::DeleteCharacterBackward), // Terminal control code

        // Text Transformation
        KeyCode::Char('u') if alt => Some(Command::UppercaseWord),
        KeyCode::Char('l') if alt => Some(Command::LowercaseWord),
        KeyCode::Char('c') if alt => Some(Command::CapitalizeWord),
        KeyCode::Char('q') if alt => Some(Command::FillParagraph),
        KeyCode::Char('^') if alt => Some(Command::JoinLines),

        // Formatting (Markdown Style)
        KeyCode::Char('b') if ctrl => Some(Command::Bold),
        KeyCode::Char('i') if ctrl => Some(Command::Italic),
        KeyCode::Char('u') if ctrl => Some(Command::Underline),
        KeyCode::Char('k') if ctrl => Some(Command::InsertHyperlink),

        // Code Editing
        KeyCode::Char(';') if alt => Some(Command::ToggleComment),
        KeyCode::Char('d') if ctrl => Some(Command::DuplicateLineSelection),
        KeyCode::Up if alt && shift => Some(Command::MoveLineUp),
        KeyCode::Down if alt && shift => Some(Command::MoveLineDown),
        KeyCode::Char(']') if ctrl => Some(Command::IndentRegion),
        KeyCode::Char('[') if ctrl => Some(Command::DedentRegion),

        // Search and Replace
        KeyCode::Char('s') if ctrl => Some(Command::IncrementalSearchForward),
        KeyCode::Char('r') if ctrl => Some(Command::IncrementalSearchBackward),
        KeyCode::Char('h') if ctrl => Some(Command::FindAndReplace),
        KeyCode::Char('%') if alt && ctrl => Some(Command::FindAndReplaceWithRegex),
        KeyCode::Char('g') if ctrl => Some(Command::FindNext),
        KeyCode::Char('g') if ctrl && shift => Some(Command::FindPrevious),

        // Mark and Region
        KeyCode::Char(' ') if ctrl => Some(Command::SetMark),
        KeyCode::Char('a') if ctrl => Some(Command::SelectAll),

        _ => None,
    }
}

fn command_clears_selection(command: Command) -> bool {
    matches!(
        command,
        Command::MoveToBeginningOfLine
            | Command::MoveToEndOfLine
            | Command::MoveForwardOneCharacter
            | Command::MoveBackwardOneCharacter
            | Command::MoveToNextLine
            | Command::MoveToPreviousLine
            | Command::MoveForwardOneWord
            | Command::MoveBackwardOneWord
            | Command::MoveToBeginningOfSentence
            | Command::MoveToEndOfSentence
            | Command::ScrollDownOneScreen
            | Command::ScrollUpOneScreen
            | Command::RecenterScreenOnCursor
            | Command::MoveToBeginningOfDocument
            | Command::MoveToEndOfDocument
            | Command::MoveToBeginningOfParagraph
            | Command::MoveToEndOfParagraph
            | Command::GoToLineNumber
            | Command::MoveToMatchingParenthesis
    )
}

// Execute a command on the TextArea
fn execute_command(
    textarea: &mut TextArea<'static>,
    command: Command,
    search_mode: &mut SearchMode,
    kill_ring: &mut KillRing,
    clipboard: &mut SystemClipboard,
) -> CommandEffect {
    use tui_textarea::{CursorMove, Scrolling};

    let before_lines: Vec<String> = textarea.lines().iter().cloned().collect();
    let before_cursor = textarea.cursor();
    let before_yank = textarea.yank_text();
    let mut effect = CommandEffect::default();
    let mut yank_already_pushed = false;

    match command {
        // Cursor Movement
        Command::MoveToBeginningOfLine => {
            textarea.move_cursor(CursorMove::Head);
        }
        Command::MoveToEndOfLine => {
            textarea.move_cursor(CursorMove::End);
        }
        Command::MoveForwardOneCharacter => {
            textarea.move_cursor(CursorMove::Forward);
        }
        Command::MoveBackwardOneCharacter => {
            textarea.move_cursor(CursorMove::Back);
        }
        Command::MoveToNextLine => {
            textarea.move_cursor(CursorMove::Down);
        }
        Command::MoveToPreviousLine => {
            textarea.move_cursor(CursorMove::Up);
        }
        Command::MoveForwardOneWord => {
            textarea.move_cursor(CursorMove::WordForward);
        }
        Command::MoveBackwardOneWord => {
            textarea.move_cursor(CursorMove::WordBack);
        }
        Command::MoveToBeginningOfSentence => {
            if let Some((row, col)) = sentence_start_position(&before_lines, before_cursor) {
                textarea.move_cursor(CursorMove::Jump(row as u16, col as u16));
            } else {
                textarea.move_cursor(CursorMove::Top);
            }
        }
        Command::MoveToEndOfSentence => {
            if let Some((row, col)) = sentence_end_position(&before_lines, before_cursor) {
                textarea.move_cursor(CursorMove::Jump(row as u16, col as u16));
            } else {
                textarea.move_cursor(CursorMove::End);
            }
        }
        Command::ScrollDownOneScreen => {
            textarea.scroll(Scrolling::PageDown);
        }
        Command::ScrollUpOneScreen => {
            textarea.scroll(Scrolling::PageUp);
        }
        Command::RecenterScreenOnCursor => {
            recenter_viewport(textarea);
        }
        Command::MoveToBeginningOfDocument => {
            textarea.move_cursor(CursorMove::Top);
        }
        Command::MoveToEndOfDocument => {
            textarea.move_cursor(CursorMove::Bottom);
        }
        Command::MoveToBeginningOfParagraph => {
            textarea.move_cursor(CursorMove::ParagraphBack);
        }
        Command::MoveToEndOfParagraph => {
            textarea.move_cursor(CursorMove::ParagraphForward);
        }
        Command::GoToLineNumber => {
            effect.action = Some(CommandAction::OpenGoToLineDialog);
        }
        Command::MoveToMatchingParenthesis => {
            if let Some((row, col)) = matching_parenthesis_position(&before_lines, before_cursor) {
                textarea.move_cursor(CursorMove::Jump(row as u16, col as u16));
            }
        }

        // Editing and Deletion
        Command::SelectAll => {
            textarea.select_all();
        }
        Command::DeleteCharacterForward => {
            textarea.delete_next_char();
        }
        Command::DeleteCharacterBackward => {
            textarea.delete_char();
        }
        Command::DeleteWordForward => {
            textarea.delete_next_word();
        }
        Command::DeleteWordBackward => {
            textarea.delete_word();
        }
        Command::DeleteToEndOfLine => {
            textarea.delete_line_by_end();
        }
        Command::Cut => {
            if textarea.cut() {
                let yank = textarea.yank_text();
                kill_ring.push(yank.clone());
                yank_already_pushed = true;
                if let Err(err) = clipboard.set_text(&yank) {
                    effect.status_message = Some(err);
                } else {
                    effect.status_message = Some("Cut to clipboard".to_string());
                }
                effect.text_changed = true;
            }
        }
        Command::Copy => {
            let had_selection = textarea.selection_range().is_some();
            textarea.copy();
            if had_selection {
                let yank = textarea.yank_text();
                kill_ring.push(yank.clone());
                yank_already_pushed = true;
                if let Err(err) = clipboard.set_text(&yank) {
                    effect.status_message = Some(err);
                } else {
                    effect.status_message = Some("Copied to clipboard".to_string());
                }
            } else {
                effect.status_message = Some("No selection to copy".to_string());
            }
        }
        Command::Paste => {
            let mut pasted = false;
            match clipboard.get_text() {
                Ok(text) if !text.is_empty() => {
                    kill_ring.push(text.clone());
                    yank_already_pushed = true;
                    textarea.set_yank_text(text);
                    if textarea.paste() {
                        pasted = true;
                        effect.status_message = Some("Pasted from clipboard".to_string());
                    }
                }
                Ok(_) => {}
                Err(err) => {
                    effect.status_message = Some(err);
                }
            }

            if !pasted && textarea.paste() {
                pasted = true;
                if effect.status_message.is_none() {
                    effect.status_message = Some("Pasted".to_string());
                }
            }

            if pasted {
                effect.text_changed = true;
                effect.caret_moved = true;
            } else if effect.status_message.is_none() {
                if let Some(err) = clipboard.last_error() {
                    effect.status_message = Some(err.clone());
                }
            }
        }
        Command::CycleThroughClipboard => {
            if let Some(next) = kill_ring.cycle_next() {
                textarea.set_yank_text(next.clone());
                yank_already_pushed = true;
                if let Err(err) = clipboard.set_text(&next) {
                    effect.status_message = Some(err);
                }
                if textarea.paste() {
                    effect.text_changed = true;
                    effect.caret_moved = true;
                    if effect.status_message.is_none() {
                        effect.status_message = Some("Cycling clipboard entries".to_string());
                    }
                }
            } else {
                effect.status_message = Some("Clipboard history empty".to_string());
            }
        }
        Command::TransposeCharacters => {
            // Transpose the character before cursor with the character at cursor
            let cursor = textarea.cursor();
            let lines = textarea.lines();

            if let Some(line) = lines.get(cursor.0) {
                let line_chars: Vec<char> = line.chars().collect();

                // Need at least 2 characters: one before cursor and one at/after cursor
                if cursor.1 > 0 && cursor.1 <= line_chars.len() {
                    let before_idx = cursor.1 - 1;
                    let at_idx = cursor.1;

                    if at_idx < line_chars.len() {
                        // Swap characters: delete both and reinsert in reverse order
                        textarea.move_cursor(CursorMove::Back); // Move to before_idx
                        let char_before = line_chars[before_idx];
                        let char_at = line_chars[at_idx];

                        // Delete both characters
                        textarea.delete_next_char(); // Delete char_before
                        textarea.delete_next_char(); // Delete char_at

                        // Reinsert in reverse order
                        textarea.insert_char(char_at);
                        textarea.insert_char(char_before);

                        // Move cursor back to after the transposed characters
                        textarea.move_cursor(CursorMove::Back);
                    }
                }
            }
        }
        Command::TransposeWords => {
            transpose_words(textarea, &before_lines, before_cursor, &before_yank);
        }
        Command::Undo => {
            textarea.undo();
        }
        Command::Redo => {
            textarea.redo();
        }
        Command::OpenNewLine => {
            textarea.insert_newline();
            effect.status_message = Some("Inserted newline".to_string());
            effect.text_changed = true;
            effect.caret_moved = true;
        }
        Command::IndentOrComplete => {
            // For now, just insert tab (indent)
            textarea.insert_tab();
        }
        Command::DeleteToBeginningOfLine => {
            textarea.delete_line_by_head();
        }

        // Text Transformation
        Command::UppercaseWord => {
            // Get current cursor position
            let cursor = textarea.cursor();
            let lines = textarea.lines();

            if let Some(line) = lines.get(cursor.0) {
                let line_chars: Vec<char> = line.chars().collect();
                let mut start = cursor.1;
                let mut end = cursor.1;

                // Find word boundaries
                while start > 0 && line_chars[start - 1].is_alphanumeric() {
                    start -= 1;
                }
                while end < line_chars.len() && line_chars[end].is_alphanumeric() {
                    end += 1;
                }

                if start < end {
                    let word = &line[start..end];
                    let uppercased = word.to_uppercase();

                    // Replace the word
                    for _ in start..end {
                        textarea.delete_next_char();
                    }
                    for _ in 0..start {
                        textarea.move_cursor(CursorMove::Back);
                    }
                    for ch in uppercased.chars() {
                        textarea.insert_char(ch);
                    }
                }
            }
        }
        Command::LowercaseWord => {
            // Get current cursor position
            let cursor = textarea.cursor();
            let lines = textarea.lines();

            if let Some(line) = lines.get(cursor.0) {
                let line_chars: Vec<char> = line.chars().collect();
                let mut start = cursor.1;
                let mut end = cursor.1;

                // Find word boundaries
                while start > 0 && line_chars[start - 1].is_alphanumeric() {
                    start -= 1;
                }
                while end < line_chars.len() && line_chars[end].is_alphanumeric() {
                    end += 1;
                }

                if start < end {
                    let word = &line[start..end];
                    let lowercased = word.to_lowercase();

                    // Replace the word
                    for _ in start..end {
                        textarea.delete_next_char();
                    }
                    for _ in 0..start {
                        textarea.move_cursor(CursorMove::Back);
                    }
                    for ch in lowercased.chars() {
                        textarea.insert_char(ch);
                    }
                }
            }
        }
        Command::CapitalizeWord => {
            // Get current cursor position
            let cursor = textarea.cursor();
            let lines = textarea.lines();

            if let Some(line) = lines.get(cursor.0) {
                let line_chars: Vec<char> = line.chars().collect();
                let mut start = cursor.1;
                let mut end = cursor.1;

                // Find word boundaries
                while start > 0 && line_chars[start - 1].is_alphanumeric() {
                    start -= 1;
                }
                while end < line_chars.len() && line_chars[end].is_alphanumeric() {
                    end += 1;
                }

                if start < end {
                    let word = &line[start..end];
                    let mut chars = word.chars();
                    let capitalized = if let Some(first) = chars.next() {
                        first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
                    } else {
                        String::new()
                    };

                    // Replace the word
                    for _ in start..end {
                        textarea.delete_next_char();
                    }
                    for _ in 0..start {
                        textarea.move_cursor(CursorMove::Back);
                    }
                    for ch in capitalized.chars() {
                        textarea.insert_char(ch);
                    }
                }
            }
        }
        Command::FillParagraph => {
            fill_paragraph(textarea, &before_lines, before_cursor, &before_yank);
        }
        Command::JoinLines => {
            // Join current line with next line
            textarea.delete_newline();
        }

        // Formatting (Markdown Style)
        Command::Bold => {
            // Check if there's a selection
            let selection_range = textarea.selection_range();
            if selection_range.is_some() {
                // Wrap selection in **
                textarea.insert_str("**");
                // Move cursor to end of selection and add closing **
                // This is tricky - for now just insert at cursor
                textarea.insert_str("****");
                textarea.move_cursor(CursorMove::Back);
                textarea.move_cursor(CursorMove::Back);
            } else {
                // No selection - insert ** and position cursor between them
                textarea.insert_str("****");
                textarea.move_cursor(CursorMove::Back);
                textarea.move_cursor(CursorMove::Back);
            }
        }
        Command::Italic => {
            // Check if there's a selection
            let selection_range = textarea.selection_range();
            if selection_range.is_some() {
                // Wrap selection in *
                textarea.insert_str("*");
                // Move to end and add closing *
                textarea.insert_str("**");
                textarea.move_cursor(CursorMove::Back);
            } else {
                // No selection - insert ** and position cursor between them
                textarea.insert_str("**");
                textarea.move_cursor(CursorMove::Back);
            }
        }
        Command::Underline => {
            // Check if there's a selection
            let selection_range = textarea.selection_range();
            if selection_range.is_some() {
                // Wrap selection in __
                textarea.insert_str("__");
                // Move to end and add closing __
                textarea.insert_str("____");
                textarea.move_cursor(CursorMove::Back);
                textarea.move_cursor(CursorMove::Back);
            } else {
                // No selection - insert ____ and position cursor between them
                textarea.insert_str("____");
                textarea.move_cursor(CursorMove::Back);
                textarea.move_cursor(CursorMove::Back);
            }
        }
        Command::InsertHyperlink => {
            // Insert [text](url) and position cursor appropriately
            textarea.insert_str("[](url)");
            textarea.move_cursor(CursorMove::Back);
            textarea.move_cursor(CursorMove::Back);
            textarea.move_cursor(CursorMove::Back);
            textarea.move_cursor(CursorMove::Back);
            textarea.move_cursor(CursorMove::Back);
        }

        // Code Editing
        Command::ToggleComment => {
            // Toggle comment on current line - use # for comments
            let cursor = textarea.cursor();
            if let Some(line) = textarea.lines().get(cursor.0).cloned() {
                textarea.move_cursor(CursorMove::Head);

                if line.trim_start().starts_with("# ") {
                    // Uncomment: remove "# " from start of line
                    while let Some(current_line) = textarea.lines().get(textarea.cursor().0) {
                        if current_line.starts_with("# ") {
                            textarea.delete_next_char();
                            textarea.delete_next_char();
                            break;
                        } else if current_line.starts_with('#') {
                            textarea.delete_next_char();
                            break;
                        } else if current_line.starts_with(' ') || current_line.starts_with('\t') {
                            textarea.delete_next_char();
                        } else {
                            break;
                        }
                    }
                } else if line.trim_start().starts_with('#') {
                    // Uncomment: remove # from start of line
                    while let Some(current_line) = textarea.lines().get(textarea.cursor().0) {
                        if current_line.starts_with('#') {
                            textarea.delete_next_char();
                            break;
                        } else if current_line.starts_with(' ') || current_line.starts_with('\t') {
                            textarea.delete_next_char();
                        } else {
                            break;
                        }
                    }
                } else {
                    // Comment: insert "# " at beginning of line content
                    // Skip leading whitespace
                    while let Some(current_line) = textarea.lines().get(textarea.cursor().0) {
                        if current_line.starts_with(' ') || current_line.starts_with('\t') {
                            textarea.move_cursor(CursorMove::Forward);
                        } else {
                            break;
                        }
                    }
                    textarea.insert_str("# ");
                }

                // Restore cursor position within the line
                textarea.move_cursor(CursorMove::Jump(cursor.0 as u16, cursor.1.max(2) as u16));
            }
        }
        Command::DuplicateLineSelection => {
            // Duplicate current line or selection
            let selection_range = textarea.selection_range();
            if selection_range.is_some() {
                // Duplicate selection
                textarea.copy();
                textarea.insert_newline();
                textarea.paste();
            } else {
                // Duplicate current line
                let cursor = textarea.cursor();
                textarea.move_cursor(CursorMove::Head);
                textarea.start_selection();
                textarea.move_cursor(CursorMove::End);
                textarea.copy();
                textarea.cancel_selection();
                textarea.move_cursor(CursorMove::End);
                textarea.insert_newline();
                textarea.paste();
                // Restore cursor to original line
                textarea.move_cursor(CursorMove::Jump(cursor.0 as u16, cursor.1 as u16));
            }
        }
        Command::MoveLineUp => {
            move_line_up(textarea, before_cursor, &before_lines, &before_yank);
        }
        Command::MoveLineDown => {
            move_line_down(textarea, before_cursor, &before_lines, &before_yank);
        }
        Command::IndentRegion => {
            indent_region(textarea);
        }
        Command::DedentRegion => {
            // Dedent current line or selected lines
            let cursor = textarea.cursor();
            textarea.move_cursor(CursorMove::Head);

            // Remove up to 4 spaces or 1 tab from start of line
            for _ in 0..4 {
                if let Some(line) = textarea.lines().get(textarea.cursor().0) {
                    if line.starts_with('\t') {
                        textarea.delete_next_char();
                        break;
                    } else if line.starts_with(' ') {
                        textarea.delete_next_char();
                    } else {
                        break;
                    }
                }
            }

            // Restore cursor position, adjusting for removed indentation
            let new_cursor_col = if cursor.1 >= 4 { cursor.1 - 4 } else { 0 };
            textarea.move_cursor(CursorMove::Jump(cursor.0 as u16, new_cursor_col as u16));
        }

        // Search and Replace
        Command::IncrementalSearchForward => {
            // Enter incremental search forward mode
            // We'll collect keystrokes in the main event loop
            *search_mode = SearchMode::IncrementalForward;
            let _ = textarea.set_search_pattern("".to_string());
        }
        Command::IncrementalSearchBackward => {
            // Enter incremental search backward mode
            *search_mode = SearchMode::IncrementalBackward;
            let _ = textarea.set_search_pattern("".to_string());
        }
        Command::FindAndReplace => {
            effect.action = Some(CommandAction::OpenFindReplace { is_regex: false });
        }
        Command::FindAndReplaceWithRegex => {
            effect.action = Some(CommandAction::OpenFindReplace { is_regex: true });
        }
        Command::FindNext => {
            // Find next occurrence of current search pattern
            if let Some(_pattern) = textarea.search_pattern() {
                textarea.search_forward(false);
            }
        }
        Command::FindPrevious => {
            // Find previous occurrence of current search pattern
            if let Some(_pattern) = textarea.search_pattern() {
                textarea.search_back(false);
            }
        }

        // Mark and Region
        Command::SetMark => {
            textarea.start_selection();
        }
        Command::ExtendSelection => {
            // This command is handled specially in the key event processing
            // The keymap detects shift+arrow but we handle it in the main event loop
        }
    }

    let after_lines = textarea.lines();
    if after_lines != before_lines.as_slice() {
        effect.text_changed = true;
    }
    if textarea.cursor() != before_cursor {
        effect.caret_moved = true;
    }

    let after_yank = textarea.yank_text();
    let yank_changed = after_yank != before_yank;
    if yank_changed && !yank_already_pushed {
        kill_ring.push(after_yank);
    }

    effect
}

// Logging function for debugging key events
fn log_key_event(key: &crossterm::event::KeyEvent, context: &str) {
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open("key_log.txt") {
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
        if matches!(
            key.code,
            KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down
        ) {
            let _ = writeln!(
                file,
                "  ARROW KEY DETECTED: code={:?}, modifiers={:?}",
                key.code, key.modifiers
            );
        }
    }
}

// Padding constants for easy editing
const TEXTAREA_LEFT_PADDING: usize = 1;
const TEXTAREA_TOP_PADDING: usize = 1;
const TEXTAREA_BOTTOM_PADDING: usize = 1;
const TEXTAREA_RIGHT_PADDING: usize = 1;
const MIN_TEXTAREA_VISIBLE_LINES: usize = 5;

const BUTTON_LEFT_PADDING: usize = 0;

const ACTIVE_TASK_LEFT_PADDING: usize = 0;

const MODAL_INNER_PADDING: usize = 1;

#[derive(Debug, Clone, PartialEq)]
enum TaskState {
    Draft,
    Active,
    Completed,
    Merged,
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
    creator: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FocusElement {
    TaskCard(usize),
    TaskDescription,
    RepositoryButton,
    BranchButton,
    ModelButton,
    GoButton,
    StopButton(usize), // Stop button for specific card
    SettingsButton,
    FilterBarLine, // Focus on the separator line itself, before any filter control
    Filter(FilterControl),
}

#[derive(Debug, Clone, Copy)]
enum DisplayItem {
    Task(usize),
    FilterBar,
    Spacer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FilterControl {
    Repository,
    Status,
    Creator,
}

impl FilterControl {
    fn index(self) -> usize {
        match self {
            FilterControl::Repository => 0,
            FilterControl::Status => 1,
            FilterControl::Creator => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FilterStatus {
    Active,
    Completed,
    Merged,
}

#[derive(Debug, Clone)]
struct FilterOptions {
    repositories: Vec<String>,
    creators: Vec<String>,
}

#[derive(Debug, Clone)]
struct FilterState {
    repository_index: usize,
    status_index: usize,
    creator_index: usize,
}

#[derive(Debug, Clone)]
struct FooterHint {
    key: String,
    key_style: Style,
    description: String,
    description_style: Style,
    action: Option<FooterAction>,
}

#[derive(Debug, Clone)]
struct FilterEditor {
    control: FilterControl,
    input: Input,
    options: Vec<String>,
    filtered: Vec<usize>,
    selected: usize,
    anchor: Rect,
}

impl FilterEditor {
    fn recompute(&mut self) {
        let query = self.input.value().trim().to_ascii_lowercase();
        if query.is_empty() {
            self.filtered = (0..self.options.len()).collect();
        } else {
            self.filtered = self
                .options
                .iter()
                .enumerate()
                .filter(|(_, value)| value.to_ascii_lowercase().contains(&query))
                .map(|(idx, _)| idx)
                .collect();
        }
        if self.filtered.is_empty() {
            self.selected = 0;
        } else {
            self.selected = self.selected.min(self.filtered.len() - 1);
        }
    }

    fn current_selection(&self) -> Option<usize> {
        self.filtered.get(self.selected).copied()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FooterAction {
    LaunchDraft,
    InsertNewLine,
    FocusNextField,
    FocusPreviousField,
    OpenShortcutHelp,
    OpenSettings,
    StopTask(usize),
    Quit,
}

#[derive(Debug, Clone)]
struct ShortcutHelpModal {
    entries: Vec<ShortcutDisplay>,
    scroll: usize,
}

#[derive(Debug, Clone)]
struct InteractiveArea {
    rect: Rect,
    action: MouseAction,
}

#[derive(Debug, Clone, Copy)]
enum MouseAction {
    SelectCard(usize),
    ActivateGoButton,
    OpenRepositoryModal,
    OpenBranchModal,
    OpenModelModal,
    StopTask(usize),
    OpenSettings,
    EditFilter(FilterControl),
    Footer(FooterAction),
}

const STATUS_FILTER_OPTIONS: &[(&str, Option<FilterStatus>)] = &[
    ("All", None),
    ("Active", Some(FilterStatus::Active)),
    ("Completed", Some(FilterStatus::Completed)),
    ("Merged", Some(FilterStatus::Merged)),
];

const SHORTCUT_HELP_VISIBLE_ROWS: usize = 16;

impl FilterOptions {
    fn from_tasks(tasks: &[TaskCard]) -> Self {
        let mut repo_set = BTreeSet::new();
        let mut creator_set = BTreeSet::new();
        for task in tasks {
            if !task.repository.is_empty() {
                repo_set.insert(task.repository.clone());
            }
            if !task.creator.is_empty() {
                creator_set.insert(task.creator.clone());
            }
        }

        let mut repositories = Vec::with_capacity(repo_set.len() + 1);
        repositories.push("All".to_string());
        repositories.extend(repo_set.into_iter());

        let mut creators = Vec::with_capacity(creator_set.len() + 1);
        creators.push("All".to_string());
        creators.extend(creator_set.into_iter());

        Self {
            repositories,
            creators,
        }
    }
}

impl FilterState {
    fn default() -> Self {
        Self {
            repository_index: 0,
            status_index: 0,
            creator_index: 0,
        }
    }

    fn repository_label<'a>(&self, options: &'a FilterOptions) -> &'a str {
        options
            .repositories
            .get(self.repository_index)
            .map(|s| s.as_str())
            .unwrap_or("All")
    }

    fn creator_label<'a>(&self, options: &'a FilterOptions) -> &'a str {
        options.creators.get(self.creator_index).map(|s| s.as_str()).unwrap_or("All")
    }

    fn status_label(&self) -> &'static str {
        STATUS_FILTER_OPTIONS
            .get(self.status_index)
            .map(|(label, _)| *label)
            .unwrap_or("All")
    }

    fn repository_filter<'a>(&self, options: &'a FilterOptions) -> Option<&'a str> {
        if self.repository_index == 0 {
            None
        } else {
            options.repositories.get(self.repository_index).map(|s| s.as_str())
        }
    }

    fn status_filter(&self) -> Option<FilterStatus> {
        STATUS_FILTER_OPTIONS.get(self.status_index).and_then(|(_, status)| *status)
    }

    fn creator_filter<'a>(&self, options: &'a FilterOptions) -> Option<&'a str> {
        if self.creator_index == 0 {
            None
        } else {
            options.creators.get(self.creator_index).map(|s| s.as_str())
        }
    }

    fn cycle_repository(&mut self, options: &FilterOptions, direction: i32) {
        if options.repositories.is_empty() {
            return;
        }
        self.repository_index =
            cycle_index(self.repository_index, options.repositories.len(), direction);
    }

    fn cycle_status(&mut self, direction: i32) {
        self.status_index = cycle_index(self.status_index, STATUS_FILTER_OPTIONS.len(), direction);
    }

    fn cycle_creator(&mut self, options: &FilterOptions, direction: i32) {
        if options.creators.is_empty() {
            return;
        }
        self.creator_index = cycle_index(self.creator_index, options.creators.len(), direction);
    }
}

#[derive(Debug, Clone, PartialEq)]
enum ModalState {
    None,
    RepositorySearch,
    BranchSearch,
    ModelSearch,
    ModelSelection,
    Settings,
    GoToLine,
    FindReplace,
    ShortcutHelp,
}

#[derive(Debug, Clone, PartialEq)]
enum SearchMode {
    None,
    IncrementalForward,
    IncrementalBackward,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsField {
    ActivityLines,
    AutocompleteBorder,
    AutocompleteBackground,
    WordWrap,
    LaunchShortcut,
    NewLineShortcut,
    NextFieldShortcut,
    PrevFieldShortcut,
    ShortcutHelpShortcut,
}

#[derive(Debug, Clone)]
struct SettingsForm {
    activity_lines_input: Input,
    autocomplete_border_input: Input,
    autocomplete_bg_input: Input,
    word_wrap_input: Input,
    launch_shortcut_input: Input,
    new_line_shortcut_input: Input,
    next_field_shortcut_input: Input,
    prev_field_shortcut_input: Input,
    shortcut_help_input: Input,
    focused_field: SettingsField,
}

impl SettingsForm {
    fn new(
        activity_lines: usize,
        border_enabled: bool,
        background: Color,
        word_wrap_enabled: bool,
        shortcuts: &InMemoryShortcutConfig,
    ) -> Self {
        let launch_binding = shortcuts.binding_strings(SHORTCUT_LAUNCH_TASK).unwrap_or_default();
        let new_line_binding = shortcuts.binding_strings(SHORTCUT_NEW_LINE).unwrap_or_default();
        let next_field_binding = shortcuts.binding_strings(SHORTCUT_NEXT_FIELD).unwrap_or_default();
        let prev_field_binding = shortcuts.binding_strings(SHORTCUT_PREV_FIELD).unwrap_or_default();
        let shortcut_help_binding =
            shortcuts.binding_strings(SHORTCUT_SHORTCUT_HELP).unwrap_or_default();

        Self {
            activity_lines_input: input_from_string(&activity_lines.to_string()),
            autocomplete_border_input: input_from_string(if border_enabled { "on" } else { "off" }),
            autocomplete_bg_input: input_from_string(&color_to_hex(background)),
            word_wrap_input: input_from_string(if word_wrap_enabled { "on" } else { "off" }),
            launch_shortcut_input: input_from_string(&format_bindings(&launch_binding)),
            new_line_shortcut_input: input_from_string(&format_bindings(&new_line_binding)),
            next_field_shortcut_input: input_from_string(&format_bindings(&next_field_binding)),
            prev_field_shortcut_input: input_from_string(&format_bindings(&prev_field_binding)),
            shortcut_help_input: input_from_string(&format_bindings(&shortcut_help_binding)),
            focused_field: SettingsField::ActivityLines,
        }
    }

    fn focus_next(&mut self) {
        self.focused_field = match self.focused_field {
            SettingsField::ActivityLines => SettingsField::AutocompleteBorder,
            SettingsField::AutocompleteBorder => SettingsField::AutocompleteBackground,
            SettingsField::AutocompleteBackground => SettingsField::WordWrap,
            SettingsField::WordWrap => SettingsField::LaunchShortcut,
            SettingsField::LaunchShortcut => SettingsField::NewLineShortcut,
            SettingsField::NewLineShortcut => SettingsField::NextFieldShortcut,
            SettingsField::NextFieldShortcut => SettingsField::PrevFieldShortcut,
            SettingsField::PrevFieldShortcut => SettingsField::ShortcutHelpShortcut,
            SettingsField::ShortcutHelpShortcut => SettingsField::ActivityLines,
        };
    }

    fn focus_prev(&mut self) {
        self.focused_field = match self.focused_field {
            SettingsField::ActivityLines => SettingsField::ShortcutHelpShortcut,
            SettingsField::AutocompleteBorder => SettingsField::ActivityLines,
            SettingsField::AutocompleteBackground => SettingsField::AutocompleteBorder,
            SettingsField::WordWrap => SettingsField::AutocompleteBackground,
            SettingsField::LaunchShortcut => SettingsField::WordWrap,
            SettingsField::NewLineShortcut => SettingsField::LaunchShortcut,
            SettingsField::NextFieldShortcut => SettingsField::NewLineShortcut,
            SettingsField::PrevFieldShortcut => SettingsField::NextFieldShortcut,
            SettingsField::ShortcutHelpShortcut => SettingsField::PrevFieldShortcut,
        };
    }

    fn focused_input_mut(&mut self) -> &mut Input {
        match self.focused_field {
            SettingsField::ActivityLines => &mut self.activity_lines_input,
            SettingsField::AutocompleteBorder => &mut self.autocomplete_border_input,
            SettingsField::AutocompleteBackground => &mut self.autocomplete_bg_input,
            SettingsField::WordWrap => &mut self.word_wrap_input,
            SettingsField::LaunchShortcut => &mut self.launch_shortcut_input,
            SettingsField::NewLineShortcut => &mut self.new_line_shortcut_input,
            SettingsField::NextFieldShortcut => &mut self.next_field_shortcut_input,
            SettingsField::PrevFieldShortcut => &mut self.prev_field_shortcut_input,
            SettingsField::ShortcutHelpShortcut => &mut self.shortcut_help_input,
        }
    }
}

fn format_bindings(values: &[String]) -> String {
    if values.is_empty() {
        String::new()
    } else {
        values.join(" | ")
    }
}

fn input_from_string(value: &str) -> Input {
    let mut input = Input::default();
    for ch in value.chars() {
        input.handle(InputRequest::InsertChar(ch));
    }
    input
}

fn color_to_hex(color: Color) -> String {
    match color {
        Color::Rgb(r, g, b) => format!("#{:02X}{:02X}{:02X}", r, g, b),
        _ => "#000000".to_string(),
    }
}

fn parse_hex_color(value: &str) -> Option<Color> {
    let trimmed = value.trim();
    let hex = trimmed.strip_prefix('#').unwrap_or(trimmed);
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Color::Rgb(r, g, b))
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

#[derive(Debug, Clone)]
struct GoToLineModal {
    input: Input,
    max_line: usize,
    error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FindReplaceStage {
    EnterSearch,
    EnterReplacement,
}

#[derive(Debug, Clone)]
struct FindReplaceModal {
    search_input: Input,
    replace_input: Input,
    is_regex: bool,
    stage: FindReplaceStage,
    error: Option<String>,
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

    /// Style for focused elements
    fn focused_style(&self) -> Style {
        Style::default().bg(self.primary).fg(Color::Black).add_modifier(Modifier::BOLD)
    }

    /// Style for selected elements
    fn selected_style(&self) -> Style {
        Style::default().bg(self.primary).fg(Color::Black)
    }

    /// Style for muted text
    fn muted_style(&self) -> Style {
        Style::default().fg(self.muted)
    }

    /// Style for neutral text
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

#[derive(Debug)]
struct AppState {
    selected_card: usize,
    focus_element: FocusElement,
    modal_state: ModalState,
    search_mode: SearchMode,
    fuzzy_modal: Option<FuzzySearchModal>,
    model_selection_modal: Option<ModelSelectionModal>,
    goto_line_modal: Option<GoToLineModal>,
    find_replace_modal: Option<FindReplaceModal>,
    task_description: TextArea<'static>,
    word_wrap_enabled: bool,
    selected_repository: String,
    selected_branch: String,
    selected_models: Vec<SelectedModel>, // Multiple models with instance counts
    activity_timer: Instant,
    activity_lines_count: usize, // Configurable number of activity lines (1-3)
    autocomplete: InlineAutocomplete,
    last_textarea_area: Cell<Option<Rect>>,
    show_autocomplete_border: bool,
    autocomplete_background: Color,
    settings_form: SettingsForm,
    kill_ring: KillRing,
    clipboard: SystemClipboard,
    status_message: Option<String>,
    shortcut_config: InMemoryShortcutConfig,
    filter_options: FilterOptions,
    filter_state: FilterState,
    interactive_areas: Vec<InteractiveArea>,
    shortcut_help_modal: Option<ShortcutHelpModal>,
    filter_button_rects: [Option<Rect>; 3],
    filter_editor: Option<FilterEditor>,
    scroll_offset: u16, // Vertical scroll offset in lines for the main tasks area
}

impl TaskCard {
    fn height(&self, app_state: &AppState) -> u16 {
        match self.state {
            TaskState::Completed | TaskState::Merged => 3, // Title + metadata + padding (2 lines content)
            TaskState::Active => 2 + app_state.activity_lines_count as u16 + 3, // Title + empty line + N activity lines + 2 for borders
            TaskState::Draft => {
                let visible_lines =
                    app_state.task_description.lines().len().max(MIN_TEXTAREA_VISIBLE_LINES);
                let inner_height = visible_lines
                    + TEXTAREA_TOP_PADDING
                    + TEXTAREA_BOTTOM_PADDING
                    + 1 // separator line
                    + 1; // button row
                inner_height as u16 + 2 // account for rounded border
            }
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
            let agent_strings: Vec<String> = self
                .agents
                .iter()
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
                    if last_activity.starts_with("Tool usage: ")
                        && !last_activity.contains("completed")
                    {
                        *last_activity = format!("Tool usage: {}: {}", tool_exec.name, line);
                    }
                }
                tool_exec.current_line_index += 1;
            } else if !tool_exec.is_complete {
                // Mark as complete and add completion message
                tool_exec.is_complete = true;
                let status = if tool_exec.success {
                    "completed successfully"
                } else {
                    "failed"
                };
                if let Some(last_activity) = self.activity.last_mut() {
                    if last_activity.starts_with("Tool usage: ")
                        && !last_activity.contains("completed")
                        && !last_activity.contains("failed")
                    {
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
            self.add_activity(format!(
                "File edits: {} (+{} -{})",
                file_path, lines_added, lines_removed
            ));
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

    fn render(
        &self,
        frame: &mut Frame,
        area: Rect,
        app_state: &mut AppState,
        theme: &Theme,
        is_selected: bool,
        card_index: usize,
    ) {
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
                    card_block.border_style(
                        Style::default().fg(theme.primary).add_modifier(Modifier::BOLD),
                    )
                } else {
                    card_block
                };

                let inner_area = final_card_block.inner(area);
                frame.render_widget(final_card_block, area);

                let is_stop_focused = matches!(app_state.focus_element, FocusElement::StopButton(idx) if idx == card_index);
                self.render_active_card(
                    frame,
                    inner_area,
                    theme,
                    is_stop_focused,
                    app_state.activity_lines_count,
                    app_state,
                    card_index,
                );
            }
            TaskState::Completed | TaskState::Merged => {
                let display_title = if self.title.len() > 40 {
                    format!("{}...", &self.title[..37])
                } else {
                    self.title.clone()
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

                self.render_completed_card(frame, inner_area, theme);
            }
        }
    }

    fn render_completed_card(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // Parse delivery indicators and apply proper colors
        let delivery_spans = if let Some(indicators) = &self.delivery_indicators {
            indicators
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
        } else {
            vec![Span::styled("⎇ br", Style::default().fg(theme.primary))]
        };

        let mut title_spans = vec![
            Span::styled("✓ ", theme.success_style().add_modifier(Modifier::BOLD)),
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

        let paragraph = Paragraph::new(vec![title_line, metadata_line]).wrap(Wrap { trim: true });

        frame.render_widget(paragraph, area);
    }

    fn render_active_card(
        &self,
        frame: &mut Frame,
        area: Rect,
        theme: &Theme,
        is_stop_focused: bool,
        activity_lines_count: usize,
        app_state: &mut AppState,
        card_index: usize,
    ) {
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
        let metadata_text = format!(
            "● {} • {} • {} • {}",
            self.repository, self.branch, agents_text, self.timestamp
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
        let stop_style = if is_stop_focused {
            Style::default().fg(theme.bg).bg(theme.error).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.error).bg(theme.surface).add_modifier(Modifier::BOLD)
        };
        line_spans.push(Span::styled(stop_button_text, stop_style));

        let stop_width = UnicodeWidthStr::width(stop_button_text);
        if stop_width > 0 {
            let stop_x = area.x.saturating_add(area.width.saturating_sub(stop_width as u16));
            app_state.interactive_areas.push(InteractiveArea {
                rect: Rect {
                    x: stop_x,
                    y: area.y,
                    width: stop_width as u16,
                    height: 1,
                },
                action: MouseAction::StopTask(card_index),
            });
        }

        let title_line = Line::from(line_spans);

        let activity_vec = self.get_recent_activity(activity_lines_count);
        let activity_lines: Vec<Line> = activity_vec
            .into_iter()
            .enumerate()
            .map(|(_, activity)| {
                if activity.trim().is_empty() {
                    // Empty activity - show a subtle placeholder
                    Line::from(vec![
                        Span::styled("  ", Style::default().fg(theme.muted)),
                        Span::styled("─", Style::default().fg(theme.border)),
                    ])
                } else {
                    let (prefix, content, color) = if activity.starts_with("Thoughts:") {
                        (
                            "💭",
                            activity
                                .strip_prefix("Thoughts:")
                                .unwrap_or(&activity)
                                .trim()
                                .to_string(),
                            theme.muted,
                        )
                    } else if activity.starts_with("Tool usage:") {
                        let tool_content =
                            activity.strip_prefix("Tool usage:").unwrap_or(&activity).trim();
                        let icon_color = if tool_content.contains("completed successfully") {
                            theme.success
                        } else if tool_content.contains("failed") {
                            theme.error
                        } else {
                            theme.primary
                        };
                        ("🔧", tool_content.to_string(), icon_color)
                    } else if activity.starts_with("  ") {
                        (
                            "  ",
                            activity.strip_prefix("  ").unwrap_or(&activity).to_string(),
                            theme.muted,
                        )
                    } else if activity.starts_with("File edits:") {
                        (
                            "📝",
                            activity
                                .strip_prefix("File edits:")
                                .unwrap_or(&activity)
                                .trim()
                                .to_string(),
                            theme.warning,
                        )
                    } else {
                        ("  ", activity, theme.text)
                    };

                    Line::from(vec![
                        Span::styled(prefix, Style::default().fg(color)),
                        Span::raw(" "),
                        Span::styled(content, Style::default().fg(theme.text)),
                    ])
                }
            })
            .collect();

        // Build all_lines dynamically based on activity_lines_count
        let mut all_lines = vec![title_line, Line::from("")]; // Title + empty separator line
        for i in 0..activity_lines_count {
            all_lines.push(activity_lines.get(i).cloned().unwrap_or_else(|| Line::from("")));
        }

        // Render each line individually with left padding
        for (i, line) in all_lines.iter().enumerate() {
            if i < area.height as usize {
                let line_area = Rect::new(
                    area.x + ACTIVE_TASK_LEFT_PADDING as u16,
                    area.y + i as u16,
                    area.width.saturating_sub(ACTIVE_TASK_LEFT_PADDING as u16),
                    1,
                );
                let para = Paragraph::new(line.clone());
                frame.render_widget(para, line_area);
            }
        }
    }

    fn render_draft_card(
        &self,
        frame: &mut Frame,
        area: Rect,
        app_state: &mut AppState,
        theme: &Theme,
    ) {
        // Draft cards render directly without outer border like ah-tui
        self.render_draft_card_content(frame, area, app_state, theme);
    }

    fn render_draft_card_content(
        &self,
        frame: &mut Frame,
        area: Rect,
        app_state: &mut AppState,
        theme: &Theme,
    ) {
        let content_height = area.height as usize;

        // Split the available area between textarea and buttons
        let button_height: usize = 1; // Single line for buttons
        let separator_height: usize = 1; // Empty line between
        let padding_total = TEXTAREA_TOP_PADDING + TEXTAREA_BOTTOM_PADDING;
        let available_content = content_height.saturating_sub(button_height + separator_height);
        let available_inner = available_content.saturating_sub(padding_total).max(1);
        let desired_lines =
            app_state.task_description.lines().len().max(MIN_TEXTAREA_VISIBLE_LINES);
        let visible_lines = desired_lines.min(available_inner).max(1);

        let textarea_inner_height = visible_lines as u16;
        let textarea_total_height = (visible_lines + padding_total) as u16;

        // Add configurable left padding for textarea and buttons
        let textarea_area = Rect {
            x: area.x + TEXTAREA_LEFT_PADDING as u16,
            y: area.y + TEXTAREA_TOP_PADDING as u16,
            width: area
                .width
                .saturating_sub((TEXTAREA_LEFT_PADDING + TEXTAREA_RIGHT_PADDING) as u16),
            height: textarea_inner_height,
        };

        let button_area = Rect {
            x: area.x + BUTTON_LEFT_PADDING as u16,
            y: area.y + textarea_total_height + separator_height as u16,
            width: area.width.saturating_sub(BUTTON_LEFT_PADDING as u16),
            height: button_height as u16,
        };

        // Render padding areas around textarea
        let padding_style = Style::default().bg(theme.bg);

        if TEXTAREA_TOP_PADDING > 0 {
            let top_padding_area = Rect {
                x: area.x,
                y: area.y,
                width: area.width,
                height: TEXTAREA_TOP_PADDING as u16,
            };
            frame.render_widget(Paragraph::new("").style(padding_style), top_padding_area);
        }

        if TEXTAREA_BOTTOM_PADDING > 0 {
            let bottom_padding_area = Rect {
                x: area.x,
                y: area
                    .y
                    .saturating_add(TEXTAREA_TOP_PADDING as u16)
                    .saturating_add(textarea_inner_height),
                width: area.width,
                height: TEXTAREA_BOTTOM_PADDING as u16,
            };
            frame.render_widget(Paragraph::new("").style(padding_style), bottom_padding_area);
        }

        if TEXTAREA_LEFT_PADDING > 0 {
            let left_padding_area = Rect {
                x: area.x,
                y: area.y + TEXTAREA_TOP_PADDING as u16,
                width: TEXTAREA_LEFT_PADDING as u16,
                height: textarea_inner_height,
            };
            frame.render_widget(Paragraph::new("").style(padding_style), left_padding_area);
        }

        if TEXTAREA_RIGHT_PADDING > 0 {
            let right_padding_area = Rect {
                x: area.x + area.width.saturating_sub(TEXTAREA_RIGHT_PADDING as u16),
                y: area.y + TEXTAREA_TOP_PADDING as u16,
                width: TEXTAREA_RIGHT_PADDING as u16,
                height: textarea_inner_height,
            };
            frame.render_widget(Paragraph::new("").style(padding_style), right_padding_area);
        }

        // Render left padding for buttons
        if BUTTON_LEFT_PADDING > 0 {
            let button_left_padding = Rect {
                x: area.x,
                y: button_area.y,
                width: BUTTON_LEFT_PADDING as u16,
                height: button_area.height,
            };
            frame.render_widget(Paragraph::new("").style(padding_style), button_left_padding);
        }

        // Render the textarea
        frame.render_widget(&app_state.task_description, textarea_area);
        app_state.last_textarea_area.set(Some(textarea_area));

        if matches!(app_state.focus_element, FocusElement::TaskDescription)
            && matches!(app_state.modal_state, ModalState::None)
            && app_state.filter_editor.is_none()
        {
            let metrics = compute_caret_metrics(&app_state.task_description, textarea_area);
            frame.set_cursor_position(ratatui::layout::Position::new(
                metrics.caret_x,
                metrics.caret_y,
            ));
        }

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
            format!(
                "🤖 {} (x{})",
                app_state.selected_models[0].name, app_state.selected_models[0].count
            )
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

        let mut cursor = button_area.x as usize;
        let repo_label = format!(" {} ", repo_button_text);
        let repo_width = UnicodeWidthStr::width(repo_label.as_str());
        app_state.interactive_areas.push(InteractiveArea {
            rect: Rect {
                x: cursor as u16,
                y: button_area.y,
                width: repo_width as u16,
                height: 1,
            },
            action: MouseAction::OpenRepositoryModal,
        });
        cursor += repo_width + 1; // account for separator space

        let branch_label = format!(" {} ", branch_button_text);
        let branch_width = UnicodeWidthStr::width(branch_label.as_str());
        app_state.interactive_areas.push(InteractiveArea {
            rect: Rect {
                x: cursor as u16,
                y: button_area.y,
                width: branch_width as u16,
                height: 1,
            },
            action: MouseAction::OpenBranchModal,
        });
        cursor += branch_width + 1;

        let models_label = format!(" {} ", models_button_text);
        let models_width = UnicodeWidthStr::width(models_label.as_str());
        app_state.interactive_areas.push(InteractiveArea {
            rect: Rect {
                x: cursor as u16,
                y: button_area.y,
                width: models_width as u16,
                height: 1,
            },
            action: MouseAction::OpenModelModal,
        });
        cursor += models_width + 1;

        let go_label = format!(" {} ", go_button_text);
        let go_width = UnicodeWidthStr::width(go_label.as_str());
        app_state.interactive_areas.push(InteractiveArea {
            rect: Rect {
                x: cursor as u16,
                y: button_area.y,
                width: go_width as u16,
                height: 1,
            },
            action: MouseAction::ActivateGoButton,
        });
    }
}

fn render_header(
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    app_state: &mut AppState,
    _image_picker: Option<&Picker>,
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

        let button_style = if matches!(app_state.focus_element, FocusElement::SettingsButton) {
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

        app_state.interactive_areas.push(InteractiveArea {
            rect: button_area,
            action: MouseAction::OpenSettings,
        });
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
    let dialog_height = 20.min(area.height - 4);

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
            Line::from(Span::styled(
                "─".repeat(line_width),
                Style::default().fg(theme.border),
            ))
        } else {
            let left_len = (line_width - title_len) / 2;
            let right_len = line_width - title_len - left_len;

            Line::from(vec![
                Span::styled("─".repeat(left_len), Style::default().fg(theme.border)),
                Span::styled(
                    title_with_spaces,
                    Style::default().fg(theme.primary).add_modifier(Modifier::BOLD),
                ),
                Span::styled("─".repeat(right_len), Style::default().fg(theme.border)),
            ])
        }
    };

    // Content
    let form = &app_state.settings_form;
    let mut row: usize = 0;

    render_settings_line(frame, inner_area, &mut row, Line::from(""));
    render_settings_line(
        frame,
        inner_area,
        &mut row,
        create_section_line("Activity Lines"),
    );
    render_settings_field(
        frame,
        inner_area,
        &mut row,
        "Activity lines (1-3)",
        SettingsField::ActivityLines,
        form,
        theme,
    );
    render_settings_line(frame, inner_area, &mut row, Line::from(""));
    render_settings_line(
        frame,
        inner_area,
        &mut row,
        create_section_line("Autocomplete"),
    );
    render_settings_field(
        frame,
        inner_area,
        &mut row,
        "Autocomplete border (on/off)",
        SettingsField::AutocompleteBorder,
        form,
        theme,
    );
    render_settings_field(
        frame,
        inner_area,
        &mut row,
        "Autocomplete background (#RRGGBB)",
        SettingsField::AutocompleteBackground,
        form,
        theme,
    );
    render_settings_field(
        frame,
        inner_area,
        &mut row,
        "Word wrap (on/off)",
        SettingsField::WordWrap,
        form,
        theme,
    );
    render_settings_line(frame, inner_area, &mut row, Line::from(""));
    render_settings_line(
        frame,
        inner_area,
        &mut row,
        create_section_line("Shortcuts"),
    );
    render_settings_field(
        frame,
        inner_area,
        &mut row,
        "Launch task",
        SettingsField::LaunchShortcut,
        form,
        theme,
    );
    render_settings_field(
        frame,
        inner_area,
        &mut row,
        "Insert newline",
        SettingsField::NewLineShortcut,
        form,
        theme,
    );
    render_settings_field(
        frame,
        inner_area,
        &mut row,
        "Next field",
        SettingsField::NextFieldShortcut,
        form,
        theme,
    );
    render_settings_field(
        frame,
        inner_area,
        &mut row,
        "Previous field",
        SettingsField::PrevFieldShortcut,
        form,
        theme,
    );
    render_settings_field(
        frame,
        inner_area,
        &mut row,
        "Shortcut help dialog",
        SettingsField::ShortcutHelpShortcut,
        form,
        theme,
    );
    render_settings_line(frame, inner_area, &mut row, Line::from(""));
    render_settings_line(frame, inner_area, &mut row, create_section_line("Controls"));
    render_settings_line(
        frame,
        inner_area,
        &mut row,
        Line::from(vec![
            Span::raw("  "),
            Span::styled("Tab", theme.warning_style()),
            Span::raw(" next field  •  "),
            Span::styled("Shift+Tab", theme.warning_style()),
            Span::raw(" previous"),
        ]),
    );
    render_settings_line(
        frame,
        inner_area,
        &mut row,
        Line::from(vec![
            Span::raw("  "),
            Span::styled("Enter", theme.success_style()),
            Span::raw(" apply changes  •  "),
            Span::styled("Esc", theme.error_style()),
            Span::raw(" close"),
        ]),
    );
}

fn render_settings_line(frame: &mut Frame, area: Rect, row: &mut usize, line: Line) {
    if *row >= area.height as usize {
        return;
    }
    let line_area = Rect::new(area.x, area.y + *row as u16, area.width, 1);
    frame.render_widget(Paragraph::new(line), line_area);
    *row += 1;
}

fn render_settings_field(
    frame: &mut Frame,
    area: Rect,
    row: &mut usize,
    label: &str,
    field: SettingsField,
    form: &SettingsForm,
    theme: &Theme,
) {
    if *row >= area.height as usize {
        return;
    }
    let line_area = Rect::new(area.x, area.y + *row as u16, area.width, 1);
    let input = match field {
        SettingsField::ActivityLines => &form.activity_lines_input,
        SettingsField::AutocompleteBorder => &form.autocomplete_border_input,
        SettingsField::AutocompleteBackground => &form.autocomplete_bg_input,
        SettingsField::WordWrap => &form.word_wrap_input,
        SettingsField::LaunchShortcut => &form.launch_shortcut_input,
        SettingsField::NewLineShortcut => &form.new_line_shortcut_input,
        SettingsField::NextFieldShortcut => &form.next_field_shortcut_input,
        SettingsField::PrevFieldShortcut => &form.prev_field_shortcut_input,
        SettingsField::ShortcutHelpShortcut => &form.shortcut_help_input,
    };
    let label_text = format!("{}: ", label);
    let label_span = Span::styled(label_text.clone(), theme.text_style());
    let value_style = if form.focused_field == field {
        theme.focused_style()
    } else {
        theme.text_style()
    };
    let value_span = Span::styled(input.value().to_string(), value_style);
    frame.render_widget(
        Paragraph::new(Line::from(vec![label_span, value_span])),
        line_area,
    );

    if form.focused_field == field {
        let cursor_offset = label_text.chars().count() as u16 + input.visual_cursor() as u16;
        let cursor_x =
            (line_area.x + cursor_offset).min(line_area.x + line_area.width.saturating_sub(1));
        frame.set_cursor_position(Position::new(cursor_x, line_area.y));
    }

    *row += 1;
}

fn render_filter_bar(frame: &mut Frame, area: Rect, app_state: &mut AppState, theme: &Theme) {
    for slot in &mut app_state.filter_button_rects {
        *slot = None;
    }

    let focused_control = match app_state.focus_element {
        FocusElement::Filter(control) => Some(control),
        _ => None,
    };
    let active_editor = app_state.filter_editor.as_ref().map(|editor| editor.control);
    let filter_bar_focused = matches!(app_state.focus_element, FocusElement::FilterBarLine);

    let repo_label = app_state.filter_state.repository_label(&app_state.filter_options).to_string();
    let status_label = app_state.filter_state.status_label().to_string();
    let creator_label = app_state.filter_state.creator_label(&app_state.filter_options).to_string();

    fn push_span(spans: &mut Vec<Span>, consumed: &mut usize, text: &str, style: Style) {
        *consumed += UnicodeWidthStr::width(text);
        spans.push(Span::styled(text.to_string(), style));
    }

    fn render_filter_value(
        spans: &mut Vec<Span>,
        consumed: &mut usize,
        app_state: &mut AppState,
        base_x: usize,
        line_y: u16,
        label: &str,
        control: FilterControl,
        style: Style,
        active_editor: Option<FilterControl>,
    ) {
        let display = format!("[{}]", label);
        let width = UnicodeWidthStr::width(display.as_str());
        let rect = Rect {
            x: (base_x + *consumed) as u16,
            y: line_y,
            width: width as u16,
            height: 1,
        };

        if let Some(editor) = app_state.filter_editor.as_mut() {
            if editor.control == control {
                editor.anchor = rect;
            }
        }
        app_state.filter_button_rects[control.index()] = Some(rect);

        *consumed += width;

        if active_editor == Some(control) {
            spans.push(Span::styled(" ".repeat(width), style));
        } else {
            spans.push(Span::styled(display, style));
            app_state.interactive_areas.push(InteractiveArea {
                rect,
                action: MouseAction::EditFilter(control),
            });
        }
    }

    let mut spans: Vec<Span> = Vec::new();
    let mut consumed = 0usize;
    let start_x = area.x as usize;

    // Use the selected task border color when FilterBarLine is focused
    let border_style = if filter_bar_focused {
        Style::default().fg(theme.border_focused)
    } else {
        Style::default().fg(theme.border)
    };
    let header_style = if filter_bar_focused {
        Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.muted)
    };

    push_span(&mut spans, &mut consumed, "─ ", border_style);
    push_span(
        &mut spans,
        &mut consumed,
        "Existing tasks",
        header_style.add_modifier(Modifier::BOLD),
    );
    push_span(&mut spans, &mut consumed, "  ", Style::default());

    let repo_style = if focused_control == Some(FilterControl::Repository) {
        theme.focused_style()
    } else {
        Style::default().fg(theme.text)
    };
    push_span(&mut spans, &mut consumed, "Repo ", header_style);
    render_filter_value(
        &mut spans,
        &mut consumed,
        app_state,
        start_x,
        area.y,
        repo_label.as_str(),
        FilterControl::Repository,
        repo_style,
        active_editor,
    );

    push_span(&mut spans, &mut consumed, "  ", Style::default());

    let status_style = if focused_control == Some(FilterControl::Status) {
        theme.focused_style()
    } else {
        Style::default().fg(theme.text)
    };
    push_span(&mut spans, &mut consumed, "Status ", header_style);
    render_filter_value(
        &mut spans,
        &mut consumed,
        app_state,
        start_x,
        area.y,
        status_label.as_str(),
        FilterControl::Status,
        status_style,
        active_editor,
    );

    push_span(&mut spans, &mut consumed, "  ", Style::default());

    let creator_style = if focused_control == Some(FilterControl::Creator) {
        theme.focused_style()
    } else {
        Style::default().fg(theme.text)
    };
    push_span(&mut spans, &mut consumed, "Creator ", header_style);
    render_filter_value(
        &mut spans,
        &mut consumed,
        app_state,
        start_x,
        area.y,
        creator_label.as_str(),
        FilterControl::Creator,
        creator_style,
        active_editor,
    );

    let line_width = area.width as usize;
    if consumed < line_width {
        let remaining = line_width - consumed;
        push_span(
            &mut spans,
            &mut consumed,
            &"─".repeat(remaining),
            border_style,
        );
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);

    // Note: filter editor rendering is now done separately at the end of the draw loop
    // to ensure it appears on top of all other UI elements
}

fn render_filter_editor(frame: &mut Frame, theme: &Theme, editor: &mut FilterEditor) {
    let frame_area = frame.area();
    let content_width = editor.anchor.width.max(16);
    let mut block_width = content_width.saturating_add(2);
    if block_width > frame_area.width {
        block_width = frame_area.width;
    }

    let max_list_rows =
        frame_area.height.saturating_sub(editor.anchor.y.saturating_add(2)).max(1) as usize;
    let desired_rows = editor.filtered.len().max(1);
    let list_rows = desired_rows.min(max_list_rows);
    let content_height = (list_rows as u16).saturating_add(2);
    let mut block_height = content_height.saturating_add(2);
    if block_height < 5 {
        block_height = 5;
    }
    if block_height > frame_area.height {
        block_height = frame_area.height;
    }

    let mut block_x = editor.anchor.x.saturating_sub(1);
    if block_x + block_width > frame_area.width {
        block_x = frame_area.width.saturating_sub(block_width);
    }
    let mut block_y = editor.anchor.y.saturating_sub(1);
    if block_y + block_height > frame_area.height {
        block_y = frame_area.height.saturating_sub(block_height);
    }

    let block_area = Rect {
        x: block_x,
        y: block_y,
        width: block_width,
        height: block_height,
    };

    // Use the same approach as autocomplete: clear the area and fill with solid background
    frame.render_widget(Clear, block_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_focused))
        .style(Style::default().bg(theme.surface));

    let inner = block.inner(block_area);
    frame.render_widget(block, block_area);

    let input_area = Rect {
        x: inner.x,
        y: inner.y,
        width: inner.width,
        height: 1,
    };
    let separator_area = Rect {
        x: inner.x,
        y: inner.y.saturating_add(1),
        width: inner.width,
        height: 1,
    };
    let list_area = Rect {
        x: inner.x,
        y: inner.y.saturating_add(2),
        width: inner.width,
        height: inner.height.saturating_sub(2),
    };

    let input_value = editor.input.value();
    let display_value = if input_value.is_empty() {
        Span::styled("Type to filter…", theme.muted_style())
    } else {
        Span::styled(input_value, theme.text_style())
    };
    frame.render_widget(Paragraph::new(Line::from(display_value)), input_area);

    let cursor_col = editor.input.visual_cursor() as u16;
    let cursor_x =
        (input_area.x + cursor_col).min(input_area.x + input_area.width.saturating_sub(1));
    frame.set_cursor_position(Position::new(cursor_x, input_area.y));

    if list_area.height > 0 {
        let separator = Line::from(Span::styled(
            "─".repeat(input_area.width as usize),
            Style::default().fg(theme.border),
        ));
        frame.render_widget(Paragraph::new(separator), separator_area);
    }

    let visible_capacity = list_area.height.max(1) as usize;
    let total = editor.filtered.len();
    let start = if editor.selected >= visible_capacity {
        editor.selected + 1 - visible_capacity
    } else {
        0
    };
    let end = (start + visible_capacity).min(total);

    let mut lines: Vec<Line> = Vec::new();
    for (row, filtered_idx) in editor.filtered[start..end].iter().enumerate() {
        let label = &editor.options[*filtered_idx];
        let style = if start + row == editor.selected {
            theme.selected_style()
        } else {
            theme.text_style()
        };
        lines.push(Line::from(Span::styled(label.clone(), style)));
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled("No matches", theme.muted_style())));
    }

    frame.render_widget(Paragraph::new(lines), list_area);
}

fn rect_contains(rect: Rect, x: u16, y: u16) -> bool {
    x >= rect.x
        && y >= rect.y
        && x < rect.x.saturating_add(rect.width)
        && y < rect.y.saturating_add(rect.height)
}

fn render_footer(
    frame: &mut Frame,
    area: Rect,
    app_state: &mut AppState,
    tasks: &[TaskCard],
    theme: &Theme,
) {
    let mut footer_area = area;
    if area.width >= 4 {
        let horizontal_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(2), Constraint::Min(1)])
            .split(area);
        footer_area = horizontal_chunks[1];
    }

    let mut spans: Vec<Span> = Vec::new();
    let mut cursor_x: usize = 0;
    let bullet = " • ";
    let bullet_width = UnicodeWidthStr::width(bullet);

    let hints = app_state.build_footer_hints(tasks, theme);

    for (index, hint) in hints.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled(
                bullet.to_string(),
                Style::default().fg(theme.muted),
            ));
            cursor_x += bullet_width;
        }

        spans.push(Span::styled(hint.key.clone(), hint.key_style));
        let key_width = UnicodeWidthStr::width(hint.key.as_str());
        cursor_x += key_width;

        spans.push(Span::raw(" "));
        cursor_x += 1;

        let desc_width = UnicodeWidthStr::width(hint.description.as_str());
        let desc_start = cursor_x;
        spans.push(Span::styled(
            hint.description.clone(),
            hint.description_style,
        ));
        cursor_x += desc_width;

        if let Some(action) = hint.action {
            if desc_width > 0 {
                let rect = Rect {
                    x: footer_area.x.saturating_add(desc_start as u16),
                    y: footer_area.y,
                    width: desc_width as u16,
                    height: 1,
                };
                app_state.interactive_areas.push(InteractiveArea {
                    rect,
                    action: MouseAction::Footer(action),
                });
            }
        }
    }

    if let Some(status) = app_state.status_message.as_deref() {
        if !spans.is_empty() {
            spans.push(Span::styled(
                bullet.to_string(),
                Style::default().fg(theme.muted),
            ));
        }
        spans.push(Span::styled(
            status.to_string(),
            Style::default().fg(theme.muted).add_modifier(Modifier::ITALIC),
        ));
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(theme.bg)),
        footer_area,
    );
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

    let input_paragraph = Paragraph::new(Line::from(display_value)).wrap(Wrap { trim: true });
    frame.render_widget(input_paragraph, input_area);

    // Show cursor
    if !input_value.is_empty() {
        let visual_cursor = modal.input.visual_cursor();
        let cursor_x = input_area.x as u16 + visual_cursor as u16;
        let cursor_y = input_area.y as u16;
        if cursor_x < input_area.x as u16 + input_area.width as u16
            && cursor_y < input_area.y as u16 + input_area.height as u16
        {
            frame.set_cursor_position(ratatui::layout::Position::new(cursor_x, cursor_y));
        }
    }

    // Render separator line
    let separator_line = Line::from(vec![Span::styled(
        "─".repeat(separator_area.width as usize),
        Style::default().fg(theme.border),
    )]);
    frame.render_widget(Paragraph::new(separator_line), separator_area);

    // Filter options based on input
    let query = modal.input.value();
    let filtered_options: Vec<&String> = if query.is_empty() {
        modal.options.iter().take(10).collect()
    } else {
        modal
            .options
            .iter()
            .filter(|opt| opt.to_lowercase().contains(&query.to_lowercase()))
            .take(10)
            .collect()
    };

    // Display filtered results directly in results area
    let result_lines: Vec<Line> = filtered_options
        .iter()
        .enumerate()
        .map(|(i, opt)| {
            let style = if i == modal.selected_index {
                theme.selected_style()
            } else {
                Style::default().fg(theme.text)
            };
            Line::from(opt.as_str()).style(style)
        })
        .collect();

    frame.render_widget(
        Paragraph::new(result_lines).wrap(Wrap { trim: true }),
        results_area,
    );
}

fn render_model_selection_modal(
    frame: &mut Frame,
    modal: &ModelSelectionModal,
    area: Rect,
    theme: &Theme,
) {
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
        Span::raw(" Select Models ")
            .style(Style::default().fg(theme.text).add_modifier(Modifier::BOLD)),
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
            Constraint::Length(1),                                      // Separator
            Constraint::Min(1),                                         // Available models section
        ])
        .split(inner_area);

    let selected_area = vertical_chunks[0];
    let separator_area = vertical_chunks[1];
    let available_area = vertical_chunks[2];

    // Render selected models
    let mut selected_lines = vec![Line::from(vec![Span::styled(
        "Selected Models:",
        Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
    )])];

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
        selected_lines.push(Line::from(vec![Span::styled(
            "  (none selected)",
            Style::default().fg(theme.muted),
        )]));
    }

    frame.render_widget(
        Paragraph::new(selected_lines).wrap(Wrap { trim: true }),
        selected_area,
    );

    // Render separator
    let separator_line = Line::from(vec![Span::styled(
        "─".repeat(separator_area.width as usize),
        Style::default().fg(theme.border),
    )]);
    frame.render_widget(Paragraph::new(separator_line), separator_area);

    // Render available models
    let mut available_lines = vec![Line::from(vec![Span::styled(
        "Available Models (↑↓ to navigate, Enter to add):",
        Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
    )])];

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
        available_area,
    );
}

fn render_go_to_line_modal(frame: &mut Frame, modal: &GoToLineModal, area: Rect, theme: &Theme) {
    let modal_width = 40.min(area.width.saturating_sub(4));
    let modal_height = 7.min(area.height.saturating_sub(4));

    let modal_area = Rect {
        x: area.x + (area.width.saturating_sub(modal_width)) / 2,
        y: area.y + (area.height.saturating_sub(modal_height)) / 2,
        width: modal_width,
        height: modal_height,
    };

    let mut shadow_area = modal_area;
    shadow_area.x = shadow_area.x.saturating_add(1);
    shadow_area.y = shadow_area.y.saturating_add(1);
    frame.render_widget(Clear, shadow_area);
    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Rgb(10, 10, 15))),
        shadow_area,
    );

    let title_line = Line::from(vec![
        Span::styled("", Style::default().fg(theme.primary)),
        Span::styled(
            " Go To Line ",
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Span::styled("", Style::default().fg(theme.primary)),
    ]);

    let block = Block::default()
        .title(title_line)
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_focused))
        .style(Style::default().bg(theme.surface))
        .padding(Padding::new(1, 1, 0, 0));

    frame.render_widget(Clear, modal_area);
    let inner = block.inner(modal_area);
    frame.render_widget(block, modal_area);

    let mut lines = Vec::new();
    lines.push(Line::from(vec![Span::styled(
        format!("Enter a line number (1-{})", modal.max_line),
        Style::default().fg(theme.muted),
    )]));

    let mut input_spans = vec![
        Span::styled("> ", Style::default().fg(theme.muted)),
        Span::styled(modal.input.value(), Style::default().fg(theme.text)),
    ];
    input_spans.push(Span::styled("▏", Style::default().fg(theme.primary)));
    lines.push(Line::from(input_spans));

    lines.push(Line::from(vec![Span::styled(
        "Press Enter to jump • Esc to cancel",
        Style::default().fg(theme.muted),
    )]));

    if let Some(error) = &modal.error {
        lines.push(Line::from(Span::styled(
            error.as_str(),
            theme.error_style(),
        )));
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

fn render_find_replace_modal(
    frame: &mut Frame,
    modal: &FindReplaceModal,
    area: Rect,
    theme: &Theme,
) {
    let modal_width = 60.min(area.width.saturating_sub(4));
    let modal_height = 9.min(area.height.saturating_sub(4));

    let modal_area = Rect {
        x: area.x + (area.width.saturating_sub(modal_width)) / 2,
        y: area.y + (area.height.saturating_sub(modal_height)) / 2,
        width: modal_width,
        height: modal_height,
    };

    let mut shadow_area = modal_area;
    shadow_area.x = shadow_area.x.saturating_add(1);
    shadow_area.y = shadow_area.y.saturating_add(1);
    frame.render_widget(Clear, shadow_area);
    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Rgb(10, 10, 15))),
        shadow_area,
    );

    let title = if modal.is_regex {
        " Find & Replace (Regex) "
    } else {
        " Find & Replace "
    };
    let title_line = Line::from(vec![
        Span::styled("", Style::default().fg(theme.primary)),
        Span::styled(
            title,
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Span::styled("", Style::default().fg(theme.primary)),
    ]);

    let block = Block::default()
        .title(title_line)
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_focused))
        .style(Style::default().bg(theme.surface))
        .padding(Padding::new(1, 1, 0, 0));

    frame.render_widget(Clear, modal_area);
    let inner = block.inner(modal_area);
    frame.render_widget(block, modal_area);

    let mut lines = Vec::new();

    lines.push(Line::from(Span::styled(
        "Provide search and replacement text",
        Style::default().fg(theme.muted),
    )));

    let search_active = modal.stage == FindReplaceStage::EnterSearch;
    lines.push(Line::from(render_modal_input_line(
        "Find",
        modal.search_input.value(),
        search_active,
        theme,
    )));

    let replace_active = modal.stage == FindReplaceStage::EnterReplacement;
    lines.push(Line::from(render_modal_input_line(
        "Replace",
        modal.replace_input.value(),
        replace_active,
        theme,
    )));

    lines.push(Line::from(vec![Span::styled(
        "Enter to continue • Tab to switch field • Esc to cancel",
        Style::default().fg(theme.muted),
    )]));

    if let Some(error) = &modal.error {
        lines.push(Line::from(Span::styled(
            error.as_str(),
            theme.error_style(),
        )));
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

fn render_shortcut_help_modal(
    frame: &mut Frame,
    modal: &ShortcutHelpModal,
    area: Rect,
    theme: &Theme,
) {
    let modal_width = 70.min(area.width.saturating_sub(4));
    let modal_height = 18.min(area.height.saturating_sub(4));

    let modal_area = Rect {
        x: area.x + (area.width.saturating_sub(modal_width)) / 2,
        y: area.y + (area.height.saturating_sub(modal_height)) / 2,
        width: modal_width,
        height: modal_height,
    };

    frame.render_widget(Clear, modal_area);

    let title_line = Line::from(vec![
        Span::styled("", Style::default().fg(theme.primary)),
        Span::styled(
            " Shortcut Help ",
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Span::styled("", Style::default().fg(theme.primary)),
    ]);

    let block = Block::default()
        .title(title_line)
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_focused))
        .style(Style::default().bg(theme.surface));

    let inner = block.inner(modal_area);
    frame.render_widget(block, modal_area);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let list_area = layout[0];
    let info_area = layout[1];

    let visible_capacity = list_area.height as usize;
    let entries = modal.entries.iter().skip(modal.scroll).take(visible_capacity);

    let mut lines: Vec<Line> = Vec::new();
    for entry in entries {
        let binding_text = entry.bindings.join(" / ");
        let content = format!("{: <18} {}", binding_text, entry.description);
        lines.push(Line::from(Span::styled(
            content,
            Style::default().fg(theme.text),
        )));
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "No shortcuts available",
            Style::default().fg(theme.muted),
        )));
    }

    frame.render_widget(Paragraph::new(lines), list_area);

    let info_text = "Esc Close • ↑↓ Scroll";
    frame.render_widget(
        Paragraph::new(info_text)
            .style(Style::default().fg(theme.muted))
            .alignment(Alignment::Center),
        info_area,
    );
}

fn render_modal_input_line(
    label: &str,
    value: &str,
    active: bool,
    theme: &Theme,
) -> Vec<Span<'static>> {
    let mut spans = vec![
        Span::styled(label.to_string(), Style::default().fg(theme.muted)),
        Span::raw(": ".to_string()),
        Span::styled(
            value.to_string(),
            if active {
                Style::default().fg(theme.text).add_modifier(Modifier::UNDERLINED)
            } else {
                Style::default().fg(theme.text)
            },
        ),
    ];
    if active {
        spans.push(Span::styled(
            "▏".to_string(),
            Style::default().fg(theme.primary),
        ));
    }
    spans
}

fn create_sample_tasks() -> Vec<TaskCard> {
    vec![
        TaskCard {
            title: "".to_string(), // Will be filled by user input
            repository: "agent-harbor".to_string(),
            branch: "main".to_string(),
            agents: vec![SelectedModel {
                name: "claude-3-5-sonnet".to_string(),
                count: 1,
            }],
            timestamp: "now".to_string(),
            state: TaskState::Draft,
            activity: vec![], // Empty for draft
            delivery_indicators: None,
            current_tool_execution: None,
            creator: "You".to_string(),
        },
        TaskCard {
            title: "Implement payment processing".to_string(),
            repository: "ecommerce-platform".to_string(),
            branch: "feature/payments".to_string(),
            agents: vec![SelectedModel {
                name: "claude-3-5-sonnet".to_string(),
                count: 1,
            }],
            timestamp: "5 min ago".to_string(),
            state: TaskState::Active,
            activity: vec![
                "Thoughts: Analyzing payment flow requirements".to_string(),
                "Tool usage: read_file".to_string(),
                "  Reading payment service contracts".to_string(),
            ],
            delivery_indicators: None,
            current_tool_execution: None,
            creator: "Priya".to_string(),
        },
        TaskCard {
            title: "Optimize database queries for user dashboard performance".to_string(),
            repository: "analytics-platform".to_string(),
            branch: "perf/dashboard-queries".to_string(),
            agents: vec![SelectedModel {
                name: "gpt-4".to_string(),
                count: 1,
            }],
            timestamp: "25 min ago".to_string(),
            state: TaskState::Active,
            activity: vec![
                "Thoughts: Identifying N+1 query issues in dashboard components".to_string(),
                "Tool usage: read_file".to_string(),
                "  Examining dashboard query patterns".to_string(),
            ],
            delivery_indicators: None,
            current_tool_execution: None,
            creator: "Miguel".to_string(),
        },
        TaskCard {
            title: "Add user authentication and session management".to_string(),
            repository: "web-app".to_string(),
            branch: "feature/user-auth".to_string(),
            agents: vec![SelectedModel {
                name: "claude-3-5-sonnet".to_string(),
                count: 1,
            }],
            timestamp: "2 hours ago".to_string(),
            state: TaskState::Completed,
            activity: vec![],
            delivery_indicators: Some("⎇ ✓".to_string()), // Branch exists + PR merged
            current_tool_execution: None,
            creator: "Allison".to_string(),
        },
        TaskCard {
            title: "Implement payment processing with Stripe integration".to_string(),
            repository: "ecommerce-platform".to_string(),
            branch: "feature/stripe-payment".to_string(),
            agents: vec![SelectedModel {
                name: "gpt-4".to_string(),
                count: 1,
            }],
            timestamp: "4 hours ago".to_string(),
            state: TaskState::Merged,
            activity: vec![],
            delivery_indicators: Some("⎇ ⇄ ✓".to_string()), // Branch exists + PR exists + PR merged
            current_tool_execution: None,
            creator: "Lina".to_string(),
        },
        TaskCard {
            title: "Add comprehensive error logging and monitoring".to_string(),
            repository: "backend-api".to_string(),
            branch: "feature/error-monitoring".to_string(),
            agents: vec![SelectedModel {
                name: "claude-3-5-sonnet".to_string(),
                count: 1,
            }],
            timestamp: "6 hours ago".to_string(),
            state: TaskState::Completed,
            activity: vec![],
            delivery_indicators: Some("⎇".to_string()), // Branch exists only
            current_tool_execution: None,
            creator: "Zahra".to_string(),
        },
    ]
}

impl AppState {
    fn new() -> Self {
        let mut textarea = TextArea::default();
        textarea.set_placeholder_text("Describe what you want the agent to do...");
        textarea.set_cursor_line_style(Style::default());
        textarea.set_placeholder_style(Style::default().fg(Color::DarkGray));
        textarea.set_word_wrap(true);
        // Make cursor invisible in placeholder mode by using same style as placeholder

        let default_theme = Theme::default();
        let shortcut_config = InMemoryShortcutConfig::new();
        let mut state = Self {
            selected_card: 0,
            focus_element: FocusElement::TaskDescription,
            modal_state: ModalState::None,
            search_mode: SearchMode::None,
            fuzzy_modal: None,
            model_selection_modal: None,
            goto_line_modal: None,
            find_replace_modal: None,
            task_description: textarea,
            word_wrap_enabled: true,
            selected_repository: "agent-harbor".to_string(),
            selected_branch: "main".to_string(),
            selected_models: vec![SelectedModel {
                name: "claude-3-5-sonnet".to_string(),
                count: 1,
            }],
            activity_timer: Instant::now(),
            activity_lines_count: 3, // Default to 3 activity lines
            autocomplete: InlineAutocomplete::new(),
            last_textarea_area: Cell::new(None),
            show_autocomplete_border: true,
            autocomplete_background: default_theme.surface,
            settings_form: SettingsForm::new(
                3,
                true,
                default_theme.surface,
                true,
                &shortcut_config,
            ),
            kill_ring: KillRing::new(32),
            clipboard: SystemClipboard::new(),
            status_message: None,
            shortcut_config,
            filter_options: FilterOptions {
                repositories: vec!["All".to_string()],
                creators: vec!["All".to_string()],
            },
            filter_state: FilterState::default(),
            interactive_areas: Vec::new(),
            shortcut_help_modal: None,
            filter_button_rects: [None, None, None],
            filter_editor: None,
            scroll_offset: 0,
        };
        state.set_autocomplete_border(true);
        state.set_autocomplete_background(default_theme.surface);
        state
    }

    fn refresh_autocomplete(&mut self, input_changed: bool) {
        if input_changed {
            self.autocomplete.notify_text_input();
        }
        self.autocomplete.after_textarea_change(&self.task_description);
    }

    fn apply_command_effect(&mut self, effect: CommandEffect) {
        if effect.text_changed {
            self.refresh_autocomplete(true);
        } else if effect.caret_moved {
            self.refresh_autocomplete(false);
        }

        if let Some(action) = effect.action {
            match action {
                CommandAction::OpenGoToLineDialog => self.open_go_to_line_modal(),
                CommandAction::OpenFindReplace { is_regex } => {
                    self.open_find_replace_modal(is_regex)
                }
            }
        }

        if let Some(message) = effect.status_message {
            self.status_message = Some(message);
        } else if effect.text_changed {
            self.status_message = None;
        }
    }

    fn extend_selection(&mut self, movement: tui_textarea::CursorMove) {
        if self.task_description.selection_range().is_none() {
            self.task_description.start_selection();
        }
        self.task_description.move_cursor(movement);
        self.refresh_autocomplete(false);
    }

    fn poll_autocomplete(&mut self) {
        self.autocomplete.poll_results();
    }

    fn autocomplete_on_tick(&mut self) {
        self.autocomplete.on_tick();
    }

    fn rebuild_settings_form(&mut self, focused: SettingsField) {
        self.settings_form = SettingsForm::new(
            self.activity_lines_count,
            self.show_autocomplete_border,
            self.autocomplete_background,
            self.word_wrap_enabled,
            &self.shortcut_config,
        );
        self.settings_form.focused_field = focused;
    }

    fn set_autocomplete_border(&mut self, enabled: bool) {
        if self.show_autocomplete_border == enabled {
            return;
        }
        self.show_autocomplete_border = enabled;
        self.autocomplete.set_show_border(enabled);

        if matches!(self.modal_state, ModalState::Settings) {
            let focused = self.settings_form.focused_field;
            self.rebuild_settings_form(focused);
        }
    }

    fn set_autocomplete_background(&mut self, color: Color) {
        if self.autocomplete_background == color {
            return;
        }
        self.autocomplete_background = color;

        if matches!(self.modal_state, ModalState::Settings) {
            let focused = self.settings_form.focused_field;
            self.rebuild_settings_form(focused);
        }
    }

    fn set_word_wrap(&mut self, enabled: bool) {
        if self.word_wrap_enabled == enabled {
            return;
        }
        self.word_wrap_enabled = enabled;
        self.task_description.set_word_wrap(enabled);
        self.refresh_autocomplete(false);

        if matches!(self.modal_state, ModalState::Settings) {
            let focused = self.settings_form.focused_field;
            self.rebuild_settings_form(focused);
        }
    }

    fn command_from_event(&self, event: &crossterm::event::KeyEvent) -> Option<Command> {
        for (command, key) in COMMAND_SHORTCUTS {
            if self.shortcut_config.matches(key, event) {
                return Some(*command);
            }
        }
        None
    }

    fn ensure_filter_editor(&mut self, control: FilterControl) {
        if self.filter_editor.as_ref().map(|editor| editor.control) != Some(control) {
            self.open_filter_editor(control);
        }
    }

    fn open_filter_editor(&mut self, control: FilterControl) {
        let options = match control {
            FilterControl::Repository => self.filter_options.repositories.clone(),
            FilterControl::Status => {
                STATUS_FILTER_OPTIONS.iter().map(|(label, _)| (*label).to_string()).collect()
            }
            FilterControl::Creator => self.filter_options.creators.clone(),
        };

        let current_label = match control {
            FilterControl::Repository => self.filter_state.repository_label(&self.filter_options),
            FilterControl::Status => self.filter_state.status_label(),
            FilterControl::Creator => self.filter_state.creator_label(&self.filter_options),
        };

        let fallback_width = current_label.chars().count().max(6) as u16;
        let anchor =
            self.filter_button_rects[control.index()].unwrap_or(Rect::new(0, 0, fallback_width, 1));

        let mut editor = FilterEditor {
            control,
            input: input_from_string(current_label),
            options,
            filtered: Vec::new(),
            selected: 0,
            anchor,
        };
        editor.recompute();

        let current_lower = current_label.to_ascii_lowercase();
        if let Some(idx) = editor
            .filtered
            .iter()
            .position(|&opt_idx| editor.options[opt_idx].to_ascii_lowercase() == current_lower)
        {
            editor.selected = idx;
        }

        self.filter_editor = Some(editor);
    }

    fn close_filter_editor(&mut self) {
        self.filter_editor = None;
    }

    fn set_filter_selection(&mut self, control: FilterControl, index: usize) {
        match control {
            FilterControl::Repository => {
                if !self.filter_options.repositories.is_empty() {
                    self.filter_state.repository_index =
                        index.min(self.filter_options.repositories.len() - 1);
                }
            }
            FilterControl::Status => {
                if !STATUS_FILTER_OPTIONS.is_empty() {
                    self.filter_state.status_index = index.min(STATUS_FILTER_OPTIONS.len() - 1);
                }
            }
            FilterControl::Creator => {
                if !self.filter_options.creators.is_empty() {
                    self.filter_state.creator_index =
                        index.min(self.filter_options.creators.len() - 1);
                }
            }
        }
    }

    fn refresh_filter_options(&mut self, tasks: &[TaskCard]) {
        self.filter_options = FilterOptions::from_tasks(tasks);

        if self.filter_state.repository_index >= self.filter_options.repositories.len() {
            self.filter_state.repository_index =
                self.filter_options.repositories.len().saturating_sub(1);
        }
        if self.filter_state.creator_index >= self.filter_options.creators.len() {
            self.filter_state.creator_index = self.filter_options.creators.len().saturating_sub(1);
        }

        if let Some(editor) = self.filter_editor.as_mut() {
            editor.options = match editor.control {
                FilterControl::Repository => self.filter_options.repositories.clone(),
                FilterControl::Status => {
                    STATUS_FILTER_OPTIONS.iter().map(|(label, _)| (*label).to_string()).collect()
                }
                FilterControl::Creator => self.filter_options.creators.clone(),
            };
            editor.recompute();
        }
    }

    fn visible_task_indices(&self, tasks: &[TaskCard]) -> Vec<usize> {
        let mut indices = Vec::new();
        for (idx, task) in tasks.iter().enumerate() {
            if idx == 0 || self.task_matches_filters(task) {
                indices.push(idx);
            }
        }
        indices
    }

    fn task_matches_filters(&self, task: &TaskCard) -> bool {
        if matches!(task.state, TaskState::Draft) {
            return true;
        }

        if let Some(repo) = self.filter_state.repository_filter(&self.filter_options) {
            if task.repository != repo {
                return false;
            }
        }

        if let Some(creator) = self.filter_state.creator_filter(&self.filter_options) {
            if task.creator != creator {
                return false;
            }
        }

        if let Some(status) = self.filter_state.status_filter() {
            let matches_status = match status {
                FilterStatus::Active => matches!(task.state, TaskState::Active),
                FilterStatus::Completed => matches!(task.state, TaskState::Completed),
                FilterStatus::Merged => matches!(task.state, TaskState::Merged),
            };
            if !matches_status {
                return false;
            }
        }

        true
    }

    fn clamp_selected_card(&mut self, tasks: &[TaskCard]) {
        if tasks.is_empty() {
            self.selected_card = 0;
            self.focus_element = FocusElement::TaskDescription;
            return;
        }

        if self.selected_card >= tasks.len() {
            self.selected_card = tasks.len() - 1;
        }

        if self.selected_card > 0 {
            if !self.task_matches_filters(&tasks[self.selected_card]) {
                if let Some((idx, _)) = tasks
                    .iter()
                    .enumerate()
                    .skip(1)
                    .find(|(_, task)| self.task_matches_filters(task))
                {
                    self.selected_card = idx;
                } else {
                    self.selected_card = 0;
                }
            }
        }

        if self.selected_card == 0 && !matches!(tasks[0].state, TaskState::Draft) {
            if let Some((idx, _)) = tasks
                .iter()
                .enumerate()
                .skip(1)
                .find(|(_, task)| self.task_matches_filters(task))
            {
                self.selected_card = idx;
            }
        }
    }

    /// Scroll the main tasks area down by the given number of lines
    fn scroll_down(&mut self, lines: u16) {
        self.scroll_offset = self.scroll_offset.saturating_add(lines);
    }

    /// Scroll the main tasks area up by the given number of lines
    fn scroll_up(&mut self, lines: u16) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
    }

    /// Calculate total content height for scrolling purposes
    fn calculate_total_content_height(&self, tasks: &[TaskCard]) -> u16 {
        let mut total = 0u16;

        // Draft cards
        for (idx, task) in tasks.iter().enumerate() {
            if matches!(task.state, TaskState::Draft) {
                total = total.saturating_add(task.height(self));
                total = total.saturating_add(1); // Spacer
            }
        }

        // Filter bar
        total = total.saturating_add(1);
        total = total.saturating_add(1); // Spacer

        // Non-draft tasks
        for task in tasks.iter() {
            if !matches!(task.state, TaskState::Draft) && self.task_matches_filters(task) {
                total = total.saturating_add(task.height(self));
                total = total.saturating_add(1); // Spacer
            }
        }

        total
    }

    /// Ensure the currently selected card is visible by adjusting scroll_offset
    fn ensure_selected_card_visible(&mut self, tasks: &[TaskCard], viewport_height: u16) {
        if tasks.is_empty() {
            return;
        }

        // Calculate the virtual y position of the selected card
        let mut virtual_y = 0u16;
        let mut found = false;
        let mut card_height = 0u16;

        // Draft cards
        for (idx, task) in tasks.iter().enumerate() {
            if matches!(task.state, TaskState::Draft) {
                if idx == self.selected_card {
                    card_height = task.height(self);
                    found = true;
                    break;
                }
                virtual_y = virtual_y.saturating_add(task.height(self));
                virtual_y = virtual_y.saturating_add(1); // Spacer
            }
        }

        if !found {
            // Filter bar
            virtual_y = virtual_y.saturating_add(1);
            virtual_y = virtual_y.saturating_add(1); // Spacer

            // Non-draft tasks
            for (idx, task) in tasks.iter().enumerate() {
                if !matches!(task.state, TaskState::Draft) && self.task_matches_filters(task) {
                    if idx == self.selected_card {
                        card_height = task.height(self);
                        found = true;
                        break;
                    }
                    virtual_y = virtual_y.saturating_add(task.height(self));
                    virtual_y = virtual_y.saturating_add(1); // Spacer
                }
            }
        }

        if !found {
            return;
        }

        let card_bottom = virtual_y.saturating_add(card_height);

        // If card top is above scroll offset, scroll up to show it
        if virtual_y < self.scroll_offset {
            self.scroll_offset = virtual_y;
        }

        // If card bottom is below visible area, scroll down to show it
        let visible_bottom = self.scroll_offset.saturating_add(viewport_height);
        if card_bottom > visible_bottom {
            self.scroll_offset = card_bottom.saturating_sub(viewport_height);
        }
    }

    fn launch_current_draft(&mut self, tasks: &mut Vec<TaskCard>) {
        if tasks.is_empty() {
            return;
        }
        if !matches!(tasks[0].state, TaskState::Draft) {
            return;
        }

        let description = self.task_description.lines().join("\n");
        if description.trim().is_empty() {
            self.status_message =
                Some("Enter a description before launching the draft".to_string());
            return;
        }

        let task = &mut tasks[0];
        task.title = description;
        task.repository = self.selected_repository.clone();
        task.branch = self.selected_branch.clone();
        task.agents = self.selected_models.clone();
        task.creator = "You".to_string();
        task.activity.clear();
        task.activity.push("Thoughts: Starting task execution".to_string());
        task.state = TaskState::Active;
        task.delivery_indicators = Some("⎇".to_string());
        task.current_tool_execution = None;

        self.status_message = Some("Draft launched".to_string());
        self.focus_element = FocusElement::TaskCard(0);
        self.refresh_filter_options(tasks);
        self.clamp_selected_card(tasks);
    }

    fn stop_task_by_index(&mut self, idx: usize, tasks: &mut Vec<TaskCard>) {
        if let Some(task) = tasks.get_mut(idx) {
            if matches!(task.state, TaskState::Active) {
                task.state = TaskState::Completed;
                task.activity.clear();
                task.current_tool_execution = None;
                task.delivery_indicators = Some("⎇ ✓".to_string());
                self.status_message = Some(format!("Task '{}' marked completed", task.title));
            }
        }
        self.refresh_filter_options(tasks);
        self.clamp_selected_card(tasks);
    }

    fn focus_next_control(&mut self, tasks: &mut Vec<TaskCard>) {
        match self.focus_element {
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
                self.focus_element = FocusElement::FilterBarLine;
            }
            FocusElement::FilterBarLine => {
                self.focus_element = FocusElement::Filter(FilterControl::Repository);
                self.ensure_filter_editor(FilterControl::Repository);
            }
            FocusElement::Filter(control) => {
                let next = match control {
                    FilterControl::Repository => FilterControl::Status,
                    FilterControl::Status => FilterControl::Creator,
                    FilterControl::Creator => {
                        self.close_filter_editor();
                        if let Some((idx, _)) = tasks
                            .iter()
                            .enumerate()
                            .skip(1)
                            .find(|(_, task)| self.task_matches_filters(task))
                        {
                            self.selected_card = idx;
                            self.focus_element = FocusElement::TaskCard(idx);
                        } else {
                            self.focus_element = FocusElement::SettingsButton;
                        }
                        return;
                    }
                };
                self.focus_element = FocusElement::Filter(next);
                self.ensure_filter_editor(next);
            }
            FocusElement::TaskCard(idx) => {
                self.focus_element = FocusElement::StopButton(idx);
            }
            FocusElement::StopButton(idx) => {
                if idx + 1 < tasks.len() {
                    self.selected_card = idx + 1;
                    self.focus_element = FocusElement::TaskCard(self.selected_card);
                } else {
                    self.focus_element = FocusElement::SettingsButton;
                }
            }
            FocusElement::SettingsButton => {
                self.focus_element = FocusElement::TaskDescription;
            }
        }
    }

    fn focus_previous_control(&mut self, _tasks: &mut Vec<TaskCard>) {
        match self.focus_element {
            FocusElement::TaskDescription => {
                self.close_filter_editor();
                self.focus_element = FocusElement::SettingsButton;
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
            FocusElement::FilterBarLine => {
                self.focus_element = FocusElement::GoButton;
            }
            FocusElement::Filter(control) => {
                let prev = match control {
                    FilterControl::Repository => {
                        self.close_filter_editor();
                        FocusElement::FilterBarLine
                    }
                    FilterControl::Status => FocusElement::Filter(FilterControl::Repository),
                    FilterControl::Creator => FocusElement::Filter(FilterControl::Status),
                };
                if let FocusElement::Filter(next_control) = prev {
                    self.focus_element = prev;
                    self.ensure_filter_editor(next_control);
                } else {
                    self.focus_element = prev;
                }
            }
            FocusElement::TaskCard(idx) => {
                if idx > 0 {
                    self.selected_card = idx;
                    self.focus_element = FocusElement::Filter(FilterControl::Creator);
                    self.ensure_filter_editor(FilterControl::Creator);
                } else {
                    self.focus_element = FocusElement::GoButton;
                }
            }
            FocusElement::StopButton(idx) => {
                self.focus_element = FocusElement::TaskCard(idx);
            }
            FocusElement::SettingsButton => {
                self.focus_element = FocusElement::Filter(FilterControl::Creator);
                self.ensure_filter_editor(FilterControl::Creator);
            }
        }
    }

    fn build_footer_hints(&self, _tasks: &[TaskCard], theme: &Theme) -> Vec<FooterHint> {
        let mut hints = Vec::new();
        let key_style = Style::default().fg(theme.primary).add_modifier(Modifier::BOLD);
        let description_style = theme.text_style();

        let mut push_hint =
            |bindings: Option<Vec<String>>, description: &str, action: Option<FooterAction>| {
                if let Some(values) = bindings {
                    if values.is_empty() {
                        return;
                    }
                    let key = format_bindings(&values);
                    if key.trim().is_empty() {
                        return;
                    }
                    hints.push(FooterHint {
                        key,
                        key_style,
                        description: description.to_string(),
                        description_style,
                        action,
                    });
                }
            };

        push_hint(
            self.shortcut_config.binding_strings(SHORTCUT_LAUNCH_TASK),
            "Launch draft",
            Some(FooterAction::LaunchDraft),
        );

        if matches!(self.focus_element, FocusElement::TaskDescription) {
            push_hint(
                self.shortcut_config.binding_strings(SHORTCUT_NEW_LINE),
                "Insert newline",
                Some(FooterAction::InsertNewLine),
            );
            push_hint(
                self.shortcut_config.binding_strings(SHORTCUT_SHORTCUT_HELP),
                "Shortcut help",
                Some(FooterAction::OpenShortcutHelp),
            );
        }

        push_hint(
            self.shortcut_config.binding_strings(SHORTCUT_NEXT_FIELD),
            "Next field",
            Some(FooterAction::FocusNextField),
        );
        push_hint(
            self.shortcut_config.binding_strings(SHORTCUT_PREV_FIELD),
            "Previous field",
            Some(FooterAction::FocusPreviousField),
        );
        push_hint(
            self.shortcut_config.binding_strings(SHORTCUT_OPEN_SETTINGS),
            "Open settings",
            Some(FooterAction::OpenSettings),
        );

        hints
    }

    fn handle_mouse(&mut self, event: MouseEvent, tasks: &mut Vec<TaskCard>) -> bool {
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let column = event.column;
                let row = event.row;
                for area in &self.interactive_areas {
                    if rect_contains(area.rect, column, row) {
                        return self.perform_mouse_action(area.action, tasks);
                    }
                }

                if let Some(textarea_area) = self.last_textarea_area.get() {
                    if rect_contains(textarea_area, column, row) {
                        self.focus_element = FocusElement::TaskDescription;
                        self.close_filter_editor();
                    }
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {}
            MouseEventKind::Drag(MouseButton::Left) => {}
            _ => {}
        }
        false
    }

    fn handle_filter_editor_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        tasks: &mut Vec<TaskCard>,
    ) -> bool {
        if let Some(editor) = self.filter_editor.as_mut() {
            match key.code {
                KeyCode::Esc => {
                    self.close_filter_editor();
                    return true;
                }
                KeyCode::Enter => {
                    if let Some(idx) = editor.current_selection() {
                        let control = editor.control;
                        self.close_filter_editor();
                        self.set_filter_selection(control, idx);
                        self.clamp_selected_card(tasks);
                    } else {
                        self.close_filter_editor();
                    }
                    return true;
                }
                KeyCode::Up => {
                    if !editor.filtered.is_empty() {
                        editor.selected = editor.selected.saturating_sub(1);
                    }
                    return true;
                }
                KeyCode::Down => {
                    if !editor.filtered.is_empty() {
                        let new_selected = (editor.selected + 1).min(editor.filtered.len() - 1);
                        if new_selected == editor.selected {
                            // We're already at the bottom, exit filter and go to settings button
                            self.close_filter_editor();
                            self.focus_element = FocusElement::SettingsButton;
                        } else {
                            editor.selected = new_selected;
                        }
                    }
                    return true;
                }
                KeyCode::PageUp => {
                    if !editor.filtered.is_empty() {
                        editor.selected = editor.selected.saturating_sub(5);
                    }
                    return true;
                }
                KeyCode::PageDown => {
                    if !editor.filtered.is_empty() {
                        let new_idx = editor.selected.saturating_add(5);
                        editor.selected = new_idx.min(editor.filtered.len() - 1);
                    }
                    return true;
                }
                KeyCode::Left => {
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        editor.input.handle(InputRequest::GoToPrevChar);
                        return true;
                    }
                    let next = match editor.control {
                        FilterControl::Repository => FilterControl::Creator,
                        FilterControl::Status => FilterControl::Repository,
                        FilterControl::Creator => FilterControl::Status,
                    };
                    self.focus_element = FocusElement::Filter(next);
                    self.open_filter_editor(next);
                    return true;
                }
                KeyCode::Right => {
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        editor.input.handle(InputRequest::GoToNextChar);
                        return true;
                    }
                    let next = match editor.control {
                        FilterControl::Repository => FilterControl::Status,
                        FilterControl::Status => FilterControl::Creator,
                        FilterControl::Creator => FilterControl::Repository,
                    };
                    self.focus_element = FocusElement::Filter(next);
                    self.open_filter_editor(next);
                    return true;
                }
                KeyCode::Home => {
                    editor.input.handle(InputRequest::GoToStart);
                    return true;
                }
                KeyCode::End => {
                    editor.input.handle(InputRequest::GoToEnd);
                    return true;
                }
                KeyCode::Backspace => {
                    editor.input.handle(InputRequest::DeletePrevChar);
                    editor.recompute();
                    editor.selected = editor.selected.min(editor.filtered.len().saturating_sub(1));
                    return true;
                }
                KeyCode::Delete => {
                    editor.input.handle(InputRequest::DeleteNextChar);
                    editor.recompute();
                    editor.selected = editor.selected.min(editor.filtered.len().saturating_sub(1));
                    return true;
                }
                KeyCode::Char(c)
                    if !key.modifiers.intersects(
                        KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER,
                    ) =>
                {
                    editor.input.handle(InputRequest::InsertChar(c));
                    editor.recompute();
                    editor.selected = 0;
                    return true;
                }
                _ => {}
            }
        }
        false
    }

    fn perform_mouse_action(&mut self, action: MouseAction, tasks: &mut Vec<TaskCard>) -> bool {
        match action {
            MouseAction::SelectCard(idx) => {
                self.close_filter_editor();
                self.selected_card = idx;
                if idx == 0 && matches!(tasks.get(0).map(|t| &t.state), Some(TaskState::Draft)) {
                    self.focus_element = FocusElement::TaskDescription;
                } else {
                    self.focus_element = FocusElement::TaskCard(idx);
                }
                false
            }
            MouseAction::ActivateGoButton => {
                self.close_filter_editor();
                self.launch_current_draft(tasks);
                false
            }
            MouseAction::OpenRepositoryModal => {
                self.close_filter_editor();
                self.open_repository_modal();
                false
            }
            MouseAction::OpenBranchModal => {
                self.close_filter_editor();
                self.open_branch_modal();
                false
            }
            MouseAction::OpenModelModal => {
                self.close_filter_editor();
                self.open_model_selection_modal();
                false
            }
            MouseAction::StopTask(idx) => {
                self.close_filter_editor();
                self.stop_task_by_index(idx, tasks);
                false
            }
            MouseAction::OpenSettings => {
                self.close_filter_editor();
                self.perform_footer_action(FooterAction::OpenSettings, tasks)
            }
            MouseAction::EditFilter(control) => {
                self.focus_element = FocusElement::Filter(control);
                self.ensure_filter_editor(control);
                false
            }
            MouseAction::Footer(action) => self.perform_footer_action(action, tasks),
        }
    }

    fn perform_footer_action(&mut self, action: FooterAction, tasks: &mut Vec<TaskCard>) -> bool {
        match action {
            FooterAction::LaunchDraft => {
                self.launch_current_draft(tasks);
                false
            }
            FooterAction::InsertNewLine => {
                let effect = execute_command(
                    &mut self.task_description,
                    Command::OpenNewLine,
                    &mut self.search_mode,
                    &mut self.kill_ring,
                    &mut self.clipboard,
                );
                self.apply_command_effect(effect);
                false
            }
            FooterAction::FocusNextField => {
                self.focus_next_control(tasks);
                false
            }
            FooterAction::FocusPreviousField => {
                self.focus_previous_control(tasks);
                false
            }
            FooterAction::OpenShortcutHelp => {
                self.open_shortcut_help_modal();
                false
            }
            FooterAction::OpenSettings => {
                self.modal_state = ModalState::Settings;
                self.settings_form = SettingsForm::new(
                    self.activity_lines_count,
                    self.show_autocomplete_border,
                    self.autocomplete_background,
                    self.word_wrap_enabled,
                    &self.shortcut_config,
                );
                false
            }
            FooterAction::StopTask(idx) => {
                self.stop_task_by_index(idx, tasks);
                false
            }
            FooterAction::Quit => true,
        }
    }

    fn apply_settings_form(&mut self) {
        let focused = self.settings_form.focused_field;

        let activity_lines = self
            .settings_form
            .activity_lines_input
            .value()
            .trim()
            .parse::<usize>()
            .map(|v| v.clamp(1, 3))
            .unwrap_or(self.activity_lines_count);
        self.activity_lines_count = activity_lines;

        let border_input =
            self.settings_form.autocomplete_border_input.value().trim().to_lowercase();
        let border_enabled = match border_input.as_str() {
            "on" | "true" | "yes" | "1" => true,
            "off" | "false" | "no" | "0" => false,
            _ => self.show_autocomplete_border,
        };
        self.set_autocomplete_border(border_enabled);

        if let Some(color) =
            parse_hex_color(self.settings_form.autocomplete_bg_input.value().trim())
        {
            self.set_autocomplete_background(color);
        }

        let wrap_input = self.settings_form.word_wrap_input.value().trim().to_lowercase();
        let wrap_enabled = match wrap_input.as_str() {
            "on" | "true" | "yes" | "1" => true,
            "off" | "false" | "no" | "0" => false,
            _ => self.word_wrap_enabled,
        };
        self.set_word_wrap(wrap_enabled);

        let mut shortcut_error: Option<String> = None;

        let launch_value = self.settings_form.launch_shortcut_input.value();
        if let Err(err) = self
            .shortcut_config
            .set_binding_from_text(SHORTCUT_LAUNCH_TASK, launch_value.trim())
        {
            shortcut_error.get_or_insert_with(|| format!("Launch shortcut: {err}"));
        }

        let new_line_value = self.settings_form.new_line_shortcut_input.value();
        if let Err(err) = self
            .shortcut_config
            .set_binding_from_text(SHORTCUT_NEW_LINE, new_line_value.trim())
        {
            shortcut_error.get_or_insert_with(|| format!("New line shortcut: {err}"));
        }

        let next_field_value = self.settings_form.next_field_shortcut_input.value();
        if let Err(err) = self
            .shortcut_config
            .set_binding_from_text(SHORTCUT_NEXT_FIELD, next_field_value.trim())
        {
            shortcut_error.get_or_insert_with(|| format!("Next field shortcut: {err}"));
        }

        let prev_field_value = self.settings_form.prev_field_shortcut_input.value();
        if let Err(err) = self
            .shortcut_config
            .set_binding_from_text(SHORTCUT_PREV_FIELD, prev_field_value.trim())
        {
            shortcut_error.get_or_insert_with(|| format!("Previous field shortcut: {err}"));
        }

        let shortcut_help_value = self.settings_form.shortcut_help_input.value();
        if let Err(err) = self
            .shortcut_config
            .set_binding_from_text(SHORTCUT_SHORTCUT_HELP, shortcut_help_value.trim())
        {
            shortcut_error.get_or_insert_with(|| format!("Shortcut help: {err}"));
        }

        if let Some(message) = shortcut_error {
            self.status_message = Some(message);
        } else {
            self.status_message = None;
        }

        self.settings_form = SettingsForm::new(
            self.activity_lines_count,
            self.show_autocomplete_border,
            self.autocomplete_background,
            self.word_wrap_enabled,
            &self.shortcut_config,
        );
        self.settings_form.focused_field = focused;
    }

    fn handle_key(&mut self, key: crossterm::event::KeyEvent, tasks: &mut Vec<TaskCard>) -> bool {
        // Log all key events
        log_key_event(&key, "MAIN");

        match self.modal_state {
            ModalState::None => self.handle_main_key(key, tasks),
            ModalState::RepositorySearch | ModalState::BranchSearch | ModalState::ModelSearch => {
                self.handle_modal_key(key)
            }
            ModalState::ModelSelection => self.handle_model_selection_key(key),
            ModalState::Settings => self.handle_settings_key(key),
            ModalState::GoToLine => self.handle_goto_line_key(key),
            ModalState::FindReplace => self.handle_find_replace_key(key),
            ModalState::ShortcutHelp => self.handle_modal_key(key),
        }
    }

    fn handle_main_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        tasks: &mut Vec<TaskCard>,
    ) -> bool {
        use crossterm::event::KeyCode;

        let ctrl_pressed = key.modifiers.contains(KeyModifiers::CONTROL);

        if matches!(self.focus_element, FocusElement::Filter(_)) {
            if self.handle_filter_editor_key(key, tasks) {
                return false;
            }
        }

        if matches!(self.focus_element, FocusElement::TaskDescription) {
            match self.autocomplete.handle_key_event(&key, &mut self.task_description) {
                AutocompleteKeyResult::Consumed { text_changed } => {
                    self.refresh_autocomplete(text_changed);
                    return false;
                }
                AutocompleteKeyResult::Ignored => {}
            }
        }

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

        if matches!(self.focus_element, FocusElement::TaskDescription) {
            if self.shortcut_config.matches(SHORTCUT_NEW_LINE, &key) {
                let effect = execute_command(
                    &mut self.task_description,
                    Command::OpenNewLine,
                    &mut self.search_mode,
                    &mut self.kill_ring,
                    &mut self.clipboard,
                );
                self.apply_command_effect(effect);
                return false;
            }

            if self.shortcut_config.matches(SHORTCUT_LAUNCH_TASK, &key) {
                self.launch_current_draft(tasks);
                return false;
            }

            if self.shortcut_config.matches(SHORTCUT_SHORTCUT_HELP, &key) {
                self.open_shortcut_help_modal();
                return false;
            }

            if let Some(command) = self.command_from_event(&key) {
                let effect = execute_command(
                    &mut self.task_description,
                    command,
                    &mut self.search_mode,
                    &mut self.kill_ring,
                    &mut self.clipboard,
                );
                self.apply_command_effect(effect);
                return false;
            }
        }

        if self.shortcut_config.matches(SHORTCUT_NEXT_FIELD, &key) {
            self.focus_next_control(tasks);
            return false;
        }

        if self.shortcut_config.matches(SHORTCUT_PREV_FIELD, &key) {
            self.focus_previous_control(tasks);
            return false;
        }

        match key.code {
            KeyCode::Esc => {
                // If focus is on any button in the draft card, move focus back to textarea
                match self.focus_element {
                    FocusElement::RepositoryButton
                    | FocusElement::BranchButton
                    | FocusElement::ModelButton
                    | FocusElement::GoButton => {
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
                        let shift_only = key.modifiers.contains(KeyModifiers::SHIFT)
                            && !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
                        if shift_only {
                            self.extend_selection(tui_textarea::CursorMove::Up);
                            return false;
                        }
                        // Check if caret can move up within text area
                        let (cursor_row, _) = self.task_description.cursor();
                        if cursor_row > 0 {
                            // Caret can move up, let text area handle it
                            let effect = execute_command(
                                &mut self.task_description,
                                Command::MoveToPreviousLine,
                                &mut self.search_mode,
                                &mut self.kill_ring,
                                &mut self.clipboard,
                            );
                            self.apply_command_effect(effect);
                            return false;
                        } else {
                            // Caret is at top, move focus to settings button
                            self.focus_element = FocusElement::SettingsButton;
                        }
                    }
                    FocusElement::TaskCard(idx) => {
                        if idx > 0 {
                            self.selected_card = idx - 1;
                            self.focus_element = FocusElement::TaskCard(self.selected_card);
                            // If moving to draft card (index 0), automatically focus description
                            if self.selected_card == 0 && matches!(tasks[0].state, TaskState::Draft)
                            {
                                self.focus_element = FocusElement::TaskDescription;
                            }
                        } else if idx == 0 {
                            // Move up from the first task card to the settings button
                            self.focus_element = FocusElement::SettingsButton;
                        }
                    }
                    FocusElement::Filter(_) => {
                        self.close_filter_editor();
                        if matches!(tasks.get(0).map(|t| &t.state), Some(TaskState::Draft)) {
                            self.focus_element = FocusElement::TaskDescription;
                        } else {
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
                            if self.selected_card == 0 && matches!(tasks[0].state, TaskState::Draft)
                            {
                                self.focus_element = FocusElement::TaskDescription;
                            }
                        } else {
                            self.focus_element = FocusElement::Filter(FilterControl::Repository);
                            self.ensure_filter_editor(FilterControl::Repository);
                        }
                    }
                    FocusElement::TaskDescription => {
                        let shift_only = key.modifiers.contains(KeyModifiers::SHIFT)
                            && !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
                        if shift_only {
                            self.extend_selection(tui_textarea::CursorMove::Down);
                            return false;
                        }
                        // Check if caret can move down within text area
                        let (cursor_row, _) = self.task_description.cursor();
                        let lines = self.task_description.lines();
                        let last_row = lines.len().saturating_sub(1);
                        if cursor_row < last_row {
                            // Caret can move down, let text area handle it
                            let effect = execute_command(
                                &mut self.task_description,
                                Command::MoveToNextLine,
                                &mut self.search_mode,
                                &mut self.kill_ring,
                                &mut self.clipboard,
                            );
                            self.apply_command_effect(effect);
                            return false;
                        } else {
                            // Caret is at bottom, move focus to the filter bar line
                            self.focus_element = FocusElement::FilterBarLine;
                        }
                    }
                    FocusElement::FilterBarLine => {
                        // Move down from the filter bar line to the first matching task
                        if let Some(idx) = tasks
                            .iter()
                            .enumerate()
                            .filter(|(_, task)| {
                                !matches!(task.state, TaskState::Draft)
                                    && self.task_matches_filters(task)
                            })
                            .map(|(idx, _)| idx)
                            .next()
                        {
                            self.selected_card = idx;
                            self.focus_element = FocusElement::TaskCard(idx);
                        }
                    }
                    FocusElement::Filter(_) => {
                        // Wrap around to settings button when reaching the bottom
                        self.focus_element = FocusElement::SettingsButton;
                    }
                    _ => {}
                }
            }
            KeyCode::Enter => {
                match self.focus_element {
                    FocusElement::TaskDescription => {
                        let effect = execute_command(
                            &mut self.task_description,
                            Command::OpenNewLine,
                            &mut self.search_mode,
                            &mut self.kill_ring,
                            &mut self.clipboard,
                        );
                        self.apply_command_effect(effect);
                    }
                    FocusElement::TaskCard(idx) => {
                        if idx == 0 && matches!(tasks[0].state, TaskState::Draft) {
                            self.focus_element = FocusElement::TaskDescription;
                        }
                    }
                    FocusElement::SettingsButton => {
                        // Open settings dialog
                        self.modal_state = ModalState::Settings;
                        self.settings_form = SettingsForm::new(
                            self.activity_lines_count,
                            self.show_autocomplete_border,
                            self.autocomplete_background,
                            self.word_wrap_enabled,
                            &self.shortcut_config,
                        );
                    }
                    FocusElement::Filter(control) => {
                        let direction = if key.modifiers.contains(KeyModifiers::SHIFT) {
                            -1
                        } else {
                            1
                        };
                        match control {
                            FilterControl::Repository => {
                                self.filter_state.cycle_repository(&self.filter_options, direction);
                            }
                            FilterControl::Status => {
                                self.filter_state.cycle_status(direction);
                            }
                            FilterControl::Creator => {
                                self.filter_state.cycle_creator(&self.filter_options, direction);
                            }
                        }
                        self.clamp_selected_card(tasks);
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
                        self.launch_current_draft(tasks);
                    }
                    FocusElement::StopButton(idx) => {
                        self.stop_task_by_index(idx, tasks);
                    }
                    FocusElement::FilterBarLine => {
                        // Enter on FilterBarLine does nothing
                    }
                }
            }
            KeyCode::Tab => {
                self.focus_next_control(tasks);
            }
            KeyCode::Right => {
                match self.focus_element {
                    FocusElement::TaskDescription => {
                        let shift_only = key.modifiers.contains(KeyModifiers::SHIFT)
                            && !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
                        if shift_only {
                            self.extend_selection(tui_textarea::CursorMove::Forward);
                            return false;
                        }
                        // When task description is focused, handle Right arrow via command system
                        if let Some(command) = key_to_command(&key) {
                            let effect = execute_command(
                                &mut self.task_description,
                                command,
                                &mut self.search_mode,
                                &mut self.kill_ring,
                                &mut self.clipboard,
                            );
                            self.apply_command_effect(effect);
                        }
                        return false;
                    }
                    FocusElement::FilterBarLine => {
                        // Right arrow from FilterBarLine moves to first filter control
                        self.focus_element = FocusElement::Filter(FilterControl::Repository);
                        self.ensure_filter_editor(FilterControl::Repository);
                    }
                    FocusElement::Filter(control) => {
                        self.focus_element = match control {
                            FilterControl::Repository => {
                                FocusElement::Filter(FilterControl::Status)
                            }
                            FilterControl::Status => FocusElement::Filter(FilterControl::Creator),
                            FilterControl::Creator => {
                                FocusElement::Filter(FilterControl::Repository)
                            }
                        };
                        if let FocusElement::Filter(next) = self.focus_element {
                            self.ensure_filter_editor(next);
                        }
                    }
                    _ => {
                        // For other elements, treat Right as Tab
                        #[allow(unreachable_patterns)]
                        match self.focus_element {
                            FocusElement::TaskCard(idx) => {
                                self.focus_element = FocusElement::StopButton(idx);
                            }
                            FocusElement::StopButton(_) => {
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
                            FocusElement::SettingsButton => {
                                // Settings button does not participate in this cycle
                            }
                            _ => {}
                        }
                    }
                }
            }
            KeyCode::Left => {
                match self.focus_element {
                    FocusElement::FilterBarLine => {
                        // Move from separator line to first filter control
                        self.focus_element = FocusElement::Filter(FilterControl::Repository);
                        self.ensure_filter_editor(FilterControl::Repository);
                    }
                    FocusElement::TaskDescription => {
                        let shift_only = key.modifiers.contains(KeyModifiers::SHIFT)
                            && !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
                        if shift_only {
                            self.extend_selection(tui_textarea::CursorMove::Back);
                            return false;
                        }
                        // When task description is focused, handle Left arrow via command system
                        if let Some(command) = key_to_command(&key) {
                            let effect = execute_command(
                                &mut self.task_description,
                                command,
                                &mut self.search_mode,
                                &mut self.kill_ring,
                                &mut self.clipboard,
                            );
                            self.apply_command_effect(effect);
                        }
                        return false;
                    }
                    FocusElement::Filter(control) => {
                        // Navigate backwards through filter controls
                        let next = match control {
                            FilterControl::Repository => FilterControl::Creator, // Wrap backwards
                            FilterControl::Status => FilterControl::Repository,
                            FilterControl::Creator => FilterControl::Status,
                        };
                        self.focus_element = FocusElement::Filter(next);
                        self.ensure_filter_editor(next);
                    }
                    _ => {
                        // For other elements, treat Left as reverse Tab
                        #[allow(unreachable_patterns)]
                        match self.focus_element {
                            FocusElement::TaskCard(_) => {
                                // Stay on card
                            }
                            FocusElement::StopButton(_) => {
                                // Stay on stop button
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
                            }
                            _ => {}
                        }
                    }
                }
            }
            KeyCode::Right => {
                match self.focus_element {
                    FocusElement::FilterBarLine => {
                        // Move from separator line to first filter control
                        self.focus_element = FocusElement::Filter(FilterControl::Repository);
                        self.ensure_filter_editor(FilterControl::Repository);
                    }
                    FocusElement::Filter(control) => {
                        // Navigate between filter controls
                        let next = match control {
                            FilterControl::Repository => FilterControl::Status,
                            FilterControl::Status => FilterControl::Creator,
                            FilterControl::Creator => FilterControl::Repository, // Wrap around
                        };
                        self.focus_element = FocusElement::Filter(next);
                        self.ensure_filter_editor(next);
                    }
                    _ => {
                        // For other elements, treat Right as forward Tab
                        self.focus_next_control(tasks);
                    }
                }
            }
            KeyCode::BackTab => {
                self.focus_previous_control(tasks);
            }
            KeyCode::PageUp => {
                // Scroll up by 10 lines
                self.scroll_up(10);
            }
            KeyCode::PageDown => {
                // Scroll down by 10 lines
                self.scroll_down(10);
            }
            // Handle text input for description using new command system
            _ if matches!(self.focus_element, FocusElement::TaskDescription) => {
                // Log the key event for debugging
                log_key_event(&key, "TEXTAREA");

                // Handle incremental search mode
                if !matches!(self.search_mode, SearchMode::None) {
                    match key.code {
                        KeyCode::Esc => {
                            // Exit search mode
                            self.search_mode = SearchMode::None;
                            return false;
                        }
                        KeyCode::Enter => {
                            // Find next/previous depending on search mode
                            match self.search_mode {
                                SearchMode::IncrementalForward => {
                                    if let Some(_pattern) = self.task_description.search_pattern() {
                                        self.task_description.search_forward(false);
                                    }
                                }
                                SearchMode::IncrementalBackward => {
                                    if let Some(_pattern) = self.task_description.search_pattern() {
                                        self.task_description.search_back(false);
                                    }
                                }
                                SearchMode::None => {}
                            }
                            return false;
                        }
                        KeyCode::Char(c)
                            if c.is_alphanumeric()
                                || c.is_whitespace()
                                || "!@#$%^&*()_+-=[]{}|;:,.<>?".contains(c) =>
                        {
                            // Add character to search pattern
                            let new_pattern = format!("{}", c); // For simplicity, start fresh each time
                            let _ = self.task_description.set_search_pattern(new_pattern);

                            // Auto-search as we type
                            match self.search_mode {
                                SearchMode::IncrementalForward => {
                                    self.task_description.search_forward(false);
                                }
                                SearchMode::IncrementalBackward => {
                                    self.task_description.search_back(false);
                                }
                                SearchMode::None => {}
                            }
                            return false;
                        }
                        KeyCode::Backspace => {
                            // Could remove last character from search pattern
                            // For now, just clear it
                            let _ = self.task_description.set_search_pattern("".to_string());
                            return false;
                        }
                        _ => {}
                    }
                }

                // Special handling for shift+arrow keys to extend selection
                let shift_pressed = key.modifiers.contains(crossterm::event::KeyModifiers::SHIFT);
                if shift_pressed {
                    match key.code {
                        crossterm::event::KeyCode::Left => {
                            // Start selection if not already active, then move left
                            if self.task_description.selection_range().is_none() {
                                self.task_description.start_selection();
                            }
                            self.task_description.move_cursor(tui_textarea::CursorMove::Back);
                            self.refresh_autocomplete(false);
                            return false;
                        }
                        crossterm::event::KeyCode::Right => {
                            // Start selection if not already active, then move right
                            if self.task_description.selection_range().is_none() {
                                self.task_description.start_selection();
                            }
                            self.task_description.move_cursor(tui_textarea::CursorMove::Forward);
                            self.refresh_autocomplete(false);
                            return false;
                        }
                        crossterm::event::KeyCode::Up => {
                            // Start selection if not already active, then move up
                            if self.task_description.selection_range().is_none() {
                                self.task_description.start_selection();
                            }
                            self.task_description.move_cursor(tui_textarea::CursorMove::Up);
                            self.refresh_autocomplete(false);
                            return false;
                        }
                        crossterm::event::KeyCode::Down => {
                            // Start selection if not already active, then move down
                            if self.task_description.selection_range().is_none() {
                                self.task_description.start_selection();
                            }
                            self.task_description.move_cursor(tui_textarea::CursorMove::Down);
                            self.refresh_autocomplete(false);
                            return false;
                        }
                        _ => {}
                    }
                }

                // First, check if this is a command
                if let Some(command) = key_to_command(&key) {
                    if !shift_pressed
                        && self.task_description.selection_range().is_some()
                        && command_clears_selection(command)
                    {
                        self.task_description.cancel_selection();
                    }
                    let effect = execute_command(
                        &mut self.task_description,
                        command,
                        &mut self.search_mode,
                        &mut self.kill_ring,
                        &mut self.clipboard,
                    );
                    self.apply_command_effect(effect);
                    return false;
                }

                // If not a command, handle as regular text input using input_without_shortcuts
                let textarea_key = match key.code {
                    crossterm::event::KeyCode::Char(c) => tui_textarea::Key::Char(c),
                    crossterm::event::KeyCode::Backspace => tui_textarea::Key::Backspace,
                    crossterm::event::KeyCode::Enter => tui_textarea::Key::Enter,
                    crossterm::event::KeyCode::Left => tui_textarea::Key::Left,
                    crossterm::event::KeyCode::Right => tui_textarea::Key::Right,
                    crossterm::event::KeyCode::Up => tui_textarea::Key::Up,
                    crossterm::event::KeyCode::Down => tui_textarea::Key::Down,
                    crossterm::event::KeyCode::Tab => tui_textarea::Key::Tab,
                    crossterm::event::KeyCode::Delete => tui_textarea::Key::Delete,
                    crossterm::event::KeyCode::Home => tui_textarea::Key::Home,
                    crossterm::event::KeyCode::End => tui_textarea::Key::End,
                    _ => tui_textarea::Key::Null,
                };

                let textarea_input = tui_textarea::Input {
                    key: textarea_key,
                    ctrl: key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL),
                    alt: key.modifiers.contains(crossterm::event::KeyModifiers::ALT),
                    shift: key.modifiers.contains(crossterm::event::KeyModifiers::SHIFT),
                };

                self.task_description.input_without_shortcuts(textarea_input);
                let text_changed = matches!(
                    textarea_key,
                    tui_textarea::Key::Char(_)
                        | tui_textarea::Key::Backspace
                        | tui_textarea::Key::Delete
                        | tui_textarea::Key::Enter
                );
                self.refresh_autocomplete(text_changed);
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
                            if let Some(existing) =
                                modal.selected_models.iter_mut().find(|m| m.name == model_name)
                            {
                                existing.count += 1;
                            } else {
                                modal.selected_models.push(SelectedModel {
                                    name: model_name,
                                    count: 1,
                                });
                            }
                        }
                    }
                }
                KeyCode::Tab => {
                    // Toggle between editing count mode and navigation mode
                    modal.editing_count = !modal.editing_count;
                    if modal.editing_count && !modal.selected_models.is_empty() {
                        modal.editing_index =
                            modal.editing_index.min(modal.selected_models.len() - 1);
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
                            modal.editing_index =
                                (modal.editing_index + 1).min(modal.selected_models.len() - 1);
                        }
                    } else {
                        // Navigate available models
                        if modal.selected_index < modal.available_models.len().saturating_sub(1) {
                            modal.selected_index =
                                (modal.selected_index + 1).min(modal.available_models.len() - 1);
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
                            if modal.editing_index >= modal.selected_models.len()
                                && modal.editing_index > 0
                            {
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

    fn handle_goto_line_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crossterm::event::KeyCode;

        if let Some(modal) = &mut self.goto_line_modal {
            modal.max_line = self.task_description.lines().len().max(1);
            match key.code {
                KeyCode::Esc => {
                    self.close_modal();
                }
                KeyCode::Enter => {
                    let value = modal.input.value().trim();
                    if value.is_empty() {
                        modal.error = Some("Please enter a line number".to_string());
                    } else if let Ok(line_num) = value.parse::<usize>() {
                        if line_num == 0 || line_num > modal.max_line {
                            modal.error =
                                Some(format!("Line must be between 1 and {}", modal.max_line));
                        } else {
                            let target_row = line_num - 1;
                            let target_col = self
                                .task_description
                                .lines()
                                .get(target_row)
                                .map(|line| line.chars().count())
                                .unwrap_or(0)
                                .min(self.task_description.cursor().1);
                            self.task_description.move_cursor(tui_textarea::CursorMove::Jump(
                                target_row as u16,
                                target_col as u16,
                            ));
                            self.close_modal();
                            self.status_message = Some(format!("Moved to line {}", line_num));
                            self.refresh_autocomplete(false);
                        }
                    } else {
                        modal.error = Some("Invalid number".to_string());
                    }
                }
                KeyCode::Backspace => {
                    modal.input.handle(InputRequest::DeletePrevChar);
                    modal.error = None;
                }
                KeyCode::Delete => {
                    modal.input.handle(InputRequest::DeleteNextChar);
                    modal.error = None;
                }
                KeyCode::Left => {
                    modal.input.handle(InputRequest::GoToPrevChar);
                }
                KeyCode::Right => {
                    modal.input.handle(InputRequest::GoToNextChar);
                }
                KeyCode::Home => {
                    modal.input.handle(InputRequest::GoToStart);
                }
                KeyCode::End => {
                    modal.input.handle(InputRequest::GoToEnd);
                }
                KeyCode::Char(c) if c.is_ascii_digit() => {
                    modal.input.handle(InputRequest::InsertChar(c));
                    modal.error = None;
                }
                _ => {}
            }
        }

        false
    }

    fn handle_find_replace_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crossterm::event::KeyCode;

        let mut pending_replace: Option<(String, String, bool)> = None;
        let mut close_modal = false;
        let mut refresh_after_replace = false;

        if let Some(modal) = self.find_replace_modal.as_mut() {
            let active_input = if modal.stage == FindReplaceStage::EnterSearch {
                &mut modal.search_input
            } else {
                &mut modal.replace_input
            };

            match key.code {
                KeyCode::Esc => {
                    close_modal = true;
                }
                KeyCode::Tab => {
                    modal.stage = FindReplaceStage::EnterReplacement;
                    modal.error = None;
                }
                KeyCode::BackTab => {
                    modal.stage = FindReplaceStage::EnterSearch;
                    modal.error = None;
                }
                KeyCode::Enter => match modal.stage {
                    FindReplaceStage::EnterSearch => {
                        let search = modal.search_input.value().trim();
                        if search.is_empty() {
                            modal.error = Some("Search text cannot be empty".to_string());
                        } else {
                            modal.stage = FindReplaceStage::EnterReplacement;
                            modal.error = None;
                        }
                    }
                    FindReplaceStage::EnterReplacement => {
                        let search = modal.search_input.value().trim().to_string();
                        if search.is_empty() {
                            modal.error = Some("Search text cannot be empty".to_string());
                            modal.stage = FindReplaceStage::EnterSearch;
                        } else {
                            let replacement = modal.replace_input.value().to_string();
                            pending_replace = Some((search, replacement, modal.is_regex));
                            modal.error = None;
                        }
                    }
                },
                KeyCode::Backspace => {
                    active_input.handle(InputRequest::DeletePrevChar);
                    modal.error = None;
                }
                KeyCode::Delete => {
                    active_input.handle(InputRequest::DeleteNextChar);
                    modal.error = None;
                }
                KeyCode::Left => {
                    active_input.handle(InputRequest::GoToPrevChar);
                }
                KeyCode::Right => {
                    active_input.handle(InputRequest::GoToNextChar);
                }
                KeyCode::Home => {
                    active_input.handle(InputRequest::GoToStart);
                }
                KeyCode::End => {
                    active_input.handle(InputRequest::GoToEnd);
                }
                KeyCode::Char(c) => {
                    active_input.handle(InputRequest::InsertChar(c));
                    modal.error = None;
                }
                _ => {}
            }
        }

        if let Some((search, replacement, is_regex)) = pending_replace {
            match self.perform_find_replace(&search, &replacement, is_regex) {
                Ok(count) => {
                    if count == 0 {
                        self.status_message = Some("No matches found".to_string());
                    } else {
                        self.status_message = Some(format!("Replaced {} occurrence(s)", count));
                        refresh_after_replace = true;
                    }
                    close_modal = true;
                }
                Err(err) => {
                    if let Some(modal) = self.find_replace_modal.as_mut() {
                        modal.error = Some(err);
                        modal.stage = FindReplaceStage::EnterSearch;
                    } else {
                        self.status_message = Some(err);
                    }
                }
            }
        }

        if close_modal {
            self.close_modal();
        }
        if refresh_after_replace {
            self.refresh_autocomplete(true);
        }
        false
    }

    fn perform_find_replace(
        &mut self,
        search: &str,
        replacement: &str,
        is_regex: bool,
    ) -> Result<usize, String> {
        let before_lines: Vec<String> = self.task_description.lines().iter().cloned().collect();
        let before_yank = self.task_description.yank_text();

        if before_lines.is_empty() {
            return Ok(0);
        }

        let original_text = before_lines.join("\n");
        if original_text.is_empty() {
            return Ok(0);
        }

        if is_regex {
            let regex = Regex::new(search).map_err(|err| err.to_string())?;
            let mut matches_iter = regex.find_iter(&original_text);
            if let Some(first_match) = matches_iter.next() {
                let count = 1 + matches_iter.count();
                let replaced_text = regex.replace_all(&original_text, replacement).to_string();
                set_text_with_offset(
                    &mut self.task_description,
                    &replaced_text,
                    &before_yank,
                    first_match.start(),
                );
                Ok(count)
            } else {
                Ok(0)
            }
        } else {
            if search.is_empty() {
                return Err("Search text cannot be empty".to_string());
            }
            let matches: Vec<_> = original_text.match_indices(search).collect();
            if matches.is_empty() {
                return Ok(0);
            }
            let replaced_text = original_text.replace(search, replacement);
            set_text_with_offset(
                &mut self.task_description,
                &replaced_text,
                &before_yank,
                matches[0].0,
            );
            Ok(matches.len())
        }
    }

    fn handle_settings_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc => {
                self.modal_state = ModalState::None;
            }
            KeyCode::Enter => {
                self.apply_settings_form();
            }
            KeyCode::Tab => {
                self.settings_form.focus_next();
            }
            KeyCode::BackTab => {
                self.settings_form.focus_prev();
            }
            KeyCode::Up => {
                self.settings_form.focus_prev();
            }
            KeyCode::Down => {
                self.settings_form.focus_next();
            }
            _ => {
                let input = self.settings_form.focused_input_mut();
                let _ = match key.code {
                    KeyCode::Char(c) => input.handle(InputRequest::InsertChar(c)),
                    KeyCode::Backspace => input.handle(InputRequest::DeletePrevChar),
                    KeyCode::Delete => input.handle(InputRequest::DeleteNextChar),
                    KeyCode::Left => input.handle(InputRequest::GoToPrevChar),
                    KeyCode::Right => input.handle(InputRequest::GoToNextChar),
                    KeyCode::Home => input.handle(InputRequest::GoToStart),
                    KeyCode::End => input.handle(InputRequest::GoToEnd),
                    _ => None,
                };
            }
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
                        modal.selected_index =
                            (modal.selected_index + 1).min(modal.options.len() - 1);
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

        if matches!(self.modal_state, ModalState::ShortcutHelp) {
            if let Some(modal) = self.shortcut_help_modal.as_mut() {
                match key.code {
                    KeyCode::Esc => {
                        self.close_modal();
                    }
                    KeyCode::Up => {
                        if modal.scroll > 0 {
                            modal.scroll -= 1;
                        }
                    }
                    KeyCode::Down => {
                        let max_scroll =
                            modal.entries.len().saturating_sub(SHORTCUT_HELP_VISIBLE_ROWS);
                        if modal.scroll < max_scroll {
                            modal.scroll += 1;
                        }
                    }
                    KeyCode::PageUp => {
                        modal.scroll = modal.scroll.saturating_sub(SHORTCUT_HELP_VISIBLE_ROWS);
                    }
                    KeyCode::PageDown => {
                        let max_scroll =
                            modal.entries.len().saturating_sub(SHORTCUT_HELP_VISIBLE_ROWS);
                        modal.scroll = (modal.scroll + SHORTCUT_HELP_VISIBLE_ROWS).min(max_scroll);
                    }
                    _ => {}
                }
            }
            return false;
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

    fn open_go_to_line_modal(&mut self) {
        self.modal_state = ModalState::GoToLine;
        self.status_message = None;
        let mut input = Input::default();
        let current_line = self.task_description.cursor().0 + 1;
        for ch in current_line.to_string().chars() {
            input.handle(InputRequest::InsertChar(ch));
        }
        let max_line = self.task_description.lines().len().max(1);
        self.goto_line_modal = Some(GoToLineModal {
            input,
            max_line,
            error: None,
        });
    }

    fn open_shortcut_help_modal(&mut self) {
        self.modal_state = ModalState::ShortcutHelp;
        self.status_message = None;
        let entries = self
            .shortcut_config
            .all_bindings()
            .into_iter()
            .filter(|entry| !entry.bindings.is_empty())
            .collect();
        self.shortcut_help_modal = Some(ShortcutHelpModal { entries, scroll: 0 });
    }

    fn open_find_replace_modal(&mut self, is_regex: bool) {
        self.modal_state = ModalState::FindReplace;
        self.status_message = None;
        let mut search_input = Input::default();
        if let Some(pattern) = self.task_description.search_pattern() {
            for ch in pattern.as_str().chars() {
                search_input.handle(InputRequest::InsertChar(ch));
            }
        }
        self.find_replace_modal = Some(FindReplaceModal {
            search_input,
            replace_input: Input::default(),
            is_regex,
            stage: FindReplaceStage::EnterSearch,
            error: None,
        });
    }

    fn close_modal(&mut self) {
        self.modal_state = ModalState::None;
        self.fuzzy_modal = None;
        self.model_selection_modal = None;
        self.goto_line_modal = None;
        self.find_replace_modal = None;
        self.shortcut_help_modal = None;
        self.search_mode = SearchMode::None;
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
                                if let Some((file, added, removed)) =
                                    files.choose(&mut rand::thread_rng())
                                {
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

fn run_app_internal(
    running: &Arc<AtomicBool>,
    enable_raw_mode: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Log run_app start
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open("key_log.txt") {
        let _ = writeln!(file, "=== run_app started ===");
    }
    // Setup terminal with state tracking
    setup_terminal(enable_raw_mode)?;
    let mut stdout = io::stdout();
    queue!(
        stdout,
        PushKeyboardEnhancementFlags(
            KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
                | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
        ),
        EnableMouseCapture
    )?;
    KB_FLAGS_PUSHED.store(true, Ordering::SeqCst);
    MOUSE_CAPTURE_ENABLED.store(true, Ordering::SeqCst);
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    let theme = Theme::default();

    // Initialize image picker and logo protocol for logo rendering
    let (image_picker, mut logo_protocol) = initialize_logo_rendering(theme.bg);

    // Initialize app state
    let mut app_state = AppState::new();
    let mut tasks = create_sample_tasks();
    app_state.refresh_filter_options(&tasks);

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

            app_state.poll_autocomplete();

            terminal.draw(|frame| {
                let size = frame.area();
                app_state.last_textarea_area.set(None);
                app_state.interactive_areas.clear();

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
                render_header(frame, main_layout[0], &theme, &mut app_state, image_picker.as_ref(), logo_protocol.as_mut());

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
                let mut display_items = Vec::new();
                let visible_indices = app_state.visible_task_indices(&tasks);

                for &idx in visible_indices.iter() {
                    if matches!(tasks[idx].state, TaskState::Draft) {
                        display_items.push(DisplayItem::Task(idx));
                        display_items.push(DisplayItem::Spacer);
                    }
                }

                display_items.push(DisplayItem::FilterBar);
                display_items.push(DisplayItem::Spacer);

                for (idx, task) in tasks.iter().enumerate() {
                    if !matches!(task.state, TaskState::Draft) && app_state.task_matches_filters(task) {
                        display_items.push(DisplayItem::Task(idx));
                        display_items.push(DisplayItem::Spacer);
                    }
                }

                if matches!(display_items.last(), Some(DisplayItem::Spacer)) {
                    display_items.pop();
                }

                let mut item_rects: Vec<(DisplayItem, Rect)> = Vec::new();
                // Track virtual y position (includes scrolled content)
                let mut virtual_y: u16 = 0;
                // Track screen y position (actual render position)
                let mut screen_y = tasks_area.y;
                let area_bottom = tasks_area.y.saturating_add(tasks_area.height);
                let scroll_offset = app_state.scroll_offset;

                for item in display_items {
                    let item_height = match item {
                        DisplayItem::Spacer => 1,
                        DisplayItem::FilterBar => 1,
                        DisplayItem::Task(idx) => {
                            let h = tasks[idx].height(&app_state);
                            if h == 0 {
                                continue;
                            }
                            h
                        }
                    };

                    // Check if this item is visible (not scrolled past)
                    let item_bottom = virtual_y.saturating_add(item_height);

                    // Skip items that are completely above the scroll offset
                    if item_bottom <= scroll_offset {
                        virtual_y = item_bottom;
                        continue;
                    }

                    // Calculate visible portion of this item
                    let visible_top_offset = if virtual_y < scroll_offset {
                        scroll_offset.saturating_sub(virtual_y)
                    } else {
                        0
                    };

                    let visible_height = item_height.saturating_sub(visible_top_offset);

                    // Stop if we've filled the screen
                    if screen_y >= area_bottom {
                        break;
                    }

                    // Clip visible height to remaining screen space
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

                for (item, rect) in item_rects {
                    match item {
                        DisplayItem::Spacer => {
                            frame.render_widget(Paragraph::new("").style(Style::default().bg(theme.bg)), rect);
                        }
                        DisplayItem::FilterBar => {
                            render_filter_bar(frame, rect, &mut app_state, &theme);
                        }
                        DisplayItem::Task(idx) => {
                            let task = &tasks[idx];
                            let is_selected = matches!(app_state.focus_element, FocusElement::TaskCard(sel) if sel == idx)
                                || (idx == 0
                                    && matches!(task.state, TaskState::Draft)
                                    && matches!(app_state.focus_element,
                                        FocusElement::TaskDescription |
                                        FocusElement::RepositoryButton |
                                        FocusElement::BranchButton |
                                        FocusElement::ModelButton |
                                        FocusElement::GoButton));
                            app_state.interactive_areas.push(InteractiveArea {
                                rect,
                                action: MouseAction::SelectCard(idx),
                            });
                            task.render(frame, rect, &mut app_state, &theme, is_selected, idx);
                        }
                    }
                }

                // Render footer
                render_footer(frame, main_layout[2], &mut app_state, &tasks, &theme);

                // Render modal if active
                if let Some(modal) = &app_state.fuzzy_modal {
                    render_fuzzy_modal(frame, modal, size, &theme);
                }
                if let Some(modal) = &app_state.model_selection_modal {
                    render_model_selection_modal(frame, modal, size, &theme);
                }
                if let Some(modal) = &app_state.goto_line_modal {
                    render_go_to_line_modal(frame, modal, size, &theme);
                }
                if let Some(modal) = &app_state.find_replace_modal {
                    render_find_replace_modal(frame, modal, size, &theme);
                }
                if matches!(app_state.modal_state, ModalState::Settings) {
                    render_settings_dialog(frame, &app_state, size, &theme);
                }
                if let Some(modal) = &app_state.shortcut_help_modal {
                    render_shortcut_help_modal(frame, modal, size, &theme);
                }
                if let Some(area) = app_state.last_textarea_area.get() {
                    app_state
                        .autocomplete
                        .render(
                            frame,
                            area,
                            &app_state.task_description,
                            &theme,
                            app_state.autocomplete_background,
                        );
                }
                // Render filter editor last to ensure it appears above all other UI elements
                if let Some(editor) = app_state.filter_editor.as_mut() {
                    render_filter_editor(frame, &theme, editor);
                }

                // Auto-scroll to keep selected card visible
                app_state.ensure_selected_card_visible(&tasks, tasks_area.height);
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
                    } else if let Event::Mouse(mouse_event) = ev {
                        if app_state.handle_mouse(mouse_event, &mut tasks) {
                            break;
                        }
                    }
                }
                recv(rx_tick) -> _ => {
                    // Periodic tick - could be used for animations, but currently just continue
                    // This keeps the app responsive even when no input events occur
                    app_state.autocomplete_on_tick();
                    app_state.poll_autocomplete();
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
fn pad_to_cell_width(
    image: DynamicImage,
    bg_color: Color,
    cell_width: Option<u16>,
) -> DynamicImage {
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
static MOUSE_CAPTURE_ENABLED: AtomicBool = AtomicBool::new(false);

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

    if MOUSE_CAPTURE_ENABLED.load(Ordering::SeqCst) {
        let _ = crossterm::execute!(std::io::stdout(), DisableMouseCapture);
        MOUSE_CAPTURE_ENABLED.store(false, Ordering::SeqCst);
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
        println!(
            "Usage: {} [OPTIONS]",
            args.get(0).unwrap_or(&"tui-exploration".to_string())
        );
        println!();
        println!("Options:");
        println!(
            "  --no-raw-mode    Disable raw mode (useful for debugging, disables keyboard input)"
        );
        println!("  --help, -h       Show this help message");
        std::process::exit(0);
    }

    Args { enable_raw_mode }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args();

    // Simple test logging
    println!("Main function reached");
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open("key_log.txt") {
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
    })
    .expect("Error setting Ctrl-C handler");

    // Install panic hook for cleanup on panic
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        cleanup_terminal();
        // Call the default panic handler
        default_panic(panic_info);
    }));

    // Run the app with panic-safe cleanup
    let result = std::panic::catch_unwind(|| run_app_internal(&running, args.enable_raw_mode));

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

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyModifiers};

    /// Helper function to create a key event for testing
    fn key_event(code: KeyCode, modifiers: KeyModifiers) -> crossterm::event::KeyEvent {
        crossterm::event::KeyEvent::new(code, modifiers)
    }

    #[test]
    fn test_full_navigation_cycle_with_text_area_boundaries() {
        let mut app_state = AppState::new();

        // Create test tasks: draft, active, completed
        let mut tasks: Vec<TaskCard> = Vec::new();

        // Draft task with multi-line description
        let draft_task = TaskCard {
            title: "Draft Task".to_string(),
            repository: "test/repo".to_string(),
            branch: "main".to_string(),
            agents: vec![SelectedModel {
                name: "Claude".to_string(),
                count: 1,
            }],
            state: TaskState::Draft,
            timestamp: "2023-01-01 12:00:00".to_string(),
            activity: vec![],
            delivery_indicators: None,
            current_tool_execution: None,
            creator: "test".to_string(),
        };
        tasks.push(draft_task);

        // Active task
        let active_task = TaskCard {
            title: "Active Task".to_string(),
            repository: "test/repo".to_string(),
            branch: "feature/x".to_string(),
            agents: vec![SelectedModel {
                name: "GPT-4".to_string(),
                count: 1,
            }],
            state: TaskState::Active,
            timestamp: "2023-01-01 12:05:00".to_string(),
            activity: vec!["Thinking...".to_string(), "Tool usage: grep".to_string()],
            delivery_indicators: None,
            current_tool_execution: None,
            creator: "test".to_string(),
        };
        tasks.push(active_task);

        // Completed task
        let completed_task = TaskCard {
            title: "Completed Task".to_string(),
            repository: "test/repo".to_string(),
            branch: "main".to_string(),
            agents: vec![SelectedModel {
                name: "Claude".to_string(),
                count: 1,
            }],
            state: TaskState::Completed,
            timestamp: "2023-01-01 12:10:00".to_string(),
            activity: vec![],
            delivery_indicators: None,
            current_tool_execution: None,
            creator: "test".to_string(),
        };
        tasks.push(completed_task);

        // Set up multi-line text in draft task
        app_state.task_description = TextArea::from(vec![
            "Line 1 of draft task description".to_string(),
            "Line 2 of draft task description".to_string(),
            "Line 3 of draft task description".to_string(),
        ]);
        // Move to the last line by going down twice
        app_state.task_description.move_cursor(tui_textarea::CursorMove::Down);
        app_state.task_description.move_cursor(tui_textarea::CursorMove::Down);

        // Initially focused on first draft card (TaskCard(0)) with text area focused
        app_state.selected_card = 0;
        app_state.focus_element = FocusElement::TaskDescription;

        // === Test navigation cycle as described in PRD ===

        // 1. Start with draft card text area focused (initial state)
        assert_eq!(app_state.focus_element, FocusElement::TaskDescription);
        assert_eq!(app_state.selected_card, 0);

        // Cursor should be on line 3 (bottom line)
        let (cursor_row, _) = app_state.task_description.cursor();
        assert_eq!(
            cursor_row, 2,
            "Expected cursor on line 3 (index 2), got {}",
            cursor_row
        );

        // 2. Press UP - should move cursor up within text area (to line 2)
        let _ = app_state.handle_key(key_event(KeyCode::Up, KeyModifiers::NONE), &mut tasks);
        let (cursor_row, _) = app_state.task_description.cursor();
        assert_eq!(cursor_row, 1); // Moved to line 2 (0-indexed)
        assert_eq!(app_state.focus_element, FocusElement::TaskDescription); // Still in text area

        // 3. Press UP again - should move cursor up within text area (to line 1)
        let _ = app_state.handle_key(key_event(KeyCode::Up, KeyModifiers::NONE), &mut tasks);
        let (cursor_row, _) = app_state.task_description.cursor();
        assert_eq!(cursor_row, 0); // Moved to line 1 (0-indexed)
        assert_eq!(app_state.focus_element, FocusElement::TaskDescription); // Still in text area

        // 4. Press UP again - cursor is at top line, should move focus to settings button
        let _ = app_state.handle_key(key_event(KeyCode::Up, KeyModifiers::NONE), &mut tasks);
        assert_eq!(app_state.focus_element, FocusElement::SettingsButton);

        // 5. Press DOWN - should move from settings button to first task card (draft)
        let _ = app_state.handle_key(key_event(KeyCode::Down, KeyModifiers::NONE), &mut tasks);
        assert_eq!(app_state.focus_element, FocusElement::TaskDescription); // Auto-focus text area
        assert_eq!(app_state.selected_card, 0);

        // 6. Press DOWN - cursor is on line 1, should move down within text area (to line 2)
        let _ = app_state.handle_key(key_event(KeyCode::Down, KeyModifiers::NONE), &mut tasks);
        let (cursor_row, _) = app_state.task_description.cursor();
        assert_eq!(cursor_row, 1); // Moved to line 2
        assert_eq!(app_state.focus_element, FocusElement::TaskDescription);

        // 7. Press DOWN again - should move down within text area (to line 3)
        let _ = app_state.handle_key(key_event(KeyCode::Down, KeyModifiers::NONE), &mut tasks);
        let (cursor_row, _) = app_state.task_description.cursor();
        assert_eq!(cursor_row, 2); // Moved to line 3 (bottom)
        assert_eq!(app_state.focus_element, FocusElement::TaskDescription);

        // 8. Press DOWN again - cursor is at bottom line, should move to filter bar separator
        let _ = app_state.handle_key(key_event(KeyCode::Down, KeyModifiers::NONE), &mut tasks);
        assert_eq!(app_state.focus_element, FocusElement::FilterBarLine);

        // 9. Press DOWN - should move from filter bar to first non-draft task (active task)
        let _ = app_state.handle_key(key_event(KeyCode::Down, KeyModifiers::NONE), &mut tasks);
        assert_eq!(app_state.focus_element, FocusElement::TaskCard(1)); // Active task at index 1
        assert_eq!(app_state.selected_card, 1);

        // 10. Press DOWN - should move to next task (completed task)
        let _ = app_state.handle_key(key_event(KeyCode::Down, KeyModifiers::NONE), &mut tasks);
        assert_eq!(app_state.focus_element, FocusElement::TaskCard(2)); // Completed task at index 2
        assert_eq!(app_state.selected_card, 2);

        // 11. Press DOWN - at last task, should move to filter controls (first filter)
        let _ = app_state.handle_key(key_event(KeyCode::Down, KeyModifiers::NONE), &mut tasks);
        assert_eq!(
            app_state.focus_element,
            FocusElement::Filter(FilterControl::Repository)
        );

        // 12. Press DOWN - should wrap around to settings button
        let _ = app_state.handle_key(key_event(KeyCode::Down, KeyModifiers::NONE), &mut tasks);
        assert_eq!(app_state.focus_element, FocusElement::SettingsButton);

        // === Test the complete navigation cycle is closed ===

        // 13. Press DOWN again - should go back to first draft card
        let _ = app_state.handle_key(key_event(KeyCode::Down, KeyModifiers::NONE), &mut tasks);
        assert_eq!(app_state.focus_element, FocusElement::TaskDescription);
        assert_eq!(app_state.selected_card, 0);

        // Verify we're back at the starting point
        let (cursor_row, _) = app_state.task_description.cursor();
        assert_eq!(cursor_row, 2); // Back on line 3 (bottom line)
    }

    #[test]
    fn test_navigation_with_no_draft_tasks() {
        let mut app_state = AppState::new();

        // Create only non-draft tasks: active and completed
        let mut tasks: Vec<TaskCard> = Vec::new();

        let active_task = TaskCard {
            title: "Active Task".to_string(),
            repository: "test/repo".to_string(),
            branch: "feature/x".to_string(),
            agents: vec![SelectedModel {
                name: "GPT-4".to_string(),
                count: 1,
            }],
            state: TaskState::Active,
            timestamp: "2023-01-01 12:05:00".to_string(),
            activity: vec!["Working...".to_string()],
            delivery_indicators: None,
            current_tool_execution: None,
            creator: "test".to_string(),
        };
        tasks.push(active_task);

        let completed_task = TaskCard {
            title: "Completed Task".to_string(),
            repository: "test/repo".to_string(),
            branch: "main".to_string(),
            agents: vec![SelectedModel {
                name: "Claude".to_string(),
                count: 1,
            }],
            state: TaskState::Completed,
            timestamp: "2023-01-01 12:10:00".to_string(),
            activity: vec![],
            delivery_indicators: None,
            current_tool_execution: None,
            creator: "test".to_string(),
        };
        tasks.push(completed_task);

        // Initially focused on first task card (active task)
        app_state.selected_card = 0;
        app_state.focus_element = FocusElement::TaskCard(0);

        // 1. Start with active task focused
        assert_eq!(app_state.focus_element, FocusElement::TaskCard(0));
        assert_eq!(app_state.selected_card, 0);

        // 2. Press UP - should move to settings button (since no draft tasks)
        let _ = app_state.handle_key(key_event(KeyCode::Up, KeyModifiers::NONE), &mut tasks);
        assert_eq!(app_state.focus_element, FocusElement::SettingsButton);

        // 3. Press DOWN - should move to first task (active)
        let _ = app_state.handle_key(key_event(KeyCode::Down, KeyModifiers::NONE), &mut tasks);
        assert_eq!(app_state.focus_element, FocusElement::TaskCard(0));
        assert_eq!(app_state.selected_card, 0);

        // 4. Press DOWN - should move to next task (completed)
        let _ = app_state.handle_key(key_event(KeyCode::Down, KeyModifiers::NONE), &mut tasks);
        assert_eq!(app_state.focus_element, FocusElement::TaskCard(1));
        assert_eq!(app_state.selected_card, 1);

        // 5. Press DOWN - should move to filter controls
        let _ = app_state.handle_key(key_event(KeyCode::Down, KeyModifiers::NONE), &mut tasks);
        assert_eq!(
            app_state.focus_element,
            FocusElement::Filter(FilterControl::Repository)
        );

        // 6. Press DOWN - should wrap to settings button
        let _ = app_state.handle_key(key_event(KeyCode::Down, KeyModifiers::NONE), &mut tasks);
        assert_eq!(app_state.focus_element, FocusElement::SettingsButton);
    }
}
