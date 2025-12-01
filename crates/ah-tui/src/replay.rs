// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

// Implementation of `ah agent replay` command
//
// Replays recorded agent sessions and displays the final terminal state,
// either by fast-forwarding through the recording or by simulating
// the original timing.

use crate::record::CliGutterPosition;
use crate::theme::Theme;
use crate::view_model::session_viewer_model::{GutterConfig, GutterPosition};
use crate::viewer::{ViewerConfig, ViewerEventLoop, build_session_viewer_view_model};
use ah_core;
use ah_recorder::replay_ahr_file;
use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

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
pub async fn execute(args: ReplayArgs) -> Result<()> {
    // For now, assume the session argument is a path to an .ahr file
    let ahr_path = PathBuf::from(&args.session);

    if !ahr_path.exists() {
        anyhow::bail!("Recording file not found: {}", args.session);
    }

    if args.print_meta {
        // Print metadata and stats
        print_metadata(&ahr_path).await?;
    } else if args.viewer {
        // Interactive viewer mode: load recording into live viewer
        run_viewer_mode(&ahr_path, &args.gutter).await?;
    } else if args.fast {
        // Fast-forward replay: immediately show final state
        fast_replay(&ahr_path, args.no_colors).await?;
    } else {
        // Timed replay: simulate original timing (not yet implemented)
        anyhow::bail!("Timed replay not yet implemented. Use --fast or --viewer for replay.");
    }

    Ok(())
}

/// Print metadata and statistics about the recording
#[allow(clippy::disallowed_methods)]
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
#[allow(clippy::disallowed_methods)]
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
async fn run_viewer_mode(ahr_path: &PathBuf, gutter: &CliGutterPosition) -> Result<()> {
    // First replay the recording to get the final terminal state
    let replay_result =
        replay_ahr_file(ahr_path).context("Failed to replay recording for viewer")?;

    // Create a TerminalState and replay all events to get the final state
    let terminal_state = Arc::new(Mutex::new(ah_recorder::TerminalState::new(
        replay_result.initial_rows,
        replay_result.initial_cols,
    )));

    // Replay all events through the TerminalState
    {
        let mut ts = terminal_state.lock().unwrap();
        for event in &replay_result.events {
            match event {
                ah_recorder::AhrEvent::Data { data, .. } => {
                    ts.process_data(data);
                }
                ah_recorder::AhrEvent::Resize { cols, rows, .. } => {
                    ts.resize(*cols, *rows);
                }
                ah_recorder::AhrEvent::Snapshot { .. } => {
                    // Snapshots don't affect the terminal display
                }
            }
        }
    }

    // Create viewer configuration (viewer gets full terminal size, display area calculated internally)
    let config = ViewerConfig {
        terminal_cols: replay_result.initial_cols as u16,
        terminal_rows: replay_result.initial_rows as u16,
        scrollback: 1000,
        gutter: cli_gutter_to_viewer_gutter(gutter), // Line numbers enabled by default
        is_replay_mode: true,
    };

    // Create TerminalState and replay all events to build accurate state
    // Use the recorded terminal size, minus gutter width
    let gutter_config = cli_gutter_to_viewer_gutter(gutter);
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

    let recording_terminal_state = std::rc::Rc::new(std::cell::RefCell::new(recording_state));

    // Create session viewer view model with recording terminal state
    let view_model =
        build_session_viewer_view_model(recording_terminal_state, &config, None, &Theme::default());

    // Create local task manager for instruction-based task creation
    let task_manager =
        ah_core::create_session_viewer_task_manager().expect("Failed to create local task manager");

    // Create event loop for the viewer (replay doesn't receive new snapshots)
    let mut event_loop =
        ViewerEventLoop::new(view_model, config.clone(), task_manager, Theme::default())
            .context("Failed to create viewer event loop")?;

    // Run the viewer event loop
    event_loop.run().await?;

    Ok(())
}

/// Strip ANSI escape sequences from text
fn strip_ansi_codes(text: &str) -> String {
    // Simple ANSI escape sequence stripper
    // This is a basic implementation - a more robust version would use a proper ANSI parser
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    // Allow while_let_on_iterator for explicit peek/next control
    #[allow(clippy::while_let_on_iterator)]
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
