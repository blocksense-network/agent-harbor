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
pub mod viewer;
pub mod writer;

// Re-export key types for convenience
pub use format::{
    AhrBlockHeader, REC_DATA, REC_RESIZE, REC_SNAPSHOT, RecData, RecResize, RecSnapshot, Record,
};
pub use ipc::{
    IpcClient, IpcCommand, IpcServer, IpcServerConfig, Request as IpcRequest,
    Response as IpcResponse,
};
pub use pty::{PtyEvent, PtyRecorder, PtyRecorderConfig, RecordingSession, TerminalState};
pub use snapshots::{
    SharedSnapshotsWriter, Snapshot, SnapshotsReader, SnapshotsWriter, create_shared_writer,
};
pub use viewer::{InstructionOverlay, SearchState, TerminalViewer, ViewerConfig, ViewerEventLoop};
pub use writer::{AhrWriter, WriterConfig, now_ns};

pub mod reader;
pub mod replay;
pub use reader::{AhrReadEvent, AhrReader};
pub use replay::{
    BranchPointsResult, InterleavedItem, ReplayResult, TerminalLine, create_branch_points,
    interleave_by_position, replay_ahr_file,
};

#[cfg(test)]
mod tests {
    use super::replay::*;
    use crate::snapshots::Snapshot;

    #[test]
    fn test_interleave_by_position() {
        // Create some terminal lines
        let lines = vec![
            TerminalLine {
                index: 0,
                text: "Line 1".to_string(),
                last_write_byte: 10,
            },
            TerminalLine {
                index: 1,
                text: "Line 2".to_string(),
                last_write_byte: 20,
            },
            TerminalLine {
                index: 2,
                text: "Line 3".to_string(),
                last_write_byte: 30,
            },
        ];

        // Create some snapshots
        let snapshots = vec![
            Snapshot {
                id: 1,
                ts_ns: 1000000000,
                label: Some("snapshot-1".to_string()),
                kind: Some("test".to_string()),
                anchor_byte: 5,
            },
            Snapshot {
                id: 2,
                ts_ns: 2000000000,
                label: Some("snapshot-2".to_string()),
                kind: Some("test".to_string()),
                anchor_byte: 15,
            },
            Snapshot {
                id: 3,
                ts_ns: 3000000000,
                label: Some("snapshot-3".to_string()),
                kind: Some("test".to_string()),
                anchor_byte: 25,
            },
        ];

        // Test interleaving
        let result = interleave_by_position(&lines, &snapshots, 30);

        // Should have alternating lines and snapshots
        assert_eq!(result.items.len(), 6); // 3 lines + 3 snapshots

        // Check the order
        match &result.items[0] {
            InterleavedItem::Snapshot(s) => assert_eq!(s.id, 1),
            _ => panic!("Expected snapshot"),
        }
        match &result.items[1] {
            InterleavedItem::Line(l) => assert_eq!(l.index, 0),
            _ => panic!("Expected line"),
        }
        match &result.items[2] {
            InterleavedItem::Snapshot(s) => assert_eq!(s.id, 2),
            _ => panic!("Expected snapshot"),
        }
        match &result.items[3] {
            InterleavedItem::Line(l) => assert_eq!(l.index, 1),
            _ => panic!("Expected line"),
        }
        match &result.items[4] {
            InterleavedItem::Snapshot(s) => assert_eq!(s.id, 3),
            _ => panic!("Expected snapshot"),
        }
        match &result.items[5] {
            InterleavedItem::Line(l) => assert_eq!(l.index, 2),
            _ => panic!("Expected line"),
        }

        assert_eq!(result.total_bytes, 30);
    }
}
