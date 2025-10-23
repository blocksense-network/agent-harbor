// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! agent-harbor specific multiplexer abstractions
//!
//! This crate provides AH-specific functionality on top of the low-level
//! multiplexer trait, including standard layouts and task management.

use ah_mux_core::*;
use std::collections::HashMap;

/// Handle to a multiplexer session with role-based pane management
#[derive(Debug, Clone)]
pub struct LayoutHandle {
    pub window_id: WindowId,
    pub panes: HashMap<PaneRole, PaneId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaneRole {
    Editor, // Left pane: editor or terminal
    Agent,  // Right pane: agent activity and logs
    Logs,   // Optional bottom pane for logs
}

/// Standard AH layout configuration
#[derive(Debug, Clone)]
pub struct LayoutConfig<'a> {
    pub task_id: &'a str,
    pub working_dir: &'a std::path::Path,
    pub editor_cmd: Option<&'a str>, // Default: $EDITOR or preferred editor
    pub agent_cmd: &'a str,
    pub log_cmd: Option<&'a str>, // Optional separate log command
}

#[derive(thiserror::Error, Debug)]
pub enum AwMuxError {
    #[error("multiplexer error: {0}")]
    Mux(#[from] MuxError),
    #[error("layout error: {0}")]
    Layout(String),
    #[error("configuration error: {0}")]
    Config(String),
}

/// AH-specific multiplexer adapter
pub struct AwMultiplexer<M: Multiplexer> {
    mux: M,
}

impl<M: Multiplexer> AwMultiplexer<M> {
    pub fn new(mux: M) -> Self {
        Self { mux }
    }

    pub fn inner(&self) -> &M {
        &self.mux
    }

    pub fn inner_mut(&mut self) -> &mut M {
        &mut self.mux
    }

    /// Create a standard AH task layout with editor (left) and agent (right) panes
    pub fn create_task_layout(&self, config: &LayoutConfig) -> Result<LayoutHandle, AwMuxError> {
        let title = format!("ah-task-{}", config.task_id);

        // Create new window
        let window_opts = WindowOptions {
            title: Some(&title),
            cwd: Some(config.working_dir),
            profile: None,
            focus: true,
        };
        let window_id = self.mux.open_window(&window_opts)?;

        let mut panes = HashMap::new();

        // The window_id is session:window, the initial pane is session:window.0
        let editor_pane = format!("{}.0", window_id);
        let editor_cmd = config.editor_cmd.unwrap_or("bash");
        self.mux.run_command(
            &editor_pane,
            editor_cmd,
            &CommandOptions {
                cwd: Some(config.working_dir),
                env: None,
            },
        )?;
        panes.insert(PaneRole::Editor, editor_pane);

        // Split for agent pane (right side)
        let agent_pane = self.mux.split_pane(
            &window_id,
            None, // Split the window itself
            SplitDirection::Horizontal,
            Some(70), // 70% for editor, 30% for agent
            &CommandOptions {
                cwd: Some(config.working_dir),
                env: None,
            },
            Some(config.agent_cmd),
        )?;
        panes.insert(PaneRole::Agent, agent_pane);

        // Optional log pane at bottom
        if let Some(log_cmd) = config.log_cmd {
            let log_pane = self.mux.split_pane(
                &window_id,
                Some(&panes[&PaneRole::Agent]),
                SplitDirection::Vertical,
                Some(70), // 70% agent, 30% logs
                &CommandOptions {
                    cwd: Some(config.working_dir),
                    env: None,
                },
                Some(log_cmd),
            )?;
            panes.insert(PaneRole::Logs, log_pane);
        }

        Ok(LayoutHandle { window_id, panes })
    }

    /// Find an existing task layout by task ID
    pub fn find_task_layout(&self, task_id: &str) -> Result<Option<LayoutHandle>, AwMuxError> {
        let title_substr = format!("ah-task-{}", task_id);
        let windows = self.mux.list_windows(Some(&title_substr))?;

        if windows.is_empty() {
            return Ok(None);
        }

        // Use the first matching window
        let window_id = windows.into_iter().next().unwrap();
        let panes = self.mux.list_panes(&window_id)?;

        // Try to identify panes by role (this is best-effort)
        let mut pane_map = HashMap::new();

        // For now, assume standard layout: pane .1 = editor, .2 = agent
        for pane in panes {
            if pane.ends_with(".1") {
                pane_map.insert(PaneRole::Editor, pane);
            } else if pane.ends_with(".2") && !pane.ends_with(".3") {
                pane_map.insert(PaneRole::Agent, pane);
            } else if pane.ends_with(".3") {
                pane_map.insert(PaneRole::Logs, pane);
            }
        }

        Ok(Some(LayoutHandle {
            window_id,
            panes: pane_map,
        }))
    }

    /// Focus an existing task layout
    pub fn focus_task_layout(&self, layout: &LayoutHandle) -> Result<(), AwMuxError> {
        self.mux.focus_window(&layout.window_id)?;
        // Focus the agent pane by default
        if let Some(agent_pane) = layout.panes.get(&PaneRole::Agent) {
            self.mux.focus_pane(agent_pane)?;
        }
        Ok(())
    }

    /// Send text to a specific pane in a layout
    pub fn send_to_pane(
        &self,
        layout: &LayoutHandle,
        role: PaneRole,
        text: &str,
    ) -> Result<(), AwMuxError> {
        if let Some(pane_id) = layout.panes.get(&role) {
            self.mux.send_text(pane_id, text)?;
        }
        Ok(())
    }
}

/// Get the default multiplexer for the current system
pub fn default_multiplexer() -> Result<Box<dyn Multiplexer + Send + Sync>, AwMuxError> {
    // This will be implemented when ah-mux provides the multiplexer implementations
    Err(AwMuxError::Config(
        "No multiplexer implementations available yet".to_string(),
    ))
}

/// Get a multiplexer by name
pub fn multiplexer_by_name(_name: &str) -> Result<Box<dyn Multiplexer + Send + Sync>, AwMuxError> {
    // This will be implemented when ah-mux provides the multiplexer implementations
    Err(AwMuxError::Config(
        "No multiplexer implementations available yet".to_string(),
    ))
}
