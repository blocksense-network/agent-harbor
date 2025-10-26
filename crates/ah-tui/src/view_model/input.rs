// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Input handling and keyboard operation mapping
//!
//! This module provides shared functionality for mapping key events to keyboard operations
//! and managing overlayable input states.

use once_cell::sync::Lazy;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::settings::KeyboardOperation;

/// Map a key event to a keyboard operation using the provided settings
pub fn key_event_to_operation(
    key_event: &KeyEvent,
    settings: &crate::Settings,
) -> Option<KeyboardOperation> {
    let KeyEvent {
        code, modifiers, ..
    } = key_event;

    // Special hardcoded handling for Ctrl+D (duplicate line) - ensure it works
    if let (KeyCode::Char('d'), mods) = (code, modifiers) {
        if mods.contains(KeyModifiers::CONTROL) {
            return Some(KeyboardOperation::DuplicateLineSelection);
        }
    }

    // Get the keymap configuration from settings and check all possible keyboard operations
    let keymap = settings.keymap();

    // Define all operations we care about in the TUI that should be configurable
    let operations_to_check = vec![
        KeyboardOperation::MoveToPreviousLine,
        KeyboardOperation::MoveToNextLine,
        KeyboardOperation::MoveToNextField,
        KeyboardOperation::MoveToPreviousField,
        KeyboardOperation::MoveToBeginningOfLine,
        KeyboardOperation::MoveToEndOfLine,
        KeyboardOperation::MoveBackwardOneCharacter,
        KeyboardOperation::MoveForwardOneCharacter,
        KeyboardOperation::MoveForwardOneWord,
        KeyboardOperation::MoveBackwardOneWord,
        KeyboardOperation::MoveToBeginningOfSentence,
        KeyboardOperation::MoveToEndOfSentence,
        KeyboardOperation::ScrollDownOneScreen,
        KeyboardOperation::ScrollUpOneScreen,
        KeyboardOperation::RecenterScreenOnCursor,
        KeyboardOperation::MoveToBeginningOfDocument,
        KeyboardOperation::MoveToEndOfDocument,
        KeyboardOperation::MoveToBeginningOfParagraph,
        KeyboardOperation::MoveToEndOfParagraph,
        KeyboardOperation::DeleteCharacterBackward,
        KeyboardOperation::DeleteCharacterForward,
        KeyboardOperation::DeleteWordForward,
        KeyboardOperation::DeleteWordBackward,
        KeyboardOperation::DeleteToEndOfLine,
        KeyboardOperation::DeleteToBeginningOfLine,
        KeyboardOperation::Cut,
        KeyboardOperation::Copy,
        KeyboardOperation::Paste,
        KeyboardOperation::CycleThroughClipboard,
        KeyboardOperation::Undo,
        KeyboardOperation::Redo,
        KeyboardOperation::OpenNewLine,
        KeyboardOperation::TransposeCharacters,
        KeyboardOperation::TransposeWords,
        KeyboardOperation::UppercaseWord,
        KeyboardOperation::LowercaseWord,
        KeyboardOperation::CapitalizeWord,
        KeyboardOperation::JoinLines,
        KeyboardOperation::Bold,
        KeyboardOperation::Italic,
        KeyboardOperation::Underline,
        KeyboardOperation::ToggleComment,
        KeyboardOperation::DuplicateLineSelection,
        KeyboardOperation::MoveLineUp,
        KeyboardOperation::MoveLineDown,
        KeyboardOperation::IndentRegion,
        KeyboardOperation::DedentRegion,
        KeyboardOperation::IncrementalSearchForward,
        KeyboardOperation::IncrementalSearchBackward,
        KeyboardOperation::FindNext,
        KeyboardOperation::FindPrevious,
        KeyboardOperation::SelectAll,
        KeyboardOperation::SelectWordUnderCursor,
        KeyboardOperation::SetMark,
        KeyboardOperation::NewDraft,
        KeyboardOperation::DismissOverlay,
    ];

    // Find the first operation that matches this key event
    for operation in operations_to_check {
        if keymap.matches(operation, key_event) {
            return Some(operation);
        }
    }

    // No configured operation matched
    None
}

/// Input state management for overlayable keyboard shortcuts
#[derive(Debug, Clone)]
pub struct InputState {
    /// Active keyboard shortcuts
    pub shortcuts: Vec<KeyboardShortcut>,
    /// Prominent keys to display in the footer
    pub prominent_keys: Vec<String>,
}

impl Default for InputState {
    fn default() -> Self {
        Self {
            shortcuts: Vec::new(),
            prominent_keys: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct KeyboardShortcut {
    pub key: String,
    pub description: String,
    pub operation: KeyboardOperation,
}

/// Stack of input states for overlayable input handling
#[derive(Debug, Clone, Default)]
pub struct InputStateStack {
    states: Vec<InputState>,
}

impl InputStateStack {
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a new input state onto the stack
    pub fn push(&mut self, state: InputState) {
        self.states.push(state);
    }

    /// Pop the top input state from the stack
    pub fn pop(&mut self) -> Option<InputState> {
        self.states.pop()
    }

    /// Get the current active input state (top of stack, or default)
    pub fn current(&self) -> &InputState {
        static DEFAULT: once_cell::sync::Lazy<InputState> =
            once_cell::sync::Lazy::new(InputState::default);
        self.states.last().unwrap_or(&DEFAULT)
    }

    /// Check if ESC should dismiss the current overlay
    pub fn should_dismiss_on_esc(&self) -> bool {
        !self.states.is_empty()
    }
}
