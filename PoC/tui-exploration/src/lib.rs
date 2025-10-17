//! TUI Exploration - MVVM Architecture Implementation
//!
//! This crate demonstrates a proper MVVM (Model-View-ViewModel) architecture
//! for terminal user interfaces using Ratatui, following the patterns described
//! in the Agent Harbor TUI PRD and research documents.
//!
//! ## Architecture Overview
//!
//! The codebase follows strict separation of concerns with clear boundaries:
//!
//! ### Model (`model.rs`)
//! - **Domain Logic**: Business rules, task operations, state transitions
//! - **Domain Entities**: `TaskExecution`, `DraftTask`, `DeliveryStatus`, `TaskState`
//! - **Domain Messages**: `DomainMsg` and `NetworkMsg` definitions and handling
//! - **UI-Agnostic**: No knowledge of UI rendering or events
//! - **Testable**: Business logic without UI dependencies
//!
//! ### ViewModel (`view_model.rs`)
//! - **UI Logic**: Key handling, navigation, focus management
//! - **Presentation Models**: `TaskCard`, UI-specific display types
//! - **UI Messages**: `Msg` enum for low-level UI events
//! - **UI State**: `FocusElement`, `ModalState`, `SearchMode` enums
//! - **State Transformation**: Domain → UI data structures
//! - **Event Translation**: UI events → domain messages
//!
//! ### View (`view.rs`)
//! - **Rendering**: Ratatui widget creation and terminal output
//! - **Pure Functions**: ViewModel → terminal display
//! - **No State**: Never modifies application state
//! - **Presentation Only**: Visual styling and layout
//!
//! ## Message Flow
//!
//! ```text
//! UI Event → ViewModel → DomainMsg → Model → State Update
//!     ↓           ↓           ↓         ↓         ↓
//!   KeyPress  Translation  BusinessOp  Domain   New State
//! ```
//!
//! ## Module Exports
//!
//! This library exports the core types and functions needed to build
//! terminal UIs using this MVVM architecture. The public API is designed
//! to be minimal and focused on the essential building blocks.

pub mod view_model;
pub mod view;
pub mod settings;
pub mod workspace_files;
pub mod workspace_workflows;
pub mod task_manager;

// Re-export commonly used types
pub use ah_core::task_manager::ToolStatus;
pub use ah_domain_types::{SelectedModel, TaskState, DraftTask, TaskExecution, DeliveryStatus, Repository, Branch, TaskInfo};
pub use view_model::{Msg, ViewModel, TaskCardTypeEnum, TaskItem};
pub use ah_tui::view_model::{TaskEntryViewModel, TaskExecutionViewModel, FocusElement, ModalState, TaskCardType, ActivityEntry, ButtonStyle, ButtonViewModel, DraftSaveState, SearchMode, DeliveryIndicator, FilterOptions, AutoSaveState, TaskCard, TaskCardMetadata};
pub use ah_tui::view_model::task_entry::DraftControlsViewModel;
pub use view::ViewCache;
pub use settings::{Settings, FontStyle, SelectionDialogStyle, KeymapConfig, KeyBinding};
pub use workspace_workflows::{WorkspaceWorkflows, PathWorkspaceWorkflows};
pub use task_manager::{
    TaskManager, MockTaskManager, TaskLaunchParams, TaskLaunchResult,
    TaskEvent, TaskStatus, LogLevel,
    SaveDraftResult
};
pub use ah_workflows::{WorkflowResult, WorkflowError};
