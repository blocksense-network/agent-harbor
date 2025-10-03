//! The Model layer - domain state and rules (no I/O, no Ratatui)
//!
//! This implements the state machine core that processes messages
//! and updates the application state.

use crate::app::AppState;
use crate::msg::{Msg, NetMsg, RestSuccessMsg};
use crate::task::{Task, TaskState};
use ah_client_api::ClientApi;
use ah_rest_api_contract::{AgentCapability, CreateTaskRequest, Project, Repository};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::sync::Arc;

/// The Model represents the domain state and business logic
/// It processes messages and updates state deterministically
pub struct Model<C: ClientApi> {
    pub state: AppState,
    client: Arc<C>,
}

impl<C: ClientApi> Model<C> {
    /// Create a new model with initial state
    pub fn new(client: Arc<C>) -> Self {
        Self {
            state: AppState::default(),
            client,
        }
    }

    /// Process a single message and update state
    ///
    /// This is the core state machine method that handles all external stimuli.
    /// It's designed to be deterministic and side-effect free (no I/O).
    pub async fn update(&mut self, msg: Msg) {
        match msg {
            Msg::Key(key_event) => self.handle_key(key_event),
            Msg::Tick => self.handle_tick(),
            Msg::Net(net_msg) => self.handle_net_msg(net_msg).await,
            Msg::Quit => {
                // Quit is handled at the application level
            }
        }
    }

    /// Handle keyboard input
    fn handle_key(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Up => {
                // Navigate up in task list
                if self.state.selected_task_index > 0 {
                    self.state.selected_task_index -= 1;
                }
            }
            KeyCode::Down => {
                // Navigate down in task list
                let max_index = self.state.tasks.len().saturating_sub(1);
                if self.state.selected_task_index < max_index {
                    self.state.selected_task_index += 1;
                }
            }
            KeyCode::Char(c) => {
                self.handle_char_input(c, key_event.modifiers);
            }
            KeyCode::Backspace => {
                self.handle_backspace();
            }
            KeyCode::Enter => {
                if key_event.modifiers.contains(KeyModifiers::SHIFT) {
                    self.handle_shift_enter();
                } else {
                    self.handle_enter();
                }
            }
            KeyCode::Esc => {
                self.handle_escape();
            }
            _ => {}
        }
    }

    /// Handle time tick
    fn handle_tick(&mut self) {
        // Periodic updates can be handled here
        // For example, polling for status updates, etc.
    }

    /// Handle network messages
    async fn handle_net_msg(&mut self, net_msg: NetMsg) {
        match net_msg {
            NetMsg::Sse(session_event) => {
                // Handle SSE events (status updates, logs, etc.)
                self.handle_sse_event(session_event);
            }
            NetMsg::RestSuccess(success_msg) => {
                // Handle successful REST responses
                self.handle_rest_success(success_msg);
            }
            NetMsg::RestError(error) => {
                // Handle REST errors
                self.state.error = Some(error);
                self.state.loading = false;
            }
        }
    }

    /// Handle SSE events
    fn handle_sse_event(&mut self, _event: ah_rest_api_contract::SessionEvent) {
        // Handle different types of SSE events
        // This would update status, logs, progress, etc.
    }

    /// Handle successful REST responses
    fn handle_rest_success(&mut self, success_msg: RestSuccessMsg) {
        match success_msg {
            RestSuccessMsg::TaskCreated(_response) => {
                // Task created successfully
                self.state.loading = false;
                self.state.new_task_description.clear();
                self.state.has_unsaved_draft = false;
                // Could show success message or navigate to monitoring view
            }
            _ => {} // Other responses not handled in this simplified version
        }
    }

    /// Handle character input for new task - not used in new design
    fn handle_char_input(&mut self, _c: char, _modifiers: KeyModifiers) {
        // Input is now handled in the UI layer for task creation
    }

    /// Handle backspace for new task - not used in new design
    fn handle_backspace(&mut self) {
        // Input is now handled in the UI layer for task creation
    }

    /// Handle Shift+Enter (add new line) - not used in new design
    fn handle_shift_enter(&mut self) {
        // Input is now handled in the UI layer for task creation
    }

    /// Handle Enter (launch task) - not used in new design
    fn handle_enter(&mut self) {
        // Input is now handled in the UI layer for task creation
    }

    /// Handle Escape (clear input) - not used in new design
    fn handle_escape(&mut self) {
        // Input is now handled in the UI layer for task creation
    }

    /// Load initial data by triggering REST calls
    /// This would typically be called once at startup
    pub async fn load_initial_data(&mut self) -> Result<(), String> {
        // For now, we'll just use sample data
        self.state.tasks = crate::task::create_sample_tasks();
        Ok(())
    }
}
