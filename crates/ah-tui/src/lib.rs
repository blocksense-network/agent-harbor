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
pub mod record;
pub mod replay;
pub mod settings;
pub mod task;
pub mod terminal;
pub mod view;
pub mod view_model;
pub mod viewer;

pub use error::*;
pub use golden::*;
pub use msg::*;
pub use settings::{
    FontStyle, KeyMatcher, KeyboardLocalization, KeyboardOperation, KeyboardShortcut, MetaKey,
    Platform, SelectionDialogStyle, Settings,
};
pub use task::{ButtonFocus, ModalState, ModelSelection, Task, TaskState};

// Re-export workspace files enumerator types from ah-core
pub use ah_core::{RepositoryFile, WorkspaceFilesEnumerator};
pub use view::{Theme, ViewCache};
pub use view_model::{
    AgentActivityRow,
    AutoSaveState,
    ButtonStyle,
    ButtonViewModel,
    DashboardFocusState,
    DeliveryIndicator,
    DraftSaveState,
    FilterOptions,
    FooterAction,
    ModalType,
    ModalViewModel,
    ModelOptionViewModel,
    // Dashboard ViewModel types
    MouseAction,
    Msg,
    SearchMode,
    SettingsFieldType,
    SettingsFieldViewModel,
    StatusBarViewModel,
    TaskCardInfo,
    TaskCardType,
    TaskCardTypeEnum,
    TaskEntryControlsViewModel,
    TaskEntryViewModel,
    TaskExecutionViewModel,
    TaskItem,
    TaskMetadataViewModel,
    TaskStatusFilter,
    TimeRangeFilter,
    ViewModel,
};
pub use viewer::{ViewerConfig, ViewerEventLoop};

use ratatui::{Terminal, backend::TestBackend};

/// Helpers for tests/runners to render with a deterministic backend
pub fn create_test_terminal(width: u16, height: u16) -> Terminal<TestBackend> {
    let backend = TestBackend::new(width, height);
    Terminal::new(backend).expect("test terminal")
}
