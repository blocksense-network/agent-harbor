// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Agent session loop with dependency injection.
//!
//! This module is the glue between the existing session viewer (vt100 playback)
//! and the new Agent Activity mock mode introduced for ACP milestone 0.5. The
//! loop is constructed from `AgentSessionDependencies`, mirroring the DI style
//! used by the dashboard. Tests and mock-agent simulations can now drive the
//! viewer without reaching into internal constructors.

use crate::session_viewer_deps::{AgentSessionDependencies, AgentSessionUiMode};
use crate::view::Theme;
use crate::view::agent_session_view::render_agent_session;
use crate::view::hit_test::HitTestRegistry;
use crate::view_model::agent_session_model::AgentSessionMouseAction;
use crate::view_model::agent_session_model::{AgentSessionMsg, AgentSessionViewModel};
use crate::view_model::task_execution::AgentActivityRow;
use crate::viewer::{ViewerConfig, ViewerEventLoop, build_session_viewer_view_model};
use ah_core::TaskManager;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{Duration, Instant};

/// Unified loop message used by the Agent Activity loop (input, ticks, scenario playback).
#[derive(Debug)]
pub(crate) enum LoopMsg {
    Input(crossterm::event::Event),
    Tick,
    Activity(AgentActivityRow),
}

/// Spawn an async scheduler that emits activity rows at their target timestamps (milliseconds).
pub(crate) fn spawn_activity_scheduler(
    mut activity: Vec<(u64, AgentActivityRow)>,
    tx: mpsc::UnboundedSender<LoopMsg>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let start = Instant::now();
        activity.sort_by_key(|(t, _)| *t);
        for (at_ms, row) in activity.into_iter() {
            let deadline = start + Duration::from_millis(at_ms);
            tokio::time::sleep_until(deadline).await;
            if tx.send(LoopMsg::Activity(row)).is_err() {
                break;
            }
        }
    })
}

/// Run an agent session UI (session viewer or Agent Activity mock).
pub async fn run_session_viewer(deps: AgentSessionDependencies) -> anyhow::Result<()> {
    match deps.ui_mode {
        AgentSessionUiMode::SessionViewer => {
            let vm = build_session_viewer_view_model(
                deps.recording_terminal_state.clone(),
                &deps.viewer_config,
                deps.autocomplete.clone(),
                &deps.theme,
            );
            let mut loop_state = ViewerEventLoop::new_with_config(
                vm,
                deps.viewer_config.clone(),
                deps.task_manager,
                deps.terminal_config,
                deps.theme.clone(),
            )?;
            loop_state.run().await?;
            Ok(())
        }
        AgentSessionUiMode::AgentActivity => {
            run_agent_activity_loop(
                deps.viewer_config.clone(),
                deps.task_manager,
                deps.activity_entries,
                deps.terminal_config,
                deps.settings.clone(),
                deps.autocomplete.clone(),
                deps.theme.clone(),
                deps.live_activity_rx,
                deps.prompt_tx,
            )
            .await
        }
    }
}

/// Build a fresh Agent Activity view model using the DI-provided viewer settings.
pub fn build_agent_activity_view_model(
    viewer_config: &ViewerConfig,
    settings: crate::settings::Settings,
    autocomplete: Option<Arc<crate::view_model::autocomplete::AutocompleteDependencies>>,
    theme: Theme,
) -> AgentSessionViewModel {
    AgentSessionViewModel::new(
        "mock-agent session",
        Vec::new(),
        viewer_config.terminal_rows.saturating_sub(4) as usize,
        settings,
        autocomplete,
        theme,
    )
}

/// Simplified loop for the Agent Activity mock UI.
#[allow(clippy::too_many_arguments)]
async fn run_agent_activity_loop(
    viewer_config: ViewerConfig,
    _task_manager: Arc<dyn TaskManager>,
    activity: Vec<(u64, crate::view_model::task_execution::AgentActivityRow)>,
    terminal_config: crate::terminal::TerminalConfig,
    settings: crate::settings::Settings,
    autocomplete: Option<Arc<crate::view_model::autocomplete::AutocompleteDependencies>>,
    theme: Theme,
    mut live_activity_rx: Option<
        tokio::sync::mpsc::UnboundedReceiver<crate::view_model::task_execution::AgentActivityRow>,
    >,
    prompt_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>,
) -> anyhow::Result<()> {
    crate::terminal::setup_terminal(terminal_config)
        .map_err(|e| anyhow::anyhow!("failed to setup terminal: {e}"))?;
    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend)?;
    let mut hit_registry: HitTestRegistry<AgentSessionMouseAction> = HitTestRegistry::new();

    let mut view_model =
        build_agent_activity_view_model(&viewer_config, settings, autocomplete, theme.clone());

    let (tx, mut rx) = mpsc::unbounded_channel::<LoopMsg>();

    // Input reader lives on a dedicated thread and forwards raw events.
    let tx_input = tx.clone();
    std::thread::spawn(move || {
        while let Ok(ev) = crossterm::event::read() {
            if tx_input.send(LoopMsg::Input(ev)).is_err() {
                break;
            }
        }
    });

    // 60 FPS ticker for animations and steady redraw cadence.
    let tx_tick = tx.clone();
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_millis(16));
        loop {
            ticker.tick().await;
            if tx_tick.send(LoopMsg::Tick).is_err() {
                break;
            }
        }
    });

    // Scenario playback: schedule activity rows at their timestamps.
    let tx_activity = tx.clone();
    spawn_activity_scheduler(activity, tx_activity);

    // Optional live activity stream: forward rows into the loop.
    if let Some(mut rx_live) = live_activity_rx.take() {
        let tx_live = tx.clone();
        tokio::spawn(async move {
            while let Some(row) = rx_live.recv().await {
                if tx_live.send(LoopMsg::Activity(row)).is_err() {
                    break;
                }
            }
        });
    }

    let mut needs_redraw = true;

    while let Some(msg) = rx.recv().await {
        match msg {
            LoopMsg::Activity(row) => {
                view_model.push_row(row);
                needs_redraw = true;
            }
            LoopMsg::Tick => {
                // Drive animations/elapsed timers even if no input.
                needs_redraw = true;
            }
            LoopMsg::Input(ev) => match ev {
                crossterm::event::Event::Key(key) => {
                    if let Some(msg) = view_model.handle_key_with_minor_modes(key) {
                        match msg {
                            AgentSessionMsg::QuitRequested => break,
                            AgentSessionMsg::RedrawRequested => needs_redraw = true,
                            AgentSessionMsg::TaskEntry(_) => {
                                if let Some(tx) = prompt_tx.as_ref() {
                                    let text =
                                        view_model.task_entry().description.lines().join("\n");
                                    let text = text.trim();
                                    if !text.is_empty() {
                                        let _ = tx.send(text.to_string());
                                    }
                                }
                                needs_redraw = true
                            }
                            AgentSessionMsg::ActivateControl { .. } => needs_redraw = true,
                            AgentSessionMsg::ForkAt(_) => needs_redraw = true,
                        }
                    }
                }
                crossterm::event::Event::Mouse(mouse) => {
                    if let Some(hit) = hit_registry.hit_test(mouse.column, mouse.row) {
                        if let Some(msg) = view_model.handle_mouse_action(hit.action) {
                            match msg {
                                AgentSessionMsg::QuitRequested => break,
                                _ => needs_redraw = true,
                            }
                        }
                    }
                }
                _ => {}
            },
        }

        if needs_redraw {
            needs_redraw = false;
            terminal.draw(|f| {
                render_agent_session(f, &view_model, &theme, Some(&mut hit_registry));
            })?;
        }
    }

    crate::terminal::cleanup_terminal();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time;

    #[tokio::test(start_paused = true)]
    async fn activity_scheduler_respects_timestamps_and_order() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let _handle = spawn_activity_scheduler(
            vec![
                (
                    30,
                    AgentActivityRow::AgentThought {
                        thought: "late".into(),
                    },
                ),
                (
                    10,
                    AgentActivityRow::AgentThought {
                        thought: "early".into(),
                    },
                ),
            ],
            tx,
        );

        // Before first deadline, no messages.
        time::advance(Duration::from_millis(9)).await;
        assert!(rx.try_recv().is_err(), "no activity before first deadline");

        // After 10ms we should receive the first (earliest) row.
        time::advance(Duration::from_millis(2)).await;
        let first = rx.recv().await.expect("first activity");
        match first {
            LoopMsg::Activity(AgentActivityRow::AgentThought { thought }) => {
                assert_eq!(thought, "early");
            }
            other => panic!("unexpected message: {other:?}"),
        }

        // Advance to 30ms to receive the second row.
        time::advance(Duration::from_millis(20)).await;
        let second = rx.recv().await.expect("second activity");
        match second {
            LoopMsg::Activity(AgentActivityRow::AgentThought { thought }) => {
                assert_eq!(thought, "late");
            }
            other => panic!("unexpected message: {other:?}"),
        }
    }
}
