// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Font style for displaying symbols and icons
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FontStyle {
    Unicode,
    NerdFont,
    Ascii,
}

impl Default for FontStyle {
    fn default() -> Self {
        FontStyle::Unicode
    }
}

/// Dialog style for selection interfaces
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SelectionDialogStyle {
    Modal,
    Inline,
    Default,
}

impl Default for SelectionDialogStyle {
    fn default() -> Self {
        SelectionDialogStyle::Default
    }
}

/// Meta key configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MetaKey {
    Alt,
    Option,
}

impl Default for MetaKey {
    fn default() -> Self {
        MetaKey::Alt
    }
}

/// Platform-specific keyboard shortcut support
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Platform {
    Pc,
    Mac,
}

impl Platform {
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Platform::Mac
        } else {
            Platform::Pc
        }
    }
}

/// Keyboard operations that can be bound to shortcuts
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeyboardOperation {
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
    MoveToNextField,
    MoveToPreviousField,
    DeleteToBeginningOfLine,
    DismissOverlay,

    // Text Transformation
    UppercaseWord,
    LowercaseWord,
    CapitalizeWord,
    JustifyParagraph,
    JoinLines,

    // Formatting (Markdown Style)
    Bold,
    Italic,
    Underline,

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
    SelectWordUnderCursor,
    SelectAll,

    // Application Actions
    NewDraft,
    CreateAndFocus,
    CreateInSplitView,
    CreateInSplitViewAndFocus,
    CreateInHorizontalSplit,
    CreateInVerticalSplit,
}

impl KeyboardOperation {
    /// Get the localization key for this operation
    pub fn localization_key(&self) -> &'static str {
        match self {
            KeyboardOperation::MoveToBeginningOfLine => "shortcut-move-to-beginning-of-line",
            KeyboardOperation::MoveToEndOfLine => "shortcut-move-to-end-of-line",
            KeyboardOperation::MoveForwardOneCharacter => "shortcut-move-forward-one-character",
            KeyboardOperation::MoveBackwardOneCharacter => "shortcut-move-backward-one-character",
            KeyboardOperation::MoveToNextLine => "shortcut-move-to-next-line",
            KeyboardOperation::MoveToPreviousLine => "shortcut-move-to-previous-line",
            KeyboardOperation::MoveForwardOneWord => "shortcut-move-forward-one-word",
            KeyboardOperation::MoveBackwardOneWord => "shortcut-move-backward-one-word",
            KeyboardOperation::MoveToBeginningOfSentence => {
                "shortcut-move-to-beginning-of-sentence"
            }
            KeyboardOperation::MoveToEndOfSentence => "shortcut-move-to-end-of-sentence",
            KeyboardOperation::ScrollDownOneScreen => "shortcut-scroll-down-one-screen",
            KeyboardOperation::ScrollUpOneScreen => "shortcut-scroll-up-one-screen",
            KeyboardOperation::RecenterScreenOnCursor => "shortcut-recenter-screen-on-cursor",
            KeyboardOperation::MoveToBeginningOfDocument => {
                "shortcut-move-to-beginning-of-document"
            }
            KeyboardOperation::MoveToEndOfDocument => "shortcut-move-to-end-of-document",
            KeyboardOperation::MoveToBeginningOfParagraph => {
                "shortcut-move-to-beginning-of-paragraph"
            }
            KeyboardOperation::MoveToEndOfParagraph => "shortcut-move-to-end-of-paragraph",
            KeyboardOperation::GoToLineNumber => "shortcut-go-to-line-number",
            KeyboardOperation::MoveToMatchingParenthesis => "shortcut-move-to-matching-parenthesis",
            KeyboardOperation::DeleteCharacterForward => "shortcut-delete-character-forward",
            KeyboardOperation::DeleteCharacterBackward => "shortcut-delete-character-backward",
            KeyboardOperation::DeleteWordForward => "shortcut-delete-word-forward",
            KeyboardOperation::DeleteWordBackward => "shortcut-delete-word-backward",
            KeyboardOperation::DeleteToEndOfLine => "shortcut-delete-to-end-of-line",
            KeyboardOperation::Cut => "shortcut-cut",
            KeyboardOperation::Copy => "shortcut-copy",
            KeyboardOperation::Paste => "shortcut-paste",
            KeyboardOperation::CycleThroughClipboard => "shortcut-cycle-through-clipboard",
            KeyboardOperation::TransposeCharacters => "shortcut-transpose-characters",
            KeyboardOperation::TransposeWords => "shortcut-transpose-words",
            KeyboardOperation::Undo => "shortcut-undo",
            KeyboardOperation::Redo => "shortcut-redo",
            KeyboardOperation::OpenNewLine => "shortcut-open-new-line",
            KeyboardOperation::IndentOrComplete => "shortcut-indent-or-complete",
            KeyboardOperation::MoveToNextField => "shortcut-move-to-next-field",
            KeyboardOperation::MoveToPreviousField => "shortcut-move-to-previous-field",
            KeyboardOperation::DeleteToBeginningOfLine => "shortcut-delete-to-beginning-of-line",
            KeyboardOperation::DismissOverlay => "shortcut-dismiss-overlay",
            KeyboardOperation::UppercaseWord => "shortcut-uppercase-word",
            KeyboardOperation::LowercaseWord => "shortcut-lowercase-word",
            KeyboardOperation::CapitalizeWord => "shortcut-capitalize-word",
            KeyboardOperation::JustifyParagraph => "shortcut-justify-paragraph",
            KeyboardOperation::JoinLines => "shortcut-join-lines",
            KeyboardOperation::Bold => "shortcut-bold",
            KeyboardOperation::Italic => "shortcut-italic",
            KeyboardOperation::Underline => "shortcut-underline",
            KeyboardOperation::ToggleComment => "shortcut-toggle-comment",
            KeyboardOperation::DuplicateLineSelection => "shortcut-duplicate-line-selection",
            KeyboardOperation::MoveLineUp => "shortcut-move-line-up",
            KeyboardOperation::MoveLineDown => "shortcut-move-line-down",
            KeyboardOperation::IndentRegion => "shortcut-indent-region",
            KeyboardOperation::DedentRegion => "shortcut-dedent-region",
            KeyboardOperation::IncrementalSearchForward => "shortcut-incremental-search-forward",
            KeyboardOperation::IncrementalSearchBackward => "shortcut-incremental-search-backward",
            KeyboardOperation::FindAndReplace => "shortcut-find-and-replace",
            KeyboardOperation::FindAndReplaceWithRegex => "shortcut-find-and-replace-with-regex",
            KeyboardOperation::FindNext => "shortcut-find-next",
            KeyboardOperation::FindPrevious => "shortcut-find-previous",
            KeyboardOperation::SetMark => "shortcut-set-mark",
            KeyboardOperation::SelectAll => "shortcut-select-all",
            KeyboardOperation::SelectWordUnderCursor => "shortcut-select-word-under-cursor",
            KeyboardOperation::NewDraft => "shortcut-new-draft",
            KeyboardOperation::CreateAndFocus => "shortcut-create-and-focus",
            KeyboardOperation::CreateInSplitView => "shortcut-create-in-split-view",
            KeyboardOperation::CreateInSplitViewAndFocus => {
                "shortcut-create-in-split-view-and-focus"
            }
            KeyboardOperation::CreateInHorizontalSplit => "shortcut-create-in-horizontal-split",
            KeyboardOperation::CreateInVerticalSplit => "shortcut-create-in-vertical-split",
        }
    }

    /// Get the default English description for this operation
    pub fn english_description(&self) -> &'static str {
        match self {
            KeyboardOperation::MoveToBeginningOfLine => "Move cursor to beginning of line",
            KeyboardOperation::MoveToEndOfLine => "Move cursor to end of line",
            KeyboardOperation::MoveForwardOneCharacter => "Move cursor forward one character",
            KeyboardOperation::MoveBackwardOneCharacter => "Move cursor backward one character",
            KeyboardOperation::MoveToNextLine => "Move cursor to next line",
            KeyboardOperation::MoveToPreviousLine => "Move cursor to previous line",
            KeyboardOperation::MoveForwardOneWord => "Move cursor forward one word",
            KeyboardOperation::MoveBackwardOneWord => "Move cursor backward one word",
            KeyboardOperation::MoveToBeginningOfSentence => "Move cursor to beginning of sentence",
            KeyboardOperation::MoveToEndOfSentence => "Move cursor to end of sentence",
            KeyboardOperation::ScrollDownOneScreen => "Scroll viewport down",
            KeyboardOperation::ScrollUpOneScreen => "Scroll viewport up",
            KeyboardOperation::RecenterScreenOnCursor => "Recenter screen on cursor",
            KeyboardOperation::MoveToBeginningOfDocument => "Move cursor to beginning of document",
            KeyboardOperation::MoveToEndOfDocument => "Move cursor to end of document",
            KeyboardOperation::MoveToBeginningOfParagraph => {
                "Move cursor to beginning of paragraph"
            }
            KeyboardOperation::MoveToEndOfParagraph => "Move cursor to end of paragraph",
            KeyboardOperation::GoToLineNumber => "Open go to line dialog",
            KeyboardOperation::MoveToMatchingParenthesis => "Jump to matching parenthesis",
            KeyboardOperation::DeleteCharacterForward => "Delete character forward",
            KeyboardOperation::DeleteCharacterBackward => "Delete character backward",
            KeyboardOperation::DeleteWordForward => "Delete word forward",
            KeyboardOperation::DeleteWordBackward => "Delete word backward",
            KeyboardOperation::DeleteToEndOfLine => "Delete to end of line",
            KeyboardOperation::Cut => "Cut selection",
            KeyboardOperation::Copy => "Copy selection",
            KeyboardOperation::Paste => "Paste",
            KeyboardOperation::CycleThroughClipboard => "Cycle clipboard entries",
            KeyboardOperation::TransposeCharacters => "Transpose characters",
            KeyboardOperation::TransposeWords => "Transpose words",
            KeyboardOperation::Undo => "Undo last edit",
            KeyboardOperation::Redo => "Redo last edit",
            KeyboardOperation::OpenNewLine => "Open new line below",
            KeyboardOperation::IndentOrComplete => "Indent or complete",
            KeyboardOperation::MoveToNextField => "Move to next field",
            KeyboardOperation::MoveToPreviousField => "Move to previous field",
            KeyboardOperation::DeleteToBeginningOfLine => "Delete to beginning of line",
            KeyboardOperation::DismissOverlay => "Dismiss modal or quit",
            KeyboardOperation::UppercaseWord => "Uppercase word",
            KeyboardOperation::LowercaseWord => "Lowercase word",
            KeyboardOperation::CapitalizeWord => "Capitalize word",
            KeyboardOperation::JustifyParagraph => "Justify paragraph",
            KeyboardOperation::JoinLines => "Join lines",
            KeyboardOperation::Bold => "Toggle bold formatting",
            KeyboardOperation::Italic => "Toggle italic formatting",
            KeyboardOperation::Underline => "Toggle underline formatting",
            KeyboardOperation::ToggleComment => "Toggle comment",
            KeyboardOperation::DuplicateLineSelection => "Duplicate line or selection",
            KeyboardOperation::MoveLineUp => "Move line up",
            KeyboardOperation::MoveLineDown => "Move line down",
            KeyboardOperation::IndentRegion => "Indent region",
            KeyboardOperation::DedentRegion => "Dedent region",
            KeyboardOperation::IncrementalSearchForward => "Incremental search forward",
            KeyboardOperation::IncrementalSearchBackward => "Incremental search backward",
            KeyboardOperation::FindAndReplace => "Find and replace",
            KeyboardOperation::FindAndReplaceWithRegex => "Find and replace with regex",
            KeyboardOperation::FindNext => "Find next match",
            KeyboardOperation::FindPrevious => "Find previous match",
            KeyboardOperation::SetMark => "Set mark for selection",
            KeyboardOperation::SelectAll => "Select all text",
            KeyboardOperation::SelectWordUnderCursor => "Select word under cursor",
            KeyboardOperation::NewDraft => "Create new draft task",
            KeyboardOperation::CreateAndFocus => "Create task and focus",
            KeyboardOperation::CreateInSplitView => "Create task in split view",
            KeyboardOperation::CreateInSplitViewAndFocus => "Create task in split view and focus",
            KeyboardOperation::CreateInHorizontalSplit => "Create task in horizontal split",
            KeyboardOperation::CreateInVerticalSplit => "Create task in vertical split",
        }
    }
}

/// Enhanced key matcher with support for required/optional modifiers
#[derive(Debug, Clone, PartialEq)]
pub struct KeyMatcher {
    pub code: ratatui::crossterm::event::KeyCode,
    pub required: ratatui::crossterm::event::KeyModifiers,
    pub optional: ratatui::crossterm::event::KeyModifiers,
    pub char_lower: Option<char>,
}

impl KeyMatcher {
    /// Create a new key matcher
    pub fn new(
        code: ratatui::crossterm::event::KeyCode,
        required: ratatui::crossterm::event::KeyModifiers,
        optional: ratatui::crossterm::event::KeyModifiers,
        char_lower: Option<char>,
    ) -> Self {
        Self {
            code,
            required,
            optional,
            char_lower,
        }
    }

    /// Convert back to a displayable string representation
    pub fn to_string(&self) -> String {
        use ratatui::crossterm::event::{KeyCode, KeyModifiers};

        let mut parts = Vec::new();

        // Add required modifiers
        if self.required.contains(KeyModifiers::CONTROL) {
            parts.push("Ctrl");
        }
        if self.required.contains(KeyModifiers::ALT) {
            parts.push("Alt");
        }
        if self.required.contains(KeyModifiers::SHIFT) {
            parts.push("Shift");
        }
        if self.required.contains(KeyModifiers::SUPER) {
            parts.push("Cmd");
        }

        // Add key code
        let key_str = match &self.code {
            KeyCode::Char(c) => c.to_string(),
            KeyCode::Enter => "Enter".to_string(),
            KeyCode::Tab => "Tab".to_string(),
            KeyCode::Esc => "Esc".to_string(),
            KeyCode::Backspace => "Backspace".to_string(),
            KeyCode::Delete => "Delete".to_string(),
            KeyCode::Up => "Up".to_string(),
            KeyCode::Down => "Down".to_string(),
            KeyCode::Left => "Left".to_string(),
            KeyCode::Right => "Right".to_string(),
            KeyCode::Home => "Home".to_string(),
            KeyCode::End => "End".to_string(),
            KeyCode::PageUp => "PageUp".to_string(),
            KeyCode::PageDown => "PageDown".to_string(),
            _ => "Unknown".to_string(),
        };

        if !parts.is_empty() {
            format!("{}+{}", parts.join("+"), key_str)
        } else {
            key_str
        }
    }

    /// Check if this matcher matches a crossterm KeyEvent
    pub fn matches(&self, event: &ratatui::crossterm::event::KeyEvent) -> bool {
        use crossterm::event::{KeyCode, KeyModifiers};

        // Check key code first
        if !self.matches_code(&event.code) {
            return false;
        }

        // Special handling for cursor movement keys: allow SHIFT for text selection
        let is_cursor_key = matches!(
            event.code,
            KeyCode::Left
                | KeyCode::Right
                | KeyCode::Up
                | KeyCode::Down
                | KeyCode::Home
                | KeyCode::End
                | KeyCode::PageUp
                | KeyCode::PageDown
        );

        // Check modifiers
        for modifier in [
            KeyModifiers::CONTROL,
            KeyModifiers::ALT,
            KeyModifiers::SHIFT,
            KeyModifiers::SUPER,
        ] {
            let required = self.required.contains(modifier);
            let optional = self.optional.contains(modifier)
                || (modifier == KeyModifiers::SHIFT && is_cursor_key);
            let present = event.modifiers.contains(modifier);

            if required && !present {
                return false;
            }
            if !required && !optional && present {
                return false;
            }
        }

        true
    }

    fn matches_code(&self, code: &ratatui::crossterm::event::KeyCode) -> bool {
        use crossterm::event::KeyCode;

        match (&self.code, code) {
            (KeyCode::Char(expected), KeyCode::Char(actual)) => {
                if let Some(lower) = self.char_lower {
                    actual.to_ascii_lowercase() == lower
                } else {
                    actual == expected
                }
            }
            _ => self.code == *code,
        }
    }
}

/// Operation definition with platform-specific defaults
#[derive(Debug, Clone)]
pub struct KeyboardOperationDefinition {
    pub operation: KeyboardOperation,
    pub defaults: Vec<String>,
}

impl KeyboardOperationDefinition {
    pub fn new(operation: KeyboardOperation, defaults: Vec<String>) -> Self {
        Self {
            operation,
            defaults,
        }
    }

    pub fn get_defaults(&self, _platform: Platform) -> &[String] {
        &self.defaults
    }
}

/// Collection of parsed key bindings for an operation
#[derive(Debug, Clone, PartialEq)]
pub struct KeyboardShortcut {
    pub operation: KeyboardOperation,
    pub bindings: Vec<KeyMatcher>,
}

impl KeyboardShortcut {
    /// Create a new keyboard shortcut
    pub fn new(operation: KeyboardOperation, bindings: Vec<KeyMatcher>) -> Self {
        Self {
            operation,
            bindings,
        }
    }

    /// Check if any of the bindings match the given KeyEvent
    pub fn matches(&self, event: &ratatui::crossterm::event::KeyEvent) -> bool {
        self.bindings.iter().any(|matcher| matcher.matches(event))
    }

    /// Get display strings for all bindings
    pub fn display_strings(&self) -> Vec<String> {
        self.bindings.iter().map(|matcher| matcher.to_string()).collect()
    }
}

/// Localization context for keyboard shortcuts
pub struct KeyboardLocalization {
    pub locale: unic_langid::LanguageIdentifier,
}

impl KeyboardLocalization {
    /// Create a new localization context
    pub fn new(locale: unic_langid::LanguageIdentifier) -> Self {
        // In a real implementation, you'd create a FluentBundle and load .ftl files here
        Self { locale }
    }

    /// Get localized description for an operation
    pub fn get_description(&self, operation: KeyboardOperation) -> String {
        // For now, return English descriptions - in full implementation,
        // this would use fluent to get localized strings
        operation.english_description().to_string()
    }

    /// Get localized modifier name
    pub fn get_modifier_name(&self, modifier: &str) -> String {
        // For now, return English names - in full implementation,
        // this would use fluent for localization
        match modifier.to_lowercase().as_str() {
            "ctrl" => "Ctrl".to_string(),
            "alt" => "Alt".to_string(),
            "shift" => "Shift".to_string(),
            "cmd" | "super" | "meta" => "Cmd".to_string(),
            "option" => "Option".to_string(),
            _ => modifier.to_string(),
        }
    }
}

/// Enhanced error handling for keyboard shortcut parsing
#[derive(Debug, thiserror::Error)]
pub enum KeyboardShortcutError {
    #[error("shortcut must contain a key code, e.g. 'Enter' or 'Ctrl+Enter'")]
    MissingKey,
    #[error("unsupported modifier '{0}' - supported modifiers: Ctrl, Alt, Shift, Cmd, Option")]
    UnsupportedModifier(String),
    #[error("unsupported key token '{0}'")]
    UnsupportedKey(String),
    #[error("invalid key binding format: {0}")]
    InvalidFormat(String),
}

/// Enhanced keyboard shortcut configuration with matcher support
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyBinding {
    pub key: String,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub super_key: bool, // Cmd on Mac, Windows key on Windows
}

impl KeyBinding {
    /// Parse a key binding from string format like "C-a", "Home", "Cmd+Left", etc.
    /// Supports both Emacs-style (C-a) and GUI-style (Ctrl+A) notation
    pub fn from_string(s: &str) -> Result<Self, KeyboardShortcutError> {
        let mut ctrl = false;
        let mut alt = false;
        let mut shift = false;
        let mut super_key = false;

        // Parse modifiers - support multiple formats for better UX
        let lower = s.to_lowercase();
        ctrl |= lower.contains("c-") || lower.contains("ctrl+") || lower.contains("control+");
        alt |= lower.contains("m-")
            || lower.contains("alt+")
            || lower.contains("option+")
            || lower.contains("opt+");
        super_key |= lower.contains("cmd+")
            || lower.contains("super+")
            || lower.contains("meta+")
            || lower.contains("win+");
        shift |= lower.contains("shift+") || lower.contains("s-");

        // Extract the key part (everything after the last + or -)
        let key = if let Some(last_plus) = s.rfind('+') {
            s[last_plus + 1..].to_string()
        } else if let Some(last_dash) = s.rfind('-') {
            s[last_dash + 1..].to_string()
        } else {
            s.to_string()
        };

        if key.is_empty() {
            return Err(KeyboardShortcutError::MissingKey);
        }

        Ok(KeyBinding {
            key,
            ctrl,
            alt,
            shift,
            super_key,
        })
    }

    /// Convert back to string representation
    pub fn to_string(&self) -> String {
        let mut parts = Vec::new();

        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.alt {
            parts.push("Alt");
        }
        if self.shift {
            parts.push("Shift");
        }
        if self.super_key {
            parts.push("Cmd");
        }

        if !parts.is_empty() {
            format!("{}+{}", parts.join("+"), self.key)
        } else {
            self.key.clone()
        }
    }

    /// Convert to a KeyMatcher for advanced matching
    pub fn to_matcher(&self) -> Result<KeyMatcher, KeyboardShortcutError> {
        use ratatui::crossterm::event::{KeyCode, KeyModifiers};

        let (code, char_lower) = self.parse_key_token()?;
        let mut required = KeyModifiers::empty();
        let mut optional = KeyModifiers::empty();

        // Set required modifiers
        if self.ctrl {
            required |= KeyModifiers::CONTROL;
        }
        if self.alt {
            required |= KeyModifiers::ALT;
        }
        if self.shift {
            required |= KeyModifiers::SHIFT;
        }
        if self.super_key {
            required |= KeyModifiers::SUPER;
        }

        // For character keys, shift is optional (allows case-insensitive matching)
        if matches!(code, KeyCode::Char(_)) && !self.shift {
            optional |= KeyModifiers::SHIFT;
        }

        Ok(KeyMatcher::new(code, required, optional, char_lower))
    }

    /// Parse the key token into KeyCode and optional lowercase character
    fn parse_key_token(
        &self,
    ) -> Result<(ratatui::crossterm::event::KeyCode, Option<char>), KeyboardShortcutError> {
        use crossterm::event::KeyCode;

        let token = &self.key;
        let lower = token.to_lowercase();

        let (code, char_lower) = match lower.as_str() {
            "enter" | "return" => (KeyCode::Enter, None),
            "tab" => (KeyCode::Tab, None),
            "esc" | "escape" => (KeyCode::Esc, None),
            "space" => (KeyCode::Char(' '), None),
            "backspace" => (KeyCode::Backspace, None),
            "delete" | "del" => (KeyCode::Delete, None),
            "up" => (KeyCode::Up, None),
            "down" => (KeyCode::Down, None),
            "left" => (KeyCode::Left, None),
            "right" => (KeyCode::Right, None),
            "home" => (KeyCode::Home, None),
            "end" => (KeyCode::End, None),
            "pageup" | "page-up" | "pgup" => (KeyCode::PageUp, None),
            "pagedown" | "page-down" | "pgdown" => (KeyCode::PageDown, None),
            _ => {
                // Single character
                let mut chars = token.chars();
                let first = chars
                    .next()
                    .ok_or_else(|| KeyboardShortcutError::UnsupportedKey(token.clone()))?;
                if chars.next().is_some() {
                    return Err(KeyboardShortcutError::UnsupportedKey(token.clone()));
                }

                let lower_char = if first.is_ascii_alphabetic() {
                    Some(first.to_ascii_lowercase())
                } else {
                    None
                };

                (KeyCode::Char(first), lower_char)
            }
        };

        Ok((code, char_lower))
    }

    /// Check if this key binding matches a crossterm KeyEvent
    pub fn matches(&self, event: &ratatui::crossterm::event::KeyEvent) -> bool {
        match self.to_matcher() {
            Ok(matcher) => matcher.matches(event),
            Err(_) => false,
        }
    }
}

impl KeymapConfig {
    /// Get the default operation definitions with platform-specific bindings
    pub fn get_operation_definitions() -> Vec<KeyboardOperationDefinition> {
        vec![
            // Cursor Movement
            KeyboardOperationDefinition::new(
                KeyboardOperation::MoveToBeginningOfLine,
                vec![
                    "Home".to_string(),
                    "Ctrl+A".to_string(),
                    "Cmd+Left".to_string(),
                ],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::MoveToEndOfLine,
                vec![
                    "End".to_string(),
                    "Ctrl+E".to_string(),
                    "Cmd+Right".to_string(),
                ],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::MoveForwardOneCharacter,
                vec!["Ctrl+F".to_string(), "Right".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::MoveBackwardOneCharacter,
                vec!["Left".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::MoveToNextLine,
                vec!["Down".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::MoveToNextField,
                vec!["Tab".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::MoveToPreviousField,
                vec!["Shift+Tab".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::DismissOverlay,
                vec!["Esc".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::MoveToPreviousLine,
                vec!["Up".to_string(), "Ctrl+P".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::MoveForwardOneWord,
                vec![
                    "Alt+F".to_string(),
                    "Ctrl+Right".to_string(),
                    "Option+Right".to_string(),
                ],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::MoveBackwardOneWord,
                vec![
                    "Alt+B".to_string(),
                    "Ctrl+Left".to_string(),
                    "Option+Left".to_string(),
                ],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::ScrollDownOneScreen,
                vec!["PageDown".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::ScrollUpOneScreen,
                vec!["PageUp".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::RecenterScreenOnCursor,
                vec!["Ctrl+L".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::MoveToBeginningOfDocument,
                vec!["Ctrl+Home".to_string(), "Cmd+Up".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::MoveToEndOfDocument,
                vec!["Ctrl+End".to_string(), "Cmd+Down".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::GoToLineNumber,
                vec![
                    "Ctrl+G".to_string(),
                    "Cmd+L".to_string(),
                    "Alt+G+G".to_string(),
                ],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::MoveToMatchingParenthesis,
                vec!["Ctrl+Alt+F".to_string(), "Ctrl+Option+F".to_string()],
            ),
            // Editing and Deletion
            KeyboardOperationDefinition::new(
                KeyboardOperation::DeleteCharacterForward,
                vec!["Delete".to_string(), "Ctrl+D".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::DeleteCharacterBackward,
                vec!["Backspace".to_string(), "Ctrl+H".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::DeleteWordForward,
                vec![
                    "Ctrl+Delete".to_string(),
                    "Alt+D".to_string(),
                    "Option+Delete".to_string(),
                ],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::DeleteWordBackward,
                vec![
                    "Ctrl+Backspace".to_string(),
                    "Alt+Backspace".to_string(),
                    "Option+Backspace".to_string(),
                ],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::DeleteToEndOfLine,
                vec!["Ctrl+K".to_string()], // Emacs-style delete to end of line
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::Cut,
                vec![
                    "Ctrl+X".to_string(),
                    "Ctrl+W".to_string(),
                    "Cmd+X".to_string(),
                ],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::Copy,
                vec![
                    "Ctrl+C".to_string(),
                    "Alt+W".to_string(),
                    "Cmd+C".to_string(),
                ],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::Paste,
                vec![
                    "Ctrl+V".to_string(),
                    "Ctrl+Y".to_string(),
                    "Cmd+V".to_string(),
                ],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::Undo,
                vec![
                    "Ctrl+Z".to_string(),
                    "Ctrl+_".to_string(),
                    "Ctrl+/".to_string(),
                    "Cmd+Z".to_string(),
                ],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::Redo,
                vec![
                    "Ctrl+Y".to_string(),
                    "Ctrl+Shift+Z".to_string(),
                    "Cmd+Shift+Z".to_string(),
                ],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::OpenNewLine,
                vec!["Shift+Enter".to_string(), "Ctrl+J".to_string()],
            ),
            // Code Editing
            KeyboardOperationDefinition::new(
                KeyboardOperation::ToggleComment,
                vec![
                    "Ctrl+/".to_string(),
                    "Alt+;".to_string(),
                    "Cmd+/".to_string(),
                ],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::DuplicateLineSelection,
                vec!["Ctrl+D".to_string(), "Cmd+Shift+D".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::MoveLineUp,
                vec!["Alt+Up".to_string(), "Option+Up".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::MoveLineDown,
                vec!["Alt+Down".to_string(), "Option+Down".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::IndentRegion,
                vec!["Ctrl+]".to_string(), "Cmd+]".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::DedentRegion,
                vec!["Ctrl+[".to_string(), "Cmd+[".to_string()],
            ),
            // Search and Replace
            KeyboardOperationDefinition::new(
                KeyboardOperation::IncrementalSearchForward,
                vec![
                    "Ctrl+F".to_string(),
                    "Ctrl+S".to_string(),
                    "Cmd+F".to_string(),
                ],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::FindAndReplace,
                vec!["Ctrl+H".to_string(), "Cmd+Shift+H".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::FindNext,
                vec!["F3".to_string(), "Cmd+G".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::FindPrevious,
                vec!["Shift+F3".to_string(), "Cmd+Shift+G".to_string()],
            ),
            // Mark and Region
            KeyboardOperationDefinition::new(
                KeyboardOperation::SetMark,
                vec!["Ctrl+Space".to_string(), "Ctrl+@".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::SelectAll,
                vec!["Ctrl+A".to_string(), "Cmd+A".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::SelectWordUnderCursor,
                vec!["Alt+@".to_string()],
            ),
            // Additional operations from research document
            KeyboardOperationDefinition::new(
                KeyboardOperation::CycleThroughClipboard,
                vec!["Alt+Y".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::TransposeCharacters,
                vec!["Ctrl+T".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::TransposeWords,
                vec!["Alt+T".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::UppercaseWord,
                vec!["Alt+U".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::LowercaseWord,
                vec!["Alt+L".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::CapitalizeWord,
                vec!["Alt+C".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::JoinLines,
                vec!["Alt+^".to_string()],
            ),
            KeyboardOperationDefinition::new(KeyboardOperation::Bold, vec!["Ctrl+B".to_string()]),
            KeyboardOperationDefinition::new(KeyboardOperation::Italic, vec!["Ctrl+I".to_string()]),
            KeyboardOperationDefinition::new(
                KeyboardOperation::Underline,
                vec!["Ctrl+U".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::ToggleComment,
                vec![
                    "Ctrl+/".to_string(),
                    "Alt+;".to_string(),
                    "Cmd+/".to_string(),
                ],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::DuplicateLineSelection,
                vec!["Ctrl+D".to_string(), "Cmd+Shift+D".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::MoveLineUp,
                vec!["Alt+Up".to_string(), "Option+Up".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::MoveLineDown,
                vec!["Alt+Down".to_string(), "Option+Down".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::IndentRegion,
                vec!["Ctrl+]".to_string(), "Cmd+]".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::DedentRegion,
                vec!["Ctrl+[".to_string(), "Cmd+[".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::IncrementalSearchForward,
                vec!["Ctrl+S".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::IncrementalSearchBackward,
                vec!["Ctrl+R".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::FindNext,
                vec!["F3".to_string(), "Cmd+G".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::FindPrevious,
                vec!["Shift+F3".to_string(), "Cmd+Shift+G".to_string()],
            ),
            // Application Actions
            KeyboardOperationDefinition::new(
                KeyboardOperation::NewDraft,
                vec!["Ctrl+N".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::CreateAndFocus,
                vec!["Alt+Enter".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::CreateInSplitView,
                vec!["Ctrl+Enter".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::CreateInSplitViewAndFocus,
                vec!["Ctrl+Alt+Enter".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::CreateInHorizontalSplit,
                vec!["Ctrl+Shift+Enter".to_string()],
            ),
            KeyboardOperationDefinition::new(
                KeyboardOperation::CreateInVerticalSplit,
                vec!["Ctrl+Shift+Alt+Enter".to_string()],
            ),
        ]
    }

    /// Check if any key binding for the given operation matches the KeyEvent
    pub fn matches(
        &self,
        operation: KeyboardOperation,
        event: &crossterm::event::KeyEvent,
    ) -> bool {
        let bindings = match operation {
            KeyboardOperation::MoveToBeginningOfLine => &self.move_to_beginning_of_line,
            KeyboardOperation::MoveToEndOfLine => &self.move_to_end_of_line,
            KeyboardOperation::MoveForwardOneCharacter => &self.move_forward_one_character,
            KeyboardOperation::MoveBackwardOneCharacter => &self.move_backward_one_character,
            KeyboardOperation::MoveToNextLine => &self.move_to_next_line,
            KeyboardOperation::MoveToNextField => &self.move_to_next_field,
            KeyboardOperation::MoveToPreviousField => &self.move_to_previous_field,
            KeyboardOperation::DismissOverlay => &self.dismiss_overlay,
            KeyboardOperation::SelectWordUnderCursor => &self.select_word_under_cursor,
            KeyboardOperation::NewDraft => &self.new_draft,
            KeyboardOperation::CreateAndFocus => &self.create_and_focus,
            KeyboardOperation::CreateInSplitView => &self.create_in_split_view,
            KeyboardOperation::CreateInSplitViewAndFocus => &self.create_in_split_view_and_focus,
            KeyboardOperation::CreateInHorizontalSplit => &self.create_in_horizontal_split,
            KeyboardOperation::CreateInVerticalSplit => &self.create_in_vertical_split,
            KeyboardOperation::MoveToPreviousLine => &self.move_to_previous_line,
            KeyboardOperation::MoveForwardOneWord => &self.move_forward_one_word,
            KeyboardOperation::MoveBackwardOneWord => &self.move_backward_one_word,
            KeyboardOperation::MoveToBeginningOfSentence => &self.move_to_beginning_of_sentence,
            KeyboardOperation::MoveToEndOfSentence => &self.move_to_end_of_sentence,
            KeyboardOperation::ScrollDownOneScreen => &self.scroll_down_one_screen,
            KeyboardOperation::ScrollUpOneScreen => &self.scroll_up_one_screen,
            KeyboardOperation::RecenterScreenOnCursor => &self.recenter_screen_on_cursor,
            KeyboardOperation::MoveToBeginningOfDocument => &self.move_to_beginning_of_document,
            KeyboardOperation::MoveToEndOfDocument => &self.move_to_end_of_document,
            KeyboardOperation::MoveToBeginningOfParagraph => &self.move_to_beginning_of_paragraph,
            KeyboardOperation::MoveToEndOfParagraph => &self.move_to_end_of_paragraph,
            KeyboardOperation::GoToLineNumber => &self.go_to_line_number,
            KeyboardOperation::MoveToMatchingParenthesis => &self.move_to_matching_parenthesis,
            KeyboardOperation::DeleteCharacterForward => &self.delete_character_forward,
            KeyboardOperation::DeleteCharacterBackward => &self.delete_character_backward,
            KeyboardOperation::DeleteWordForward => &self.delete_word_forward,
            KeyboardOperation::DeleteWordBackward => &self.delete_word_backward,
            KeyboardOperation::DeleteToEndOfLine => &self.delete_to_end_of_line,
            KeyboardOperation::Cut => &self.cut,
            KeyboardOperation::Copy => &self.copy,
            KeyboardOperation::Paste => &self.paste,
            KeyboardOperation::CycleThroughClipboard => &self.cycle_through_clipboard,
            KeyboardOperation::TransposeCharacters => &self.transpose_characters,
            KeyboardOperation::TransposeWords => &self.transpose_words,
            KeyboardOperation::Undo => &self.undo,
            KeyboardOperation::Redo => &self.redo,
            KeyboardOperation::OpenNewLine => &self.open_new_line,
            KeyboardOperation::IndentOrComplete => &self.indent_or_complete,
            KeyboardOperation::DeleteToBeginningOfLine => &self.delete_to_beginning_of_line,
            KeyboardOperation::UppercaseWord => &self.uppercase_word,
            KeyboardOperation::LowercaseWord => &self.lowercase_word,
            KeyboardOperation::CapitalizeWord => &self.capitalize_word,
            KeyboardOperation::JustifyParagraph => &self.justify_paragraph,
            KeyboardOperation::JoinLines => &self.join_lines,
            KeyboardOperation::Bold => &self.bold,
            KeyboardOperation::Italic => &self.italic,
            KeyboardOperation::Underline => &self.underline,
            KeyboardOperation::ToggleComment => &self.toggle_comment,
            KeyboardOperation::DuplicateLineSelection => &self.duplicate_line_selection,
            KeyboardOperation::MoveLineUp => &self.move_line_up,
            KeyboardOperation::MoveLineDown => &self.move_line_down,
            KeyboardOperation::IndentRegion => &self.indent_region,
            KeyboardOperation::DedentRegion => &self.dedent_region,
            KeyboardOperation::IncrementalSearchForward => &self.incremental_search_forward,
            KeyboardOperation::IncrementalSearchBackward => &self.incremental_search_backward,
            KeyboardOperation::FindAndReplace => &self.find_and_replace,
            KeyboardOperation::FindAndReplaceWithRegex => &self.find_and_replace_with_regex,
            KeyboardOperation::FindNext => &self.find_next,
            KeyboardOperation::FindPrevious => &self.find_previous,
            KeyboardOperation::SetMark => &self.set_mark,
            KeyboardOperation::SelectAll => &self.select_all,
        };

        if let Some(bindings) = bindings {
            bindings.iter().any(|binding| binding.matches(event))
        } else {
            false
        }
    }

    /// Get all key bindings for a specific operation as display strings
    pub fn get_bindings_display(&self, operation: KeyboardOperation) -> Vec<String> {
        let shortcut = KeyboardShortcut::new(operation, self.get_matchers(operation));
        shortcut.display_strings()
    }

    /// Get matchers for a specific operation
    fn get_matchers(&self, operation: KeyboardOperation) -> Vec<KeyMatcher> {
        match operation {
            KeyboardOperation::MoveToBeginningOfLine => {
                self.move_to_beginning_of_line.clone().unwrap_or_default()
            }
            KeyboardOperation::MoveToEndOfLine => {
                self.move_to_end_of_line.clone().unwrap_or_default()
            }
            KeyboardOperation::MoveForwardOneCharacter => {
                self.move_forward_one_character.clone().unwrap_or_default()
            }
            KeyboardOperation::MoveBackwardOneCharacter => {
                self.move_backward_one_character.clone().unwrap_or_default()
            }
            KeyboardOperation::MoveToNextLine => self.move_to_next_line.clone().unwrap_or_default(),
            KeyboardOperation::MoveToNextField => {
                self.move_to_next_field.clone().unwrap_or_default()
            }
            KeyboardOperation::MoveToPreviousField => {
                self.move_to_previous_field.clone().unwrap_or_default()
            }
            KeyboardOperation::DismissOverlay => self.dismiss_overlay.clone().unwrap_or_default(),
            KeyboardOperation::SelectWordUnderCursor => {
                self.select_word_under_cursor.clone().unwrap_or_default()
            }
            KeyboardOperation::NewDraft => self.new_draft.clone().unwrap_or_default(),
            KeyboardOperation::CreateAndFocus => self.create_and_focus.clone().unwrap_or_default(),
            KeyboardOperation::CreateInSplitView => {
                self.create_in_split_view.clone().unwrap_or_default()
            }
            KeyboardOperation::CreateInSplitViewAndFocus => {
                self.create_in_split_view_and_focus.clone().unwrap_or_default()
            }
            KeyboardOperation::CreateInHorizontalSplit => {
                self.create_in_horizontal_split.clone().unwrap_or_default()
            }
            KeyboardOperation::CreateInVerticalSplit => {
                self.create_in_vertical_split.clone().unwrap_or_default()
            }
            KeyboardOperation::MoveToPreviousLine => {
                self.move_to_previous_line.clone().unwrap_or_default()
            }
            KeyboardOperation::MoveForwardOneWord => {
                self.move_forward_one_word.clone().unwrap_or_default()
            }
            KeyboardOperation::MoveBackwardOneWord => {
                self.move_backward_one_word.clone().unwrap_or_default()
            }
            KeyboardOperation::MoveToBeginningOfSentence => {
                self.move_to_beginning_of_sentence.clone().unwrap_or_default()
            }
            KeyboardOperation::MoveToEndOfSentence => {
                self.move_to_end_of_sentence.clone().unwrap_or_default()
            }
            KeyboardOperation::ScrollDownOneScreen => {
                self.scroll_down_one_screen.clone().unwrap_or_default()
            }
            KeyboardOperation::ScrollUpOneScreen => {
                self.scroll_up_one_screen.clone().unwrap_or_default()
            }
            KeyboardOperation::RecenterScreenOnCursor => {
                self.recenter_screen_on_cursor.clone().unwrap_or_default()
            }
            KeyboardOperation::MoveToBeginningOfDocument => {
                self.move_to_beginning_of_document.clone().unwrap_or_default()
            }
            KeyboardOperation::MoveToEndOfDocument => {
                self.move_to_end_of_document.clone().unwrap_or_default()
            }
            KeyboardOperation::MoveToBeginningOfParagraph => {
                self.move_to_beginning_of_paragraph.clone().unwrap_or_default()
            }
            KeyboardOperation::MoveToEndOfParagraph => {
                self.move_to_end_of_paragraph.clone().unwrap_or_default()
            }
            KeyboardOperation::GoToLineNumber => self.go_to_line_number.clone().unwrap_or_default(),
            KeyboardOperation::MoveToMatchingParenthesis => {
                self.move_to_matching_parenthesis.clone().unwrap_or_default()
            }
            KeyboardOperation::DeleteCharacterForward => {
                self.delete_character_forward.clone().unwrap_or_default()
            }
            KeyboardOperation::DeleteCharacterBackward => {
                self.delete_character_backward.clone().unwrap_or_default()
            }
            KeyboardOperation::DeleteWordForward => {
                self.delete_word_forward.clone().unwrap_or_default()
            }
            KeyboardOperation::DeleteWordBackward => {
                self.delete_word_backward.clone().unwrap_or_default()
            }
            KeyboardOperation::DeleteToEndOfLine => {
                self.delete_to_end_of_line.clone().unwrap_or_default()
            }
            KeyboardOperation::Cut => self.cut.clone().unwrap_or_default(),
            KeyboardOperation::Copy => self.copy.clone().unwrap_or_default(),
            KeyboardOperation::Paste => self.paste.clone().unwrap_or_default(),
            KeyboardOperation::CycleThroughClipboard => {
                self.cycle_through_clipboard.clone().unwrap_or_default()
            }
            KeyboardOperation::TransposeCharacters => {
                self.transpose_characters.clone().unwrap_or_default()
            }
            KeyboardOperation::TransposeWords => self.transpose_words.clone().unwrap_or_default(),
            KeyboardOperation::Undo => self.undo.clone().unwrap_or_default(),
            KeyboardOperation::Redo => self.redo.clone().unwrap_or_default(),
            KeyboardOperation::OpenNewLine => self.open_new_line.clone().unwrap_or_default(),
            KeyboardOperation::IndentOrComplete => {
                self.indent_or_complete.clone().unwrap_or_default()
            }
            KeyboardOperation::DeleteToBeginningOfLine => {
                self.delete_to_beginning_of_line.clone().unwrap_or_default()
            }
            KeyboardOperation::UppercaseWord => self.uppercase_word.clone().unwrap_or_default(),
            KeyboardOperation::LowercaseWord => self.lowercase_word.clone().unwrap_or_default(),
            KeyboardOperation::CapitalizeWord => self.capitalize_word.clone().unwrap_or_default(),
            KeyboardOperation::JustifyParagraph => {
                self.justify_paragraph.clone().unwrap_or_default()
            }
            KeyboardOperation::JoinLines => self.join_lines.clone().unwrap_or_default(),
            KeyboardOperation::Bold => self.bold.clone().unwrap_or_default(),
            KeyboardOperation::Italic => self.italic.clone().unwrap_or_default(),
            KeyboardOperation::Underline => self.underline.clone().unwrap_or_default(),
            KeyboardOperation::ToggleComment => self.toggle_comment.clone().unwrap_or_default(),
            KeyboardOperation::DuplicateLineSelection => {
                self.duplicate_line_selection.clone().unwrap_or_default()
            }
            KeyboardOperation::MoveLineUp => self.move_line_up.clone().unwrap_or_default(),
            KeyboardOperation::MoveLineDown => self.move_line_down.clone().unwrap_or_default(),
            KeyboardOperation::IndentRegion => self.indent_region.clone().unwrap_or_default(),
            KeyboardOperation::DedentRegion => self.dedent_region.clone().unwrap_or_default(),
            KeyboardOperation::IncrementalSearchForward => {
                self.incremental_search_forward.clone().unwrap_or_default()
            }
            KeyboardOperation::IncrementalSearchBackward => {
                self.incremental_search_backward.clone().unwrap_or_default()
            }
            KeyboardOperation::FindAndReplace => self.find_and_replace.clone().unwrap_or_default(),
            KeyboardOperation::FindAndReplaceWithRegex => {
                self.find_and_replace_with_regex.clone().unwrap_or_default()
            }
            KeyboardOperation::FindNext => self.find_next.clone().unwrap_or_default(),
            KeyboardOperation::FindPrevious => self.find_previous.clone().unwrap_or_default(),
            KeyboardOperation::SetMark => self.set_mark.clone().unwrap_or_default(),
            KeyboardOperation::SelectAll => self.select_all.clone().unwrap_or_default(),
        }
    }
}

/// Default key bindings as specified in TUI-PRD.md
impl Default for KeyBinding {
    fn default() -> Self {
        KeyBinding {
            key: "".to_string(),
            ctrl: false,
            alt: false,
            shift: false,
            super_key: false,
        }
    }
}

/// Default implementation for KeymapConfig
impl Default for KeymapConfig {
    fn default() -> Self {
        let platform = Platform::current();
        let definitions = Self::get_operation_definitions();

        let mut config = KeymapConfig {
            meta_key: None,
            // Initialize all fields to None - they will be populated below
            move_to_beginning_of_line: None,
            move_to_end_of_line: None,
            move_forward_one_character: None,
            move_backward_one_character: None,
            move_to_next_line: None,
            move_to_next_field: None,
            move_to_previous_field: None,
            dismiss_overlay: None,
            select_word_under_cursor: None,
            move_to_previous_line: None,
            move_forward_one_word: None,
            move_backward_one_word: None,
            move_to_beginning_of_sentence: None,
            move_to_end_of_sentence: None,
            scroll_down_one_screen: None,
            scroll_up_one_screen: None,
            recenter_screen_on_cursor: None,
            move_to_beginning_of_document: None,
            move_to_end_of_document: None,
            move_to_beginning_of_paragraph: None,
            move_to_end_of_paragraph: None,
            go_to_line_number: None,
            move_to_matching_parenthesis: None,
            delete_character_forward: None,
            delete_character_backward: None,
            delete_word_forward: None,
            delete_word_backward: None,
            delete_to_end_of_line: None,
            cut: None,
            copy: None,
            paste: None,
            cycle_through_clipboard: None,
            transpose_characters: None,
            transpose_words: None,
            undo: None,
            redo: None,
            open_new_line: None,
            indent_or_complete: None,
            delete_to_beginning_of_line: None,
            uppercase_word: None,
            lowercase_word: None,
            capitalize_word: None,
            justify_paragraph: None,
            join_lines: None,
            bold: None,
            italic: None,
            underline: None,
            toggle_comment: None,
            duplicate_line_selection: None,
            move_line_up: None,
            move_line_down: None,
            indent_region: None,
            dedent_region: None,
            incremental_search_forward: None,
            incremental_search_backward: None,
            find_and_replace: None,
            find_and_replace_with_regex: None,
            find_next: None,
            find_previous: None,
            set_mark: None,
            select_all: None,
            new_draft: None,
            create_and_focus: None,
            create_in_split_view: None,
            create_in_split_view_and_focus: None,
            create_in_horizontal_split: None,
            create_in_vertical_split: None,
        };

        // Populate the config with parsed default bindings
        for def in definitions {
            let defaults = def.get_defaults(platform);
            let mut matchers = Vec::new();

            for binding_str in defaults {
                if let Ok(key_binding) = KeyBinding::from_string(binding_str) {
                    if let Ok(matcher) = key_binding.to_matcher() {
                        matchers.push(matcher);
                    }
                }
            }

            if !matchers.is_empty() {
                match def.operation {
                    KeyboardOperation::MoveToBeginningOfLine => {
                        config.move_to_beginning_of_line = Some(matchers)
                    }
                    KeyboardOperation::MoveToEndOfLine => {
                        config.move_to_end_of_line = Some(matchers)
                    }
                    KeyboardOperation::MoveForwardOneCharacter => {
                        config.move_forward_one_character = Some(matchers)
                    }
                    KeyboardOperation::MoveBackwardOneCharacter => {
                        config.move_backward_one_character = Some(matchers)
                    }
                    KeyboardOperation::MoveToNextLine => config.move_to_next_line = Some(matchers),
                    KeyboardOperation::MoveToNextField => {
                        config.move_to_next_field = Some(matchers)
                    }
                    KeyboardOperation::MoveToPreviousField => {
                        config.move_to_previous_field = Some(matchers)
                    }
                    KeyboardOperation::DismissOverlay => config.dismiss_overlay = Some(matchers),
                    KeyboardOperation::SelectWordUnderCursor => {
                        config.select_word_under_cursor = Some(matchers)
                    }
                    KeyboardOperation::NewDraft => config.new_draft = Some(matchers),
                    KeyboardOperation::CreateAndFocus => config.create_and_focus = Some(matchers),
                    KeyboardOperation::CreateInSplitView => {
                        config.create_in_split_view = Some(matchers)
                    }
                    KeyboardOperation::CreateInSplitViewAndFocus => {
                        config.create_in_split_view_and_focus = Some(matchers)
                    }
                    KeyboardOperation::CreateInHorizontalSplit => {
                        config.create_in_horizontal_split = Some(matchers)
                    }
                    KeyboardOperation::CreateInVerticalSplit => {
                        config.create_in_vertical_split = Some(matchers)
                    }
                    KeyboardOperation::MoveToPreviousLine => {
                        config.move_to_previous_line = Some(matchers)
                    }
                    KeyboardOperation::MoveForwardOneWord => {
                        config.move_forward_one_word = Some(matchers)
                    }
                    KeyboardOperation::MoveBackwardOneWord => {
                        config.move_backward_one_word = Some(matchers)
                    }
                    KeyboardOperation::MoveToBeginningOfSentence => {
                        config.move_to_beginning_of_sentence = Some(matchers)
                    }
                    KeyboardOperation::MoveToEndOfSentence => {
                        config.move_to_end_of_sentence = Some(matchers)
                    }
                    KeyboardOperation::ScrollDownOneScreen => {
                        config.scroll_down_one_screen = Some(matchers)
                    }
                    KeyboardOperation::ScrollUpOneScreen => {
                        config.scroll_up_one_screen = Some(matchers)
                    }
                    KeyboardOperation::RecenterScreenOnCursor => {
                        config.recenter_screen_on_cursor = Some(matchers)
                    }
                    KeyboardOperation::MoveToBeginningOfDocument => {
                        config.move_to_beginning_of_document = Some(matchers)
                    }
                    KeyboardOperation::MoveToEndOfDocument => {
                        config.move_to_end_of_document = Some(matchers)
                    }
                    KeyboardOperation::MoveToBeginningOfParagraph => {
                        config.move_to_beginning_of_paragraph = Some(matchers)
                    }
                    KeyboardOperation::MoveToEndOfParagraph => {
                        config.move_to_end_of_paragraph = Some(matchers)
                    }
                    KeyboardOperation::GoToLineNumber => config.go_to_line_number = Some(matchers),
                    KeyboardOperation::MoveToMatchingParenthesis => {
                        config.move_to_matching_parenthesis = Some(matchers)
                    }
                    KeyboardOperation::DeleteCharacterForward => {
                        config.delete_character_forward = Some(matchers)
                    }
                    KeyboardOperation::DeleteCharacterBackward => {
                        config.delete_character_backward = Some(matchers)
                    }
                    KeyboardOperation::DeleteWordForward => {
                        config.delete_word_forward = Some(matchers)
                    }
                    KeyboardOperation::DeleteWordBackward => {
                        config.delete_word_backward = Some(matchers)
                    }
                    KeyboardOperation::DeleteToEndOfLine => {
                        config.delete_to_end_of_line = Some(matchers)
                    }
                    KeyboardOperation::Cut => config.cut = Some(matchers),
                    KeyboardOperation::Copy => config.copy = Some(matchers),
                    KeyboardOperation::Paste => config.paste = Some(matchers),
                    KeyboardOperation::CycleThroughClipboard => {
                        config.cycle_through_clipboard = Some(matchers)
                    }
                    KeyboardOperation::TransposeCharacters => {
                        config.transpose_characters = Some(matchers)
                    }
                    KeyboardOperation::TransposeWords => config.transpose_words = Some(matchers),
                    KeyboardOperation::Undo => config.undo = Some(matchers),
                    KeyboardOperation::Redo => config.redo = Some(matchers),
                    KeyboardOperation::OpenNewLine => config.open_new_line = Some(matchers),
                    KeyboardOperation::IndentOrComplete => {
                        config.indent_or_complete = Some(matchers)
                    }
                    KeyboardOperation::DeleteToBeginningOfLine => {
                        config.delete_to_beginning_of_line = Some(matchers)
                    }
                    KeyboardOperation::UppercaseWord => config.uppercase_word = Some(matchers),
                    KeyboardOperation::LowercaseWord => config.lowercase_word = Some(matchers),
                    KeyboardOperation::CapitalizeWord => config.capitalize_word = Some(matchers),
                    KeyboardOperation::JustifyParagraph => {
                        config.justify_paragraph = Some(matchers)
                    }
                    KeyboardOperation::JoinLines => config.join_lines = Some(matchers),
                    KeyboardOperation::Bold => config.bold = Some(matchers),
                    KeyboardOperation::Italic => config.italic = Some(matchers),
                    KeyboardOperation::Underline => config.underline = Some(matchers),
                    KeyboardOperation::ToggleComment => config.toggle_comment = Some(matchers),
                    KeyboardOperation::DuplicateLineSelection => {
                        config.duplicate_line_selection = Some(matchers)
                    }
                    KeyboardOperation::MoveLineUp => config.move_line_up = Some(matchers),
                    KeyboardOperation::MoveLineDown => config.move_line_down = Some(matchers),
                    KeyboardOperation::IndentRegion => config.indent_region = Some(matchers),
                    KeyboardOperation::DedentRegion => config.dedent_region = Some(matchers),
                    KeyboardOperation::IncrementalSearchForward => {
                        config.incremental_search_forward = Some(matchers)
                    }
                    KeyboardOperation::IncrementalSearchBackward => {
                        config.incremental_search_backward = Some(matchers)
                    }
                    KeyboardOperation::FindAndReplace => config.find_and_replace = Some(matchers),
                    KeyboardOperation::FindAndReplaceWithRegex => {
                        config.find_and_replace_with_regex = Some(matchers)
                    }
                    KeyboardOperation::FindNext => config.find_next = Some(matchers),
                    KeyboardOperation::FindPrevious => config.find_previous = Some(matchers),
                    KeyboardOperation::SetMark => config.set_mark = Some(matchers),
                    KeyboardOperation::SelectAll => config.select_all = Some(matchers),
                }
            }
        }

        config
    }
}

/// Keyboard shortcuts configuration section
#[derive(Debug, Clone, PartialEq)]
pub struct KeymapConfig {
    pub meta_key: Option<MetaKey>,

    // Cursor Movement
    pub move_to_beginning_of_line: Option<Vec<KeyMatcher>>,
    pub move_to_end_of_line: Option<Vec<KeyMatcher>>,
    pub move_forward_one_character: Option<Vec<KeyMatcher>>,
    pub move_backward_one_character: Option<Vec<KeyMatcher>>,
    pub move_to_next_line: Option<Vec<KeyMatcher>>,
    pub move_to_next_field: Option<Vec<KeyMatcher>>,
    pub move_to_previous_field: Option<Vec<KeyMatcher>>,
    pub dismiss_overlay: Option<Vec<KeyMatcher>>,
    pub select_word_under_cursor: Option<Vec<KeyMatcher>>,
    pub move_to_previous_line: Option<Vec<KeyMatcher>>,
    pub move_forward_one_word: Option<Vec<KeyMatcher>>,
    pub move_backward_one_word: Option<Vec<KeyMatcher>>,
    pub move_to_beginning_of_sentence: Option<Vec<KeyMatcher>>,
    pub move_to_end_of_sentence: Option<Vec<KeyMatcher>>,
    pub scroll_down_one_screen: Option<Vec<KeyMatcher>>,
    pub scroll_up_one_screen: Option<Vec<KeyMatcher>>,
    pub recenter_screen_on_cursor: Option<Vec<KeyMatcher>>,
    pub move_to_beginning_of_document: Option<Vec<KeyMatcher>>,
    pub move_to_end_of_document: Option<Vec<KeyMatcher>>,
    pub move_to_beginning_of_paragraph: Option<Vec<KeyMatcher>>,
    pub move_to_end_of_paragraph: Option<Vec<KeyMatcher>>,
    pub go_to_line_number: Option<Vec<KeyMatcher>>,
    pub move_to_matching_parenthesis: Option<Vec<KeyMatcher>>,

    // Editing and Deletion
    pub delete_character_forward: Option<Vec<KeyMatcher>>,
    pub delete_character_backward: Option<Vec<KeyMatcher>>,
    pub delete_word_forward: Option<Vec<KeyMatcher>>,
    pub delete_word_backward: Option<Vec<KeyMatcher>>,
    pub delete_to_end_of_line: Option<Vec<KeyMatcher>>,
    pub cut: Option<Vec<KeyMatcher>>,
    pub copy: Option<Vec<KeyMatcher>>,
    pub paste: Option<Vec<KeyMatcher>>,
    pub cycle_through_clipboard: Option<Vec<KeyMatcher>>,
    pub transpose_characters: Option<Vec<KeyMatcher>>,
    pub transpose_words: Option<Vec<KeyMatcher>>,
    pub undo: Option<Vec<KeyMatcher>>,
    pub redo: Option<Vec<KeyMatcher>>,
    pub open_new_line: Option<Vec<KeyMatcher>>,
    pub indent_or_complete: Option<Vec<KeyMatcher>>,
    pub delete_to_beginning_of_line: Option<Vec<KeyMatcher>>,

    // Text Transformation
    pub uppercase_word: Option<Vec<KeyMatcher>>,
    pub lowercase_word: Option<Vec<KeyMatcher>>,
    pub capitalize_word: Option<Vec<KeyMatcher>>,
    pub justify_paragraph: Option<Vec<KeyMatcher>>,
    pub join_lines: Option<Vec<KeyMatcher>>,

    // Formatting (Markdown Style)
    pub bold: Option<Vec<KeyMatcher>>,
    pub italic: Option<Vec<KeyMatcher>>,
    pub underline: Option<Vec<KeyMatcher>>,

    // Code Editing
    pub toggle_comment: Option<Vec<KeyMatcher>>,
    pub duplicate_line_selection: Option<Vec<KeyMatcher>>,
    pub move_line_up: Option<Vec<KeyMatcher>>,
    pub move_line_down: Option<Vec<KeyMatcher>>,
    pub indent_region: Option<Vec<KeyMatcher>>,
    pub dedent_region: Option<Vec<KeyMatcher>>,

    // Search and Replace
    pub incremental_search_forward: Option<Vec<KeyMatcher>>,
    pub incremental_search_backward: Option<Vec<KeyMatcher>>,
    pub find_and_replace: Option<Vec<KeyMatcher>>,
    pub find_and_replace_with_regex: Option<Vec<KeyMatcher>>,
    pub find_next: Option<Vec<KeyMatcher>>,
    pub find_previous: Option<Vec<KeyMatcher>>,

    // Mark and Region
    pub set_mark: Option<Vec<KeyMatcher>>,
    pub select_all: Option<Vec<KeyMatcher>>,

    // Application Actions
    pub new_draft: Option<Vec<KeyMatcher>>,
    pub create_and_focus: Option<Vec<KeyMatcher>>,
    pub create_in_split_view: Option<Vec<KeyMatcher>>,
    pub create_in_split_view_and_focus: Option<Vec<KeyMatcher>>,
    pub create_in_horizontal_split: Option<Vec<KeyMatcher>>,
    pub create_in_vertical_split: Option<Vec<KeyMatcher>>,
}

/// Main settings configuration structure
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// Number of activity rows to display for active tasks (default: 3)
    pub active_sessions_activity_rows: Option<usize>,

    /// Font style for symbols and icons
    pub font_style: Option<FontStyle>,

    /// Selection dialog style preference
    pub selection_dialog_style: Option<SelectionDialogStyle>,

    /// Keyboard shortcuts configuration
    pub keymap: Option<KeymapConfig>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            active_sessions_activity_rows: Some(3),
            font_style: Some(FontStyle::Unicode),
            selection_dialog_style: Some(SelectionDialogStyle::Default),
            keymap: Some(KeymapConfig::default()),
        }
    }
}

impl Settings {
    /// Get the number of activity rows, with default fallback
    pub fn activity_rows(&self) -> usize {
        self.active_sessions_activity_rows.unwrap_or(3)
    }

    /// Get the font style, with default fallback
    pub fn font_style(&self) -> FontStyle {
        self.font_style.clone().unwrap_or(FontStyle::Unicode)
    }

    /// Get the selection dialog style, with default fallback
    pub fn selection_dialog_style(&self) -> SelectionDialogStyle {
        self.selection_dialog_style.clone().unwrap_or(SelectionDialogStyle::Default)
    }

    /// Get the keymap configuration, with default fallback
    pub fn keymap(&self) -> KeymapConfig {
        self.keymap.clone().unwrap_or_else(KeymapConfig::default)
    }
}
