// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared TUI Runtime - Threading setup and initialization
//!
//! This module provides shared initialization for TUI applications following
//! the threading model specified in TUI-Threading.md. It handles:
//! - Dedicated UI thread spawning
//! - Current-thread Tokio runtime with LocalSet
//! - Event reader thread setup
//! - Channel architecture setup
//! - 60 FPS ticker initialization

use crate::terminal::{self, TerminalConfig};
use crate::view::TuiDependencies;
use crate::view_model::input::InputState;
use crossbeam_channel as chan;
use crossterm::event::Event;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::thread;
use std::time::Duration;
use tokio::runtime::Builder;
use tokio::task::LocalSet;

/// Type alias for runtime results that can cross thread boundaries
type RuntimeResult<T> = Result<T, String>;

/// Custom error type that implements Send + Sync for thread boundaries
#[derive(Debug)]
pub struct TuiRuntimeError {
    message: String,
}

impl TuiRuntimeError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for TuiRuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for TuiRuntimeError {}

impl From<String> for TuiRuntimeError {
    fn from(message: String) -> Self {
        Self::new(message)
    }
}

impl From<&str> for TuiRuntimeError {
    fn from(message: &str) -> Self {
        Self::new(message)
    }
}

/// Result of TUI runtime initialization
pub struct TuiRuntime<AppMsg> {
    /// Handle to the UI thread (for joining)
    pub ui_thread: thread::JoinHandle<Result<(), Box<dyn std::error::Error>>>,
    /// Channel for sending UI messages from background tasks
    pub ui_sender: chan::Sender<UiMsg<AppMsg>>,
}

/// UI message types for the general UI thread inbox
/// Generic over application-specific message types
#[derive(Debug, Clone)]
pub enum UiMsg<AppMsg> {
    /// User input event from event reader thread
    UserInput(Event),
    /// Tick event from 60 FPS ticker
    Tick,
    /// Application-specific message
    AppMsg(AppMsg),
}

/// Initialize and run a TUI application with proper threading
///
/// This function sets up the threading model specified in TUI-Threading.md:
/// - Spawns a dedicated OS thread for UI operations
/// - Creates current-thread Tokio runtime with LocalSet for !Send futures
/// - Sets up event reader thread for user input
/// - Provides channel-based communication for UI mutations
/// - Passes coalescing tick receiver directly to UI loop (matching dashboard_loop.rs pattern)
/// - Runs UI loop within LocalSet context for !Send future spawning
/// - Handles full terminal lifecycle including signal handlers and panic hooks
/// - Creates terminal instance and input state (matching dashboard_loop.rs elaboration)
/// - Provides robust cleanup: signal handlers, panic hooks, and explicit cleanup on normal exit
///
/// # How Applications Use LocalSet
/// The UI loop function runs within the LocalSet context, allowing applications to spawn !Send futures:
/// ```rust
/// // Within the UI loop function, spawn LocalSet tasks for continuous UI updates
/// tokio::task::spawn_local(async move {
///     // This task can hold weak references to UI components
///     // Automatic cleanup when UI elements are dropped
///     while let Some(update) = stream.next().await {
///         if let Some(ui_component) = weak_ui.upgrade() {
///             ui_component.borrow_mut().update(update);
///         } else {
///             break; // UI element dropped, terminate task
///         }
///     }
/// });
/// ```
///
/// # Type Parameters
/// - `AppMsg`: Application-specific message type for the UI inbox
pub fn run_tui_with_single_tokio_thread<AppMsg, F, Fut>(
    deps: TuiDependencies,
    ui_loop_fn: F,
) -> Result<(), anyhow::Error>
where
    AppMsg: Send + 'static,
    F: FnOnce(
            TuiDependencies,
            chan::Receiver<UiMsg<AppMsg>>,
            chan::Sender<UiMsg<AppMsg>>,
            chan::Receiver<std::time::Instant>,
            Terminal<CrosstermBackend<std::io::Stdout>>,
            InputState,
        ) -> Fut
        + 'static,
    Fut: std::future::Future<Output = Result<(), anyhow::Error>> + 'static,
{
    // Create UI message channels
    let (tx_ui, rx_ui) = chan::unbounded::<UiMsg<AppMsg>>();

    // Create coalescing tick channel (never builds backlog) - matching dashboard_loop.rs
    let rx_tick = chan::tick(Duration::from_millis(16)); // ~60 FPS

    // Install signal handler for graceful shutdown (matching dashboard_loop.rs)
    let running = Arc::new(AtomicBool::new(true));

    // Track input state for key event preprocessing (matching dashboard_loop.rs)
    let input_state = InputState::new();

    // Create current-thread runtime
    let rt = Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to create tokio runtime: {}", e))?;

    // Run with LocalSet for !Send futures
    let local = LocalSet::new();

    // Spawn event reader thread
    let event_sender = tx_ui.clone();
    thread::spawn(move || {
        while let Ok(event) = crossterm::event::read() {
            // Send event to UI thread via UiMsg
            let _ = event_sender.send(UiMsg::UserInput(event));
        }
    });

    local.block_on(&rt, async {
        // Setup terminal with signal handlers (matching dashboard_loop.rs)
        terminal::setup_terminal(TerminalConfig::default().with_running_flag(running.clone()))
            .map_err(|e| anyhow::anyhow!("Failed to setup terminal: {}", e))?;

        // Create terminal instance (matching dashboard_loop.rs)
        let terminal = Terminal::new(CrosstermBackend::new(std::io::stdout()))
            .map_err(|e| anyhow::anyhow!("Failed to create terminal: {}", e))?;

        // Run the UI loop function provided by the consumer
        // Applications can spawn !Send futures using tokio::task::spawn_local() within this context
        let result = ui_loop_fn(deps, rx_ui, tx_ui.clone(), rx_tick, terminal, input_state).await;

        // Ensure cleanup happens (matching dashboard_loop.rs)
        // Signal handlers provide cleanup on abnormal termination, but normal exit needs explicit cleanup
        terminal::cleanup_terminal();

        result
    })
}

/// Helper to convert UiMsg to specific event types for backwards compatibility
pub fn handle_ui_msg<AppMsg, F>(
    msg: UiMsg<AppMsg>,
    mut event_handler: F,
    tick_handler: &mut dyn FnMut(),
) -> Result<(), Box<dyn std::error::Error>>
where
    F: FnMut(Event) -> Result<(), Box<dyn std::error::Error>>,
{
    match msg {
        UiMsg::UserInput(event) => {
            event_handler(event)?;
        }
        UiMsg::Tick => {
            tick_handler();
        }
        UiMsg::AppMsg(_) => {
            // Application-specific messages are handled by the consumer
        }
    }
    Ok(())
}

/// Convenience type aliases for common use cases
pub type DashboardUiMsg = UiMsg<crate::msg::Msg>;
pub type RecordUiMsg = UiMsg<crate::view_model::session_viewer_model::SessionViewerMsg>;
pub type ReplayUiMsg = UiMsg<crate::view_model::session_viewer_model::SessionViewerMsg>;
