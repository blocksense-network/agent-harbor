// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! ViewModel Layer - UI State and Presentation Models
//!
//! This module contains the presentation models and UI state management
//! that bridge the domain model to the UI rendering layer. The ViewModel
//! layer handles all UI logic, input processing, and state transformations.
//!
//! ## What Belongs Here:
//!
//! ✅ **Presentation Models**: ViewModel structs that transform domain data for UI
//! ✅ **UI State Management**: Focus tracking, selection states, modal states
//! ✅ **Input Processing**: Key handling, mouse event processing, validation
//! ✅ **State Transformations**: Domain objects → UI display formats
//! ✅ **UI Logic**: Navigation, focus management, interaction handling
//! ✅ **Network Operations**: API calls and data fetching are allowed here!
//!
//! ## Architecture Deviation from Classic MVVM:
//!
//! Unlike classic MVVM architecture, we allow network API calls to be implemented
//! directly within the ViewModel. This enables the most natural code expression
//! without requiring complex message passing for input handling, network requests,
//! and responses.
//!
//! ## Primary Mission: Headless Testing
//!
//! The main goal of the ViewModel is to enable **fully headless testing** that can
//! run with simulated time at maximum CPU speed. This is made possible by Tokio's
//! fake time support, allowing the ViewModel's mission to be accomplished while
//! keeping the code simple and natural.
//!
//! ## What Does NOT Belong Here:
//!
//! ❌ **Business Logic**: Domain rules, validation, data operations
//! ❌ **Rendering**: Ratatui widget creation, styling, layout
//! ❌ **Domain Entities**: Core business objects, persistence
//!
//! ## Architecture Role:
//!
//! The ViewModel is the reactive bridge between Model and View:
//! 1. **Receives Domain Events** - Updates from the Model layer
//! 2. **Manages UI State** - Focus, selection, navigation state
//! 3. **Processes Input** - Key events, mouse events, user interactions
//! 4. **Transforms Data** - Domain objects → presentation models
//! 5. **Updates View** - Notifies View of state changes for re-rendering
//!
//! ## Design Principles:
//!
//! - **Reactive Updates**: ViewModel responds to domain changes and user input
//! - **State Encapsulation**: All UI state is managed in one place
//! - **Input Abstraction**: Platform-agnostic input event handling
//! - **Data Transformation**: Domain → UI data structure conversions
//! - **Event-Driven**: Clear separation of input → processing → output
//! - **Testability**: Fully headless testing with mocked external APIs
//!                    and simulated time

pub mod autocomplete;
pub mod dashboard_model;
pub mod input;
pub mod session_viewer_model;
pub mod task_entry;
pub mod task_execution;

// Re-export the main types
pub use autocomplete::{AutocompleteKeyResult, InlineAutocomplete, Item, MenuContext, Trigger};
pub use dashboard_model::{
    DashboardFocusState, FooterAction, ModalType, ModalViewModel, ModelOptionViewModel,
    MouseAction, Msg, SettingsFieldType, SettingsFieldViewModel, StatusBarViewModel, TaskCardInfo,
    TaskCardTypeEnum, TaskItem, ViewModel, create_draft_card_from_task,
};
pub use session_viewer_model::{
    DisplayItem, GutterConfig, GutterPosition, SessionViewerFocusState as RecorderFocusState,
    SessionViewerMode, SessionViewerMouseAction, SessionViewerMsg as RecorderMsg,
    SessionViewerViewModel as RecorderViewModel, StatusBarViewModel as RecorderStatusBarViewModel,
};
pub use task_entry::{TaskEntryControlsViewModel, TaskEntryViewModel};
pub use task_execution::{
    AgentActivityRow, TaskCardType, TaskExecutionFocusState, TaskExecutionViewModel,
    TaskMetadataViewModel,
};

// Filter bar types are defined in this module, no need to re-export them

// External dependencies
use ratatui::style::Color;

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

/// Filter control types for task filtering
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FilterControl {
    Repository,
    Status,
    Creator,
}

impl FilterControl {
    pub fn index(self) -> usize {
        match self {
            FilterControl::Repository => 0,
            FilterControl::Status => 1,
            FilterControl::Creator => 2,
        }
    }
}

/// Filter bar view model containing all state needed for rendering
#[derive(Debug, Clone)]
pub struct FilterBarViewModel {
    pub repository_value: String,
    pub status_value: String,
    pub creator_value: String,
    pub focused_element: Option<FilterControl>,
    pub filter_bar_focused: bool,
}

impl Default for FilterBarViewModel {
    fn default() -> Self {
        Self {
            repository_value: "All".to_string(),
            status_value: "All".to_string(),
            creator_value: "All".to_string(),
            focused_element: None,
            filter_bar_focused: false,
        }
    }
}

/// Theme for filter bar styling
#[derive(Debug, Clone)]
pub struct FilterBarTheme {
    pub border: Color,
    pub border_focused: Color,
    pub text: Color,
    pub muted: Color,
    pub primary: Color,
}

impl Default for FilterBarTheme {
    fn default() -> Self {
        Self {
            border: Color::Blue,
            border_focused: Color::Cyan,
            text: Color::White,
            muted: Color::Gray,
            primary: Color::Blue,
        }
    }
}
