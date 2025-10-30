// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

/// Events emitted by the AHR reader during replay
///
/// These events represent the chronological sequence of terminal operations
/// that were recorded in an AHR file.
#[derive(Debug, Clone)]
pub enum AhrEvent {
    /// PTY data record containing raw terminal output
    Data {
        ts_ns: u64,
        start_byte_off: u64,
        data: Vec<u8>,
    },
    /// Terminal resize record
    Resize { ts_ns: u64, cols: u16, rows: u16 },
    /// Filesystem snapshot record
    Snapshot(AhrSnapshot),
}

/// A snapshot event from an AHR file
///
/// Contains the basic snapshot information read from the AHR format,
/// without the derived positioning information.
#[derive(Debug, Clone)]
pub struct AhrSnapshot {
    /// Timestamp in nanoseconds since UNIX epoch
    pub ts_ns: u64,
    /// Label for this snapshot (optional, for human readability)
    pub label: Option<String>,
}
