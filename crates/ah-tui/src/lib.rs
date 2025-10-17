//! Terminal User Interface for agent-harbor
//!
//! This crate provides a Ratatui-based TUI for creating, monitoring,
//! and managing agent coding sessions with seamless multiplexer integration.

pub mod app;
pub mod error;
pub mod event;
pub mod golden;
pub mod model;
pub mod msg;
pub mod task;
pub mod test_runtime;
pub mod ui;
pub mod view;
pub mod viewmodel;
pub mod view_model;

pub use app::*;
pub use error::*;
pub use golden::*;
pub use model::*;
pub use msg::*;
pub use task::{ButtonFocus, ModalState, ModelSelection, Task, TaskState};
pub use test_runtime::*;
pub use viewmodel::*;
pub use view::{ViewCache, Theme};
pub use view_model::{TaskEntryViewModel, TaskExecutionViewModel, AgentActivityRow, TaskCardType, TaskMetadataViewModel, TaskEntryControlsViewModel, FocusElement, ButtonStyle, ButtonViewModel, DraftSaveState, SearchMode, DeliveryIndicator, FilterOptions, TaskStatusFilter, TimeRangeFilter, AutoSaveState};

use ratatui::{backend::TestBackend, Terminal};

/// Helpers for tests/runners to render with a deterministic backend
pub fn create_test_terminal(width: u16, height: u16) -> Terminal<TestBackend> {
    let backend = TestBackend::new(width, height);
    Terminal::new(backend).expect("test terminal")
}
