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
use crate::settings::{KeyboardOperation, KeyboardShortcut};
use crate::view_model::autocomplete::{AutocompleteAcceptance, InlineAutocomplete};
use crate::view_model::input::{InputMinorMode, minor_modes};
use crate::view_model::task_entry::{
    CardFocusElement, DRAFT_TEXT_EDITING_MODE, KeyboardOperationResult,
};
use crate::view_model::{
    AgentActivityRow, ButtonStyle, ButtonViewModel, DraftSaveState, FilterControl, ModalState,
    SearchMode, TaskCardType, TaskEntryControlsViewModel, TaskEntryViewModel,
    TaskExecutionFocusState, TaskExecutionViewModel, TaskMetadataViewModel,
};
use ah_core::branches_enumerator::BranchesEnumerator;
use ah_core::repositories_enumerator::RepositoriesEnumerator;
use ah_core::task_manager::SaveDraftResult;
use ah_core::task_manager::{TaskEvent, TaskManager};
use ah_core::{WorkspaceFilesEnumerator, WorkspaceTermsEnumerator};
use ah_domain_types::{
    AgentChoice, AgentSoftware, AgentSoftwareBuild, DeliveryStatus, DraftTask, TaskExecution,
    TaskInfo, TaskState,
};
use ah_workflows::WorkspaceWorkflowsEnumerator;
use chrono;
use crossbeam_channel::Sender as UiSender;
use futures::stream::StreamExt;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::{Modifier, Style};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::oneshot;
use tracing::{debug, trace};
use uuid;

const ESC_CONFIRMATION_MESSAGE: &str = "Press Esc again to quit";
const AUTO_SAVE_DEBOUNCE: Duration = Duration::from_millis(500);

// Minor mode for modal dialogs (navigation, text editing, and model selection)
static MODAL_NAVIGATION_MODE: InputMinorMode = InputMinorMode::new(&[
    KeyboardOperation::MoveToNextLine,
    KeyboardOperation::MoveToPreviousLine,
    KeyboardOperation::MoveToNextField,
    KeyboardOperation::MoveToPreviousField,
    KeyboardOperation::ActivateCurrentItem,
    KeyboardOperation::DismissOverlay,
    // Text editing operations for modal input fields
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
    // Clipboard operations
    KeyboardOperation::Cut,
    KeyboardOperation::Copy,
    KeyboardOperation::Paste,
    KeyboardOperation::CycleThroughClipboard,
    // Model selection specific
    KeyboardOperation::IncrementValue,
    KeyboardOperation::DecrementValue,
]);

// Minor mode for model selection dialogs (navigation, count adjustment, and text input)
static MODEL_SELECTION_MODE: InputMinorMode = InputMinorMode::new(&[
    KeyboardOperation::MoveToNextLine,
    KeyboardOperation::MoveToPreviousLine,
    KeyboardOperation::ActivateCurrentItem,
    KeyboardOperation::IncrementValue,
    KeyboardOperation::DecrementValue,
    KeyboardOperation::DismissOverlay,
    // Text editing operations for the filter input
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
]);

// Minor mode for transitioning from draft textarea to buttons (Tab/Shift+Tab)
static DRAFT_TEXTAREA_TO_BUTTONS_MODE: InputMinorMode = InputMinorMode::new(&[
    KeyboardOperation::MoveToNextField,
    KeyboardOperation::MoveToPreviousField,
    KeyboardOperation::ShowLaunchOptions,
    KeyboardOperation::DeleteCurrentTask,
]);

// Minor mode for navigating draft card buttons (Repository, Branch, Model, Go)
static DRAFT_BUTTON_NAVIGATION_MODE: InputMinorMode = InputMinorMode::new(&[
    KeyboardOperation::MoveToNextField,
    KeyboardOperation::MoveToPreviousField,
    KeyboardOperation::MoveForwardOneCharacter,
    KeyboardOperation::MoveBackwardOneCharacter,
    KeyboardOperation::ActivateCurrentItem,
]);

// Minor mode for navigating matched items in selection dialogs
#[allow(dead_code)] // Reserved for selection dialog navigation refactor (not active yet).
static SELECTION_DIALOG_NAVIGATION_MODE: InputMinorMode = InputMinorMode::new(&[
    KeyboardOperation::MoveToNextLine,
    KeyboardOperation::MoveToPreviousLine,
    KeyboardOperation::ActivateCurrentItem,
    KeyboardOperation::IncrementValue,
    KeyboardOperation::DecrementValue,
    KeyboardOperation::DismissOverlay,
]);

// Minor mode for active task cards
#[allow(dead_code)] // Will be used when activating task card navigation minor mode.
static ACTIVE_TASK_NAVIGATION_MODE: InputMinorMode = InputMinorMode::new(&[
    KeyboardOperation::MoveToNextLine,
    KeyboardOperation::MoveToPreviousLine,
    KeyboardOperation::MoveToNextField,
    KeyboardOperation::MoveToPreviousField,
    KeyboardOperation::ActivateCurrentItem,
    KeyboardOperation::MoveToPreviousSnapshot,
    KeyboardOperation::MoveToNextSnapshot,
]);

// Minor mode for settings dialog navigation
#[allow(dead_code)] // Placeholder for settings dialog feature expansion.
static SETTINGS_DIALOG_NAVIGATION_MODE: InputMinorMode = InputMinorMode::new(&[
    KeyboardOperation::MoveToNextLine,
    KeyboardOperation::MoveToPreviousLine,
    KeyboardOperation::ActivateCurrentItem,
    KeyboardOperation::DismissOverlay,
]);

// Minor mode for settings field editing
#[allow(dead_code)] // Placeholder for granular settings field editing navigation.
static SETTINGS_FIELD_EDITING_MODE: InputMinorMode = InputMinorMode::new(&[
    KeyboardOperation::MoveToBeginningOfLine,
    KeyboardOperation::MoveToEndOfLine,
    KeyboardOperation::MoveForwardOneCharacter,
    KeyboardOperation::MoveBackwardOneCharacter,
    KeyboardOperation::DeleteCharacterForward,
    KeyboardOperation::DeleteCharacterBackward,
    KeyboardOperation::ActivateCurrentItem,
    KeyboardOperation::DismissOverlay,
]);

// Minor mode for general dashboard navigation (global navigation between cards and UI elements)
static DASHBOARD_NAVIGATION_MODE: InputMinorMode = InputMinorMode::new(&[
    KeyboardOperation::MoveToNextLine,
    KeyboardOperation::MoveToPreviousLine,
    KeyboardOperation::MoveToNextField,
    KeyboardOperation::MoveToPreviousField,
    KeyboardOperation::MoveForwardOneCharacter,
    KeyboardOperation::MoveBackwardOneCharacter,
    KeyboardOperation::DismissOverlay,
    KeyboardOperation::DraftNewTask,
    KeyboardOperation::ActivateCurrentItem,
    KeyboardOperation::DeleteCurrentTask,
]);

// Minor mode for task card selection and navigation (when cards are focused)
#[allow(dead_code)] // Will enable specific task card navigation flows when multiple cards active.
static TASK_CARD_NAVIGATION_MODE: InputMinorMode = InputMinorMode::new(&[
    KeyboardOperation::MoveToNextLine,
    KeyboardOperation::MoveToPreviousLine,
    KeyboardOperation::ActivateCurrentItem,
    KeyboardOperation::DismissOverlay,
]);

#[derive(Clone)]
struct AutoSaveRequestPayload {
    draft_id: String,
    request_id: u64,
    generation: u64,
    description: String,
    repository: String,
    branch: String,
    models: Vec<AgentChoice>,
}

/// Represents different types of items in a filtered options list
#[derive(Debug, Clone, PartialEq)]
pub enum FilteredOption {
    /// A selectable option with text and selection state
    Option { text: String, selected: bool },
    /// A separator with an optional label
    Separator { label: Option<String> },
}

/// Focus states specific to the dashboard interface
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DashboardFocusState {
    SettingsButton,
    FilterBarSeparator,
    FilterBarLine,
    DraftTask(usize),
    ExistingTask(usize),
    TaskCard(usize),
    ModalElement,
    // Legacy variants for compatibility
    RepositoryButton,
    BranchButton,
    ModelButton,
    Filter(FilterControl),
}

/// Focus control navigation (similar to main.rs)
impl ViewModel {
    fn handle_overlay_navigation(&mut self, direction: NavigationDirection) -> bool {
        if self.handle_modal_navigation(direction) {
            return true;
        }

        if let Some(modal) = &mut self.active_modal {
            if let ModalType::LaunchOptions { view_model: _ } = &mut modal.modal_type {
                if let NavigationDirection::Next = direction {
                    // Not handled by handle_modal_navigation if it's meant for column switching,
                    // but handle_modal_navigation handles up/down.
                    // We need to check if `direction` maps to left/right in `handle_dashboard_operation` context.
                    // Actually `NavigationDirection` is only Next/Previous (usually tab/shift-tab or arrows depending on mapping).
                    // Let's check `handle_modal_operation`.
                }
            }
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

    /// Gets the agent model name (alias) for a display model name
    pub fn get_agent_model_name(&self, display_model_name: &str) -> String {
        self.available_models
            .iter()
            .find(|model| model.display_name() == display_model_name)
            .map(|model| model.agent.version.clone())
            .unwrap_or_else(|| display_model_name.to_string()) // Fallback to original name
    }

    /// Map a model name to its corresponding agent type
    #[allow(dead_code)] // Mapping retained for future multi-agent model classification UI.
    fn get_agent_type_for_model(model_name: &str) -> ah_domain_types::AgentSoftware {
        if model_name.starts_with("Claude") {
            AgentSoftware::Claude
        } else if model_name.starts_with("GPT-5") {
            AgentSoftware::Codex
        } else {
            // Default fallback - this shouldn't happen with our current models
            AgentSoftware::Codex
        }
    }

    fn handle_dismiss_overlay(&mut self) -> bool {
        if self.modal_state != ModalState::None {
            // Check if there's an inline popup to close first
            if let Some(modal) = self.active_modal.as_mut() {
                if let ModalType::LaunchOptions { view_model } = &mut modal.modal_type {
                    if view_model.inline_enum_popup.is_some() {
                        // Close the inline popup
                        view_model.inline_enum_popup = None;
                        self.needs_redraw = true;
                        return true;
                    }
                }
            }

            // Determine which focus element to restore based on modal type
            let focus_element_to_restore = if let Some(modal) = &self.active_modal {
                match modal.modal_type {
                    ModalType::Search { .. } => Some(CardFocusElement::TaskDescription),
                    ModalType::AgentSelection { .. } => Some(CardFocusElement::TaskDescription),
                    ModalType::LaunchOptions { .. } => Some(CardFocusElement::TaskDescription),
                    ModalType::Settings { .. } => Some(CardFocusElement::TaskDescription),
                    _ => None,
                }
            } else {
                None
            };

            self.close_modal(false); // Dismiss overlay - discard changes

            // Restore focus after closing the modal
            if let Some(focus_element) = focus_element_to_restore {
                self.change_focus(DashboardFocusState::DraftTask(0));
                if let Some(card) = self.draft_cards.get_mut(0) {
                    card.focus_element = focus_element;
                }
            }

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

    pub fn focus_previous_control(&mut self) -> bool {
        // Implement reverse tab navigation for draft cards
        match self.focus_element {
            DashboardFocusState::DraftTask(idx) => {
                // Handle shift+tab navigation within the draft card
                if let Some(card) = self.draft_cards.get_mut(idx) {
                    match card.focus_element {
                        CardFocusElement::TaskDescription => {
                            card.focus_element = CardFocusElement::AdvancedOptionsButton;
                        }
                        CardFocusElement::AdvancedOptionsButton => {
                            card.focus_element = CardFocusElement::GoButton;
                        }
                        CardFocusElement::GoButton => {
                            card.focus_element = CardFocusElement::ModelSelector;
                        }
                        CardFocusElement::ModelSelector => {
                            card.focus_element = CardFocusElement::BranchSelector;
                        }
                        CardFocusElement::BranchSelector => {
                            card.focus_element = CardFocusElement::RepositorySelector;
                        }
                        CardFocusElement::RepositorySelector => {
                            card.focus_element = CardFocusElement::TaskDescription;
                        }
                    }
                    self.needs_redraw = true;
                    true
                } else {
                    false
                }
            }
            // For other global focus elements, handle normally
            DashboardFocusState::SettingsButton => {
                if !self.draft_cards.is_empty() {
                    self.change_focus(DashboardFocusState::DraftTask(0));
                    true
                } else if !self.task_cards.is_empty() {
                    self.change_focus(DashboardFocusState::FilterBarSeparator);
                    true
                } else {
                    false // Stay on settings if nothing else
                }
            }
            _ => {
                self.change_focus(DashboardFocusState::SettingsButton);
                true
            }
        }
    }

    fn handle_increment_decrement_value(&mut self, increment: bool) -> bool {
        if let Some(modal) = self.active_modal.as_mut() {
            if let ModalType::AgentSelection { options } = &mut modal.modal_type {
                // Find the currently selected filtered option
                let selected_filtered_option = modal.filtered_options.get(modal.selected_index);

                if let Some(FilteredOption::Option { text, .. }) = selected_filtered_option {
                    // Parse the model name from the text (format: "Model Name (xCOUNT)")
                    if let Some(model_name) = text.split(" (x").next().map(|s| s.trim()) {
                        // Find the model in options and increment/decrement its count
                        for option in options.iter_mut() {
                            if option.name == model_name {
                                if increment {
                                    option.count = option.count.saturating_add(1);
                                } else {
                                    option.count = option.count.saturating_sub(1);
                                }
                                option.is_selected = option.count > 0;

                                // Update the filtered options to reflect the new count, preserving the current filter
                                Self::update_model_selection_filtered_options(modal);

                                self.needs_redraw = true;
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// Update filtered options for search modals (repository/branch search)
    #[allow(dead_code)] // Unified search modal filtering retained; may replace duplicated logic below.
    fn update_search_modal_filtered_options(&mut self, modal: &mut ModalViewModel) {
        let all_options: &[String] = match self.modal_state {
            ModalState::RepositorySearch => &self.available_repositories,
            ModalState::BranchSearch => &self.available_branches,
            ModalState::ModelSearch => {
                // For model search, we need to convert ModelInfo to display names
                self.model_display_names_cache.get_or_insert_with(|| {
                    self.available_models.iter().map(|m| m.display_name()).collect()
                });
                self.model_display_names_cache.as_ref().unwrap()
            }
            _ => &[],
        };

        let query = modal.input_value.to_lowercase();
        let mut filtered: Vec<FilteredOption> = all_options
            .iter()
            .filter(|option| {
                if query.is_empty() {
                    true // Show all options when no query
                } else {
                    option.to_lowercase().contains(&query)
                }
            })
            .cloned()
            .map(|opt| FilteredOption::Option {
                text: opt,
                selected: false,
            })
            .collect();

        // Reset selected index if it's out of bounds
        if modal.selected_index >= filtered.len() && !filtered.is_empty() {
            modal.selected_index = 0;
        }

        // Mark the selected option
        if !filtered.is_empty() && modal.selected_index < filtered.len() {
            if let FilteredOption::Option { selected, .. } = &mut filtered[modal.selected_index] {
                *selected = true;
            }
        }

        modal.filtered_options = filtered;
    }

    /// Update filtered options for model selection, preserving the current filter
    fn update_model_selection_filtered_options(modal: &mut ModalViewModel) {
        if let ModalType::AgentSelection { options } = &modal.modal_type {
            let query = modal.input_value.to_lowercase();
            let mut filtered: Vec<FilteredOption> = Vec::new();

            // First, add models that match the filter
            let matching_count = options
                .iter()
                .filter(|option| {
                    if query.is_empty() {
                        true // Show all options when no query
                    } else {
                        option.name.to_lowercase().contains(&query)
                    }
                })
                .count();

            let matching_options: Vec<FilteredOption> = options
                .iter()
                .filter(|option| {
                    if query.is_empty() {
                        true // Show all options when no query
                    } else {
                        option.name.to_lowercase().contains(&query)
                    }
                })
                .map(|opt| FilteredOption::Option {
                    text: format!("{} (x{})", opt.name, opt.count),
                    selected: false,
                })
                .collect();

            filtered.extend(matching_options);

            // Then, add models that don't match but have non-zero counts
            if !query.is_empty() {
                let already_selected: Vec<FilteredOption> = options
                    .iter()
                    .filter(|option| {
                        !option.name.to_lowercase().contains(&query) && option.count > 0
                    })
                    .map(|opt| FilteredOption::Option {
                        text: format!("{} (x{})", opt.name, opt.count),
                        selected: false,
                    })
                    .collect();

                if !already_selected.is_empty() && matching_count > 0 {
                    // Add separator only if there are matching options above
                    filtered.push(FilteredOption::Separator {
                        label: Some("Already Selected".to_string()),
                    });
                    filtered.extend(already_selected);
                } else if !already_selected.is_empty() {
                    // If no matching options, just add the already selected ones without separator
                    filtered.extend(already_selected);
                }
            }

            // Reset selected index if it's out of bounds
            if modal.selected_index >= filtered.len() && !filtered.is_empty() {
                modal.selected_index = 0;
            }

            // Mark the selected option
            if !filtered.is_empty() && modal.selected_index < filtered.len() {
                if let FilteredOption::Option { selected, .. } = &mut filtered[modal.selected_index]
                {
                    *selected = true;
                }
            }

            modal.filtered_options = filtered;
        }
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
                    if let FilteredOption::Option { selected, .. } = option {
                        *selected = idx == modal.selected_index;
                    }
                }
                self.needs_redraw = true;
                true
            }
            ModalType::AgentSelection { options } => {
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
            ModalType::LaunchOptions { view_model } => {
                // Launch options modal - allow navigation through options
                if let Some(popup) = &mut view_model.inline_enum_popup {
                    // Navigate within the inline popup
                    match direction {
                        NavigationDirection::Next => {
                            popup.selected_index =
                                (popup.selected_index + 1).min(popup.options.len() - 1);
                        }
                        NavigationDirection::Previous => {
                            popup.selected_index = popup.selected_index.saturating_sub(1);
                        }
                    }
                } else {
                    match view_model.active_column {
                        LaunchOptionsColumn::Options => {
                            // Approximate count or use a safe upper bound
                            // Actual count is around 24.
                            let max_options = 30;
                            match direction {
                                NavigationDirection::Next => {
                                    view_model.selected_option_index =
                                        (view_model.selected_option_index + 1).min(max_options);
                                }
                                NavigationDirection::Previous => {
                                    view_model.selected_option_index =
                                        view_model.selected_option_index.saturating_sub(1);
                                }
                            }
                        }
                        LaunchOptionsColumn::Actions => {
                            let max_actions = 8; // 4 regular + 4 focus variants
                            match direction {
                                NavigationDirection::Next => {
                                    view_model.selected_action_index =
                                        (view_model.selected_action_index + 1).min(max_actions - 1);
                                }
                                NavigationDirection::Previous => {
                                    view_model.selected_action_index =
                                        view_model.selected_action_index.saturating_sub(1);
                                }
                            }
                        }
                    }
                }
                self.needs_redraw = true;
                true
            }
            ModalType::EnumSelection {
                options,
                selected_index,
                ..
            } => {
                if options.is_empty() {
                    return false;
                }
                match direction {
                    NavigationDirection::Next => {
                        *selected_index = (*selected_index + 1) % options.len();
                    }
                    NavigationDirection::Previous => {
                        if *selected_index == 0 {
                            *selected_index = options.len() - 1;
                        } else {
                            *selected_index -= 1;
                        }
                    }
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
            DashboardFocusState::DraftTask(idx) => self
                .draft_cards
                .get(idx)
                .map(|card| card.focus_element == CardFocusElement::TaskDescription)
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

    #[allow(dead_code)] // Superseded by specialized filtering; kept for consolidation later.
    fn update_modal_filtered_options(&mut self, modal: &mut ModalViewModel) {
        match &modal.modal_type {
            ModalType::Search { .. } => {
                // Get all available options based on modal state
                let all_options: &[String] = match self.modal_state {
                    ModalState::RepositorySearch => &self.available_repositories,
                    ModalState::BranchSearch => &self.available_branches,
                    ModalState::ModelSearch => {
                        // For model search, we need to convert ModelInfo to display names
                        // This is a temporary allocation - in the future we might want to cache this
                        self.model_display_names_cache.get_or_insert_with(|| {
                            self.available_models.iter().map(|m| m.display_name()).collect()
                        });
                        self.model_display_names_cache.as_ref().unwrap()
                    }
                    _ => &[],
                };

                // Filter options based on input value (case-insensitive fuzzy match)
                let query = modal.input_value.to_lowercase();
                let mut filtered: Vec<FilteredOption> = all_options
                    .iter()
                    .filter(|option| {
                        if query.is_empty() {
                            true // Show all options when no query
                        } else {
                            option.to_lowercase().contains(&query)
                        }
                    })
                    .cloned()
                    .map(|opt| FilteredOption::Option {
                        text: opt,
                        selected: false,
                    })
                    .collect();

                // Reset selected index if it's out of bounds
                if modal.selected_index >= filtered.len() && !filtered.is_empty() {
                    modal.selected_index = 0;
                }

                // Mark the selected option
                if !filtered.is_empty() && modal.selected_index < filtered.len() {
                    if let FilteredOption::Option { selected, .. } =
                        &mut filtered[modal.selected_index]
                    {
                        *selected = true;
                    }
                }

                modal.filtered_options = filtered;
            }
            ModalType::AgentSelection { options } => {
                // For model selection, filter the available model options
                let query = modal.input_value.to_lowercase();
                let mut filtered: Vec<FilteredOption> = options
                    .iter()
                    .filter(|option| {
                        if query.is_empty() {
                            true // Show all options when no query
                        } else {
                            option.name.to_lowercase().contains(&query)
                        }
                    })
                    .map(|opt| FilteredOption::Option {
                        text: format!("{} (x{})", opt.name, opt.count),
                        selected: false,
                    })
                    .collect();

                // Reset selected index if it's out of bounds
                if modal.selected_index >= filtered.len() && !filtered.is_empty() {
                    modal.selected_index = 0;
                }

                // Mark the selected option
                if !filtered.is_empty() && modal.selected_index < filtered.len() {
                    if let FilteredOption::Option { selected, .. } =
                        &mut filtered[modal.selected_index]
                    {
                        *selected = true;
                    }
                }

                modal.filtered_options = filtered;
            }
            ModalType::Settings { .. } => {
                // Settings don't have filtered options based on input
                modal.filtered_options = Vec::new();
            }
            ModalType::LaunchOptions { .. } => {
                // Launch options don't have filtered options based on input
                // The options are static
            }
            ModalType::EnumSelection { .. } => {
                // Enum selection doesn't use filtered_options
            }
        }
    }
    /// Navigate to the next focusable control
    pub fn focus_next_control(&mut self) -> bool {
        // Implement PRD-compliant tab navigation for draft cards
        match self.focus_element {
            DashboardFocusState::DraftTask(idx) => {
                // Handle tab navigation within the draft card
                if let Some(card) = self.draft_cards.get_mut(idx) {
                    match card.focus_element {
                        CardFocusElement::TaskDescription => {
                            card.focus_element = CardFocusElement::RepositorySelector;
                        }
                        CardFocusElement::BranchSelector => {
                            card.focus_element = CardFocusElement::ModelSelector;
                        }
                        CardFocusElement::RepositorySelector => {
                            card.focus_element = CardFocusElement::BranchSelector;
                        }
                        CardFocusElement::ModelSelector => {
                            card.focus_element = CardFocusElement::GoButton;
                        }
                        CardFocusElement::GoButton => {
                            card.focus_element = CardFocusElement::AdvancedOptionsButton;
                        }
                        CardFocusElement::AdvancedOptionsButton => {
                            card.focus_element = CardFocusElement::TaskDescription;
                        }
                    }
                    self.needs_redraw = true;
                    true
                } else {
                    false
                }
            }
            // For other global focus elements, handle normally
            DashboardFocusState::SettingsButton => {
                if !self.draft_cards.is_empty() {
                    self.change_focus(DashboardFocusState::DraftTask(0));
                    true
                } else if !self.task_cards.is_empty() {
                    self.change_focus(DashboardFocusState::FilterBarSeparator);
                    true
                } else {
                    false // Stay on settings if nothing else
                }
            }
            _ => {
                self.change_focus(DashboardFocusState::SettingsButton);
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
                    let all_options: &[String] = match self.modal_state {
                        ModalState::RepositorySearch => &self.available_repositories,
                        ModalState::BranchSearch => &self.available_branches,
                        ModalState::ModelSearch => {
                            // For model search, we need to convert ModelInfo to display names
                            self.model_display_names_cache.get_or_insert_with(|| {
                                self.available_models.iter().map(|m| m.display_name()).collect()
                            });
                            self.model_display_names_cache.as_ref().unwrap()
                        }
                        _ => &[],
                    };

                    let query = modal.input_value.to_lowercase();
                    let mut filtered: Vec<FilteredOption> = all_options
                        .iter()
                        .filter(|option| {
                            if query.is_empty() {
                                true // Show all options when no query
                            } else {
                                option.to_lowercase().contains(&query)
                            }
                        })
                        .cloned()
                        .map(|opt| FilteredOption::Option {
                            text: opt,
                            selected: false,
                        })
                        .collect();

                    // Reset selected index if it's out of bounds
                    if modal.selected_index >= filtered.len() && !filtered.is_empty() {
                        modal.selected_index = 0;
                    }

                    // Mark the selected option
                    if !filtered.is_empty() && modal.selected_index < filtered.len() {
                        if let FilteredOption::Option { selected, .. } =
                            &mut filtered[modal.selected_index]
                        {
                            *selected = true;
                        }
                    }

                    modal.filtered_options = filtered;
                    self.needs_redraw = true;
                    return true;
                }
                ModalType::AgentSelection { .. } => {
                    // Model selection modals use search input similar to search modals
                    modal.input_value.push(ch);

                    // Inline filtering logic to avoid double borrow
                    let query = modal.input_value.to_lowercase();
                    let mut filtered: Vec<FilteredOption> =
                        if let ModalType::AgentSelection { options } = &modal.modal_type {
                            let mut result = Vec::new();

                            // First, add models that match the filter
                            let matching_options: Vec<FilteredOption> = options
                                .iter()
                                .filter(|option| {
                                    if query.is_empty() {
                                        true // Show all options when no query
                                    } else {
                                        option.name.to_lowercase().contains(&query)
                                    }
                                })
                                .map(|opt| FilteredOption::Option {
                                    text: format!("{} (x{})", opt.name, opt.count),
                                    selected: false,
                                })
                                .collect();

                            result.extend(matching_options.clone());

                            // Then, add models that don't match but have non-zero counts
                            if !query.is_empty() {
                                let already_selected: Vec<FilteredOption> = options
                                    .iter()
                                    .filter(|option| {
                                        !option.name.to_lowercase().contains(&query)
                                            && option.count > 0
                                    })
                                    .map(|opt| FilteredOption::Option {
                                        text: format!("{} (x{})", opt.name, opt.count),
                                        selected: false,
                                    })
                                    .collect();

                                result.extend(already_selected);
                            }

                            result
                        } else {
                            Vec::new()
                        };

                    // Reset selected index if it's out of bounds
                    if modal.selected_index >= filtered.len() && !filtered.is_empty() {
                        modal.selected_index = 0;
                    }

                    // Mark the selected option (skip separators)
                    let mut selectable_index = 0;
                    for item in filtered.iter_mut() {
                        if let FilteredOption::Option { selected, .. } = item {
                            if selectable_index == modal.selected_index {
                                *selected = true;
                                break;
                            }
                            selectable_index += 1;
                        }
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
                ModalType::EnumSelection { .. } => {
                    // Enum selection doesn't handle character input
                    return false;
                }
                ModalType::LaunchOptions { view_model } => {
                    // Handle shortcut keys for launch options
                    let option = match ch.to_ascii_lowercase() {
                        't' => Some(if ch.is_uppercase() {
                            "Launch in new tab and focus (T)".to_string()
                        } else {
                            "Launch in new tab (t)".to_string()
                        }),
                        's' => Some(if ch.is_uppercase() {
                            "Launch in split view and focus (S)".to_string()
                        } else {
                            "Launch in split view (s)".to_string()
                        }),
                        'h' => Some(if ch.is_uppercase() {
                            "Launch in horizontal split and focus (H)".to_string()
                        } else {
                            "Launch in horizontal split (h)".to_string()
                        }),
                        'v' => Some(if ch.is_uppercase() {
                            "Launch in vertical split and focus (V)".to_string()
                        } else {
                            "Launch in vertical split (v)".to_string()
                        }),
                        _ => None, // Ignore other characters
                    };

                    if let Some(option_text) = option {
                        // Extract values before dropping the borrow
                        let draft_id = view_model.draft_id.clone();
                        let config = view_model.config.clone();

                        // Save the advanced options to the draft card BEFORE launching
                        // This ensures they are available when launch_task_with_option reads them
                        if let Some(card) = self.draft_cards.iter_mut().find(|c| c.id == draft_id) {
                            card.advanced_options = Some(config);
                            tracing::debug!(
                                "✅ Saved advanced options to draft card before launching via shortcut: {}",
                                draft_id
                            );
                        }

                        // Release borrow explicitly
                        let _ = modal;
                        self.launch_task_with_option(draft_id, option_text);
                        self.close_modal(true); // Close modal (config already saved above)

                        // Restore focus to TaskDescription after launching with shortcut
                        self.change_focus(DashboardFocusState::DraftTask(0));
                        if let Some(card) = self.draft_cards.get_mut(0) {
                            card.focus_element = CardFocusElement::TaskDescription;
                        }

                        return true;
                    }
                    return false;
                }
            }
        }

        // Allow text input when focused on draft-related elements
        if let DashboardFocusState::DraftTask(_) = self.focus_element {
            if let DashboardFocusState::DraftTask(0) = self.focus_element {
                if let Some(card) = self.draft_cards.get_mut(0) {
                    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
                    let key_event = KeyEvent::new(KeyCode::Char(ch), KeyModifiers::empty());
                    self.autocomplete.notify_text_input();
                    card.description.input(key_event);
                    self.autocomplete
                        .after_textarea_change(&card.description, &mut self.needs_redraw);
                }
                self.mark_draft_dirty(0);
                return true;
            } else if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                if let Some(card) = self.draft_cards.get_mut(idx) {
                    if card.focus_element == CardFocusElement::TaskDescription {
                        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
                        let key_event = KeyEvent::new(KeyCode::Char(ch), KeyModifiers::empty());
                        self.autocomplete.notify_text_input();
                        card.description.input(key_event);
                        self.autocomplete
                            .after_textarea_change(&card.description, &mut self.needs_redraw);
                        self.mark_draft_dirty(idx);
                        return true;
                    }
                }
            }
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
                        let all_options: &[String] = match self.modal_state {
                            ModalState::RepositorySearch => &self.available_repositories,
                            ModalState::BranchSearch => &self.available_branches,
                            ModalState::ModelSearch => {
                                // For model search, we need to convert ModelInfo to display names
                                self.model_display_names_cache.get_or_insert_with(|| {
                                    self.available_models.iter().map(|m| m.display_name()).collect()
                                });
                                self.model_display_names_cache.as_ref().unwrap()
                            }
                            _ => &[],
                        };

                        let query = modal.input_value.to_lowercase();
                        let mut filtered: Vec<FilteredOption> = all_options
                            .iter()
                            .filter(|option| {
                                if query.is_empty() {
                                    true // Show all options when no query
                                } else {
                                    option.to_lowercase().contains(&query)
                                }
                            })
                            .cloned()
                            .map(|opt| FilteredOption::Option {
                                text: opt,
                                selected: false,
                            })
                            .collect();

                        // Reset selected index if it's out of bounds
                        if modal.selected_index >= filtered.len() && !filtered.is_empty() {
                            modal.selected_index = 0;
                        }

                        // Mark the selected option
                        if !filtered.is_empty() && modal.selected_index < filtered.len() {
                            if let FilteredOption::Option { selected, .. } =
                                &mut filtered[modal.selected_index]
                            {
                                *selected = true;
                            }
                        }

                        modal.filtered_options = filtered;
                        self.needs_redraw = true;
                        return true;
                    }
                }
                ModalType::AgentSelection { .. } => {
                    // For model selection modals, remove last character from input value
                    if !modal.input_value.is_empty() {
                        modal.input_value.pop();
                        Self::update_model_selection_filtered_options(modal);
                        self.needs_redraw = true;
                        return true;
                    }
                }
                ModalType::LaunchOptions { .. } => {
                    // Launch options modal doesn't handle backspace
                    return false;
                }
                ModalType::EnumSelection { .. } => {
                    // Enum selection modal doesn't handle backspace
                    return false;
                }
                ModalType::Settings { .. } => {
                    // Settings modals don't handle backspace
                    return false;
                }
            }
        }

        // Handle backspace on draft cards
        if let DashboardFocusState::DraftTask(idx) = self.focus_element {
            if let Some(card) = self.draft_cards.get_mut(idx) {
                if card.focus_element == CardFocusElement::TaskDescription {
                    let key_event = KeyEvent::new(KeyCode::Backspace, KeyModifiers::empty());
                    return matches!(
                        self.handle_task_entry_operation(
                            idx,
                            KeyboardOperation::DeleteCharacterBackward,
                            &key_event
                        ),
                        KeyboardOperationResult::Handled
                    );
                }
            }
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
                        let all_options: &[String] = match self.modal_state {
                            ModalState::RepositorySearch => &self.available_repositories,
                            ModalState::BranchSearch => &self.available_branches,
                            ModalState::ModelSearch => {
                                // For model search, we need to convert ModelInfo to display names
                                self.model_display_names_cache.get_or_insert_with(|| {
                                    self.available_models.iter().map(|m| m.display_name()).collect()
                                });
                                self.model_display_names_cache.as_ref().unwrap()
                            }
                            _ => &[],
                        };

                        let query = modal.input_value.to_lowercase();
                        let mut filtered: Vec<FilteredOption> = all_options
                            .iter()
                            .filter(|option| {
                                if query.is_empty() {
                                    true // Show all options when no query
                                } else {
                                    option.to_lowercase().contains(&query)
                                }
                            })
                            .cloned()
                            .map(|opt| FilteredOption::Option {
                                text: opt,
                                selected: false,
                            })
                            .collect();

                        // Reset selected index if it's out of bounds
                        if modal.selected_index >= filtered.len() && !filtered.is_empty() {
                            modal.selected_index = 0;
                        }

                        // Mark the selected option
                        if !filtered.is_empty() && modal.selected_index < filtered.len() {
                            if let FilteredOption::Option { selected, .. } =
                                &mut filtered[modal.selected_index]
                            {
                                *selected = true;
                            }
                        }

                        modal.filtered_options = filtered;
                        self.needs_redraw = true;
                        return true;
                    }
                }
                ModalType::AgentSelection { .. } => {
                    // Model selection modals use search input similar to search modals
                    modal.input_value.pop();
                    Self::update_model_selection_filtered_options(modal);
                    self.needs_redraw = true;
                    return true;
                }
                ModalType::LaunchOptions { .. } => {
                    // Launch options modal doesn't handle delete
                    return false;
                }
                ModalType::EnumSelection { .. } => {
                    // Enum selection modal doesn't handle delete
                    return false;
                }
                ModalType::Settings { .. } => {
                    // Settings modals don't handle delete
                    return false;
                }
            }
        }

        // Delete handling is now done by early delegation to task_entry
        false
    }

    /// Handle enter key (including shift+enter for newlines)
    pub fn handle_enter(&mut self, shift: bool) -> bool {
        // Handle Enter on draft cards based on internal focus
        if let DashboardFocusState::DraftTask(idx) = self.focus_element {
            if let Some(card) = self.draft_cards.get(idx) {
                match card.focus_element {
                    CardFocusElement::TaskDescription => {
                        // Enter on TaskDescription: launch task (or add newline with Shift+Enter)
                        if shift {
                            // Shift+Enter: add newline to description
                            use crate::settings::KeyboardOperation;
                            use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

                            let key_event = KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT);
                            return self.handle_keyboard_operation(
                                KeyboardOperation::OpenNewLine,
                                &key_event,
                            );
                        } else {
                            // Regular Enter: launch task
                            use crate::settings::KeyboardOperation;
                            use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

                            let key_event = KeyEvent::new(KeyCode::Enter, KeyModifiers::empty());
                            return self.handle_keyboard_operation(
                                KeyboardOperation::IndentOrComplete,
                                &key_event,
                            );
                        }
                    }
                    CardFocusElement::RepositorySelector => {
                        self.open_modal(ModalState::RepositorySearch);
                        return true;
                    }
                    CardFocusElement::BranchSelector => {
                        self.open_modal(ModalState::BranchSearch);
                        return true;
                    }
                    CardFocusElement::ModelSelector => {
                        self.open_modal(ModalState::ModelSearch);
                        return true;
                    }
                    CardFocusElement::GoButton => {
                        let default_split_mode = self.settings.default_split_mode();

                        // Use advanced options from draft card, or defaults if not configured
                        let advanced_options = card
                            .advanced_options
                            .clone()
                            .or_else(|| Some(AdvancedLaunchOptions::default()));

                        tracing::info!(
                            "TUI direct go button launch: idx={}, split_mode={:?}, advanced_options={:?} (from config)",
                            idx,
                            default_split_mode,
                            advanced_options
                        );
                        return self.launch_task(
                            idx,
                            default_split_mode,
                            false,
                            None,
                            None,
                            advanced_options,
                        );
                    }
                    CardFocusElement::AdvancedOptionsButton => {
                        // Open advanced launch options modal
                        let draft_id = card.id.clone();
                        self.open_launch_options_modal(draft_id);
                        return true;
                    }
                }
            }
        }

        match self.focus_element {
            DashboardFocusState::SettingsButton => {
                self.open_modal(ModalState::Settings);
                true
            }
            _ => false,
        }
    }

    /// Launch task from the specified draft card
    pub fn launch_task(
        &mut self,
        draft_card_index: usize,
        split_mode: ah_core::SplitMode,
        focus: bool,
        starting_point: Option<ah_core::task_manager::StartingPoint>,
        working_copy_mode: Option<ah_core::WorkingCopyMode>,
        advanced_options: Option<AdvancedLaunchOptions>,
    ) -> bool {
        tracing::info!(
            "TUI launch_task: draft_card_index={}, split_mode={:?}, focus={}, advanced_options={}",
            draft_card_index,
            split_mode,
            focus,
            advanced_options.is_some()
        );

        // Get the specified draft card
        if let Some(card) = self.draft_cards.get(draft_card_index) {
            // Validate that description and models are provided
            let description = card.description.lines().join("\n");
            if description.trim().is_empty() {
                self.status_bar.error_message = Some("Task description is required".to_string());
                return false; // Validation failed
            }
            if card.selected_agents.is_empty() {
                self.status_bar.error_message =
                    Some("At least one AI model must be selected".to_string());
                return false; // Validation failed
            }

            // Determine agent type and model name
            let selected_agent = card.selected_agents.first().unwrap(); // We validated it's not empty
            let agent_type = selected_agent.agent.software.clone();
            let _model_name = selected_agent.model.clone(); // retained for future telemetry; currently unused

            // Extract card data before removing it
            let card_repository = card.repository.clone();
            let card_branch = card.branch.clone();
            let card_agents = card.selected_agents.clone();

            // Launch the task via task_manager
            let _task_manager = self.task_manager.clone(); // unused: launch uses builder directly
            let starting_point = starting_point.unwrap_or_else(|| {
                ah_core::task_manager::StartingPoint::RepositoryBranch {
                    repository: card_repository.clone(),
                    branch: card_branch.clone(),
                }
            });
            let working_copy_mode = working_copy_mode.unwrap_or(ah_core::WorkingCopyMode::InPlace);
            let mut builder = ah_core::task_manager::TaskLaunchParams::builder()
                .starting_point(starting_point)
                .working_copy_mode(working_copy_mode)
                .description(description.clone())
                .agents(card_agents.clone())
                .agent_type(agent_type)
                .split_mode(split_mode)
                .focus(focus)
                .record(true); // Enable recording by default

            // Add advanced launch options if provided
            if let Some(options) = &advanced_options {
                tracing::debug!(
                    "TUI launch_task: Applying advanced options to builder: {:?}",
                    options
                );
                builder = builder
                    .sandbox_profile(options.sandbox_profile.clone())
                    .fs_snapshots(options.fs_snapshots.clone())
                    .devcontainer_path(options.devcontainer_path.clone())
                    .allow_egress(options.allow_egress)
                    .allow_containers(options.allow_containers)
                    .allow_vms(options.allow_vms)
                    .allow_web_search(options.allow_web_search)
                    .interactive_mode(options.interactive_mode)
                    .output_format(options.output_format.clone())
                    .record_output(options.record_output)
                    .timeout(options.timeout.clone())
                    .llm_provider(options.llm_provider.clone())
                    .environment_variables(options.environment_variables.clone())
                    .delivery_method(options.delivery_method.clone())
                    .target_branch(options.target_branch.clone())
                    .create_task_files(options.create_task_files)
                    .create_metadata_commits(options.create_metadata_commits)
                    .notifications(options.notifications)
                    .labels(options.labels.clone())
                    .fleet(options.fleet.clone());
            } else {
                tracing::debug!("TUI launch_task: No advanced options to apply");
            }

            builder = builder.task_id(card.id.clone()); // Use the draft card's stable ID

            let params = match builder.build() {
                Ok(params) => params,
                Err(e) => {
                    self.status_bar.error_message = Some(format!("Invalid task parameters: {}", e));
                    return false;
                }
            };

            // Create TaskExecution for the launched task (assume success for now)
            let task_execution = TaskExecution {
                id: format!("task_{}", chrono::Utc::now().timestamp()), // Temporary ID, will be updated when launch completes
                repository: card_repository.clone(),
                branch: card_branch.clone(),
                agents: card_agents.clone(),
                state: TaskState::Queued,
                timestamp: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                activity: vec![],
                delivery_status: vec![],
            };

            // Add the task to the UI
            let task_card = create_task_card_from_execution(task_execution.clone(), &self.settings);
            let task_card_arc = Arc::new(Mutex::new(task_card));
            self.task_cards.push(task_card_arc.clone());

            // Launch the task asynchronously (only if runtime is available)
            if tokio::runtime::Handle::try_current().is_ok() {
                let task_manager_for_events = Arc::clone(&self.task_manager);
                let task_card_for_events: Arc<Mutex<TaskExecutionViewModel>> =
                    Arc::clone(&task_card_arc);
                let settings_for_events = self.settings.clone();

                tokio::spawn(async move {
                    match task_manager_for_events.launch_task(params).await {
                        ah_core::task_manager::TaskLaunchResult::Success { session_ids } => {
                            debug!("Task launched successfully: {:?}", session_ids);
                            // Handle events for all launched sessions
                            for session_id in session_ids {
                                let task_manager_for_session = Arc::clone(&task_manager_for_events);
                                let task_card_for_session = Arc::clone(&task_card_for_events);
                                let settings_for_session = settings_for_events.clone();

                                tokio::spawn(async move {
                                    tracing::info!(
                                        "Setting up event handling for session: {}",
                                        session_id
                                    );

                                    // Get the receiver for this session and start consuming events
                                    let mut receiver =
                                        task_manager_for_session.task_events_receiver(&session_id);

                                    // Start a loop to consume events and directly update the task card
                                    loop {
                                        match receiver.recv().await {
                                            Ok(event) => {
                                                debug!(
                                                    "Received event for session {}: {:?}",
                                                    session_id, event
                                                );
                                                // Directly update the task card
                                                if let Ok(mut task_card) =
                                                    task_card_for_session.lock()
                                                {
                                                    task_card.process_task_event(
                                                        event,
                                                        &settings_for_session,
                                                    );
                                                }
                                            }
                                            Err(
                                                tokio::sync::broadcast::error::RecvError::Closed,
                                            ) => {
                                                debug!(
                                                    "Event receiver closed for session {}",
                                                    session_id
                                                );
                                                break;
                                            }
                                            Err(
                                                tokio::sync::broadcast::error::RecvError::Lagged(_),
                                            ) => {
                                                debug!(
                                                    "Event receiver lagged for session {}, continuing",
                                                    session_id
                                                );
                                                continue;
                                            } // Removed unreachable catch-all Err(e) arm; specific variants handled above.
                                        }
                                    }
                                });
                            }
                        }
                        ah_core::task_manager::TaskLaunchResult::Failure { error } => {
                            tracing::error!("Task launch failed: {}", error);
                            // Update the task card to show failure state
                            if let Ok(mut task_card) = task_card_for_events.lock() {
                                use ah_domain_types::task::TaskState;
                                // Update the domain task state to completed (since it failed)
                                task_card.task.state = TaskState::Completed;
                                // Change card type to completed to show it as finished
                                task_card.card_type = TaskCardType::Completed {
                                    delivery_indicators: "Failed".to_string(),
                                };
                                // Update metadata to reflect failure
                                task_card.metadata.state = TaskState::Completed;
                                // Add failure message as activity entry
                                if let TaskCardType::Completed {
                                    ref mut delivery_indicators,
                                } = task_card.card_type
                                {
                                    *delivery_indicators = format!("Failed: {}", error);
                                }
                            }
                        }
                    }
                });
            }

            // Clear any previous error and show success
            self.status_bar.error_message = None;
            let message = match (split_mode, focus) {
                (ah_core::SplitMode::None, false) => "Task launched successfully",
                (_, false) => "Task launched in split view successfully",
                (ah_core::SplitMode::None, true) => "Task launched and focused successfully",
                (_, true) => "Task launched in split view and focused successfully",
            };
            self.status_bar.status_message = Some(message.to_string());

            // Save the advanced options before removing the card so we can preserve them
            let card_advanced_options = card.advanced_options.clone();

            // Remove the draft card that was just dispatched
            self.draft_cards.remove(draft_card_index);

            // If there are no draft cards left, create a new empty one
            if self.draft_cards.is_empty() {
                let empty_draft_task = DraftTask {
                    id: uuid::Uuid::new_v4().to_string(),
                    description: String::new(),
                    repository: card_repository, // Keep the same repo/branch for convenience
                    branch: card_branch,
                    selected_agents: card_agents, // Keep the same models
                    created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                };
                let mut new_card = create_draft_card_from_task(
                    empty_draft_task,
                    CardFocusElement::TaskDescription,
                    Some(self.repositories_enumerator.clone()),
                    Some(self.branches_enumerator.clone()),
                    Arc::clone(&self.workspace_files),
                    Arc::clone(&self.workspace_workflows),
                    Arc::clone(&self.workspace_terms),
                );
                // Preserve the advanced options from the previous draft card
                new_card.advanced_options = card_advanced_options;
                self.draft_cards.push(new_card);
            }

            // Adjust focus - since we removed index 0 and potentially added a new one at index 0,
            // the focus should remain on index 0
            self.change_focus(DashboardFocusState::DraftTask(0));

            return true; // Success
        }
        false
    }

    /// Launch a task with the selected launch option
    /// This function is used for ALL launch scenarios (keyboard shortcuts, modal actions, etc.)
    /// and ALWAYS uses the advanced options stored in the draft card
    fn launch_task_with_option(&mut self, draft_id: String, selected_option: String) {
        tracing::debug!(
            "TUI launch_task_with_option: draft_id={}, option={}",
            draft_id,
            selected_option
        );

        // Find the draft card by ID
        if let Some(draft_index) = self.draft_cards.iter().position(|card| card.id == draft_id) {
            // Parse the selected option to determine split mode and focus
            let (split_mode, focus) = match selected_option.as_str() {
                "Launch in new tab (t)" | "Go (Enter)" => (ah_core::SplitMode::None, false),
                "Launch in split view (s)" => (ah_core::SplitMode::Auto, false),
                "Launch in horizontal split (h)" => (ah_core::SplitMode::Horizontal, false),
                "Launch in vertical split (v)" => (ah_core::SplitMode::Vertical, false),
                "Launch in new tab and focus (T)" => (ah_core::SplitMode::None, true),
                "Launch in split view and focus (S)" => (ah_core::SplitMode::Auto, true),
                "Launch in horizontal split and focus (H)" => {
                    (ah_core::SplitMode::Horizontal, true)
                }
                "Launch in vertical split and focus (V)" => (ah_core::SplitMode::Vertical, true),
                _ => (ah_core::SplitMode::Auto, false), // Default fallback
            };

            // Save the user's split mode choice as the session default preference
            // (only for the current TUI session - not persisted to disk)
            self.settings.default_split_mode = Some(split_mode);
            tracing::info!(
                "🔧 TUI: Updated session split mode preference: {:?}",
                split_mode
            );

            // Get advanced options from the draft card, or use defaults if not configured
            // This ensures consistent behavior whether the user has opened the advanced options modal or not
            let advanced_options = self
                .draft_cards
                .get(draft_index)
                .and_then(|card| card.advanced_options.clone())
                .or_else(|| Some(AdvancedLaunchOptions::default()));

            // Launch the task with the determined split mode, focus, and advanced options from card
            self.launch_task(draft_index, split_mode, focus, None, None, advanced_options);
        }
    }

    /// Apply the selected option from a modal and close it
    pub fn apply_modal_selection(&mut self, modal_type: ModalType, selected_option: String) {
        match modal_type {
            ModalType::Search { .. } => match self.modal_state {
                ModalState::RepositorySearch => {
                    // Apply selected repository to the current draft card
                    if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                        if let Some(card) = self.draft_cards.get_mut(idx) {
                            card.repository = selected_option;
                            card.focus_element = CardFocusElement::TaskDescription;
                        }
                    }
                }
                ModalState::BranchSearch => {
                    // Apply selected branch to the current draft card
                    if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                        if let Some(card) = self.draft_cards.get_mut(idx) {
                            card.branch = selected_option;
                            card.focus_element = CardFocusElement::TaskDescription;
                        }
                    }
                }
                ModalState::ModelSearch => {
                    // Apply selected model to the current draft card
                    if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                        if let Some(card) = self.draft_cards.get_mut(idx) {
                            // Find the AgentChoice that matches this display name
                            if let Some(model) = self
                                .available_models
                                .iter()
                                .find(|model| model.display_name() == selected_option)
                            {
                                // Add the model to the card's model list
                                let selected_agent = AgentChoice {
                                    agent: model.agent.clone(),
                                    model: model.model.clone(),
                                    count: 1,
                                    settings: model.settings.clone(),
                                    display_name: model.display_name.clone(),
                                    acp_stdio_launch_command: model
                                        .acp_stdio_launch_command
                                        .clone(),
                                };
                                card.selected_agents = vec![selected_agent];
                                card.focus_element = CardFocusElement::TaskDescription;
                            }
                        }
                    }
                }
                _ => {} // Other modal types don't need selection handling
            },
            ModalType::AgentSelection { options } => {
                // Apply all selected models with their counts to the current draft card
                if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        // Convert AgentSelectionViewModel to AgentChoice, filtering out models with count 0
                        card.selected_agents = options
                            .into_iter()
                            .filter(|opt| opt.count > 0)
                            .filter_map(|opt| {
                                self.available_models
                                    .iter()
                                    .find(|model| model.display_name() == opt.name)
                                    .map(|model| AgentChoice {
                                        agent: model.agent.clone(),
                                        model: model.model.clone(),
                                        count: opt.count,
                                        settings: model.settings.clone(),
                                        display_name: model.display_name.clone(),
                                        acp_stdio_launch_command: model
                                            .acp_stdio_launch_command
                                            .clone(),
                                    })
                            })
                            .collect();
                        card.focus_element = CardFocusElement::TaskDescription;
                    }
                }
            }
            ModalType::LaunchOptions { view_model } => {
                // Launch the task with the selected options
                // Advanced options are already saved to the draft card when modal closes
                self.launch_task_with_option(view_model.draft_id.clone(), selected_option);
            }
            _ => {} // Other modal types don't need selection handling
        }

        // Close the modal and return focus to task description
        self.close_modal(true); // Applying selection - save changes
        self.change_focus(DashboardFocusState::DraftTask(0));
        if let Some(card) = self.draft_cards.get_mut(0) {
            card.focus_element = CardFocusElement::TaskDescription;
        }
    }

    /// Handle escape key
    pub fn handle_escape(&mut self) -> bool {
        self.handle_dismiss_overlay()
    }

    /// Handle Ctrl+N to create new draft task
    pub fn handle_ctrl_n(&mut self) -> bool {
        if !self.draft_cards.is_empty() {
            // Create a new draft task based on the first (current) draft
            if let Some(current_card) = self.draft_cards.first() {
                let new_card = self.create_draft_task(
                    String::new(), // empty description
                    current_card.repository.clone(),
                    current_card.branch.clone(),
                    current_card.selected_agents.clone(),
                    CardFocusElement::TaskDescription,
                );
                // Note: create_draft_task uses the ViewModel's workspace_files/workspace_workflows
                self.draft_cards.push(new_card);
                let new_index = self.draft_cards.len() - 1;
                self.change_focus(DashboardFocusState::DraftTask(new_index)); // Focus on the new draft task
                return true;
            }
        }
        false
    }

    /// Create a new draft task with the specified parameters
    pub fn create_draft_task(
        &self,
        description: String,
        repository: String,
        branch: String,
        models: Vec<AgentChoice>,
        focus_element: CardFocusElement,
    ) -> TaskEntryViewModel {
        Self::create_draft_task_internal(
            self.workspace_files.clone(),
            self.workspace_workflows.clone(),
            self.workspace_terms.clone(),
            description,
            repository,
            branch,
            models,
            focus_element,
            Some(self.repositories_enumerator.clone()),
            Some(self.branches_enumerator.clone()),
        )
    }

    /// Internal helper to create a draft task
    #[allow(clippy::too_many_arguments)] // Constructor aggregation: keeping explicit dependencies for clarity; consider Builder for future ergonomic refactor.
    fn create_draft_task_internal(
        workspace_files: Arc<dyn WorkspaceFilesEnumerator>,
        workspace_workflows: Arc<dyn WorkspaceWorkflowsEnumerator>,
        workspace_terms: Arc<dyn WorkspaceTermsEnumerator>,
        description: String,
        repository: String,
        branch: String,
        models: Vec<AgentChoice>,
        focus_element: CardFocusElement,
        repositories_enumerator: Option<Arc<dyn RepositoriesEnumerator>>,
        branches_enumerator: Option<Arc<dyn BranchesEnumerator>>,
    ) -> TaskEntryViewModel {
        let draft_task = DraftTask {
            id: uuid::Uuid::new_v4().to_string(),
            description,
            repository,
            branch,
            selected_agents: models,
            created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        };

        create_draft_card_from_task(
            draft_task,
            focus_element,
            repositories_enumerator,
            branches_enumerator,
            workspace_files,
            workspace_workflows,
            workspace_terms,
        )
    }

    pub fn handle_tick(&mut self) -> bool {
        let mut changed = false;
        let now = Instant::now();
        for idx in 0..self.draft_cards.len() {
            if self.try_start_auto_save(idx, now) {
                changed = true;
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
    ActivateAdvancedOptionsModal,
    ActivateRepositoryModal,
    ActivateBranchModal,
    ActivateModelModal,
    LaunchTask,
    StopTask(usize),
    OpenSettings,
    EditFilter(FilterControl),
    Footer(FooterAction),
    AutocompleteSelect(usize),
    ModelIncrementCount(usize),
    ModelDecrementCount(usize),
    ModalSelectOption(usize),
    ModalApplyChanges,
    ModalCancelChanges,
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
    /// Mouse drag within a text area (for text selection)
    MouseDrag {
        column: u16,
        row: u16,
        bounds: ratatui::layout::Rect,
    },
    /// Mouse button released
    MouseUp { column: u16, row: u16 },
    /// Mouse scroll upwards (equivalent to navigating up)
    MouseScrollUp,
    /// Mouse scroll downwards (equivalent to navigating down)
    MouseScrollDown,
    /// Auto-save completion for draft tasks
    DraftSaveCompleted {
        draft_id: String,
        request_id: u64,
        generation: u64,
        result: SaveDraftResult,
    },
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

#[derive(Debug, Clone, PartialEq)]
pub enum LaunchOptionsColumn {
    Options,
    Actions,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AdvancedLaunchOptions {
    // Sandbox & Environment
    pub sandbox_profile: String,
    pub working_copy_mode: String,
    pub fs_snapshots: String,
    pub devcontainer_path: String,
    pub allow_egress: bool,
    pub allow_containers: bool,
    pub allow_vms: bool,
    pub allow_web_search: bool,

    // Agent Configuration
    pub interactive_mode: bool,
    pub output_format: String,
    pub record_output: bool,
    pub timeout: String,
    pub llm_provider: String,
    pub environment_variables: Vec<(String, String)>,

    // Task Management
    pub delivery_method: String,
    pub target_branch: String,
    pub create_task_files: bool,
    pub create_metadata_commits: bool,
    pub notifications: bool,
    pub labels: Vec<(String, String)>,
    pub fleet: String,
}

impl Default for AdvancedLaunchOptions {
    fn default() -> Self {
        Self {
            sandbox_profile: "disabled".to_string(),
            working_copy_mode: "auto".to_string(),
            fs_snapshots: "auto".to_string(),
            devcontainer_path: "".to_string(),
            allow_egress: false,
            allow_containers: false,
            allow_vms: false,
            allow_web_search: false,
            interactive_mode: true,
            output_format: "text".to_string(),
            record_output: false,
            timeout: "".to_string(),
            llm_provider: "".to_string(),
            environment_variables: vec![],
            delivery_method: "pr".to_string(),
            target_branch: "".to_string(),
            create_task_files: true,
            create_metadata_commits: true,
            notifications: true,
            labels: vec![],
            fleet: "default".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct InlineEnumPopup {
    pub options: Vec<String>,
    pub selected_index: usize,
    pub option_index: usize, // The option index this popup is for
}

#[derive(Debug, Clone, PartialEq)]
pub struct LaunchOptionsViewModel {
    pub draft_id: String,
    pub config: AdvancedLaunchOptions,
    pub original_config: AdvancedLaunchOptions, // Store original config to restore on Esc
    pub active_column: LaunchOptionsColumn,
    pub selected_option_index: usize,
    pub selected_action_index: usize,
    pub inline_enum_popup: Option<InlineEnumPopup>,
}

/// Modal dialog view models
#[derive(Debug, Clone, PartialEq)]
pub struct ModalViewModel {
    pub title: String,
    pub input_value: String,
    pub filtered_options: Vec<FilteredOption>,
    pub selected_index: usize,
    pub modal_type: ModalType,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum ModalType {
    Search {
        placeholder: String,
    },
    AgentSelection {
        options: Vec<AgentSelectionViewModel>,
    },
    Settings {
        fields: Vec<SettingsFieldViewModel>,
    },
    LaunchOptions {
        view_model: LaunchOptionsViewModel,
    },
    EnumSelection {
        title: String,
        options: Vec<String>,
        selected_index: usize,
        original_option_index: usize, // Index in the launch options config
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentSelectionViewModel {
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
    /// Current UI focus state. DO NOT modify directly - use change_focus() instead
    /// to ensure footer and other dependent state is updated correctly.
    pub focus_element: DashboardFocusState,

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

    // TUI configuration
    pub tui_config: crate::tui_config::TuiConfig,

    // UI state (moved from Model)
    pub modal_state: ModalState,
    pub search_mode: SearchMode,
    pub word_wrap_enabled: bool,
    pub show_autocomplete_border: bool,
    pub status_message: Option<String>,
    pub error_message: Option<String>,

    // Cursor style state
    pub cursor_style: crossterm::cursor::SetCursorStyle,

    // Escape handling state
    pub exit_confirmation_armed: bool,
    pub exit_requested: bool,

    // Mouse click timing state for multi-click detection
    pub last_click_time: Option<std::time::Instant>,
    pub last_click_position: Option<(u16, u16)>,
    pub click_count: u8,

    // Mouse drag state for text selection
    pub is_dragging: bool,
    pub drag_start_position: Option<(u16, u16)>,
    pub drag_start_bounds: Option<ratatui::layout::Rect>,

    // Loading states (moved from Model)
    pub loading_task_creation: bool,
    pub loading_repositories: bool,
    pub loading_branches: bool,
    pub loading_models: bool,

    // Service dependencies
    pub workspace_files: Arc<dyn WorkspaceFilesEnumerator>,
    pub workspace_workflows: Arc<dyn WorkspaceWorkflowsEnumerator>,
    pub workspace_terms: Arc<dyn WorkspaceTermsEnumerator>,
    pub task_manager: Arc<dyn TaskManager>, // Task launching abstraction
    pub repositories_enumerator: Arc<dyn RepositoriesEnumerator>,
    pub branches_enumerator: Arc<dyn BranchesEnumerator>,
    pub agents_enumerator: Arc<dyn ah_core::AgentsEnumerator>,

    // Autocomplete system
    pub autocomplete: InlineAutocomplete,

    // Domain state - available options
    pub available_repositories: Vec<String>,
    pub available_branches: Vec<String>,
    pub available_models: Vec<AgentChoice>,

    // Cache for model display names (to avoid repeated allocations)
    model_display_names_cache: Option<Vec<String>>,

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
    pub repositories_sender: Option<oneshot::Sender<Vec<String>>>,
    pub branches_sender: Option<oneshot::Sender<Vec<String>>>,
    pub agents_sender: Option<oneshot::Sender<Vec<AgentChoice>>>,
    pub files_receiver: Option<oneshot::Receiver<Vec<String>>>,
    pub workflows_receiver: Option<oneshot::Receiver<Vec<String>>>,
    pub repositories_receiver: Option<oneshot::Receiver<Vec<String>>>,
    pub branches_receiver: Option<oneshot::Receiver<Vec<String>>>,
    pub agents_receiver: Option<oneshot::Receiver<Vec<AgentChoice>>>,
    save_request_counter: u64,
    ui_tx: UiSender<Msg>,

    // Task collections - cards contain the domain objects
    pub draft_cards: Vec<TaskEntryViewModel>, // Draft tasks (editable)
    pub task_cards: Vec<Arc<Mutex<TaskExecutionViewModel>>>, // Regular tasks (active/completed/merged)

    // UI interaction state
    pub selected_card: usize,
    pub last_textarea_area: Option<ratatui::layout::Rect>, // Last rendered textarea area for caret positioning

    pub needs_redraw: bool, // Flag to indicate when UI needs to be redrawn

    // Temporary storage for launch options modal state during enum selection
    temp_launch_options: Option<LaunchOptionsViewModel>,

    _pending_bubbled_operation: Option<KeyboardOperation>, // underscore: reserved for future bubbling chain handling
    _bubbled_operation_consumed: bool, // underscore: reserved for future bubbling chain handling
}

impl ViewModel {
    /// Create a new ViewModel with service dependencies and start background loading
    /// Create a new ViewModel without background loading (for tests)
    #[allow(clippy::too_many_arguments)] // Public constructor: high fan-in of core enumerators; grouping would obscure dependency semantics.
    pub fn new(
        workspace_files: Arc<dyn WorkspaceFilesEnumerator>,
        workspace_workflows: Arc<dyn WorkspaceWorkflowsEnumerator>,
        workspace_terms: Arc<dyn WorkspaceTermsEnumerator>,
        task_manager: Arc<dyn TaskManager>,
        repositories_enumerator: Arc<dyn RepositoriesEnumerator>,
        branches_enumerator: Arc<dyn BranchesEnumerator>,
        agents_enumerator: Arc<dyn ah_core::AgentsEnumerator>,
        settings: Settings,
        tui_config: crate::tui_config::TuiConfig,
        ui_tx: UiSender<Msg>,
    ) -> Self {
        Self::new_internal(
            workspace_files,
            workspace_workflows,
            workspace_terms,
            task_manager,
            repositories_enumerator,
            branches_enumerator,
            agents_enumerator,
            settings,
            tui_config,
            false,
            None,
            ui_tx,
        )
    }

    /// Create a new ViewModel with background loading enabled
    #[allow(clippy::too_many_arguments)] // Variant constructor adds repo context; accepted for now; candidate for Builder pattern.
    pub fn new_with_background_loading_and_current_repo(
        workspace_files: Arc<dyn WorkspaceFilesEnumerator>,
        workspace_workflows: Arc<dyn WorkspaceWorkflowsEnumerator>,
        workspace_terms: Arc<dyn WorkspaceTermsEnumerator>,
        task_manager: Arc<dyn TaskManager>,
        repositories_enumerator: Arc<dyn RepositoriesEnumerator>,
        branches_enumerator: Arc<dyn BranchesEnumerator>,
        agents_enumerator: Arc<dyn ah_core::AgentsEnumerator>,
        settings: Settings,
        tui_config: crate::tui_config::TuiConfig,
        current_repository: Option<String>,
        ui_tx: UiSender<Msg>,
    ) -> Self {
        Self::new_internal(
            workspace_files,
            workspace_workflows,
            workspace_terms,
            task_manager,
            repositories_enumerator,
            branches_enumerator,
            agents_enumerator,
            settings,
            tui_config,
            true,
            current_repository,
            ui_tx,
        )
    }

    #[allow(clippy::too_many_arguments)] // Internal shared initializer; parameter list mirrors public constructors for consistency.
    fn new_internal(
        workspace_files: Arc<dyn WorkspaceFilesEnumerator>,
        workspace_workflows: Arc<dyn WorkspaceWorkflowsEnumerator>,
        workspace_terms: Arc<dyn WorkspaceTermsEnumerator>,
        task_manager: Arc<dyn TaskManager>,
        repositories_enumerator: Arc<dyn RepositoriesEnumerator>,
        branches_enumerator: Arc<dyn BranchesEnumerator>,
        agents_enumerator: Arc<dyn ah_core::AgentsEnumerator>,
        settings: Settings,
        tui_config: crate::tui_config::TuiConfig,
        with_background_loading: bool,
        current_repository: Option<String>,
        ui_tx: UiSender<Msg>,
    ) -> Self {
        // Initialize available options (will be populated asynchronously)
        let available_repositories = vec![];
        let available_branches = vec![];
        let available_models = vec![]; // Start empty, will be populated from enumerator

        // Initialize preloaded data and loading state
        let preloaded_files = vec![];
        let preloaded_workflows = vec![];
        let loading_files = false;
        let loading_workflows = false;
        let files_loaded = false;
        let workflows_loaded = false;

        // Create communication channels for background loading (only if enabled)
        let (
            files_sender,
            files_receiver,
            workflows_sender,
            workflows_receiver,
            repositories_sender,
            repositories_receiver,
            branches_sender,
            branches_receiver,
            agents_sender,
            agents_receiver,
        ) = if with_background_loading {
            let (files_sender, files_receiver) = oneshot::channel();
            let (workflows_sender, workflows_receiver) = oneshot::channel();
            let (repositories_sender, repositories_receiver) = oneshot::channel();
            let (branches_sender, branches_receiver) = oneshot::channel();
            let (agents_sender, agents_receiver) = oneshot::channel();
            (
                Some(files_sender),
                Some(files_receiver),
                Some(workflows_sender),
                Some(workflows_receiver),
                Some(repositories_sender),
                Some(repositories_receiver),
                Some(branches_sender),
                Some(branches_receiver),
                Some(agents_sender),
                Some(agents_receiver),
            )
        } else {
            (None, None, None, None, None, None, None, None, None, None)
        };

        // Determine initial focus element per PRD: "The initially focused element is the top draft task card."
        let initial_global_focus = DashboardFocusState::DraftTask(0); // Focus on the single draft task
        let initial_card_focus = CardFocusElement::TaskDescription; // Initially focus the text area within the card

        // Create initial draft card
        let default_agents = settings.default_agents.clone().unwrap_or_else(|| {
            vec![AgentChoice {
                agent: AgentSoftwareBuild {
                    software: AgentSoftware::Claude,
                    version: "latest".to_string(),
                },
                model: "sonnet".to_string(),
                count: 1,
                settings: std::collections::HashMap::new(),
                display_name: Some("Claude Sonnet".to_string()),
                acp_stdio_launch_command: None,
            }]
        });
        let draft_cards = vec![Self::create_draft_task_internal(
            Arc::clone(&workspace_files),
            Arc::clone(&workspace_workflows),
            Arc::clone(&workspace_terms),
            String::new(), // empty description
            current_repository.unwrap_or_else(|| "blocksense/agent-harbor".to_string()),
            "main".to_string(),
            default_agents,
            initial_card_focus,
            Some(Arc::clone(&repositories_enumerator)),
            Some(Arc::clone(&branches_enumerator)),
        )];
        let task_cards = vec![]; // Start with no task cards

        // Extract the initial draft task for modal initialization
        let initial_draft_task = DraftTask {
            id: draft_cards[0].id.clone(),
            description: draft_cards[0].description.lines().join("\n"),
            repository: draft_cards[0].repository.clone(),
            branch: draft_cards[0].branch.clone(),
            selected_agents: draft_cards[0].selected_agents.clone(),
            created_at: draft_cards[0].created_at.clone(),
        };
        let active_modal = create_modal_view_model(
            ModalState::None,
            &available_repositories,
            &available_branches,
            &available_models,
            &Some(initial_draft_task.clone()),
            settings.activity_rows(),
            true,
            false,
        );
        let footer = create_footer_view_model(
            Some(&initial_draft_task.clone()),
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
            .filter_map(|card: &Arc<Mutex<TaskExecutionViewModel>>| card.lock().ok())
            .map(|card| card.height + 1) // +1 for spacer
            .sum::<u16>()
            + 1; // Filter bar height

        let mut viewmodel = ViewModel {
            focus_element: initial_global_focus,

            // Domain state
            available_repositories: available_repositories.clone(),
            available_branches: available_branches.clone(),
            available_models: available_models.clone(),
            model_display_names_cache: None,

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
            repositories_sender,
            branches_sender,
            agents_sender,
            files_receiver,
            workflows_receiver,
            repositories_receiver,
            branches_receiver,
            agents_receiver,
            save_request_counter: 0,
            ui_tx: ui_tx.clone(),

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

            // TUI configuration
            tui_config: tui_config.clone(),

            // Initialize UI state with defaults (moved from Model)
            modal_state: ModalState::None,
            search_mode: SearchMode::None,
            word_wrap_enabled: true,
            show_autocomplete_border: false,
            status_message: None,
            error_message: None,
            cursor_style: crossterm::cursor::SetCursorStyle::SteadyBar,
            exit_confirmation_armed: false,
            exit_requested: false,

            // Mouse click timing state for multi-click detection
            last_click_time: None,
            last_click_position: None,
            click_count: 0,

            // Mouse drag state for text selection
            is_dragging: false,
            drag_start_position: None,
            drag_start_bounds: None,

            // Initialize loading states
            loading_task_creation: false,
            loading_repositories: false,
            loading_branches: false,
            loading_models: false,

            // Service dependencies
            workspace_files: workspace_files.clone(),
            workspace_workflows: workspace_workflows.clone(),
            workspace_terms: workspace_terms.clone(),
            task_manager: task_manager.clone(),
            repositories_enumerator: repositories_enumerator.clone(),
            branches_enumerator: branches_enumerator.clone(),
            agents_enumerator: agents_enumerator.clone(),

            // Autocomplete system
            autocomplete: {
                let deps = Arc::new(crate::view_model::autocomplete::AutocompleteDependencies {
                    workspace_files: workspace_files.clone(),
                    workspace_workflows: workspace_workflows.clone(),
                    workspace_terms: workspace_terms.clone(),
                    settings: settings.clone(),
                });
                InlineAutocomplete::with_dependencies(deps)
            },

            needs_redraw: true,
            temp_launch_options: None,
            _pending_bubbled_operation: None,
            _bubbled_operation_consumed: false,
        };

        // Sync cursor style for initial focus on textarea
        viewmodel.sync_cursor_style_for_focused_textarea();

        viewmodel
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
                if matches!(key_event.kind, KeyEventKind::Press | KeyEventKind::Repeat)
                    && self.handle_key_event(key_event)
                {
                    self.needs_redraw = true;
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
            Msg::MouseDrag {
                column,
                row,
                bounds,
            } => {
                if self.handle_mouse_drag(column, row, &bounds) {
                    self.needs_redraw = true;
                }
            }
            Msg::MouseUp { column, row } => {
                if self.handle_mouse_up(column, row) {
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
            Msg::DraftSaveCompleted {
                draft_id,
                request_id,
                generation,
                result,
            } => {
                self.apply_save_result(draft_id, request_id, generation, result);
            }
            Msg::Tick => {
                // Handle periodic updates (activity simulation, etc.)
                let had_activity_changes = self.update_active_task_activities();

                // Autocomplete updates are handled synchronously now

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

    fn focused_textarea_index(&self) -> Option<usize> {
        match self.focus_element {
            DashboardFocusState::DraftTask(idx) => self.draft_cards.get(idx).and_then(|card| {
                if card.focus_element == CardFocusElement::TaskDescription {
                    Some(idx)
                } else {
                    None
                }
            }),
            _ => None,
        }
    }

    fn focused_draft_buttons_index(&self) -> Option<usize> {
        match self.focus_element {
            DashboardFocusState::DraftTask(idx) => {
                self.draft_cards.get(idx).and_then(|card| match card.focus_element {
                    CardFocusElement::RepositorySelector
                    | CardFocusElement::BranchSelector
                    | CardFocusElement::ModelSelector
                    | CardFocusElement::GoButton => Some(idx),
                    _ => None,
                })
            }
            _ => None,
        }
    }

    fn handle_task_entry_operation(
        &mut self,
        draft_index: usize,
        operation: KeyboardOperation,
        key_event: &KeyEvent,
    ) -> KeyboardOperationResult {
        if let Some(card) = self.draft_cards.get_mut(draft_index) {
            match card.handle_keyboard_operation(operation, key_event, &mut self.needs_redraw) {
                KeyboardOperationResult::Handled => {
                    if card.focus_element == CardFocusElement::TaskDescription {
                        self.autocomplete
                            .after_textarea_change(&card.description, &mut self.needs_redraw);
                    }
                    KeyboardOperationResult::Handled
                }
                KeyboardOperationResult::NotHandled => {
                    // Handle button activation that wasn't handled by the card
                    if matches!(operation, KeyboardOperation::ActivateCurrentItem) {
                        match card.focus_element {
                            CardFocusElement::RepositorySelector => {
                                self.open_modal(ModalState::RepositorySearch);
                                KeyboardOperationResult::Handled
                            }
                            CardFocusElement::BranchSelector => {
                                self.open_modal(ModalState::BranchSearch);
                                KeyboardOperationResult::Handled
                            }
                            CardFocusElement::ModelSelector => {
                                self.open_modal(ModalState::ModelSearch);
                                KeyboardOperationResult::Handled
                            }
                            CardFocusElement::GoButton => {
                                let default_split_mode = self.settings.default_split_mode();
                                // Use advanced options from draft card, or defaults if not configured
                                let advanced_options = card
                                    .advanced_options
                                    .clone()
                                    .or_else(|| Some(AdvancedLaunchOptions::default()));
                                // Launch the task
                                if self.launch_task(
                                    draft_index,
                                    default_split_mode,
                                    false,
                                    None,
                                    None,
                                    advanced_options,
                                ) {
                                    KeyboardOperationResult::Handled
                                } else {
                                    KeyboardOperationResult::NotHandled
                                }
                            }
                            CardFocusElement::AdvancedOptionsButton => {
                                // Open advanced launch options modal
                                trace!(
                                    "handle_task_entry_operation: ActivateCurrentItem on AdvancedOptionsButton"
                                );
                                let draft_id = card.id.clone();
                                self.open_launch_options_modal(draft_id);
                                KeyboardOperationResult::Handled
                            }
                            CardFocusElement::TaskDescription => {
                                // This should have been handled by the card
                                KeyboardOperationResult::NotHandled
                            }
                        }
                    } else {
                        // Handle operations that should fall back to dashboard level
                        match operation {
                            KeyboardOperation::MoveToNextField => {
                                if self.autocomplete.is_open() && self.autocomplete.select_next() {
                                    self.needs_redraw = true;
                                    return KeyboardOperationResult::Handled;
                                }
                                if self.focus_next_control() {
                                    KeyboardOperationResult::Handled
                                } else {
                                    KeyboardOperationResult::NotHandled
                                }
                            }
                            KeyboardOperation::MoveToPreviousField => {
                                if self.autocomplete.is_open()
                                    && self.autocomplete.select_previous()
                                {
                                    self.needs_redraw = true;
                                    return KeyboardOperationResult::Handled;
                                }
                                if self.focus_previous_control() {
                                    KeyboardOperationResult::Handled
                                } else {
                                    KeyboardOperationResult::NotHandled
                                }
                            }
                            KeyboardOperation::MoveForwardOneCharacter => {
                                // Handle right arrow for draft card navigation
                                if self.focus_next_control() {
                                    KeyboardOperationResult::Handled
                                } else {
                                    KeyboardOperationResult::NotHandled
                                }
                            }
                            KeyboardOperation::MoveBackwardOneCharacter => {
                                // Handle left arrow for draft card navigation
                                if self.focus_previous_control() {
                                    KeyboardOperationResult::Handled
                                } else {
                                    KeyboardOperationResult::NotHandled
                                }
                            }
                            _ => KeyboardOperationResult::NotHandled,
                        }
                    }
                }
                KeyboardOperationResult::Bubble {
                    operation: bubbled_operation,
                } => {
                    match bubbled_operation {
                        KeyboardOperation::ActivateCurrentItem => {
                            // Handle bubbled activation from textarea, Go button, or Advanced Options button
                            if card.focus_element == CardFocusElement::TaskDescription
                                || card.focus_element == CardFocusElement::GoButton
                            {
                                let default_split_mode = self.settings.default_split_mode();
                                // Use advanced options from draft card, or defaults if not configured
                                let advanced_options = card
                                    .advanced_options
                                    .clone()
                                    .or_else(|| Some(AdvancedLaunchOptions::default()));
                                if self.launch_task(
                                    draft_index,
                                    default_split_mode,
                                    false,
                                    None,
                                    None,
                                    advanced_options,
                                ) {
                                    KeyboardOperationResult::Handled
                                } else {
                                    KeyboardOperationResult::NotHandled
                                }
                            } else if card.focus_element == CardFocusElement::AdvancedOptionsButton
                            {
                                // Open advanced launch options modal
                                trace!(
                                    "handle_task_entry_operation: bubbled ActivateCurrentItem on AdvancedOptionsButton"
                                );
                                let draft_id = card.id.clone();
                                self.open_launch_options_modal(draft_id);
                                KeyboardOperationResult::Handled
                            } else {
                                KeyboardOperationResult::NotHandled
                            }
                        }
                        _ => KeyboardOperationResult::Bubble {
                            operation: bubbled_operation,
                        },
                    }
                }
                KeyboardOperationResult::TaskLaunched {
                    split_mode,
                    focus,
                    starting_point,
                    working_copy_mode,
                } => {
                    // Use advanced options from draft card, or defaults if not configured
                    let advanced_options = card
                        .advanced_options
                        .clone()
                        .or_else(|| Some(AdvancedLaunchOptions::default()));
                    if self.launch_task(
                        draft_index,
                        split_mode,
                        focus,
                        starting_point,
                        working_copy_mode,
                        advanced_options,
                    ) {
                        KeyboardOperationResult::Handled
                    } else {
                        KeyboardOperationResult::NotHandled
                    }
                }
            }
        } else {
            KeyboardOperationResult::NotHandled
        }
    }

    fn handle_bubbled_operation(
        &mut self,
        bubbled_operation: KeyboardOperation,
        key_event: &KeyEvent,
    ) -> bool {
        self.handle_dashboard_operation(bubbled_operation, key_event)
    }

    fn handle_modal_operation(
        &mut self,
        operation: KeyboardOperation,
        _key_event: &KeyEvent,
    ) -> bool {
        if self.modal_state == ModalState::None {
            return false;
        }

        match operation {
            KeyboardOperation::MoveToNextLine | KeyboardOperation::MoveToNextField => {
                // Special handling for LaunchOptions column switching
                if let Some(modal) = self.active_modal.as_mut() {
                    if let ModalType::LaunchOptions { view_model } = &mut modal.modal_type {
                        if matches!(operation, KeyboardOperation::MoveToNextField) {
                            // Tab cycle: Options -> Actions -> Options
                            view_model.active_column = match view_model.active_column {
                                LaunchOptionsColumn::Options => LaunchOptionsColumn::Actions,
                                LaunchOptionsColumn::Actions => LaunchOptionsColumn::Options,
                            };
                            self.needs_redraw = true;
                            return true;
                        } else if let KeyboardOperation::MoveToNextLine = operation {
                            // Down arrow
                            return self.handle_modal_navigation(NavigationDirection::Next);
                        }
                    }
                }
                self.handle_modal_navigation(NavigationDirection::Next)
            }
            KeyboardOperation::MoveToPreviousLine | KeyboardOperation::MoveToPreviousField => {
                // Special handling for LaunchOptions column switching
                if let Some(modal) = self.active_modal.as_mut() {
                    if let ModalType::LaunchOptions { view_model } = &mut modal.modal_type {
                        if matches!(operation, KeyboardOperation::MoveToPreviousField) {
                            // Shift+Tab cycle: Actions -> Options -> Actions
                            view_model.active_column = match view_model.active_column {
                                LaunchOptionsColumn::Options => LaunchOptionsColumn::Actions,
                                LaunchOptionsColumn::Actions => LaunchOptionsColumn::Options,
                            };
                            self.needs_redraw = true;
                            return true;
                        } else if let KeyboardOperation::MoveToPreviousLine = operation {
                            // Up arrow
                            return self.handle_modal_navigation(NavigationDirection::Previous);
                        }
                    }
                }
                self.handle_modal_navigation(NavigationDirection::Previous)
            }
            KeyboardOperation::MoveForwardOneCharacter => {
                // Right arrow: Switch to Actions column if in LaunchOptions
                if let Some(modal) = self.active_modal.as_mut() {
                    if let ModalType::LaunchOptions { view_model } = &mut modal.modal_type {
                        if view_model.active_column == LaunchOptionsColumn::Options {
                            view_model.active_column = LaunchOptionsColumn::Actions;
                            self.needs_redraw = true;
                            return true;
                        }
                    }
                }
                false
            }
            KeyboardOperation::MoveBackwardOneCharacter => {
                // Left arrow: Switch to Options column if in LaunchOptions
                if let Some(modal) = self.active_modal.as_mut() {
                    if let ModalType::LaunchOptions { view_model } = &mut modal.modal_type {
                        if view_model.active_column == LaunchOptionsColumn::Actions {
                            view_model.active_column = LaunchOptionsColumn::Options;
                            self.needs_redraw = true;
                            return true;
                        }
                    }
                }
                false
            }
            KeyboardOperation::ActivateCurrentItem => {
                if let Some(modal) = self.active_modal.as_mut() {
                    match &mut modal.modal_type {
                        ModalType::LaunchOptions { view_model } => {
                            // Handle inline popup activation first
                            if let Some(popup) = &view_model.inline_enum_popup {
                                if let Some(selected_value) =
                                    popup.options.get(popup.selected_index)
                                {
                                    // Apply the selected value to the config
                                    match popup.option_index {
                                        1 => {
                                            view_model.config.sandbox_profile =
                                                selected_value.clone()
                                        }
                                        2 => {
                                            view_model.config.working_copy_mode =
                                                selected_value.clone()
                                        }
                                        3 => {
                                            view_model.config.fs_snapshots = selected_value.clone()
                                        }
                                        11 => {
                                            view_model.config.output_format = selected_value.clone()
                                        }
                                        17 => {
                                            view_model.config.delivery_method =
                                                selected_value.clone()
                                        }
                                        23 => view_model.config.fleet = selected_value.clone(),
                                        _ => {}
                                    }
                                    // Close the popup
                                    view_model.inline_enum_popup = None;
                                    self.needs_redraw = true;
                                    return true;
                                }
                                false // Invalid popup selection
                            } else if view_model.active_column == LaunchOptionsColumn::Options {
                                // Handle options editing
                                match view_model.selected_option_index {
                                    // Boolean toggles
                                    5 => {
                                        view_model.config.allow_egress =
                                            !view_model.config.allow_egress
                                    }
                                    6 => {
                                        view_model.config.allow_containers =
                                            !view_model.config.allow_containers
                                    }
                                    7 => view_model.config.allow_vms = !view_model.config.allow_vms,
                                    8 => {
                                        view_model.config.allow_web_search =
                                            !view_model.config.allow_web_search
                                    }
                                    10 => {
                                        view_model.config.interactive_mode =
                                            !view_model.config.interactive_mode
                                    }
                                    12 => {
                                        view_model.config.record_output =
                                            !view_model.config.record_output
                                    }
                                    19 => {
                                        view_model.config.create_task_files =
                                            !view_model.config.create_task_files
                                    }
                                    20 => {
                                        view_model.config.create_metadata_commits =
                                            !view_model.config.create_metadata_commits
                                    }
                                    21 => {
                                        view_model.config.notifications =
                                            !view_model.config.notifications
                                    }

                                    // Enum selections - open inline popup
                                    1 => {
                                        view_model.inline_enum_popup = Some(InlineEnumPopup {
                                            options: vec![
                                                "local".to_string(),
                                                "devcontainer".to_string(),
                                                "vm".to_string(),
                                                "disabled".to_string(),
                                            ],
                                            selected_index: 0,
                                            option_index: view_model.selected_option_index,
                                        });
                                        self.exit_confirmation_armed = false;
                                    }
                                    2 => {
                                        view_model.inline_enum_popup = Some(InlineEnumPopup {
                                            options: vec![
                                                "auto".to_string(),
                                                "cow-overlay".to_string(),
                                                "worktree".to_string(),
                                                "in-place".to_string(),
                                            ],
                                            selected_index: 0,
                                            option_index: view_model.selected_option_index,
                                        });
                                        self.exit_confirmation_armed = false;
                                    }
                                    3 => {
                                        view_model.inline_enum_popup = Some(InlineEnumPopup {
                                            options: vec![
                                                "auto".to_string(),
                                                "zfs".to_string(),
                                                "btrfs".to_string(),
                                                "agentfs".to_string(),
                                                "git".to_string(),
                                                "disable".to_string(),
                                            ],
                                            selected_index: 0,
                                            option_index: view_model.selected_option_index,
                                        });
                                        self.exit_confirmation_armed = false;
                                    }
                                    11 => {
                                        view_model.inline_enum_popup = Some(InlineEnumPopup {
                                            options: vec![
                                                "text".to_string(),
                                                "text-normalized".to_string(),
                                            ],
                                            selected_index: 0,
                                            option_index: view_model.selected_option_index,
                                        });
                                        self.exit_confirmation_armed = false;
                                    }
                                    17 => {
                                        view_model.inline_enum_popup = Some(InlineEnumPopup {
                                            options: vec![
                                                "pr".to_string(),
                                                "branch".to_string(),
                                                "patch".to_string(),
                                            ],
                                            selected_index: 0,
                                            option_index: view_model.selected_option_index,
                                        });
                                        self.exit_confirmation_armed = false;
                                    }
                                    23 => {
                                        view_model.inline_enum_popup = Some(InlineEnumPopup {
                                            options: vec!["default".to_string()], // TODO: add actual fleet options
                                            selected_index: 0,
                                            option_index: view_model.selected_option_index,
                                        });
                                        self.exit_confirmation_armed = false;
                                    }

                                    _ => {} // Headers or other non-editable options
                                }
                                self.needs_redraw = true;
                                true
                            } else {
                                // Handle actions column activation
                                let action_str = match view_model.selected_action_index {
                                    0 => "Launch in new tab (t)",
                                    1 => "Launch in split view (s)",
                                    2 => "Launch in horizontal split (h)",
                                    3 => "Launch in vertical split (v)",
                                    4 => "Launch in new tab and focus (T)", // separator is index 4, so these shift
                                    5 => "Launch in split view and focus (S)",
                                    6 => "Launch in horizontal split and focus (H)",
                                    7 => "Launch in vertical split and focus (V)",
                                    _ => "",
                                };
                                let modal_type = modal.modal_type.clone();
                                self.apply_modal_selection(modal_type, action_str.to_string());
                                true
                            }
                        }
                        ModalType::EnumSelection {
                            options,
                            selected_index,
                            original_option_index,
                            ..
                        } => {
                            // Apply the selected enum value and return to launch options modal
                            if let Some(selected_value) = options.get(*selected_index) {
                                if let Some(mut launch_vm) = self.temp_launch_options.take() {
                                    // Update the config in the stored launch options
                                    match *original_option_index {
                                        1 => {
                                            launch_vm.config.sandbox_profile =
                                                selected_value.clone()
                                        }
                                        2 => {
                                            launch_vm.config.working_copy_mode =
                                                selected_value.clone()
                                        }
                                        3 => launch_vm.config.fs_snapshots = selected_value.clone(),
                                        11 => {
                                            launch_vm.config.output_format = selected_value.clone()
                                        }
                                        17 => {
                                            launch_vm.config.delivery_method =
                                                selected_value.clone()
                                        }
                                        23 => launch_vm.config.fleet = selected_value.clone(),
                                        _ => {}
                                    }

                                    // Create the launch options modal with updated config
                                    let launch_modal = ModalViewModel {
                                        title: "Advanced Launch Options".to_string(),
                                        input_value: "".to_string(),
                                        filtered_options: vec![],
                                        selected_index: 0,
                                        modal_type: ModalType::LaunchOptions {
                                            view_model: launch_vm,
                                        },
                                    };

                                    self.active_modal = Some(launch_modal);
                                    self.needs_redraw = true;
                                    return true;
                                }
                            }
                            false
                        }
                        ModalType::AgentSelection { options } => {
                            // Get the currently selected option
                            let selected_filtered_option =
                                modal.filtered_options.get(modal.selected_index);

                            if let Some(FilteredOption::Option { text, .. }) =
                                selected_filtered_option
                            {
                                // Parse the model name from the text (format: "Model Name (xCOUNT)")
                                if let Some(selected_model_name) =
                                    text.split(" (x").next().map(|s| s.trim())
                                {
                                    // Parse the current count
                                    let current_count = text
                                        .split(" (x")
                                        .nth(1)
                                        .and_then(|s| s.trim_end_matches(')').parse::<u32>().ok())
                                        .unwrap_or(0);

                                    if current_count == 0 {
                                        // If current row has 0 count: select only this model with count 1, remove all others
                                        for option in options.iter_mut() {
                                            if option.name == selected_model_name {
                                                option.count = 1;
                                                option.is_selected = true;
                                            } else {
                                                option.count = 0;
                                                option.is_selected = false;
                                            }
                                        }
                                    } else {
                                        // If current row has non-zero count: keep all current non-zero count models with their current counts
                                        // (They already have the correct counts, just ensure they are marked as selected)
                                        for option in options.iter_mut() {
                                            option.is_selected = option.count > 0;
                                        }
                                    }
                                }
                            }

                            // Apply all current selections and close modal
                            let modal_type = modal.modal_type.clone();
                            self.apply_modal_selection(modal_type, String::new());
                            // Focus returns to task description (handled by apply_modal_selection)
                            true
                        }
                        #[allow(unreachable_patterns)]
                        ModalType::LaunchOptions { view_model } => {
                            if view_model.active_column == LaunchOptionsColumn::Actions {
                                let action_str = match view_model.selected_action_index {
                                    0 => "Go (Enter)",
                                    1 => "Launch in background (b)",
                                    2 => "Launch in split view (s)",
                                    3 => "Launch in horizontal split (h)",
                                    4 => "Launch in vertical split (v)",
                                    _ => return false,
                                };
                                let modal_type = modal.modal_type.clone();
                                self.apply_modal_selection(modal_type, action_str.to_string());
                                true
                            } else {
                                // TODO: Handle option selection (toggles/edits) in left column
                                true
                            }
                        }
                        _ => {
                            // For other modals, use the selected option from filtered_options
                            let selection = modal
                                .filtered_options
                                .iter()
                                .find(|opt| {
                                    matches!(opt, FilteredOption::Option { selected: true, .. })
                                })
                                .and_then(|opt| {
                                    if let FilteredOption::Option { text, .. } = opt {
                                        Some((modal.modal_type.clone(), text.clone()))
                                    } else {
                                        None
                                    }
                                });

                            if let Some((modal_type, selected_option)) = selection {
                                self.apply_modal_selection(modal_type, selected_option);
                                true
                            } else {
                                false
                            }
                        }
                    }
                } else {
                    false
                }
            }
            KeyboardOperation::DismissOverlay => self.handle_dismiss_overlay(),
            KeyboardOperation::IncrementValue => self.handle_increment_decrement_value(true),
            KeyboardOperation::DecrementValue => self.handle_increment_decrement_value(false),
            // Text editing operations for modal input
            KeyboardOperation::DeleteCharacterBackward => self.handle_delete(),
            KeyboardOperation::DeleteToEndOfLine => {
                if let Some(modal) = self.active_modal.as_mut() {
                    modal.input_value.clear();
                    // Inline update logic for AgentSelection
                    if let ModalType::AgentSelection { options } = &modal.modal_type {
                        let mut filtered: Vec<FilteredOption> = options
                            .iter()
                            .map(|opt| FilteredOption::Option {
                                text: format!("{} (x{})", opt.name, opt.count),
                                selected: false,
                            })
                            .collect();

                        if modal.selected_index >= filtered.len() && !filtered.is_empty() {
                            modal.selected_index = 0;
                        }

                        if !filtered.is_empty() && modal.selected_index < filtered.len() {
                            if let FilteredOption::Option { selected, .. } =
                                &mut filtered[modal.selected_index]
                            {
                                *selected = true;
                            }
                        }

                        modal.filtered_options = filtered;
                    }
                    self.needs_redraw = true;
                    true
                } else {
                    false
                }
            }
            KeyboardOperation::MoveToBeginningOfLine => {
                // For simple string input, beginning is already the start
                true
            }
            KeyboardOperation::MoveToEndOfLine => {
                // For simple string input, end is already the end
                true
            }
            _ => false,
        }
    }

    fn handle_dashboard_operation(
        &mut self,
        operation: KeyboardOperation,
        key_event: &KeyEvent,
    ) -> bool {
        self.process_dashboard_operation(operation, key_event)
    }

    fn handle_autocomplete_accept(
        &mut self,
        idx: usize,
        acceptance: AutocompleteAcceptance,
    ) -> bool {
        if let Some(card) = self.draft_cards.get_mut(idx) {
            if self.autocomplete.accept_completion(
                &mut card.description,
                &mut self.needs_redraw,
                acceptance,
            ) {
                card.on_content_changed();
                card.autocomplete
                    .after_textarea_change(&card.description, &mut self.needs_redraw);
                self.autocomplete
                    .after_textarea_change(&card.description, &mut self.needs_redraw);
                self.mark_draft_dirty(idx);
                self.clear_exit_confirmation();
                return true;
            }
        }

        false
    }

    /// Handle keyboard events by translating to KeyboardOperation and dispatching
    pub fn handle_key_event(&mut self, key: KeyEvent) -> bool {
        use ratatui::crossterm::event::KeyCode;

        trace!("handle_key_event: received key event: {:?}", key);

        if self.autocomplete.has_actionable_suggestion() {
            if let Some(idx) = self.focused_textarea_index() {
                if let Some(operation) = minor_modes::AUTOCOMPLETE_ACTIVE_MODE
                    .resolve_key_to_operation(&key, &self.settings)
                {
                    trace!(
                        "handle_key_event: AUTOCOMPLETE_ACTIVE_MODE resolved operation: {:?}",
                        operation
                    );
                    if operation == KeyboardOperation::IndentOrComplete
                        && self.handle_autocomplete_accept(
                            idx,
                            AutocompleteAcceptance::SharedExtension,
                        )
                    {
                        return true;
                    }
                }
            }
        }

        if key.code == KeyCode::Right && key.modifiers.is_empty() {
            if let Some(idx) = self.focused_textarea_index() {
                if self.handle_autocomplete_accept(idx, AutocompleteAcceptance::FullCompletion) {
                    return true;
                }
            }
        }

        // Special handling for LaunchOptions modal key mappings
        // In LaunchOptions modal, we override the default key behavior:
        // - Space → ActivateCurrentItem (toggle/activate options)
        // - Enter → Save and close modal (or select enum value if in popup)
        if self.modal_state == ModalState::LaunchOptions {
            if let Some(modal) = self.active_modal.as_ref() {
                if let ModalType::LaunchOptions { view_model } = &modal.modal_type {
                    // Handle Space key - always activates/toggles current item
                    if key.code == KeyCode::Char(' ') && key.modifiers.is_empty() {
                        trace!("handle_key_event: Space in LaunchOptions → ActivateCurrentItem");
                        let handled = self
                            .handle_modal_operation(KeyboardOperation::ActivateCurrentItem, &key);
                        if handled {
                            self.clear_exit_confirmation();
                        }
                        return handled;
                    }

                    // Handle Enter key - behavior depends on context
                    if key.code == KeyCode::Enter && key.modifiers.is_empty() {
                        if view_model.inline_enum_popup.is_some() {
                            // Inside enum popup: Enter selects the enum value
                            trace!("handle_key_event: Enter in enum popup → ActivateCurrentItem");
                            let handled = self.handle_modal_operation(
                                KeyboardOperation::ActivateCurrentItem,
                                &key,
                            );
                            if handled {
                                self.clear_exit_confirmation();
                            }
                            return handled;
                        } else {
                            // Outside popup: Enter saves and closes the modal
                            trace!("handle_key_event: Enter in LaunchOptions → Save and close");

                            // Save the config changes to the draft card
                            let draft_id = view_model.draft_id.clone();
                            let config = view_model.config.clone();

                            if let Some(card) =
                                self.draft_cards.iter_mut().find(|c| c.id == draft_id)
                            {
                                card.advanced_options = Some(config);
                            }

                            // Close modal and restore focus
                            self.close_modal(true);
                            self.change_focus(DashboardFocusState::DraftTask(0));
                            if let Some(card) = self.draft_cards.get_mut(0) {
                                card.focus_element = CardFocusElement::TaskDescription;
                            }

                            self.clear_exit_confirmation();
                            return true;
                        }
                    }
                }
            }
        }

        // Try handlers in priority order (like the original input stack)
        // Modal > Draft buttons > Textarea > Dashboard

        // Try modal operations first (highest priority)
        if self.modal_state != ModalState::None {
            // Choose the appropriate input mode based on modal type
            let mode = if let Some(modal) = self.active_modal.as_ref() {
                match modal.modal_type {
                    ModalType::AgentSelection { .. } => &MODEL_SELECTION_MODE,
                    _ => &MODAL_NAVIGATION_MODE,
                }
            } else {
                trace!("handle_key_event: trying MODAL_NAVIGATION_MODE (no active modal)");
                &MODAL_NAVIGATION_MODE
            };

            if let Some(operation) = mode.resolve_key_to_operation(&key, &self.settings) {
                trace!(
                    "handle_key_event: modal mode resolved operation: {:?}",
                    operation
                );
                let handled = self.handle_modal_operation(operation, &key);
                if handled && operation != KeyboardOperation::DismissOverlay {
                    self.clear_exit_confirmation();
                }
                return handled;
            }

            // Handle character input for modal input fields
            if let Some(modal) = self.active_modal.as_mut() {
                if let KeyCode::Char(c) = key.code {
                    // Only allow character input for certain modal types
                    match &modal.modal_type {
                        ModalType::Search { .. } => {
                            modal.input_value.push(c);

                            // Inline filtering logic for search modals
                            let all_options: &[String] = match self.modal_state {
                                ModalState::RepositorySearch => &self.available_repositories,
                                ModalState::BranchSearch => &self.available_branches,
                                ModalState::ModelSearch => {
                                    // For model search, we need to convert ModelInfo to display names
                                    self.model_display_names_cache.get_or_insert_with(|| {
                                        self.available_models
                                            .iter()
                                            .map(|m| m.display_name())
                                            .collect()
                                    });
                                    self.model_display_names_cache.as_ref().unwrap()
                                }
                                _ => &[],
                            };

                            let query = modal.input_value.to_lowercase();
                            let mut filtered: Vec<FilteredOption> = all_options
                                .iter()
                                .filter(|option| {
                                    if query.is_empty() {
                                        true // Show all options when no query
                                    } else {
                                        option.to_lowercase().contains(&query)
                                    }
                                })
                                .cloned()
                                .map(|opt| FilteredOption::Option {
                                    text: opt,
                                    selected: false,
                                })
                                .collect();

                            // Reset selected index if it's out of bounds
                            if modal.selected_index >= filtered.len() && !filtered.is_empty() {
                                modal.selected_index = 0;
                            }

                            // Mark the selected option
                            if !filtered.is_empty() && modal.selected_index < filtered.len() {
                                if let FilteredOption::Option { selected, .. } =
                                    &mut filtered[modal.selected_index]
                                {
                                    *selected = true;
                                }
                            }

                            modal.filtered_options = filtered;
                            self.needs_redraw = true;
                            return true;
                        }
                        ModalType::AgentSelection { .. } => {
                            modal.input_value.push(c);
                            Self::update_model_selection_filtered_options(modal);
                            self.needs_redraw = true;
                            return true;
                        }
                        _ => {}
                    }
                }
            }
        }

        // Try draft button operations
        if let DashboardFocusState::DraftTask(idx) = self.focus_element {
            if let Some(operation) =
                DRAFT_BUTTON_NAVIGATION_MODE.resolve_key_to_operation(&key, &self.settings)
            {
                let handled = matches!(
                    self.handle_task_entry_operation(idx, operation, &key),
                    KeyboardOperationResult::Handled
                );
                if handled && operation != KeyboardOperation::DismissOverlay {
                    self.clear_exit_confirmation();
                }
                return handled;
            }
        }

        // Try textarea operations
        if let Some(idx) = self.focused_textarea_index() {
            if let Some(operation) =
                DRAFT_TEXT_EDITING_MODE.resolve_key_to_operation(&key, &self.settings)
            {
                match self.handle_task_entry_operation(idx, operation, &key) {
                    KeyboardOperationResult::Handled => {
                        if operation != KeyboardOperation::DismissOverlay {
                            self.clear_exit_confirmation();
                        }
                        return true;
                    }
                    KeyboardOperationResult::NotHandled => {
                        // Try dashboard operations for operations not handled by the card
                        if matches!(operation, KeyboardOperation::ShowLaunchOptions) {
                            let handled = self.handle_dashboard_operation(operation, &key);
                            if handled && operation != KeyboardOperation::DismissOverlay {
                                self.clear_exit_confirmation();
                            }
                            return handled;
                        }
                        return false;
                    }
                    KeyboardOperationResult::Bubble { operation: bubbled } => {
                        let bubbled_handled = self.handle_bubbled_operation(bubbled, &key);
                        if bubbled_handled && bubbled != KeyboardOperation::DismissOverlay {
                            self.clear_exit_confirmation();
                        }
                        return bubbled_handled;
                    }
                    KeyboardOperationResult::TaskLaunched { .. } => {
                        if operation != KeyboardOperation::DismissOverlay {
                            self.clear_exit_confirmation();
                        }
                        return true;
                    }
                }
            }
        }

        // Try draft navigation operations (textarea to buttons, or button navigation)
        if let DashboardFocusState::DraftTask(idx) = self.focus_element {
            if let Some(card) = self.draft_cards.get(idx) {
                let mode = if card.focus_element == CardFocusElement::TaskDescription {
                    trace!("handle_key_event: trying DRAFT_TEXTAREA_TO_BUTTONS_MODE");
                    &DRAFT_TEXTAREA_TO_BUTTONS_MODE
                } else {
                    trace!("handle_key_event: trying DRAFT_BUTTON_NAVIGATION_MODE");
                    &DRAFT_BUTTON_NAVIGATION_MODE
                };

                if let Some(operation) = mode.resolve_key_to_operation(&key, &self.settings) {
                    let handled = self.handle_dashboard_operation(operation, &key);
                    if handled && operation != KeyboardOperation::DismissOverlay {
                        self.clear_exit_confirmation();
                    }
                    return handled;
                } else {
                    trace!("handle_key_event: draft mode did not resolve key to any operation");
                }
            }
        }

        // Try dashboard operations (lowest priority)
        trace!("handle_key_event: trying DASHBOARD_NAVIGATION_MODE");
        if let Some(operation) =
            DASHBOARD_NAVIGATION_MODE.resolve_key_to_operation(&key, &self.settings)
        {
            trace!(
                "handle_key_event: DASHBOARD_NAVIGATION_MODE resolved operation: {:?}",
                operation
            );
            let handled = self.handle_dashboard_operation(operation, &key);
            if handled && operation != KeyboardOperation::DismissOverlay {
                self.clear_exit_confirmation();
            }
            return handled;
        }

        // Handle character input directly if it's not a recognized operation
        if let KeyCode::Char(ch) = key.code {
            let handled = self.handle_char_input(ch);
            if handled {
                self.clear_exit_confirmation();
            }
            return handled;
        }

        // If no operation matched and it's not character input, the key is not handled
        trace!("handle_key_event: no mode handled key event: {:?}", key);
        false
    }

    /// Handle a KeyboardOperation with the original KeyEvent context
    pub fn handle_keyboard_operation(
        &mut self,
        operation: KeyboardOperation,
        key: &KeyEvent,
    ) -> bool {
        if operation != KeyboardOperation::DismissOverlay {
            self.clear_exit_confirmation();
        }

        // Try handlers in priority order (like the original input stack)
        // Modal > Draft buttons > Textarea > Dashboard

        // Simulate the input stack by prioritizing active UI contexts
        // Modal > Draft buttons > Textarea > Dashboard

        // Try modal operations first (highest priority) - if modal is active, it gets first chance
        if self.modal_state != ModalState::None {
            return self.handle_modal_operation(operation, key);
        }

        // Try draft button operations - if draft buttons are focused, they get priority
        if let Some(idx) = self.focused_draft_buttons_index() {
            match self.handle_task_entry_operation(idx, operation, key) {
                KeyboardOperationResult::Handled => return true,
                KeyboardOperationResult::NotHandled => { /* fall through to lower priority handlers */
                }
                KeyboardOperationResult::Bubble { operation: bubbled } => {
                    return self.handle_bubbled_operation(bubbled, key);
                }
                KeyboardOperationResult::TaskLaunched { .. } => return true,
            }
        }

        // Try textarea operations - if textarea is focused, it gets priority
        if let Some(idx) = self.focused_textarea_index() {
            match self.handle_task_entry_operation(idx, operation, key) {
                KeyboardOperationResult::Handled => return true,
                KeyboardOperationResult::NotHandled => { /* fall through to lower priority handlers */
                }
                KeyboardOperationResult::Bubble { operation: bubbled } => {
                    return self.handle_bubbled_operation(bubbled, key);
                }
                KeyboardOperationResult::TaskLaunched { .. } => return true,
            }
        }

        // Try dashboard operations (lowest priority) - fallback for any operation
        self.handle_dashboard_operation(operation, key)
    }

    pub fn take_exit_request(&mut self) -> bool {
        if self.exit_requested {
            self.exit_requested = false;
            return true;
        }
        false
    }

    /// Handle a dashboard-level KeyboardOperation with the original KeyEvent context
    fn process_dashboard_operation(
        &mut self,
        operation: KeyboardOperation,
        key: &KeyEvent,
    ) -> bool {
        // Any keyboard operation (except ESC) should clear exit confirmation
        if operation != KeyboardOperation::DismissOverlay {
            self.clear_exit_confirmation();
        }

        // Check if autocomplete is open and should handle navigation operations
        if self.autocomplete.is_open() {
            match operation {
                KeyboardOperation::MoveToNextLine | KeyboardOperation::MoveToNextField => {
                    if self.autocomplete.select_next() {
                        self.needs_redraw = true;
                        return true;
                    }
                }
                KeyboardOperation::MoveToPreviousLine => {
                    if self.autocomplete.select_previous() {
                        self.needs_redraw = true;
                        return true;
                    }
                }
                _ => {}
            }
        }

        // Handle modal selection with Enter when a modal is active
        if let Some(modal) = self.active_modal.as_ref() {
            if operation == KeyboardOperation::ActivateCurrentItem {
                // Select the currently highlighted option in the modal
                // Get the selected option first to avoid borrowing issues
                let selected_option = modal
                    .filtered_options
                    .iter()
                    .find(|opt| matches!(opt, FilteredOption::Option { selected: true, .. }))
                    .and_then(|opt| {
                        if let FilteredOption::Option { text, .. } = opt {
                            Some(text.clone())
                        } else {
                            None
                        }
                    });

                if let Some(selected_option) = selected_option {
                    self.apply_modal_selection(modal.modal_type.clone(), selected_option);
                }
                return true;
            }
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
            KeyboardOperation::ActivateCurrentItem => {
                // Enter key: activate the current item
                if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get(idx) {
                        match card.focus_element {
                            CardFocusElement::RepositorySelector => {
                                self.open_modal(ModalState::RepositorySearch);
                                return true;
                            }
                            CardFocusElement::BranchSelector => {
                                self.open_modal(ModalState::BranchSearch);
                                return true;
                            }
                            CardFocusElement::ModelSelector => {
                                self.open_modal(ModalState::ModelSearch);
                                return true;
                            }
                            CardFocusElement::GoButton => {
                                // Launch the task
                                let default_split_mode = self.settings.default_split_mode();
                                // Use advanced options from draft card, or defaults if not configured
                                let advanced_options = card
                                    .advanced_options
                                    .clone()
                                    .or_else(|| Some(AdvancedLaunchOptions::default()));
                                self.launch_task(
                                    idx,
                                    default_split_mode,
                                    false,
                                    None,
                                    None,
                                    advanced_options,
                                );
                                return true;
                            }
                            CardFocusElement::AdvancedOptionsButton => {
                                // Open advanced launch options
                                let draft_id = card.id.clone();
                                self.open_launch_options_modal(draft_id);
                                return true;
                            }
                            CardFocusElement::TaskDescription => {
                                // This should be handled by the textarea operations
                                return false;
                            }
                        }
                    }
                }
                false
            }
            KeyboardOperation::MoveForwardOneCharacter => {
                // Right arrow: handle filter navigation or draft card navigation

                // Handle filter navigation
                match self.focus_element {
                    DashboardFocusState::FilterBarLine => {
                        // Move from separator line to first filter control
                        self.change_focus(DashboardFocusState::Filter(FilterControl::Repository));
                        return true;
                    }
                    DashboardFocusState::Filter(control) => {
                        // Navigate between filter controls
                        let next = match control {
                            FilterControl::Repository => FilterControl::Status,
                            FilterControl::Status => FilterControl::Creator,
                            FilterControl::Creator => FilterControl::Repository, // Wrap around
                        };
                        self.change_focus(DashboardFocusState::Filter(next));
                        return true;
                    }
                    DashboardFocusState::DraftTask(_) => {
                        // Right arrow in draft card: same as Tab
                        return self.focus_next_control();
                    }
                    _ => {}
                }

                false
            }
            KeyboardOperation::MoveBackwardOneCharacter => {
                // Left arrow: handle filter navigation or draft card navigation

                // Handle filter navigation
                match self.focus_element {
                    DashboardFocusState::FilterBarLine => {
                        // Move from separator line to first filter control
                        self.change_focus(DashboardFocusState::Filter(FilterControl::Repository));
                        return true;
                    }
                    DashboardFocusState::Filter(control) => {
                        // Navigate backwards through filter controls
                        let next = match control {
                            FilterControl::Repository => FilterControl::Creator, // Wrap backwards
                            FilterControl::Status => FilterControl::Repository,
                            FilterControl::Creator => FilterControl::Status,
                        };
                        self.change_focus(DashboardFocusState::Filter(next));
                        return true;
                    }
                    DashboardFocusState::DraftTask(_) => {
                        // Left arrow in draft card: same as Shift+Tab
                        return self.focus_previous_control();
                    }
                    _ => {}
                }

                // Task entry handling is now done by early delegation
                false
            }
            KeyboardOperation::DecrementValue => {
                // Left arrow: handle filter navigation (same as MoveBackwardOneCharacter)

                // Handle filter navigation
                match self.focus_element {
                    DashboardFocusState::FilterBarLine => {
                        // Move from separator line to first filter control
                        self.change_focus(DashboardFocusState::Filter(FilterControl::Repository));
                        return true;
                    }
                    DashboardFocusState::Filter(control) => {
                        // Navigate backwards through filter controls
                        let next = match control {
                            FilterControl::Repository => FilterControl::Creator, // Wrap backwards
                            FilterControl::Status => FilterControl::Repository,
                            FilterControl::Creator => FilterControl::Status,
                        };
                        self.change_focus(DashboardFocusState::Filter(next));
                        return true;
                    }
                    DashboardFocusState::DraftTask(_) => {
                        // Left arrow in draft card: same as Shift+Tab
                        return self.focus_previous_control();
                    }
                    _ => {}
                }

                // Task entry handling is now done by early delegation
                false
            }
            KeyboardOperation::MoveToPreviousLine => {
                if self.handle_overlay_navigation(NavigationDirection::Previous) {
                    return true;
                }
                if key.modifiers.contains(ratatui::crossterm::event::KeyModifiers::SHIFT) {
                    if let Some(idx) = self.focused_textarea_index() {
                        self.change_focus(DashboardFocusState::DraftTask(idx));
                        return true;
                    }
                }
                // Task entry handling is done by early delegation
                // If we reach here, navigate up hierarchy
                self.navigate_up_hierarchy()
            }
            KeyboardOperation::MoveToNextLine => {
                // Task entry handling is done by early delegation
                // If we reach here, navigate down hierarchy
                self.navigate_down_hierarchy()
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
            // Removed unreachable duplicate KeyboardOperation::ActivateCurrentItem arm
            KeyboardOperation::DismissOverlay => self.handle_dismiss_overlay(),
            KeyboardOperation::DraftNewTask => self.handle_ctrl_n(),
            KeyboardOperation::DeleteToEndOfLine => {
                if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == CardFocusElement::TaskDescription {
                            let before_text = card.description.lines().join("\n");
                            card.description.delete_line_by_end();
                            let after_text = card.description.lines().join("\n");
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
                false
            }
            KeyboardOperation::DeleteToBeginningOfLine => {
                if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == CardFocusElement::TaskDescription {
                            let before_text = card.description.lines().join("\n");
                            card.description.delete_line_by_head();
                            let after_text = card.description.lines().join("\n");
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
                false
            }
            KeyboardOperation::SelectAll => {
                if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == CardFocusElement::TaskDescription {
                            card.description.select_all();
                            self.autocomplete
                                .after_textarea_change(&card.description, &mut self.needs_redraw);
                            return true;
                        }
                    }
                }
                false
            }
            KeyboardOperation::Bold => {
                if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == CardFocusElement::TaskDescription {
                            card.description.insert_str("****");
                            use tui_textarea::CursorMove;
                            card.description.move_cursor(CursorMove::Back);
                            card.description.move_cursor(CursorMove::Back);
                            self.autocomplete
                                .after_textarea_change(&card.description, &mut self.needs_redraw);
                            return true;
                        }
                    }
                }
                false
            }
            KeyboardOperation::MoveToBeginningOfSentence => {
                // Move to beginning of sentence (approximated as beginning of line)
                if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == CardFocusElement::TaskDescription {
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
                if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == CardFocusElement::TaskDescription {
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
                if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == CardFocusElement::TaskDescription {
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
                if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == CardFocusElement::TaskDescription {
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
                if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == CardFocusElement::TaskDescription {
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
                if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == CardFocusElement::TaskDescription {
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
                if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == CardFocusElement::TaskDescription {
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
                if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == CardFocusElement::TaskDescription {
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
                if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == CardFocusElement::TaskDescription {
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
                if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == CardFocusElement::TaskDescription {
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
                if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == CardFocusElement::TaskDescription {
                            // Get current cursor line and viewport height
                            let cursor = card.description.cursor();
                            let _lines = card.description.lines(); // unused; centered scroll logic only needs cursor & viewport
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
                if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == CardFocusElement::TaskDescription {
                            let lines = card.description.lines(); // lines used below for comment application
                            let cursor_row = card.description.cursor().0;
                            let _cursor_col = card.description.cursor().1; // unused; selection range logic only uses row

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
                // Duplicate line/selection (Ctrl+Shift+D / Cmd+Shift+D) - copy and paste below
                if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == CardFocusElement::TaskDescription {
                            let (cursor_row, _) = card.description.cursor();
                            let lines = card.description.lines(); // lines used for duplication logic

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
                if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == CardFocusElement::TaskDescription {
                            let cursor_row = card.description.cursor().0;
                            let _lines = card.description.lines(); // underscore: reserved for future line-based operations

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
                if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == CardFocusElement::TaskDescription {
                            let cursor_row = card.description.cursor().0;
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
                if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == CardFocusElement::TaskDescription {
                            // Get selection range or current line
                            let (_start_line, _end_line) =
                                if let Some(range) = card.description.selection_range() {
                                    (range.0.0, range.1.0)
                                } else {
                                    let cursor_row = card.description.cursor().0;
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
                if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == CardFocusElement::TaskDescription {
                            // Get selection range or current line
                            let (_start_line, _end_line) =
                                if let Some(range) = card.description.selection_range() {
                                    (range.0.0, range.1.0)
                                } else {
                                    let cursor_row = card.description.cursor().0;
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
                if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == CardFocusElement::TaskDescription {
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
                                        let mut all_lines: Vec<String> = lines.to_vec();
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
                if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == CardFocusElement::TaskDescription {
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
                                        let mut all_lines: Vec<String> = lines.to_vec();
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
                if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == CardFocusElement::TaskDescription {
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
                if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == CardFocusElement::TaskDescription {
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
            // (Second Bold arm removed as duplicate; first Bold arm earlier handles this operation)
            KeyboardOperation::Italic => {
                // Italic (Ctrl+I) - wrap selection or next word with *
                if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == CardFocusElement::TaskDescription {
                            if card.description.selection_range().is_some() {
                                // Copy selection to yank buffer
                                card.description.copy();
                                let selected_text = card.description.yank_text();
                                if !selected_text.is_empty() {
                                    // Replace selection with wrapped text
                                    card.description.insert_str(format!("*{}*", selected_text));
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
                if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == CardFocusElement::TaskDescription {
                            if card.description.selection_range().is_some() {
                                // Copy selection to yank buffer
                                card.description.copy();
                                let selected_text = card.description.yank_text();
                                if !selected_text.is_empty() {
                                    // Replace selection with wrapped text
                                    card.description
                                        .insert_str(format!("<u>{}</u>", selected_text));
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
                if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == CardFocusElement::TaskDescription {
                            // Placeholder - would need yank ring implementation
                            return true;
                        }
                    }
                }
                false
            }
            KeyboardOperation::TransposeCharacters => {
                // Transpose characters (Ctrl+T) - swap character before cursor with character after
                if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == CardFocusElement::TaskDescription {
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
                if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == CardFocusElement::TaskDescription {
                            // Simplified implementation - would need complex word boundary detection
                            return true; // Placeholder
                        }
                    }
                }
                false
            }
            KeyboardOperation::IncrementalSearchForward => {
                // Incremental search forward (Ctrl+S) - start search mode
                if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == CardFocusElement::TaskDescription {
                            // Set search pattern (would need search dialog/input in real implementation)
                            let _ = card.description.set_search_pattern("search_term");
                            card.description.search_forward(false);
                            return true;
                        }
                    }
                }
                false
            }
            KeyboardOperation::IncrementalSearchBackward => {
                // Incremental search backward (Ctrl+R) - start reverse search mode
                if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == CardFocusElement::TaskDescription {
                            // Set search pattern and search backward
                            let _ = card.description.set_search_pattern("search_term");
                            card.description.search_back(false);
                            return true;
                        }
                    }
                }
                false
            }
            KeyboardOperation::FindNext => {
                // Find next (F3) - jump to next search match
                if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == CardFocusElement::TaskDescription {
                            card.description.search_forward(false);
                            return true;
                        }
                    }
                }
                false
            }
            KeyboardOperation::FindPrevious => {
                // Find previous (Shift+F3) - jump to previous search match
                if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        if card.focus_element == CardFocusElement::TaskDescription {
                            card.description.search_back(false);
                            return true;
                        }
                    }
                }
                false
            }
            KeyboardOperation::DeleteCurrentTask => {
                // Delete current task (Ctrl+W, Cmd+W, C-x k)
                match self.focus_element {
                    DashboardFocusState::DraftTask(idx) => {
                        if idx < self.draft_cards.len() {
                            // Delete draft card without leaving a trace
                            self.draft_cards.remove(idx);
                            // Adjust focus after removal
                            if self.draft_cards.is_empty() {
                                // No more draft cards, focus on settings button
                                self.focus_element = DashboardFocusState::SettingsButton;
                            } else if idx >= self.draft_cards.len() {
                                // Removed last card, focus on new last card
                                self.focus_element =
                                    DashboardFocusState::DraftTask(self.draft_cards.len() - 1);
                            } // else focus stays on the same index (now pointing to next card)
                            self.needs_redraw = true;
                            return true;
                        }
                    }
                    DashboardFocusState::ExistingTask(card_index) => {
                        // Delete existing task (active or completed/merged)
                        if let Some(task_card) = self.task_cards.get(card_index) {
                            let task_state = {
                                let card = task_card.lock().unwrap();
                                card.metadata.state
                            };

                            match task_state {
                                TaskState::Running => {
                                    // For active tasks, abort any running agents
                                    // TODO: Implement agent abortion logic when available
                                    // For now, just remove the card
                                    self.task_cards.remove(card_index);
                                    // Adjust focus after removal
                                    if self.task_cards.is_empty() {
                                        self.focus_element = DashboardFocusState::SettingsButton;
                                    } else if card_index >= self.task_cards.len() {
                                        self.focus_element = DashboardFocusState::ExistingTask(
                                            self.task_cards.len() - 1,
                                        );
                                    }
                                    self.needs_redraw = true;
                                    return true;
                                }
                                TaskState::Completed | TaskState::Merged => {
                                    // For completed/merged tasks, archive them (hide from listings)
                                    // TODO: Implement archiving logic when available
                                    // For now, just remove from display
                                    self.task_cards.remove(card_index);
                                    // Adjust focus after removal
                                    if self.task_cards.is_empty() {
                                        self.focus_element = DashboardFocusState::SettingsButton;
                                    } else if card_index >= self.task_cards.len() {
                                        self.focus_element = DashboardFocusState::ExistingTask(
                                            self.task_cards.len() - 1,
                                        );
                                    }
                                    self.needs_redraw = true;
                                    return true;
                                }
                                _ => {} // Other states not handled
                            }
                        }
                    }
                    _ => {} // Other focus states don't support deletion
                }
                false
            }
            KeyboardOperation::ShowLaunchOptions => {
                // Show advanced launch options menu (Ctrl+Enter in draft textarea)
                trace!("handle_dashboard_operation: ShowLaunchOptions triggered");
                trace!(
                    "handle_dashboard_operation: current focus_element: {:?}",
                    self.focus_element
                );
                match self.focus_element {
                    DashboardFocusState::DraftTask(idx) => {
                        trace!("handle_dashboard_operation: focused on draft task {}", idx);
                        if let Some(card) = self.draft_cards.get(idx) {
                            trace!(
                                "handle_dashboard_operation: card focus_element: {:?}",
                                card.focus_element
                            );
                            if card.focus_element == CardFocusElement::TaskDescription {
                                // Create launch options modal
                                let draft_id = card.id.clone();
                                trace!(
                                    "handle_dashboard_operation: opening modal for draft_id: {}",
                                    draft_id
                                );
                                self.open_launch_options_modal(draft_id);
                                return true;
                            } else {
                                trace!(
                                    "handle_dashboard_operation: card focus is not on TaskDescription"
                                );
                            }
                        } else {
                            trace!("handle_dashboard_operation: no card found at index {}", idx);
                        }
                    }
                    _ => {
                        trace!("handle_dashboard_operation: not focused on a draft task");
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

    /// Handle mouse drag events for text selection within text areas
    pub fn handle_mouse_drag(
        &mut self,
        column: u16,
        row: u16,
        textarea_area: &ratatui::layout::Rect,
    ) -> bool {
        debug!(
            "Mouse drag at screen ({}, {}) in textarea area {:?}",
            column, row, textarea_area
        );

        // Only handle drag if we're in a draft task and dragging is active
        if !matches!(self.focus_element, DashboardFocusState::DraftTask(0)) || !self.is_dragging {
            return false;
        }

        // Calculate relative position within textarea with padding awareness
        let padding = 1u16;
        let raw_relative_x = column as i32 - textarea_area.x as i32 - padding as i32;
        let relative_x = raw_relative_x.max(0) as u16;
        let relative_y = (row as i32 - textarea_area.y as i32).max(0) as u16;

        // Update selection to current drag position
        if let Some(card) = self.draft_cards.first_mut() {
            // Calculate precise cursor position for current drag location
            let line_index =
                relative_y.min(card.description.lines().len().saturating_sub(1) as u16) as usize;
            let line = card.description.lines().get(line_index).map_or("", |s| s);

            // Find the character position that best matches the current drag location
            let mut visual_width = 0u16;
            let mut col_index = 0;

            // If drag was in the padding area (raw_relative_x < 0), position at start of line
            if raw_relative_x < 0 {
                // Position at beginning of line (like HOME key)
                col_index = 0;
            } else {
                for ch in line.chars() {
                    let char_width = if ch.is_ascii() { 1 } else { 2 }; // Simple heuristic for wide chars
                    visual_width += char_width;
                    col_index += 1;

                    // Position cursor after the character if drag is within or at its end
                    if visual_width > relative_x {
                        break;
                    }
                }
            }

            debug!(
                "Drag to cursor position: line={}, col={}",
                line_index, col_index
            );

            // Move cursor to current drag position (this extends the selection)
            card.description.move_cursor(tui_textarea::CursorMove::Jump(
                line_index as u16,
                col_index as u16,
            ));

            let (final_row, final_col) = card.description.cursor();
            debug!("Selection extended to ({}, {})", final_row, final_col);
        }

        true
    }

    /// Stop any ongoing text selection drag operation
    fn stop_dragging(&mut self) {
        if self.is_dragging {
            debug!("Stopping text selection drag");
            self.is_dragging = false;
            self.drag_start_position = None;
            self.drag_start_bounds = None;
        }
    }

    /// Handle mouse up events to clear selection state for non-drag clicks
    pub fn handle_mouse_up(&mut self, column: u16, row: u16) -> bool {
        debug!("Mouse up at ({}, {})", column, row);

        // If mouse up at the same position as mouse down, it was just a click, not a drag
        // Clear the selection and drag state
        if self.is_dragging && self.drag_start_position == Some((column, row)) {
            self.stop_dragging();
            if let Some(card) = self.draft_cards.first_mut() {
                if card.focus_element == CardFocusElement::TaskDescription {
                    card.description.cancel_selection();
                }
            }
            true
        } else {
            false
        }
    }

    /// Handle mouse scroll actions by mapping them to hierarchical navigation.
    fn handle_mouse_scroll(&mut self, direction: NavigationDirection) -> bool {
        self.clear_exit_confirmation();

        // Handle modal list scrolling first
        if let Some(modal) = &mut self.active_modal {
            match &modal.modal_type {
                ModalType::Search { .. } | ModalType::AgentSelection { .. } => {
                    // For modal lists, change the selected index
                    let options_count = modal.filtered_options.len();
                    if options_count > 0 {
                        let new_index = match direction {
                            NavigationDirection::Next => {
                                (modal.selected_index + 1).min(options_count.saturating_sub(1))
                            }
                            NavigationDirection::Previous => modal.selected_index.saturating_sub(1),
                        };
                        if new_index != modal.selected_index {
                            modal.selected_index = new_index;
                            self.needs_redraw = true;
                            return true;
                        }
                    }
                }
                _ => {} // Other modal types don't have scrollable lists
            }
        }

        if self.autocomplete.is_open() {
            let changed = match direction {
                NavigationDirection::Next => self.autocomplete.select_next(),
                NavigationDirection::Previous => self.autocomplete.select_previous(),
            };
            if changed {
                self.needs_redraw = true;
                return true;
            }
        }
        if let DashboardFocusState::DraftTask(idx) = self.focus_element {
            if let Some(card) = self.draft_cards.get_mut(idx) {
                if card.focus_element == CardFocusElement::TaskDescription {
                    use tui_textarea::Scrolling;
                    let before = card.description.viewport_origin();
                    match direction {
                        NavigationDirection::Next => {
                            card.description.scroll(Scrolling::Delta { rows: 1, cols: 0 })
                        }
                        NavigationDirection::Previous => {
                            card.description.scroll(Scrolling::Delta { rows: -1, cols: 0 })
                        }
                    }
                    let after = card.description.viewport_origin();
                    if after != before {
                        self.needs_redraw = true;
                        return true;
                    }
                }
            }
        }
        match direction {
            NavigationDirection::Next => self.navigate_down_hierarchy(),
            NavigationDirection::Previous => self.navigate_up_hierarchy(),
        }
    }

    /// Start background loading of workspace files and workflows
    pub fn start_background_loading(&mut self) {
        self.workspace_terms.request_refresh();
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

        // Start loading repositories if not already loaded
        if !self.loading_repositories {
            self.loading_repositories = true;
            let repositories_enumerator = Arc::clone(&self.repositories_enumerator);

            // Take the sender to move into the async task
            if let Some(sender) = self.repositories_sender.take() {
                tokio::spawn(async move {
                    let repositories = repositories_enumerator.list_repositories().await;
                    let repository_names: Vec<String> =
                        repositories.into_iter().map(|repo| repo.name).collect();
                    // Send the loaded repositories back via the channel
                    let _ = sender.send(repository_names);
                });
            }
        }

        // Start loading branches if not already loaded
        if !self.loading_branches {
            self.loading_branches = true;
            let branches_enumerator = Arc::clone(&self.branches_enumerator);

            // Take the sender to move into the async task
            if let Some(sender) = self.branches_sender.take() {
                tokio::spawn(async move {
                    // For now, load branches for a default repository or empty list
                    // TODO: This should be context-aware based on selected repository
                    let branches = branches_enumerator.list_branches("").await;
                    let branch_names: Vec<String> =
                        branches.into_iter().map(|branch| branch.name).collect();
                    // Send the loaded branches back via the channel
                    let _ = sender.send(branch_names);
                });
            }
        }

        // Start loading agents if not already loaded
        if !self.loading_models {
            self.loading_models = true;
            let agents_enumerator = Arc::clone(&self.agents_enumerator);

            // Take the sender to move into the async task
            if let Some(sender) = self.agents_sender.take() {
                tokio::spawn(async move {
                    match agents_enumerator.enumerate_agents().await {
                        Ok(catalog) => {
                            let agents: Vec<AgentChoice> = catalog
                                .agents
                                .into_iter()
                                .map(|metadata| metadata.to_agent_choice())
                                .collect();
                            // Send the loaded agents back via the channel
                            let _ = sender.send(agents);
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

        // Check if repositories have been loaded
        if let Some(receiver) = self.repositories_receiver.as_mut() {
            if let Ok(repositories) = receiver.try_recv() {
                self.available_repositories = repositories;
                self.loading_repositories = false;
                self.repositories_receiver = None;
                self.needs_redraw = true; // Trigger redraw to update UI
            }
        }

        // Check if branches have been loaded
        if let Some(receiver) = self.branches_receiver.as_mut() {
            if let Ok(branches) = receiver.try_recv() {
                self.available_branches = branches;
                self.loading_branches = false;
                self.branches_receiver = None;
                self.needs_redraw = true; // Trigger redraw to update UI
            }
        }

        // Check if agents have been loaded
        if let Some(receiver) = self.agents_receiver.as_mut() {
            if let Ok(agents) = receiver.try_recv() {
                self.available_models = agents;
                self.loading_models = false;
                self.agents_receiver = None;
                self.needs_redraw = true; // Trigger redraw to update UI
            }
        }
    }

    fn apply_save_result(
        &mut self,
        draft_id: String,
        request_id: u64,
        generation: u64,
        result: SaveDraftResult,
    ) {
        if let Some(idx) = self.draft_cards.iter().position(|card| card.id == draft_id) {
            let update_footer_needed;
            {
                let card = &mut self.draft_cards[idx];
                if card.pending_save_request_id != Some(request_id) {
                    return;
                }

                card.pending_save_request_id = None;
                card.pending_save_invalidated = false;

                match result {
                    SaveDraftResult::Success => {
                        card.last_saved_generation = card.last_saved_generation.max(generation);

                        if card.dirty_generation == generation {
                            card.last_saved_generation = generation;
                            card.save_state = DraftSaveState::Saved;
                            card.auto_save_timer = None;
                            self.status_bar.error_message = None;
                        } else {
                            card.save_state = DraftSaveState::Unsaved;
                            card.auto_save_timer = Some(Instant::now());
                        }
                    }
                    SaveDraftResult::Failure { error } => {
                        card.save_state = DraftSaveState::Error;
                        card.auto_save_timer = None;
                        self.status_bar.error_message =
                            Some(format!("Draft auto-save failed: {}", error));
                    }
                }

                update_footer_needed = matches!(self.focus_element, DashboardFocusState::DraftTask(focus_idx) if focus_idx == idx);
            }

            self.needs_redraw = true;
            if update_footer_needed {
                self.update_footer();
            }
        }
    }

    fn try_start_auto_save(&mut self, idx: usize, now: Instant) -> bool {
        let Some(card) = self.draft_cards.get_mut(idx) else {
            return false;
        };

        if card.dirty_generation <= card.last_saved_generation {
            card.auto_save_timer = None;
            return false;
        }

        let Some(timer) = card.auto_save_timer else {
            return false;
        };

        if now.saturating_duration_since(timer) < AUTO_SAVE_DEBOUNCE {
            return false;
        }

        if card.pending_save_request_id.is_some() {
            return false;
        }

        let request_id = self.save_request_counter;
        self.save_request_counter = self.save_request_counter.wrapping_add(1);

        let payload = AutoSaveRequestPayload {
            draft_id: card.id.clone(),
            request_id,
            generation: card.dirty_generation,
            description: card.description.lines().join("\n"),
            repository: card.repository.clone(),
            branch: card.branch.clone(),
            models: card.selected_agents.clone(),
        };

        card.pending_save_request_id = Some(request_id);
        card.pending_save_invalidated = false;
        card.save_state = DraftSaveState::Saving;
        card.auto_save_timer = None;

        let update_footer_needed = matches!(self.focus_element, DashboardFocusState::DraftTask(focus_idx) if focus_idx == idx);

        self.needs_redraw = true;
        if update_footer_needed {
            self.update_footer();
        }

        let handle = tokio::runtime::Handle::try_current().expect(
            "Draft auto-save requires an active Tokio runtime; \
             ensure ViewModel operations run inside a Tokio executor",
        );
        let tx = self.ui_tx.clone();
        let task_manager = Arc::clone(&self.task_manager);
        let payload_clone = payload.clone();
        handle.spawn(async move {
            let result = task_manager
                .save_draft_task(
                    &payload_clone.draft_id,
                    &payload_clone.description,
                    &payload_clone.repository,
                    &payload_clone.branch,
                    &payload_clone.models,
                )
                .await;
            let _ = tx.send(Msg::DraftSaveCompleted {
                draft_id: payload_clone.draft_id,
                request_id: payload_clone.request_id,
                generation: payload_clone.generation,
                result,
            });
        });

        true
    }

    /// Close autocomplete if focus is moving away from textarea elements
    pub fn close_autocomplete_if_leaving_textarea(&mut self, new_focus: DashboardFocusState) {
        let was_on_textarea = matches!(self.focus_element, DashboardFocusState::DraftTask(0))
            || matches!(self.focus_element, DashboardFocusState::DraftTask(idx) if self.draft_cards.get(idx).is_some_and(|card| card.focus_element == CardFocusElement::TaskDescription));

        let moving_to_textarea = matches!(new_focus, DashboardFocusState::DraftTask(0))
            || matches!(new_focus, DashboardFocusState::DraftTask(idx) if self.draft_cards.get(idx).is_some_and(|card| card.focus_element == CardFocusElement::TaskDescription));

        if was_on_textarea && !moving_to_textarea {
            self.autocomplete.close(&mut self.needs_redraw);
            // Cancel any active text selection and dragging when leaving textarea
            if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                if let Some(card) = self.draft_cards.get_mut(idx) {
                    if card.focus_element == CardFocusElement::TaskDescription
                        && card.description.selection_range().is_some()
                    {
                        card.description.cancel_selection();
                    }
                }
            }
            self.stop_dragging();
        }

        // Sync cursor style when moving to a textarea
        if moving_to_textarea && new_focus != self.focus_element {
            self.sync_cursor_style_for_focused_textarea();
        }
    }

    /// Sync cursor style for the currently focused textarea
    fn sync_cursor_style_for_focused_textarea(&mut self) {
        // First, check if we have a focused textarea and get the overwrite state
        let overwrite_state = self
            .get_focused_draft_card()
            .map(|card| {
                card.focus_element == CardFocusElement::TaskDescription
                    && card.description.is_overwrite()
            })
            .unwrap_or(false);

        // Update ViewModel's cursor style based on overwrite mode
        self.cursor_style = if overwrite_state {
            crossterm::cursor::SetCursorStyle::SteadyBlock
        } else {
            crossterm::cursor::SetCursorStyle::SteadyBar
        };

        // Update visual cursor style within the TUI (for consistency, though disabled)
        if let Some(card) = self.get_focused_draft_card_mut() {
            if card.focus_element == CardFocusElement::TaskDescription {
                let visual_style = if card.description.is_overwrite() {
                    // Block cursor appearance for overwrite mode
                    ratatui::style::Style::default()
                        .add_modifier(ratatui::style::Modifier::REVERSED)
                        .add_modifier(ratatui::style::Modifier::BOLD)
                } else {
                    // Bar cursor appearance for insert mode
                    ratatui::style::Style::default()
                        .add_modifier(ratatui::style::Modifier::UNDERLINED)
                };
                card.description.set_cursor_style(visual_style);
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
        debug!(
            "Textarea click at screen ({}, {}) in textarea area {:?}",
            column, row, textarea_area
        );
        self.last_textarea_area = Some(*textarea_area);

        // Focus on the draft task description if not already focused
        if !matches!(self.focus_element, DashboardFocusState::DraftTask(0)) {
            self.change_focus(DashboardFocusState::DraftTask(0));
        }

        // Calculate relative position within textarea with padding awareness
        // Account for 1 character padding on left/right edges as per PRD
        let padding = 1u16;
        let raw_relative_x = column as i32 - textarea_area.x as i32 - padding as i32;
        let relative_x = raw_relative_x.max(0) as u16;
        let relative_y = (row as i32 - textarea_area.y as i32).max(0) as u16;
        debug!(
            "Relative position after padding: ({}, {})",
            relative_x, relative_y
        );

        // Check for multi-click timing (double-click threshold: 500ms)
        let now = std::time::Instant::now();
        let click_position = (column, row);
        let is_multi_click = if let (Some(last_time), Some(last_pos)) =
            (self.last_click_time, self.last_click_position)
        {
            now.duration_since(last_time).as_millis() < 500 && last_pos == click_position
        } else {
            false
        };

        if is_multi_click {
            self.click_count = (self.click_count % 4) + 1; // Cycle through 1-4 clicks
        } else {
            self.click_count = 1;
        }

        debug!(
            "Click count: {} (multi-click: {})",
            self.click_count, is_multi_click
        );
        self.last_click_time = Some(now);
        self.last_click_position = Some(click_position);

        // Stop dragging for multi-clicks before borrowing card
        if self.click_count > 1 {
            self.stop_dragging();
        }

        // Position caret and handle selection based on click count
        if let Some(card) = self.draft_cards.first_mut() {
            // Calculate precise cursor position
            let line_index =
                relative_y.min(card.description.lines().len().saturating_sub(1) as u16) as usize;
            let line = card.description.lines().get(line_index).map_or("", |s| s);
            debug!("Line {}: '{}'", line_index, line);

            // Find the character position that best matches the click location
            // This accounts for variable character widths (wide characters, emojis)
            let mut visual_width = 0u16;
            let mut col_index = 0;

            // If click was in the padding area (raw_relative_x < 0), position at start of line
            if raw_relative_x < 0 {
                // Position at beginning of line (like HOME key)
                col_index = 0;
            } else {
                for ch in line.chars() {
                    let char_width = if ch.is_ascii() { 1 } else { 2 }; // Simple heuristic for wide chars
                    visual_width += char_width;
                    col_index += 1;

                    // Position cursor after the character if click is within or at its end
                    if visual_width > relative_x {
                        break;
                    }
                }
            }
            debug!(
                "Calculated cursor position: line={}, col={}",
                line_index, col_index
            );

            match self.click_count {
                1 => {
                    // Single click: start text selection at clicked location
                    debug!(
                        "Single click: starting selection at ({}, {})",
                        line_index, col_index
                    );

                    // Start selection at clicked position
                    card.description.move_cursor(tui_textarea::CursorMove::Jump(
                        line_index as u16,
                        col_index as u16,
                    ));
                    card.description.start_selection();

                    // Initialize drag state
                    self.is_dragging = true;
                    self.drag_start_position = Some((column, row));
                    self.drag_start_bounds = Some(*textarea_area);

                    let (final_row, final_col) = card.description.cursor();
                    debug!("Selection started at ({}, {})", final_row, final_col);
                }
                2 => {
                    // Double click: select word at cursor position
                    card.description.cancel_selection();
                    card.description.move_cursor(tui_textarea::CursorMove::Jump(
                        line_index as u16,
                        col_index as u16,
                    ));

                    // Find word boundaries: move to end first, then back to start, then select to end
                    card.description.move_cursor(tui_textarea::CursorMove::WordBack);
                    card.description.start_selection();
                    card.description.move_cursor(tui_textarea::CursorMove::WordEnd);
                    // For some reason, the above command doesnt position the caret
                    // after the word end, but rather at the last character in the
                    // word, so we need to move forward one character
                    card.description.move_cursor(tui_textarea::CursorMove::Forward);
                }
                3 => {
                    // Triple click: select entire line
                    card.description.cancel_selection();
                    card.description.move_cursor(tui_textarea::CursorMove::Jump(
                        line_index as u16,
                        0, // Start of line
                    ));
                    card.description.start_selection();
                    card.description.move_cursor(tui_textarea::CursorMove::End); // End of line
                }
                4 => {
                    // Quadruple click: select entire textarea
                    card.description.select_all();
                }
                _ => unreachable!(),
            }

            self.autocomplete
                .after_textarea_change(&card.description, &mut self.needs_redraw);
        }

        self.needs_redraw = true;
    }

    /// Perform mouse action (similar to main.rs perform_mouse_action)
    pub fn perform_mouse_action(&mut self, action: MouseAction) {
        debug!("Processing mouse action: {:?}", action);
        match action {
            MouseAction::OpenSettings => {
                self.close_autocomplete_if_leaving_textarea(DashboardFocusState::SettingsButton);
                self.change_focus(DashboardFocusState::SettingsButton);
                self.open_modal(ModalState::Settings);
                // TODO: Initialize settings form
            }
            MouseAction::SelectCard(idx) => {
                self.selected_card = idx;
                let new_focus = if idx == 0 {
                    // Draft card - focus on description
                    DashboardFocusState::DraftTask(0)
                } else {
                    // Regular task card - idx is offset by 1, so array index is idx - 1
                    DashboardFocusState::ExistingTask(idx - 1)
                };
                self.close_autocomplete_if_leaving_textarea(new_focus);
                self.focus_element = new_focus;
            }
            MouseAction::SelectFilterBarLine => {
                self.close_autocomplete_if_leaving_textarea(DashboardFocusState::FilterBarLine);
                self.change_focus(DashboardFocusState::FilterBarLine);
            }
            MouseAction::ActivateRepositoryModal => {
                self.close_autocomplete_if_leaving_textarea(DashboardFocusState::RepositoryButton);
                self.change_focus(DashboardFocusState::RepositoryButton);
                self.open_modal(ModalState::RepositorySearch);
            }
            MouseAction::ActivateBranchModal => {
                self.close_autocomplete_if_leaving_textarea(DashboardFocusState::BranchButton);
                self.change_focus(DashboardFocusState::BranchButton);
                self.open_modal(ModalState::BranchSearch);
            }
            MouseAction::ActivateModelModal => {
                self.close_autocomplete_if_leaving_textarea(DashboardFocusState::ModelButton);
                self.change_focus(DashboardFocusState::ModelButton);
                self.open_modal(ModalState::ModelSearch);
            }
            MouseAction::LaunchTask => {
                self.close_autocomplete_if_leaving_textarea(DashboardFocusState::DraftTask(0));
                self.change_focus(DashboardFocusState::DraftTask(0));
                let default_split_mode = self.settings.default_split_mode();
                // Use advanced options from draft card, or defaults if not configured
                let advanced_options = self
                    .draft_cards
                    .first()
                    .and_then(|card| card.advanced_options.clone())
                    .or_else(|| Some(AdvancedLaunchOptions::default()));
                self.launch_task(0, default_split_mode, false, None, None, advanced_options);
            }
            MouseAction::ActivateAdvancedOptionsModal => {
                self.close_autocomplete_if_leaving_textarea(DashboardFocusState::DraftTask(0));
                self.change_focus(DashboardFocusState::DraftTask(0));
                if let Some(card) = self.draft_cards.first() {
                    let draft_id = card.id.clone();
                    self.open_launch_options_modal(draft_id);
                } else {
                    trace!("perform_mouse_action: no draft card found at index 0");
                }
            }
            MouseAction::FocusDraftTextarea(_idx) => {
                self.close_autocomplete_if_leaving_textarea(DashboardFocusState::DraftTask(0));
                self.change_focus(DashboardFocusState::DraftTask(0));
            }
            MouseAction::AutocompleteSelect(index) => {
                if self.autocomplete.is_open() {
                    if let Some(draft_index) = self.focused_textarea_index() {
                        if let Some(card) = self.draft_cards.get_mut(draft_index) {
                            if self.autocomplete.set_selected_index(index) {
                                self.needs_redraw = true;
                            }

                            // Combine commit vs ghost accept logic: both paths set `changed = true`.
                            // We attempt commit first (higher-confidence), otherwise attempt ghost acceptance.
                            let changed = self.autocomplete.commit_current_selection(
                                &mut card.description,
                                &mut self.needs_redraw,
                            ) || self.autocomplete.accept_ghost(
                                &mut card.description,
                                &mut self.needs_redraw,
                                true,
                            );

                            if changed {
                                card.on_content_changed();
                                card.autocomplete.after_textarea_change(
                                    &card.description,
                                    &mut self.needs_redraw,
                                );
                                self.autocomplete.after_textarea_change(
                                    &card.description,
                                    &mut self.needs_redraw,
                                );
                                self.mark_draft_dirty(draft_index);
                            }
                        }
                    }
                }
            }
            MouseAction::ModelIncrementCount(index) => {
                if let Some(modal) = &mut self.active_modal {
                    if let crate::view_model::ModalType::AgentSelection { options } =
                        &mut modal.modal_type
                    {
                        if index < options.len() {
                            options[index].count = options[index].count.saturating_add(1);
                            self.needs_redraw = true;
                        }
                    }
                }
            }
            MouseAction::ModelDecrementCount(index) => {
                if let Some(modal) = &mut self.active_modal {
                    if let crate::view_model::ModalType::AgentSelection { options } =
                        &mut modal.modal_type
                    {
                        if index < options.len() && options[index].count > 0 {
                            options[index].count = options[index].count.saturating_sub(1);
                            self.needs_redraw = true;
                        }
                    }
                }
            }
            MouseAction::ModalSelectOption(index) => {
                if let Some(modal) = &self.active_modal {
                    match &modal.modal_type {
                        ModalType::AgentSelection { options } => {
                            // For ModelSelection modals, get the option name from the options array
                            if let Some(option) = options.get(index) {
                                self.apply_modal_selection(
                                    modal.modal_type.clone(),
                                    option.name.clone(),
                                );
                            }
                        }
                        _ => {
                            // For other modal types, use filtered_options
                            if let Some(FilteredOption::Option { text, .. }) =
                                modal.filtered_options.get(index)
                            {
                                self.apply_modal_selection(modal.modal_type.clone(), text.clone());
                            }
                        }
                    }
                }
            }
            MouseAction::ModalApplyChanges => {
                // Apply changes and close modal (same logic as 'A' key)
                if let Some(modal) = &self.active_modal {
                    if let ModalType::LaunchOptions { view_model } = &modal.modal_type {
                        // Save the current config to the draft card
                        if let Some(card) =
                            self.draft_cards.iter_mut().find(|card| card.id == view_model.draft_id)
                        {
                            card.advanced_options = Some(view_model.config.clone());
                        }
                        self.close_modal(true); // Save changes

                        // Restore focus to TaskDescription (as per PRD)
                        self.focus_element = DashboardFocusState::DraftTask(0);
                        if let Some(card) = self.draft_cards.first_mut() {
                            card.focus_element = CardFocusElement::TaskDescription;
                        }
                    }
                }
            }
            MouseAction::ModalCancelChanges => {
                // Discard changes and close modal (same logic as 'Esc' key)
                self.handle_dismiss_overlay();
            }
            _ => {
                // TODO: Handle other mouse actions like ActivateGoButton, StopTask, EditFilter, Footer
            }
        }
        self.needs_redraw = true;
    }

    // Process any pending task events from the event receiver

    // Update the selection state in task cards based on current focus_element

    /// Get the active minor modes for the current focus state and modal state
    fn get_active_minor_modes(&self) -> Vec<&'static crate::view_model::input::InputMinorMode> {
        use crate::view_model::input::minor_modes;

        let mut modes = Vec::new();

        // Always include standard navigation
        modes.push(&minor_modes::STANDARD_NAVIGATION_MODE);

        match (self.focus_element, self.modal_state) {
            // Modal contexts
            (_, ModalState::RepositorySearch)
            | (_, ModalState::BranchSearch)
            | (_, ModalState::ModelSearch)
            | (_, ModalState::Settings) => {
                modes.push(&minor_modes::SELECTION_PROMINENT_MODE);
            }

            // Draft textarea focused
            (DashboardFocusState::DraftTask(_), ModalState::None) => {
                modes.push(&minor_modes::TEXT_EDITING_PROMINENT_MODE);
            }

            // Navigation contexts (task feed, draft card selected, active/completed tasks)
            _ => {
                modes.push(&minor_modes::NAVIGATION_PROMINENT_MODE);
            }
        }

        modes
    }

    /// Create footer shortcuts from the prominent operations of active minor modes
    fn create_footer_from_minor_modes(&self) -> FooterViewModel {
        use crate::settings::KeyboardShortcut;

        let active_modes = self.get_active_minor_modes();
        let mut shortcuts = Vec::new();

        // Collect all prominent operations from active modes
        for mode in active_modes {
            for &operation in mode.prominent_operations() {
                // Get the key bindings for this operation from settings
                let bindings = self.settings.keymap().get_matchers(operation);
                if !bindings.is_empty() {
                    shortcuts.push(KeyboardShortcut::new(operation, bindings));
                }
            }
        }

        FooterViewModel { shortcuts }
    }

    /// Update the footer based on current focus state
    pub fn update_footer(&mut self) {
        self.footer = self.create_footer_from_minor_modes();
    }

    /// Change the current focus state and update dependent UI elements
    /// This should be used instead of directly setting focus_element
    pub fn change_focus(&mut self, new_focus: DashboardFocusState) {
        self.focus_element = new_focus;
        self.update_footer();
        self.needs_redraw = true;
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
                selected_agents: card.selected_agents.clone(),
                created_at: card.created_at.clone(),
            }),
            self.settings.activity_rows(),
            self.word_wrap_enabled,
            self.show_autocomplete_border,
        );
        self.exit_confirmation_armed = false;
    }

    /// Open the advanced launch options modal for a draft task
    pub fn open_launch_options_modal(&mut self, draft_id: String) {
        trace!(
            "open_launch_options_modal called with draft_id: {}",
            draft_id
        );
        trace!("current modal_state: {:?}", self.modal_state);
        trace!("active_modal is_some: {}", self.active_modal.is_some());

        // Load existing advanced options from the draft card, or use defaults
        let config = self
            .draft_cards
            .iter()
            .find(|card| card.id == draft_id)
            .and_then(|card| card.advanced_options.clone())
            .unwrap_or_default();

        let modal = ModalViewModel {
            title: "Advanced Launch Options".to_string(),
            input_value: String::new(),
            filtered_options: vec![],
            selected_index: 0,
            modal_type: ModalType::LaunchOptions {
                view_model: LaunchOptionsViewModel {
                    draft_id,
                    original_config: config.clone(), // Store original for restoration on Esc
                    config,
                    active_column: LaunchOptionsColumn::Options,
                    selected_option_index: 0,
                    selected_action_index: 0,
                    inline_enum_popup: None,
                },
            },
        };

        self.active_modal = Some(modal);
        self.modal_state = ModalState::LaunchOptions;
        self.exit_confirmation_armed = false;
    }

    /// Close the current modal
    pub fn close_modal(&mut self, save_changes: bool) {
        // For Launch Options modal, save or restore the config based on save_changes flag
        if let Some(modal) = &self.active_modal {
            if let ModalType::LaunchOptions { view_model } = &modal.modal_type {
                // Find the draft card
                if let Some(card) =
                    self.draft_cards.iter_mut().find(|card| card.id == view_model.draft_id)
                {
                    if save_changes {
                        // Save the modified config to the draft card
                        card.advanced_options = Some(view_model.config.clone());
                        tracing::debug!(
                            "✅ Saved advanced options for draft card: {}",
                            view_model.draft_id
                        );
                    } else {
                        // Discard changes and restore original config (Esc behavior)
                        card.advanced_options = Some(view_model.original_config.clone());
                        tracing::debug!(
                            "🔄 Restored original advanced options for draft card: {} (changes discarded)",
                            view_model.draft_id
                        );
                    }
                }
            }
        }

        self.modal_state = ModalState::None;
        self.active_modal = None;
        self.exit_confirmation_armed = false;
    }

    /// Select a repository from modal
    pub fn select_repository(&mut self, repo: String) {
        let mut update_footer_needed = false;
        if let DashboardFocusState::DraftTask(idx) = self.focus_element {
            if let Some(draft_card) = self.draft_cards.get_mut(idx) {
                if draft_card.repository != repo {
                    draft_card.repository = repo;
                    draft_card.on_content_changed();
                    update_footer_needed = true;
                }
            }
            self.needs_redraw = true;
        }
        self.close_modal(true); // Applying selection - save changes
        if update_footer_needed {
            self.update_footer();
        }
    }

    /// Select a branch from modal
    pub fn select_branch(&mut self, branch: String) {
        let mut update_footer_needed = false;
        if let DashboardFocusState::DraftTask(idx) = self.focus_element {
            if let Some(draft_card) = self.draft_cards.get_mut(idx) {
                if draft_card.branch != branch {
                    draft_card.branch = branch;
                    draft_card.on_content_changed();
                    update_footer_needed = true;
                }
            }
            self.needs_redraw = true;
        }
        self.close_modal(true); // Applying selection - save changes
        if update_footer_needed {
            self.update_footer();
        }
    }

    /// Select model names from modal
    pub fn select_model_names(&mut self, model_names: Vec<String>) {
        let mut update_footer_needed = false;
        if let DashboardFocusState::DraftTask(idx) = self.focus_element {
            if let Some(draft_card) = self.draft_cards.get_mut(idx) {
                let new_models: Vec<AgentChoice> = model_names
                    .into_iter()
                    .filter_map(|display_name| {
                        self.available_models
                            .iter()
                            .find(|model| model.display_name() == display_name)
                            .map(|model| AgentChoice {
                                agent: model.agent.clone(),
                                model: model.model.clone(),
                                count: 1,
                                settings: model.settings.clone(),
                                display_name: model.display_name.clone(),
                                acp_stdio_launch_command: model.acp_stdio_launch_command.clone(),
                            })
                    })
                    .collect();
                if draft_card.selected_agents != new_models {
                    draft_card.selected_agents = new_models;
                    draft_card.on_content_changed();
                    update_footer_needed = true;
                }
            }
            self.needs_redraw = true;
        }
        self.close_modal(true); // Applying selection - save changes
        if update_footer_needed {
            self.update_footer();
        }
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

    /// Find TaskCardInfo for a given task_id by iterating over the cards
    pub fn find_task_card_info(&self, task_id: &str) -> Option<TaskCardInfo> {
        // Check draft cards first
        for (index, card) in self.draft_cards.iter().enumerate() {
            if card.id == task_id {
                return Some(TaskCardInfo {
                    card_type: TaskCardTypeEnum::Draft,
                    index,
                });
            }
        }

        // Check task cards
        for (index, card) in self.task_cards.iter().enumerate() {
            if let Ok(card_guard) = card.lock() {
                if card_guard.task.id == task_id {
                    return Some(TaskCardInfo {
                        card_type: TaskCardTypeEnum::Task,
                        index,
                    });
                }
            }
        }

        None
    }

    // Handle launch task operation and return domain messages

    /// Process a TaskEvent and update the corresponding task card's activity entries
    pub fn process_task_event(&mut self, task_id: &str, event: TaskEvent) {
        debug!("Processing task event for task {}: {:?}", task_id, event);
        // Find the card info for this task_id
        if let Some(card_info) = self.find_task_card_info(task_id) {
            match card_info.card_type {
                TaskCardTypeEnum::Draft => {
                    // Draft cards don't have activity events - they're just text inputs
                    // Task events for draft cards don't make sense in this context
                }
                TaskCardTypeEnum::Task => {
                    if card_info.index < self.task_cards.len() {
                        if let Ok(mut card) = self.task_cards[card_info.index].lock() {
                            card.process_task_event(event, &self.settings);
                        }
                    }
                }
            }
        }
    }

    /// Process any pending task events (non-blocking)
    /// Check if any task cards need redrawing due to recent event processing
    pub fn process_pending_task_events(&mut self) {
        // Check if any task cards need redrawing
        let mut needs_redraw = false;
        for task_card in &self.task_cards {
            if let Ok(card) = task_card.lock() {
                if card.needs_redraw() {
                    needs_redraw = true;
                    // Clear the flag after checking (lazy redrawing)
                    // We can't modify the card here since we have a read lock
                    break;
                }
            }
        }

        if needs_redraw {
            // Clear redraw flags on all task cards that need it
            for task_card in &self.task_cards {
                if let Ok(mut card) = task_card.lock() {
                    if card.needs_redraw() {
                        card.clear_needs_redraw();
                    }
                }
            }
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
                    selected_agents: if draft_info.models.is_empty() {
                        vec![AgentChoice {
                            agent: AgentSoftwareBuild {
                                software: AgentSoftware::Claude,
                                version: "latest".to_string(),
                            },
                            model: "sonnet".to_string(),
                            count: 1,
                            settings: std::collections::HashMap::new(),
                            display_name: Some("Claude Sonnet".to_string()),
                            acp_stdio_launch_command: None,
                        }] // Default model if none saved
                    } else {
                        draft_info.models.clone()
                    },
                    created_at: draft_info.created_at,
                };
                let draft_card = create_draft_card_from_task(
                    draft,
                    CardFocusElement::TaskDescription,
                    Some(self.repositories_enumerator.clone()),
                    Some(self.branches_enumerator.clone()),
                    Arc::clone(&self.workspace_files),
                    Arc::clone(&self.workspace_workflows),
                    Arc::clone(&self.workspace_terms),
                );
                self.draft_cards.push(draft_card);
            }
        }

        // Convert task TaskExecution to task cards
        for task_execution in &task_executions {
            let task_card = create_task_card_from_execution(task_execution.clone(), &self.settings);
            let task_card_arc = Arc::new(Mutex::new(task_card));
            self.task_cards.push(task_card_arc);
        }

        // UI is already updated since we pushed the cards directly

        // Note: Active tasks loaded from the task manager should already have their own
        // spawned event consumers. The main thread doesn't need to get receivers for them.

        Ok(())
    }

    /// Get the currently focused draft card (mutable reference)
    pub fn get_focused_draft_card_mut(&mut self) -> Option<&mut TaskEntryViewModel> {
        if let DashboardFocusState::DraftTask(index) = self.focus_element {
            self.draft_cards.get_mut(index)
        } else {
            None
        }
    }

    /// Get the currently focused draft card (immutable reference)
    pub fn get_focused_draft_card(&self) -> Option<&TaskEntryViewModel> {
        if let DashboardFocusState::DraftTask(index) = self.focus_element {
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
        let models = card.selected_agents.clone();

        // Find and update the draft card in the view model to show "Saving" state
        // Note: We search by ID, not by current focus, since focus might change during await
        if let Some(card) = self.draft_cards.iter_mut().find(|c| c.id == draft_id) {
            card.save_state = DraftSaveState::Saving;
            card.pending_save_request_id = None;
            card.pending_save_invalidated = false;
            card.auto_save_timer = None;
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
                    card.last_saved_generation = card.dirty_generation;
                    card.auto_save_timer = None;
                    Ok(())
                }
                SaveDraftResult::Failure { error } => {
                    card.save_state = DraftSaveState::Error;
                    card.auto_save_timer = None;
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
        if let DashboardFocusState::DraftTask(idx) = self.focus_element {
            self.mark_draft_dirty(idx);
        }
    }

    fn mark_draft_dirty(&mut self, idx: usize) {
        let mut update_footer_needed = false;
        if let Some(card) = self.draft_cards.get_mut(idx) {
            card.on_content_changed();
            update_footer_needed =
                matches!(self.focus_element, DashboardFocusState::DraftTask(i) if i == idx);
        }
        self.needs_redraw = true;
        if update_footer_needed {
            self.update_footer();
        }
    }

    // Domain business logic methods (moved from Model)

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
            card.description.disable_cursor_rendering();
        }
    }

    /// Set draft repository
    pub fn set_draft_repository(&mut self, repo: &str, draft_id: &str) {
        if let Some(card) = self.draft_cards.iter_mut().find(|c| c.id == draft_id) {
            if card.repository != repo {
                card.repository = repo.to_string();
                card.on_content_changed();
            }
        }
    }

    /// Set draft branch
    pub fn set_draft_branch(&mut self, branch: &str, draft_id: &str) {
        if let Some(card) = self.draft_cards.iter_mut().find(|c| c.id == draft_id) {
            if card.branch != branch {
                card.branch = branch.to_string();
                card.on_content_changed();
            }
        }
    }

    /// Set draft model names
    pub fn set_draft_model_names(&mut self, model_names: Vec<String>, draft_id: &str) {
        if let Some(card) = self.draft_cards.iter_mut().find(|c| c.id == draft_id) {
            let new_models: Vec<AgentChoice> = model_names
                .into_iter()
                .filter_map(|display_name| {
                    self.available_models
                        .iter()
                        .find(|model| model.display_name() == display_name)
                        .map(|model| AgentChoice {
                            agent: model.agent.clone(),
                            model: model.model.clone(),
                            count: 1,
                            settings: model.settings.clone(),
                            display_name: model.display_name.clone(),
                            acp_stdio_launch_command: model.acp_stdio_launch_command.clone(),
                        })
                })
                .collect();
            if card.selected_agents != new_models {
                card.selected_agents = new_models;
                card.on_content_changed();
            }
        }
    }

    /// Update active task activities (simulation)
    pub fn update_active_task_activities(&mut self) -> bool {
        // Simulate activity updates for active tasks
        let mut had_changes = false;
        for card in &self.task_cards {
            if let Ok(card_guard) = card.lock() {
                let is_active = matches!(
                    card_guard.task.state,
                    TaskState::Queued
                        | TaskState::Provisioning
                        | TaskState::Running
                        | TaskState::Pausing
                        | TaskState::Paused
                        | TaskState::Resuming
                        | TaskState::Stopping
                        | TaskState::Stopped
                );
                if is_active {
                    // In real implementation, would receive via SSE
                    // For testing, simulate random activities
                    // For now, just return false since we don't actually change anything
                    had_changes = false;
                }
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
        // Convert string models to AgentChoice structs
        // For now, assume all models are Claude models - this may need refinement
        self.available_models = models
            .into_iter()
            .map(|display_name| {
                // Extract software and model from display name pattern
                let (software, model) = if display_name.starts_with("Claude") {
                    let model_name = if display_name.contains("Sonnet") {
                        "sonnet".to_string()
                    } else if display_name.contains("Opus") {
                        "opus".to_string()
                    } else {
                        display_name.to_lowercase().replace(" ", "")
                    };
                    (AgentSoftware::Claude, model_name)
                } else if display_name.starts_with("GPT-5") {
                    let model_name = display_name.to_lowercase().replace(" ", "");
                    (AgentSoftware::Codex, model_name)
                } else {
                    (AgentSoftware::Claude, display_name.to_string())
                };
                AgentChoice {
                    agent: AgentSoftwareBuild {
                        software,
                        version: "latest".to_string(),
                    },
                    model,
                    count: 1,
                    settings: std::collections::HashMap::new(),
                    display_name: Some(display_name),
                    acp_stdio_launch_command: None,
                }
            })
            .collect();
        self.loading_models = false;
        // Clear cache since models changed
        self.model_display_names_cache = None;
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
            DashboardFocusState::SettingsButton => {
                // At top, wrap to bottom (last existing task or filter separator or last draft)
                if !self.task_cards.is_empty() {
                    DashboardFocusState::ExistingTask(self.task_cards.len() - 1)
                } else if self.draft_cards.is_empty() {
                    DashboardFocusState::FilterBarSeparator
                } else {
                    DashboardFocusState::DraftTask(self.draft_cards.len() - 1)
                }
            }
            DashboardFocusState::DraftTask(idx) => {
                if idx == 0 {
                    // First draft, go to settings
                    DashboardFocusState::SettingsButton
                } else {
                    // Previous draft
                    DashboardFocusState::DraftTask(idx - 1)
                }
            }
            DashboardFocusState::FilterBarSeparator => {
                // From filter separator, go to last draft or settings
                if !self.draft_cards.is_empty() {
                    DashboardFocusState::DraftTask(self.draft_cards.len() - 1)
                } else {
                    DashboardFocusState::SettingsButton
                }
            }
            DashboardFocusState::ExistingTask(idx) => {
                if idx == 0 {
                    // First existing task, go to filter separator
                    DashboardFocusState::FilterBarSeparator
                } else {
                    // Previous existing task
                    DashboardFocusState::ExistingTask(idx - 1)
                }
            }
            // Other focus elements stay the same
            other => other,
        };

        if new_focus != self.focus_element {
            // Reset internal focus of draft cards when they lose global focus
            if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                if !matches!(new_focus, DashboardFocusState::DraftTask(_)) {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        card.focus_element = CardFocusElement::TaskDescription;
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
            DashboardFocusState::SettingsButton => {
                // From settings, go to first draft or filter separator or first existing
                if !self.draft_cards.is_empty() {
                    DashboardFocusState::DraftTask(0)
                } else if !self.task_cards.is_empty() {
                    DashboardFocusState::FilterBarSeparator
                } else {
                    DashboardFocusState::ExistingTask(0)
                }
            }
            DashboardFocusState::DraftTask(idx) => {
                if idx >= self.draft_cards.len() - 1 {
                    // Last draft, go to filter separator if we have existing tasks
                    if !self.task_cards.is_empty() {
                        DashboardFocusState::FilterBarSeparator
                    } else {
                        // No existing tasks, wrap to settings
                        DashboardFocusState::SettingsButton
                    }
                } else {
                    // Next draft
                    DashboardFocusState::DraftTask(idx + 1)
                }
            }
            DashboardFocusState::FilterBarSeparator => {
                // From filter separator, go to first existing task or wrap to settings
                if !self.task_cards.is_empty() {
                    DashboardFocusState::ExistingTask(0)
                } else {
                    DashboardFocusState::SettingsButton
                }
            }
            DashboardFocusState::ExistingTask(idx) => {
                if idx >= self.task_cards.len() - 1 {
                    // Last existing task, wrap to settings
                    DashboardFocusState::SettingsButton
                } else {
                    // Next existing task
                    DashboardFocusState::ExistingTask(idx + 1)
                }
            }
            // Other focus elements stay the same
            other => other,
        };

        if new_focus != self.focus_element {
            // Reset internal focus of draft cards when they lose global focus
            if let DashboardFocusState::DraftTask(idx) = self.focus_element {
                if !matches!(new_focus, DashboardFocusState::DraftTask(_)) {
                    if let Some(card) = self.draft_cards.get_mut(idx) {
                        card.focus_element = CardFocusElement::TaskDescription;
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
    // Disable cursor rendering since we use real terminal cursor
    textarea.disable_cursor_rendering();
    if text.is_empty() {
        textarea.set_placeholder_text("Describe what you want the agent to do...");
    }
    textarea
}

/// Create a draft card from a DraftTask
pub fn create_draft_card_from_task(
    task: DraftTask,
    focus_element: CardFocusElement,
    repositories_enumerator: Option<Arc<dyn RepositoriesEnumerator>>,
    branches_enumerator: Option<Arc<dyn BranchesEnumerator>>,
    workspace_files: Arc<dyn WorkspaceFilesEnumerator>,
    workspace_workflows: Arc<dyn WorkspaceWorkflowsEnumerator>,
    workspace_terms: Arc<dyn WorkspaceTermsEnumerator>,
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
                .selected_agents
                .first()
                .map(|m| format!("{:?} {}", m.agent.software, m.agent.version))
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

    let deps = Arc::new(crate::view_model::autocomplete::AutocompleteDependencies {
        workspace_files,
        workspace_workflows,
        workspace_terms,
        settings: crate::settings::Settings::default(),
    });

    // Create autocomplete dependencies if enumerators are available
    let autocomplete = crate::view_model::autocomplete::InlineAutocomplete::with_dependencies(deps);

    TaskEntryViewModel {
        id: task.id,
        repository: task.repository,
        branch: task.branch,
        selected_agents: task.selected_agents,
        created_at: task.created_at,
        height,
        controls,
        save_state: DraftSaveState::Unsaved,
        description,
        focus_element,
        auto_save_timer: None,
        dirty_generation: 0,
        last_saved_generation: 0,
        pending_save_request_id: None,
        pending_save_invalidated: false,
        advanced_options: None, // No advanced options by default
        repositories_enumerator,
        branches_enumerator,
        autocomplete,
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
        // Map running/intermediate states to Active card type
        TaskState::Queued
        | TaskState::Provisioning
        | TaskState::Running
        | TaskState::Pausing
        | TaskState::Paused
        | TaskState::Resuming
        | TaskState::Stopping
        | TaskState::Stopped => {
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
        // Map final states to Completed card type
        TaskState::Completed | TaskState::Failed | TaskState::Cancelled => {
            TaskCardType::Completed {
                delivery_indicators: match task.state {
                    TaskState::Failed => "Failed".to_string(),
                    TaskState::Cancelled => "Cancelled".to_string(),
                    _ => String::new(),
                },
            }
        }
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
        focus_element: TaskExecutionFocusState::None, // Default focus for task cards (not used)
        needs_redraw: false,
    }
}

/// Create ViewModel representations for draft tasks
#[allow(dead_code)] // Future: used by multi-draft rendering pipeline; kept for planned dashboard expansion.
fn create_draft_card_view_models(
    draft_tasks: &[DraftTask],
    _task_executions: &[TaskExecution],
    _focus_element: DashboardFocusState,
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
                        .selected_agents
                        .first()
                        .map(|m| format!("{:?} {}", m.agent.software, m.agent.version))
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

            #[allow(unreachable_code)]
            TaskEntryViewModel {
                id: draft.id.clone(),
                repository: draft.repository.clone(),
                branch: draft.branch.clone(),
                selected_agents: draft.selected_agents.clone(),
                created_at: draft.created_at.clone(),
                height,
                controls,
                save_state: DraftSaveState::Unsaved,
                description: textarea,
                focus_element: CardFocusElement::TaskDescription,
                auto_save_timer: None,
                dirty_generation: 0,
                last_saved_generation: 0,
                pending_save_request_id: None,
                pending_save_invalidated: false,
                advanced_options: None, // No advanced options by default
                repositories_enumerator: None,
                branches_enumerator: None,
                autocomplete: panic!("This function should not be used"),
            }
        })
        .collect()
}

/// Create ViewModel representations for regular tasks (active/completed/merged)
#[allow(dead_code)] // Legacy task card VM builder retained for reference; draft+execution unified path forthcoming.
fn create_task_card_view_models(
    draft_tasks: &[DraftTask],
    task_executions: &[TaskExecution],
    settings: &Settings,
) -> Vec<TaskExecutionViewModel> {
    let visible_tasks = TaskItem::all_tasks_from_state(draft_tasks, task_executions);

    visible_tasks
        .into_iter()
        .map(|task_item| {
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
                        // Map running/intermediate states to Active card type
                        TaskState::Queued
                        | TaskState::Provisioning
                        | TaskState::Running
                        | TaskState::Pausing
                        | TaskState::Paused
                        | TaskState::Resuming
                        | TaskState::Stopping
                        | TaskState::Stopped => TaskCardType::Active {
                            activity_entries: task_execution
                                .activity
                                .iter()
                                .map(|activity| AgentActivityRow::AgentThought {
                                    thought: activity.clone(),
                                })
                                .collect(),
                            pause_delete_buttons: "Pause | Delete".to_string(),
                        },
                        // Map final states to Completed card type
                        TaskState::Completed | TaskState::Failed | TaskState::Cancelled => {
                            TaskCardType::Completed {
                                delivery_indicators: match task_execution.state {
                                    TaskState::Failed => "Failed".to_string(),
                                    TaskState::Cancelled => "Cancelled".to_string(),
                                    _ => String::new(),
                                },
                            }
                        }
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
                        focus_element: TaskExecutionFocusState::None,
                        needs_redraw: false,
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

#[allow(dead_code)] // Height calculation will be replaced by dynamic layout metrics; kept for transition.
fn calculate_card_height(task: &TaskExecution, settings: &Settings) -> u16 {
    // Calculate height based on activity lines + fixed overhead
    let activity_lines = settings.activity_rows().min(task.activity.len()) as u16;
    3 + activity_lines // Header + metadata + activity
}

#[allow(clippy::too_many_arguments)] // Modal factory needs separate slices & flags; wrapping would add unnecessary indirection currently.
fn create_modal_view_model(
    modal_state: ModalState,
    available_repositories: &[String],
    available_branches: &[String],
    available_models: &[AgentChoice],
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
            // Create model options from available models, with counts from current draft
            let model_options: Vec<AgentSelectionViewModel> = available_models
                .iter()
                .map(|model_info| {
                    let count = current_draft
                        .as_ref()
                        .and_then(|draft| {
                            draft.selected_agents.iter().find(|m| {
                                m.agent.software == model_info.agent.software
                                    && m.agent.version == model_info.agent.version
                            })
                        })
                        .map(|m| m.count)
                        .unwrap_or(0);
                    AgentSelectionViewModel {
                        name: model_info.display_name(),
                        count,
                        is_selected: count > 0,
                    }
                })
                .collect();

            // Find selected index (first model with count > 0, or 0 if none)
            let selected_index = model_options.iter().position(|opt| opt.is_selected).unwrap_or(0);

            // Create initial filtered options (all models with their counts)
            let filtered_options: Vec<FilteredOption> = model_options
                .iter()
                .enumerate()
                .map(|(i, opt)| FilteredOption::Option {
                    text: format!("{} (x{})", opt.name, opt.count),
                    selected: i == selected_index,
                })
                .collect();

            Some(ModalViewModel {
                title: "Select models".to_string(),
                input_value: String::new(),
                filtered_options,
                selected_index,
                modal_type: ModalType::AgentSelection {
                    options: model_options,
                },
            })
        }
        ModalState::LaunchOptions => {
            // Launch options modal is created directly by open_launch_options_modal
            None
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
) -> (Vec<FilteredOption>, usize) {
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
            FilteredOption::Option {
                text: value.clone(),
                selected: false,
            }
        })
        .collect::<Vec<_>>();

    let mut filtered_options = filtered_options;
    if let Some(FilteredOption::Option { selected, .. }) = filtered_options.get_mut(selected_index)
    {
        *selected = true;
    }

    (filtered_options, selected_index)
}

fn create_footer_view_model(
    focused_draft: Option<&DraftTask>,
    focus_element: DashboardFocusState,
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
                KeyboardOperation::ActivateCurrentItem,
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
                KeyboardOperation::ActivateCurrentItem,
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
        (_, ModalState::LaunchOptions) => {
            // Launch options modal: "↑↓ Navigate • Enter Select • Esc Back • b/s/h/v Shortcuts"
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
                KeyboardOperation::ActivateCurrentItem,
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
        (DashboardFocusState::DraftTask(_), ModalState::None) if focused_draft.is_some() => {
            // Draft textarea focused: "Ctrl+Enter Advanced Options • Enter Launch Agent(s) • Shift+Enter New Line • Tab Complete/Next Field"
            shortcuts.push(KeyboardShortcut::new(
                KeyboardOperation::ShowLaunchOptions,
                vec![KeyMatcher::new(
                    KeyCode::Enter,
                    KeyModifiers::CONTROL,
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
                KeyboardOperation::OpenNewLine,
                vec![KeyMatcher::new(
                    KeyCode::Enter,
                    KeyModifiers::SHIFT,
                    KeyModifiers::empty(),
                    None,
                )],
            ));
            shortcuts.push(KeyboardShortcut::new(
                KeyboardOperation::IndentOrComplete,
                vec![KeyMatcher::new(
                    KeyCode::Tab,
                    KeyModifiers::empty(),
                    KeyModifiers::empty(),
                    None,
                )],
            ));
        }
        (DashboardFocusState::ExistingTask(_), ModalState::None) => {
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
                KeyboardOperation::ActivateCurrentItem,
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
                KeyboardOperation::ActivateCurrentItem,
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
