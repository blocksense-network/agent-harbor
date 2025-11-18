// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

// Implementation of `ah agent replay` command
//
// Replays recorded agent sessions and displays the final terminal state,
// either by fast-forwarding through the recording or by simulating
// the original timing.

use crate::record::CliGutterPosition;
use crate::tui_runtime::{self, UiMsg};
use crate::view::TuiDependencies;
use crate::view_model::input::InputState;
use crate::view_model::session_viewer_model::{GutterConfig, GutterPosition, SessionViewerMsg};
use crate::viewer::{
    ViewerConfig, build_session_viewer_view_model, handle_mouse_click_for_view, render_view_frame,
    update_row_metadata_with_autofollow,
};
use ah_core::{AgentExecutionConfig, local_task_manager::GenericLocalTaskManager};
use ah_mux::TmuxMultiplexer;
use ah_recorder::replay_ahr_file;
use anyhow::{Context, Result};
use clap::Parser;
use crossbeam_channel as chan;
use crossterm::event::{Event, MouseButton, MouseEventKind};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::{debug, info};

/// UI message type for replay functionality
pub type ReplayUiMsg = UiMsg<SessionViewerMsg>;

/// Components needed to construct ReplayState (separated to avoid Send issues)
struct ReplayComponents {
    view_model: crate::view_model::session_viewer_model::SessionViewerViewModel,
    viewer_config: ViewerConfig,
    task_manager: Arc<dyn ah_core::TaskManager>,
}

/// State for the replay session that needs to persist across event loop iterations
struct ReplayState {
    view_model: crate::view_model::session_viewer_model::SessionViewerViewModel,
    viewer_config: ViewerConfig,
    task_manager: Arc<dyn ah_core::TaskManager>,
}

/// Convert CLI gutter position to viewer gutter config
fn cli_gutter_to_viewer_gutter(cli_pos: &CliGutterPosition) -> GutterConfig {
    let position = match cli_pos {
        CliGutterPosition::Left => GutterPosition::Left,
        CliGutterPosition::Right => GutterPosition::Right,
        CliGutterPosition::None => GutterPosition::None,
    };
    GutterConfig {
        position,
        show_line_numbers: false, // Disable line numbers by default
    }
}

/// Replay a recorded agent session
#[derive(Parser, Debug, Clone)]
pub struct ReplayArgs {
    /// Path to the .ahr recording file or session ID
    #[arg(value_name = "SESSION")]
    pub session: String,

    /// Fast-forward replay: immediately show final terminal state
    #[arg(long)]
    pub fast: bool,

    /// Interactive viewer mode: load recording into live viewer for testing
    #[arg(long)]
    pub viewer: bool,

    /// Don't emit ANSI color codes in output
    #[arg(long)]
    pub no_colors: bool,

    /// Print metadata and stats instead of rendering output
    #[arg(long)]
    pub print_meta: bool,

    /// Position of the snapshot indicator gutter column
    #[arg(long, default_value = "right", value_enum)]
    pub gutter: CliGutterPosition,
}

/// Execute the replay command
pub async fn execute(deps: crate::view::TuiDependencies, args: ReplayArgs) -> Result<()> {
    // For now, assume the session argument is a path to an .ahr file
    let ahr_path = PathBuf::from(&args.session);

    if !ahr_path.exists() {
        anyhow::bail!("Recording file not found: {}", args.session);
    }

    if args.print_meta {
        // Print metadata and stats (no TUI needed)
        print_metadata(&ahr_path).await?;
    } else if args.viewer {
        // Interactive viewer mode: load recording into live viewer
        run_viewer_mode(deps, &ahr_path, &args.gutter)?;
    } else if args.fast {
        // Fast-forward replay: immediately show final state (no TUI needed)
        fast_replay(&ahr_path, args.no_colors).await?;
    } else {
        // Timed replay: simulate original timing (not yet implemented)
        anyhow::bail!("Timed replay not yet implemented. Use --fast or --viewer for replay.");
    }

    Ok(())
}

/// Print metadata and statistics about the recording
async fn print_metadata(ahr_path: &PathBuf) -> Result<()> {
    let replay_result =
        replay_ahr_file(ahr_path).context("Failed to replay recording for metadata")?;

    println!("Recording: {}", ahr_path.display());
    println!(
        "Initial terminal size: {}x{}",
        replay_result.initial_cols, replay_result.initial_rows
    );
    println!("Total bytes processed: {}", replay_result.total_bytes);

    // Count snapshots from events
    let snapshot_count = replay_result
        .events
        .iter()
        .filter(|e| matches!(e, ah_recorder::AhrEvent::Snapshot { .. }))
        .count();
    println!("Snapshots: {}", snapshot_count);

    if snapshot_count > 0 {
        println!("\nSnapshots:");
        for event in &replay_result.events {
            if let ah_recorder::AhrEvent::Snapshot(snapshot) = event {
                let label = snapshot.label.as_deref().unwrap_or("<unnamed>");
                println!("  {}: {} (ts_ns {})", snapshot.ts_ns, label, snapshot.ts_ns);
            }
        }
    }

    Ok(())
}

/// Perform fast-forward replay and display final terminal state
async fn fast_replay(ahr_path: &PathBuf, no_colors: bool) -> Result<()> {
    let replay_result = replay_ahr_file(ahr_path).context("Failed to replay recording")?;

    // Create a TerminalState and replay all events to get the final state
    let mut terminal_state =
        ah_recorder::TerminalState::new(replay_result.initial_rows, replay_result.initial_cols);

    for event in &replay_result.events {
        match event {
            ah_recorder::AhrEvent::Data { data, .. } => {
                terminal_state.process_data(data);
            }
            ah_recorder::AhrEvent::Resize { cols, rows, .. } => {
                terminal_state.resize(*cols, *rows);
            }
            ah_recorder::AhrEvent::Snapshot { .. } => {
                // Snapshots don't affect the final terminal display
            }
        }
    }

    // Display the final terminal state
    for line_idx in 0..terminal_state.line_count() {
        let line_content = terminal_state.line_content(ah_recorder::InMemoryLineIndex(line_idx));
        if no_colors {
            // Strip ANSI sequences for plain text output
            let plain_text = strip_ansi_codes(&line_content);
            println!("{}", plain_text);
        } else {
            // Output with ANSI colors preserved
            println!("{}", line_content);
        }
    }

    Ok(())
}

/// Run the viewer in interactive mode for testing
fn run_viewer_mode(
    deps: crate::view::TuiDependencies,
    ahr_path: &PathBuf,
    gutter: &CliGutterPosition,
) -> Result<()> {
    // Clone the parameters to avoid lifetime issues
    let ahr_path = ahr_path.clone();
    let gutter = gutter.clone();

    // Run using shared TUI runtime - all async initialization happens inside the closure
    tui_runtime::run_tui_with_single_tokio_thread::<SessionViewerMsg, _, _>(
        deps,
        move |deps, rx_ui, tx_ui, rx_tick, terminal, input_state| async move {
            // First replay the recording to get the final terminal state
            let replay_result =
                replay_ahr_file(ahr_path).context("Failed to replay recording for viewer")?;

            // Create TerminalState and replay all events to build accurate state
            // Use the recorded terminal size, minus gutter width
            let gutter_config = cli_gutter_to_viewer_gutter(&gutter);
            let gutter_width = gutter_config.width();
            let recording_cols = if replay_result.initial_cols > gutter_width as u16 {
                replay_result.initial_cols - gutter_width as u16
            } else {
                replay_result.initial_cols
            };
            let mut recording_state =
                ah_recorder::TerminalState::new(replay_result.initial_rows, recording_cols);

            // Replay all data events and snapshots in chronological order
            // This builds the correct terminal state with accurate snapshot positioning
            for event in &replay_result.events {
                match event {
                    ah_recorder::AhrEvent::Data { data, .. } => {
                        recording_state.process_data(data);
                    }
                    ah_recorder::AhrEvent::Snapshot(snapshot) => {
                        recording_state.record_snapshot(snapshot.clone());
                    }
                    ah_recorder::AhrEvent::Resize { cols, rows, .. } => {
                        recording_state.resize(*cols, *rows);
                    }
                }
            }

            let recording_terminal_state =
                std::rc::Rc::new(std::cell::RefCell::new(recording_state));

            // Create viewer configuration (viewer gets full terminal size, display area calculated internally)
            let config = ViewerConfig {
                terminal_cols: replay_result.initial_cols as u16,
                terminal_rows: replay_result.initial_rows as u16,
                scrollback: 1000,
                gutter: gutter_config,
                is_replay_mode: true,
            };

            // Create session viewer view model with recording terminal state
            let view_model =
                build_session_viewer_view_model(recording_terminal_state, &config, None);

            // Create local task manager for instruction-based task creation
            let task_manager: Arc<dyn ah_core::TaskManager> =
                ah_core::create_session_viewer_task_manager()
                    .expect("Failed to create local task manager");

            // Construct the replay state with captured components
            let replay_state = ReplayState {
                view_model,
                viewer_config: config,
                task_manager,
            };
            run_replay_event_loop(replay_state, rx_ui, tx_ui, rx_tick, terminal, input_state).await
        },
    )
    .map_err(|e| anyhow::anyhow!("{}", e))
}

/// Run the replay event loop using the shared TUI runtime
async fn run_replay_event_loop(
    mut replay_state: ReplayState,
    mut rx_ui: chan::Receiver<ReplayUiMsg>,
    _tx_ui: chan::Sender<ReplayUiMsg>,
    mut rx_tick: chan::Receiver<std::time::Instant>,
    mut terminal: Terminal<CrosstermBackend<std::io::Stdout>>,
    mut input_state: InputState,
) -> Result<(), anyhow::Error> {
    loop {
        // Use biased select to prefer UI messages over ticks
        chan::select_biased! {
            recv(rx_ui) -> ui_msg => {
                let ui_msg = match ui_msg {
                    Ok(msg) => msg,
                    Err(_) => break,
                };

                match ui_msg {
                    ReplayUiMsg::UserInput(event) => {
                        handle_replay_user_input_event(
                            &mut replay_state,
                            &mut input_state,
                            &mut terminal,
                            event,
                        ).await?;
                    }
                    ReplayUiMsg::Tick => {
                        handle_replay_tick_event(&mut replay_state, &mut terminal).await?;
                    }
                    ReplayUiMsg::AppMsg(session_viewer_msg) => {
                        handle_replay_session_viewer_message(
                            &mut replay_state,
                            &mut terminal,
                            session_viewer_msg,
                        ).await?;
                    }
                }

                // Check for exit
                if replay_state.view_model.exit_requested {
                    break;
                }
            }
            recv(rx_tick) -> _ => {
                handle_replay_tick_event(&mut replay_state, &mut terminal).await?;

                if replay_state.view_model.exit_requested {
                    break;
                }
            }
        }
    }

    Ok(())
}

/// Handle user input events for replay
async fn handle_replay_user_input_event(
    replay_state: &mut ReplayState,
    input_state: &mut InputState,
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    event: Event,
) -> Result<(), anyhow::Error> {
    match event {
        Event::Key(key) => {
            debug!(
                key_code = ?key.code,
                modifiers = ?key.modifiers,
                key_kind = ?key.kind,
                focus_element = ?replay_state.view_model.focus_element,
                "Key event received in replay viewer"
            );

            // Clear exit confirmation on any non-ESC key
            if !matches!(key.code, crossterm::event::KeyCode::Esc) {
                replay_state.view_model.exit_confirmation_armed = false;
            }

            if key.code == crossterm::event::KeyCode::Esc {
                if replay_state.view_model.task_entry_visible {
                    replay_state.view_model.cancel_instruction_overlay();
                    return Ok(());
                }

                if replay_state.view_model.search_state.is_some() {
                    replay_state.view_model.exit_search();
                    replay_state.view_model.exit_confirmation_armed = false;
                    return Ok(());
                }

                if replay_state.view_model.exit_confirmation_armed {
                    info!("ESC pressed again, exiting");
                    replay_state.view_model.exit_requested = true;
                    return Ok(());
                } else {
                    info!("ESC pressed, arming exit confirmation");
                    replay_state.view_model.exit_confirmation_armed = true;
                    return Ok(());
                }
            }

            if replay_state.view_model.task_entry_visible {
                if replay_state.view_model.handle_instruction_key(&key) {
                    return Ok(());
                }

                if key.code == crossterm::event::KeyCode::Enter {
                    if let Some(instruction) = replay_state.view_model.instruction_text() {
                        let recording_state =
                            replay_state.view_model.recording_terminal_state.clone();
                        replay_state.view_model.cancel_instruction_overlay();
                        crate::viewer::launch_task_from_instruction(
                            recording_state,
                            Arc::clone(&replay_state.task_manager),
                            instruction,
                            &replay_state.view_model.task_entry.selected_agents,
                        )
                        .await;
                    }
                }
                return Ok(());
            }

            // Try view model's keyboard operation handling
            let msgs = replay_state.view_model.update(SessionViewerMsg::Key(key.clone()));
            if !msgs.is_empty() {
                return Ok(());
            }
        }
        Event::Mouse(mouse_event) => match mouse_event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                handle_mouse_click_for_view(
                    &mut replay_state.view_model,
                    &replay_state.viewer_config,
                    mouse_event.column,
                    mouse_event.row,
                );
            }
            MouseEventKind::ScrollUp => {
                let _ = replay_state.view_model.update(SessionViewerMsg::MouseScrollUp);
            }
            MouseEventKind::ScrollDown => {
                let _ = replay_state.view_model.update(SessionViewerMsg::MouseScrollDown);
            }
            _ => {}
        },
        Event::Resize(_width, _height) => {
            let _ = terminal.autoresize();
        }
        _ => {}
    }
    Ok(())
}

/// Handle tick events for replay
async fn handle_replay_tick_event(
    replay_state: &mut ReplayState,
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
) -> Result<(), anyhow::Error> {
    // Always re-render viewer
    update_row_metadata_with_autofollow(&mut replay_state.view_model, &replay_state.viewer_config);
    let recorded_dims = replay_state.view_model.recording_dims();
    let exit_confirmation = replay_state.view_model.exit_confirmation_armed;
    terminal.draw(|f| {
        render_view_frame(
            f,
            &mut replay_state.view_model,
            &replay_state.viewer_config,
            exit_confirmation,
            recorded_dims,
        );
    })?;

    Ok(())
}

/// Handle session viewer messages for replay
async fn handle_replay_session_viewer_message(
    replay_state: &mut ReplayState,
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    msg: SessionViewerMsg,
) -> Result<(), anyhow::Error> {
    // Handle session viewer messages if needed
    let _msgs = replay_state.view_model.update(msg);
    Ok(())
}

/// Strip ANSI escape sequences from text
fn strip_ansi_codes(text: &str) -> String {
    // Simple ANSI escape sequence stripper
    // This is a basic implementation - a more robust version would use a proper ANSI parser
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            // Start of ANSI escape sequence
            if let Some('[') = chars.peek() {
                chars.next(); // consume '['
                // Skip until we find a letter (end of sequence)
                while let Some(c) = chars.next() {
                    if c.is_ascii_alphabetic() || c == '@' {
                        break;
                    }
                }
            }
        } else {
            result.push(ch);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_ansi_codes() {
        let input = "\x1b[31mRed text\x1b[0m normal";
        let result = strip_ansi_codes(input);
        assert_eq!(result, "Red text normal");

        let input2 = "Plain text without colors";
        let result2 = strip_ansi_codes(input2);
        assert_eq!(result2, input2);
    }
}
