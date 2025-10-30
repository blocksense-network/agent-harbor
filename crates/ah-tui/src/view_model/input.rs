// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Input handling and keyboard operation mapping
//!
//! This module provides a flexible input system for managing keyboard shortcuts and input states
//! in the TUI. The system is designed around the concept of "input states" that can be pushed
//! and popped from a stack, allowing different UI contexts to define their own keyboard behavior.
//!
//! ## Architecture
//!
//! The input system consists of three main components:
//!
//! 1. **Keyboard Operations**: Semantic operations (like `MoveToNextLine`, `DeleteCharacterForward`)
//!    that represent user intentions, defined in `settings.rs`
//!
//! 2. **Input States**: Context-specific collections of keyboard operations mapped to closures.
//!    Each input state defines which operations are available in a particular UI context and
//!    what should happen when those operations are triggered.
//!
//! 3. **Input State Stack**: A stack of input states that allows overlaying different input
//!    contexts (e.g., modal dialogs on top of the main interface).
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
//! use crate::view_model::input::{operations, InputResult};
//! use std::rc::Rc;
//! use std::cell::RefCell;
//!
//! // Create a shared view model for coordinated state management
//! let view_model = Rc::new(RefCell::new(MyViewModel::new()));
//!
//! // In a view model, define an input state locally using operation constants
//! // (no memory allocation for the operation list - uses static references)
//! let view_model_clone = view_model.clone();
//! let input_state = InputState::with_prominent_operations(
//!     operations::STANDARD_NAVIGATION,
//!     operations::prominent::ACTIONS,  // Show these operations prominently in footer
//!     move |operation| {
//!         let mut vm = view_model_clone.borrow_mut();
//!         match operation {
//!             KeyboardOperation::MoveToNextLine => {
//!                 vm.move_cursor_down();
//!                 InputResult::Handled
//!             }
//!             KeyboardOperation::NewDraft => {
//!                 // Bubble this operation to lower states in the stack
//!                 // Keyboard shortcuts won't be resolved again
//!                 InputResult::Bubble(KeyboardOperation::CreateNewDocument)
//!             }
//!             _ => InputResult::NotHandled
//!         }
//!     }
//! );
//!
//! // Push the state when entering the context
//! input_state_stack.push(input_state);
//!
//! // Handle key events
//! if input_state_stack.handle_key_event(&key_event, &settings) {
//!     // Event was handled (directly or via bubbling)
//! } else {
//!     // Event was not handled by any input state
//! }
//!
//! // Pop when leaving the context
//! input_state_stack.pop();
//! ```
//!
//! ## Input Result Types
//!
//! Input handlers return an [`InputResult`] enum to control event processing:
//!
//! - **`Handled`**: The operation was fully handled, stop processing
//! - **`NotHandled`**: This state doesn't handle the operation, continue to next state in stack
//! - **`Bubble(operation)`**: Transform and bubble the operation to lower states in the stack.
//!   Keyboard shortcuts are not resolved again - the system looks for states that accept the
//!   specific bubbled operation. This allows operations to be transformed as they propagate
//!   through the input stack.
//!
//! ## Input State Definition
//!
//! Input states should be defined locally in each module where they're relevant:
//! - **Operation constants**: Use the `operations` module constants for common operation sets
//!   (zero memory allocation - static references to pre-defined operation arrays)
//! - **Dynamic states**: Create InputState instances with appropriate operations and handlers
//!
//! Operation constants minimize memory allocations by referencing static slices instead of
//! allocating collections. While complete input states cannot be constants (due to closures),
//! operation sets are stored as static arrays that require no runtime allocation.
//!
//! This keeps the input logic co-located with the UI logic that uses it.
//!
//! ## Memory Management Note
//!
//! For realistic TUI applications, **shared mutable state is required** for coordinated input handling.
//! Since input handlers must be `'static` closures, you cannot borrow external state directly.
//!
//! **Required Pattern:** Use `Rc<RefCell<T>>` (or `Arc<Mutex<T>>` for multi-threading) to share
//! view model state between multiple input handlers. This provides:
//! - **Single source of truth**: All input handlers coordinate on the same state
//! - **Safe interior mutability**: Multiple closures can mutate shared state
//! - **Reference counting**: Automatic cleanup when no longer needed
//!
//! Direct borrowing or cloning state defeats the purpose of coordinated input handling.

use ratatui::crossterm::event::KeyEvent;

use crate::settings::KeyboardOperation;

/// Result of handling a keyboard operation by an input state
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputResult {
    /// The operation was handled and processing should stop
    Handled,
    /// This state doesn't handle this operation, continue searching down the stack
    NotHandled,
    /// Bubble this operation up to higher states in the stack
    /// Keyboard shortcuts won't be resolved again - we'll look for states that accept this specific operation
    Bubble(KeyboardOperation),
}

/// Common operation sets for input states
///
/// These constants define groups of keyboard operations that are commonly
/// handled together in different UI contexts.
pub mod operations {
    use super::KeyboardOperation;

    /// Navigation operations (arrow keys, tab, etc.)
    pub const NAVIGATION: &[KeyboardOperation] = &[
        KeyboardOperation::MoveToNextLine,
        KeyboardOperation::MoveToPreviousLine,
        KeyboardOperation::MoveToNextField,
        KeyboardOperation::MoveToPreviousField,
    ];

    /// Selection operations (enter, space, etc.)
    pub const SELECTION: &[KeyboardOperation] = &[
        KeyboardOperation::DismissOverlay,
        KeyboardOperation::SelectAll,
        KeyboardOperation::NewDraft,
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

    /// Search operations
    pub const SEARCH: &[KeyboardOperation] = &[
        KeyboardOperation::IncrementalSearchForward,
        KeyboardOperation::IncrementalSearchBackward,
        KeyboardOperation::FindNext,
        KeyboardOperation::FindPrevious,
    ];

    /// Clipboard operations
    pub const CLIPBOARD: &[KeyboardOperation] = &[
        KeyboardOperation::Cut,
        KeyboardOperation::Copy,
        KeyboardOperation::Paste,
        KeyboardOperation::CycleThroughClipboard,
    ];

    /// Undo/redo operations
    pub const UNDO_REDO: &[KeyboardOperation] = &[KeyboardOperation::Undo, KeyboardOperation::Redo];

    /// All standard navigation and selection operations combined
    pub const STANDARD_NAVIGATION: &[KeyboardOperation] = &[
        KeyboardOperation::MoveToNextLine,
        KeyboardOperation::MoveToPreviousLine,
        KeyboardOperation::MoveToNextField,
        KeyboardOperation::MoveToPreviousField,
        KeyboardOperation::DismissOverlay,
        KeyboardOperation::SelectAll,
        KeyboardOperation::NewDraft,
    ];

    /// Common operations to display prominently in footers
    pub mod prominent {
        use super::KeyboardOperation;

        /// Basic navigation operations (up/down/left/right)
        pub const NAVIGATION: &[KeyboardOperation] = &[
            KeyboardOperation::MoveToNextLine,
            KeyboardOperation::MoveToPreviousLine,
            KeyboardOperation::MoveToNextField,
            KeyboardOperation::MoveToPreviousField,
        ];

        /// Selection and action operations
        pub const ACTIONS: &[KeyboardOperation] = &[
            KeyboardOperation::DismissOverlay,
            KeyboardOperation::SelectAll,
            KeyboardOperation::NewDraft,
        ];

        /// Common text editing operations
        pub const TEXT_EDITING: &[KeyboardOperation] = &[
            KeyboardOperation::DeleteCharacterForward,
            KeyboardOperation::DeleteCharacterBackward,
            KeyboardOperation::Undo,
            KeyboardOperation::Copy,
            KeyboardOperation::Paste,
        ];
    }
}

/// Map a key event to a keyboard operation using the provided settings
///
/// This function is kept for backwards compatibility but is generally not recommended.
/// Instead, use InputState::handle_key_event which only checks operations that the
/// input state actually handles.
pub fn key_event_to_operation(
    key_event: &KeyEvent,
    settings: &crate::Settings,
) -> Option<KeyboardOperation> {
    // Get the keymap configuration from settings and check all possible keyboard operations
    let keymap = settings.keymap();

    // Define all operations we care about in the TUI that should be configurable
    let operations_to_check = vec![
        KeyboardOperation::MoveToPreviousLine,
        KeyboardOperation::MoveToNextLine,
        KeyboardOperation::MoveToNextField,
        KeyboardOperation::MoveToPreviousField,
        KeyboardOperation::NextSnapshot,
        KeyboardOperation::PreviousSnapshot,
        KeyboardOperation::MoveToBeginningOfDocument,
        KeyboardOperation::MoveToEndOfDocument,
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

/// Input state management for overlayable keyboard operations
///
/// An input state defines which keyboard operations are available in a particular UI context
/// and what should happen when those operations are triggered. Input states can be pushed
/// and popped from an InputStateStack to create overlayable input contexts.
///
/// To minimize memory allocations, input states can reference static operation slices
/// instead of owning collections of operations.
pub struct InputState {
    /// Reference to static slice of keyboard operations that this input state handles
    /// (avoids allocating a HashSet for small operation sets)
    operations: &'static [KeyboardOperation],
    /// Handler closure that receives the keyboard operation and originating key event to process
    /// Returns an InputResult indicating how the operation was handled
    handler: Option<Box<dyn FnMut(KeyboardOperation, &KeyEvent) -> InputResult + 'static>>,
    /// Keyboard operations to display prominently in the footer
    /// (rendered dynamically based on configured shortcuts)
    prominent_operations: &'static [KeyboardOperation],
}

impl std::fmt::Debug for InputState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InputState")
            .field("operations", &self.operations)
            .field("has_handler", &self.handler.is_some())
            .field("prominent_operations", &self.prominent_operations)
            .finish()
    }
}

impl InputState {
    /// Create a new input state with the specified operations and handler
    ///
    /// # Parameters
    /// - `operations`: The static slice of keyboard operations this input state should handle
    /// - `handler`: The closure that will be called with keyboard operations to process.
    ///              Returns an InputResult indicating how the operation was handled.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use crate::view_model::input::{operations, InputResult};
    ///
    /// let input_state = InputState::new(operations::STANDARD_NAVIGATION, |operation, key_event| {
    ///     view_model.handle_keyboard_operation(operation, key_event);
    ///     InputResult::Handled // Indicate that the operation was handled
    /// });
    /// ```
    pub fn new<F>(operations: &'static [KeyboardOperation], handler: F) -> Self
    where
        F: FnMut(KeyboardOperation, &KeyEvent) -> InputResult + 'static,
    {
        Self {
            operations,
            handler: Some(Box::new(handler)),
            prominent_operations: &[],
        }
    }

    /// Create a new input state with the specified operations and handler, plus prominent operations
    ///
    /// # Parameters
    /// - `operations`: The static slice of keyboard operations this input state should handle
    /// - `prominent_operations`: Operations to display prominently in the footer
    /// - `handler`: The closure that will be called with keyboard operations to process.
    ///              Returns an InputResult indicating how the operation was handled.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use crate::view_model::input::InputResult;
    ///
    /// let input_state = InputState::with_prominent_operations(
    ///     operations::STANDARD_NAVIGATION,
    ///     &[KeyboardOperation::DismissOverlay, KeyboardOperation::SelectAll],
    ///     |operation, key_event| {
    ///         view_model.handle_keyboard_operation(operation, key_event);
    ///         InputResult::Handled // Indicate that the operation was handled
    ///     }
    /// );
    /// ```
    pub fn with_prominent_operations<F>(
        operations: &'static [KeyboardOperation],
        prominent_operations: &'static [KeyboardOperation],
        handler: F,
    ) -> Self
    where
        F: FnMut(KeyboardOperation, &KeyEvent) -> InputResult + 'static,
    {
        Self {
            operations,
            handler: Some(Box::new(handler)),
            prominent_operations,
        }
    }

    /// Check if this input state handles the given keyboard operation
    pub fn handles_operation(&self, operation: &KeyboardOperation) -> bool {
        self.operations.contains(operation)
    }

    /// Execute the handler for the given keyboard operation
    ///
    /// Returns an InputResult indicating how the operation was handled.
    pub fn execute_operation(
        &mut self,
        operation: KeyboardOperation,
        key_event: &KeyEvent,
    ) -> InputResult {
        if self.handles_operation(&operation) {
            if let Some(handler) = &mut self.handler {
                handler(operation, key_event)
            } else {
                InputResult::NotHandled
            }
        } else {
            InputResult::NotHandled
        }
    }

    /// Handle a key event by mapping it to operations this state handles
    ///
    /// This method is more efficient than the global key_event_to_operation function
    /// because it only checks operations that this input state actually handles.
    ///
    /// Returns an InputResult indicating how the key event was handled.
    pub fn handle_key_event(
        &mut self,
        key_event: &KeyEvent,
        settings: &crate::Settings,
    ) -> InputResult {
        // Get the keymap configuration from settings
        let keymap = settings.keymap();

        // Only check operations that this input state actually handles
        for operation in self.operations.iter() {
            if keymap.matches(*operation, key_event) {
                return self.execute_operation(*operation, key_event);
            }
        }

        // No operation this state handles matched the key event
        InputResult::NotHandled
    }

    /// Get all operations supported by this input state
    pub fn supported_operations(&self) -> impl Iterator<Item = &KeyboardOperation> {
        self.operations.iter()
    }

    /// Get the operations that should be displayed prominently in the footer
    pub fn prominent_operations(&self) -> &'static [KeyboardOperation] {
        self.prominent_operations
    }
}

impl Default for InputState {
    fn default() -> Self {
        // Create an empty input state that handles no operations
        static EMPTY_OPERATIONS: &[KeyboardOperation] = &[];
        Self {
            operations: EMPTY_OPERATIONS,
            handler: None,
            prominent_operations: EMPTY_OPERATIONS,
        }
    }
}

impl Clone for InputState {
    fn clone(&self) -> Self {
        // Note: closures cannot be cloned, so we create an empty state
        // This is by design - input states should be constructed fresh
        Self {
            operations: self.operations, // Copy the reference to the static slice
            handler: None,               // Handler cannot be cloned
            prominent_operations: self.prominent_operations, // Copy the reference to the static slice
        }
    }
}

/// Stack of input states for overlayable input handling
///
/// The InputStateStack manages a stack of input states, allowing different UI contexts
/// to define their own keyboard behavior. When handling a key event, the stack checks
/// the top-most (current) input state first, then falls back to lower states if the
/// key event is not handled.
#[derive(Debug)]
pub struct InputStateStack {
    states: Vec<InputState>,
    default_state: InputState,
}

impl InputStateStack {
    /// Create a new empty input state stack
    pub fn new() -> Self {
        Self {
            states: Vec::new(),
            default_state: InputState::default(),
        }
    }

    /// Get a reference to the current stack of input states
    ///
    /// Returns the internal vector of pushed states (not including the default state).
    pub fn states(&self) -> &Vec<InputState> {
        &self.states
    }

    /// Push a new input state onto the stack
    ///
    /// The new state becomes the current active input state.
    /// Use this when entering a new UI context (e.g., opening a modal dialog).
    pub fn push(&mut self, state: InputState) {
        self.states.push(state);
    }

    /// Pop the top input state from the stack
    ///
    /// Returns the popped state, or None if the stack was empty.
    /// Use this when leaving a UI context.
    pub fn pop(&mut self) -> Option<InputState> {
        self.states.pop()
    }

    /// Clear all input states from the stack
    ///
    /// After clearing, only the default state will be available.
    pub fn clear(&mut self) {
        self.states.clear();
    }

    /// Get the current active input state (top of stack, or default)
    ///
    /// Returns a reference to the top-most input state, or a default empty state
    /// if the stack is empty.
    pub fn current(&self) -> &InputState {
        self.states.last().unwrap_or(&self.default_state)
    }

    /// Get the current active input state mutably
    ///
    /// This is used internally for executing operations on the current state.
    /// Returns None if the stack is empty (no operations to execute).
    fn current_mut(&mut self) -> Option<&mut InputState> {
        self.states.last_mut()
    }

    /// Handle a key event by mapping it to a keyboard operation and executing the appropriate handler
    ///
    /// This method tries each input state in the stack (starting from the top/most recent)
    /// to handle the key event. Each state only checks operations that it actually handles,
    /// making this much more efficient than checking all possible operations globally.
    ///
    /// States can bubble operations to higher states in the stack using InputResult::Bubble.
    /// When an operation is bubbled, keyboard shortcuts are not resolved again - we look for
    /// the first state that accepts the specific bubbled operation.
    ///
    /// # Returns
    ///
    /// `true` if any input state handled the key event (directly or via bubbling), `false` otherwise.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use crate::view_model::input::InputResult;
    ///
    /// if input_state_stack.handle_key_event(&key_event, &settings) {
    ///     // Event was handled, no further processing needed
    /// } else {
    ///     // Event was not handled, handle it with default logic
    /// }
    /// ```
    pub fn handle_key_event(&mut self, key_event: &KeyEvent, settings: &crate::Settings) -> bool {
        // Try each state in the stack, starting from the top (most recent)
        for i in (0..self.states.len()).rev() {
            match self.states[i].handle_key_event(key_event, settings) {
                InputResult::Handled => return true,
                InputResult::NotHandled => continue,
                InputResult::Bubble(operation) => {
                    // Bubble the operation to remaining states in the stack
                    // Continue searching down the stack for a state that handles this operation
                    if self.handle_bubbled_operation(operation, i, key_event) {
                        return true;
                    }
                }
            }
        }
        // No state handled the key event
        false
    }

    /// Dispatch a keyboard operation directly to the stack without re-mapping shortcuts
    ///
    /// This is useful for tests or for callers that have already resolved the operation
    /// (for example, when bubbling from higher level states). It mirrors the behaviour
    /// of [`handle_key_event`] but skips the key→operation resolution step. Optionally
    /// provide the originating key event so handlers that need key context can access it.
    pub fn dispatch_operation(
        &mut self,
        operation: KeyboardOperation,
        key_event: &KeyEvent,
    ) -> bool {
        for i in (0..self.states.len()).rev() {
            if !self.states[i].handles_operation(&operation) {
                continue;
            }

            match self.states[i].execute_operation(operation, key_event) {
                InputResult::Handled => return true,
                InputResult::NotHandled => continue,
                InputResult::Bubble(next_operation) => {
                    if self.handle_bubbled_operation(next_operation, i, key_event) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Handle a bubbled operation by searching remaining states in the stack
    ///
    /// Returns true if any remaining state handled the operation, false otherwise.
    fn handle_bubbled_operation(
        &mut self,
        operation: KeyboardOperation,
        current_index: usize,
        key_event: &KeyEvent,
    ) -> bool {
        // Search remaining states below the current one (less recent)
        for i in (0..current_index).rev() {
            match self.states[i].execute_operation(operation, key_event) {
                InputResult::Handled => return true,
                InputResult::NotHandled => continue,
                InputResult::Bubble(new_operation) => {
                    // Continue bubbling with the new operation
                    if self.handle_bubbled_operation(new_operation, i, key_event) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Check if ESC should dismiss the current overlay
    ///
    /// Returns `true` if there are any input states on the stack (indicating
    /// we're in an overlay context), `false` if the stack is empty.
    pub fn should_dismiss_on_esc(&self) -> bool {
        !self.states.is_empty()
    }

    /// Check if the stack has any active input states
    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }

    /// Get the number of input states on the stack
    pub fn len(&self) -> usize {
        self.states.len()
    }
}

impl Clone for InputStateStack {
    fn clone(&self) -> Self {
        // Note: InputState cannot be cloned due to closures, so we create empty states
        // This is by design - input state stacks should be managed, not cloned
        Self {
            states: self.states.iter().map(|_| InputState::default()).collect(),
            default_state: InputState::default(),
        }
    }
}
