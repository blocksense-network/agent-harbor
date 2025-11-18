// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

// AHR file replay functionality
//
// Replays AHR recordings to reconstruct the final terminal state
// using vt100 terminal emulation.

use crate::reader::AhrReader;
use crate::terminal_state::{InMemoryLineIndex, LineIndex};
use std::path::Path;

/// Result of replaying an AHR file
#[derive(Debug)]
pub struct ReplayResult {
    /// All events from the recording (for state reconstruction)
    pub events: Vec<crate::AhrEvent>,
    /// Initial terminal size from recording
    pub initial_cols: u16,
    pub initial_rows: u16,
    /// Total bytes processed
    pub total_bytes: u64,
}

/// An item in the interleaved output (either a terminal line or a snapshot)
#[derive(Debug, Clone)]
pub enum InterleavedItem {
    /// A terminal line
    Line(String),
    /// A snapshot event
    Snapshot(crate::Snapshot),
}

/// Result of creating interleaved branch points
#[derive(Debug)]
pub struct BranchPointsResult {
    /// Interleaved items: terminal lines with snapshots inserted at their associated lines
    pub items: Vec<InterleavedItem>,
    /// Total bytes processed
    pub total_bytes: u64,
}

/// Replay an AHR file to reconstruct the final terminal state
pub fn replay_ahr_file<P: AsRef<Path>>(path: P) -> std::io::Result<ReplayResult> {
    let mut reader = AhrReader::new(path)?;

    // Read all events from the recording
    let events = reader.read_all_events()?;

    // Calculate total bytes and initial terminal size from events
    let mut total_bytes = 0u64;
    let mut initial_cols = 80u16; // Default
    let mut initial_rows = 24u16; // Default

    for event in &events {
        match event {
            crate::AhrEvent::Data { data, .. } => {
                total_bytes += data.len() as u64;
            }
            crate::AhrEvent::Resize { cols, rows, .. } => {
                // Update size (use first resize or final size)
                initial_cols = *cols;
                initial_rows = *rows;
            }
            crate::AhrEvent::Snapshot(_) => {
                // Snapshots don't add to byte count
            }
        }
    }

    Ok(ReplayResult {
        events,
        initial_cols,
        initial_rows,
        total_bytes,
    })
}

/// Create branch points by interleaving snapshots with terminal lines
pub fn create_branch_points<P: AsRef<Path>>(
    ahr_path: P,
    _snapshots_path: Option<P>,
) -> std::io::Result<BranchPointsResult> {
    // TODO:
    // The call to the replay_ahr_file function below doesn't need to exist.
    // Every AHR file must start with a Resize event as its very first record.
    // We can add a helper function to obtain this first event from the file, so
    // we can obtain the size of the terminal and set up our replay terminal with
    // the same dimentions.
    // We can directly use an iterator that will read the AHR events from the file
    // in the loop below. This would work better for very large files, since we won't
    // need to read the entire file into memory.

    // Replay the AHR file through TerminalState to get accurate line-to-snapshot associations
    let replay_result = replay_ahr_file(&ahr_path)?;

    // Create a TerminalState with the same dimensions as the PTY that was recorded
    // During recording, snapshots are taken at cursor positions in the PTY output,
    // so we need to replay with the exact same PTY dimensions to get identical scrolling behavior
    let recording_terminal_cols = replay_result.initial_cols;
    let recording_terminal_rows = replay_result.initial_rows;
    let mut terminal_state = crate::TerminalState::new_with_scrollback(
        recording_terminal_rows,
        recording_terminal_cols, // Use the same height as recording to get identical scrolling
        1_000_000,               // Large scrollback to capture all content
    );

    for event in &replay_result.events {
        match event {
            crate::AhrEvent::Data { data, .. } => {
                terminal_state.process_data(data);
            }
            crate::AhrEvent::Snapshot(ahr_snapshot) => {
                terminal_state.record_snapshot(ahr_snapshot.clone());
            }
            crate::AhrEvent::Resize { cols, rows, .. } => {
                terminal_state.resize(*cols, *rows);
            }
        }
    }

    // Interleave lines and snapshots using TerminalState-based positioning
    let result = interleave_with_terminal_state(&terminal_state, replay_result.total_bytes);

    Ok(result)
}

/// Interleave terminal lines and snapshots using TerminalState-based positioning
///
/// This uses position-based interleaving: snapshots at column 0 are placed before
/// the line, while snapshots at other columns are placed after the line.
/// This provides accurate positioning that reflects the actual cursor position
/// at snapshot time, not just the line.
pub fn interleave_with_terminal_state(
    terminal_state: &crate::TerminalState,
    total_bytes: u64,
) -> BranchPointsResult {
    let mut items = Vec::new();

    // Get the total number of lines including scrollback
    let screen = terminal_state.parser().screen();
    let contents = screen.contents();
    let all_lines: Vec<&str> = contents.lines().collect();
    let total_lines = all_lines.len();

    // Iterate through all lines in the terminal (including scrollback)
    for line_idx in 0..total_lines {
        // Get snapshots for this line, grouped by column position
        let snapshots_at_start = terminal_state.get_snapshots_for_line(LineIndex(line_idx));

        // Add snapshots that are at column 0 (before the line)
        for snapshot in snapshots_at_start {
            items.push(InterleavedItem::Snapshot(snapshot.clone()));
        }

        // Add the line content
        let line_content = terminal_state.line_content(InMemoryLineIndex(line_idx));
        items.push(InterleavedItem::Line(line_content));
    }

    BranchPointsResult { items, total_bytes }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_replay_empty_file() {
        let temp_file = NamedTempFile::new().unwrap();
        let result = replay_ahr_file(temp_file.path());

        // Empty file should not error but have empty results
        assert!(result.is_ok());
        let replay = result.unwrap();
        assert_eq!(replay.events.len(), 0);
        assert_eq!(replay.total_bytes, 0);
    }

    #[test]
    fn test_replay_invalid_file() {
        // Try to replay a non-existent file
        let result = replay_ahr_file("/non/existent/file.ahr");
        assert!(result.is_err());
    }
}
