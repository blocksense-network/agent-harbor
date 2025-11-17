// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Input handling and keyboard operation mapping
//!
//! This module provides a flexible input system for managing keyboard shortcuts and input contexts
//! in the TUI. The system resolves key events to semantic operations based on the current UI context.
//!
//! ## Architecture
//!
//! The input system consists of two main components:
//!
//! 1. **Keyboard Operations**: Semantic operations (like `MoveToNextLine`, `DeleteCharacterForward`)
//!    that represent user intentions, defined in `settings.rs`
//!
//! 2. **Input Minor Modes**: Named collections of keyboard operations that define which operations
//!    are available in a particular UI context. These are used to resolve key events to operations
//!    based on user-configured shortcut mappings.
//!
//! **Important**: Different minor modes may translate the same KeyEvent to different KeyboardOperations.
//! For example, ESC might map to `DismissOverlay` in a modal context but to `CancelOperation` in an
//! editor context. This allows the same physical key to have different meanings in different UI contexts.
//!
//! ## Key Binding Resolution
//!
//! Key events are mapped to keyboard operations through the settings system, which supports:
//! - User-configurable key bindings
//! - Platform-specific defaults (PC vs Mac)
//! - Multiple key combinations per operation
//!
//! ## Usage Pattern
//!
//! ```rust,ignore
//! use crate::view_model::input::{minor_modes, InputResult, InputState};
//!
//! // In your view model, maintain input state and handle key events by checking input minor modes
//! // in priority order until one handles the key event
//! fn handle_key_event(&mut self, raw_key_event: KeyEvent, settings: &Settings) -> InputResult {
//!     // First update input state with the raw event
//!     self.input_state.update(&raw_key_event);
//!
//!     // Preprocess the key event to handle terminal translations (e.g., SHIFT+ENTER → CTRL+J)
//!     let key_event = self.input_state.preprocess_key_event(raw_key_event);
//!     // Check modal overlay first (highest priority)
//!     if self.modal_active {
//!         if let Some(operation) = minor_modes::SELECTION_MODE.resolve_key_to_operation(&key_event, settings) {
//!             match operation {
//!                 KeyboardOperation::DismissOverlay => {
//!                     self.close_modal();
//!                     return InputResult::Handled;
//!                 }
//!                 KeyboardOperation::SelectAll => {
//!                     self.select_all_in_modal();
//!                     return InputResult::Handled;
//!                 }
//!                 _ => {}
//!             }
//!         }
//!     }
//!
//!     // Check current focus context
//!     match self.current_focus {
//!         FocusState::TextArea => {
//!             // Try textarea-specific operations first
//!             if let Some(operation) = minor_modes::TEXT_EDITING_MODE.resolve_key_to_operation(&key_event, settings) {
//!                 match operation {
//!                     KeyboardOperation::MoveToNextLine => {
//!                         self.textarea.move_cursor_down();
//!                         return InputResult::Handled;
//!                     }
//!                     KeyboardOperation::DeleteCharacterForward => {
//!                         self.textarea.delete_forward();
//!                         return InputResult::Handled;
//!                     }
//!                     _ => {}
//!                 }
//!             }
//!             // Fall through to navigation operations
//!             if let Some(operation) = crate::view_model::session_viewer_model::SESSION_VIEWER_MODE.resolve_key_to_operation(&key_event, settings) {
//!                 match operation {
//!                     KeyboardOperation::MoveToNextLine => {
//!                         self.move_cursor_down();
//!                         return InputResult::Handled;
//!                     }
//!                     KeyboardOperation::PreviousSnapshot => {
//!                         self.navigate_to_previous_snapshot();
//!                         return InputResult::Handled;
//!                     }
//!                     _ => {}
//!                 }
//!             }
//!         }
//!         FocusState::ListView => {
//!             // Try list-specific operations first
//!             if let Some(operation) = resolve_key_to_operation(&key_event, &minor_modes::SELECTION_MODE, settings) {
//!                 match operation {
//!                     KeyboardOperation::SelectAll => {
//!                         self.select_all_items();
//!                         return InputResult::Handled;
//!                     }
//!                     _ => {}
//!                 }
//!             }
//!             // Fall through to navigation operations
//!             if let Some(operation) = crate::view_model::session_viewer_model::SESSION_VIEWER_MODE.resolve_key_to_operation(&key_event, settings) {
//!                 match operation {
//!                     KeyboardOperation::MoveToNextLine => {
//!                         self.select_next_item();
//!                         return InputResult::Handled;
//!                     }
//!                     _ => {}
//!                 }
//!             }
//!         }
//!     }
//!
//!     InputResult::NotHandled
//! }
//! ```
//!
//! ## Input Result Types
//!
//! Input handlers return an [`InputResult`] enum to control event processing:
//!
//! - **`Handled`**: The operation was fully handled, stop processing
//! - **`NotHandled`**: This context doesn't handle the operation, continue with default behavior
//!
//! ## Input Minor Mode Definition
//!
//! Input minor modes are defined as static `InputMinorMode` instances:
//!
//! ```rust,ignore
//! use crate::view_model::input::InputMinorMode;
//!
//! // Define operation sets for different UI contexts
//! static NAVIGATION_MODE: InputMinorMode = InputMinorMode::new(&[
//!     KeyboardOperation::MoveToNextLine,
//!     KeyboardOperation::MoveToPreviousLine,
//!     KeyboardOperation::MoveToNextField,
//!     KeyboardOperation::MoveToPreviousField,
//! ]);
//!
//! // For modes with prominent operations (displayed in status bar)
//! static TEXT_EDITING_MODE: InputMinorMode = InputMinorMode::with_prominent_operations(
//!     &[
//!         KeyboardOperation::MoveToBeginningOfLine,
//!         KeyboardOperation::MoveToEndOfLine,
//!         KeyboardOperation::DeleteCharacterForward,
//!         KeyboardOperation::DeleteCharacterBackward,
//!         // ... more operations
//!     ],
//!     &[
//!         KeyboardOperation::DeleteCharacterForward,
//!         KeyboardOperation::DeleteCharacterBackward,
//!         KeyboardOperation::Undo,
//!         // ... prominent operations
//!     ],
//! );
//! ```
//!
//! These static instances minimize memory allocations and keep operation definitions co-located
//! with the UI logic that uses them. Pre-defined minor modes are available in the `minor_modes` module.
//!
//! ## Special Case: Textarea Bubbling
//!
//! The textarea input handling has a special case where certain operations may be handled by
//! the parent context. This is implemented through the `handle_keyboard_operation` function's
//! return value, which can indicate operations that should be delegated upward.
//!
//! ## Input Preprocessing with InputState
//!
//! For advanced input handling, use [`InputState`] to track modifier key states and preprocess
//! key events before resolving them to operations. This is essential for handling terminal-specific
//! key translations where modifier information may be lost (e.g., SHIFT+ENTER → CTRL+J).
//!
//! The recommended pattern is:
//! 1. Update `InputState` with raw key events
//! 2. Preprocess events to correct terminal translations
//! 3. Resolve preprocessed events to keyboard operations
//! 4. Handle operations based on UI context
//!
//! This approach ensures robust input handling across different terminal environments.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::settings::KeyboardOperation;

/// Resolve a key event to a keyboard operation using the provided input minor mode and settings
///
/// This function checks if the given key event matches any keyboard shortcut configured
/// for the operations in the provided input minor mode. It's more efficient than the global
/// `key_event_to_operation` function because it only checks the operations you care about.
///
/// # Parameters
/// - `key_event`: The key event to resolve
/// - `minor_mode`: Input minor mode containing the operations to check for matches
/// - `settings`: Settings containing the keymap configuration
///
/// # Returns
/// `Some(operation)` if a matching operation was found, `None` otherwise

/// Result of handling a keyboard operation by an input context
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputResult {
    /// The operation was handled and processing should stop
    Handled,
    /// This context doesn't handle this operation, continue with default behavior
    NotHandled,
}

/// Tracks input state that affects key event interpretation
///
/// InputState maintains state about modifier keys and other input context
/// that can change how key events are interpreted. This is particularly useful
/// for handling terminal-specific key translations (like SHIFT+ENTER → CTRL+J).
///
/// # Example
///
/// ```rust,ignore
/// use crate::view_model::input::InputState;
///
/// let mut input_state = InputState::new();
///
/// // Update state with key events
/// input_state.update(&key_event);
///
/// // Preprocess key events before passing to input modes
/// let processed_event = input_state.preprocess_key_event(key_event);
///
/// // Now resolve the processed event to operations
/// if let Some(operation) = minor_mode.resolve_key_to_operation(&processed_event, settings) {
///     // Handle the operation...
/// }
/// ```
#[derive(Debug, Clone)]
pub struct InputState {
    /// Whether the SHIFT key is currently pressed
    pub shift_pressed: bool,
}

impl InputState {
    /// Create a new input state with default values
    pub fn new() -> Self {
        Self {
            shift_pressed: false,
        }
    }

    /// Update the input state based on a key event
    ///
    /// This tracks modifier key presses and releases to maintain accurate
    /// state for key event preprocessing.
    pub fn update(&mut self, key: &KeyEvent) {
        match key.code {
            KeyCode::Modifier(crossterm::event::ModifierKeyCode::LeftShift)
            | KeyCode::Modifier(crossterm::event::ModifierKeyCode::RightShift) => match key.kind {
                KeyEventKind::Press => {
                    self.shift_pressed = true;
                }
                KeyEventKind::Release => {
                    self.shift_pressed = false;
                }
                _ => {}
            },
            _ => {}
        }
    }

    /// Preprocess a key event to handle terminal-specific translations
    ///
    /// This method handles special cases where terminals translate certain
    /// key combinations in ways that lose modifier information. For example,
    /// some terminals translate SHIFT+ENTER to CTRL+J, losing the SHIFT modifier.
    /// This method detects such cases and restores the correct modifiers.
    ///
    /// # Parameters
    /// - `key`: The raw key event from the terminal
    ///
    /// # Returns
    /// A processed key event with corrected modifiers if applicable
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Terminal sends CTRL+J for SHIFT+ENTER
    /// let ctrl_j_event = KeyEvent {
    ///     code: KeyCode::Char('j'),
    ///     modifiers: KeyModifiers::CONTROL,
    ///     // ... other fields
    /// };
    ///
    /// // But shift is currently pressed
    /// input_state.shift_pressed = true;
    ///
    /// // Preprocessing restores the correct SHIFT+ENTER combination
    /// let processed = input_state.preprocess_key_event(ctrl_j_event);
    /// assert_eq!(processed.code, KeyCode::Enter);
    /// assert_eq!(processed.modifiers, KeyModifiers::SHIFT);
    /// ```
    pub fn preprocess_key_event(&self, key: KeyEvent) -> KeyEvent {
        // If we see CTRL+J and shift is currently pressed, treat it as SHIFT+ENTER
        if key.code == KeyCode::Char('j')
            && key.modifiers == KeyModifiers::CONTROL
            && self.shift_pressed
        {
            tracing::debug!("SHIFT+ENTER detected: transforming CTRL+J to SHIFT+ENTER");
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::SHIFT,
                kind: key.kind,
                state: key.state,
            }
        } else {
            key
        }
    }
}

impl Default for InputState {
    fn default() -> Self {
        Self::new()
    }
}

/// Common operation sets for building input minor modes
///
/// These constants define groups of keyboard operations that are commonly
/// used together when building input minor modes.
pub mod operations {
    use super::KeyboardOperation;

    /// Navigation operations (arrow keys, tab, etc.)
    pub const NAVIGATION: &[KeyboardOperation] = &[
        KeyboardOperation::MoveToNextLine,
        KeyboardOperation::MoveToPreviousLine,
        KeyboardOperation::MoveToNextField,
        KeyboardOperation::MoveToPreviousField,
        KeyboardOperation::MoveToPreviousSnapshot,
        KeyboardOperation::MoveToNextSnapshot,
    ];

    /// Selection operations (enter, space, etc.)
    pub const SELECTION: &[KeyboardOperation] = &[
        KeyboardOperation::DismissOverlay,
        KeyboardOperation::SelectAll,
        KeyboardOperation::DraftNewTask,
    ];

    /// Text editing operations (cursor movement, deletion, etc.)
    pub const TEXT_EDITING: &[KeyboardOperation] = &[
        KeyboardOperation::MoveToBeginningOfLine,
        KeyboardOperation::MoveToEndOfLine,
        KeyboardOperation::MoveForwardOneCharacter,
        KeyboardOperation::MoveBackwardOneCharacter,
        KeyboardOperation::MoveForwardOneWord,
        KeyboardOperation::MoveBackwardOneWord,
        KeyboardOperation::DeleteCharacterForward,
        KeyboardOperation::DeleteCharacterBackward,
        KeyboardOperation::DeleteWordForward,
        KeyboardOperation::DeleteWordBackward,
        KeyboardOperation::DeleteToEndOfLine,
        KeyboardOperation::DeleteToBeginningOfLine,
    ];
}

/// Common input minor modes for different UI contexts
///
/// These constants define InputMinorMode instances that are commonly
/// used in different UI contexts throughout the application.
pub mod minor_modes {
    use super::{InputMinorMode, KeyboardOperation, operations};

    /// Selection minor mode (enter, space, etc.)
    pub static SELECTION_MODE: InputMinorMode = InputMinorMode::new(operations::SELECTION);

    /// Text editing minor mode (cursor movement, deletion, etc.)
    pub static TEXT_EDITING_MODE: InputMinorMode = InputMinorMode::new(operations::TEXT_EDITING);

    /// Search minor mode
    pub static SEARCH_MODE: InputMinorMode = InputMinorMode::new(&[
        KeyboardOperation::IncrementalSearchForward,
        KeyboardOperation::IncrementalSearchBackward,
        KeyboardOperation::FindNext,
        KeyboardOperation::FindPrevious,
    ]);

    /// Clipboard minor mode
    pub static CLIPBOARD_MODE: InputMinorMode = InputMinorMode::new(&[
        KeyboardOperation::Cut,
        KeyboardOperation::Copy,
        KeyboardOperation::Paste,
        KeyboardOperation::CycleThroughClipboard,
    ]);

    /// Undo/redo minor mode
    pub static UNDO_REDO_MODE: InputMinorMode =
        InputMinorMode::new(&[KeyboardOperation::Undo, KeyboardOperation::Redo]);

    /// All standard navigation and selection operations combined
    pub static STANDARD_NAVIGATION_MODE: InputMinorMode = InputMinorMode::new(&[
        KeyboardOperation::MoveToNextLine,
        KeyboardOperation::MoveToPreviousLine,
        KeyboardOperation::MoveToNextField,
        KeyboardOperation::MoveToPreviousField,
        KeyboardOperation::DismissOverlay,
        KeyboardOperation::SelectAll,
        KeyboardOperation::DraftNewTask,
    ]);

    /// Text editing with prominent operations for status bar
    pub static TEXT_EDITING_PROMINENT_MODE: InputMinorMode =
        InputMinorMode::with_prominent_operations(
            &[
                KeyboardOperation::MoveToBeginningOfLine,
                KeyboardOperation::MoveToEndOfLine,
                KeyboardOperation::MoveForwardOneCharacter,
                KeyboardOperation::MoveBackwardOneCharacter,
                KeyboardOperation::MoveForwardOneWord,
                KeyboardOperation::MoveBackwardOneWord,
                KeyboardOperation::DeleteCharacterForward,
                KeyboardOperation::DeleteCharacterBackward,
                KeyboardOperation::DeleteWordForward,
                KeyboardOperation::DeleteWordBackward,
                KeyboardOperation::DeleteToEndOfLine,
                KeyboardOperation::DeleteToBeginningOfLine,
            ],
            &[
                KeyboardOperation::DeleteCharacterForward,
                KeyboardOperation::DeleteCharacterBackward,
                KeyboardOperation::Undo,
                KeyboardOperation::Copy,
                KeyboardOperation::Paste,
            ],
        );

    /// Navigation with prominent operations
    pub static NAVIGATION_PROMINENT_MODE: InputMinorMode =
        InputMinorMode::with_prominent_operations(
            operations::NAVIGATION,
            &[
                KeyboardOperation::MoveToNextLine,
                KeyboardOperation::MoveToPreviousLine,
                KeyboardOperation::MoveToNextField,
                KeyboardOperation::MoveToPreviousField,
            ],
        );

    /// Selection with prominent operations
    pub static SELECTION_PROMINENT_MODE: InputMinorMode =
        InputMinorMode::with_prominent_operations(operations::SELECTION, operations::SELECTION);

    /// Session viewer search mode
    pub static SESSION_VIEWER_SEARCH_MODE: InputMinorMode = InputMinorMode::new(&[
        KeyboardOperation::IncrementalSearchForward,
        KeyboardOperation::IncrementalSearchBackward,
        KeyboardOperation::FindNext,
        KeyboardOperation::FindPrevious,
    ]);

    /// Session viewer selection mode (for overlays and instructions)
    pub static SESSION_VIEWER_SELECTION_MODE: InputMinorMode = InputMinorMode::new(&[
        KeyboardOperation::DismissOverlay,
        KeyboardOperation::DraftNewTask,
        KeyboardOperation::SelectAll,
    ]);

    /// Autocomplete accept mode (shown when inline suggestions are available)
    pub static AUTOCOMPLETE_ACTIVE_MODE: InputMinorMode = InputMinorMode::with_prominent_operations(
        &[KeyboardOperation::IndentOrComplete],
        &[KeyboardOperation::IndentOrComplete],
    );
}

/// Input minor mode defining a set of keyboard operations for a UI context
///
/// An input minor mode defines which keyboard operations are available in a particular UI context.
/// These are used to resolve key events to operations based on user-configured shortcut mappings.
/// Input minor modes are static collections of operations that minimize memory allocations.
pub struct InputMinorMode {
    /// Reference to static slice of keyboard operations that this input minor mode handles
    /// (avoids allocating a HashSet for small operation sets)
    operations: &'static [KeyboardOperation],
    /// Keyboard operations to display prominently in the footer
    /// (rendered dynamically based on configured shortcuts)
    prominent_operations: &'static [KeyboardOperation],
}

impl std::fmt::Debug for InputMinorMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InputMinorMode")
            .field("operations", &self.operations)
            .field("prominent_operations", &self.prominent_operations)
            .finish()
    }
}

impl InputMinorMode {
    /// Create a new input minor mode with the specified operations
    ///
    /// # Parameters
    /// - `operations`: The static slice of keyboard operations this input minor mode should handle
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use crate::view_model::input::minor_modes;
    ///
    /// let input_minor_mode = &minor_modes::STANDARD_NAVIGATION_MODE;
    /// ```
    pub const fn new(operations: &'static [KeyboardOperation]) -> Self {
        Self {
            operations,
            prominent_operations: &[],
        }
    }

    /// Create a new input minor mode with the specified operations and prominent operations
    ///
    /// # Parameters
    /// - `operations`: The static slice of keyboard operations this input minor mode should handle
    /// - `prominent_operations`: Operations to display prominently in the footer
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use crate::view_model::input::minor_modes;
    ///
    /// let input_minor_mode = &minor_modes::TEXT_EDITING_MODE_PROMINENT;
    /// ```
    pub const fn with_prominent_operations(
        operations: &'static [KeyboardOperation],
        prominent_operations: &'static [KeyboardOperation],
    ) -> Self {
        Self {
            operations,
            prominent_operations,
        }
    }

    /// Check if this input minor mode handles the given keyboard operation
    pub fn handles_operation(&self, operation: &KeyboardOperation) -> bool {
        self.operations.contains(operation)
    }

    /// Get all operations supported by this input minor mode as a slice
    pub fn operations(&self) -> &'static [KeyboardOperation] {
        self.operations
    }

    /// Get all operations supported by this input minor mode
    pub fn supported_operations(&self) -> impl Iterator<Item = &KeyboardOperation> {
        self.operations.iter()
    }

    /// Get the operations that should be displayed prominently in the footer
    pub fn prominent_operations(&self) -> &'static [KeyboardOperation] {
        self.prominent_operations
    }

    /// Resolve a key event to a keyboard operation within this input minor mode
    ///
    /// Checks if the given key event matches any of the keyboard operations
    /// defined in this input minor mode, using the current keymap settings.
    ///
    /// # Parameters
    /// - `key_event`: The key event to resolve
    /// - `settings`: Settings containing the keymap configuration
    ///
    /// # Returns
    /// `Some(operation)` if a matching operation was found, `None` otherwise
    pub fn resolve_key_to_operation(
        &self,
        key_event: &KeyEvent,
        settings: &crate::Settings,
    ) -> Option<KeyboardOperation> {
        let keymap = settings.keymap();

        // Check each operation in this minor mode for a match
        for operation in self.operations.iter() {
            if keymap.matches(*operation, key_event) {
                return Some(*operation);
            }
        }

        None
    }
}

impl Default for InputMinorMode {
    fn default() -> Self {
        // Create an empty input minor mode that handles no operations
        static EMPTY_OPERATIONS: &[KeyboardOperation] = &[];
        Self {
            operations: EMPTY_OPERATIONS,
            prominent_operations: EMPTY_OPERATIONS,
        }
    }
}

impl Clone for InputMinorMode {
    fn clone(&self) -> Self {
        // Copy the references to the static slices
        Self {
            operations: self.operations,
            prominent_operations: self.prominent_operations,
        }
    }
}
