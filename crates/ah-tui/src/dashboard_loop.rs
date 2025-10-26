// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Dashboard Loop - Main application event loop and rendering
//!
//! This module contains the main event loop and rendering logic for the TUI dashboard.
//! It handles user input, updates the view model, and renders the UI.
//! Dependencies are injected, making this module independent of specific service implementations.

use crossbeam_channel as chan;
use crossterm::{
    ExecutableCommand,
    event::{
        DisableMouseCapture, EnableMouseCapture, KeyboardEnhancementFlags,
        PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    event::{Event, KeyEventKind, MouseButton, MouseEventKind},
    terminal::{EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::{
    io::{self, Stdout},
    panic,
    sync::Arc,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};
use tracing::trace;

// External dependencies
use ctrlc;

// Global flag to ensure cleanup only happens once
static CLEANUP_DONE: AtomicBool = AtomicBool::new(false);

// Track what we modified so we can restore properly
static RAW_MODE_ENABLED: AtomicBool = AtomicBool::new(false);
static ALTERNATE_SCREEN_ACTIVE: AtomicBool = AtomicBool::new(false);
static KB_FLAGS_PUSHED: AtomicBool = AtomicBool::new(false);
static MOUSE_CAPTURE_ENABLED: AtomicBool = AtomicBool::new(false);

use crate::{
    Theme, ViewCache, WorkspaceFilesEnumerator,
    settings::Settings,
    view::{self, HitTestRegistry, header, modals},
    view_model::{MouseAction, Msg as ViewModelMsg, ViewModel},
};
use ah_core::TaskManager;
use ah_workflows::WorkspaceWorkflowsEnumerator;

/// Dashboard dependencies that are injected
pub struct DashboardDependencies {
    pub workspace_files: Arc<dyn WorkspaceFilesEnumerator>,
    pub workspace_workflows: Arc<dyn WorkspaceWorkflowsEnumerator>,
    pub task_manager: Arc<dyn TaskManager>,
    pub settings: Settings,
}

/// Setup terminal for TUI
fn setup_terminal() -> Result<(), Box<dyn std::error::Error>> {
    let mut stdout = io::stdout();

    crossterm::terminal::enable_raw_mode()?;
    RAW_MODE_ENABLED.store(true, Ordering::SeqCst);

    stdout.execute(EnterAlternateScreen)?;
    ALTERNATE_SCREEN_ACTIVE.store(true, Ordering::SeqCst);

    // Setup enhanced keyboard and mouse support for better image rendering
    stdout.execute(PushKeyboardEnhancementFlags(
        KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
            | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
            | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
            | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS,
    ))?;
    KB_FLAGS_PUSHED.store(true, Ordering::SeqCst);

    stdout.execute(EnableMouseCapture)?;
    MOUSE_CAPTURE_ENABLED.store(true, Ordering::SeqCst);

    Ok(())
}

/// Cleanup terminal after TUI
fn cleanup_terminal() {
    if CLEANUP_DONE.swap(true, Ordering::SeqCst) {
        return; // Already cleaned up
    }

    let mut stdout = io::stdout();

    // Pop keyboard enhancement flags first (must be done while still in raw mode/alternate screen)
    if KB_FLAGS_PUSHED.load(Ordering::SeqCst) {
        let _ = stdout.execute(PopKeyboardEnhancementFlags);
        KB_FLAGS_PUSHED.store(false, Ordering::SeqCst);
    }

    if MOUSE_CAPTURE_ENABLED.load(Ordering::SeqCst) {
        let _ = stdout.execute(DisableMouseCapture);
        MOUSE_CAPTURE_ENABLED.store(false, Ordering::SeqCst);
    }

    // Disable raw mode next
    if RAW_MODE_ENABLED.load(Ordering::SeqCst) {
        let _ = crossterm::terminal::disable_raw_mode();
        RAW_MODE_ENABLED.store(false, Ordering::SeqCst);
    }

    // Leave alternate screen last
    if ALTERNATE_SCREEN_ACTIVE.load(Ordering::SeqCst) {
        let _ = stdout.execute(LeaveAlternateScreen);
        ALTERNATE_SCREEN_ACTIVE.store(false, Ordering::SeqCst);
    }
}

/// Run the dashboard application with injected dependencies
pub async fn run_dashboard(deps: DashboardDependencies) -> Result<(), Box<dyn std::error::Error>> {
    // Setup terminal
    setup_terminal()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(std::io::stdout()))?;

    // Install signal handler for graceful shutdown
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        cleanup_terminal();
        r.store(false, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");

    // Install panic hook for cleanup on panic
    let default_panic = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        cleanup_terminal();
        default_panic(panic_info);
    }));

    // Initialize MVVM components with injected dependencies
    let mut view_model = ViewModel::new_with_background_loading(
        deps.workspace_files,
        deps.workspace_workflows,
        deps.task_manager,
        deps.settings,
    );

    // Start background loading of workspace data
    view_model.start_background_loading();

    // Initialize view cache with image rendering components
    let theme = Theme::default();
    let (picker, logo_protocol) = header::initialize_logo_rendering(theme.bg);
    let mut view_cache = ViewCache::new();
    view_cache.picker = picker;
    view_cache.logo_protocol = logo_protocol;
    let mut hit_registry: HitTestRegistry<MouseAction> = HitTestRegistry::new();

    // Load initial tasks to populate the UI
    view_model.load_initial_tasks().await?;

    // Create channels for event handling
    let (tx_ev, rx_ev) = chan::unbounded::<Event>();

    // Use coalescing tick channel that never builds a backlog
    let rx_tick = chan::tick(Duration::from_millis(16));

    // Event reader thread
    thread::spawn(move || {
        while let Ok(ev) = crossterm::event::read() {
            // Send event to main thread
            let _ = tx_ev.send(ev);
        }
    });

    // Main event loop
    loop {
        // Check if we should exit due to interrupt signal
        if !running.load(Ordering::SeqCst) {
            break;
        }

        // Use biased select to prefer input events over ticks
        chan::select_biased! {
            recv(rx_ev) -> msg => {
                let event = match msg {
                    Ok(e) => e,
                    Err(_) => break,
                };

                match event {
                    Event::Key(key) => {
                        // Key logging for debugging (trace level, disabled by default)
                        trace!(
                            key_code = ?key.code,
                            modifiers = ?key.modifiers,
                            key_kind = ?key.kind,
                            focus_element = ?view_model.focus_element,
                            "Key event received"
                        );

                        // Send key event to ViewModel via message system
                        let msg = ViewModelMsg::Key(key);
                        if let Err(error) = view_model.update(msg) {
                            eprintln!("Error handling key event: {}", error);
                        }
                        if view_model.take_exit_request() {
                            break;
                        }
                    }
                    Event::Mouse(mouse_event) => {
                        match mouse_event.kind {
                            MouseEventKind::Down(MouseButton::Left) => {
                                if let Some(hit) = hit_registry.hit_test(mouse_event.column, mouse_event.row) {
                                    let msg = ViewModelMsg::MouseClick {
                                        action: hit.action,
                                        column: mouse_event.column,
                                        row: mouse_event.row,
                                        bounds: hit.rect,
                                    };
                                    if let Err(error) = view_model.update(msg) {
                                        eprintln!("Error handling mouse click: {}", error);
                                    }
                                }
                            }
                            MouseEventKind::ScrollUp => {
                                let _ = view_model.update(ViewModelMsg::MouseScrollUp);
                            }
                            MouseEventKind::ScrollDown => {
                                let _ = view_model.update(ViewModelMsg::MouseScrollDown);
                            }
                            _ => {}
                        }

                        if view_model.take_exit_request() {
                            break;
                        }
                    }
                    Event::Resize(_width, _height) => {
                        // Handle resize events if needed
                        let _ = terminal.autoresize();
                        view_model.needs_redraw = true; // Force redraw on resize
                    }
                    _ => {}
                }

                // Process any pending task events
                view_model.process_pending_task_events();

                // After handling an event, update footer and redraw if needed
                view_model.update_footer();
                if view_model.needs_redraw {
                    terminal.draw(|frame| {
                        let size = frame.area();
                        // Use full render for production
                        view::render(frame, &mut view_model, &mut view_cache, &mut hit_registry);

                        // Render modals on top of main UI
                        modals::render_modals(frame, &view_model, size, &theme);
                    })?;
                    view_model.needs_redraw = false;
                }
            }
            recv(rx_tick) -> _ => {
                // Handle tick events (activity simulation)
                let msg = ViewModelMsg::Tick;
                let _ = view_model.update(msg);

                // Only redraw if tick actually changed something
                if view_model.needs_redraw {
                    view_model.update_footer();
                    terminal.draw(|frame| {
                        let size = frame.area();
                        // Use full render for production
                        view::render(frame, &mut view_model, &mut view_cache, &mut hit_registry);

                        // Render modals on top of main UI
                        modals::render_modals(frame, &view_model, size, &theme);
                    })?;
                    view_model.needs_redraw = false;
                }
            }
        }
    }

    // Ensure cleanup happens
    cleanup_terminal();

    Ok(())
}
