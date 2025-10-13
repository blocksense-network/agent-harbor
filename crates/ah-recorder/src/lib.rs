// Agent Harbor Recording and Replay
//
// This crate implements the `ah agent record` and `ah agent replay` functionality
// for capturing terminal sessions with byte-perfect fidelity using PTY capture,
// vt100 parsing, and Brotli-compressed .ahr file format.
//
// See: specs/Public/ah-agent-record.md for complete specification

pub mod format;
pub mod pty;
pub mod snapshots;
pub mod writer;

// Re-export key types for convenience
pub use format::{AhrBlockHeader, Record, RecData, RecResize, RecSnapshot, REC_DATA, REC_RESIZE, REC_SNAPSHOT};
pub use pty::{PtyEvent, PtyRecorder, PtyRecorderConfig, RecordingSession, TerminalState};
pub use snapshots::{Snapshot, SnapshotsReader, SnapshotsWriter, SharedSnapshotsWriter, create_shared_writer};
pub use writer::{AhrWriter, WriterConfig, now_ns};
