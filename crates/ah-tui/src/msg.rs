//! Messages that drive the TUI state machine
//!
//! All external stimuli are funneled into these typed messages
//! that are consumed by the Model's update(msg) method.

use ah_rest_api_contract::SessionEvent;
use crossterm::event::{Event as CrosstermEvent, KeyEvent};

/// Messages that can be sent to the TUI state machine
#[derive(Debug, Clone)]
pub enum Msg {
    /// Keyboard input event
    Key(KeyEvent),
    /// Time tick event (driven by fake time in tests)
    Tick,
    /// Network event (REST results and SSE streams)
    Net(NetMsg),
    /// Quit the application
    Quit,
}

/// Network-related messages
#[derive(Debug, Clone)]
pub enum NetMsg {
    /// Server-sent event from SSE stream
    Sse(SessionEvent),
    /// REST API response (success)
    RestSuccess(RestSuccessMsg),
    /// REST API error
    RestError(String),
}

/// Successful REST API responses
#[derive(Debug, Clone)]
pub enum RestSuccessMsg {
    /// Projects list response
    ProjectsList(Vec<ah_rest_api_contract::Project>),
    /// Repositories list response
    RepositoriesList(Vec<ah_rest_api_contract::Repository>),
    /// Agents list response
    AgentsList(Vec<ah_rest_api_contract::AgentCapability>),
    /// Task creation response
    TaskCreated(ah_rest_api_contract::CreateTaskResponse),
}

/// Convert crossterm events to our Msg types
impl From<CrosstermEvent> for Msg {
    fn from(event: CrosstermEvent) -> Self {
        match event {
            CrosstermEvent::Key(key_event) => Msg::Key(key_event),
            // Other event types could be mapped here if needed
            _ => Msg::Tick, // Default to Tick for now
        }
    }
}
