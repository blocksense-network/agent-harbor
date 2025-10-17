//! ViewModel types and structures for the TUI
//!
//! This module contains the presentation models and UI state management
//! types that bridge the domain model to the UI rendering layer.

pub mod task_entry;
pub mod task_execution;

// Re-export the main types
pub use task_entry::{TaskEntryViewModel, DraftControlsViewModel};
pub use task_execution::{TaskExecutionViewModel, ActivityEntry, TaskCardType, TaskCardMetadata};

// Common UI types used across view models
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FocusElement {
    TaskDescription,
    RepositorySelector,
    BranchSelector,
    ModelSelector,
    GoButton,
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
    StopButton(usize),
    Filter(usize),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ModalState {
    None,
    RepositorySearch,
    BranchSearch,
    ModelSearch,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ButtonStyle {
    Normal,
    Focused,
    Active,
    Disabled,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ButtonViewModel {
    pub text: String,
    pub is_focused: bool,
    pub style: ButtonStyle,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DraftSaveState {
    Saved,
    Saving,
    Unsaved,
    Error,
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

/// Task card presentation model - how tasks are displayed in the UI
#[derive(Debug, Clone, PartialEq)]
pub struct TaskCard {
    pub id: String,
    pub title: String,
    pub repository: String,
    pub branch: String,
    pub agents: Vec<ah_domain_types::SelectedModel>,
    pub state: ah_domain_types::TaskState,
    pub timestamp: String,
    pub activity: Vec<String>, // For active tasks
    pub delivery_indicators: Vec<DeliveryIndicator>, // For completed/merged tasks
}
