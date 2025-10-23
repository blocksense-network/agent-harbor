// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

// Implementation of `ah agent replay` command
//
// Replays recorded agent sessions and displays the final terminal state,
// either by fast-forwarding through the recording or by simulating
// the original timing.

use crate::agent::record::CliGutterPosition;
use ah_recorder::viewer::GutterPosition;
use ah_recorder::{ReplayResult, TerminalViewer, ViewerConfig, ViewerEventLoop, replay_ahr_file};
use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use vt100::Parser as Vt100Parser;

/// Convert CLI gutter position to viewer gutter position
fn cli_gutter_to_viewer_gutter(cli_pos: &CliGutterPosition) -> GutterPosition {
    match cli_pos {
        CliGutterPosition::Left => GutterPosition::Left,
        CliGutterPosition::Right => GutterPosition::Right,
        CliGutterPosition::None => GutterPosition::None,
    }
}

/// Replay a recorded agent session
#[derive(Parser, Debug)]
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
async fn print_metadata(ahr_path: &PathBuf) -> Result<()> {
    let replay_result =
        replay_ahr_file(ahr_path).context("Failed to replay recording for metadata")?;

    println!("Recording: {}", ahr_path.display());
    println!(
        "Initial terminal size: {}x{}",
        replay_result.initial_cols, replay_result.initial_rows
    );
    println!("Total bytes processed: {}", replay_result.total_bytes);
    println!("Terminal lines: {}", replay_result.lines.len());
    println!("Snapshots: {}", replay_result.snapshots.len());

    if !replay_result.snapshots.is_empty() {
        println!("\nSnapshots:");
        for snapshot in &replay_result.snapshots {
            let label = snapshot.label.as_deref().unwrap_or("<unnamed>");
            println!(
                "  ID {}: {} (byte {})",
                snapshot.id, label, snapshot.anchor_byte
            );
        }
    }

    Ok(())
}

/// Perform fast-forward replay and display final terminal state
async fn fast_replay(ahr_path: &PathBuf, no_colors: bool) -> Result<()> {
    let replay_result = replay_ahr_file(ahr_path).context("Failed to replay recording")?;

    // Display the final terminal state
    for line in &replay_result.lines {
        if no_colors {
            // Strip ANSI sequences for plain text output
            let plain_text = strip_ansi_codes(&line.text);
            println!("{}", plain_text);
        } else {
            // Output with ANSI colors preserved
            println!("{}", line.text);
        }
    }

    Ok(())
}

/// Run the viewer in interactive mode for testing
async fn run_viewer_mode(ahr_path: &PathBuf, gutter: &CliGutterPosition) -> Result<()> {
    // First replay the recording to get the final terminal state
    let replay_result =
        replay_ahr_file(ahr_path).context("Failed to replay recording for viewer")?;

    // Create a terminal state with the final state by simulating the data
    // For testing, we'll create a terminal state and feed it enough data to show the final state
    use ah_recorder::pty::TerminalState;
    let terminal_state = Arc::new(Mutex::new(TerminalState::new(
        replay_result.initial_rows,
        replay_result.initial_cols,
    )));

    // Simulate feeding the final content to the terminal state
    // This is a simplified approach - in practice, we'd replay the actual PTY data
    {
        let mut ts = terminal_state.lock().unwrap();
        // For each line in the replay result, simulate writing it
        for line in &replay_result.lines {
            // Write the line content followed by newline
            let data = format!("{}\n", line.text);
            ts.parser_mut().process(data.as_bytes());
        }
    }

    // Create viewer configuration
    let config = ViewerConfig {
        cols: replay_result.initial_cols as u16,
        rows: replay_result.initial_rows as u16,
        scrollback: 1000,
        gutter: cli_gutter_to_viewer_gutter(gutter),
    };

    // Create terminal viewer
    let viewer = TerminalViewer::new(terminal_state, config);

    // Create event loop for the viewer
    let mut event_loop = ViewerEventLoop::new(viewer, replay_result.snapshots)
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
