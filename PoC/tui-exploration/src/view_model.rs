// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! ViewModel layer for TUI presentation logic
//!
//! ViewModel - UI Presentation Logic and State
//!
//! This module transforms domain state into presentation-ready data structures
//! that are optimized for UI rendering. It handles all UI-specific concerns
//! while keeping the domain model UI-agnostic.
//!
//! ## What Belongs Here:
//!
//! ✅ **UI Events**: Key event handling, mouse events, input processing
//! ✅ **UI State**: Selection indices, visual focus states, modal states
//! ✅ **Presentation Models**: `TaskCard`, `ButtonViewModel`, `FooterViewModel`
//! ✅ **UI Logic**: Navigation, focus management, input validation
//! ✅ **UI Types**: `DeliveryIndicator`, `FilterOptions`, `AutoSaveState`
//! ✅ **UI Messages**: `Msg` definition for low-level UI events
//! ✅ **UI Enums**: `FocusElement`, `ModalState`, `SearchMode` for UI state
//! ✅ **Formatting**: Text formatting, display calculations, UI transformations
//!
//! ## What Does NOT Belong Here:
//!
//! ❌ **Business Logic**: Task creation, state transitions, business rules
//! ❌ **Domain Entities**: Core business objects like `TaskExecution`, `DraftTask`
//! ❌ **Rendering**: Actual terminal drawing, Ratatui widget creation
//! ❌ **Domain State**: Collections of business entities, core application state
//!
//! ## Architecture Role:
//!
//! The ViewModel acts as the bridge between the domain model and the UI:
//! 1. **Receives UI events** (key presses, mouse clicks) and translates them to domain messages
//! 2. **Maintains UI state** (current selection, focus, modal visibility)
//! 3. **Transforms domain data** into presentation models optimized for display
//! 4. **Handles UI navigation** (arrow keys, tab navigation, focus cycling)
//!
//! ## Testing Benefits:
//!
//! Following the MVVM architecture outlined in `MVVM-in-Ratatui.md`, the ViewModel is designed
//! to be **fully testable without running the app in an actual terminal**:
//! - **Pure unit tests**: All ViewModel logic can be tested with plain Rust unit tests
//! - **No terminal dependencies**: No need for `TestBackend` or terminal simulation for UI logic
//! - **Fast and reliable**: Tests run instantly without async runtime or terminal setup
//! - **Deterministic**: ViewModel transformations are pure functions of domain state
//! - **Comprehensive coverage**: 90%+ of UI behavior can be tested without touching a terminal
//!
//! ## Message Flow:
//!
//! ```text
//! UI Event → ViewModel.handle_key_event() → Vec<DomainMsg> → Model.update_domain()
//!         ↑                                                        ↓
//!   Updates UI state                                       Updates domain state
//!   (selection, focus)                                     (drafts, executions)
//! ```

use crate::Settings;
use crate::settings::{KeyMatcher, KeyboardOperation, KeyboardShortcut};
use crate::workspace_files::WorkspaceFiles;
use ah_core::task_manager::{
    SaveDraftResult, TaskEvent, TaskLaunchParams, TaskLaunchResult, TaskManager,
};
use ah_domain_types::task::ToolStatus;
use ah_domain_types::{
    DeliveryStatus, DraftTask, SelectedModel, TaskExecution, TaskInfo, TaskState,
};
use ah_tui::view_model::FilterBarViewModel;
use ah_tui::view_model::autocomplete::{AutocompleteKeyResult, InlineAutocomplete};
use ah_tui::view_model::{
    AgentActivityRow, AutoSaveState, ButtonStyle, ButtonViewModel, DeliveryIndicator,
    DraftSaveState, FilterControl, FilterOptions, FocusElement, ModalState, SearchMode,
    TaskCardType, TaskEntryControlsViewModel, TaskEntryViewModel, TaskExecutionViewModel,
    TaskMetadataViewModel,
};
use ah_workflows::WorkspaceWorkflowsEnumerator;
use futures::stream::StreamExt;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::{Modifier, Style};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

const ESC_CONFIRMATION_MESSAGE: &str = "Press Esc again to quit";

/// Focus control navigation (similar to main.rs)
impl ViewModel {
    fn handle_overlay_navigation(&mut self, direction: NavigationDirection) -> bool {
        if self.handle_modal_navigation(direction) {
            return true;
        }
        self.handle_autocomplete_navigation(direction)
    }

    fn clear_exit_confirmation(&mut self) {
        if self.exit_confirmation_armed {
            self.exit_confirmation_armed = false;
            self.exit_requested = false;
            if matches!(
                self.status_bar.status_message.as_deref(),
                Some(ESC_CONFIRMATION_MESSAGE)
            ) {
                self.status_bar.status_message = None;
            }
        }
    }

    fn handle_dismiss_overlay(&mut self) -> bool {
        if self.modal_state != ModalState::None {
            self.close_modal();
            return true;
        }

        if self.autocomplete.is_open() {
            self.autocomplete.close(&mut self.needs_redraw);
            self.exit_confirmation_armed = false;
            self.exit_requested = false;
            if matches!(
                self.status_bar.status_message.as_deref(),
                Some(ESC_CONFIRMATION_MESSAGE)
            ) {
                self.status_bar.status_message = None;
            }
            return true;
        }

        if self.exit_confirmation_armed {
            self.exit_confirmation_armed = false;
            self.exit_requested = true;
            if matches!(
                self.status_bar.status_message.as_deref(),
                Some(ESC_CONFIRMATION_MESSAGE)
            ) {
                self.status_bar.status_message = None;
            }
            return true;
        }

        self.exit_confirmation_armed = true;
        self.exit_requested = false;
        self.status_bar.status_message = Some(ESC_CONFIRMATION_MESSAGE.to_string());
        true
    }

    fn handle_modal_navigation(&mut self, direction: NavigationDirection) -> bool {
        if self.modal_state == ModalState::None {
            return false;
        }

        let Some(modal) = self.active_modal.as_mut() else {
            return false;
        };

        match &mut modal.modal_type {
            ModalType::Search { .. } => {
                if modal.filtered_options.is_empty() {
                    return false;
                }
                match direction {
                    NavigationDirection::Next => {
                        modal.selected_index =
                            (modal.selected_index + 1) % modal.filtered_options.len();
                    }
                    NavigationDirection::Previous => {
                        if modal.selected_index == 0 {
                            modal.selected_index = modal.filtered_options.len() - 1;
                        } else {
                            modal.selected_index -= 1;
                        }
                    }
                }
                for (idx, option) in modal.filtered_options.iter_mut().enumerate() {
                    option.1 = idx == modal.selected_index;
                }
                self.needs_redraw = true;
                true
            }
            ModalType::ModelSelection { options } => {
                if options.is_empty() {
                    return false;
                }
                match direction {
                    NavigationDirection::Next => {
                        modal.selected_index = (modal.selected_index + 1) % options.len();
                    }
                    NavigationDirection::Previous => {
                        if modal.selected_index == 0 {
                            modal.selected_index = options.len() - 1;
                        } else {
                            modal.selected_index -= 1;
                        }
                    }
                }
                self.needs_redraw = true;
                true
            }
            ModalType::Settings { fields } => {
                if fields.is_empty() {
                    return false;
                }
                for field in fields.iter_mut() {
                    field.is_focused = false;
                }
                match direction {
                    NavigationDirection::Next => {
                        modal.selected_index = (modal.selected_index + 1) % fields.len();
                    }
                    NavigationDirection::Previous => {
                        if modal.selected_index == 0 {
                            modal.selected_index = fields.len() - 1;
                        } else {
                            modal.selected_index -= 1;
                        }
                    }
                }
                if let Some(field) = fields.get_mut(modal.selected_index) {
                    field.is_focused = true;
                }
                self.needs_redraw = true;
                true
            }
        }
    }

    fn handle_autocomplete_navigation(&mut self, direction: NavigationDirection) -> bool {
        if !self.autocomplete.is_open() {
            return false;
        }

        let textarea_active = match self.focus_element {
            FocusElement::TaskDescription => true,
            FocusElement::DraftTask(idx) => self
                .draft_cards
                .get(idx)
                .map(|card| card.focus_element == FocusElement::TaskDescription)
                .unwrap_or(false),
            _ => false,
        };

        if !textarea_active {
            return false;
        }

        let handled = match direction {
            NavigationDirection::Next => self.autocomplete.select_next(),
            NavigationDirection::Previous => self.autocomplete.select_previous(),
        };

        if handled {
            self.needs_redraw = true;
        }

        handled
    }

    fn update_modal_filtered_options(&mut self, modal: &mut ModalViewModel) {
        match &modal.modal_type {
            ModalType::Search { .. } => {
                // Get all available options based on modal state
                let all_options = match self.modal_state {
                    ModalState::RepositorySearch => &self.available_repositories,
                    ModalState::BranchSearch => &self.available_branches,
                    ModalState::ModelSearch => &self.available_models,
                    _ => &Vec::new(),
                };

                // Filter options based on input value (case-insensitive fuzzy match)
                let query = modal.input_value.to_lowercase();
                let mut filtered: Vec<(String, bool)> = all_options
                    .iter()
                    .filter(|option| {
                        if query.is_empty() {
                            true // Show all options when no query
                        } else {
                            option.to_lowercase().contains(&query)
                        }
                    })
                    .cloned()
                    .map(|opt| (opt, false))
                    .collect();

                // Reset selected index if it's out of bounds
                if modal.selected_index >= filtered.len() && !filtered.is_empty() {
                    modal.selected_index = 0;
                }

                // Mark the selected option
                if !filtered.is_empty() && modal.selected_index < filtered.len() {
                    filtered[modal.selected_index].1 = true;
                }

                modal.filtered_options = filtered;
            }
            ModalType::ModelSelection { options } => {
                // For model selection, filter the available model options
                let query = modal.input_value.to_lowercase();
                let mut filtered: Vec<(String, bool)> = options
                    .iter()
                    .filter(|option| {
                        if query.is_empty() {
                            true // Show all options when no query
                        } else {
                            option.name.to_lowercase().contains(&query)
                        }
                    })
                    .map(|opt| (format!("{} (x{})", opt.name, opt.count), false))
                    .collect();

                // Reset selected index if it's out of bounds
                if modal.selected_index >= filtered.len() && !filtered.is_empty() {
                    modal.selected_index = 0;
                }

                // Mark the selected option
                if !filtered.is_empty() && modal.selected_index < filtered.len() {
                    filtered[modal.selected_index].1 = true;
                }

                modal.filtered_options = filtered;
            }
            ModalType::Settings { .. } => {
                // Settings don't have filtered options based on input
                modal.filtered_options = Vec::new();
            }
        }
    }
    /// Navigate to the next focusable control
    pub fn focus_next_control(&mut self) -> bool {
        // Implement PRD-compliant tab navigation for draft cards
        match self.focus_element {
            FocusElement::DraftTask(idx) => {
                // When on a draft task, Tab should cycle through the card's internal controls
                if let Some(card) = self.draft_cards.get_mut(idx) {
                    match card.focus_element {
                        FocusElement::TaskDescription => {
                            card.focus_element = FocusElement::RepositorySelector;
                        }
                        FocusElement::RepositorySelector => {
                            card.focus_element = FocusElement::BranchSelector;
                        }
                        FocusElement::BranchSelector => {
                            card.focus_element = FocusElement::ModelSelector;
                        }
                        FocusElement::ModelSelector => {
                            card.focus_element = FocusElement::GoButton;
                        }
                        FocusElement::GoButton => {
                            card.focus_element = FocusElement::TaskDescription; // Cycle back to start
                        }
                        _ => {
                            card.focus_element = FocusElement::RepositorySelector;
                        }
                    }
                    return true;
                }
                false
            }
            // For other global focus elements, handle normally
            FocusElement::SettingsButton => {
                if !self.draft_cards.is_empty() {
                    self.focus_element = FocusElement::DraftTask(0);
                    true
                } else if !self.task_cards.is_empty() {
                    self.focus_element = FocusElement::FilterBarSeparator;
                    true
                } else {
                    false // Stay on settings if nothing else
                }
            }
            _ => {
                self.focus_element = FocusElement::SettingsButton;
                true
            }
        }
    }

    /// Navigate to the previous focusable control
    pub fn focus_previous_control(&mut self) -> bool {
        // Implement PRD-compliant shift+tab navigation for draft cards (reverse order)
        match self.focus_element {
            FocusElement::DraftTask(idx) => {
                // When on a draft task, Shift+Tab should cycle through the card's internal controls in reverse
                if let Some(card) = self.draft_cards.get_mut(idx) {
                    let old_internal_focus = card.focus_element;
                    match card.focus_element {
                        FocusElement::TaskDescription => {
                            card.focus_element = FocusElement::GoButton;
                        }
                        FocusElement::GoButton => {
                            card.focus_element = FocusElement::ModelSelector;
                        }
                        FocusElement::ModelSelector => {
                            card.focus_element = FocusElement::BranchSelector;
                        }
                        FocusElement::BranchSelector => {
                            card.focus_element = FocusElement::RepositorySelector;
                        }
                        FocusElement::RepositorySelector => {
                            card.focus_element = FocusElement::TaskDescription; // Cycle back to end
                        }
                        _ => {
                            card.focus_element = FocusElement::GoButton;
                        }
                    }
                    return true;
                }
                false
            }
            // For other global focus elements, handle normally
            FocusElement::SettingsButton => {
                if !self.task_cards.is_empty() {
                    self.focus_element = FocusElement::ExistingTask(self.task_cards.len() - 1);
                    true
                } else if !self.draft_cards.is_empty() {
                    self.focus_element = FocusElement::DraftTask(self.draft_cards.len() - 1);
                    true
                } else {
                    false // Stay on settings if nothing else
                }
            }
            _ => {
                self.focus_element = FocusElement::SettingsButton;
                true
            }
        }
    }

    /// Handle character input in focused text areas
    pub fn handle_char_input(&mut self, ch: char) -> bool {
        // Handle modal input when a modal is active
        if let Some(ref mut modal) = self.active_modal.as_mut() {
            match &modal.modal_type {
                ModalType::Search { .. } => {
                    // For search modals, add character to input value and update filtering
                    modal.input_value.push(ch);

                    // Inline filtering logic to avoid double borrow
                    let all_options = match self.modal_state {
                        ModalState::RepositorySearch => &self.available_repositories,
                        ModalState::BranchSearch => &self.available_branches,
                        ModalState::ModelSearch => &self.available_models,
                        _ => &Vec::new(),
                    };

                    let query = modal.input_value.to_lowercase();
                    let mut filtered: Vec<(String, bool)> = all_options
                        .iter()
                        .filter(|option| {
                            if query.is_empty() {
                                true // Show all options when no query
                            } else {
                                option.to_lowercase().contains(&query)
                            }
                        })
                        .cloned()
                        .map(|opt| (opt, false))
                        .collect();

                    // Reset selected index if it's out of bounds
                    if modal.selected_index >= filtered.len() && !filtered.is_empty() {
                        modal.selected_index = 0;
                    }

                    // Mark the selected option
                    if !filtered.is_empty() && modal.selected_index < filtered.len() {
                        filtered[modal.selected_index].1 = true;
                    }

                    modal.filtered_options = filtered;
                    self.needs_redraw = true;
                    return true;
                }
                ModalType::ModelSelection { .. } => {
                    // Model selection modals use search input similar to search modals
                    modal.input_value.push(ch);

                    // Inline filtering logic to avoid double borrow
                    let query = modal.input_value.to_lowercase();
                    let mut filtered: Vec<(String, bool)> =
                        if let ModalType::ModelSelection { options } = &modal.modal_type {
                            options
                                .iter()
                                .filter(|option| {
                                    if query.is_empty() {
                                        true // Show all options when no query
                                    } else {
                                        option.name.to_lowercase().contains(&query)
                                    }
                                })
                                .map(|opt| (format!("{} (x{})", opt.name, opt.count), false))
                                .collect()
                        } else {
                            Vec::new()
                        };

                    // Reset selected index if it's out of bounds
                    if modal.selected_index >= filtered.len() && !filtered.is_empty() {
                        modal.selected_index = 0;
                    }

                    // Mark the selected option
                    if !filtered.is_empty() && modal.selected_index < filtered.len() {
                        filtered[modal.selected_index].1 = true;
                    }

                    modal.filtered_options = filtered;
                    self.needs_redraw = true;
                    return true;
                }
                ModalType::Settings { .. } => {
                    // Settings modals may have text input fields - for now, ignore character input
                    // as settings are handled via navigation and selection
                    return false;
                }
            }
        }

        // Allow text input when focused on draft-related elements
        match self.focus_element {
            FocusElement::TaskDescription
            | FocusElement::RepositorySelector
            | FocusElement::BranchSelector
            | FocusElement::ModelSelector
            | FocusElement::GoButton
            | FocusElement::DraftTask(_) => {
                // Support editing the description when focused on TaskDescription or any DraftTask
                if let FocusElement::TaskDescription = self.focus_element {
                    // Get the first (and currently only) draft card
                    if let Some(card) = self.draft_cards.get_mut(0) {
                        // Feed the character to the textarea widget
                        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
                        let key_event = KeyEvent::new(KeyCode::Char(ch), KeyModifiers::empty());
                        self.autocomplete.notify_text_input();
                        card.description.input(key_event);

                        // Trigger autocomplete after textarea change
                        self.autocomplete
                            .after_textarea_change(&card.description, &mut self.needs_redraw);

                        card.save_state = DraftSaveState::Unsaved;
                        // Reset auto-save timer
                        card.auto_save_timer = Some(std::time::Instant::now());
                        return true;
                    }
                } else if let FocusElement::DraftTask(idx) = self.focus_element {
                    // When a draft task is focused, edit its description only if internal focus is on text area
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == FocusElement::TaskDescription {
                            // Feed the character to the textarea widget
                            use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
                            let key_event = KeyEvent::new(KeyCode::Char(ch), KeyModifiers::empty());
                            self.autocomplete.notify_text_input();
                            card.description.input(key_event);

                            // Trigger autocomplete after textarea change
                            self.autocomplete
                                .after_textarea_change(&card.description, &mut self.needs_redraw);

                            card.save_state = DraftSaveState::Unsaved;
                            // Reset auto-save timer
                            card.auto_save_timer = Some(std::time::Instant::now());
                            return true;
                        }
                    }
                }
            }
            _ => {}
        }
        false
    }

    /// Handle backspace in focused text areas and modals
    pub fn handle_backspace(&mut self) -> bool {
        // Handle modal backspace when a modal is active
        if let Some(ref mut modal) = self.active_modal {
            match &modal.modal_type {
                ModalType::Search { .. } => {
                    // For search modals, remove last character from input value
                    if !modal.input_value.is_empty() {
                        modal.input_value.pop();

                        // Inline filtering logic to avoid double borrow
                        let all_options = match self.modal_state {
                            ModalState::RepositorySearch => &self.available_repositories,
                            ModalState::BranchSearch => &self.available_branches,
                            ModalState::ModelSearch => &self.available_models,
                            _ => &Vec::new(),
                        };

                        let query = modal.input_value.to_lowercase();
                        let mut filtered: Vec<(String, bool)> = all_options
                            .iter()
                            .filter(|option| {
                                if query.is_empty() {
                                    true // Show all options when no query
                                } else {
                                    option.to_lowercase().contains(&query)
                                }
                            })
                            .cloned()
                            .map(|opt| (opt, false))
                            .collect();

                        // Reset selected index if it's out of bounds
                        if modal.selected_index >= filtered.len() && !filtered.is_empty() {
                            modal.selected_index = 0;
                        }

                        // Mark the selected option
                        if !filtered.is_empty() && modal.selected_index < filtered.len() {
                            filtered[modal.selected_index].1 = true;
                        }

                        modal.filtered_options = filtered;
                        self.needs_redraw = true;
                        return true;
                    }
                }
                ModalType::ModelSelection { .. } => {
                    // For model selection modals, remove last character from input value
                    if !modal.input_value.is_empty() {
                        modal.input_value.pop();

                        // Inline filtering logic to avoid double borrow
                        let query = modal.input_value.to_lowercase();
                        let mut filtered: Vec<(String, bool)> =
                            if let ModalType::ModelSelection { options } = &modal.modal_type {
                                options
                                    .iter()
                                    .filter(|option| {
                                        if query.is_empty() {
                                            true // Show all options when no query
                                        } else {
                                            option.name.to_lowercase().contains(&query)
                                        }
                                    })
                                    .map(|opt| (format!("{} (x{})", opt.name, opt.count), false))
                                    .collect()
                            } else {
                                Vec::new()
                            };

                        // Reset selected index if it's out of bounds
                        if modal.selected_index >= filtered.len() && !filtered.is_empty() {
                            modal.selected_index = 0;
                        }

                        // Mark the selected option
                        if !filtered.is_empty() && modal.selected_index < filtered.len() {
                            filtered[modal.selected_index].1 = true;
                        }

                        modal.filtered_options = filtered;
                        self.needs_redraw = true;
                        return true;
                    }
                }
                ModalType::Settings { .. } => {
                    // Settings modals don't handle backspace
                    return false;
                }
            }
        }

        match self.focus_element {
            FocusElement::TaskDescription => {
                // Note: tui-textarea automatically deletes selected text when backspace is pressed

                // Get the first (and currently only) draft card
                if let Some(card) = self.draft_cards.get_mut(0) {
                    // Feed backspace to the textarea widget
                    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
                    let key_event = KeyEvent::new(KeyCode::Backspace, KeyModifiers::empty());
                    self.autocomplete.notify_text_input();
                    card.description.input(key_event);

                    self.autocomplete
                        .after_textarea_change(&card.description, &mut self.needs_redraw);

                    card.save_state = DraftSaveState::Unsaved;
                    // Reset auto-save timer
                    card.auto_save_timer = Some(std::time::Instant::now());
                    return true;
                }
            }
            FocusElement::DraftTask(idx) => {
                // Note: tui-textarea automatically deletes selected text when backspace is pressed

                // When a draft task is focused, edit its description only if internal focus is on text area
                if let Some(card) = self.draft_cards.get_mut(idx) {
                    if card.focus_element == FocusElement::TaskDescription {
                        // Feed backspace to the textarea widget
                        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
                        let key_event = KeyEvent::new(KeyCode::Backspace, KeyModifiers::empty());
                        self.autocomplete.notify_text_input();
                        card.description.input(key_event);

                        self.autocomplete
                            .after_textarea_change(&card.description, &mut self.needs_redraw);

                        card.save_state = DraftSaveState::Unsaved;
                        // Reset auto-save timer
                        card.auto_save_timer = Some(std::time::Instant::now());
                        return true;
                    }
                }
            }
            _ => {}
        }
        false
    }

    pub fn handle_delete(&mut self) -> bool {
        // Handle modal delete when a modal is active
        if let Some(ref mut modal) = self.active_modal {
            match &modal.modal_type {
                ModalType::Search { .. } => {
                    // For search modals, remove last character from input value (same as backspace)
                    if !modal.input_value.is_empty() {
                        modal.input_value.pop();

                        // Inline filtering logic to avoid double borrow
                        let all_options = match self.modal_state {
                            ModalState::RepositorySearch => &self.available_repositories,
                            ModalState::BranchSearch => &self.available_branches,
                            ModalState::ModelSearch => &self.available_models,
                            _ => &Vec::new(),
                        };

                        let query = modal.input_value.to_lowercase();
                        let mut filtered: Vec<(String, bool)> = all_options
                            .iter()
                            .filter(|option| {
                                if query.is_empty() {
                                    true // Show all options when no query
                                } else {
                                    option.to_lowercase().contains(&query)
                                }
                            })
                            .cloned()
                            .map(|opt| (opt, false))
                            .collect();

                        // Reset selected index if it's out of bounds
                        if modal.selected_index >= filtered.len() && !filtered.is_empty() {
                            modal.selected_index = 0;
                        }

                        // Mark the selected option
                        if !filtered.is_empty() && modal.selected_index < filtered.len() {
                            filtered[modal.selected_index].1 = true;
                        }

                        modal.filtered_options = filtered;
                        self.needs_redraw = true;
                        return true;
                    }
                }
                ModalType::ModelSelection { .. } => {
                    // Model selection modals use search input similar to search modals
                    modal.input_value.pop();

                    // Inline filtering logic to avoid double borrow
                    let query = modal.input_value.to_lowercase();
                    let mut filtered: Vec<(String, bool)> =
                        if let ModalType::ModelSelection { options } = &modal.modal_type {
                            options
                                .iter()
                                .filter(|option| {
                                    if query.is_empty() {
                                        true // Show all options when no query
                                    } else {
                                        option.name.to_lowercase().contains(&query)
                                    }
                                })
                                .map(|opt| (format!("{} (x{})", opt.name, opt.count), false))
                                .collect()
                        } else {
                            Vec::new()
                        };

                    // Reset selected index if it's out of bounds
                    if modal.selected_index >= filtered.len() && !filtered.is_empty() {
                        modal.selected_index = 0;
                    }

                    // Mark the selected option
                    if !filtered.is_empty() && modal.selected_index < filtered.len() {
                        filtered[modal.selected_index].1 = true;
                    }

                    modal.filtered_options = filtered;
                    self.needs_redraw = true;
                    return true;
                }
                ModalType::Settings { .. } => {
                    // Settings modals don't handle delete
                    return false;
                }
            }
        }

        match self.focus_element {
            FocusElement::TaskDescription => {
                // Note: tui-textarea automatically deletes selected text when delete is pressed

                // Get the first (and currently only) draft card
                if let Some(card) = self.draft_cards.get_mut(0) {
                    // Feed delete to the textarea widget
                    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
                    let key_event = KeyEvent::new(KeyCode::Delete, KeyModifiers::empty());
                    self.autocomplete.notify_text_input();
                    card.description.input(key_event);

                    self.autocomplete
                        .after_textarea_change(&card.description, &mut self.needs_redraw);

                    card.save_state = DraftSaveState::Unsaved;
                    // Reset auto-save timer
                    card.auto_save_timer = Some(std::time::Instant::now());
                    return true;
                }
            }
            FocusElement::DraftTask(idx) => {
                // Note: tui-textarea automatically deletes selected text when delete is pressed

                // When a draft task is focused, edit its description only if internal focus is on text area
                if let Some(card) = self.draft_cards.get_mut(idx) {
                    if card.focus_element == FocusElement::TaskDescription {
                        // Feed delete to the textarea widget
                        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
                        let key_event = KeyEvent::new(KeyCode::Delete, KeyModifiers::empty());
                        self.autocomplete.notify_text_input();
                        card.description.input(key_event);

                        self.autocomplete
                            .after_textarea_change(&card.description, &mut self.needs_redraw);

                        card.save_state = DraftSaveState::Unsaved;
                        // Reset auto-save timer
                        card.auto_save_timer = Some(std::time::Instant::now());
                        return true;
                    }
                }
            }
            _ => {}
        }
        false
    }

    /// Handle enter key (including shift+enter for newlines)
    pub fn handle_enter(&mut self, shift: bool) -> bool {
        match self.focus_element {
            FocusElement::DraftTask(idx) => {
                // Handle Enter on a draft card based on its internal focus
                if let Some(card) = self.draft_cards.get(idx) {
                    match card.focus_element {
                        FocusElement::TaskDescription => {
                            if shift {
                                // Shift+Enter: add newline to description
                                if let Some(card) = self.draft_cards.get_mut(idx) {
                                    use ratatui::crossterm::event::{
                                        KeyCode, KeyEvent, KeyModifiers,
                                    };
                                    let key_event =
                                        KeyEvent::new(KeyCode::Enter, KeyModifiers::empty());
                                    self.autocomplete.notify_text_input();
                                    card.description.input(key_event);
                                    self.autocomplete.after_textarea_change(
                                        &card.description,
                                        &mut self.needs_redraw,
                                    );
                                    card.save_state = DraftSaveState::Unsaved;
                                    card.auto_save_timer = Some(std::time::Instant::now());
                                    return true;
                                }
                            } else {
                                // Enter: launch task (same as Go button)
                                return self.handle_go_button();
                            }
                        }
                        FocusElement::RepositorySelector => {
                            self.open_modal(ModalState::RepositorySearch);
                            return true;
                        }
                        FocusElement::BranchSelector => {
                            self.open_modal(ModalState::BranchSearch);
                            return true;
                        }
                        FocusElement::ModelSelector => {
                            self.open_modal(ModalState::ModelSearch);
                            return true;
                        }
                        FocusElement::GoButton => {
                            return self.handle_go_button();
                        }
                        _ => return false,
                    }
                }
                false
            }
            FocusElement::TaskDescription => {
                if shift {
                    // Shift+Enter: add newline to description
                    // Get the first (and currently only) draft card
                    if let Some(card) = self.draft_cards.get_mut(0) {
                        // Feed enter to the textarea widget
                        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
                        let key_event = KeyEvent::new(KeyCode::Enter, KeyModifiers::empty());
                        self.autocomplete.notify_text_input();
                        card.description.input(key_event);
                        self.autocomplete
                            .after_textarea_change(&card.description, &mut self.needs_redraw);

                        card.save_state = DraftSaveState::Unsaved;
                        card.auto_save_timer = Some(std::time::Instant::now());
                        return true;
                    }
                    return false; // No draft card found
                } else {
                    // Enter: launch task (same as Go button)
                    return self.handle_go_button();
                }
            }
            FocusElement::GoButton => {
                return self.handle_go_button();
            }
            FocusElement::RepositorySelector => {
                self.open_modal(ModalState::RepositorySearch);
                return true;
            }
            FocusElement::BranchSelector => {
                self.open_modal(ModalState::BranchSearch);
                return true;
            }
            FocusElement::ModelSelector => {
                self.open_modal(ModalState::ModelSearch);
                return true;
            }
            FocusElement::SettingsButton => {
                self.open_modal(ModalState::Settings);
                return true;
            }
            _ => return false,
        }
    }

    /// Handle Go button activation (task launch)
    pub fn handle_go_button(&mut self) -> bool {
        // Get the first (and currently only) draft card
        if let Some(card) = self.draft_cards.get(0) {
            // Validate that description and models are provided
            if card.description.lines().join("\n").trim().is_empty() {
                self.status_bar.error_message = Some("Task description is required".to_string());
                return false; // Validation failed
            }
            if card.models.is_empty() {
                self.status_bar.error_message =
                    Some("At least one AI model must be selected".to_string());
                return false; // Validation failed
            }

            // TODO: Launch the task via task_manager
            // For now, just clear the error and show success
            self.status_bar.error_message = None;
            self.status_bar.status_message = Some("Task launched successfully".to_string());
            return true; // Success
        }
        false
    }

    /// Handle escape key
    pub fn handle_escape(&mut self) -> bool {
        self.handle_dismiss_overlay()
    }

    /// Handle Ctrl+N to create new draft task
    pub fn handle_ctrl_n(&mut self) -> bool {
        if !self.draft_cards.is_empty() {
            // Create a new draft task based on the first (current) draft
            if let Some(current_card) = self.draft_cards.get(0) {
                let new_draft = DraftTask {
                    id: format!("draft_{}", chrono::Utc::now().timestamp()),
                    description: String::new(),
                    repository: current_card.repository.clone(),
                    branch: current_card.branch.clone(),
                    models: current_card.models.clone(),
                    created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                };

                let new_card =
                    create_draft_card_from_task(new_draft, FocusElement::TaskDescription);
                self.draft_cards.push(new_card);
                let new_index = self.draft_cards.len() - 1;
                self.focus_element = FocusElement::DraftTask(new_index); // Focus on the new draft task
                return true;
            }
        }
        false
    }

    /// Handle auto-save timer tick
    pub fn handle_tick(&mut self) -> bool {
        let mut changed = false;
        for card in &mut self.draft_cards {
            if let Some(timer) = card.auto_save_timer {
                if timer.elapsed() > std::time::Duration::from_millis(500) {
                    if card.save_state == DraftSaveState::Unsaved {
                        card.save_state = DraftSaveState::Saved;
                        card.auto_save_timer = None;
                        changed = true;
                        // TODO: Actually save to storage
                    }
                }
            }
        }
        changed
    }
}

/// Mouse action types for interactive areas
#[derive(Debug, Clone, PartialEq)]
pub enum MouseAction {
    FocusDraftTextarea(usize),
    SelectCard(usize),
    SelectFilterBarLine,
    ActivateGoButton,
    ActivateRepositoryModal,
    ActivateBranchModal,
    ActivateModelModal,
    LaunchTask,
    StopTask(usize),
    OpenSettings,
    EditFilter(FilterControl),
    Footer(FooterAction),
}

/// Footer action types for keyboard shortcuts
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FooterAction {
    LaunchDraft,
    InsertNewLine,
    FocusNextField,
    FocusPreviousField,
    OpenShortcutHelp,
    OpenSettings,
    StopTask(usize),
    Quit,
}

/// UI helper enum to represent items in the unified task list
/// This is used for presentation logic, not domain logic
#[derive(Debug, Clone, PartialEq)]
pub enum TaskItem {
    Draft(DraftTask),
    Task(TaskExecution, usize), // TaskExecution and its original index in the task_executions vector
}

impl TaskItem {
    /// Get the combined list of all tasks (drafts + executions) for UI presentation
    pub fn all_tasks_from_state(
        draft_tasks: &[DraftTask],
        task_executions: &[TaskExecution],
    ) -> Vec<TaskItem> {
        let mut result = Vec::new();

        // Add all draft tasks
        for draft in draft_tasks {
            result.push(TaskItem::Draft(draft.clone()));
        }

        // Add all task executions
        for (i, task) in task_executions.iter().enumerate() {
            result.push(TaskItem::Task(task.clone(), i));
        }

        result
    }
}

/// UI-level messages that are handled by the ViewModel
#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    /// User keyboard input events (translated to domain messages by ViewModel)
    Key(KeyEvent),
    /// Mouse click on a registered interactive element
    MouseClick {
        action: MouseAction,
        column: u16,
        row: u16,
        bounds: ratatui::layout::Rect,
    },
    /// Mouse scroll upwards (equivalent to navigating up)
    MouseScrollUp,
    /// Mouse scroll downwards (equivalent to navigating down)
    MouseScrollDown,
    /// Periodic timer tick for animations/updates
    Tick,
    /// Application lifecycle events
    Quit,
}

/// Information about a task card for fast lookups
#[derive(Debug, Clone)]
pub struct TaskCardInfo {
    pub card_type: TaskCardTypeEnum, // Draft or Task
    pub index: usize,                // Index within the respective collection
}

#[derive(Debug, Clone)]
pub enum TaskCardTypeEnum {
    Draft,
    Task,
}

#[derive(Copy, Clone)]
enum NavigationDirection {
    Next,
    Previous,
}

/// Modal dialog view models
#[derive(Debug, Clone, PartialEq)]
pub struct ModalViewModel {
    pub title: String,
    pub input_value: String,
    pub filtered_options: Vec<(String, bool)>, // (option, is_selected)
    pub selected_index: usize,
    pub modal_type: ModalType,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ModalType {
    Search { placeholder: String },
    ModelSelection { options: Vec<ModelOptionViewModel> },
    Settings { fields: Vec<SettingsFieldViewModel> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelOptionViewModel {
    pub name: String,
    pub count: usize,
    pub is_selected: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SettingsFieldViewModel {
    pub label: String,
    pub value: String,
    pub is_focused: bool,
    pub field_type: SettingsFieldType,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SettingsFieldType {
    Number,
    Boolean,
    Text,
    Selection,
}

/// Footer shortcuts presentation
#[derive(Debug, Clone, PartialEq)]
pub struct FooterViewModel {
    pub shortcuts: Vec<KeyboardShortcut>,
}

/// Status bar presentation
#[derive(Debug, Clone, PartialEq)]
pub struct StatusBarViewModel {
    pub backend_indicator: String, // "local" or hostname
    pub last_operation: String,
    pub connection_status: String,
    pub error_message: Option<String>,
    pub status_message: Option<String>, // Success/status messages
}

/// Main ViewModel containing all presentation state
pub struct ViewModel {
    pub focus_element: FocusElement, // Current UI focus state

    // Modals
    pub active_modal: Option<ModalViewModel>,

    // Footer
    pub footer: FooterViewModel,

    // Status bar
    pub status_bar: StatusBarViewModel,

    // Layout hints
    pub scroll_offset: u16,
    pub needs_scrollbar: bool,
    pub total_content_height: u16,
    pub visible_area_height: u16,

    // Settings configuration
    pub settings: Settings,

    // UI state (moved from Model)
    pub modal_state: ModalState,
    pub search_mode: SearchMode,
    pub word_wrap_enabled: bool,
    pub show_autocomplete_border: bool,
    pub status_message: Option<String>,
    pub error_message: Option<String>,

    // Escape handling state
    pub exit_confirmation_armed: bool,
    pub exit_requested: bool,

    // Loading states (moved from Model)
    pub loading_task_creation: bool,
    pub loading_repositories: bool,
    pub loading_branches: bool,
    pub loading_models: bool,

    // Service dependencies
    pub workspace_files: Arc<dyn WorkspaceFiles>,
    pub workspace_workflows: Arc<dyn WorkspaceWorkflowsEnumerator>,
    pub task_manager: Arc<dyn TaskManager>, // Task launching abstraction

    // Autocomplete system
    pub autocomplete: InlineAutocomplete,

    // Domain state - available options
    pub available_repositories: Vec<String>,
    pub available_branches: Vec<String>,
    pub available_models: Vec<String>,

    // Preloaded autocomplete data
    pub preloaded_files: Vec<String>,
    pub preloaded_workflows: Vec<String>,

    // Background loading state
    pub loading_files: bool,
    pub loading_workflows: bool,
    pub files_loaded: bool,
    pub workflows_loaded: bool,

    // Background loading communication channels
    pub files_sender: Option<oneshot::Sender<Vec<String>>>,
    pub workflows_sender: Option<oneshot::Sender<Vec<String>>>,
    pub files_receiver: Option<oneshot::Receiver<Vec<String>>>,
    pub workflows_receiver: Option<oneshot::Receiver<Vec<String>>>,

    // Task collections - cards contain the domain objects
    pub draft_cards: Vec<TaskEntryViewModel>, // Draft tasks (editable)
    pub task_cards: Vec<TaskExecutionViewModel>, // Regular tasks (active/completed/merged)

    // UI interaction state
    pub selected_card: usize,
    pub last_textarea_area: Option<ratatui::layout::Rect>, // Last rendered textarea area for caret positioning

    // Task event streaming
    pub task_event_sender: Option<mpsc::Sender<(String, TaskEvent)>>, // Shared sender for all task events
    pub task_event_receiver: Option<mpsc::Receiver<(String, TaskEvent)>>, // Shared receiver for all task events
    pub active_task_streams: HashMap<String, tokio::task::JoinHandle<()>>, // Active task event consumers
    pub task_id_to_card_info: HashMap<String, TaskCardInfo>, // Maps task_id to card type and index for fast lookups
    pub needs_redraw: bool, // Flag to indicate when UI needs to be redrawn
}

impl ViewModel {
    /// Create a new ViewModel with service dependencies and start background loading
    /// Create a new ViewModel without background loading (for tests)
    pub fn new(
        workspace_files: Arc<dyn WorkspaceFiles>,
        workspace_workflows: Arc<dyn WorkspaceWorkflowsEnumerator>,
        task_manager: Arc<dyn TaskManager>,
        settings: Settings,
    ) -> Self {
        Self::new_internal(
            workspace_files,
            workspace_workflows,
            task_manager,
            settings,
            false,
        )
    }

    /// Create a new ViewModel with background loading enabled
    pub fn new_with_background_loading(
        workspace_files: Arc<dyn WorkspaceFiles>,
        workspace_workflows: Arc<dyn WorkspaceWorkflowsEnumerator>,
        task_manager: Arc<dyn TaskManager>,
        settings: Settings,
    ) -> Self {
        Self::new_internal(
            workspace_files,
            workspace_workflows,
            task_manager,
            settings,
            true,
        )
    }

    fn new_internal(
        workspace_files: Arc<dyn WorkspaceFiles>,
        workspace_workflows: Arc<dyn WorkspaceWorkflowsEnumerator>,
        task_manager: Arc<dyn TaskManager>,
        settings: Settings,
        with_background_loading: bool,
    ) -> Self {
        // Initialize available options
        let available_repositories = vec![
            "blocksense/agent-harbor".to_string(),
            "example/project".to_string(),
        ];
        let available_branches = vec!["main".to_string(), "develop".to_string()];
        let available_models = vec![
            "Claude 3.5 Sonnet".to_string(),
            "GPT-4".to_string(),
            "Claude 3 Opus".to_string(),
        ];

        // Initialize preloaded data and loading state
        let preloaded_files = vec![];
        let preloaded_workflows = vec![];
        let loading_files = false;
        let loading_workflows = false;
        let files_loaded = false;
        let workflows_loaded = false;

        // Create communication channels for background loading (only if enabled)
        let (files_sender, files_receiver, workflows_sender, workflows_receiver) =
            if with_background_loading {
                let (files_sender, files_receiver) = oneshot::channel();
                let (workflows_sender, workflows_receiver) = oneshot::channel();
                (
                    Some(files_sender),
                    Some(files_receiver),
                    Some(workflows_sender),
                    Some(workflows_receiver),
                )
            } else {
                (None, None, None, None)
            };

        // Create initial draft card with embedded domain object
        let initial_draft = DraftTask {
            id: "current".to_string(),
            description: String::new(),
            repository: "blocksense/agent-harbor".to_string(),
            branch: "main".to_string(),
            models: vec![SelectedModel {
                name: "Claude 3.5 Sonnet".to_string(),
                count: 1,
            }],
            created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        };

        // Determine initial focus element per PRD: "The initially focused element is the top draft task card."
        let initial_global_focus = FocusElement::DraftTask(0); // Focus on the single draft task
        let initial_card_focus = FocusElement::TaskDescription; // Initially focus the text area within the card

        // Create task collections - cards contain the domain objects
        let draft_cards = vec![create_draft_card_from_task(
            initial_draft.clone(),
            initial_card_focus,
        )];
        let task_cards = vec![]; // Start with no task cards

        let focused_draft = &initial_draft;
        let active_modal = create_modal_view_model(
            ModalState::None,
            &available_repositories,
            &available_branches,
            &available_models,
            &Some(initial_draft.clone()),
            settings.activity_rows(),
            true,
            false,
        );
        let footer = create_footer_view_model(
            Some(focused_draft),
            initial_global_focus,
            ModalState::None,
            &settings,
            true,
            false,
        ); // Use initial focus
        let status_bar = create_status_bar_view_model(None, None, false, false, false, false);

        // Calculate layout metrics
        let total_content_height: u16 = task_cards
            .iter()
            .map(|card: &TaskExecutionViewModel| card.height + 1) // +1 for spacer
            .sum::<u16>()
            + 1; // Filter bar height

        ViewModel {
            focus_element: initial_global_focus,

            // Domain state
            available_repositories: available_repositories.clone(),
            available_branches: available_branches.clone(),
            available_models: available_models.clone(),

            // Preloaded autocomplete data
            preloaded_files: preloaded_files.clone(),
            preloaded_workflows: preloaded_workflows.clone(),

            // Background loading state
            loading_files,
            loading_workflows,
            files_loaded,
            workflows_loaded,

            // Background loading communication channels
            files_sender,
            workflows_sender,
            files_receiver,
            workflows_receiver,

            draft_cards: draft_cards.clone(),
            task_cards: task_cards.clone(),
            selected_card: 0,
            last_textarea_area: None,
            active_modal: active_modal.clone(),
            footer: footer.clone(),
            status_bar: status_bar.clone(),
            scroll_offset: 0, // Calculated by View layer based on selection
            needs_scrollbar: total_content_height > 20, // Rough estimate, View layer refines
            total_content_height,
            visible_area_height: 20, // Will be set by View layer

            // Settings configuration
            settings: settings.clone(),

            // Initialize UI state with defaults (moved from Model)
            modal_state: ModalState::None,
            search_mode: SearchMode::None,
            word_wrap_enabled: true,
            show_autocomplete_border: false,
            status_message: None,
            error_message: None,
            exit_confirmation_armed: false,
            exit_requested: false,

            // Initialize loading states
            loading_task_creation: false,
            loading_repositories: false,
            loading_branches: false,
            loading_models: false,

            // Initialize quit flag

            // Service dependencies
            workspace_files: workspace_files.clone(),
            workspace_workflows: workspace_workflows.clone(),
            task_manager: task_manager.clone(),

            // Autocomplete system - initialize with empty providers for now
            autocomplete: InlineAutocomplete::new(),

            // Task event streaming
            task_event_sender: None,
            task_event_receiver: None,
            active_task_streams: HashMap::new(),
            task_id_to_card_info: HashMap::new(),
            needs_redraw: true,
        }
    }
}

impl ViewModel {
    /// Handle incoming UI messages and update ViewModel state
    pub fn update(&mut self, msg: Msg) -> Result<(), String> {
        // Check for completed background loading tasks
        self.check_background_loading();

        match msg {
            Msg::Key(key_event) => {
                // Ignore key up events - we only want to process key down events
                // to avoid double processing (key down and key up)
                use ratatui::crossterm::event::KeyEventKind;
                if matches!(key_event.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                    if self.handle_key_event(key_event) {
                        self.needs_redraw = true;
                    }
                }
            }
            Msg::MouseClick {
                action,
                column,
                row,
                bounds,
            } => {
                if self.handle_mouse_click(action, column, row, &bounds) {
                    self.needs_redraw = true;
                }
            }
            Msg::MouseScrollUp => {
                if self.handle_mouse_scroll(NavigationDirection::Previous) {
                    self.needs_redraw = true;
                }
            }
            Msg::MouseScrollDown => {
                if self.handle_mouse_scroll(NavigationDirection::Next) {
                    self.needs_redraw = true;
                }
            }
            Msg::Tick => {
                // Handle periodic updates (activity simulation, etc.)
                let had_activity_changes = self.update_active_task_activities();

                // Handle autocomplete periodic updates
                self.autocomplete.on_tick();
                self.autocomplete.poll_results();

                if had_activity_changes {
                    self.needs_redraw = true;
                }
            }
            Msg::Quit => {
                // Application is quitting, no state changes needed
            }
        }
        Ok(())
    }

    /// Translate a KeyEvent to a KeyboardOperation by consulting the user's configured key bindings
    fn key_event_to_operation(&self, key: &KeyEvent) -> Option<KeyboardOperation> {
        use crate::settings::*;
        use ratatui::crossterm::event::{KeyCode, KeyModifiers};

        // Special hardcoded handling for Ctrl+N (new draft) - bypass keymap
        if let (KeyCode::Char('n'), mods) = (key.code, key.modifiers) {
            if mods.contains(KeyModifiers::CONTROL) {
                return None; // Let it be handled as character input for new draft
            }
        }

        // Special hardcoded handling for Ctrl+D (duplicate line) - for testing
        if let (KeyCode::Char('d'), mods) = (key.code, key.modifiers) {
            if mods.contains(KeyModifiers::CONTROL) {
                return Some(KeyboardOperation::DuplicateLineSelection);
            }
        }

        // Get the keymap configuration from settings
        let keymap = self.settings.keymap();

        // Check all possible keyboard operations to see if any match this key event
        // This approach allows users to fully customize their key bindings

        // Define all operations we care about in the TUI
        // These are operations that have default key bindings defined
        let operations_to_check = vec![
            KeyboardOperation::MoveToPreviousLine,         // Up arrow
            KeyboardOperation::MoveToNextLine,             // Down arrow
            KeyboardOperation::MoveToNextField,            // Tab
            KeyboardOperation::MoveToPreviousField,        // Shift+Tab
            KeyboardOperation::MoveToBeginningOfLine,      // Home
            KeyboardOperation::MoveToEndOfLine,            // End
            KeyboardOperation::MoveForwardOneCharacter,    // Right arrow
            KeyboardOperation::MoveBackwardOneCharacter,   // Left arrow
            KeyboardOperation::MoveForwardOneWord,         // Ctrl+Right
            KeyboardOperation::MoveBackwardOneWord,        // Ctrl+Left
            KeyboardOperation::MoveToBeginningOfSentence,  // Alt+A
            KeyboardOperation::MoveToEndOfSentence,        // Alt+E
            KeyboardOperation::ScrollDownOneScreen,        // PageDown
            KeyboardOperation::ScrollUpOneScreen,          // PageUp
            KeyboardOperation::RecenterScreenOnCursor,     // Ctrl+L
            KeyboardOperation::MoveToBeginningOfDocument,  // Ctrl+Home
            KeyboardOperation::MoveToEndOfDocument,        // Ctrl+End
            KeyboardOperation::MoveToBeginningOfParagraph, // Alt+{
            KeyboardOperation::MoveToEndOfParagraph,       // Alt+}
            KeyboardOperation::DeleteCharacterBackward,    // Backspace
            KeyboardOperation::DeleteCharacterForward,     // Delete
            KeyboardOperation::DeleteWordForward,          // Ctrl+Delete
            KeyboardOperation::DeleteWordBackward,         // Ctrl+Backspace
            KeyboardOperation::DeleteToEndOfLine,          // Ctrl+K
            KeyboardOperation::DeleteToBeginningOfLine,    // Ctrl+U
            KeyboardOperation::Cut,                        // Ctrl+X
            KeyboardOperation::Copy,                       // Ctrl+C
            KeyboardOperation::Paste,                      // Ctrl+V
            KeyboardOperation::CycleThroughClipboard,      // Alt+Y
            KeyboardOperation::Undo,                       // Ctrl+Z
            KeyboardOperation::Redo,                       // Ctrl+Y
            KeyboardOperation::OpenNewLine,                // Shift+Enter
            KeyboardOperation::TransposeCharacters,        // Ctrl+T
            KeyboardOperation::TransposeWords,             // Alt+T
            KeyboardOperation::UppercaseWord,              // Alt+U
            KeyboardOperation::LowercaseWord,              // Alt+L
            KeyboardOperation::CapitalizeWord,             // Alt+C
            KeyboardOperation::JoinLines,                  // Alt+^
            KeyboardOperation::Bold,                       // Ctrl+B
            KeyboardOperation::Italic,                     // Ctrl+I
            KeyboardOperation::Underline,                  // Ctrl+U
            KeyboardOperation::ToggleComment,              // Ctrl+/
            KeyboardOperation::DuplicateLineSelection,     // Ctrl+D
            KeyboardOperation::MoveLineUp,                 // Alt+Up
            KeyboardOperation::MoveLineDown,               // Alt+Down
            KeyboardOperation::IndentRegion,               // Ctrl+]
            KeyboardOperation::DedentRegion,               // Ctrl+[
            KeyboardOperation::IncrementalSearchForward,   // Ctrl+S
            KeyboardOperation::IncrementalSearchBackward,  // Ctrl+R
            KeyboardOperation::FindNext,                   // F3
            KeyboardOperation::FindPrevious,               // Shift+F3
            KeyboardOperation::SelectAll,                  // Ctrl+A
            KeyboardOperation::SelectWordUnderCursor,      // Alt+@
            KeyboardOperation::SetMark,                    // Ctrl+Space
            KeyboardOperation::DismissOverlay,             // Escape
        ];

        // Find the first operation that matches this key event
        for operation in operations_to_check {
            if keymap.matches(operation, key) {
                return Some(operation);
            }
        }

        // No configured operation matched
        None
    }

    /// Handle keyboard events by translating to KeyboardOperation and dispatching
    pub fn handle_key_event(&mut self, key: KeyEvent) -> bool {
        use ratatui::crossterm::event::{KeyCode, KeyModifiers};

        if !matches!(key.code, KeyCode::Esc) {
            self.clear_exit_confirmation();
        }

        // Special handling for Ctrl+N (new draft) - check before keymap lookup
        if let (KeyCode::Char('n'), mods) = (key.code, key.modifiers) {
            if mods.contains(KeyModifiers::CONTROL) {
                return self.handle_ctrl_n();
            }
        }

        // Handle autocomplete keys when focused on TaskDescription
        if matches!(self.focus_element, FocusElement::TaskDescription) {
            if let Some(card) = self.draft_cards.get_mut(0) {
                match self.autocomplete.handle_key_event(
                    &key,
                    &mut card.description,
                    &mut self.needs_redraw,
                ) {
                    AutocompleteKeyResult::Consumed { text_changed } => {
                        if text_changed {
                            self.autocomplete.notify_text_input();
                            self.autocomplete
                                .after_textarea_change(&card.description, &mut self.needs_redraw);
                        }
                        return true;
                    }
                    AutocompleteKeyResult::Ignored => {
                        // Continue with normal key handling
                    }
                }
            }
        }

        // First try to translate the key event to a keyboard operation
        if let Some(operation) = self.key_event_to_operation(&key) {
            tracing::trace!("key_event_to_operation: {:?} -> {:?}", key, operation);
            return self.handle_keyboard_operation(operation, &key);
        } else {
            tracing::trace!("key_event_to_operation: {:?} -> None", key);
        }

        // Handle special key codes directly
        match key.code {
            KeyCode::Enter => {
                return self.handle_enter(
                    key.modifiers.contains(ratatui::crossterm::event::KeyModifiers::SHIFT),
                );
            }
            _ => {}
        }

        // Handle character input directly if it's not a recognized operation
        if let KeyCode::Char(ch) = key.code {
            return self.handle_char_input(ch);
        }

        // If no operation matched and it's not character input, the key is not handled
        false
    }

    pub fn take_exit_request(&mut self) -> bool {
        if self.exit_requested {
            self.exit_requested = false;
            return true;
        }
        false
    }

    /// Handle a KeyboardOperation with the original KeyEvent context
    pub fn handle_keyboard_operation(
        &mut self,
        operation: KeyboardOperation,
        key: &KeyEvent,
    ) -> bool {
        // Any keyboard operation (except ESC) should clear exit confirmation
        if operation != KeyboardOperation::DismissOverlay {
            self.clear_exit_confirmation();
        }

        match operation {
            KeyboardOperation::MoveToNextField => {
                if self.handle_overlay_navigation(NavigationDirection::Next) {
                    return true;
                }
                // Tab key: move between controls within current focus element
                self.focus_next_control()
            }
            KeyboardOperation::MoveToPreviousField => {
                if self.handle_overlay_navigation(NavigationDirection::Previous) {
                    return true;
                }
                // Shift+Tab key: move backward between controls within current focus element
                self.focus_previous_control()
            }
            KeyboardOperation::MoveToBeginningOfLine => {
                // Home key: move cursor to beginning of line in text area
                if let FocusElement::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == FocusElement::TaskDescription {
                            use tui_textarea::CursorMove;
                            let before = card.description.cursor();

                            // Handle shift+home selection (CUA style)
                            let shift_pressed = key
                                .modifiers
                                .contains(ratatui::crossterm::event::KeyModifiers::SHIFT);
                            if shift_pressed {
                                // Start selection if not already active
                                if card.description.selection_range().is_none() {
                                    card.description.start_selection();
                                }
                            } else {
                                // Clear any existing selection when moving without shift
                                if card.description.selection_range().is_some() {
                                    card.description.cancel_selection();
                                }
                            }

                            card.description.move_cursor(CursorMove::Head);
                            if card.description.cursor() != before {
                                self.autocomplete.after_textarea_change(
                                    &card.description,
                                    &mut self.needs_redraw,
                                );
                            }
                            return true;
                        }
                    }
                }
                false
            }
            KeyboardOperation::MoveToEndOfLine => {
                // End key: move cursor to end of line in text area
                if let FocusElement::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == FocusElement::TaskDescription {
                            use tui_textarea::CursorMove;
                            let before = card.description.cursor();

                            // Handle shift+end selection (CUA style)
                            let shift_pressed = key
                                .modifiers
                                .contains(ratatui::crossterm::event::KeyModifiers::SHIFT);
                            if shift_pressed {
                                // Start selection if not already active
                                if card.description.selection_range().is_none() {
                                    card.description.start_selection();
                                }
                            } else {
                                // Clear any existing selection when moving without shift
                                if card.description.selection_range().is_some() {
                                    card.description.cancel_selection();
                                }
                            }

                            card.description.move_cursor(CursorMove::End);
                            if card.description.cursor() != before {
                                self.autocomplete.after_textarea_change(
                                    &card.description,
                                    &mut self.needs_redraw,
                                );
                            }
                            return true;
                        }
                    }
                }
                false
            }
            KeyboardOperation::MoveForwardOneCharacter => {
                // Right arrow: handle filter navigation or move cursor forward in text area

                // Handle filter navigation
                match self.focus_element {
                    FocusElement::FilterBarLine => {
                        // Move from separator line to first filter control
                        self.focus_element = FocusElement::Filter(FilterControl::Repository);
                        return true;
                    }
                    FocusElement::Filter(control) => {
                        // Navigate between filter controls
                        let next = match control {
                            FilterControl::Repository => FilterControl::Status,
                            FilterControl::Status => FilterControl::Creator,
                            FilterControl::Creator => FilterControl::Repository, // Wrap around
                        };
                        self.focus_element = FocusElement::Filter(next);
                        return true;
                    }
                    _ => {}
                }

                // Default: move cursor forward one character in text area
                if let FocusElement::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == FocusElement::TaskDescription {
                            use tui_textarea::CursorMove;
                            let before = card.description.cursor();

                            // Handle shift+arrow selection (CUA style)
                            let shift_pressed = key
                                .modifiers
                                .contains(ratatui::crossterm::event::KeyModifiers::SHIFT);
                            if shift_pressed {
                                // Start selection if not already active
                                if card.description.selection_range().is_none() {
                                    card.description.start_selection();
                                }
                            } else {
                                // Clear any existing selection when moving without shift
                                if card.description.selection_range().is_some() {
                                    card.description.cancel_selection();
                                }
                            }

                            card.description.move_cursor(CursorMove::Forward);

                            if card.description.cursor() != before {
                                self.autocomplete.after_textarea_change(
                                    &card.description,
                                    &mut self.needs_redraw,
                                );
                            }
                            return true;
                        }
                    }
                }
                false
            }
            KeyboardOperation::MoveBackwardOneCharacter => {
                // Left arrow: handle filter navigation or move cursor backward in text area

                // Handle filter navigation
                match self.focus_element {
                    FocusElement::FilterBarLine => {
                        // Move from separator line to first filter control
                        self.focus_element = FocusElement::Filter(FilterControl::Repository);
                        return true;
                    }
                    FocusElement::Filter(control) => {
                        // Navigate backwards through filter controls
                        let next = match control {
                            FilterControl::Repository => FilterControl::Creator, // Wrap backwards
                            FilterControl::Status => FilterControl::Repository,
                            FilterControl::Creator => FilterControl::Status,
                        };
                        self.focus_element = FocusElement::Filter(next);
                        return true;
                    }
                    _ => {}
                }

                // Default: move cursor backward one character in text area
                if let FocusElement::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == FocusElement::TaskDescription {
                            use tui_textarea::CursorMove;
                            let before = card.description.cursor();

                            // Handle shift+arrow selection (CUA style)
                            let shift_pressed = key
                                .modifiers
                                .contains(ratatui::crossterm::event::KeyModifiers::SHIFT);
                            if shift_pressed {
                                // Start selection if not already active
                                if card.description.selection_range().is_none() {
                                    card.description.start_selection();
                                }
                            } else {
                                // Clear any existing selection when moving without shift
                                if card.description.selection_range().is_some() {
                                    card.description.cancel_selection();
                                }
                            }

                            card.description.move_cursor(CursorMove::Back);

                            if card.description.cursor() != before {
                                self.autocomplete.after_textarea_change(
                                    &card.description,
                                    &mut self.needs_redraw,
                                );
                            }
                            return true;
                        }
                    }
                }
                false
            }
            KeyboardOperation::MoveForwardOneWord => {
                // Ctrl+Right: move cursor forward one word in text area
                if let FocusElement::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == FocusElement::TaskDescription {
                            use tui_textarea::CursorMove;
                            let before = card.description.cursor();

                            // Handle shift+ctrl+arrow selection (CUA style)
                            let shift_pressed = key
                                .modifiers
                                .contains(ratatui::crossterm::event::KeyModifiers::SHIFT);
                            if shift_pressed {
                                // Start selection if not already active
                                if card.description.selection_range().is_none() {
                                    card.description.start_selection();
                                }
                            } else {
                                // Clear any existing selection when moving without shift
                                if card.description.selection_range().is_some() {
                                    card.description.cancel_selection();
                                }
                            }

                            card.description.move_cursor(CursorMove::WordForward);

                            if card.description.cursor() != before {
                                self.autocomplete.after_textarea_change(
                                    &card.description,
                                    &mut self.needs_redraw,
                                );
                            }
                            return true;
                        }
                    }
                }
                false
            }
            KeyboardOperation::MoveBackwardOneWord => {
                // Ctrl+Left: move cursor backward one word in text area
                if let FocusElement::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == FocusElement::TaskDescription {
                            use tui_textarea::CursorMove;
                            let before = card.description.cursor();

                            // Handle shift+ctrl+arrow selection (CUA style)
                            let shift_pressed = key
                                .modifiers
                                .contains(ratatui::crossterm::event::KeyModifiers::SHIFT);
                            if shift_pressed {
                                // Start selection if not already active
                                if card.description.selection_range().is_none() {
                                    card.description.start_selection();
                                }
                            } else {
                                // Clear any existing selection when moving without shift
                                if card.description.selection_range().is_some() {
                                    card.description.cancel_selection();
                                }
                            }

                            card.description.move_cursor(CursorMove::WordBack);

                            if card.description.cursor() != before {
                                self.autocomplete.after_textarea_change(
                                    &card.description,
                                    &mut self.needs_redraw,
                                );
                            }
                            return true;
                        }
                    }
                }
                false
            }
            KeyboardOperation::DeleteWordForward => {
                // Ctrl+Delete: delete word forward
                match self.focus_element {
                    FocusElement::TaskDescription => {
                        // Get the first (and currently only) draft card
                        if let Some(card) = self.draft_cards.get_mut(0) {
                            // Use tui-textarea's built-in word deletion method
                            let before_text = card.description.lines().join("\\n");
                            card.description.delete_next_word();
                            let after_text = card.description.lines().join("\\n");
                            if before_text != after_text {
                                self.autocomplete.after_textarea_change(
                                    &card.description,
                                    &mut self.needs_redraw,
                                );
                            }
                            return true;
                        }
                    }
                    FocusElement::DraftTask(idx) => {
                        if let Some(card) = self.draft_cards.get_mut(idx) {
                            if card.focus_element == FocusElement::TaskDescription {
                                // Use tui-textarea's built-in word deletion method
                                let before_text = card.description.lines().join("\\n");
                                card.description.delete_next_word();
                                let after_text = card.description.lines().join("\\n");
                                if before_text != after_text {
                                    self.autocomplete.after_textarea_change(
                                        &card.description,
                                        &mut self.needs_redraw,
                                    );
                                }
                                return true;
                            }
                        }
                    }
                    _ => {}
                }
                false
            }
            KeyboardOperation::DeleteWordBackward => {
                // Ctrl+Backspace: delete word backward
                match self.focus_element {
                    FocusElement::TaskDescription => {
                        // Get the first (and currently only) draft card
                        if let Some(card) = self.draft_cards.get_mut(0) {
                            // Use tui-textarea's built-in word deletion method
                            let before_text = card.description.lines().join("\\n");
                            card.description.delete_word();
                            let after_text = card.description.lines().join("\\n");
                            if before_text != after_text {
                                self.autocomplete.after_textarea_change(
                                    &card.description,
                                    &mut self.needs_redraw,
                                );
                            }
                            return true;
                        }
                    }
                    FocusElement::DraftTask(idx) => {
                        if let Some(card) = self.draft_cards.get_mut(idx) {
                            if card.focus_element == FocusElement::TaskDescription {
                                // Use tui-textarea's built-in word deletion method
                                let before_text = card.description.lines().join("\\n");
                                card.description.delete_word();
                                let after_text = card.description.lines().join("\\n");
                                if before_text != after_text {
                                    self.autocomplete.after_textarea_change(
                                        &card.description,
                                        &mut self.needs_redraw,
                                    );
                                }
                                return true;
                            }
                        }
                    }
                    _ => {}
                }
                false
            }
            KeyboardOperation::MoveToPreviousLine => {
                if self.handle_overlay_navigation(NavigationDirection::Previous) {
                    return true;
                }
                match self.focus_element {
                    FocusElement::DraftTask(idx) => {
                        // First try to move cursor up in the draft card's text area
                        if let Some(card) = self.draft_cards.get_mut(idx) {
                            use tui_textarea::CursorMove;
                            let old_cursor = card.description.cursor();

                            // Handle shift+arrow selection (CUA style)
                            let shift_pressed = key
                                .modifiers
                                .contains(ratatui::crossterm::event::KeyModifiers::SHIFT);
                            if shift_pressed {
                                // Start selection if not already active
                                if card.description.selection_range().is_none() {
                                    card.description.start_selection();
                                }
                            } else {
                                // Clear any existing selection when moving without shift
                                if card.description.selection_range().is_some() {
                                    card.description.cancel_selection();
                                }
                            }

                            card.description.move_cursor(CursorMove::Up);
                            let new_cursor = card.description.cursor();
                            if new_cursor != old_cursor {
                                // Cursor moved successfully within text area
                                self.autocomplete.after_textarea_change(
                                    &card.description,
                                    &mut self.needs_redraw,
                                );
                                return true;
                            }
                        }
                        // Cursor can't move up, navigate to settings button
                        self.navigate_up_hierarchy()
                    }
                    _ => self.navigate_up_hierarchy(),
                }
            }
            KeyboardOperation::MoveToNextLine => {
                if self.handle_overlay_navigation(NavigationDirection::Next) {
                    return true;
                }
                match self.focus_element {
                    FocusElement::DraftTask(idx) => {
                        // First try to move cursor down in the draft card's text area
                        if let Some(card) = self.draft_cards.get_mut(idx) {
                            use tui_textarea::CursorMove;
                            let old_cursor = card.description.cursor();

                            // Handle shift+arrow selection (CUA style)
                            let shift_pressed = key
                                .modifiers
                                .contains(ratatui::crossterm::event::KeyModifiers::SHIFT);
                            if shift_pressed {
                                // Start selection if not already active
                                if card.description.selection_range().is_none() {
                                    card.description.start_selection();
                                }
                            } else {
                                // Clear any existing selection when moving without shift
                                if card.description.selection_range().is_some() {
                                    card.description.cancel_selection();
                                }
                            }

                            card.description.move_cursor(CursorMove::Down);
                            let new_cursor = card.description.cursor();
                            if new_cursor != old_cursor {
                                // Cursor moved successfully within text area
                                self.autocomplete.after_textarea_change(
                                    &card.description,
                                    &mut self.needs_redraw,
                                );
                                return true;
                            }
                        }
                        // Cursor can't move down, navigate down hierarchy
                        self.navigate_down_hierarchy()
                    }
                    _ => self.navigate_down_hierarchy(),
                }
            }
            KeyboardOperation::DeleteCharacterBackward => {
                // Backspace
                self.handle_backspace()
            }
            KeyboardOperation::DeleteCharacterForward => {
                // Delete key
                self.handle_delete()
            }
            KeyboardOperation::OpenNewLine => {
                // Shift+Enter
                self.handle_enter(true)
            }
            KeyboardOperation::DismissOverlay => self.handle_dismiss_overlay(),
            KeyboardOperation::Cut => {
                // Cut selected text
                match self.focus_element {
                    FocusElement::TaskDescription => {
                        if let Some(card) = self.draft_cards.get_mut(0) {
                            let before_text = card.description.lines().join("\\n");
                            card.description.cut();
                            let after_text = card.description.lines().join("\\n");
                            if before_text != after_text {
                                self.autocomplete.after_textarea_change(
                                    &card.description,
                                    &mut self.needs_redraw,
                                );
                            }
                            return true;
                        }
                    }
                    FocusElement::DraftTask(idx) => {
                        if let Some(card) = self.draft_cards.get_mut(idx) {
                            if card.focus_element == FocusElement::TaskDescription {
                                let before_text = card.description.lines().join("\\n");
                                card.description.cut();
                                let after_text = card.description.lines().join("\\n");
                                if before_text != after_text {
                                    self.autocomplete.after_textarea_change(
                                        &card.description,
                                        &mut self.needs_redraw,
                                    );
                                }
                                return true;
                            }
                        }
                    }
                    _ => {}
                }
                false
            }
            KeyboardOperation::Copy => {
                // Copy selected text
                match self.focus_element {
                    FocusElement::TaskDescription => {
                        if let Some(card) = self.draft_cards.get_mut(0) {
                            card.description.copy();
                            return true;
                        }
                    }
                    FocusElement::DraftTask(idx) => {
                        if let Some(card) = self.draft_cards.get_mut(idx) {
                            if card.focus_element == FocusElement::TaskDescription {
                                card.description.copy();
                                return true;
                            }
                        }
                    }
                    _ => {}
                }
                false
            }
            KeyboardOperation::Paste => {
                // Paste from clipboard
                match self.focus_element {
                    FocusElement::TaskDescription => {
                        if let Some(card) = self.draft_cards.get_mut(0) {
                            let before_text = card.description.lines().join("\\n");
                            card.description.paste();
                            let after_text = card.description.lines().join("\\n");
                            if before_text != after_text {
                                self.autocomplete.after_textarea_change(
                                    &card.description,
                                    &mut self.needs_redraw,
                                );
                            }
                            return true;
                        }
                    }
                    FocusElement::DraftTask(idx) => {
                        if let Some(card) = self.draft_cards.get_mut(idx) {
                            if card.focus_element == FocusElement::TaskDescription {
                                let before_text = card.description.lines().join("\\n");
                                card.description.paste();
                                let after_text = card.description.lines().join("\\n");
                                if before_text != after_text {
                                    self.autocomplete.after_textarea_change(
                                        &card.description,
                                        &mut self.needs_redraw,
                                    );
                                }
                                return true;
                            }
                        }
                    }
                    _ => {}
                }
                false
            }
            KeyboardOperation::Undo => {
                // Undo last operation
                match self.focus_element {
                    FocusElement::TaskDescription => {
                        if let Some(card) = self.draft_cards.get_mut(0) {
                            let before_text = card.description.lines().join("\\n");
                            card.description.undo();
                            let after_text = card.description.lines().join("\\n");
                            if before_text != after_text {
                                self.autocomplete.after_textarea_change(
                                    &card.description,
                                    &mut self.needs_redraw,
                                );
                            }
                            return true;
                        }
                    }
                    FocusElement::DraftTask(idx) => {
                        if let Some(card) = self.draft_cards.get_mut(idx) {
                            if card.focus_element == FocusElement::TaskDescription {
                                let before_text = card.description.lines().join("\\n");
                                card.description.undo();
                                let after_text = card.description.lines().join("\\n");
                                if before_text != after_text {
                                    self.autocomplete.after_textarea_change(
                                        &card.description,
                                        &mut self.needs_redraw,
                                    );
                                }
                                return true;
                            }
                        }
                    }
                    _ => {}
                }
                false
            }
            KeyboardOperation::Redo => {
                // Redo last operation
                match self.focus_element {
                    FocusElement::TaskDescription => {
                        if let Some(card) = self.draft_cards.get_mut(0) {
                            let before_text = card.description.lines().join("\\n");
                            card.description.redo();
                            let after_text = card.description.lines().join("\\n");
                            if before_text != after_text {
                                self.autocomplete.after_textarea_change(
                                    &card.description,
                                    &mut self.needs_redraw,
                                );
                            }
                            return true;
                        }
                    }
                    FocusElement::DraftTask(idx) => {
                        if let Some(card) = self.draft_cards.get_mut(idx) {
                            if card.focus_element == FocusElement::TaskDescription {
                                let before_text = card.description.lines().join("\\n");
                                card.description.redo();
                                let after_text = card.description.lines().join("\\n");
                                if before_text != after_text {
                                    self.autocomplete.after_textarea_change(
                                        &card.description,
                                        &mut self.needs_redraw,
                                    );
                                }
                                return true;
                            }
                        }
                    }
                    _ => {}
                }
                false
            }
            KeyboardOperation::DeleteToEndOfLine => {
                // Delete from cursor to end of line
                match self.focus_element {
                    FocusElement::TaskDescription => {
                        if let Some(card) = self.draft_cards.get_mut(0) {
                            let before_text = card.description.lines().join("\\n");
                            card.description.delete_line_by_end();
                            let after_text = card.description.lines().join("\\n");
                            if before_text != after_text {
                                self.autocomplete.after_textarea_change(
                                    &card.description,
                                    &mut self.needs_redraw,
                                );
                            }
                            return true;
                        }
                    }
                    FocusElement::DraftTask(idx) => {
                        if let Some(card) = self.draft_cards.get_mut(idx) {
                            if card.focus_element == FocusElement::TaskDescription {
                                let before_text = card.description.lines().join("\\n");
                                card.description.delete_line_by_end();
                                let after_text = card.description.lines().join("\\n");
                                if before_text != after_text {
                                    self.autocomplete.after_textarea_change(
                                        &card.description,
                                        &mut self.needs_redraw,
                                    );
                                }
                                return true;
                            }
                        }
                    }
                    _ => {}
                }
                false
            }
            KeyboardOperation::DeleteToBeginningOfLine => {
                // Delete from cursor to beginning of line
                match self.focus_element {
                    FocusElement::TaskDescription => {
                        if let Some(card) = self.draft_cards.get_mut(0) {
                            let before_text = card.description.lines().join("\\n");
                            card.description.delete_line_by_head();
                            let after_text = card.description.lines().join("\\n");
                            if before_text != after_text {
                                self.autocomplete.after_textarea_change(
                                    &card.description,
                                    &mut self.needs_redraw,
                                );
                            }
                            return true;
                        }
                    }
                    FocusElement::DraftTask(idx) => {
                        if let Some(card) = self.draft_cards.get_mut(idx) {
                            if card.focus_element == FocusElement::TaskDescription {
                                let before_text = card.description.lines().join("\\n");
                                card.description.delete_line_by_head();
                                let after_text = card.description.lines().join("\\n");
                                if before_text != after_text {
                                    self.autocomplete.after_textarea_change(
                                        &card.description,
                                        &mut self.needs_redraw,
                                    );
                                }
                                return true;
                            }
                        }
                    }
                    _ => {}
                }
                false
            }
            KeyboardOperation::SelectAll => {
                // Select all text
                match self.focus_element {
                    FocusElement::TaskDescription => {
                        if let Some(card) = self.draft_cards.get_mut(0) {
                            card.description.select_all();
                            self.autocomplete
                                .after_textarea_change(&card.description, &mut self.needs_redraw);
                            return true;
                        }
                    }
                    FocusElement::DraftTask(idx) => {
                        if let Some(card) = self.draft_cards.get_mut(idx) {
                            if card.focus_element == FocusElement::TaskDescription {
                                card.description.select_all();
                                self.autocomplete.after_textarea_change(
                                    &card.description,
                                    &mut self.needs_redraw,
                                );
                                return true;
                            }
                        }
                    }
                    _ => {}
                }
                false
            }
            KeyboardOperation::MoveToBeginningOfSentence => {
                // Move to beginning of sentence (approximated as beginning of line)
                if let FocusElement::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == FocusElement::TaskDescription {
                            use tui_textarea::CursorMove;
                            let before = card.description.cursor();

                            // Handle shift+sentence selection (CUA style)
                            let shift_pressed = key
                                .modifiers
                                .contains(ratatui::crossterm::event::KeyModifiers::SHIFT);
                            if shift_pressed {
                                // Start selection if not already active
                                if card.description.selection_range().is_none() {
                                    card.description.start_selection();
                                }
                            } else {
                                // Clear any existing selection when moving without shift
                                if card.description.selection_range().is_some() {
                                    card.description.cancel_selection();
                                }
                            }

                            card.description.move_cursor(CursorMove::Head);
                            if card.description.cursor() != before {
                                self.autocomplete.after_textarea_change(
                                    &card.description,
                                    &mut self.needs_redraw,
                                );
                            }
                            return true;
                        }
                    }
                }
                false
            }
            KeyboardOperation::MoveToEndOfSentence => {
                // Move to end of sentence (approximated as end of line)
                if let FocusElement::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == FocusElement::TaskDescription {
                            use tui_textarea::CursorMove;
                            let before = card.description.cursor();

                            // Handle shift+sentence selection (CUA style)
                            let shift_pressed = key
                                .modifiers
                                .contains(ratatui::crossterm::event::KeyModifiers::SHIFT);
                            if shift_pressed {
                                // Start selection if not already active
                                if card.description.selection_range().is_none() {
                                    card.description.start_selection();
                                }
                            } else {
                                // Clear any existing selection when moving without shift
                                if card.description.selection_range().is_some() {
                                    card.description.cancel_selection();
                                }
                            }

                            card.description.move_cursor(CursorMove::End);
                            if card.description.cursor() != before {
                                self.autocomplete.after_textarea_change(
                                    &card.description,
                                    &mut self.needs_redraw,
                                );
                            }
                            return true;
                        }
                    }
                }
                false
            }
            KeyboardOperation::MoveToBeginningOfDocument => {
                // Move to beginning of document (first line, first character)
                if let FocusElement::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == FocusElement::TaskDescription {
                            use tui_textarea::CursorMove;
                            let before = card.description.cursor();

                            // Handle shift+document selection (CUA style)
                            let shift_pressed = key
                                .modifiers
                                .contains(ratatui::crossterm::event::KeyModifiers::SHIFT);
                            if shift_pressed {
                                // Start selection if not already active
                                if card.description.selection_range().is_none() {
                                    card.description.start_selection();
                                }
                            } else {
                                // Clear any existing selection when moving without shift
                                if card.description.selection_range().is_some() {
                                    card.description.cancel_selection();
                                }
                            }

                            // Move to first line, then to beginning of that line
                            let mut prev_cursor = card.description.cursor();
                            loop {
                                card.description.move_cursor(CursorMove::Up);
                                let new_cursor = card.description.cursor();
                                if new_cursor == prev_cursor {
                                    break; // Can't move further up
                                }
                                prev_cursor = new_cursor;
                            }
                            card.description.move_cursor(CursorMove::Head);

                            if card.description.cursor() != before {
                                self.autocomplete.after_textarea_change(
                                    &card.description,
                                    &mut self.needs_redraw,
                                );
                            }
                            return true;
                        }
                    }
                }
                false
            }
            KeyboardOperation::MoveToEndOfDocument => {
                // Move to end of document (last line, last character)
                if let FocusElement::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == FocusElement::TaskDescription {
                            use tui_textarea::CursorMove;
                            let before = card.description.cursor();

                            // Handle shift+document selection (CUA style)
                            let shift_pressed = key
                                .modifiers
                                .contains(ratatui::crossterm::event::KeyModifiers::SHIFT);
                            if shift_pressed {
                                // Start selection if not already active
                                if card.description.selection_range().is_none() {
                                    card.description.start_selection();
                                }
                            } else {
                                // Clear any existing selection when moving without shift
                                if card.description.selection_range().is_some() {
                                    card.description.cancel_selection();
                                }
                            }

                            // Move to last line, then to end of that line
                            let mut prev_cursor = card.description.cursor();
                            loop {
                                card.description.move_cursor(CursorMove::Down);
                                let new_cursor = card.description.cursor();
                                if new_cursor == prev_cursor {
                                    break; // Can't move further down
                                }
                                prev_cursor = new_cursor;
                            }
                            card.description.move_cursor(CursorMove::End);

                            if card.description.cursor() != before {
                                self.autocomplete.after_textarea_change(
                                    &card.description,
                                    &mut self.needs_redraw,
                                );
                            }
                            return true;
                        }
                    }
                }
                false
            }
            KeyboardOperation::MoveToBeginningOfParagraph => {
                // Move to beginning of paragraph (approximated as beginning of current line)
                if let FocusElement::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == FocusElement::TaskDescription {
                            use tui_textarea::CursorMove;
                            let before = card.description.cursor();

                            // Handle shift+paragraph selection (CUA style)
                            let shift_pressed = key
                                .modifiers
                                .contains(ratatui::crossterm::event::KeyModifiers::SHIFT);
                            if shift_pressed {
                                // Start selection if not already active
                                if card.description.selection_range().is_none() {
                                    card.description.start_selection();
                                }
                            } else {
                                // Clear any existing selection when moving without shift
                                if card.description.selection_range().is_some() {
                                    card.description.cancel_selection();
                                }
                            }

                            card.description.move_cursor(CursorMove::Head);
                            if card.description.cursor() != before {
                                self.autocomplete.after_textarea_change(
                                    &card.description,
                                    &mut self.needs_redraw,
                                );
                            }
                            return true;
                        }
                    }
                }
                false
            }
            KeyboardOperation::MoveToEndOfParagraph => {
                // Move to end of paragraph (approximated as end of current line)
                if let FocusElement::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == FocusElement::TaskDescription {
                            use tui_textarea::CursorMove;
                            let before = card.description.cursor();

                            // Handle shift+paragraph selection (CUA style)
                            let shift_pressed = key
                                .modifiers
                                .contains(ratatui::crossterm::event::KeyModifiers::SHIFT);
                            if shift_pressed {
                                // Start selection if not already active
                                if card.description.selection_range().is_none() {
                                    card.description.start_selection();
                                }
                            } else {
                                // Clear any existing selection when moving without shift
                                if card.description.selection_range().is_some() {
                                    card.description.cancel_selection();
                                }
                            }

                            card.description.move_cursor(CursorMove::End);
                            if card.description.cursor() != before {
                                self.autocomplete.after_textarea_change(
                                    &card.description,
                                    &mut self.needs_redraw,
                                );
                            }
                            return true;
                        }
                    }
                }
                false
            }
            KeyboardOperation::SelectWordUnderCursor => {
                // Select word under cursor
                if let FocusElement::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == FocusElement::TaskDescription {
                            // For now, just select all as a simple approximation
                            // A more sophisticated implementation would find word boundaries
                            card.description.select_all();
                            self.autocomplete
                                .after_textarea_change(&card.description, &mut self.needs_redraw);
                            return true;
                        }
                    }
                }
                false
            }
            KeyboardOperation::SetMark => {
                // Set mark for selection (CUA style selection start)
                if let FocusElement::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == FocusElement::TaskDescription {
                            // Start selection at current cursor position
                            card.description.start_selection();
                            return true;
                        }
                    }
                }
                false
            }
            KeyboardOperation::ScrollDownOneScreen => {
                // Scroll viewport down one screen (PageDown)
                if let FocusElement::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == FocusElement::TaskDescription {
                            use tui_textarea::Scrolling;
                            card.description.scroll(Scrolling::PageDown);
                            return true;
                        }
                    }
                }
                false
            }
            KeyboardOperation::ScrollUpOneScreen => {
                // Scroll viewport up one screen (PageUp)
                if let FocusElement::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == FocusElement::TaskDescription {
                            use tui_textarea::Scrolling;
                            card.description.scroll(Scrolling::PageUp);
                            return true;
                        }
                    }
                }
                false
            }
            KeyboardOperation::RecenterScreenOnCursor => {
                // Recenter cursor in middle of screen (Ctrl+L)
                if let FocusElement::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == FocusElement::TaskDescription {
                            // Get current cursor line and viewport height
                            let cursor = card.description.cursor();
                            let lines = card.description.lines();
                            let viewport_height = card.description.viewport_origin().1 as usize; // Approximation

                            // Calculate target top line to center cursor
                            let target_top = cursor.0.saturating_sub(viewport_height / 2);

                            // Scroll to center cursor
                            card.description.scroll((target_top as i16, 0));
                            return true;
                        }
                    }
                }
                false
            }
            KeyboardOperation::ToggleComment => {
                // Toggle comment (Ctrl+/) - add/remove comment markers from lines
                if let FocusElement::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == FocusElement::TaskDescription {
                            let lines = card.description.lines();
                            let cursor_row = card.description.cursor().0 as usize;
                            let cursor_col = card.description.cursor().1 as usize;

                            // Determine lines to comment/uncomment
                            let (_start_line, _end_line) =
                                if let Some(range) = card.description.selection_range() {
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
                                        line.strip_prefix(comment_marker)
                                            .unwrap_or(line)
                                            .to_string()
                                    } else {
                                        line.clone()
                                    };
                                    lines_to_modify.push(modified_line);
                                }
                            }

                            // Replace the lines in textarea
                            // This is a simplified approach - in practice you'd need to handle this more carefully
                            // For now, we'll just implement a basic version
                            return true; // Placeholder - full implementation would modify textarea content
                        }
                    }
                }
                false
            }
            KeyboardOperation::DuplicateLineSelection => {
                // Duplicate line/selection (Ctrl+D) - copy and paste below
                if let FocusElement::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == FocusElement::TaskDescription {
                            let (cursor_row, _) = card.description.cursor();
                            let lines = card.description.lines();

                            if cursor_row < lines.len() {
                                let current_line = lines[cursor_row].clone();

                                // Move to end of current line and insert newline + duplicated content
                                card.description.move_cursor(tui_textarea::CursorMove::End);
                                card.description.insert_char('\n');
                                card.description.insert_str(&current_line);
                            }

                            self.autocomplete
                                .after_textarea_change(&card.description, &mut self.needs_redraw);
                            return true;
                        }
                    }
                }
                false
            }
            KeyboardOperation::MoveLineUp => {
                // Move line up (Alt+↑) - cut and reinsert above previous line
                if let FocusElement::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == FocusElement::TaskDescription {
                            let cursor_row = card.description.cursor().0 as usize;
                            let lines = card.description.lines();

                            // Can't move first line up
                            if cursor_row == 0 {
                                return false;
                            }

                            // Select current line (simplified - would need proper line selection)
                            card.description.move_cursor(tui_textarea::CursorMove::Head);
                            card.description.start_selection();
                            card.description.move_cursor(tui_textarea::CursorMove::End);
                            // Note: This doesn't include the newline - simplified implementation

                            // Cut the line
                            card.description.cut();

                            // Move cursor up to previous line
                            card.description.move_cursor(tui_textarea::CursorMove::Up);
                            card.description.move_cursor(tui_textarea::CursorMove::Head);

                            // Paste above the current line
                            card.description.paste();

                            self.autocomplete
                                .after_textarea_change(&card.description, &mut self.needs_redraw);
                            return true;
                        }
                    }
                }
                false
            }
            KeyboardOperation::MoveLineDown => {
                // Move line down (Alt+↓) - cut and reinsert below next line
                if let FocusElement::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == FocusElement::TaskDescription {
                            let cursor_row = card.description.cursor().0 as usize;
                            let lines = card.description.lines();

                            // Can't move last line down
                            if cursor_row >= lines.len().saturating_sub(1) {
                                return false;
                            }

                            // Select current line (simplified)
                            card.description.move_cursor(tui_textarea::CursorMove::Head);
                            card.description.start_selection();
                            card.description.move_cursor(tui_textarea::CursorMove::End);

                            // Cut the line
                            card.description.cut();

                            // Move cursor down to next line
                            card.description.move_cursor(tui_textarea::CursorMove::Down);
                            card.description.move_cursor(tui_textarea::CursorMove::End);

                            // Insert newline and paste
                            card.description.insert_newline();
                            card.description.paste();

                            self.autocomplete
                                .after_textarea_change(&card.description, &mut self.needs_redraw);
                            return true;
                        }
                    }
                }
                false
            }
            KeyboardOperation::IndentRegion => {
                // Indent region (Ctrl+]) - insert spaces at start of selected lines
                if let FocusElement::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == FocusElement::TaskDescription {
                            // Get selection range or current line
                            let (_start_line, _end_line) =
                                if let Some(range) = card.description.selection_range() {
                                    (range.0.0, range.1.0)
                                } else {
                                    let cursor_row = card.description.cursor().0 as usize;
                                    (cursor_row, cursor_row)
                                };

                            // Insert 4 spaces (or tab) at start of each line
                            // This is simplified - full implementation would need to modify textarea content directly
                            return true; // Placeholder
                        }
                    }
                }
                false
            }
            KeyboardOperation::DedentRegion => {
                // Dedent region (Ctrl+[) - remove spaces from start of selected lines
                if let FocusElement::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == FocusElement::TaskDescription {
                            // Get selection range or current line
                            let (_start_line, _end_line) =
                                if let Some(range) = card.description.selection_range() {
                                    (range.0.0, range.1.0)
                                } else {
                                    let cursor_row = card.description.cursor().0 as usize;
                                    (cursor_row, cursor_row)
                                };

                            // Remove up to 4 spaces from start of each line
                            // This is simplified - full implementation would need to modify textarea content directly
                            return true; // Placeholder
                        }
                    }
                }
                false
            }
            KeyboardOperation::UppercaseWord => {
                // Uppercase word (Alt+U) - transform word at/after cursor to uppercase
                if let FocusElement::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == FocusElement::TaskDescription {
                            // Get current line and cursor position
                            let lines = card.description.lines();
                            let (cursor_row, cursor_col) = card.description.cursor();

                            if cursor_row < lines.len() {
                                let current_line = &lines[cursor_row];
                                let chars: Vec<char> = current_line.chars().collect();

                                if cursor_col < chars.len() {
                                    // Find word boundaries around cursor
                                    let mut word_start = cursor_col;
                                    let mut word_end = cursor_col;

                                    // Find start of word (move left until non-alphanumeric)
                                    while word_start > 0 && chars[word_start - 1].is_alphanumeric()
                                    {
                                        word_start -= 1;
                                    }

                                    // Find end of word (move right until non-alphanumeric)
                                    while word_end < chars.len()
                                        && chars[word_end].is_alphanumeric()
                                    {
                                        word_end += 1;
                                    }

                                    if word_start < word_end {
                                        // Extract and uppercase the word
                                        let word: String =
                                            chars[word_start..word_end].iter().collect();
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
                                        card.description = tui_textarea::TextArea::new(all_lines);

                                        // Restore cursor position (after the uppercased word)
                                        let new_cursor_col =
                                            word_start + uppercased.chars().count();
                                        card.description.move_cursor(
                                            tui_textarea::CursorMove::Jump(
                                                cursor_row as u16,
                                                new_cursor_col as u16,
                                            ),
                                        );
                                    }
                                }
                            }

                            self.autocomplete
                                .after_textarea_change(&card.description, &mut self.needs_redraw);
                            return true;
                        }
                    }
                }
                false
            }
            KeyboardOperation::LowercaseWord => {
                // Lowercase word (Alt+L) - transform word at/after cursor to lowercase
                if let FocusElement::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == FocusElement::TaskDescription {
                            // Get current line and cursor position
                            let lines = card.description.lines();
                            let (cursor_row, cursor_col) = card.description.cursor();

                            if cursor_row < lines.len() {
                                let current_line = &lines[cursor_row];
                                let chars: Vec<char> = current_line.chars().collect();

                                if cursor_col < chars.len() {
                                    // Find word boundaries around cursor
                                    let mut word_start = cursor_col;
                                    let mut word_end = cursor_col;

                                    // Find start of word (move left until non-alphanumeric)
                                    while word_start > 0 && chars[word_start - 1].is_alphanumeric()
                                    {
                                        word_start -= 1;
                                    }

                                    // Find end of word (move right until non-alphanumeric)
                                    while word_end < chars.len()
                                        && chars[word_end].is_alphanumeric()
                                    {
                                        word_end += 1;
                                    }

                                    if word_start < word_end {
                                        // Extract and lowercase the word
                                        let word: String =
                                            chars[word_start..word_end].iter().collect();
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
                                        card.description = tui_textarea::TextArea::new(all_lines);

                                        // Restore cursor position (after the lowercased word)
                                        let new_cursor_col =
                                            word_start + lowercased.chars().count();
                                        card.description.move_cursor(
                                            tui_textarea::CursorMove::Jump(
                                                cursor_row as u16,
                                                new_cursor_col as u16,
                                            ),
                                        );
                                    }
                                }
                            }

                            self.autocomplete
                                .after_textarea_change(&card.description, &mut self.needs_redraw);
                            return true;
                        }
                    }
                }
                false
            }
            KeyboardOperation::CapitalizeWord => {
                // Capitalize word (Alt+C) - capitalize word at/after cursor
                if let FocusElement::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == FocusElement::TaskDescription {
                            // Select word at/after cursor
                            card.description.start_selection();
                            card.description.move_cursor(tui_textarea::CursorMove::WordForward);
                            card.description.copy();

                            // Get the copied word and capitalize it
                            let word = card.description.yank_text();
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
                                card.description.set_yank_text(capitalized);

                                // Replace the selection
                                card.description.paste();

                                self.autocomplete.after_textarea_change(
                                    &card.description,
                                    &mut self.needs_redraw,
                                );
                                return true;
                            }
                        }
                    }
                }
                false
            }
            KeyboardOperation::JoinLines => {
                // Join lines (Alt+^) - delete newline between lines
                if let FocusElement::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == FocusElement::TaskDescription {
                            // Move cursor to end of line and delete newline
                            card.description.move_cursor(tui_textarea::CursorMove::End);
                            card.description.delete_next_char(); // This should delete the newline

                            self.autocomplete
                                .after_textarea_change(&card.description, &mut self.needs_redraw);
                            return true;
                        }
                    }
                }
                false
            }
            KeyboardOperation::Bold => {
                // Bold (Ctrl+B) - wrap selection or next word with **
                if let FocusElement::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == FocusElement::TaskDescription {
                            if card.description.selection_range().is_some() {
                                // Copy selection to yank buffer
                                card.description.copy();
                                let selected_text = card.description.yank_text();
                                if !selected_text.is_empty() {
                                    // Replace selection with wrapped text
                                    card.description.insert_str(&format!("**{}**", selected_text));
                                }
                            } else {
                                // Insert ** and position cursor between them
                                card.description.insert_char('*');
                                card.description.insert_char('*');
                                card.description.insert_char('*');
                                card.description.insert_char('*');
                                card.description.move_cursor(tui_textarea::CursorMove::Back);
                                card.description.move_cursor(tui_textarea::CursorMove::Back);
                            }

                            self.autocomplete
                                .after_textarea_change(&card.description, &mut self.needs_redraw);
                            return true;
                        }
                    }
                }
                false
            }
            KeyboardOperation::Italic => {
                // Italic (Ctrl+I) - wrap selection or next word with *
                if let FocusElement::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == FocusElement::TaskDescription {
                            if card.description.selection_range().is_some() {
                                // Copy selection to yank buffer
                                card.description.copy();
                                let selected_text = card.description.yank_text();
                                if !selected_text.is_empty() {
                                    // Replace selection with wrapped text
                                    card.description.insert_str(&format!("*{}*", selected_text));
                                }
                            } else {
                                // Insert ** and position cursor between them
                                card.description.insert_str("**");
                                card.description.move_cursor(tui_textarea::CursorMove::Back);
                            }

                            self.autocomplete
                                .after_textarea_change(&card.description, &mut self.needs_redraw);
                            return true;
                        }
                    }
                }
                false
            }
            KeyboardOperation::Underline => {
                // Underline (Ctrl+U) - wrap selection with <u> tags
                if let FocusElement::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == FocusElement::TaskDescription {
                            if card.description.selection_range().is_some() {
                                // Copy selection to yank buffer
                                card.description.copy();
                                let selected_text = card.description.yank_text();
                                if !selected_text.is_empty() {
                                    // Replace selection with wrapped text
                                    card.description
                                        .insert_str(&format!("<u>{}</u>", selected_text));
                                }
                            } else {
                                // Insert tags and position cursor
                                card.description.insert_str("<u></u>");
                                card.description.move_cursor(tui_textarea::CursorMove::Back);
                                card.description.move_cursor(tui_textarea::CursorMove::Back);
                                card.description.move_cursor(tui_textarea::CursorMove::Back);
                                card.description.move_cursor(tui_textarea::CursorMove::Back);
                            }

                            self.autocomplete
                                .after_textarea_change(&card.description, &mut self.needs_redraw);
                            return true;
                        }
                    }
                }
                false
            }
            KeyboardOperation::CycleThroughClipboard => {
                // Cycle through clipboard (Alt+Y) - cycle through yank ring
                // This would require implementing a yank ring - simplified for now
                if let FocusElement::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == FocusElement::TaskDescription {
                            // Placeholder - would need yank ring implementation
                            return true;
                        }
                    }
                }
                false
            }
            KeyboardOperation::TransposeCharacters => {
                // Transpose characters (Ctrl+T) - swap character before cursor with character after
                if let FocusElement::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == FocusElement::TaskDescription {
                            // Simplified implementation
                            card.description.move_cursor(tui_textarea::CursorMove::Back);
                            card.description.delete_next_char();
                            card.description.move_cursor(tui_textarea::CursorMove::Forward);
                            // Full implementation would need to save characters and swap them
                            return true; // Placeholder
                        }
                    }
                }
                false
            }
            KeyboardOperation::TransposeWords => {
                // Transpose words (Alt+T) - swap word before cursor with word after
                if let FocusElement::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == FocusElement::TaskDescription {
                            // Simplified implementation - would need complex word boundary detection
                            return true; // Placeholder
                        }
                    }
                }
                false
            }
            KeyboardOperation::IncrementalSearchForward => {
                // Incremental search forward (Ctrl+S) - start search mode
                if let FocusElement::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == FocusElement::TaskDescription {
                            // Set search pattern (would need search dialog/input in real implementation)
                            card.description.set_search_pattern("search_term".to_string());
                            card.description.search_forward(false);
                            return true;
                        }
                    }
                }
                false
            }
            KeyboardOperation::IncrementalSearchBackward => {
                // Incremental search backward (Ctrl+R) - start reverse search mode
                if let FocusElement::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == FocusElement::TaskDescription {
                            // Set search pattern and search backward
                            card.description.set_search_pattern("search_term".to_string());
                            card.description.search_back(false);
                            return true;
                        }
                    }
                }
                false
            }
            KeyboardOperation::FindNext => {
                // Find next (F3) - jump to next search match
                if let FocusElement::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == FocusElement::TaskDescription {
                            card.description.search_forward(false);
                            return true;
                        }
                    }
                }
                false
            }
            KeyboardOperation::FindPrevious => {
                // Find previous (Shift+F3) - jump to previous search match
                if let FocusElement::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == FocusElement::TaskDescription {
                            card.description.search_back(false);
                            return true;
                        }
                    }
                }
                false
            }
            _ => false, // Other operations not implemented yet
        }
    }

    /// Handle a mouse click that was resolved to a semantic action by the view layer.
    pub fn handle_mouse_click(
        &mut self,
        action: MouseAction,
        column: u16,
        row: u16,
        bounds: &ratatui::layout::Rect,
    ) -> bool {
        self.clear_exit_confirmation();

        match action.clone() {
            MouseAction::FocusDraftTextarea(_card_index) => {
                self.handle_textarea_click(column, row, bounds);
                true
            }
            _ => {
                self.perform_mouse_action(action);
                true
            }
        }
    }

    /// Handle mouse scroll actions by mapping them to hierarchical navigation.
    fn handle_mouse_scroll(&mut self, direction: NavigationDirection) -> bool {
        self.clear_exit_confirmation();
        match direction {
            NavigationDirection::Next => self.navigate_down_hierarchy(),
            NavigationDirection::Previous => self.navigate_up_hierarchy(),
        }
    }

    /// Start background loading of workspace files and workflows
    pub fn start_background_loading(&mut self) {
        // Start loading files if not already loaded
        if !self.files_loaded && !self.loading_files {
            self.loading_files = true;
            let workspace_files = Arc::clone(&self.workspace_files);

            // Take the sender to move into the async task
            if let Some(sender) = self.files_sender.take() {
                tokio::spawn(async move {
                    match workspace_files.stream_repository_files().await {
                        Ok(mut stream) => {
                            let mut files = Vec::new();
                            while let Some(result) = stream.next().await {
                                if let Ok(repo_file) = result {
                                    files.push(repo_file.path);
                                }
                            }
                            // Send the loaded files back via the channel
                            let _ = sender.send(files);
                        }
                        Err(_) => {
                            // Send empty vec on error
                            let _ = sender.send(Vec::new());
                        }
                    }
                });
            }
        }

        // Start loading workflows if not already loaded
        if !self.workflows_loaded && !self.loading_workflows {
            self.loading_workflows = true;
            let workspace_workflows = Arc::clone(&self.workspace_workflows);

            // Take the sender to move into the async task
            if let Some(sender) = self.workflows_sender.take() {
                tokio::spawn(async move {
                    match workspace_workflows.enumerate_workflow_commands().await {
                        Ok(commands) => {
                            let workflows: Vec<String> =
                                commands.into_iter().map(|c| c.name).collect();
                            // Send the loaded workflows back via the channel
                            let _ = sender.send(workflows);
                        }
                        Err(_) => {
                            // Send empty vec on error
                            let _ = sender.send(Vec::new());
                        }
                    }
                });
            }
        }
    }

    /// Check for completed background loading tasks and update state
    pub fn check_background_loading(&mut self) {
        // Check if files have been loaded
        if let Some(receiver) = self.files_receiver.as_mut() {
            if let Ok(files) = receiver.try_recv() {
                self.preloaded_files = files;
                self.files_loaded = true;
                self.loading_files = false;
                self.files_receiver = None;
                self.needs_redraw = true; // Trigger redraw to update autocomplete
            }
        }

        // Check if workflows have been loaded
        if let Some(receiver) = self.workflows_receiver.as_mut() {
            if let Ok(workflows) = receiver.try_recv() {
                self.preloaded_workflows = workflows;
                self.workflows_loaded = true;
                self.loading_workflows = false;
                self.workflows_receiver = None;
                self.needs_redraw = true; // Trigger redraw to update autocomplete
            }
        }
    }

    /// Close autocomplete if focus is moving away from textarea elements
    pub fn close_autocomplete_if_leaving_textarea(&mut self, new_focus: FocusElement) {
        let was_on_textarea = matches!(self.focus_element, FocusElement::TaskDescription)
            || matches!(self.focus_element, FocusElement::DraftTask(idx) if self.draft_cards.get(idx).map_or(false, |card| card.focus_element == FocusElement::TaskDescription));

        let moving_to_textarea = matches!(new_focus, FocusElement::TaskDescription)
            || matches!(new_focus, FocusElement::DraftTask(idx) if self.draft_cards.get(idx).map_or(false, |card| card.focus_element == FocusElement::TaskDescription));

        if was_on_textarea && !moving_to_textarea {
            self.autocomplete.close(&mut self.needs_redraw);
            // Cancel any active text selection when leaving textarea
            if let FocusElement::DraftTask(idx) = self.focus_element {
                if let Some(card) = self.draft_cards.get_mut(idx) {
                    if card.focus_element == FocusElement::TaskDescription
                        && card.description.selection_range().is_some()
                    {
                        card.description.cancel_selection();
                    }
                }
            }
        }
    }

    /// Handle textarea click to position caret
    fn handle_textarea_click(
        &mut self,
        column: u16,
        row: u16,
        textarea_area: &ratatui::layout::Rect,
    ) {
        self.last_textarea_area = Some(*textarea_area);

        // Focus on the draft task description if not already focused
        if !matches!(self.focus_element, FocusElement::TaskDescription) {
            self.focus_element = FocusElement::TaskDescription;
        }

        // Calculate relative position within textarea
        let relative_x = (column as i32 - textarea_area.x as i32).max(0) as u16;
        let relative_y = (row as i32 - textarea_area.y as i32).max(0) as u16;

        // Position caret in the first draft card's textarea
        if let Some(card) = self.draft_cards.first_mut() {
            // Use textarea's built-in cursor positioning
            // This is a simplified implementation - a full implementation would need
            // to calculate line/column from the click coordinates
            let line_index =
                relative_y.min(card.description.lines().len().saturating_sub(1) as u16) as usize;
            let line = card.description.lines().get(line_index).map_or("", |s| s);
            let col_index = relative_x.min(line.chars().count() as u16) as usize;

            // Set cursor position in textarea
            card.description.move_cursor(tui_textarea::CursorMove::Jump(
                line_index as u16,
                col_index as u16,
            ));
            self.autocomplete
                .after_textarea_change(&card.description, &mut self.needs_redraw);
        }

        self.needs_redraw = true;
    }

    /// Perform mouse action (similar to main.rs perform_mouse_action)
    pub fn perform_mouse_action(&mut self, action: MouseAction) {
        match action {
            MouseAction::OpenSettings => {
                self.close_autocomplete_if_leaving_textarea(FocusElement::SettingsButton);
                self.focus_element = FocusElement::SettingsButton;
                self.open_modal(ModalState::Settings);
                // TODO: Initialize settings form
            }
            MouseAction::SelectCard(idx) => {
                self.selected_card = idx;
                let new_focus = if idx == 0 {
                    // Draft card - focus on description
                    FocusElement::TaskDescription
                } else {
                    // Regular task card - idx is offset by 1, so array index is idx - 1
                    FocusElement::ExistingTask(idx - 1)
                };
                self.close_autocomplete_if_leaving_textarea(new_focus);
                self.focus_element = new_focus;
            }
            MouseAction::SelectFilterBarLine => {
                self.close_autocomplete_if_leaving_textarea(FocusElement::FilterBarLine);
                self.focus_element = FocusElement::FilterBarLine;
            }
            MouseAction::ActivateRepositoryModal => {
                self.close_autocomplete_if_leaving_textarea(FocusElement::RepositoryButton);
                self.focus_element = FocusElement::RepositoryButton;
                self.open_modal(ModalState::RepositorySearch);
            }
            MouseAction::ActivateBranchModal => {
                self.close_autocomplete_if_leaving_textarea(FocusElement::BranchButton);
                self.focus_element = FocusElement::BranchButton;
                self.open_modal(ModalState::BranchSearch);
            }
            MouseAction::ActivateModelModal => {
                self.close_autocomplete_if_leaving_textarea(FocusElement::ModelButton);
                self.focus_element = FocusElement::ModelButton;
                self.open_modal(ModalState::ModelSearch);
            }
            MouseAction::LaunchTask => {
                self.close_autocomplete_if_leaving_textarea(FocusElement::GoButton);
                self.focus_element = FocusElement::GoButton;
                self.handle_go_button();
            }
            MouseAction::FocusDraftTextarea(_idx) => {
                self.close_autocomplete_if_leaving_textarea(FocusElement::TaskDescription);
                self.focus_element = FocusElement::TaskDescription;
            }
            _ => {
                // TODO: Handle other mouse actions like ActivateGoButton, StopTask, EditFilter, Footer
            }
        }
        self.needs_redraw = true;
    }

    /// Process any pending task events from the event receiver

    /// Update the selection state in task cards based on current focus_element

    /// Update the footer based on current focus state
    pub fn update_footer(&mut self) {
        let focused_draft = self.get_focused_draft_card().map(|card| DraftTask {
            id: card.id.clone(),
            description: card.description.lines().join("\n"),
            repository: card.repository.clone(),
            branch: card.branch.clone(),
            models: card.models.clone(),
            created_at: card.created_at.clone(),
        });
        self.footer = create_footer_view_model(
            focused_draft.as_ref(),
            self.focus_element,
            self.modal_state,
            &self.settings,
            self.word_wrap_enabled,
            self.show_autocomplete_border,
        );
    }

    /// Open a modal dialog
    pub fn open_modal(&mut self, modal_state: ModalState) {
        self.modal_state = modal_state;
        self.active_modal = create_modal_view_model(
            modal_state,
            &self.available_repositories,
            &self.available_branches,
            &self.available_models,
            &self.get_focused_draft_card().map(|card| DraftTask {
                id: card.id.clone(),
                description: card.description.lines().join("\n"),
                repository: card.repository.clone(),
                branch: card.branch.clone(),
                models: card.models.clone(),
                created_at: card.created_at.clone(),
            }),
            self.settings.activity_rows(),
            self.word_wrap_enabled,
            self.show_autocomplete_border,
        );
        self.exit_confirmation_armed = false;
    }

    /// Close the current modal
    pub fn close_modal(&mut self) {
        self.modal_state = ModalState::None;
        self.active_modal = None;
        self.exit_confirmation_armed = false;
    }

    /// Select a repository from modal
    pub fn select_repository(&mut self, repo: String) {
        if let FocusElement::DraftTask(idx) = self.focus_element {
            if let Some(draft_card) = self.draft_cards.get_mut(idx) {
                draft_card.repository = repo;
            }
        }
        self.close_modal();
    }

    /// Select a branch from modal
    pub fn select_branch(&mut self, branch: String) {
        if let FocusElement::DraftTask(idx) = self.focus_element {
            if let Some(draft_card) = self.draft_cards.get_mut(idx) {
                draft_card.branch = branch;
            }
        }
        self.close_modal();
    }

    /// Select model names from modal
    pub fn select_model_names(&mut self, model_names: Vec<String>) {
        if let FocusElement::DraftTask(idx) = self.focus_element {
            if let Some(draft_card) = self.draft_cards.get_mut(idx) {
                draft_card.models =
                    model_names.into_iter().map(|name| SelectedModel { name, count: 1 }).collect();
            }
        }
        self.close_modal();
    }

    /// Set status message
    pub fn set_status_message(&mut self, message: String) {
        self.status_message = Some(message);
    }

    /// Clear status message
    pub fn clear_status_message(&mut self) {
        self.status_message = None;
    }

    /// Set error message
    pub fn set_error_message(&mut self, message: String) {
        self.error_message = Some(message);
    }

    /// Clear error message
    pub fn clear_error_message(&mut self) {
        self.error_message = None;
    }

    /// Handle launch task operation and return domain messages
    /// Process a TaskEvent and update the corresponding task card's activity entries
    pub fn process_task_event(&mut self, task_id: &str, event: TaskEvent) {
        // Find the card info for this task_id
        if let Some(card_info) = self.task_id_to_card_info.get(task_id) {
            match card_info.card_type {
                TaskCardTypeEnum::Draft => {
                    // Draft cards don't have activity events - they're just text inputs
                    // Task events for draft cards don't make sense in this context
                }
                TaskCardTypeEnum::Task => {
                    if let Some(card) = self.task_cards.get_mut(card_info.index) {
                        if let TaskCardType::Active {
                            ref mut activity_entries,
                            ..
                        } = card.card_type
                        {
                            match event {
                                TaskEvent::Thought { thought, .. } => {
                                    // Add new thought entry
                                    let activity_entry = AgentActivityRow::AgentThought {
                                        thought: thought.clone(),
                                    };
                                    activity_entries.push(activity_entry);
                                }
                                TaskEvent::FileEdit {
                                    file_path,
                                    lines_added,
                                    lines_removed,
                                    description,
                                    ..
                                } => {
                                    // Add new file edit entry
                                    let activity_entry = AgentActivityRow::AgentEdit {
                                        file_path: file_path.clone(),
                                        lines_added,
                                        lines_removed,
                                        description: description.clone(),
                                    };
                                    activity_entries.push(activity_entry);
                                }
                                TaskEvent::ToolUse {
                                    tool_name,
                                    tool_execution_id,
                                    status,
                                    ..
                                } => {
                                    // Add new tool use entry
                                    let activity_entry = AgentActivityRow::ToolUse {
                                        tool_name: tool_name.clone(),
                                        tool_execution_id: tool_execution_id.clone(),
                                        last_line: None,
                                        completed: false,
                                        status,
                                    };
                                    activity_entries.push(activity_entry);
                                }
                                TaskEvent::Log {
                                    message,
                                    tool_execution_id: Some(tool_exec_id),
                                    ..
                                } => {
                                    // Update existing tool use entry with log message as last_line
                                    if let Some(AgentActivityRow::ToolUse { tool_execution_id: _, ref mut last_line, .. }) =
                                        activity_entries.iter_mut().rev().find(|entry| {
                                            matches!(entry, AgentActivityRow::ToolUse { tool_execution_id: exec_id, .. } if exec_id == &tool_exec_id)
                                        }) {
                                        *last_line = Some(message.clone());
                                    } else {
                                    }
                                }
                                TaskEvent::ToolResult {
                                    tool_name: _,
                                    tool_output,
                                    tool_execution_id,
                                    status: result_status,
                                    ..
                                } => {
                                    // Update existing tool use entry to mark as completed
                                    if let Some(AgentActivityRow::ToolUse { ref mut completed, ref mut last_line, ref mut status, .. }) =
                                        activity_entries.iter_mut().rev().find(|entry| {
                                            matches!(entry, AgentActivityRow::ToolUse { tool_execution_id: exec_id, .. } if exec_id == &tool_execution_id)
                                        }) {
                                        *completed = true;
                                        *status = result_status.clone();
                                        // Set last_line to first line of final output if not already set
                                        if last_line.is_none() {
                                            *last_line = Some(tool_output.lines().next().unwrap_or("Completed").to_string());
                                        }
                                    } else {
                                    }
                                }
                                // Other events (Status, Log without tool_execution_id) are not converted to activity entries
                                // They might be used for other purposes like status updates
                                _ => return, // Skip events that don't affect activity entries
                            };

                            // Keep only the most recent N events
                            let before_trim = activity_entries.len();
                            while activity_entries.len() > self.settings.activity_rows() {
                                activity_entries.remove(0);
                            }
                            if before_trim > activity_entries.len() {}

                            // Height remains fixed at 5 for active cards (title + separator + max 3 activity lines)
                        }
                    }
                }
            }
        }
    }

    /// Initialize the shared task event channel
    fn initialize_task_event_channel(&mut self) {
        if self.task_event_sender.is_none() {
            let (tx, rx) = mpsc::channel(100);
            self.task_event_sender = Some(tx);
            self.task_event_receiver = Some(rx);
        }
    }

    /// Rebuild the task_id to card info mapping for fast lookups
    pub fn rebuild_task_id_mapping(&mut self) {
        self.task_id_to_card_info.clear();

        // Add draft cards
        for (index, card) in self.draft_cards.iter().enumerate() {
            self.task_id_to_card_info.insert(
                card.id.clone(),
                TaskCardInfo {
                    card_type: TaskCardTypeEnum::Draft,
                    index,
                },
            );
        }

        // Add task cards
        for (index, card) in self.task_cards.iter().enumerate() {
            self.task_id_to_card_info.insert(
                card.task.id.clone(),
                TaskCardInfo {
                    card_type: TaskCardTypeEnum::Task,
                    index,
                },
            );
        }
    }

    /// Start consuming events for a launched task
    fn start_task_event_consumption(&mut self, task_id: &str) {
        // Initialize shared channel if not already done
        self.initialize_task_event_channel();

        let stream = self.task_manager.task_events_stream(task_id);
        let task_id_owned = task_id.to_string();
        let task_id_for_hashmap = task_id.to_string();

        // Clone the shared sender for this task
        let tx = self.task_event_sender.as_ref().unwrap().clone();

        // Spawn a task to consume the event stream and send events with task_id to the shared channel
        let handle = tokio::spawn(async move {
            let mut stream = stream;
            while let Some(event) = stream.next().await {
                let _ = tx.send((task_id_owned.clone(), event)).await;
            }
        });

        self.active_task_streams.insert(task_id_for_hashmap, handle);
    }

    /// Process any pending task events from the shared receiver (non-blocking)
    pub fn process_pending_task_events(&mut self) {
        // Collect all available events first to avoid borrow conflicts
        let mut pending_events = Vec::new();
        if let Some(ref mut receiver) = self.task_event_receiver {
            while let Ok(event) = receiver.try_recv() {
                pending_events.push(event);
            }
        }

        // Now process the collected events
        let mut events_processed = false;
        for (task_id, event) in pending_events {
            self.process_task_event(&task_id, event);
            events_processed = true;
        }

        // If we processed events, we need to redraw the UI
        if events_processed {
            self.needs_redraw = true;
        }
    }

    /// Load initial tasks from the TaskManager
    pub async fn load_initial_tasks(&mut self) -> Result<(), String> {
        let (draft_infos, task_executions) = self.task_manager.get_initial_tasks().await;

        // Only add draft cards from TaskManager if we don't already have any draft cards
        if self.draft_cards.is_empty() {
            // Convert draft TaskInfo to draft cards with embedded tasks
            for draft_info in draft_infos {
                let draft = DraftTask {
                    id: draft_info.id,
                    description: draft_info.title, // Use title as initial description
                    repository: draft_info.repository,
                    branch: draft_info.branch,
                    models: vec![SelectedModel {
                        name: "Claude".to_string(),
                        count: 1,
                    }], // Default model
                    created_at: draft_info.created_at,
                };
                let draft_card = create_draft_card_from_task(draft, self.focus_element);
                self.draft_cards.push(draft_card);
            }
        }

        // Convert task TaskExecution to task cards
        for task_execution in &task_executions {
            let task_card = create_task_card_from_execution(task_execution.clone(), &self.settings);
            self.task_cards.push(task_card);
        }

        // UI is already updated since we pushed the cards directly

        // Build the task ID mapping for fast lookups
        self.rebuild_task_id_mapping();

        // Start event consumption for active tasks so they show live activity
        for task_execution in &task_executions {
            if matches!(task_execution.state, TaskState::Active) {
                self.start_task_event_consumption(&task_execution.id);
            }
        }

        Ok(())
    }

    /// Get the currently focused draft card (mutable reference)
    pub fn get_focused_draft_card_mut(&mut self) -> Option<&mut TaskEntryViewModel> {
        if let FocusElement::DraftTask(index) = self.focus_element {
            self.draft_cards.get_mut(index)
        } else {
            None
        }
    }

    /// Get the currently focused draft card (immutable reference)
    pub fn get_focused_draft_card(&self) -> Option<&TaskEntryViewModel> {
        if let FocusElement::DraftTask(index) = self.focus_element {
            self.draft_cards.get(index)
        } else {
            None
        }
    }

    /// Auto-save the currently focused draft task
    pub async fn save_current_draft(&mut self) -> Result<(), String> {
        // Get the focused draft card with its embedded task data
        let Some(card) = self.get_focused_draft_card() else {
            return Ok(()); // No focused draft to save
        };

        let draft_id = card.id.clone();
        let description = card.description.lines().join("\n");
        let repository = card.repository.clone();
        let branch = card.branch.clone();
        let models = card.models.clone();

        // Find and update the draft card in the view model to show "Saving" state
        // Note: We search by ID, not by current focus, since focus might change during await
        if let Some(card) = self.draft_cards.iter_mut().find(|c| c.id == draft_id) {
            card.save_state = DraftSaveState::Saving;
        }

        let result = self
            .task_manager
            .save_draft_task(&draft_id, &description, &repository, &branch, &models)
            .await;

        // Update save state based on result - find the card by ID again
        // The card might have been deleted while the save was in flight
        if let Some(card) = self.draft_cards.iter_mut().find(|c| c.id == draft_id) {
            match result {
                SaveDraftResult::Success => {
                    card.save_state = DraftSaveState::Saved;
                    Ok(())
                }
                SaveDraftResult::Failure { error } => {
                    card.save_state = DraftSaveState::Error;
                    Err(error)
                }
            }
        } else {
            // Draft card was deleted while save was in flight - ignore the result
            Ok(())
        }
    }

    /// Mark the currently focused draft as having unsaved changes
    pub fn mark_focused_draft_unsaved(&mut self) {
        if let Some(card) = self.get_focused_draft_card_mut() {
            card.save_state = DraftSaveState::Unsaved;
        }
    }

    // Domain business logic methods (moved from Model)

    /// Launch a task by draft ID
    pub async fn launch_task(&mut self, draft_id: &str) -> Result<(), String> {
        if let Some(card) = self.draft_cards.iter().find(|c| c.id == draft_id) {
            if !card.description.lines().join("\n").trim().is_empty() && !card.models.is_empty() {
                // Set loading state
                self.loading_task_creation = true;

                // In real implementation, this would send a network request
                // For now, we simulate success by calling the task manager directly
                let params = TaskLaunchParams {
                    description: card.description.lines().join("\n"),
                    repository: card.repository.clone(),
                    branch: card.branch.clone(),
                    models: card.models.clone(),
                };

                match self.task_manager.launch_task(params).await {
                    TaskLaunchResult::Success { task_id } => {
                        // Create a new task execution
                        let task_execution = TaskExecution {
                            id: task_id.clone(),
                            repository: card.repository.clone(),
                            branch: card.branch.clone(),
                            agents: card.models.clone(),
                            state: TaskState::Active,
                            timestamp: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                            activity: vec![],
                            delivery_status: vec![],
                        };

                        // Create a new task card with the embedded task execution
                        let task_card =
                            create_task_card_from_execution(task_execution, &self.settings);
                        self.task_cards.push(task_card);

                        // Start listening to task events
                        self.start_task_event_consumption(&task_id);

                        // Clear loading state
                        self.loading_task_creation = false;

                        // Update UI
                        self.refresh_task_cards();

                        Ok(())
                    }
                    TaskLaunchResult::Failure { error } => {
                        self.loading_task_creation = false;
                        Err(error)
                    }
                }
            } else {
                Ok(())
            }
        } else {
            Ok(())
        }
    }

    /// Create a new draft task
    pub fn create_new_draft_task(&mut self, draft_id: &str) {
        if let Some(card_index) = self.draft_cards.iter().position(|c| c.id == draft_id) {
            let current_card = &self.draft_cards[card_index];
            let current_description = current_card.description.lines().join("\n");
            if !current_description.trim().is_empty() {
                let draft_task = DraftTask {
                    id: format!("draft_{}", chrono::Utc::now().timestamp()),
                    description: current_description,
                    repository: current_card.repository.clone(),
                    branch: current_card.branch.clone(),
                    models: current_card.models.clone(),
                    created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                };

                // Create a new draft card with the embedded task
                let new_card = create_draft_card_from_task(draft_task, self.focus_element);
                self.draft_cards.insert(0, new_card);

                // Update UI
                self.refresh_draft_cards();

                // Update UI for the cleared draft
                self.refresh_draft_cards();
            }
        }
    }

    /// Delete a task by its index in the combined draft + task list
    pub fn delete_task_by_index(&mut self, combined_index: usize) {
        let total_drafts = self.draft_cards.len();

        if combined_index < total_drafts {
            // Delete draft task
            self.draft_cards.remove(combined_index);
            self.refresh_draft_cards();
        } else {
            // Delete regular task
            let regular_task_index = combined_index - total_drafts;
            if regular_task_index < self.task_cards.len() {
                self.task_cards.remove(regular_task_index);
                self.refresh_task_cards();
            }
        }
    }

    /// Update draft text
    pub fn update_draft_text(&mut self, text: &str, draft_id: &str) {
        if let Some(card) = self.draft_cards.iter_mut().find(|c| c.id == draft_id) {
            // Update textarea content by clearing and inserting new text
            // Note: ratatui_textarea doesn't have select_all, so we recreate the textarea
            card.description = tui_textarea::TextArea::new(
                text.lines().map(|s| s.to_string()).collect::<Vec<String>>(),
            );
            card.description.set_cursor_line_style(ratatui::style::Style::default());
        }
    }

    /// Set draft repository
    pub fn set_draft_repository(&mut self, repo: &str, draft_id: &str) {
        if let Some(card) = self.draft_cards.iter_mut().find(|c| c.id == draft_id) {
            card.repository = repo.to_string();
        }
    }

    /// Set draft branch
    pub fn set_draft_branch(&mut self, branch: &str, draft_id: &str) {
        if let Some(card) = self.draft_cards.iter_mut().find(|c| c.id == draft_id) {
            card.branch = branch.to_string();
        }
    }

    /// Set draft model names
    pub fn set_draft_model_names(&mut self, model_names: Vec<String>, draft_id: &str) {
        if let Some(card) = self.draft_cards.iter_mut().find(|c| c.id == draft_id) {
            // Convert model names to SelectedModel with count 1
            card.models =
                model_names.into_iter().map(|name| SelectedModel { name, count: 1 }).collect();
        }
    }

    /// Update active task activities (simulation)
    pub fn update_active_task_activities(&mut self) -> bool {
        // Simulate activity updates for active tasks
        let mut had_changes = false;
        for card in self.task_cards.iter_mut() {
            if card.task.state == TaskState::Active {
                // In real implementation, would receive via SSE
                // For testing, simulate random activities
                // For now, just return false since we don't actually change anything
                had_changes = false;
            }
        }
        had_changes
    }

    /// Handle network messages (simplified since we don't use NetworkMsg anymore)
    pub fn handle_repositories_loaded(&mut self, repos: Vec<String>) {
        self.available_repositories = repos;
        self.loading_repositories = false;
    }

    pub fn handle_branches_loaded(&mut self, branches: Vec<String>) {
        self.available_branches = branches;
        self.loading_branches = false;
    }

    pub fn handle_models_loaded(&mut self, models: Vec<String>) {
        self.available_models = models;
        self.loading_models = false;
    }

    pub fn handle_task_created(&mut self, _task_id: String) {
        self.loading_task_creation = false;
    }

    pub fn handle_initial_tasks_loaded(&mut self, tasks: Vec<TaskInfo>) {
        // Convert TaskInfo to TaskExecution objects and add them
        for _task_info in tasks {
            // This would need to be implemented based on how TaskInfo maps to TaskExecution
        }
    }

    // UI refresh helpers
    /// Navigate up through the UI hierarchy
    pub fn navigate_up_hierarchy(&mut self) -> bool {
        let new_focus = match self.focus_element {
            FocusElement::SettingsButton => {
                // At top, wrap to bottom (last existing task or filter separator or last draft)
                if !self.task_cards.is_empty() {
                    FocusElement::ExistingTask(self.task_cards.len() - 1)
                } else if self.draft_cards.is_empty() {
                    FocusElement::FilterBarSeparator
                } else {
                    FocusElement::DraftTask(self.draft_cards.len() - 1)
                }
            }
            FocusElement::DraftTask(idx) => {
                if idx == 0 {
                    // First draft, go to settings
                    FocusElement::SettingsButton
                } else {
                    // Previous draft
                    FocusElement::DraftTask(idx - 1)
                }
            }
            FocusElement::FilterBarSeparator => {
                // From filter separator, go to last draft or settings
                if !self.draft_cards.is_empty() {
                    FocusElement::DraftTask(self.draft_cards.len() - 1)
                } else {
                    FocusElement::SettingsButton
                }
            }
            FocusElement::ExistingTask(idx) => {
                if idx == 0 {
                    // First existing task, go to filter separator
                    FocusElement::FilterBarSeparator
                } else {
                    // Previous existing task
                    FocusElement::ExistingTask(idx - 1)
                }
            }
            // Other focus elements stay the same
            other => other,
        };

        if new_focus != self.focus_element {
            // Reset internal focus of draft cards when they lose global focus
            if let FocusElement::DraftTask(idx) = self.focus_element {
                if !matches!(new_focus, FocusElement::DraftTask(_)) {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        card.focus_element = FocusElement::TaskDescription;
                    }
                }
            }

            self.focus_element = new_focus;
            self.update_footer();
            true
        } else {
            false
        }
    }

    /// Navigate down through the UI hierarchy
    pub fn navigate_down_hierarchy(&mut self) -> bool {
        let new_focus = match self.focus_element {
            FocusElement::SettingsButton => {
                // From settings, go to first draft or filter separator or first existing
                if !self.draft_cards.is_empty() {
                    FocusElement::DraftTask(0)
                } else if !self.task_cards.is_empty() {
                    FocusElement::FilterBarSeparator
                } else {
                    FocusElement::ExistingTask(0)
                }
            }
            FocusElement::DraftTask(idx) => {
                if idx >= self.draft_cards.len() - 1 {
                    // Last draft, go to filter separator if we have existing tasks
                    if !self.task_cards.is_empty() {
                        FocusElement::FilterBarSeparator
                    } else {
                        // No existing tasks, wrap to settings
                        FocusElement::SettingsButton
                    }
                } else {
                    // Next draft
                    FocusElement::DraftTask(idx + 1)
                }
            }
            FocusElement::FilterBarSeparator => {
                // From filter separator, go to first existing task or wrap to settings
                if !self.task_cards.is_empty() {
                    FocusElement::ExistingTask(0)
                } else {
                    FocusElement::SettingsButton
                }
            }
            FocusElement::ExistingTask(idx) => {
                if idx >= self.task_cards.len() - 1 {
                    // Last existing task, wrap to settings
                    FocusElement::SettingsButton
                } else {
                    // Next existing task
                    FocusElement::ExistingTask(idx + 1)
                }
            }
            // Other focus elements stay the same
            other => other,
        };

        if new_focus != self.focus_element {
            // Reset internal focus of draft cards when they lose global focus
            if let FocusElement::DraftTask(idx) = self.focus_element {
                if !matches!(new_focus, FocusElement::DraftTask(_)) {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        card.focus_element = FocusElement::TaskDescription;
                    }
                }
            }

            self.focus_element = new_focus;
            self.update_footer();
            true
        } else {
            false
        }
    }

    // UI refresh helpers
    pub fn refresh_draft_cards(&mut self) {
        // Since cards contain the tasks directly, we don't need to recreate them
        // Just update any UI-specific properties if needed
    }

    pub fn refresh_task_cards(&mut self) {
        // Since cards contain the tasks directly, we don't need to recreate them
        // Just update any UI-specific properties if needed
    }
}

// View Model Creation Functions (moved from model.rs)

/// Create a properly configured TextArea for draft card descriptions
fn create_draft_card_textarea(text: &str) -> tui_textarea::TextArea<'static> {
    let mut textarea =
        tui_textarea::TextArea::new(text.lines().map(|s| s.to_string()).collect::<Vec<String>>());
    // Remove underline styling from textarea
    textarea.set_style(Style::default().remove_modifier(Modifier::UNDERLINED));
    textarea.set_cursor_line_style(Style::default());
    if text.is_empty() {
        textarea.set_placeholder_text("Describe what you want the agent to do...");
    }
    textarea
}

/// Create a draft card from a DraftTask
pub fn create_draft_card_from_task(
    task: DraftTask,
    focus_element: FocusElement,
) -> TaskEntryViewModel {
    let description = create_draft_card_textarea(&task.description);

    let controls = TaskEntryControlsViewModel {
        repository_button: ButtonViewModel {
            text: task.repository.clone(),
            is_focused: false,
            style: ButtonStyle::Normal,
        },
        branch_button: ButtonViewModel {
            text: task.branch.clone(),
            is_focused: false,
            style: ButtonStyle::Normal,
        },
        model_button: ButtonViewModel {
            text: task
                .models
                .first()
                .map(|m| m.name.clone())
                .unwrap_or_else(|| "Select model".to_string()),
            is_focused: false,
            style: ButtonStyle::Normal,
        },
        go_button: ButtonViewModel {
            text: "Go".to_string(),
            is_focused: false,
            style: ButtonStyle::Normal,
        },
    };

    // Calculate height dynamically like in main.rs TaskCard::height for Draft
    let visible_lines = description.lines().len().max(5); // MIN_TEXTAREA_VISIBLE_LINES = 5
    let inner_height = visible_lines + 1 + 1 + 1 + 1; // TEXTAREA_TOP_PADDING + TEXTAREA_BOTTOM_PADDING + separator + button_row
    let height = inner_height as u16 + 2; // account for rounded border

    TaskEntryViewModel {
        id: task.id,
        repository: task.repository,
        branch: task.branch,
        models: task.models,
        created_at: task.created_at,
        height,
        controls,
        save_state: DraftSaveState::Unsaved,
        description,
        focus_element,
        auto_save_timer: None,
    }
}

/// Create a task card from a TaskExecution
fn create_task_card_from_execution(
    task: TaskExecution,
    _settings: &Settings,
) -> TaskExecutionViewModel {
    let title = format_title_from_execution(&task);

    let metadata = TaskMetadataViewModel {
        repository: task.repository.clone(),
        branch: task.branch.clone(),
        models: task.agents.clone(),
        state: task.state,
        timestamp: task.timestamp.clone(),
        delivery_indicators: task
            .delivery_status
            .iter()
            .map(|status| match status {
                DeliveryStatus::BranchCreated => "⎇",
                DeliveryStatus::PullRequestCreated { .. } => "⇄",
                DeliveryStatus::PullRequestMerged { .. } => "✓",
            })
            .collect::<Vec<_>>()
            .join(" "),
    };

    let card_type = match task.state {
        TaskState::Active => {
            let activity_entries = task
                .activity
                .iter()
                .map(|activity| AgentActivityRow::AgentThought {
                    thought: activity.clone(),
                })
                .collect::<Vec<_>>();
            TaskCardType::Active {
                activity_entries,
                pause_delete_buttons: "Pause | Delete".to_string(),
            }
        }
        TaskState::Completed => TaskCardType::Completed {
            delivery_indicators: String::new(),
        },
        TaskState::Merged => TaskCardType::Merged {
            delivery_indicators: String::new(),
        },
        TaskState::Draft => unreachable!("Drafts should not be in task_executions"),
    };

    TaskExecutionViewModel {
        id: task.id.clone(),
        task: task.clone(),
        title: title.clone(),
        metadata,
        height: 1, // Will be calculated in the view layer
        card_type,
        focus_element: FocusElement::GoButton, // Default focus for task cards
    }
}

/// Create ViewModel representations for draft tasks
fn create_draft_card_view_models(
    draft_tasks: &[DraftTask],
    _task_executions: &[TaskExecution],
    focus_element: FocusElement,
) -> Vec<TaskEntryViewModel> {
    draft_tasks
        .iter()
        .map(|draft| {
            let textarea = create_draft_card_textarea(&draft.description);

            let controls = TaskEntryControlsViewModel {
                repository_button: ButtonViewModel {
                    text: draft.repository.clone(),
                    is_focused: false,
                    style: ButtonStyle::Normal,
                },
                branch_button: ButtonViewModel {
                    text: draft.branch.clone(),
                    is_focused: false,
                    style: ButtonStyle::Normal,
                },
                model_button: ButtonViewModel {
                    text: draft
                        .models
                        .first()
                        .map(|m| m.name.clone())
                        .unwrap_or_else(|| "Select model".to_string()),
                    is_focused: false,
                    style: ButtonStyle::Normal,
                },
                go_button: ButtonViewModel {
                    text: "Go".to_string(),
                    is_focused: false,
                    style: ButtonStyle::Normal,
                },
            };

            // Calculate height dynamically like in main.rs TaskCard::height for Draft
            let visible_lines = textarea.lines().len().max(5); // MIN_TEXTAREA_VISIBLE_LINES = 5
            let inner_height = visible_lines + 1 + 1 + 1 + 1; // TEXTAREA_TOP_PADDING + TEXTAREA_BOTTOM_PADDING + separator + button_row
            let height = inner_height as u16 + 2; // account for rounded border

            TaskEntryViewModel {
                id: draft.id.clone(),
                repository: draft.repository.clone(),
                branch: draft.branch.clone(),
                models: draft.models.clone(),
                created_at: draft.created_at.clone(),
                height,
                controls,
                save_state: DraftSaveState::Unsaved,
                description: textarea,
                focus_element,
                auto_save_timer: None,
            }
        })
        .collect()
}

/// Create ViewModel representations for regular tasks (active/completed/merged)
fn create_task_card_view_models(
    draft_tasks: &[DraftTask],
    task_executions: &[TaskExecution],
    focus_element: FocusElement,
    settings: &Settings,
) -> Vec<TaskExecutionViewModel> {
    let visible_tasks = TaskItem::all_tasks_from_state(draft_tasks, task_executions);

    visible_tasks
        .into_iter()
        .enumerate()
        .map(|(_idx, task_item)| {
            match task_item {
                TaskItem::Task(task_execution, _) => {
                    let title = format_title_from_execution(&task_execution);

                    let metadata = TaskMetadataViewModel {
                        repository: task_execution.repository.clone(),
                        branch: task_execution.branch.clone(),
                        models: task_execution.agents.clone(),
                        state: task_execution.state,
                        timestamp: task_execution.timestamp.clone(),
                        delivery_indicators: task_execution
                            .delivery_status
                            .iter()
                            .map(|status| match status {
                                DeliveryStatus::BranchCreated => "⎇",
                                DeliveryStatus::PullRequestCreated { .. } => "⇄",
                                DeliveryStatus::PullRequestMerged { .. } => "✓",
                            })
                            .collect::<Vec<_>>()
                            .join(" "),
                    };

                    let card_type = match task_execution.state {
                        TaskState::Active => TaskCardType::Active {
                            activity_entries: task_execution
                                .activity
                                .iter()
                                .map(|activity| AgentActivityRow::AgentThought {
                                    thought: activity.clone(),
                                })
                                .collect(),
                            pause_delete_buttons: "Pause | Delete".to_string(),
                        },
                        TaskState::Completed => TaskCardType::Completed {
                            delivery_indicators: String::new(),
                        },
                        TaskState::Merged => TaskCardType::Merged {
                            delivery_indicators: String::new(),
                        },
                        TaskState::Draft => unreachable!("Drafts should not be in task_executions"),
                    };

                    TaskExecutionViewModel {
                        id: task_execution.id.clone(),
                        task: task_execution.clone(),
                        title,
                        metadata,
                        height: calculate_card_height(&task_execution, settings),
                        card_type,
                        focus_element,
                    }
                }
                TaskItem::Draft(_) => {
                    // Drafts are now handled by create_draft_card_view_models
                    unreachable!("Drafts should not appear in task card creation")
                }
            }
        })
        .collect()
}

fn format_title_from_execution(task: &TaskExecution) -> String {
    // For executed tasks, we might want to generate a title from the repository/branch
    // or use some other logic. For now, use a generic title.
    format!("Task {}", task.id)
}

fn calculate_card_height(task: &TaskExecution, settings: &Settings) -> u16 {
    // Calculate height based on activity lines + fixed overhead
    let activity_lines = settings.activity_rows().min(task.activity.len()) as u16;
    3 + activity_lines // Header + metadata + activity
}

fn create_modal_view_model(
    modal_state: ModalState,
    available_repositories: &[String],
    available_branches: &[String],
    available_models: &[String],
    current_draft: &Option<DraftTask>,
    activity_lines_count: usize,
    word_wrap_enabled: bool,
    show_autocomplete_border: bool,
) -> Option<ModalViewModel> {
    match modal_state {
        ModalState::None => None,
        ModalState::RepositorySearch => {
            let selected_repo = current_draft.as_ref().map(|draft| draft.repository.as_str());
            let (options, selected_index) =
                build_modal_options(available_repositories, selected_repo);
            Some(ModalViewModel {
                title: "Select repository".to_string(),
                input_value: String::new(),
                filtered_options: options,
                selected_index,
                modal_type: ModalType::Search {
                    placeholder: "Filter repositories...".to_string(),
                },
            })
        }
        ModalState::BranchSearch => {
            let selected_branch = current_draft.as_ref().map(|draft| draft.branch.as_str());
            let (options, selected_index) =
                build_modal_options(available_branches, selected_branch);
            Some(ModalViewModel {
                title: "Select branch".to_string(),
                input_value: String::new(),
                filtered_options: options,
                selected_index,
                modal_type: ModalType::Search {
                    placeholder: "Filter branches...".to_string(),
                },
            })
        }
        ModalState::ModelSearch => {
            let selected_model = current_draft
                .as_ref()
                .and_then(|draft| draft.models.first())
                .map(|model| model.name.as_str());
            let (options, selected_index) = build_modal_options(available_models, selected_model);
            Some(ModalViewModel {
                title: "Select model".to_string(),
                input_value: String::new(),
                filtered_options: options,
                selected_index,
                modal_type: ModalType::Search {
                    placeholder: "Filter models...".to_string(),
                },
            })
        }
        ModalState::Settings => {
            let mut fields = vec![
                SettingsFieldViewModel {
                    label: "Activity rows".to_string(),
                    value: activity_lines_count.to_string(),
                    is_focused: true,
                    field_type: SettingsFieldType::Number,
                },
                SettingsFieldViewModel {
                    label: "Word wrap".to_string(),
                    value: if word_wrap_enabled { "On" } else { "Off" }.to_string(),
                    is_focused: false,
                    field_type: SettingsFieldType::Boolean,
                },
                SettingsFieldViewModel {
                    label: "Autocomplete border".to_string(),
                    value: if show_autocomplete_border {
                        "On"
                    } else {
                        "Off"
                    }
                    .to_string(),
                    is_focused: false,
                    field_type: SettingsFieldType::Boolean,
                },
            ];

            if let Some(draft) = current_draft {
                fields.push(SettingsFieldViewModel {
                    label: "Current repository".to_string(),
                    value: draft.repository.clone(),
                    is_focused: false,
                    field_type: SettingsFieldType::Selection,
                });
                fields.push(SettingsFieldViewModel {
                    label: "Current branch".to_string(),
                    value: draft.branch.clone(),
                    is_focused: false,
                    field_type: SettingsFieldType::Selection,
                });
            }

            Some(ModalViewModel {
                title: "Settings".to_string(),
                input_value: String::new(),
                filtered_options: Vec::new(),
                selected_index: 0,
                modal_type: ModalType::Settings { fields },
            })
        }
    }
}

fn build_modal_options(
    options: &[String],
    selected_value: Option<&str>,
) -> (Vec<(String, bool)>, usize) {
    if options.is_empty() {
        return (Vec::new(), 0);
    }

    let mut selected_index = 0;
    let filtered_options = options
        .iter()
        .enumerate()
        .map(|(idx, value)| {
            if selected_value == Some(value.as_str()) {
                selected_index = idx;
            }
            (value.clone(), false)
        })
        .collect::<Vec<_>>();

    let mut filtered_options = filtered_options;
    if let Some(option) = filtered_options.get_mut(selected_index) {
        option.1 = true;
    }

    (filtered_options, selected_index)
}

fn create_footer_view_model(
    focused_draft: Option<&DraftTask>,
    focus_element: FocusElement,
    modal_state: ModalState,
    _settings: &Settings,
    _word_wrap_enabled: bool,
    _show_autocomplete_border: bool,
) -> FooterViewModel {
    use crate::settings::{KeyMatcher, KeyboardOperation, KeyboardShortcut};
    use ratatui::crossterm::event::{KeyCode, KeyModifiers};

    let mut shortcuts = Vec::new();

    // Create hardcoded shortcuts based on PRD specifications
    // These are the context-sensitive shortcuts that should be displayed in the footer

    match (focus_element, modal_state) {
        (_, ModalState::RepositorySearch)
        | (_, ModalState::BranchSearch)
        | (_, ModalState::ModelSearch) => {
            // Modal active: "↑↓ Navigate • Enter Select • Esc Back"
            shortcuts.push(KeyboardShortcut::new(
                KeyboardOperation::MoveToNextLine,
                vec![KeyMatcher::new(
                    KeyCode::Down,
                    KeyModifiers::empty(),
                    KeyModifiers::empty(),
                    None,
                )],
            ));
            shortcuts.push(KeyboardShortcut::new(
                KeyboardOperation::IndentOrComplete,
                vec![KeyMatcher::new(
                    KeyCode::Enter,
                    KeyModifiers::empty(),
                    KeyModifiers::empty(),
                    None,
                )],
            ));
            shortcuts.push(KeyboardShortcut::new(
                KeyboardOperation::DismissOverlay,
                vec![KeyMatcher::new(
                    KeyCode::Esc,
                    KeyModifiers::empty(),
                    KeyModifiers::empty(),
                    None,
                )],
            ));
        }
        (_, ModalState::Settings) => {
            // Settings modal shortcuts - similar to other modals
            shortcuts.push(KeyboardShortcut::new(
                KeyboardOperation::MoveToNextLine,
                vec![KeyMatcher::new(
                    KeyCode::Down,
                    KeyModifiers::empty(),
                    KeyModifiers::empty(),
                    None,
                )],
            ));
            shortcuts.push(KeyboardShortcut::new(
                KeyboardOperation::IndentOrComplete,
                vec![KeyMatcher::new(
                    KeyCode::Enter,
                    KeyModifiers::empty(),
                    KeyModifiers::empty(),
                    None,
                )],
            ));
            shortcuts.push(KeyboardShortcut::new(
                KeyboardOperation::DismissOverlay,
                vec![KeyMatcher::new(
                    KeyCode::Esc,
                    KeyModifiers::empty(),
                    KeyModifiers::empty(),
                    None,
                )],
            ));
        }
        (FocusElement::DraftTask(_), ModalState::None) if focused_draft.is_some() => {
            // Draft textarea focused: "Enter Launch Agent(s) • Shift+Enter New Line • Tab Next Field"
            shortcuts.push(KeyboardShortcut::new(
                KeyboardOperation::IndentOrComplete,
                vec![KeyMatcher::new(
                    KeyCode::Enter,
                    KeyModifiers::empty(),
                    KeyModifiers::empty(),
                    None,
                )],
            ));
            shortcuts.push(KeyboardShortcut::new(
                KeyboardOperation::OpenNewLine,
                vec![KeyMatcher::new(
                    KeyCode::Enter,
                    KeyModifiers::SHIFT,
                    KeyModifiers::empty(),
                    None,
                )],
            ));
            shortcuts.push(KeyboardShortcut::new(
                KeyboardOperation::MoveToNextField,
                vec![KeyMatcher::new(
                    KeyCode::Tab,
                    KeyModifiers::empty(),
                    KeyModifiers::empty(),
                    None,
                )],
            ));
        }
        (FocusElement::ExistingTask(_), ModalState::None) => {
            // Completed/merged task focused: "↑↓ Navigate • Enter Show Task Details • Ctrl+C x2 Quit"
            shortcuts.push(KeyboardShortcut::new(
                KeyboardOperation::MoveToNextLine,
                vec![KeyMatcher::new(
                    KeyCode::Down,
                    KeyModifiers::empty(),
                    KeyModifiers::empty(),
                    None,
                )],
            ));
            shortcuts.push(KeyboardShortcut::new(
                KeyboardOperation::IndentOrComplete,
                vec![KeyMatcher::new(
                    KeyCode::Enter,
                    KeyModifiers::empty(),
                    KeyModifiers::empty(),
                    None,
                )],
            ));
        }
        _ => {
            // Default navigation: "↑↓ Navigate • Enter Select Task • Ctrl+C x2 Quit"
            shortcuts.push(KeyboardShortcut::new(
                KeyboardOperation::MoveToNextLine,
                vec![KeyMatcher::new(
                    KeyCode::Down,
                    KeyModifiers::empty(),
                    KeyModifiers::empty(),
                    None,
                )],
            ));
            shortcuts.push(KeyboardShortcut::new(
                KeyboardOperation::IndentOrComplete,
                vec![KeyMatcher::new(
                    KeyCode::Enter,
                    KeyModifiers::empty(),
                    KeyModifiers::empty(),
                    None,
                )],
            ));
        }
    }

    FooterViewModel { shortcuts }
}

fn create_status_bar_view_model(
    status_message: Option<&String>,
    error_message: Option<&String>,
    _loading_task_creation: bool,
    _loading_repositories: bool,
    _loading_branches: bool,
    _loading_models: bool,
) -> StatusBarViewModel {
    StatusBarViewModel {
        backend_indicator: "local".to_string(),
        last_operation: "Ready".to_string(),
        connection_status: "Connected".to_string(),
        error_message: error_message.cloned(),
        status_message: status_message.cloned(),
    }
}
