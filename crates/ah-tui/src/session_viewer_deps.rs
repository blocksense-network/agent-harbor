// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Dependency container for running session viewer loops.
//!
//! The live session viewer originally wired its dependencies inline inside the
//! record/replay commands. For mock-agent driven simulations and unit-style
//! harnesses we need a lightweight way to assemble the viewer with alternate
//! data sources (e.g. pre-baked terminal states or scenario transcripts).
//! `AgentSessionDependencies` mirrors the dependency-injection style used by
//! `dashboard_loop.rs`, enabling test/production parity while keeping the
//! viewer code agnostic of how the dependencies are produced.

use std::{cell::RefCell, rc::Rc, sync::Arc};

use crate::settings::Settings;
use crate::view_model::task_execution::AgentActivityRow;
use crate::{
    terminal::TerminalConfig, view_model::autocomplete::AutocompleteDependencies,
    viewer::ViewerConfig,
};
use ah_core::TaskManager;
use ah_recorder::TerminalState;

/// Which UI should be rendered for an agent session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentSessionUiMode {
    /// Existing vt100-based session viewer.
    SessionViewer,
    /// Agent Activity TUI (milestone 0.5 mock mode).
    AgentActivity,
}

/// Bundles all dependencies required to run an agent session loop.
#[derive(Clone)]
pub struct AgentSessionDependencies {
    pub recording_terminal_state: Rc<RefCell<TerminalState>>,
    pub viewer_config: ViewerConfig,
    pub task_manager: Arc<dyn TaskManager>,
    pub autocomplete: Option<Arc<AutocompleteDependencies>>,
    pub settings: Settings,
    pub theme: crate::theme::Theme,
    pub terminal_config: TerminalConfig,
    pub ui_mode: AgentSessionUiMode,
    /// Optional pre-seeded activity entries for Agent Activity UI mode (time, row).
    pub activity_entries: Vec<(u64, AgentActivityRow)>,
}

impl AgentSessionDependencies {
    /// Convenience constructor for the existing session viewer mode.
    pub fn session_viewer(
        recording_terminal_state: Rc<RefCell<TerminalState>>,
        viewer_config: ViewerConfig,
        task_manager: Arc<dyn TaskManager>,
        autocomplete: Option<Arc<AutocompleteDependencies>>,
    ) -> Self {
        Self {
            recording_terminal_state,
            viewer_config,
            task_manager,
            autocomplete,
            settings: Settings::default(),
            theme: crate::theme::Theme::default(),
            terminal_config: TerminalConfig::minimal(),
            ui_mode: AgentSessionUiMode::SessionViewer,
            activity_entries: Vec::new(),
        }
    }

    /// Convenience constructor for Agent Activity mock sessions (used by mock-agent and tests).
    pub fn agent_activity(
        recording_terminal_state: Rc<RefCell<TerminalState>>,
        viewer_config: ViewerConfig,
        task_manager: Arc<dyn TaskManager>,
        settings: Settings,
        theme: crate::theme::Theme,
        terminal_config: TerminalConfig,
        activity_entries: Vec<(u64, AgentActivityRow)>,
    ) -> Self {
        Self {
            recording_terminal_state,
            viewer_config,
            task_manager,
            autocomplete: None,
            settings,
            theme,
            terminal_config,
            ui_mode: AgentSessionUiMode::AgentActivity,
            activity_entries,
        }
    }
}
