// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

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
pub mod view_model;
pub mod viewmodel;

pub use app::*;
pub use error::*;
pub use golden::*;
pub use model::*;
pub use msg::*;
pub use task::{ButtonFocus, ModalState, ModelSelection, Task, TaskState};
pub use test_runtime::*;
pub use view::{Theme, ViewCache};
pub use view_model::{
    AgentActivityRow, AutoSaveState, ButtonStyle, ButtonViewModel, DeliveryIndicator,
    DraftSaveState, FilterOptions, FocusElement, SearchMode, TaskCardType,
    TaskEntryControlsViewModel, TaskEntryViewModel, TaskExecutionViewModel, TaskMetadataViewModel,
    TaskStatusFilter, TimeRangeFilter,
};
pub use viewmodel::*;

use ratatui::{Terminal, backend::TestBackend};

/// Helpers for tests/runners to render with a deterministic backend
pub fn create_test_terminal(width: u16, height: u16) -> Terminal<TestBackend> {
    let backend = TestBackend::new(width, height);
    Terminal::new(backend).expect("test terminal")
}
