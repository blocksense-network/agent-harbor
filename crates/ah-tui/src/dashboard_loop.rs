// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Dashboard Loop - Main application event loop and rendering
//!
//! This module contains the main event loop and rendering logic for the TUI dashboard.
//! It handles user input, updates the view model, and renders the UI.
//! Dependencies are injected, making this module independent of specific service implementations.

use crossbeam_channel as chan;
use crossterm::event::{Event, MouseButton, MouseEventKind};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::{
    sync::Arc,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};
use tracing::debug;

use crate::{
    Theme, ViewCache,
    terminal::{self, TerminalConfig},
    view::{self, HitTestRegistry, TuiDependencies, header, modals},
    view_model::{MouseAction, Msg as ViewModelMsg, ViewModel},
};

/// Run the dashboard application with injected dependencies
pub async fn run_dashboard(deps: TuiDependencies) -> Result<(), Box<dyn std::error::Error>> {
    // Install signal handler for graceful shutdown
    let running = Arc::new(AtomicBool::new(true));

    // Setup terminal with signal handlers
    terminal::setup_terminal(TerminalConfig::default().with_running_flag(running.clone()))?;
    let mut terminal = Terminal::new(CrosstermBackend::new(std::io::stdout()))?;

    // Initialize MVVM components with injected dependencies
    let (tx_ui, rx_ui) = chan::unbounded::<ViewModelMsg>();

    let mut view_model = ViewModel::new_with_background_loading_and_current_repo(
        deps.workspace_files,
        deps.workspace_workflows,
        deps.workspace_terms,
        deps.task_manager,
        deps.repositories_enumerator,
        deps.branches_enumerator,
        deps.settings,
        deps.current_repository,
        tx_ui.clone(),
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
            recv(rx_ui) -> msg => {
                let msg = match msg {
                    Ok(m) => m,
                    Err(_) => break,
                };

                if let Err(error) = view_model.update(msg) {
                    eprintln!("Error handling UI message: {}", error);
                }

                if view_model.take_exit_request() {
                    break;
                }

                refresh_ui(&mut view_model, &mut terminal, &mut view_cache, &mut hit_registry, &theme)?;
            }
            recv(rx_ev) -> msg => {
                let event = match msg {
                    Ok(e) => e,
                    Err(_) => break,
                };

                match event {
                    Event::Key(key) => {
                        debug!(
                            key_code = ?key.code,
                            modifiers = ?key.modifiers,
                            key_kind = ?key.kind,
                            focus_element = ?view_model.focus_element,
                            "Key event received in dashboard"
                        );

                        if let Err(error) = view_model.update(ViewModelMsg::Key(key)) {
                            eprintln!("Error handling key event: {}", error);
                        }

                        if view_model.take_exit_request() {
                            break;
                        }

                        refresh_ui(&mut view_model, &mut terminal, &mut view_cache, &mut hit_registry, &theme)?;
                    }
                    Event::Mouse(mouse_event) => {
                        // Skip mouse events if mouse support is disabled
                        if !view_model.settings.mouse_enabled() {
                            continue;
                        }

                        let mut handled = false;
                        match mouse_event.kind {
                            MouseEventKind::Down(MouseButton::Left) => {
                                debug!("Mouse click at ({}, {})", mouse_event.column, mouse_event.row);
                                if let Some(hit) = hit_registry.hit_test(mouse_event.column, mouse_event.row) {
                                    debug!("Hit test found action: {:?} in bounds {:?}", hit.action, hit.rect);
                                    if let Err(error) = view_model.update(ViewModelMsg::MouseClick {
                                        action: hit.action,
                                        column: mouse_event.column,
                                        row: mouse_event.row,
                                        bounds: hit.rect,
                                    }) {
                                        eprintln!("Error handling mouse click: {}", error);
                                    }
                                    handled = true;
                                } else {
                                    debug!("No hit found at ({}, {})", mouse_event.column, mouse_event.row);
                                }
                            }
                            MouseEventKind::ScrollUp => {
                                if let Err(error) = view_model.update(ViewModelMsg::MouseScrollUp) {
                                    eprintln!("Error handling mouse scroll up: {}", error);
                                }
                                handled = true;
                            }
                            MouseEventKind::ScrollDown => {
                                if let Err(error) = view_model.update(ViewModelMsg::MouseScrollDown) {
                                    eprintln!("Error handling mouse scroll down: {}", error);
                                }
                                handled = true;
                            }
                            _ => {}
                        }

                        if handled {
                            if view_model.take_exit_request() {
                                break;
                            }

                            refresh_ui(&mut view_model, &mut terminal, &mut view_cache, &mut hit_registry, &theme)?;
                        }
                    }
                    Event::Resize(_width, _height) => {
                        let _ = terminal.autoresize();
                        view_model.needs_redraw = true;
                        refresh_ui(&mut view_model, &mut terminal, &mut view_cache, &mut hit_registry, &theme)?;
                    }
                    _ => {}
                }
            }
            recv(rx_tick) -> _ => {
                if let Err(error) = view_model.update(ViewModelMsg::Tick) {
                    eprintln!("Error handling tick event: {}", error);
                }

                if view_model.take_exit_request() {
                    break;
                }

                refresh_ui(&mut view_model, &mut terminal, &mut view_cache, &mut hit_registry, &theme)?;
            }
        }
    }

    // Ensure cleanup happens
    terminal::cleanup_terminal();

    Ok(())
}

fn refresh_ui(
    view_model: &mut ViewModel,
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    view_cache: &mut ViewCache,
    hit_registry: &mut HitTestRegistry<MouseAction>,
    theme: &Theme,
) -> Result<(), Box<dyn std::error::Error>> {
    view_model.process_pending_task_events();
    view_model.update_footer();

    if view_model.needs_redraw {
        terminal.draw(|frame| {
            let size = frame.area();
            view::render(frame, view_model, view_cache, hit_registry);
            modals::render_modals(frame, view_model, size, theme, hit_registry);
        })?;
        view_model.needs_redraw = false;
    }

    Ok(())
}
