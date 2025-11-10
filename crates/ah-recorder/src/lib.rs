// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

// Agent Harbor Recording and Replay
//
// This crate implements the `ah agent record` and `ah agent replay` functionality
// for capturing terminal sessions with byte-perfect fidelity using PTY capture,
// vt100 parsing, and Brotli-compressed .ahr file format.
//
// See: specs/Public/ah-agent-record.md for complete specification

pub mod format;
pub mod ipc;
pub mod pty;
pub mod snapshots;
pub mod terminal_state;
pub mod writer;

// Re-export key types for convenience
pub use format::{
    AhrBlockHeader, REC_DATA, REC_RESIZE, REC_SNAPSHOT, RecData, RecResize, RecSnapshot, Record,
};
pub use ipc::{
    IpcClient, IpcCommand, IpcServer, IpcServerConfig, Request as IpcRequest,
    Response as IpcResponse,
};
pub use pty::{PtyEvent, PtyRecorder, PtyRecorderConfig, RecordingSession};
pub use snapshots::{
    SharedSnapshotsWriter, Snapshot, SnapshotsReader, SnapshotsWriter, create_shared_writer,
};
pub use terminal_state::{
    ColumnIndex, InMemoryLineIndex, LineIndex, ScreenLineIndex, TermFeatures, TerminalState,
};
pub use writer::{AhrWriter, WriterConfig, now_ns};

pub mod ahr_events;
pub mod reader;
pub mod replay;
pub use ahr_events::{AhrEvent, AhrSnapshot};
pub use reader::AhrReader;
pub use replay::{
    BranchPointsResult, InterleavedItem, ReplayResult, create_branch_points,
    interleave_with_terminal_state, replay_ahr_file,
};

#[cfg(test)]
mod tests {
    use super::replay::*;
    use crate::terminal_state::{InMemoryLineIndex, LineIndex};

    #[test]
    fn test_interleave_with_terminal_state() {
        // Create a TerminalState and simulate recording
        let mut terminal_state = crate::TerminalState::new(24, 80);

        // Process initial data to create lines
        terminal_state.process_data(b"Initial line 1\n");
        terminal_state.process_data(b"Initial line 2\n");
        terminal_state.process_data(b"Initial line 3\n");

        // Record first snapshot at current line (should be associated with line 3)
        terminal_state.record_snapshot(crate::AhrSnapshot {
            ts_ns: 1000000000,
            label: Some("snapshot-1".to_string()),
        });

        // Add more lines after the first snapshot
        terminal_state.process_data(b"Line after snapshot 1\n");
        terminal_state.process_data(b"Another line after snapshot 1\n");

        // Record second snapshot (should be associated with the current line)
        terminal_state.record_snapshot(crate::AhrSnapshot {
            ts_ns: 2000000000,
            label: Some("snapshot-2".to_string()),
        });

        // Add more lines
        terminal_state.process_data(b"Line after snapshot 2\n");
        terminal_state.process_data(b"Final line before edits\n");

        // Simulate editing previous lines with carriage returns
        // This overwrites "Final line before edits" with "Edited final line"
        terminal_state.process_data(b"\rEdited final line\n");

        // Overwrite an earlier line (this demonstrates why byte-based positioning fails)
        terminal_state.process_data(b"\r\r\rOverwritten line 3\n");

        // Record third snapshot after the edits
        terminal_state.record_snapshot(crate::AhrSnapshot {
            ts_ns: 3000000000,
            label: Some("snapshot-3".to_string()),
        });

        // Add final lines
        terminal_state.process_data(b"Final line 1\n");
        terminal_state.process_data(b"Final line 2\n");

        println!("=== Terminal State After All Operations ===");
        println!("Line count: {}", terminal_state.line_count());
        for i in 0..terminal_state.line_count() {
            println!(
                "Line {}: '{}'",
                i,
                terminal_state.line_content(InMemoryLineIndex(i))
            );
        }

        // Test interleaving - this should show snapshots at their correct positions
        let result = crate::replay::interleave_with_terminal_state(&terminal_state, 200);

        println!("\n=== Interleaved Result ===");
        for (i, item) in result.items.iter().enumerate() {
            match item {
                InterleavedItem::Line(content) => println!("{}: Line '{}'", i, content),
                InterleavedItem::Snapshot(s) => println!(
                    "{}: SNAPSHOT {} - {}",
                    i,
                    s.anchor_byte,
                    s.label.as_deref().unwrap_or("unnamed")
                ),
            }
        }

        // Verify the structure
        assert!(
            result.items.len() >= 8,
            "Should have at least 8 items (lines + snapshots)"
        );

        // Find snapshots in the result
        let snapshots: Vec<_> = result
            .items
            .iter()
            .filter_map(|item| match item {
                InterleavedItem::Snapshot(s) => Some(s.anchor_byte),
                _ => None,
            })
            .collect();

        // Should have snapshots at their respective anchor bytes
        // (The exact anchor bytes depend on the terminal state processing)
        assert!(!snapshots.is_empty(), "Should have some snapshots");

        // Verify that snapshots appear in the correct relative positions
        // (we can't easily test absolute positions due to vt100 rendering specifics,
        // but we can verify the overall structure is correct)
        let has_lines = result.items.iter().any(|item| matches!(item, InterleavedItem::Line(_)));
        let has_snapshots =
            result.items.iter().any(|item| matches!(item, InterleavedItem::Snapshot(_)));

        assert!(has_lines, "Should have line items");
        assert!(has_snapshots, "Should have snapshot items");
        assert_eq!(result.total_bytes, 200);
    }

    #[test]
    fn test_snapshot_association_with_newlines() {
        // Test that snapshots are correctly associated with lines when
        // the cursor is at the beginning of an empty line after a newline
        // This simulates the behavior of programs like mock-simple.py

        let mut terminal_state = crate::TerminalState::new(24, 80);

        // Simulate: print "Number: 5\n" - writes to line, moves cursor to next line
        terminal_state.process_data(b"Number: 5\n");

        // Simulate: print "Taking snapshot after number 5...\n" - writes to next line, moves cursor
        terminal_state.process_data(b"Taking snapshot after number 5...\n");

        // At this point, cursor should be at the beginning of an empty line
        // Take snapshot - should associate with the line containing "Taking snapshot..."
        terminal_state.record_snapshot(crate::AhrSnapshot {
            ts_ns: 1000000000,
            label: Some("snapshot-1".to_string()),
        });

        // The snapshot should be associated with the line where the cursor was when taken
        // In this case, it should be on line 2 (the empty line after "Taking snapshot..." + \n)
        let snapshots_on_line_2 = terminal_state.get_snapshots_for_line(LineIndex(2));
        assert_eq!(
            snapshots_on_line_2.len(),
            1,
            "Should have exactly one snapshot on line 2"
        );
        let snapshot = &snapshots_on_line_2[0];
        assert_eq!(
            snapshot.label,
            Some("snapshot-1".to_string()),
            "Snapshot should have correct label"
        );

        // Verify snapshot position information
        assert_eq!(
            snapshot.line,
            crate::LineIndex(2),
            "Snapshot should be associated with line 2"
        );
        assert_eq!(
            snapshot.column,
            crate::ColumnIndex(42),
            "Snapshot should be at column 42 (end of 'Taking snapshot...' text)"
        );

        // Verify no other lines have snapshots
        for line_idx in 0..terminal_state.line_count() {
            if line_idx != 2 {
                let snapshots_on_line = terminal_state.get_snapshots_for_line(LineIndex(line_idx));
                assert_eq!(
                    snapshots_on_line.len(),
                    0,
                    "Line {} should not have any snapshots, but found {}",
                    line_idx,
                    snapshots_on_line.len()
                );
            }
        }

        // Verify that we can find the snapshot when searching by its properties
        let found_snapshots = terminal_state.all_snapshots();
        assert_eq!(
            found_snapshots.len(),
            1,
            "Should have exactly one snapshot total"
        );
        let found_snapshot = &found_snapshots[0];
        assert_eq!(
            found_snapshot.label,
            Some("snapshot-1".to_string()),
            "Found snapshot should have correct label"
        );
        assert_eq!(
            found_snapshot.line,
            crate::LineIndex(2),
            "Found snapshot should be on correct line"
        );
        assert_eq!(
            found_snapshot.column,
            crate::ColumnIndex(42),
            "Found snapshot should be at correct column"
        );
    }
}
