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
use std::sync::Arc;
use tracing::debug;

use crate::{
    Settings, Theme, ViewCache,
    tui_runtime::{self, UiMsg},
    view::{self, HitTestRegistry, TuiDependencies, header, modals},
    view_model::{MouseAction, Msg as ViewModelMsg, ViewModel, input::InputState},
};

/// Dashboard-specific state that needs to persist across event loop iterations
struct DashboardState {
    view_model: ViewModel,
    view_cache: ViewCache,
    hit_registry: HitTestRegistry<MouseAction>,
    theme: Theme,
}

/// Run the dashboard application with injected dependencies
pub fn run_dashboard(deps: TuiDependencies) -> Result<(), anyhow::Error> {
    // Run the dashboard using the shared TUI runtime
    // Pass the original deps to preserve dependency injection pattern
    tui_runtime::run_tui_with_single_tokio_thread::<ViewModelMsg, _, _>(
        deps,
        move |deps, rx_ui, tx_ui, rx_tick, terminal, input_state| async move {
            // Initialize MVVM components with injected dependencies
            // Create a wrapper sender that converts UiMsg to Msg
            let (view_model_tx, view_model_rx) = chan::unbounded::<ViewModelMsg>();
            let ui_tx_clone = tx_ui.clone();

            // Spawn a task to forward messages from view model to UI
            tokio::spawn(async move {
                while let Ok(msg) = view_model_rx.recv() {
                    let _ = ui_tx_clone.send(UiMsg::AppMsg(msg));
                }
            });

            let mut view_model = ViewModel::new_with_background_loading_and_current_repo(
                deps.workspace_files.clone(),
                deps.workspace_workflows.clone(),
                deps.workspace_terms.clone(),
                deps.task_manager.clone(),
                deps.repositories_enumerator.clone(),
                deps.branches_enumerator.clone(),
                deps.settings.clone(),
                deps.current_repository.clone(),
                view_model_tx.clone(),
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

            debug!("Loading initial tasks");
            // Load initial tasks to populate the UI
            view_model.load_initial_tasks().await.map_err(anyhow::Error::msg)?;

            debug!("Loaded initial tasks");
            // Create dashboard state that will be moved into the event loop
            let dashboard_state = DashboardState {
                view_model,
                view_cache,
                hit_registry,
                theme,
            };

            run_dashboard_event_loop(
                dashboard_state,
                rx_ui,
                tx_ui,
                rx_tick,
                terminal,
                input_state,
            )
            .await
        },
    )
}

/// Run the dashboard event loop using the shared TUI runtime infrastructure
async fn run_dashboard_event_loop(
    mut dashboard_state: DashboardState,
    mut rx_ui: chan::Receiver<UiMsg<ViewModelMsg>>,
    _tx_ui: chan::Sender<UiMsg<ViewModelMsg>>,
    mut rx_tick: chan::Receiver<std::time::Instant>,
    mut terminal: Terminal<CrosstermBackend<std::io::Stdout>>,
    mut input_state: InputState,
) -> Result<(), anyhow::Error> {
    loop {
        debug!("Running dashboard event loop");
        // Use biased select to prefer UI messages over ticks (matching original pattern)
        chan::select_biased! {
            recv(rx_ui) -> ui_msg => {
                let ui_msg = match ui_msg {
                    Ok(msg) => msg,
                    Err(_) => break,
                };

                match ui_msg {
                    UiMsg::UserInput(event) => {
                        handle_user_input_event(
                            &mut dashboard_state,
                            &mut input_state,
                            &mut terminal,
                            event,
                        ).await?;
                    }
                    UiMsg::Tick => {
                        handle_tick_event(&mut dashboard_state, &mut terminal).await?;
                    }
                    UiMsg::AppMsg(view_model_msg) => {
                        handle_view_model_message(
                            &mut dashboard_state,
                            &mut terminal,
                            view_model_msg,
                        ).await?;
                    }
                }

                if dashboard_state.view_model.take_exit_request() {
                    break;
                }
            }
            recv(rx_tick) -> _ => {
                handle_tick_event(&mut dashboard_state, &mut terminal).await?;

                if dashboard_state.view_model.take_exit_request() {
                    break;
                }
            }
        }
    }

    Ok(())
}

/// Handle user input events (keyboard, mouse, resize)
async fn handle_user_input_event(
    dashboard_state: &mut DashboardState,
    input_state: &mut InputState,
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    event: Event,
) -> Result<(), anyhow::Error> {
    match event {
        Event::Key(key) => {
            // Update input state tracking
            input_state.update(&key);

            // Preprocess key event to handle SHIFT+ENTER -> CTRL+J translation
            let processed_key = input_state.preprocess_key_event(key);

            debug!(
                original_key_code = ?key.code,
                original_modifiers = ?key.modifiers,
                processed_key_code = ?processed_key.code,
                processed_modifiers = ?processed_key.modifiers,
                key_kind = ?processed_key.kind,
                focus_element = ?dashboard_state.view_model.focus_element,
                "Key event received in dashboard"
            );

            if let Err(error) = dashboard_state.view_model.update(ViewModelMsg::Key(processed_key))
            {
                eprintln!("Error handling key event: {}", error);
            }

            refresh_ui(
                &mut dashboard_state.view_model,
                terminal,
                &mut dashboard_state.view_cache,
                &mut dashboard_state.hit_registry,
                &dashboard_state.theme,
            )?;
        }
        Event::Mouse(mouse_event) => {
            // Skip mouse events if mouse support is disabled
            if !dashboard_state.view_model.settings.mouse_enabled() {
                return Ok(());
            }

            let mut handled = false;
            match mouse_event.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    debug!(
                        "Mouse click at ({}, {})",
                        mouse_event.column, mouse_event.row
                    );
                    if let Some(hit) =
                        dashboard_state.hit_registry.hit_test(mouse_event.column, mouse_event.row)
                    {
                        debug!(
                            "Hit test found action: {:?} in bounds {:?}",
                            hit.action, hit.rect
                        );
                        if let Err(error) =
                            dashboard_state.view_model.update(ViewModelMsg::MouseClick {
                                action: hit.action,
                                column: mouse_event.column,
                                row: mouse_event.row,
                                bounds: hit.rect,
                            })
                        {
                            eprintln!("Error handling mouse click: {}", error);
                        }
                        handled = true;
                    } else {
                        debug!(
                            "No hit found at ({}, {})",
                            mouse_event.column, mouse_event.row
                        );
                    }
                }
                MouseEventKind::Drag(MouseButton::Left) => {
                    debug!(
                        "Mouse drag at ({}, {})",
                        mouse_event.column, mouse_event.row
                    );
                    // For drag events, we send MouseDrag messages regardless of hit test
                    // since dragging can continue outside the original hit area
                    if let Err(error) = dashboard_state.view_model.update(ViewModelMsg::MouseDrag {
                        column: mouse_event.column,
                        row: mouse_event.row,
                        bounds: dashboard_state.view_model.last_textarea_area.unwrap_or_default(),
                    }) {
                        eprintln!("Error handling mouse drag: {}", error);
                    }
                    handled = true;
                }
                MouseEventKind::Up(MouseButton::Left) => {
                    debug!("Mouse up at ({}, {})", mouse_event.column, mouse_event.row);
                    if let Err(error) = dashboard_state.view_model.update(ViewModelMsg::MouseUp {
                        column: mouse_event.column,
                        row: mouse_event.row,
                    }) {
                        eprintln!("Error handling mouse up: {}", error);
                    }
                    handled = true;
                }
                MouseEventKind::ScrollUp => {
                    if let Err(error) =
                        dashboard_state.view_model.update(ViewModelMsg::MouseScrollUp)
                    {
                        eprintln!("Error handling mouse scroll up: {}", error);
                    }
                    handled = true;
                }
                MouseEventKind::ScrollDown => {
                    if let Err(error) =
                        dashboard_state.view_model.update(ViewModelMsg::MouseScrollDown)
                    {
                        eprintln!("Error handling mouse scroll down: {}", error);
                    }
                    handled = true;
                }
                _ => {}
            }

            if handled {
                refresh_ui(
                    &mut dashboard_state.view_model,
                    terminal,
                    &mut dashboard_state.view_cache,
                    &mut dashboard_state.hit_registry,
                    &dashboard_state.theme,
                )?;
            }
        }
        Event::Resize(_width, _height) => {
            let _ = terminal.autoresize();
            dashboard_state.view_model.needs_redraw = true;
            refresh_ui(
                &mut dashboard_state.view_model,
                terminal,
                &mut dashboard_state.view_cache,
                &mut dashboard_state.hit_registry,
                &dashboard_state.theme,
            )?;
        }
        _ => {}
    }

    Ok(())
}

/// Handle tick events
async fn handle_tick_event(
    dashboard_state: &mut DashboardState,
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
) -> Result<(), anyhow::Error> {
    if let Err(error) = dashboard_state.view_model.update(ViewModelMsg::Tick) {
        eprintln!("Error handling tick event: {}", error);
    }

    refresh_ui(
        &mut dashboard_state.view_model,
        terminal,
        &mut dashboard_state.view_cache,
        &mut dashboard_state.hit_registry,
        &dashboard_state.theme,
    )?;

    Ok(())
}

/// Handle view model messages sent from background tasks
async fn handle_view_model_message(
    dashboard_state: &mut DashboardState,
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    msg: ViewModelMsg,
) -> Result<(), anyhow::Error> {
    if let Err(error) = dashboard_state.view_model.update(msg) {
        eprintln!("Error handling UI message: {}", error);
    }

    refresh_ui(
        &mut dashboard_state.view_model,
        terminal,
        &mut dashboard_state.view_cache,
        &mut dashboard_state.hit_registry,
        &dashboard_state.theme,
    )?;

    Ok(())
}

fn refresh_ui(
    view_model: &mut ViewModel,
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    view_cache: &mut ViewCache,
    hit_registry: &mut HitTestRegistry<MouseAction>,
    theme: &Theme,
) -> Result<(), anyhow::Error> {
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
