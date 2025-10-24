// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Terminal User Interface for agent-harbor
//!
//! This crate provides a Ratatui-based TUI for creating, monitoring,
//! and managing agent coding sessions with seamless multiplexer integration.

pub mod dashboard_loop;
pub mod error;
pub mod event;
pub mod golden;
pub mod msg;
pub mod settings;
pub mod task;
pub mod view;
pub mod view_model;

pub use error::*;
pub use golden::*;
pub use msg::*;
pub use settings::{FontStyle, KeyboardLocalization, KeyboardOperation, KeyboardShortcut, KeyMatcher, MetaKey, Platform, SelectionDialogStyle, Settings};
pub use task::{ButtonFocus, ModalState, ModelSelection, Task, TaskState};

// Re-export workspace files enumerator types from ah-core
pub use ah_core::{RepositoryFile, WorkspaceFilesEnumerator};
pub use view::{Theme, ViewCache};
pub use view_model::{
    AgentActivityRow, AutoSaveState, ButtonStyle, ButtonViewModel, DeliveryIndicator,
    DraftSaveState, FilterOptions, FocusElement, SearchMode, TaskCardType,
    TaskEntryControlsViewModel, TaskEntryViewModel, TaskExecutionViewModel, TaskMetadataViewModel,
    TaskStatusFilter, TimeRangeFilter,
    // Dashboard ViewModel types
    MouseAction, Msg, ViewModel, FooterAction, ModalType, ModalViewModel,
    ModelOptionViewModel, SettingsFieldType, SettingsFieldViewModel, StatusBarViewModel,
    TaskCardInfo, TaskCardTypeEnum, TaskItem,
};

use ratatui::{Terminal, backend::TestBackend};

/// Helpers for tests/runners to render with a deterministic backend
pub fn create_test_terminal(width: u16, height: u16) -> Terminal<TestBackend> {
    let backend = TestBackend::new(width, height);
    Terminal::new(backend).expect("test terminal")
}
