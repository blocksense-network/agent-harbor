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

use crate::model::{Model, TaskExecution, TaskItem, DraftTask, SelectedModel, DeliveryStatus, DomainMsg, TaskState};
use crate::workspace_files::{WorkspaceFiles, RepositoryFile};
use crate::workspace_workflows::WorkspaceWorkflows;
use ah_workflows::WorkflowError;
use crossterm::event::{KeyCode, KeyModifiers, KeyEvent, MouseEvent};

/// UI-level messages that are handled by the ViewModel
#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    /// User keyboard input events (translated to domain messages by ViewModel)
    Key(KeyEvent),
    /// User mouse input events
    Mouse(MouseEvent),
    /// Periodic timer tick for animations/updates
    Tick,
    /// Application lifecycle events
    Quit,
}

/// User interface focus states
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FocusElement {
    /// Focus on settings button (top of screen)
    Settings,
    /// Focus on draft task by index
    DraftTask(usize),
    /// Focus on filter bar separator line
    FilterBarSeparator,
    /// Focus on existing task by index
    ExistingTask(usize),
    /// Focus on draft task description textarea
    TaskDescription,
    /// Focus on repository selector
    RepositorySelector,
    /// Focus on branch selector
    BranchSelector,
    /// Focus on model selector
    ModelSelector,
    /// Focus on Go button
    GoButton,
    /// Focus on filter controls
    Filter(usize), // index of filter button
}

/// Modal dialog states
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ModalState {
    /// No modal active
    None,
    /// Repository search modal
    RepositorySearch,
    /// Branch search modal
    BranchSearch,
    /// Model selection modal
    ModelSearch,
    /// Model multi-selection modal
    ModelSelection,
    /// Settings modal
    Settings,
    /// Go to line modal
    GoToLine,
    /// Find and replace modal
    FindReplace,
    /// Keyboard shortcut help modal
    ShortcutHelp,
}

/// Search and filter modes
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SearchMode {
    /// No search active
    None,
    /// Fuzzy search mode
    Fuzzy,
    /// Text search mode
    Text,
}

/// Task card presentation model - how tasks are displayed in the UI
#[derive(Debug, Clone, PartialEq)]
pub struct TaskCard {
    pub id: String,
    pub title: String,
    pub repository: String,
    pub branch: String,
    pub agents: Vec<SelectedModel>,
    pub state: TaskState,
    pub timestamp: String,
    pub activity: Vec<String>, // For active tasks
    pub delivery_indicators: Vec<DeliveryIndicator>, // For completed/merged tasks
}

impl TaskCard {
    /// Get recent activity for display
    pub fn get_recent_activity(&self, count: usize) -> Vec<String> {
        if self.state == TaskState::Active {
            let recent: Vec<String> = self.activity.iter()
                .rev()
                .take(count)
                .cloned()
                .collect();
            let mut result: Vec<String> = recent.into_iter().rev().collect();

            // Always return exactly count lines, padding with empty strings at the beginning
            while result.len() < count {
                result.insert(0, String::new());
            }
            result
        } else {
            vec![String::new(); count]
        }
    }
}

/// UI display indicators for delivery status
#[derive(Debug, Clone, PartialEq)]
pub enum DeliveryIndicator {
    BranchCreated,
    PrCreated { pr_number: u32, title: String },
    PrMerged { pr_number: u32 },
}

/// Filter options for task list display
#[derive(Debug, Clone, PartialEq)]
pub struct FilterOptions {
    pub status: TaskStatusFilter,
    pub time_range: TimeRangeFilter,
    pub search_query: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TaskStatusFilter {
    All,
    Active,
    Completed,
    Merged,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimeRangeFilter {
    AllTime,
    Today,
    Week,
    Month,
}

/// Auto-save state for UI display
#[derive(Debug, Clone, PartialEq)]
pub enum AutoSaveState {
    Saved,
    Saving,
    Unsaved,
    Error(String),
}

/// Card display information for rendering
#[derive(Debug, Clone, PartialEq)]
pub struct TaskCardViewModel {
    pub id: String,
    pub title: String,
    pub metadata_line: String,
    pub height: u16,
    pub is_selected: bool,
    pub card_type: TaskCardType,
}

/// Different visual types of task cards
#[derive(Debug, Clone, PartialEq)]
pub enum TaskCardType {
    /// Draft card with editable content
    Draft {
        description: String,
        cursor_position: (u16, u16), // row, col within textarea
        show_placeholder: bool,
        controls: DraftControlsViewModel,
        auto_save_indicator: String,
    },
    /// Active task with real-time activity
    Active {
        activity_lines: Vec<String>, // Exactly N lines (configurable)
        pause_delete_buttons: String,
    },
    /// Completed task with delivery indicators
    Completed {
        delivery_indicators: String, // Formatted indicator text with colors
    },
    /// Merged task with delivery indicators
    Merged {
        delivery_indicators: String, // Formatted indicator text with colors
    },
}

impl TaskCardType {
    /// Check if this card type represents a draft task
    pub fn is_draft(&self) -> bool {
        matches!(self, TaskCardType::Draft { .. })
    }

    /// Check if this card type represents an active task
    pub fn is_active(&self) -> bool {
        matches!(self, TaskCardType::Active { .. })
    }

    /// Check if this card type represents a completed task
    pub fn is_completed(&self) -> bool {
        matches!(self, TaskCardType::Completed { .. })
    }

    /// Check if this card type represents a merged task
    pub fn is_merged(&self) -> bool {
        matches!(self, TaskCardType::Merged { .. })
    }
}

/// Draft card controls view model
#[derive(Debug, Clone, PartialEq)]
pub struct DraftControlsViewModel {
    pub repository_button: ButtonViewModel,
    pub branch_button: ButtonViewModel,
    pub model_button: ButtonViewModel,
    pub go_button: ButtonViewModel,
}

/// Button presentation state
#[derive(Debug, Clone, PartialEq)]
pub struct ButtonViewModel {
    pub text: String,
    pub is_focused: bool,
    pub style: ButtonStyle,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ButtonStyle {
    Normal,
    Focused,
    Active,
    Disabled,
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
    ModelSelection {
        options: Vec<ModelOptionViewModel>
    },
    Settings {
        fields: Vec<SettingsFieldViewModel>
    },
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
    pub shortcuts: Vec<ShortcutViewModel>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShortcutViewModel {
    pub key: String,
    pub description: String,
}

/// Filter bar presentation
#[derive(Debug, Clone, PartialEq)]
pub struct FilterBarViewModel {
    pub status_filter: FilterButtonViewModel,
    pub time_filter: FilterButtonViewModel,
    pub search_box: SearchBoxViewModel,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FilterButtonViewModel {
    pub current_value: String,
    pub is_focused: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchBoxViewModel {
    pub value: String,
    pub placeholder: String,
    pub is_focused: bool,
    pub cursor_position: usize,
}

/// Status bar presentation
#[derive(Debug, Clone, PartialEq)]
pub struct StatusBarViewModel {
    pub backend_indicator: String, // "local" or hostname
    pub last_operation: String,
    pub connection_status: String,
    pub error_message: Option<String>,
}

/// Main ViewModel containing all presentation state
pub struct ViewModel {
    // Header
    pub title: String,
    pub show_settings_button: bool,

    // Task cards
    pub task_cards: Vec<TaskCardViewModel>,
    pub has_draft_cards: bool,
    pub focus_element: FocusElement, // Current UI focus state

    // Modals
    pub active_modal: Option<ModalViewModel>,

    // Footer
    pub footer: FooterViewModel,

    // Filter bar
    pub filter_bar: FilterBarViewModel,

    // Status bar
    pub status_bar: StatusBarViewModel,

    // Layout hints
    pub scroll_offset: u16,
    pub needs_scrollbar: bool,
    pub total_content_height: u16,
    pub visible_area_height: u16,

    // Text editing state
    pub draft_textarea: Option<tui_textarea::TextArea<'static>>, // TextArea for draft editing

    // UI state (moved from Model)
    pub modal_state: ModalState,
    pub search_mode: SearchMode,
    pub activity_lines_count: usize, // 1-3 configurable activity lines
    pub word_wrap_enabled: bool,
    pub show_autocomplete_border: bool,
    pub status_message: Option<String>,
    pub error_message: Option<String>,

    // Service dependencies
    pub workspace_files: Box<dyn WorkspaceFiles>,
    pub workspace_workflows: Box<dyn WorkspaceWorkflows>,
}

impl ViewModel {
    /// Create a new ViewModel with service dependencies
    pub fn new(
        model: &Model,
        workspace_files: Box<dyn WorkspaceFiles>,
        workspace_workflows: Box<dyn WorkspaceWorkflows>,
    ) -> Self {
        // Determine initial focus element per PRD: "The initially focused element is the top draft task card."
        let initial_focus = if !model.draft_tasks.is_empty() {
            FocusElement::DraftTask(0) // Focus on first draft task
        } else if !model.task_executions.is_empty() {
            FocusElement::ExistingTask(0) // Focus on first existing task
        } else {
            FocusElement::Settings // Fall back to settings if no tasks
        };

        let task_cards = create_task_card_view_models(model, initial_focus);
        let active_modal = create_modal_view_model(ModalState::None, &model.available_repositories, &model.available_branches, &model.available_models, &model.current_draft, model.activity_lines_count, model.word_wrap_enabled, model.show_autocomplete_border);
        let footer = create_footer_view_model(model, initial_focus, ModalState::None, model.activity_lines_count, model.word_wrap_enabled, model.show_autocomplete_border); // Use initial focus
        let filter_bar = create_filter_bar_view_model(model);
        let status_bar = create_status_bar_view_model(model, None, None);

        // Calculate layout metrics
        let total_content_height: u16 = task_cards.iter()
            .map(|card| card.height + 1) // +1 for spacer
            .sum::<u16>()
            + 1; // Filter bar height

        ViewModel {
            title: "Agent Harbor".to_string(),
            show_settings_button: true,
            has_draft_cards: task_cards.iter().any(|card| matches!(card.card_type, TaskCardType::Draft { .. })),
            focus_element: initial_focus,
            task_cards,
            active_modal,
            footer,
            filter_bar,
            status_bar,
            scroll_offset: 0, // Calculated by View layer based on selection
            needs_scrollbar: total_content_height > 20, // Rough estimate, View layer refines
            total_content_height,
            visible_area_height: 20, // Will be set by View layer
            draft_textarea: None, // Initialized when entering draft editing

            // Initialize UI state with defaults (moved from Model)
            modal_state: ModalState::None,
            search_mode: SearchMode::None,
            activity_lines_count: model.activity_lines_count,
            word_wrap_enabled: model.word_wrap_enabled,
            show_autocomplete_border: model.show_autocomplete_border,
            status_message: None,
            error_message: None,

            // Service dependencies
                    workspace_files,
                    workspace_workflows,
        }
    }
}

impl ViewModel {
    /// Update the selection state in task cards based on current focus_element
    pub fn update_task_card_selections(&mut self) {
        for (idx, card) in self.task_cards.iter_mut().enumerate() {
            card.is_selected = match self.focus_element {
                FocusElement::DraftTask(task_idx) | FocusElement::ExistingTask(task_idx) => Some(idx) == Some(task_idx),
                _ => false,
            };
        }
    }

    /// Update the footer based on current focus state
    pub fn update_footer(&mut self, model: &Model) {
        self.footer = create_footer_view_model(model, self.focus_element, self.modal_state, self.activity_lines_count, self.word_wrap_enabled, self.show_autocomplete_border);
    }

    /// Open a modal dialog
    pub fn open_modal(&mut self, modal_state: ModalState) {
        self.modal_state = modal_state;
    }

    /// Close the current modal
    pub fn close_modal(&mut self) {
        self.modal_state = ModalState::None;
    }

    /// Select a repository from modal
    pub fn select_repository(&mut self, repo: String, model: &mut Model) {
        if let Some(ref mut draft) = model.current_draft {
            draft.repository = repo;
        }
        self.close_modal();
    }

    /// Select a branch from modal
    pub fn select_branch(&mut self, branch: String, model: &mut Model) {
        if let Some(ref mut draft) = model.current_draft {
            draft.branch = branch;
        }
        self.close_modal();
    }

    /// Select model names from modal
    pub fn select_model_names(&mut self, model_names: Vec<String>, model: &mut Model) {
        if let Some(ref mut draft) = model.current_draft {
            draft.models = model_names.into_iter()
                .map(|name| SelectedModel { name, count: 1 })
                .collect();
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
    fn handle_launch_task(&mut self, model: &Model) -> Vec<DomainMsg> {
        if let Some(draft) = &model.current_draft {
            if !draft.description.trim().is_empty() && !draft.models.is_empty() {
                self.set_status_message("Creating task...".to_string());
                vec![DomainMsg::LaunchTask]
            } else {
                self.set_error_message("Please provide a task description and select at least one model".to_string());
                vec![]
            }
        } else {
            vec![]
        }
    }

    /// Handle key events and return domain messages for Model updates
    /// Returns a vector of domain messages to send to the Model
    pub fn handle_key_event(&mut self, key: KeyEvent, model: &Model) -> Vec<DomainMsg> {
        use KeyCode::*;

        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);

        // Handle modal state first
        if self.modal_state != ModalState::None {
            return self.handle_modal_key_event(key);
        }

        match (key.code, ctrl, shift) {
            // Navigation
            (Up, false, false) => {
                self.handle_navigation(NavigationDirection::Up, model);
                vec![]
            }
            (Down, false, false) => {
                self.handle_navigation(NavigationDirection::Down, model);
                vec![]
            }

            // Enter key - context sensitive
            (Enter, false, false) => {
                match self.focus_element {
                    FocusElement::Settings => {
                        // Open settings modal (not implemented yet)
                        vec![]
                    }
                    FocusElement::DraftTask(idx) => {
                        // Enter draft editing mode for this draft
                        if model.draft_tasks.get(idx).is_some() {
                            self.enter_draft_editing_mode(idx, model);
                        }
                        vec![]
                    }
                    FocusElement::FilterBarSeparator => {
                        // Open filter interface (not implemented yet)
                        vec![]
                    }
                    FocusElement::ExistingTask(idx) => {
                        // Show task details (not implemented yet)
                        vec![]
                    }
                    FocusElement::GoButton => self.handle_launch_task(model),
                    FocusElement::TaskDescription => self.handle_launch_task(model),
                    FocusElement::RepositorySelector => {
                        self.open_modal(ModalState::RepositorySearch);
                        vec![]
                    }
                    FocusElement::BranchSelector => {
                        self.open_modal(ModalState::BranchSearch);
                        vec![]
                    }
                    FocusElement::ModelSelector => {
                        self.open_modal(ModalState::ModelSelection);
                        vec![]
                    }
                    FocusElement::Filter(_) => {
                        // Open filter interface
                        vec![]
                    }
                }
            }

            // Tab navigation in draft editing
            (Tab, false, false) => {
                self.handle_tab_navigation(false);
                vec![]
            }
            (BackTab, false, true) => {
                self.handle_tab_navigation(true);
                vec![]
            }

            // Escape - return to navigation mode
            (Esc, false, false) => {
                match self.focus_element {
                    FocusElement::TaskDescription | FocusElement::RepositorySelector
                    | FocusElement::BranchSelector | FocusElement::ModelSelector
                    | FocusElement::GoButton => {
                        // Return to draft task navigation
                        self.exit_draft_editing_mode();
                        vec![]
                    }
                    _ => {
                        self.close_modal();
                        vec![]
                    }
                }
            }

            // Create new draft
            (Char('n'), true, false) => {
                vec![DomainMsg::CreateDraft]
            }

            // Delete current task
            (Char('w'), true, false) => {
                match self.focus_element {
                    FocusElement::DraftTask(idx) => vec![DomainMsg::DeleteTask(idx)],
                    FocusElement::ExistingTask(idx) => {
                        // Convert to combined index (drafts + existing)
                        let combined_idx = model.draft_tasks.len() + idx;
                        vec![DomainMsg::DeleteTask(combined_idx)]
                    }
                    _ => vec![],
                }
            }

            // Text input in description area
            (Char(c), false, false) if matches!(self.focus_element, FocusElement::TaskDescription) => {
                self.handle_text_input(c, model)
            }
            (Backspace, false, false) if matches!(self.focus_element, FocusElement::TaskDescription) => {
                self.handle_text_backspace(model)
            }
            (Enter, false, true) if matches!(self.focus_element, FocusElement::TaskDescription) => {
                // Shift+Enter for new line
                self.handle_text_input('\n', model)
            }

            _ => vec![],
        }
    }

    /// Enter draft editing mode for a specific draft
    fn enter_draft_editing_mode(&mut self, draft_idx: usize, model: &Model) {
        if let Some(draft) = model.draft_tasks.get(draft_idx) {
            // Create TextArea with the draft's current content
            let mut textarea = tui_textarea::TextArea::default();
            for line in draft.description.lines() {
                textarea.insert_str(line);
                textarea.move_cursor(tui_textarea::CursorMove::End);
                if draft.description.contains('\n') {
                    textarea.insert_newline();
                }
            }
            // Remove the trailing newline if it exists
            if draft.description.ends_with('\n') {
                let lines = textarea.lines();
                if lines.len() > 1 {
                    textarea.move_cursor(tui_textarea::CursorMove::Up);
                    textarea.move_cursor(tui_textarea::CursorMove::End);
                    textarea.delete_str(1);
                }
            }

            self.draft_textarea = Some(textarea);
            self.focus_element = FocusElement::TaskDescription;
        }
    }

    /// Exit draft editing mode
    fn exit_draft_editing_mode(&mut self) {
        self.draft_textarea = None;
        // Return to the appropriate draft task focus
        // This would need to be updated based on which draft we were editing
        self.focus_element = FocusElement::DraftTask(0); // Simplified
    }

    /// Handle text input in the textarea
    fn handle_text_input(&mut self, c: char, model: &Model) -> Vec<DomainMsg> {
        if let Some(textarea) = &mut self.draft_textarea {
            textarea.insert_char(c);
            let content = textarea.lines().join("\n");
            vec![DomainMsg::UpdateDraftText(content)]
    } else {
            vec![]
        }
    }

    /// Handle backspace in the textarea
    fn handle_text_backspace(&mut self, model: &Model) -> Vec<DomainMsg> {
        if let Some(textarea) = &mut self.draft_textarea {
            textarea.delete_char();
            let content = textarea.lines().join("\n");
            vec![DomainMsg::UpdateDraftText(content)]
            } else {
            vec![]
        }
    }


    fn handle_modal_key_event(&mut self, key: KeyEvent) -> Vec<DomainMsg> {
        match key.code {
            KeyCode::Esc => {
                self.close_modal();
                vec![]
            }
            KeyCode::Enter => {
                // Modal selection would be handled here
                // For now, just close modal
                self.close_modal();
                vec![]
            }
            _ => vec![],
        }
    }

    fn handle_tab_navigation(&mut self, backward: bool) {
        // This would handle tab navigation within the ViewModel's focus state
        // For now, simplified - just cycle through draft editing fields
        // Implementation would depend on current focus state
    }

    /// Handle navigation through the UI hierarchy
    /// Returns true if the navigation was handled (focus changed)
    pub fn handle_navigation(&mut self, direction: NavigationDirection, model: &Model) -> bool {
        // Handle textarea navigation first if we're in a textarea
        if matches!(self.focus_element, FocusElement::TaskDescription) {
            if self.handle_textarea_navigation(direction, model) {
                return true;
            }
        }

        // Handle hierarchical navigation
        match direction {
            NavigationDirection::Up => self.navigate_up_hierarchy(model),
            NavigationDirection::Down => self.navigate_down_hierarchy(model),
        }
    }

    /// Handle navigation within a textarea before moving focus
    fn handle_textarea_navigation(&mut self, direction: NavigationDirection, _model: &Model) -> bool {
        if let Some(textarea) = &mut self.draft_textarea {
            match direction {
                NavigationDirection::Up => {
                    // Try to move up in textarea, if at top, let hierarchy navigation handle it
                    let (row, _) = textarea.cursor();
                    if row == 0 {
                        // At top of textarea, allow focus to move up
                        return false;
                    }
                    // Move up within textarea
                    textarea.move_cursor(tui_textarea::CursorMove::Up);
                    true
                }
                NavigationDirection::Down => {
                    // Try to move down in textarea, if at bottom, let hierarchy navigation handle it
                    let (row, _) = textarea.cursor();
                    let lines = textarea.lines().len();
                    if row >= lines.saturating_sub(1) {
                        // At bottom of textarea, allow focus to move down
                        return false;
                    }
                    // Move down within textarea
                    textarea.move_cursor(tui_textarea::CursorMove::Down);
                    true
                }
            }
    } else {
            false
        }
    }

    /// Navigate up through the UI hierarchy
    fn navigate_up_hierarchy(&mut self, model: &Model) -> bool {
        let new_focus = match self.focus_element {
            FocusElement::Settings => {
                // At top, wrap to bottom (last existing task or filter separator or last draft)
                if !model.task_executions.is_empty() {
                    FocusElement::ExistingTask(model.task_executions.len() - 1)
                } else if model.draft_tasks.is_empty() {
                    FocusElement::FilterBarSeparator
            } else {
                    FocusElement::DraftTask(model.draft_tasks.len() - 1)
                }
            }
            FocusElement::DraftTask(idx) => {
                if idx == 0 {
                    // First draft, go to settings
                    FocusElement::Settings
                } else {
                    // Previous draft
                    FocusElement::DraftTask(idx - 1)
                }
            }
            FocusElement::FilterBarSeparator => {
                // From filter separator, go to last draft or settings
                if !model.draft_tasks.is_empty() {
                    FocusElement::DraftTask(model.draft_tasks.len() - 1)
                } else {
                    FocusElement::Settings
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
            self.focus_element = new_focus;
            self.update_task_card_selections();
            self.update_footer(model);
            true
        } else {
            false
        }
    }

    /// Navigate down through the UI hierarchy
    fn navigate_down_hierarchy(&mut self, model: &Model) -> bool {
        let new_focus = match self.focus_element {
            FocusElement::Settings => {
                // From settings, go to first draft or filter separator or first existing
                if !model.draft_tasks.is_empty() {
                    FocusElement::DraftTask(0)
                } else if !model.task_executions.is_empty() {
                    FocusElement::FilterBarSeparator
                } else {
                    FocusElement::ExistingTask(0)
                }
            }
            FocusElement::DraftTask(idx) => {
                if idx >= model.draft_tasks.len() - 1 {
                    // Last draft, go to filter separator if we have existing tasks
                    if !model.task_executions.is_empty() {
                        FocusElement::FilterBarSeparator
                    } else {
                        // No existing tasks, wrap to settings
                        FocusElement::Settings
                    }
                } else {
                    // Next draft
                    FocusElement::DraftTask(idx + 1)
                }
            }
            FocusElement::FilterBarSeparator => {
                // From filter separator, go to first existing task or wrap to settings
                if !model.task_executions.is_empty() {
                    FocusElement::ExistingTask(0)
                } else {
                    FocusElement::Settings
                }
            }
            FocusElement::ExistingTask(idx) => {
                if idx >= model.task_executions.len() - 1 {
                    // Last existing task, wrap to settings
                    FocusElement::Settings
                } else {
                    // Next existing task
                    FocusElement::ExistingTask(idx + 1)
                }
            }
            // Other focus elements stay the same
            other => other,
        };

        if new_focus != self.focus_element {
            self.focus_element = new_focus;
            self.update_task_card_selections();
            self.update_footer(model);
            true
        } else {
            false
        }
    }
}

/// Navigation directions for the hierarchical UI
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NavigationDirection {
    Up,
    Down,
}

fn create_task_card_view_models(model: &Model, focus_element: FocusElement) -> Vec<TaskCardViewModel> {
    let visible_tasks = model.all_tasks();

    visible_tasks.into_iter().enumerate().map(|(idx, task_item)| {
        match task_item {
            TaskItem::Draft(draft) => {
                TaskCardViewModel {
                    id: draft.id.clone(),
                    title: if draft.description.is_empty() {
                        "New Task".to_string()
                    } else {
                        // Use first line of description as title
                        draft.description.lines().next().unwrap_or("New Task").to_string()
                    },
                    metadata_line: format!("Draft • {} • {}", draft.repository, draft.branch),
                    height: calculate_draft_card_height(&draft, model),
                    is_selected: false, // Will be updated by update_task_card_selections
                    card_type: TaskCardType::Draft {
                        description: draft.description.clone(),
                        cursor_position: (0, 0), // TODO: Calculate actual cursor position
                        show_placeholder: draft.description.is_empty(),
                        controls: create_draft_controls_view_model(model, focus_element),
                        auto_save_indicator: format_auto_save_indicator(AutoSaveState::Saved), // Default for drafts
                    },
                }
            }
            TaskItem::Task(task_execution, _) => {
                // Convert TaskExecution to UI TaskCard
                let ui_task = TaskCard {
                    id: task_execution.id.clone(),
                    title: format_title_from_execution(&task_execution),
                    repository: task_execution.repository.clone(),
                    branch: task_execution.branch.clone(),
                    agents: task_execution.agents.clone(),
                    state: task_execution.state,
                    timestamp: task_execution.timestamp.clone(),
                    activity: task_execution.activity.clone(),
                    delivery_indicators: task_execution.delivery_status.iter().map(|status| {
                        match status {
                            DeliveryStatus::BranchCreated => DeliveryIndicator::BranchCreated,
                            DeliveryStatus::PullRequestCreated { pr_number, title } =>
                                DeliveryIndicator::PrCreated { pr_number: *pr_number, title: title.clone() },
                            DeliveryStatus::PullRequestMerged { pr_number } =>
                                DeliveryIndicator::PrMerged { pr_number: *pr_number },
                        }
                    }).collect(),
                };

                TaskCardViewModel {
                    id: ui_task.id.clone(),
                    title: ui_task.title.clone(),
                    metadata_line: format_metadata_line(&ui_task),
                    height: calculate_card_height(&ui_task, model),
                    is_selected: false, // Will be updated by update_task_card_selections
                    card_type: create_card_type_view_model(&ui_task, model, false),
                }
            }
        }
    }).collect()
}

fn format_title_from_execution(task: &TaskExecution) -> String {
    // For executed tasks, we might want to generate a title from the repository/branch
    // or use some other logic. For now, use a generic title.
    format!("Task {}", &task.id)
}

fn calculate_draft_card_height(_draft: &DraftTask, _model: &Model) -> u16 {
    // Draft cards have description + controls
    // For now, use a fixed height - can be made dynamic later
    6 // Description (3 lines) + controls (3 lines)
}

fn create_draft_controls_view_model(model: &Model, focus_element: FocusElement) -> DraftControlsViewModel {
    DraftControlsViewModel {
                    repository_button: ButtonViewModel {
            text: model.current_draft.as_ref().map(|d| d.repository.clone()).unwrap_or_default(),
                        is_focused: matches!(focus_element, FocusElement::RepositorySelector),
                        style: if matches!(focus_element, FocusElement::RepositorySelector) {
                            ButtonStyle::Focused
                        } else {
                            ButtonStyle::Normal
                        },
                    },
                    branch_button: ButtonViewModel {
            text: model.current_draft.as_ref().map(|d| d.branch.clone()).unwrap_or_default(),
                        is_focused: matches!(focus_element, FocusElement::BranchSelector),
                        style: if matches!(focus_element, FocusElement::BranchSelector) {
                            ButtonStyle::Focused
                        } else {
                            ButtonStyle::Normal
                        },
                    },
                    model_button: ButtonViewModel {
            text: format!("{} model{}", model.current_draft.as_ref()
                .map(|d| d.models.len()).unwrap_or(0), if model.current_draft.as_ref()
                .map(|d| d.models.len()).unwrap_or(0) != 1 { "s" } else { "" }),
                        is_focused: matches!(focus_element, FocusElement::ModelSelector),
                        style: if matches!(focus_element, FocusElement::ModelSelector) {
                            ButtonStyle::Focused
                        } else {
                            ButtonStyle::Normal
                        },
                    },
                    go_button: ButtonViewModel {
            text: "Launch".to_string(),
                        is_focused: matches!(focus_element, FocusElement::GoButton),
                        style: if matches!(focus_element, FocusElement::GoButton) {
                ButtonStyle::Active
                        } else {
                            ButtonStyle::Normal
                        },
                    },
    }
}

fn format_auto_save_indicator(state: AutoSaveState) -> String {
    match state {
        AutoSaveState::Saved => "✓".to_string(),
        AutoSaveState::Saving => "⟳".to_string(),
        AutoSaveState::Unsaved => "●".to_string(),
        AutoSaveState::Error(_) => "✗".to_string(),
    }
}

fn format_metadata_line(task: &TaskCard) -> String {
    match task.state {
        TaskState::Active => {
            format!("{} • {} • {} • {}",
                task.repository,
                task.branch,
                format_models(&task.agents),
                task.timestamp)
        },
        TaskState::Completed => {
            format!("{} • {} • {} • Completed {}",
                task.repository,
                task.branch,
                format_models(&task.agents),
                task.timestamp)
        },
        TaskState::Merged => {
            format!("{} • {} • {} • Merged {}",
                task.repository,
                task.branch,
                format_models(&task.agents),
                task.timestamp)
        },
        TaskState::Draft => {
            // This shouldn't happen since drafts are handled separately now
            format!("{} • {} • Draft", task.repository, task.branch)
        }
    }
}

fn format_models(models: &[SelectedModel]) -> String {
    if models.is_empty() {
        "No models".to_string()
    } else if models.len() == 1 {
        format!("{} (x{})", models[0].name, models[0].count)
    } else {
        let model_strings: Vec<String> = models.iter()
            .map(|model| format!("{} (x{})", model.name, model.count))
            .collect();
        model_strings.join(", ")
    }
}

fn calculate_card_height(task: &TaskCard, model: &Model) -> u16 {
    match task.state {
        TaskState::Completed | TaskState::Merged => 3, // Title + metadata + padding
        TaskState::Active => {
            2 + model.activity_lines_count as u16 + 3 // Title + metadata + N activity lines + borders
        },
        TaskState::Draft => {
            // This shouldn't happen since drafts are handled separately now
            6 // Fallback height
        }
    }
}

fn create_card_type_view_model(task: &TaskCard, model: &Model, is_selected: bool) -> TaskCardType {
    match task.state {
        TaskState::Draft => {
            // This shouldn't happen since drafts are handled separately now
            TaskCardType::Completed {
                delivery_indicators: "Legacy Draft".to_string(),
            }
        },
        TaskState::Active => {
            TaskCardType::Active {
                activity_lines: task.get_recent_activity(model.activity_lines_count),
                pause_delete_buttons: "⏸ Pause • ✕ Delete".to_string(),
            }
        },
        TaskState::Completed => {
            TaskCardType::Completed {
                delivery_indicators: format_delivery_indicators(&task.delivery_indicators),
            }
        },
        TaskState::Merged => {
            TaskCardType::Merged {
                delivery_indicators: format_delivery_indicators(&task.delivery_indicators),
            }
        },
    }
}

fn format_delivery_indicators(indicators: &[DeliveryIndicator]) -> String {
    indicators.iter().map(|indicator| {
        match indicator {
            DeliveryIndicator::BranchCreated => "⎇ branch".to_string(),
            DeliveryIndicator::PrCreated { pr_number, title } => {
                format!("⇄ PR #{} — \"{}\"", pr_number, title)
            },
            DeliveryIndicator::PrMerged { pr_number } => {
                format!("✓ PR #{} merged to main", pr_number)
            }
        }
    }).collect::<Vec<_>>().join(" • ")
}

fn create_modal_view_model(modal_state: ModalState, available_repositories: &[String], available_branches: &[String], available_models: &[String], current_draft: &Option<DraftTask>, activity_lines_count: usize, word_wrap_enabled: bool, show_autocomplete_border: bool) -> Option<ModalViewModel> {
    match modal_state {
        ModalState::None => None,
        ModalState::RepositorySearch => Some(ModalViewModel {
            title: "Select Repository".to_string(),
            input_value: String::new(),
            filtered_options: available_repositories.iter()
                .map(|repo| (repo.clone(), current_draft.as_ref().map(|d| &d.repository == repo).unwrap_or(false)))
                .collect(),
            selected_index: 0,
            modal_type: ModalType::Search {
                placeholder: "Type to search repositories...".to_string()
            },
        }),
        ModalState::BranchSearch => Some(ModalViewModel {
            title: "Select Branch".to_string(),
            input_value: String::new(),
            filtered_options: available_branches.iter()
                .map(|branch| (branch.clone(), current_draft.as_ref().map(|d| &d.branch == branch).unwrap_or(false)))
                .collect(),
            selected_index: 0,
            modal_type: ModalType::Search {
                placeholder: "Type to search branches...".to_string()
            },
        }),
        ModalState::ModelSelection => Some(ModalViewModel {
            title: "Select Models".to_string(),
            input_value: String::new(),
            filtered_options: available_models.iter()
                .map(|model_name| (model_name.clone(), false))
                .collect(),
            selected_index: 0,
            modal_type: ModalType::ModelSelection {
                options: available_models.iter().map(|model_name| {
                    let selected_model = current_draft.as_ref()
                        .map(|d| d.models.iter().find(|sm| sm.name == *model_name))
                        .flatten();
                    ModelOptionViewModel {
                        name: model_name.clone(),
                        count: selected_model.map_or(0, |sm| sm.count),
                        is_selected: selected_model.is_some(),
                    }
                }).collect()
            },
        }),
        ModalState::Settings => Some(ModalViewModel {
            title: "Settings".to_string(),
            input_value: String::new(),
            filtered_options: Vec::new(),
            selected_index: 0,
            modal_type: ModalType::Settings {
                fields: vec![
                    SettingsFieldViewModel {
                        label: "Activity Lines".to_string(),
                        value: activity_lines_count.to_string(),
                        is_focused: false,
                        field_type: SettingsFieldType::Number,
                    },
                    SettingsFieldViewModel {
                        label: "Word Wrap".to_string(),
                        value: word_wrap_enabled.to_string(),
                        is_focused: false,
                        field_type: SettingsFieldType::Boolean,
                    },
                    SettingsFieldViewModel {
                        label: "Autocomplete Border".to_string(),
                        value: show_autocomplete_border.to_string(),
                        is_focused: false,
                        field_type: SettingsFieldType::Boolean,
                    },
                ]
            },
        }),
        _ => None, // Other modals not implemented yet
    }
}

fn create_footer_view_model(model: &Model, focus_element: FocusElement, modal_state: ModalState, activity_lines_count: usize, word_wrap_enabled: bool, show_autocomplete_border: bool) -> FooterViewModel {
    let shortcuts = match (&modal_state, &focus_element) {
        // Modal active - takes precedence over focus element
        (modal_state, _) if *modal_state != ModalState::None => vec![
            ShortcutViewModel { key: "↑↓".to_string(), description: "Navigate".to_string() },
            ShortcutViewModel { key: "Enter".to_string(), description: "Select".to_string() },
            ShortcutViewModel { key: "Esc".to_string(), description: "Back".to_string() },
        ],

        // Settings button focused
        (_, FocusElement::Settings) => vec![
            ShortcutViewModel { key: "↓".to_string(), description: "Next".to_string() },
            ShortcutViewModel { key: "Enter".to_string(), description: "Settings".to_string() },
            ShortcutViewModel { key: "Ctrl+C x2".to_string(), description: "Quit".to_string() },
        ],

        // Draft task focused
        (_, FocusElement::DraftTask(_)) => vec![
            ShortcutViewModel { key: "↑↓".to_string(), description: "Navigate".to_string() },
            ShortcutViewModel { key: "Enter".to_string(), description: "Edit Draft".to_string() },
            ShortcutViewModel { key: "Ctrl+C x2".to_string(), description: "Quit".to_string() },
        ],

        // Filter bar separator focused
        (_, FocusElement::FilterBarSeparator) => vec![
            ShortcutViewModel { key: "↑↓".to_string(), description: "Navigate".to_string() },
            ShortcutViewModel { key: "Enter".to_string(), description: "Filter".to_string() },
            ShortcutViewModel { key: "Ctrl+C x2".to_string(), description: "Quit".to_string() },
        ],

        // Existing task focused
        (_, FocusElement::ExistingTask(_)) => {
            // Get the task state to determine appropriate shortcuts
            let task_state = match focus_element {
                FocusElement::ExistingTask(idx) => {
                    model.task_executions.get(idx).map(|t| t.state)
                }
                _ => None,
            };
            match task_state {
                Some(TaskState::Active) => vec![
                    ShortcutViewModel { key: "↑↓".to_string(), description: "Navigate".to_string() },
                    ShortcutViewModel { key: "Enter".to_string(), description: "Show Task Progress".to_string() },
                    ShortcutViewModel { key: "Ctrl+C x2".to_string(), description: "Quit".to_string() },
                ],
                Some(TaskState::Completed) | Some(TaskState::Merged) => vec![
                    ShortcutViewModel { key: "↑↓".to_string(), description: "Navigate".to_string() },
                    ShortcutViewModel { key: "Enter".to_string(), description: "Show Task Details".to_string() },
                    ShortcutViewModel { key: "Ctrl+C x2".to_string(), description: "Quit".to_string() },
                ],
                _ => vec![
            ShortcutViewModel { key: "↑↓".to_string(), description: "Navigate".to_string() },
            ShortcutViewModel { key: "Enter".to_string(), description: "Select Task".to_string() },
            ShortcutViewModel { key: "Ctrl+C x2".to_string(), description: "Quit".to_string() },
        ],
            }
        },

        // Draft textarea focused
        (_, FocusElement::TaskDescription) => {
            let agent_text = if model.current_draft.as_ref()
                .map(|d| d.models.len() <= 1).unwrap_or(true) {
                "Launch Agent".to_string()
            } else {
                "Launch Agents".to_string()
            };
            vec![
                ShortcutViewModel { key: "Enter".to_string(), description: agent_text },
                ShortcutViewModel { key: "Shift+Enter".to_string(), description: "New Line".to_string() },
                ShortcutViewModel { key: "Tab".to_string(), description: "Next Field".to_string() },
            ]
        },

        // Other focus states (repository, branch, model selectors, etc.)
        _ => vec![
            ShortcutViewModel { key: "↑↓".to_string(), description: "Navigate".to_string() },
            ShortcutViewModel { key: "Enter".to_string(), description: "Select".to_string() },
            ShortcutViewModel { key: "Esc".to_string(), description: "Back".to_string() },
        ],
    };

    FooterViewModel { shortcuts }
}

fn create_filter_bar_view_model(_model: &Model) -> FilterBarViewModel {
    // Filters are now handled at the ViewModel level
    // Return default values for now
    FilterBarViewModel {
        status_filter: FilterButtonViewModel {
            current_value: "All".to_string(),
            is_focused: false,
        },
        time_filter: FilterButtonViewModel {
            current_value: "All Time".to_string(),
            is_focused: false,
        },
        search_box: SearchBoxViewModel {
            value: String::new(),
            placeholder: "Search tasks...".to_string(),
            is_focused: false,
            cursor_position: 0,
        },
    }
}

fn create_status_bar_view_model(model: &Model, status_message: Option<&String>, error_message: Option<&String>) -> StatusBarViewModel {
    let backend_indicator = "local".to_string(); // In real app, would be dynamic

    let last_operation: String = if model.loading_states.task_creation {
        "Creating task...".to_string()
    } else if model.loading_states.repositories {
        "Loading repositories...".to_string()
    } else if model.loading_states.branches {
        "Loading branches...".to_string()
    } else if model.loading_states.models {
        "Loading models...".to_string()
    } else if let Some(msg) = status_message {
        (*msg).clone()
    } else {
        "Ready".to_string()
    };

    let connection_status = if model.loading_states.repositories ||
                              model.loading_states.branches ||
                              model.loading_states.models ||
                              model.loading_states.task_creation {
        "Connecting...".to_string()
    } else {
        "Connected".to_string()
    };

    StatusBarViewModel {
        backend_indicator,
        last_operation,
        connection_status,
        error_message: error_message.map(|s| s.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_model_formats_title_and_shows_settings() {
        let model = Model::default();
        let vm: ViewModel = (&model).into();

        assert_eq!(vm.title, "Agent Harbor");
        assert!(vm.show_settings_button);
    }

    // #[test]
    // fn view_model_creates_draft_card_with_placeholder() {
    //     // This test needs to be updated for the new MVVM architecture
    //     // TODO: Update to use TaskExecution and proper domain messages
    // }

    // #[test]
    // fn view_model_formats_active_task_with_activity() {
    //     // This test needs to be updated for the new MVVM architecture
    //     // TODO: Update to use TaskExecution and proper domain messages
    // }

    #[test]
    fn view_model_shows_correct_footer_for_focus_state() {
        // Test draft textarea focused
        let mut model = Model::default();
        model.focus_element = FocusElement::TaskDescription;
        if let Some(ref mut draft) = model.current_draft {
            draft.models = vec![SelectedModel { name: "Claude".to_string(), count: 1 }];
        }

        let vm: ViewModel = (&model).into();

        assert!(vm.footer.shortcuts.iter().any(|s| s.description == "Launch Agent"));
        assert!(vm.footer.shortcuts.iter().any(|s| s.description == "New Line"));
        assert!(vm.footer.shortcuts.iter().any(|s| s.description == "Next Field"));

        // Test plural case for agents
        if let Some(ref mut draft) = model.current_draft {
            draft.models.push(SelectedModel { name: "GPT-4".to_string(), count: 1 });
        }
        let vm: ViewModel = (&model).into();

        assert!(vm.footer.shortcuts.iter().any(|s| s.description == "Launch Agents"));
    }

    // #[test]
    // fn view_model_shows_correct_footer_for_task_navigation() {
    //     // This test needs to be updated for the new MVVM architecture
    //     // TODO: Update to use TaskExecution and proper domain messages
    // }

    #[test]
    fn view_model_shows_correct_footer_for_modals() {
        let model = Model::default();
        let mut vm: ViewModel = (&model).into();
        vm.modal_state = ModalState::RepositorySearch;

        vm.update_footer(&model);

        assert_footer_shortcuts(&vm, &[
            ("↑↓", "Navigate"),
            ("Enter", "Select"),
            ("Esc", "Back"),
        ]);
    }

    #[test]
    fn view_model_handles_modal_states() {
        let mut model = Model::default();
        model.modal_state = ModalState::RepositorySearch;

        let vm: ViewModel = (&model).into();

        assert!(vm.active_modal.is_some());
        let modal = vm.active_modal.unwrap();
        assert_eq!(modal.title, "Select Repository");
        assert!(!modal.filtered_options.is_empty());
    }

    /// Test hierarchical navigation order: Settings → Draft cards → Filter bar → Existing tasks
    #[test]
    fn navigation_follows_hierarchical_order() {
        let model = create_model_with_drafts_and_tasks(2, 1, 0, 0);
        let mut vm: ViewModel = (&model).into();

        // Initially focused on first draft task (per PRD)
        assert_eq!(vm.focus_element, FocusElement::DraftTask(0));

        // Down arrow should move to second draft task
        vm.handle_navigation(NavigationDirection::Down, &model);
        assert_eq!(vm.focus_element, FocusElement::DraftTask(1));

        // Down arrow should move to filter bar separator
        vm.handle_navigation(NavigationDirection::Down, &model);
        assert_eq!(vm.focus_element, FocusElement::FilterBarSeparator);

        // Down arrow should move to first existing task
        vm.handle_navigation(NavigationDirection::Down, &model);
        assert_eq!(vm.focus_element, FocusElement::ExistingTask(0));

        // Down arrow should wrap to settings button
        vm.handle_navigation(NavigationDirection::Down, &model);
        assert_eq!(vm.focus_element, FocusElement::Settings);

        // Down arrow should move to first draft task
        vm.handle_navigation(NavigationDirection::Down, &model);
        assert_eq!(vm.focus_element, FocusElement::DraftTask(0));

        // Up arrow should wrap back to settings
        vm.handle_navigation(NavigationDirection::Up, &model);
        assert_eq!(vm.focus_element, FocusElement::Settings);
    }

    /// Test that draft tasks are displayed at the top, sorted newest first
    #[test]
    fn draft_tasks_display_at_top_newest_first() {
        let mut model = Model::default();

        // Add draft tasks with different timestamps (older first, but should display newest first)
        model.draft_tasks.push(DraftTask {
            id: "draft_older".to_string(),
            description: "Older draft".to_string(),
            repository: "repo".to_string(),
            branch: "main".to_string(),
            models: vec![SelectedModel { name: "Claude".to_string(), count: 1 }],
            created_at: "2023-01-01 09:00:00".to_string(),
        });
        model.draft_tasks.push(DraftTask {
            id: "draft_newer".to_string(),
            description: "Newer draft".to_string(),
            repository: "repo".to_string(),
            branch: "main".to_string(),
            models: vec![SelectedModel { name: "GPT-4".to_string(), count: 1 }],
            created_at: "2023-01-01 10:00:00".to_string(),
        });

        let vm: ViewModel = (&model).into();

        // Should have 2 task cards
        assert_eq!(vm.task_cards.len(), 2);

        // First card should be the newer draft
        assert_eq!(vm.task_cards[0].title, "Newer draft");
        assert!(vm.task_cards[0].card_type.is_draft());

        // Second card should be the older draft
        assert_eq!(vm.task_cards[1].title, "Older draft");
        assert!(vm.task_cards[1].card_type.is_draft());
    }

    /// Test that existing tasks follow draft tasks in chronological order (newest first)
    #[test]
    fn existing_tasks_follow_drafts_chronological_order() {
        let mut model = Model::default();

        // Add one draft task
        model.draft_tasks.push(DraftTask {
            id: "draft_1".to_string(),
            description: "Draft task".to_string(),
            repository: "repo".to_string(),
            branch: "main".to_string(),
            models: vec![SelectedModel { name: "Claude".to_string(), count: 1 }],
            created_at: "2023-01-01 12:00:00".to_string(),
        });

        // Add existing tasks with different timestamps
        model.task_executions.push(TaskExecution {
            id: "task_older".to_string(),
            repository: "repo1".to_string(),
            branch: "main".to_string(),
            agents: vec![SelectedModel { name: "Claude".to_string(), count: 1 }],
            state: TaskState::Completed,
            timestamp: "2023-01-01 10:00:00".to_string(),
            activity: vec![],
            delivery_status: vec![],
        });
        model.task_executions.push(TaskExecution {
            id: "task_newer".to_string(),
            repository: "repo2".to_string(),
            branch: "main".to_string(),
            agents: vec![SelectedModel { name: "GPT-4".to_string(), count: 1 }],
            state: TaskState::Active,
            timestamp: "2023-01-01 11:00:00".to_string(),
            activity: vec!["Processing...".to_string()],
            delivery_status: vec![],
        });

        let vm: ViewModel = (&model).into();

        // Should have 3 task cards total
        assert_eq!(vm.task_cards.len(), 3);

        // First card should be the draft
        assert_eq!(vm.task_cards[0].title, "Draft task");
        assert!(vm.task_cards[0].card_type.is_draft());

        // Second card should be the newer existing task
        assert_eq!(vm.task_cards[1].title, "Task task_newer");
        assert!(vm.task_cards[1].card_type.is_active());

        // Third card should be the older existing task
        assert_eq!(vm.task_cards[2].title, "Task task_older");
        assert!(vm.task_cards[2].card_type.is_completed());
    }

    /// Test that draft cards have variable height based on content
    #[test]
    fn draft_cards_variable_height_based_on_content() {
        let mut model = Model::default();

        // Add draft with short description
        model.draft_tasks.push(DraftTask {
            id: "draft_short".to_string(),
            description: "Short".to_string(),
            repository: "repo".to_string(),
            branch: "main".to_string(),
            models: vec![SelectedModel { name: "Claude".to_string(), count: 1 }],
            created_at: "2023-01-01 12:00:00".to_string(),
        });

        // Add draft with long description
        model.draft_tasks.push(DraftTask {
            id: "draft_long".to_string(),
            description: "This is a much longer description\nwith multiple lines\nand more content\nthat should result in\na taller card".to_string(),
            repository: "repo".to_string(),
            branch: "main".to_string(),
            models: vec![SelectedModel { name: "GPT-4".to_string(), count: 1 }],
            created_at: "2023-01-01 12:01:00".to_string(),
        });

        let vm: ViewModel = (&model).into();

        // Both should be draft cards
        assert!(vm.task_cards[0].card_type.is_draft());
        assert!(vm.task_cards[1].card_type.is_draft());

        // Heights should be different (longer content = taller card)
        // Note: Actual height calculation may vary, but they should be different
        assert_ne!(vm.task_cards[0].height, vm.task_cards[1].height);
    }

    /// Test that active cards maintain fixed height regardless of activity
    #[test]
    fn active_cards_fixed_height_regardless_of_activity() {
        let mut model = Model::default();

        // Add active task with minimal activity
        model.task_executions.push(TaskExecution {
            id: "active_minimal".to_string(),
            repository: "repo".to_string(),
            branch: "main".to_string(),
            agents: vec![SelectedModel { name: "Claude".to_string(), count: 1 }],
            state: TaskState::Active,
            timestamp: "2023-01-01 12:00:00".to_string(),
            activity: vec!["Starting...".to_string()],
            delivery_status: vec![],
        });

        // Add active task with extensive activity
        model.task_executions.push(TaskExecution {
            id: "active_extensive".to_string(),
            repository: "repo".to_string(),
            branch: "main".to_string(),
            agents: vec![SelectedModel { name: "GPT-4".to_string(), count: 1 }],
            state: TaskState::Active,
            timestamp: "2023-01-01 12:01:00".to_string(),
            activity: vec![
                "Analyzing codebase structure".to_string(),
                "Tool usage: search_codebase".to_string(),
                "Found 42 matches in 12 files".to_string(),
                "Processing results".to_string(),
                "Generating summary".to_string(),
                "Tool usage: analyze_code".to_string(),
                "Completed analysis".to_string(),
            ],
            delivery_status: vec![],
        });

        let vm: ViewModel = (&model).into();

        // Both should be active cards
        assert!(vm.task_cards[0].card_type.is_active());
        assert!(vm.task_cards[1].card_type.is_active());

        // Heights should be the same (fixed height for active cards)
        assert_eq!(vm.task_cards[0].height, vm.task_cards[1].height);
    }

    /// Test that completed/merged cards maintain fixed height
    #[test]
    fn completed_cards_fixed_height() {
        let mut model = Model::default();

        // Add completed task
        model.task_executions.push(TaskExecution {
            id: "completed_1".to_string(),
            repository: "repo".to_string(),
            branch: "main".to_string(),
            agents: vec![SelectedModel { name: "Claude".to_string(), count: 1 }],
            state: TaskState::Completed,
            timestamp: "2023-01-01 12:00:00".to_string(),
            activity: vec![],
            delivery_status: vec![],
        });

        // Add merged task
        model.task_executions.push(TaskExecution {
            id: "merged_1".to_string(),
            repository: "repo".to_string(),
            branch: "main".to_string(),
            agents: vec![SelectedModel { name: "GPT-4".to_string(), count: 1 }],
            state: TaskState::Merged,
            timestamp: "2023-01-01 12:01:00".to_string(),
            activity: vec![],
            delivery_status: vec![],
        });

        let vm: ViewModel = (&model).into();

        // Both should be completed cards
        assert!(vm.task_cards[0].card_type.is_completed());
        assert!(vm.task_cards[1].card_type.is_completed());

        // Heights should be the same (fixed height for completed cards)
        assert_eq!(vm.task_cards[0].height, vm.task_cards[1].height);
    }

    /// Test completed card metadata format: "✓ Task title • Delivery indicators"
    #[test]
    fn completed_card_metadata_format() {
        let mut model = Model::default();

        model.task_executions.push(TaskExecution {
            id: "completed_1".to_string(),
            repository: "test/repo".to_string(),
            branch: "feature/auth".to_string(),
            agents: vec![SelectedModel { name: "Claude".to_string(), count: 1 }],
            state: TaskState::Completed,
            timestamp: "2023-01-01 12:00:00".to_string(),
            activity: vec![],
            delivery_status: vec![DeliveryStatus::BranchCreated, DeliveryStatus::PullRequestCreated {
                pr_number: 42,
                title: "Add authentication".to_string()
            }],
        });

        let vm: ViewModel = (&model).into();

        // Check title line format
        assert_eq!(vm.task_cards[0].title, "Task completed_1");

        // Check metadata line contains repository and branch
        assert!(vm.task_cards[0].metadata_line.contains("test/repo"));
        assert!(vm.task_cards[0].metadata_line.contains("feature/auth"));

        // Check card type is completed
        assert!(vm.task_cards[0].card_type.is_completed());
    }

    /// Test active card metadata format with pause/delete buttons
    #[test]
    fn active_card_metadata_with_buttons() {
        let mut model = Model::default();

        model.task_executions.push(TaskExecution {
            id: "active_1".to_string(),
            repository: "test/repo".to_string(),
            branch: "main".to_string(),
            agents: vec![SelectedModel { name: "Claude".to_string(), count: 1 }],
            state: TaskState::Active,
            timestamp: "2023-01-01 12:00:00".to_string(),
            activity: vec!["Working...".to_string()],
            delivery_status: vec![],
        });

        let vm: ViewModel = (&model).into();

        // Check title line format
        assert_eq!(vm.task_cards[0].title, "Task active_1");

        // Check metadata line contains repository and branch
        assert!(vm.task_cards[0].metadata_line.contains("test/repo"));
        assert!(vm.task_cards[0].metadata_line.contains("main"));

        // Check card type is active
        assert!(vm.task_cards[0].card_type.is_active());
    }

    /// Test draft card shows placeholder when empty
    #[test]
    fn draft_card_shows_placeholder_when_empty() {
        let mut model = Model::default();

        // Add draft with empty description
        model.draft_tasks.push(DraftTask {
            id: "draft_empty".to_string(),
            description: String::new(),
            repository: "test/repo".to_string(),
            branch: "main".to_string(),
            models: vec![SelectedModel { name: "Claude".to_string(), count: 1 }],
            created_at: "2023-01-01 12:00:00".to_string(),
        });

        let vm: ViewModel = (&model).into();

        // Check that it's a draft card
        assert!(vm.task_cards[0].card_type.is_draft());

        // Check title for empty draft
        assert_eq!(vm.task_cards[0].title, "New Task");
    }

    /// Test draft card shows first line of description as title
    #[test]
    fn draft_card_title_from_first_line() {
        let mut model = Model::default();

        // Add draft with multi-line description
        model.draft_tasks.push(DraftTask {
            id: "draft_multi".to_string(),
            description: "First line of description\nSecond line\nThird line".to_string(),
            repository: "test/repo".to_string(),
            branch: "main".to_string(),
            models: vec![SelectedModel { name: "Claude".to_string(), count: 1 }],
            created_at: "2023-01-01 12:00:00".to_string(),
        });

        let vm: ViewModel = (&model).into();

        // Check title is first line of description
        assert_eq!(vm.task_cards[0].title, "First line of description");
    }


    #[test]
    fn footer_shortcuts_settings_focused() {
        let model = Model::default();
        let mut vm: ViewModel = (&model).into();
        vm.focus_element = FocusElement::Settings;

        vm.update_footer(&model);

        assert_footer_shortcuts(&vm, &[
            ("↓", "Next"),
            ("Enter", "Settings"),
            ("Ctrl+C x2", "Quit"),
        ]);
    }

    #[test]
    fn footer_shortcuts_draft_selected() {
        let model = create_model_with_drafts_and_tasks(1, 0, 0, 0);
        let mut vm: ViewModel = (&model).into();
        vm.focus_element = FocusElement::DraftTask(0); // Focus on the draft

        vm.update_footer(&model);

        assert_footer_shortcuts(&vm, &[
            ("↑↓", "Navigate"),
            ("Enter", "Edit Draft"),
            ("Ctrl+C x2", "Quit"),
        ]);
    }

    #[test]
    fn footer_shortcuts_filter_separator_focused() {
        let model = create_model_with_drafts_and_tasks(1, 1, 0, 0);
        let mut vm: ViewModel = (&model).into();
        vm.focus_element = FocusElement::FilterBarSeparator;

        vm.update_footer(&model);

        assert_footer_shortcuts(&vm, &[
            ("↑↓", "Navigate"),
            ("Enter", "Filter"),
            ("Ctrl+C x2", "Quit"),
        ]);
    }

    #[test]
    fn footer_shortcuts_active_task_selected() {
        let model = create_model_with_drafts_and_tasks(0, 1, 0, 0);
        let mut vm: ViewModel = (&model).into();
        vm.focus_element = FocusElement::ExistingTask(0); // Focus on the active task

        vm.update_footer(&model);

        assert_footer_shortcuts(&vm, &[
            ("↑↓", "Navigate"),
            ("Enter", "Show Task Progress"),
            ("Ctrl+C x2", "Quit"),
        ]);
    }

    #[test]
    fn footer_shortcuts_completed_task_selected() {
        let model = create_model_with_drafts_and_tasks(0, 0, 1, 0);
        let mut vm: ViewModel = (&model).into();
        vm.focus_element = FocusElement::ExistingTask(0); // Focus on the completed task

        vm.update_footer(&model);

        assert_footer_shortcuts(&vm, &[
            ("↑↓", "Navigate"),
            ("Enter", "Show Task Details"),
            ("Ctrl+C x2", "Quit"),
        ]);
    }

    #[test]
    fn footer_shortcuts_merged_task_selected() {
        let model = create_model_with_drafts_and_tasks(0, 0, 0, 1);
        let mut vm: ViewModel = (&model).into();
        vm.focus_element = FocusElement::ExistingTask(0); // Focus on the merged task

        vm.update_footer(&model);

        assert_footer_shortcuts(&vm, &[
            ("↑↓", "Navigate"),
            ("Enter", "Show Task Details"),
            ("Ctrl+C x2", "Quit"),
        ]);
    }

    #[test]
    fn footer_shortcuts_draft_textarea_focused_single_agent() {
        let mut model = create_model_with_drafts_and_tasks(1, 0, 0, 0);
        // Set up draft with single agent
        if let Some(draft) = model.current_draft.as_mut() {
            draft.models = vec![SelectedModel { name: "Claude".to_string(), count: 1 }];
        }

        let mut vm: ViewModel = (&model).into();
        vm.focus_element = FocusElement::TaskDescription;

        vm.update_footer(&model);

        assert_footer_shortcuts(&vm, &[
            ("Enter", "Launch Agent"),
            ("Shift+Enter", "New Line"),
            ("Tab", "Next Field"),
        ]);
    }

    #[test]
    fn footer_shortcuts_draft_textarea_focused_multiple_agents() {
        let mut model = create_model_with_drafts_and_tasks(1, 0, 0, 0);
        // Set up draft with multiple agents
        if let Some(draft) = model.current_draft.as_mut() {
            draft.models = vec![
                SelectedModel { name: "Claude".to_string(), count: 1 },
                SelectedModel { name: "GPT-4".to_string(), count: 1 },
            ];
        }

        let mut vm: ViewModel = (&model).into();
        vm.focus_element = FocusElement::TaskDescription;

        vm.update_footer(&model);

        assert_footer_shortcuts(&vm, &[
            ("Enter", "Launch Agents"),
            ("Shift+Enter", "New Line"),
            ("Tab", "Next Field"),
        ]);
    }

    #[test]
    fn footer_shortcuts_modal_active() {
        let model = create_model_with_drafts_and_tasks(1, 1, 0, 0);
        let mut vm: ViewModel = (&model).into();
        vm.modal_state = ModalState::RepositorySearch;

        vm.update_footer(&model); // Modal takes precedence

        assert_footer_shortcuts(&vm, &[
            ("↑↓", "Navigate"),
            ("Enter", "Select"),
            ("Esc", "Back"),
        ]);
    }

    /// Test that visual selection state is highlighted
    #[test]
    fn visual_selection_state_highlighted() {
        let model = create_model_with_drafts_and_tasks(1, 1, 0, 0);
        let mut vm: ViewModel = (&model).into();

        // Initially focused on first draft task
        assert_eq!(vm.focus_element, FocusElement::DraftTask(0));
        assert!(vm.task_cards[0].is_selected);
        assert!(!vm.task_cards[1].is_selected);

        // Navigate to second draft task
        vm.handle_navigation(NavigationDirection::Down, &model);
        assert_eq!(vm.focus_element, FocusElement::DraftTask(1));
        assert!(!vm.task_cards[0].is_selected);
        assert!(vm.task_cards[1].is_selected);

        // Navigate to filter separator
        vm.handle_navigation(NavigationDirection::Down, &model);
        assert_eq!(vm.focus_element, FocusElement::FilterBarSeparator);
        assert!(!vm.task_cards[0].is_selected);
        assert!(!vm.task_cards[1].is_selected);
    }

    /// Test navigation wraps around the complete hierarchy
    #[test]
    fn navigation_wraps_around_hierarchy() {
        let model = create_model_with_drafts_and_tasks(1, 1, 0, 0);
        let mut vm: ViewModel = (&model).into();

        // Start with focus on first draft task
        assert_eq!(vm.focus_element, FocusElement::DraftTask(0));

        // Down to filter separator
        vm.handle_navigation(NavigationDirection::Down, &model);
        assert_eq!(vm.focus_element, FocusElement::FilterBarSeparator);

        // Down to existing task
        vm.handle_navigation(NavigationDirection::Down, &model);
        assert_eq!(vm.focus_element, FocusElement::ExistingTask(0));

        // Down should wrap to settings
        vm.handle_navigation(NavigationDirection::Down, &model);
        assert_eq!(vm.focus_element, FocusElement::Settings);

        // Down to first draft task
        vm.handle_navigation(NavigationDirection::Down, &model);
        assert_eq!(vm.focus_element, FocusElement::DraftTask(0));
    }

    /// Test that initially focused element is the top draft task
    #[test]
    fn initially_focused_element_is_top_draft_task() {
        let mut model = Model::default();

        // Add draft task
        model.draft_tasks.push(DraftTask {
            id: "draft_1".to_string(),
            description: "Test draft".to_string(),
            repository: "repo".to_string(),
            branch: "main".to_string(),
            models: vec![SelectedModel { name: "Claude".to_string(), count: 1 }],
            created_at: "2023-01-01 12:00:00".to_string(),
        });

        let vm: ViewModel = (&model).into();

        // Initially focused on the draft task
        assert_eq!(vm.focus_element, FocusElement::DraftTask(0));
    }

    /// Test textarea navigation prioritizes text lines over focus movement
    #[test]
    fn textarea_navigation_prioritizes_text_lines() {
        let mut model = create_model_with_drafts_and_tasks(1, 0, 0, 0);
        let mut vm: ViewModel = (&model).into();

        // Manually create a multi-line draft for testing
        if let Some(draft) = model.draft_tasks.get_mut(0) {
            draft.description = "Line 1\nLine 2\nLine 3".to_string();
        }

        // Enter draft editing mode
        vm.enter_draft_editing_mode(0, &model);
        assert_eq!(vm.focus_element, FocusElement::TaskDescription);

        // Should be able to navigate within textarea first
        let handled = vm.handle_textarea_navigation(NavigationDirection::Down, &model);
        assert!(handled); // Should handle navigation within textarea

        // At bottom of textarea, should allow focus to move
        // (This would require setting cursor to bottom first, simplified for test)
    }

    // Test helper functions for creating common test scenarios

    /// Helper function to create a draft task
    fn create_draft_task(id: &str, description: &str, repository: &str, branch: &str) -> DraftTask {
        DraftTask {
            id: id.to_string(),
            description: description.to_string(),
            repository: repository.to_string(),
            branch: branch.to_string(),
            models: vec![SelectedModel { name: "Claude".to_string(), count: 1 }],
            created_at: "2023-01-01 12:00:00".to_string(),
        }
    }

    /// Helper function to create an active task execution
    fn create_active_task(id: &str, repository: &str, branch: &str) -> TaskExecution {
        TaskExecution {
            id: id.to_string(),
            repository: repository.to_string(),
            branch: branch.to_string(),
            agents: vec![SelectedModel { name: "Claude".to_string(), count: 1 }],
            state: TaskState::Active,
            timestamp: "2023-01-01 12:00:00".to_string(),
            activity: vec!["Working...".to_string()],
            delivery_status: vec![],
        }
    }

    /// Helper function to create a completed task execution
    fn create_completed_task(id: &str, repository: &str, branch: &str) -> TaskExecution {
        TaskExecution {
            id: id.to_string(),
            repository: repository.to_string(),
            branch: branch.to_string(),
            agents: vec![SelectedModel { name: "Claude".to_string(), count: 1 }],
            state: TaskState::Completed,
            timestamp: "2023-01-01 12:00:00".to_string(),
            activity: vec![],
            delivery_status: vec![DeliveryStatus::BranchCreated],
        }
    }

    /// Helper function to create a merged task execution
    fn create_merged_task(id: &str, repository: &str, branch: &str) -> TaskExecution {
        TaskExecution {
            id: id.to_string(),
            repository: repository.to_string(),
            branch: branch.to_string(),
            agents: vec![SelectedModel { name: "Claude".to_string(), count: 1 }],
            state: TaskState::Merged,
            timestamp: "2023-01-01 12:00:00".to_string(),
            activity: vec![],
            delivery_status: vec![DeliveryStatus::PullRequestMerged { pr_number: 42 }],
        }
    }

    /// Helper function to create a model with initial draft and tasks
    fn create_model_with_drafts_and_tasks(draft_count: usize, active_count: usize, completed_count: usize, merged_count: usize) -> Model {
        let mut model = Model::default();

        // Add draft tasks (newest first, so we add them in reverse order)
        for i in (0..draft_count).rev() {
            let timestamp = format!("2023-01-01 1{}:00:00", i);
            let mut draft = create_draft_task(&format!("draft_{}", i), &format!("Draft {}", i), "repo", "main");
            draft.created_at = timestamp;
            model.draft_tasks.push(draft);
        }

        // Add active tasks
        for i in 0..active_count {
            model.task_executions.push(create_active_task(&format!("active_{}", i), "repo", "main"));
        }

        // Add completed tasks
        for i in 0..completed_count {
            model.task_executions.push(create_completed_task(&format!("completed_{}", i), "repo", "main"));
        }

        // Add merged tasks
        for i in 0..merged_count {
            model.task_executions.push(create_merged_task(&format!("merged_{}", i), "repo", "main"));
        }

        model
    }

    /// Helper function to assert exact footer shortcuts in order
    fn assert_footer_shortcuts(vm: &ViewModel, expected: &[(&str, &str)]) {
        assert_eq!(vm.footer.shortcuts.len(), expected.len(),
            "Expected {} shortcuts, got {}", expected.len(), vm.footer.shortcuts.len());

        for (i, (expected_key, expected_desc)) in expected.iter().enumerate() {
            assert_eq!(vm.footer.shortcuts[i].key, *expected_key,
                "Shortcut {} key mismatch: expected '{}', got '{}'",
                i, expected_key, vm.footer.shortcuts[i].key);
            assert_eq!(vm.footer.shortcuts[i].description, *expected_desc,
                "Shortcut {} description mismatch: expected '{}', got '{}'",
                i, expected_desc, vm.footer.shortcuts[i].description);
        }
    }

    // #[test]
    // fn view_model_formats_delivery_indicators() {
    //     // This test needs to be updated for the new DeliveryIndicator variants
    //     // TODO: Update to use BranchCreated, PrCreated, PrMerged
    // }
}
