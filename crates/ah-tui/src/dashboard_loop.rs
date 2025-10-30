// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Dashboard Loop - Main application event loop and rendering
//!
//! This module contains the main event loop and rendering logic for the TUI dashboard.
//! It handles user input, updates the view model, and renders the UI.
//! Dependencies are injected, making this module independent of specific service implementations.

use crossbeam_channel as chan;
use crossterm::event::{Event, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::{
    sync::Arc,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};
use tracing::{debug, trace};

use crate::{
    Theme, ViewCache, WorkspaceFilesEnumerator,
    settings::Settings,
    terminal::{self, TerminalConfig},
    view::{self, HitTestRegistry, TuiDependencies, header, modals},
    view_model::{MouseAction, Msg as ViewModelMsg, ViewModel},
};
use ah_core::TaskManager;
use ah_workflows::WorkspaceWorkflowsEnumerator;

/// Run the dashboard application with injected dependencies
pub async fn run_dashboard(deps: TuiDependencies) -> Result<(), Box<dyn std::error::Error>> {
    // Install signal handler for graceful shutdown
    let running = Arc::new(AtomicBool::new(true));

    // Setup terminal with signal handlers
    terminal::setup_terminal(TerminalConfig::default().with_running_flag(running.clone()))?;
    let mut terminal = Terminal::new(CrosstermBackend::new(std::io::stdout()))?;

    // Initialize MVVM components with injected dependencies
    let mut view_model = ViewModel::new_with_background_loading_and_current_repo(
        deps.workspace_files,
        deps.workspace_workflows,
        deps.task_manager,
        deps.repositories_enumerator,
        deps.branches_enumerator,
        deps.settings,
        deps.current_repository,
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
                        // Debug logging for key events
                        debug!(
                            key_code = ?key.code,
                            modifiers = ?key.modifiers,
                            key_kind = ?key.kind,
                            focus_element = ?view_model.focus_element,
                            "Key event received in dashboard"
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
    terminal::cleanup_terminal();

    Ok(())
}
