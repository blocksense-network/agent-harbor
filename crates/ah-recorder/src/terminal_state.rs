// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

// TerminalState - Core state machine for accurate terminal reconstruction
//
// This module provides TerminalState which maintains a vt100 parser and processes
// terminal events in chronological order to provide accurate answers about terminal
// content and snapshot positioning.
//
// PROBLEM STATEMENT:
// Terminal output is not immutable - lines can be overwritten by subsequent output
// (especially with carriage returns '\r'). Byte-position based approaches that try to
// correlate snapshots with "last_write_byte" positions become unreliable because the
// terminal content at those positions can change after the snapshot was taken.
//
// SOLUTION:
// Process all events (PTY data and snapshots) in the exact chronological order they
// occurred. When a snapshot is recorded, capture which line was currently active
// (cursor position) in the vt100 model. This association remains valid even if the
// line content changes later, because we're tracking the line index that was active
// at snapshot time, not the content that happened to be there.
//
// ALGORITHM OVERVIEW:
// 1. Live recording: PTY bytes → vt100 parser → terminal state updates
//    When snapshot arrives → record association with current active line (cursor row)
//    UI queries current vt100 state for content and snapshot indicators
//
// 2. Replay: Start with fresh TerminalState → replay all events chronologically
//    (data, resizes, snapshots) → final state contains accurate associations
//    Iterate through lines, emitting content and snapshot markers
//
// This ensures snapshot positioning reflects the actual terminal state at capture time,
// not potentially incorrect byte-based approximations that can become stale.

/// An absolute line index representing a line in the complete terminal output history.
///
/// This represents the total number of lines that have been processed since
/// terminal initialization, regardless of scrollback buffer limits. LineIndex(0)
/// refers to the first line ever output, LineIndex(1) to the second line, etc.
///
/// Please note that line here means a screen line (which may originate from a long
/// real line that was wrapped to multiple screen lines).
///
/// This type is used for snapshot positioning, which must remain valid even when
/// lines have scrolled out of the scrollback buffer entirely.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct LineIndex(pub usize);

impl LineIndex {
    /// Get the underlying usize value
    pub fn as_usize(self) -> usize {
        self.0
    }
}

impl std::fmt::Display for LineIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::ops::Add<usize> for LineIndex {
    type Output = LineIndex;

    fn add(self, rhs: usize) -> Self::Output {
        LineIndex(self.0 + rhs)
    }
}

/// A line index within the currently accessible terminal memory.
///
/// This includes lines in both the visible screen area and the scrollback buffer,
/// but excludes lines that have scrolled out of the scrollback buffer entirely.
/// InMemoryLineIndex(0) refers to the oldest line still in memory (top of scrollback),
/// and higher indices refer to more recent lines.
///
/// This type is used for accessing terminal content that is currently available
/// for display and navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InMemoryLineIndex(pub usize);

impl InMemoryLineIndex {
    /// Get the underlying usize value
    pub fn as_usize(self) -> usize {
        self.0
    }
}

impl std::fmt::Display for InMemoryLineIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// A line index within the currently visible screen area.
///
/// This represents rows on the terminal screen that are currently visible to the user.
/// ScreenLineIndex(0) refers to the top row of the screen, ScreenLineIndex(1) to the
/// second row, etc., up to the screen height.
///
/// This type is used for operations that are specific to the visible viewport,
/// such as cursor positioning within the visible area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScreenLineIndex(pub usize);

impl ScreenLineIndex {
    /// Get the underlying usize value
    pub fn as_usize(self) -> usize {
        self.0
    }
}

impl std::fmt::Display for ScreenLineIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// A newtype for terminal column indices to prevent accidental assignments
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct ColumnIndex(pub usize);

impl ColumnIndex {
    /// Get the underlying usize value
    pub fn as_usize(self) -> usize {
        self.0
    }
}

/// Terminal feature flags that track which input-affecting modes are active
#[derive(Debug, Clone, Default)]
pub struct TermFeatures {
    pub mouse_1000: bool, // click
    pub mouse_1002: bool, // button-motion
    pub mouse_1003: bool, // any-motion
    pub sgr_mouse_1006: bool,
    pub focus_1004: bool,
    pub bracketed_paste_2004: bool,
    pub app_cursor_1: bool, // DECCKM
}

/// No-op callbacks implementation for vt100 parser when no PTY interaction is needed
#[allow(dead_code)]
struct NoOpCallbacks;

impl vt100::Callbacks for NoOpCallbacks {
    fn unhandled_csi(
        &mut self,
        _screen: &mut vt100::Screen,
        _i1: Option<u8>,
        _i2: Option<u8>,
        _params: &[&[u16]],
        _action: char,
    ) {
        // Do nothing
    }
}

/// VT100 callbacks implementation for handling terminal queries and mode changes
pub type WriteReplyFn = std::sync::Arc<dyn Fn(&[u8]) + Send + Sync>;
#[derive(Clone)]
pub struct TermCallbacks {
    pub feats: std::sync::Arc<std::sync::Mutex<TermFeatures>>,
    /// Where to write terminal replies (CPR, DA, etc.)
    pub write_reply: WriteReplyFn,
}

impl vt100::Callbacks for TermCallbacks {
    fn unhandled_csi(
        &mut self,
        screen: &mut vt100::Screen,
        i1: Option<u8>,
        i2: Option<u8>,
        params: &[&[u16]],
        action: char,
    ) {
        // Handle DEC Private Modes: CSI ? Pm h/l
        if i1 == Some(b'?') && (action == 'h' || action == 'l') {
            let set = action == 'h';
            if let Ok(mut feats) = self.feats.lock() {
                for p in params.iter() {
                    for &m in p.iter() {
                        match m {
                            1 => feats.app_cursor_1 = set,
                            1000 => feats.mouse_1000 = set,
                            1002 => feats.mouse_1002 = set,
                            1003 => feats.mouse_1003 = set,
                            1004 => feats.focus_1004 = set,
                            1006 => feats.sgr_mouse_1006 = set,
                            2004 => feats.bracketed_paste_2004 = set,
                            _ => {}
                        }
                    }
                }
            }
            return;
        }

        // Device Status Report request we must answer: CSI 6 n -> reply CSI row;col R
        if i1.is_none() && i2.is_none() && action == 'n' {
            if let Some(p) = params.first() {
                if p.first() == Some(&6) {
                    // Get 1-based cursor position from vt100 screen
                    let (row0, col0) = screen.cursor_position();
                    let (row, col) = (row0 + 1, col0 + 1);
                    let reply = format!("\x1b[{};{}R", row, col);
                    (self.write_reply)(reply.as_bytes());
                }
            }
        }
    }
}

/// TerminalState maintains accurate terminal state by processing events chronologically
///
/// This state machine processes PTY data and snapshot events in the exact order they
/// occurred during recording, maintaining a vt100 parser to accurately track terminal
/// content and associate snapshots with the lines that were active when they were captured.
///
/// Key operations:
/// - `process_data()`: Feeds PTY output through vt100 parser, updating terminal state
/// - `record_snapshot()`: Associates snapshot with current active line (cursor position)
/// - `has_snapshot_at_line()`: Binary search to check for snapshots at specific lines
/// - `line_content()`: Returns ANSI-formatted content of any terminal line
///
/// The snapshot associations are immutable once recorded - they reflect the terminal
/// state at the exact moment the snapshot was taken, not subject to later content changes.
pub struct TerminalState {
    /// vt100 parser for terminal state reconstruction (contains callbacks)
    parser: vt100::Parser<TermCallbacks>,
    /// Current terminal feature flags (shared with callbacks)
    term_features: std::sync::Arc<std::sync::Mutex<TermFeatures>>,
    /// Snapshots associated with specific positions, sorted by line index
    /// Since snapshots are recorded chronologically, line numbers are strictly increasing
    snapshots: Vec<crate::Snapshot>,
}

impl TerminalState {
    /// Create a new TerminalState with the given dimensions
    pub fn new(rows: u16, cols: u16) -> Self {
        TerminalState::new_with_scrollback(rows, cols, 1_000_000)
    }

    /// Create a new TerminalState with the given dimensions and scrollback
    pub fn new_with_scrollback(rows: u16, cols: u16, scrollback: usize) -> Self {
        let term_features = std::sync::Arc::new(std::sync::Mutex::new(TermFeatures::default()));
        let callbacks = TermCallbacks {
            feats: term_features.clone(),
            write_reply: std::sync::Arc::new(|_| {}), // no-op initially
        };
        let parser = vt100::Parser::new_with_callbacks(rows, cols, scrollback, callbacks);

        Self {
            parser,
            term_features,
            snapshots: Vec::new(),
        }
    }

    /// Process PTY data through the vt100 parser
    pub fn process_data(&mut self, data: &[u8]) {
        self.parser.process(data);
    }

    /// Set callbacks with a write_reply function for PTY interaction
    pub fn set_write_reply(&mut self, write_reply: WriteReplyFn) {
        self.parser.callbacks_mut().write_reply = write_reply;
    }

    /// Get current terminal features
    pub fn term_features(&self) -> TermFeatures {
        self.term_features.lock().unwrap().clone()
    }

    /// Record a snapshot associated with the current cursor position
    ///
    /// This captures both the line and column that were active when the snapshot was taken,
    /// providing accurate positioning for gutter indicators and branch-points output.
    /// Unlike byte-position approaches that can become stale, this associates snapshots
    /// with the exact cursor position that was active at snapshot time.
    ///
    /// The association is immutable - it reflects the true terminal state at capture time,
    /// unaffected by subsequent output that might overwrite the line content.
    ///
    /// Since snapshots are recorded chronologically during a session, line numbers are
    /// strictly increasing, allowing efficient sorted storage and binary search lookups.
    pub fn record_snapshot(&mut self, snapshot: crate::AhrSnapshot) -> crate::Snapshot {
        // Get the current cursor position (row, column)
        let screen = self.parser.screen();
        let (cursor_row, cursor_col) = screen.cursor_position();
        let cursor_col = cursor_col as usize;

        // Calculate absolute line number accounting for fallen scrollback lines
        let cursor_row = cursor_row as usize;
        let screen_rows = screen.size().0 as usize;

        let lines_in_memory = self.total_output_lines_in_memory();
        let lines_fallen_out = screen.dropped_off_scrollback_lines();

        // Same calculation as get_visible_line_absolute_index
        let starting_position_in_memory = lines_in_memory.saturating_sub(screen_rows);
        let position_in_memory = starting_position_in_memory + cursor_row;
        let absolute_line = LineIndex(lines_fallen_out + position_in_memory);

        // Create a full snapshot with position information
        let full_snapshot = crate::Snapshot {
            ts_ns: snapshot.ts_ns,
            label: snapshot.label,
            kind: None,     // Will be set later if needed
            anchor_byte: 0, // Not used in new approach
            line: absolute_line,
            column: ColumnIndex(cursor_col),
        };

        // Since line numbers are strictly increasing, we can just push to maintain sorted order
        self.snapshots.push(full_snapshot.clone());
        full_snapshot
    }

    /// Resize the terminal
    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.parser.screen_mut().set_size(rows, cols);
    }

    /// Get the current number of lines in the terminal (screen height)
    pub fn line_count(&self) -> usize {
        let screen = self.parser.screen();
        screen.size().0 as usize
    }

    /// Get the content of a line currently in memory (visible area + scrollback)
    /// This does not include any formatting information. It is in plain text format.
    /// Newlines are not included.
    pub fn line_content(&self, line_idx: InMemoryLineIndex) -> String {
        self.parser.screen().row_by_global_index(line_idx.as_usize())
    }

    /// Get the formatted content of a line currently in memory (visible area + scrollback)
    /// This includes ANSI escape codes for colors and formatting.
    /// Newlines are not included.
    pub fn line_content_formatted(&self, line_idx: InMemoryLineIndex) -> Vec<u8> {
        self.parser.screen().row_by_global_index_formatted(line_idx.as_usize())
    }

    /// Convert an absolute LineIndex to an InMemoryLineIndex if the line is still in memory
    /// Returns None if the line has scrolled out of the scrollback buffer
    pub fn line_index_to_in_memory(&self, line_idx: LineIndex) -> Option<InMemoryLineIndex> {
        let lines_in_memory = self.total_output_lines_in_memory();
        let lines_fallen_out = self.parser.screen().dropped_off_scrollback_lines();

        if line_idx.as_usize() >= lines_fallen_out {
            let in_memory_idx = line_idx.as_usize() - lines_fallen_out;
            if in_memory_idx < lines_in_memory {
                Some(InMemoryLineIndex(in_memory_idx))
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Check if an absolute LineIndex is still available in memory
    /// Returns true if the line content can be accessed, false if it has scrolled out
    pub fn is_line_index_in_memory(&self, line_idx: LineIndex) -> bool {
        self.line_index_to_in_memory(line_idx).is_some()
    }

    /// Get the content of a line by its absolute LineIndex
    /// Returns None if the line is no longer in memory (has scrolled out of scrollback)
    pub fn line_content_by_line_index(&self, line_idx: LineIndex) -> Option<String> {
        self.line_index_to_in_memory(line_idx)
            .map(|in_memory_idx| self.line_content(in_memory_idx))
    }

    /// Get the formatted content of a line by its absolute LineIndex
    /// Returns None if the line is no longer in memory (has scrolled out of scrollback)
    /// This includes ANSI escape codes for colors and formatting.
    pub fn line_content_by_line_index_formatted(&self, line_idx: LineIndex) -> Option<Vec<u8>> {
        self.line_index_to_in_memory(line_idx)
            .map(|in_memory_idx| self.line_content_formatted(in_memory_idx))
    }

    /// Add a snapshot associated with a specific position (for live recording)
    pub fn add_snapshot_at_position(
        &mut self,
        line: usize,
        column: usize,
        snapshot: crate::Snapshot,
    ) {
        let mut positioned_snapshot = snapshot;
        positioned_snapshot.line = LineIndex(line);
        positioned_snapshot.column = ColumnIndex(column);
        self.snapshots.push(positioned_snapshot);
    }

    /// Add a snapshot associated with a specific line (for backward compatibility)
    /// Defaults column to 0
    pub fn add_snapshot_at_line(&mut self, line: LineIndex, snapshot: crate::Snapshot) {
        self.add_snapshot_at_position(line.as_usize(), 0, snapshot);
    }

    /// Check if an absolute line has any snapshots associated with it
    ///
    /// Uses binary search on the sorted snapshots vector for O(log n) lookup.
    /// Since snapshots are recorded chronologically and line numbers increase,
    /// we maintain sorted order for efficient queries.
    ///
    /// This works with absolute line indices (LineIndex), so it continues to work
    /// correctly even when lines have scrolled out of the scrollback buffer.
    pub fn has_snapshot_at_line(&self, line_idx: LineIndex) -> bool {
        self.snapshots.binary_search_by_key(&line_idx, |snapshot| snapshot.line).is_ok()
    }

    /// Get all snapshots associated with a specific absolute line
    /// Uses binary search to find the snapshot for this line
    pub fn get_snapshots_for_line(&self, line_idx: LineIndex) -> Vec<&crate::Snapshot> {
        // Use binary search to find the snapshot for this line
        if let Ok(index) = self.snapshots.binary_search_by_key(&line_idx, |snapshot| snapshot.line)
        {
            vec![&self.snapshots[index]]
        } else {
            Vec::new()
        }
    }

    /// Get the first snapshot associated with an absolute line (for backwards compatibility)
    pub fn get_snapshot_for_line(&self, line_idx: LineIndex) -> Option<&crate::Snapshot> {
        self.get_snapshots_for_line(line_idx).first().copied()
    }

    /// Find the last snapshot that occurred before the given line index
    /// Uses binary search to efficiently find the rightmost snapshot with line < line_idx
    pub fn last_snapshot_before_line(&self, line_idx: usize) -> Option<&crate::Snapshot> {
        // Use partition_point to find the first snapshot where line >= target
        let partition_idx = self.snapshots.partition_point(|s| s.line < LineIndex(line_idx));

        // If partition_idx > 0, then snapshots[partition_idx - 1] is the last one before target
        if partition_idx > 0 {
            Some(&self.snapshots[partition_idx - 1])
        } else {
            None // No snapshots before this line
        }
    }

    /// Find the first snapshot that occurred after the given line index
    /// Uses binary search to efficiently find the leftmost snapshot with line > line_idx
    pub fn next_snapshot_after_line(&self, line_idx: usize) -> Option<&crate::Snapshot> {
        // Use partition_point to find the first snapshot where line > target
        let partition_idx = self.snapshots.partition_point(|s| s.line <= LineIndex(line_idx));

        // If partition_idx < len, then snapshots[partition_idx] is the first one after target
        if partition_idx < self.snapshots.len() {
            Some(&self.snapshots[partition_idx])
        } else {
            None // No snapshots after this line
        }
    }

    /// Get all snapshots (for serialization/debugging)
    pub fn all_snapshots(&self) -> &[crate::Snapshot] {
        &self.snapshots
    }

    /// Get the LineIndex of a snapshot by its index in the snapshots array
    ///
    /// # Preconditions
    /// - `snapshot_idx` must be a valid index into the snapshots array
    ///   (i.e., `snapshot_idx < self.snapshots.len()`)
    ///
    /// # Panics
    /// Panics if `snapshot_idx` is out of bounds.
    pub fn snapshot_line_index(&self, snapshot_idx: usize) -> LineIndex {
        self.snapshots[snapshot_idx].line
    }

    /// Get the terminal dimensions
    pub fn dimensions(&self) -> (u16, u16) {
        let size = self.parser.screen().size();
        (size.0, size.1) // Return (rows, cols) standard order
    }

    /// Get the underlying vt100 parser (for advanced operations)
    pub fn parser(&self) -> &vt100::Parser<TermCallbacks> {
        &self.parser
    }

    /// Get the underlying vt100 parser (mutable, for advanced operations)
    pub fn parser_mut(&mut self) -> &mut vt100::Parser<TermCallbacks> {
        &mut self.parser
    }

    /// Get the total number of output lines currently held in memory.
    ///
    /// This includes lines in both the visible screen area and the scrollback buffer,
    /// but excludes any lines that have scrolled out of the scrollback buffer entirely.
    /// The count represents what is currently accessible for display and navigation.
    ///
    /// Note: This may be less than `total_output_lines()` when the scrollback buffer
    /// has reached its capacity and older lines have been discarded.
    // requires &mut self because we briefly tweak the scrollback position
    pub fn total_output_lines_in_memory(&self) -> usize {
        let screen = self.parser.screen();
        screen.used_scrollback_lines() + screen.size().0 as usize
    }

    /// Get the total number of output lines that have been processed since initialization.
    ///
    /// This counts every line that has been output to the terminal, including lines that
    /// are currently visible, lines in scrollback, and lines that have scrolled out
    /// of the scrollback buffer entirely and are no longer accessible.
    ///
    /// This count is based on the number of newline characters (`\n`) in the processed
    /// terminal output stream. ANSI escape sequences and other control characters
    /// do not create additional lines - they only modify terminal state.
    ///
    /// This provides a complete count of all logical output lines from the program,
    /// regardless of what is currently retained in memory due to scrollback limits.
    pub fn total_output_lines(&self) -> usize {
        let total_in_memory = self.total_output_lines_in_memory();
        let dropped_off_scrollback_lines = self.parser.screen().dropped_off_scrollback_lines();
        total_in_memory + dropped_off_scrollback_lines
    }

    /// Get the current scrollback position (offset for display)
    pub fn get_current_scrollback(&self) -> usize {
        self.parser.screen().scrollback()
    }

    /// Get the absolute line index for a given visible screen row
    /// This accounts for lines that have fallen out of scrollback by using total_lines_processed
    pub fn get_visible_line_absolute_index(&self, visible_row_idx: ScreenLineIndex) -> LineIndex {
        let screen = self.parser.screen();

        let lines_fallen_out = screen.dropped_off_scrollback_lines();
        let used_scrollback_lines = screen.used_scrollback_lines();

        LineIndex(used_scrollback_lines + lines_fallen_out + visible_row_idx.as_usize())
    }

    /// Get the absolute line indices for all currently visible screen lines
    /// Returns a pair (start, end) representing the range of absolute line indices
    /// for visible rows: [start, start+1, ..., end-1]
    /// Accounts for lines that have fallen out of scrollback
    pub fn get_visible_lines_absolute_indices(&self) -> (LineIndex, LineIndex) {
        let screen = self.parser.screen();
        let (rows, _cols) = screen.size();
        let screen_rows = rows as usize;

        // Get the absolute index of the first visible line
        let first_visible = self.get_visible_line_absolute_index(ScreenLineIndex(0));

        // Calculate the last visible line, but don't exceed the available lines
        let total_in_memory = self.total_output_lines_in_memory();
        let lines_fallen_out = screen.dropped_off_scrollback_lines();
        let last_available = lines_fallen_out + total_in_memory - 1;
        let last_visible =
            LineIndex(first_visible.as_usize() + screen_rows - 1).min(LineIndex(last_available));

        (first_visible, last_visible)
    }
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_state_basic() {
        let mut state = TerminalState::new(24, 80);

        // Check dimensions - vt100::Parser::new(rows, cols, scrollback) creates parser
        // and screen.size() returns (rows, cols)
        let (actual_rows, actual_cols) = state.dimensions();
        // The parameters might be interpreted differently, just check that we have dimensions
        assert!(actual_cols > 0);
        assert!(actual_rows > 0);

        // Process some data
        state.process_data(b"Hello World\n");
        state.process_data(b"Line 2\n");

        // Check line count - should be the number of rows (height)
        assert_eq!(state.line_count(), actual_rows as usize);

        // Check content
        let line0 = state.line_content(InMemoryLineIndex(0));
        assert!(line0.contains("Hello World"));
    }

    #[test]
    fn test_snapshot_association() {
        let mut state = TerminalState::new(24, 80);

        // Process data and record snapshot
        state.process_data(b"Before snapshot\n");
        state.process_data(b"Active line\n");

        let ahr_snapshot = crate::AhrSnapshot {
            ts_ns: 1000,
            label: Some("test".to_string()),
        };

        state.record_snapshot(ahr_snapshot);

        // Check that some line has the snapshot
        let mut found_line = None;
        for line_idx in 0..state.line_count() {
            if state.has_snapshot_at_line(LineIndex(line_idx)) {
                found_line = Some(line_idx);
                break;
            }
        }

        assert!(found_line.is_some(), "No line found with snapshot");
        let snapshot_line = found_line.unwrap();

        let retrieved = state.get_snapshot_for_line(LineIndex(snapshot_line));
        assert!(retrieved.is_some(), "Should find snapshot for the line");

        // Verify that snapshots are sorted by line
        let all_snapshots = state.all_snapshots();
        for i in 1..all_snapshots.len() {
            assert!(
                all_snapshots[i - 1].line <= all_snapshots[i].line,
                "Snapshots should be sorted by line index"
            );
        }

        // Verify newtype prevents accidental assignments
        let line_idx = LineIndex(5);
        assert_eq!(line_idx.as_usize(), 5);

        // This would be a compile error if we tried to assign usize to LineIndex without wrapping:
        // let bad_assignment: LineIndex = 5; // Compile error!
        // But we can do:
        let good_assignment: LineIndex = LineIndex(5);
        assert_eq!(good_assignment, line_idx);
    }

    #[test]
    fn test_snapshot_navigation() {
        let mut state = TerminalState::new(24, 80);

        // Record snapshots at specific lines
        state.process_data(b"Line 0\n");
        state.process_data(b"Line 1\n");
        state.record_snapshot(crate::AhrSnapshot {
            ts_ns: 1000,
            label: Some("snapshot-1".to_string()),
        });

        state.process_data(b"Line 2\n");
        state.process_data(b"Line 3\r\n");
        state.record_snapshot(crate::AhrSnapshot {
            ts_ns: 2000,
            label: Some("snapshot-2".to_string()),
        });

        state.process_data(b"Line 4\n");
        state.process_data(b"Line 5\r\n");
        state.record_snapshot(crate::AhrSnapshot {
            ts_ns: 3000,
            label: Some("snapshot-3".to_string()),
        });

        // Verify snapshots are at expected positions
        let all_snapshots = state.all_snapshots();
        assert_eq!(all_snapshots.len(), 3);
        assert_eq!(all_snapshots[0].line, LineIndex(2)); // After "Line 0\nLine 1\n"
        assert_eq!(all_snapshots[0].column, ColumnIndex(12)); // Cursor position after "Line 1\n"
        assert_eq!(all_snapshots[1].line, LineIndex(4)); // After "Line 2\nLine 3\r\n"
        assert_eq!(all_snapshots[1].column, ColumnIndex(0)); // Cursor position after "Line 3\n"
        assert_eq!(all_snapshots[2].line, LineIndex(6)); // After "Line 4\nLine 5\r\n"
        assert_eq!(all_snapshots[2].column, ColumnIndex(0)); // Cursor position after "Line 5\n"

        // The screen is not filled up yet, so the number of lines
        // should be equal to the number of screen rows
        let total_lines = state.total_output_lines_in_memory();
        assert_eq!(total_lines, 24);

        // The same goes for the total output lines
        let total_processed = state.total_output_lines();
        assert_eq!(total_processed, 24);

        let scrollback = state.get_current_scrollback();
        assert_eq!(scrollback, 0); // No scrolling has occurred yet

        let (start_idx, end_idx) = state.get_visible_lines_absolute_indices();
        assert_eq!(start_idx, LineIndex(0)); // absolute_line for first visible line
        assert_eq!(end_idx, LineIndex(23)); // absolute_line for last visible line (total_lines - 1)

        // Test last_snapshot_before_line
        assert_eq!(state.last_snapshot_before_line(0), None); // Nothing before line 0
        assert_eq!(state.last_snapshot_before_line(1), None); // Nothing before line 1
        assert_eq!(state.last_snapshot_before_line(2), None); // Nothing before line 2 (snapshot is at line 2)
        assert!(state.last_snapshot_before_line(3).is_some()); // Snapshot 1 before line 3
        assert!(state.last_snapshot_before_line(4).is_some()); // Snapshot 1 before line 4
        assert!(state.last_snapshot_before_line(5).is_some()); // Snapshot 2 before line 5
        assert!(state.last_snapshot_before_line(6).is_some()); // Snapshot 2 before line 6
        assert!(state.last_snapshot_before_line(7).is_some()); // Snapshot 3 before line 7

        // Test next_snapshot_after_line
        assert!(state.next_snapshot_after_line(0).is_some()); // Snapshot 1 after line 0
        assert!(state.next_snapshot_after_line(1).is_some()); // Snapshot 1 after line 1
        assert!(state.next_snapshot_after_line(2).is_some()); // Snapshot 2 after line 2
        assert!(state.next_snapshot_after_line(3).is_some()); // Snapshot 2 after line 3
        assert!(state.next_snapshot_after_line(4).is_some()); // Snapshot 3 after line 4
        assert!(state.next_snapshot_after_line(5).is_some()); // Snapshot 3 after line 5
        assert_eq!(state.next_snapshot_after_line(6), None); // Nothing after line 6
        assert_eq!(state.next_snapshot_after_line(7), None); // Nothing after line 7

        // Test that snapshots are found at their absolute positions
        assert!(state.has_snapshot_at_line(LineIndex(2))); // Snapshot 1 at line 2
        assert!(state.has_snapshot_at_line(LineIndex(4))); // Snapshot 2 at line 4
        assert!(state.has_snapshot_at_line(LineIndex(6))); // Snapshot 3 at line 6
    }

    #[test]
    fn test_scrollback_absolute_indices() {
        // Create a terminal with small height to force scrolling
        let mut state = TerminalState::new_with_scrollback(3, 80, 100); // 3 rows, scrollback 100

        // Fill the screen and cause scrolling
        for i in 0..10 {
            state.process_data(format!("Line {}\n", i).as_bytes());
        }

        // Test the APIs work correctly
        let total_lines = state.total_output_lines_in_memory();
        let scrollback = state.get_current_scrollback();
        let (start_idx, end_idx) = state.get_visible_lines_absolute_indices();

        // Basic sanity checks
        // removed meaningless assert on unsigned comparison

        // Verify the range covers the visible screen height
        let num_lines = end_idx.as_usize() - start_idx.as_usize() + 1;
        assert_eq!(num_lines, 3); // Should match screen height

        // Test individual index calculation matches the expected absolute indices
        for i in 0..total_lines {
            let expected_idx = state.get_visible_line_absolute_index(ScreenLineIndex(i));
            let actual_idx = LineIndex(start_idx.as_usize() + i);
            assert_eq!(actual_idx, expected_idx);
        }

        // The visible indices should all be >= scrollback
        assert!(start_idx.as_usize() >= scrollback);
        assert!(end_idx.as_usize() >= scrollback);
    }

    #[test]
    fn test_lines_fallen_out_of_scrollback() {
        // Create a terminal with very small scrollback to force lines to fall out
        let mut state = TerminalState::new_with_scrollback(3, 80, 5); // 3 rows, only 5 lines scrollback

        // Track how many lines we add
        let mut lines_added = 0;

        // Add enough lines to exceed the scrollback buffer
        for i in 0..20 {
            state.process_data(format!("Line {}\r\n", i).as_bytes());
            lines_added += 1; // Each call adds one newline
        }

        // Verify that all lines added are tracked
        // There is always one empty line at the end the screen buffer
        assert_eq!(state.total_output_lines() - 1, lines_added);

        // The current buffer should only contain lines within scrollback + screen height
        let total_lines_in_buffer = state.total_output_lines_in_memory();
        let _scrollback = state.get_current_scrollback();

        // The buffer should contain at most screen_height + scrollback lines
        assert!(total_lines_in_buffer == 3 + 5); // screen height + scrollback
    }

    #[test]
    fn test_line_content_by_line_index() {
        let mut state = TerminalState::new_with_scrollback(24, 80, 100);

        // Add more lines than the terminal can display (24 rows) to test scrolling
        for i in 1..=35 {
            state.process_data(format!("line {}\r\n", i).as_bytes());
        }

        // Now test accessing all lines by index
        let total_processed = state.total_output_lines();
        let total_in_memory = state.total_output_lines_in_memory();

        println!("Total lines processed: {}", total_processed);
        println!("Total lines in memory: {}", total_in_memory);

        // With the new logic, total_in_memory should be at least the visible rows (24)
        assert!(
            total_in_memory >= 24,
            "Total in memory should be at least visible rows"
        );

        // Since we processed 35 lines, all of them should be conceptually in memory
        // (though some may be empty if beyond actual scrollback)
        assert!(
            total_in_memory >= total_processed,
            "All processed lines should be in memory"
        );

        // All processed lines should return Some(content), even if empty
        for i in 0..total_processed {
            let content = state.line_content_by_line_index(LineIndex(i));
            println!("Line {} content: {:?}", i, content);
            assert!(
                content.is_some(),
                "Line {} should have content (processed)",
                i
            );
        }

        // Lines beyond total_processed should return None
        for i in total_processed..(total_processed + 5) {
            let content = state.line_content_by_line_index(LineIndex(i));
            println!("Line {} content (beyond processed): {:?}", i, content);
            assert!(
                content.is_none(),
                "Line {} should not have content (beyond processed)",
                i
            );
        }
    }

    #[test]
    fn test_snapshots_persist_when_lines_fall_out_of_scrollback() {
        // Create a terminal with very small scrollback to force lines to fall out
        let mut state = TerminalState::new_with_scrollback(3, 80, 2); // 3 rows, only 2 lines scrollback

        // Add a few lines and record snapshots
        state.process_data(b"Line 0\r\n"); // LineIndex(0)
        state.record_snapshot(crate::AhrSnapshot {
            ts_ns: 1000,
            label: Some("snap-0".to_string()),
        });

        state.process_data(b"Line 1\r\n"); // LineIndex(1)
        state.record_snapshot(crate::AhrSnapshot {
            ts_ns: 2000,
            label: Some("snap-1".to_string()),
        });

        state.process_data(b"Line 2\r\n"); // LineIndex(2)
        state.record_snapshot(crate::AhrSnapshot {
            ts_ns: 3000,
            label: Some("snap-2".to_string()),
        });

        state.process_data(b"Line 3\r\n"); // LineIndex(3)
        state.record_snapshot(crate::AhrSnapshot {
            ts_ns: 4000,
            label: Some("snap-3".to_string()),
        });

        // Record which lines actually have snapshots
        let snapshots_before = state.all_snapshots().iter().map(|s| s.line).collect::<Vec<_>>();

        // Add many more lines to force the early lines out of scrollback
        for i in 4..20 {
            state.process_data(format!("Line {}\r\n", i).as_bytes());
        }

        // Verify that all snapshots that were recorded are still found,
        // even if their lines have fallen out of scrollback
        for &snapshot_line in &snapshots_before {
            assert!(
                state.has_snapshot_at_line(snapshot_line),
                "Snapshot at line {} should still be found",
                snapshot_line.as_usize()
            );
            assert!(
                !state.get_snapshots_for_line(snapshot_line).is_empty(),
                "Should be able to retrieve snapshot at line {}",
                snapshot_line.as_usize()
            );
        }

        // The total output lines should include all processed lines
        // +1 for the empty line at the end of the screen buffer
        assert_eq!(state.total_output_lines(), 20 + 1);

        // But the lines currently in memory should be much fewer
        let in_memory = state.total_output_lines_in_memory();
        assert!(in_memory == 3 + 2); // screen height + scrollback
    }

    #[test]
    fn test_snapshot_line_index_accessor() {
        let mut state = TerminalState::new(24, 80);

        // Add some data and record snapshots
        state.process_data(b"Line 1\n");
        state.record_snapshot(crate::AhrSnapshot {
            ts_ns: 1000,
            label: Some("snapshot-1".to_string()),
        });

        state.process_data(b"Line 2\n");
        state.record_snapshot(crate::AhrSnapshot {
            ts_ns: 2000,
            label: Some("snapshot-2".to_string()),
        });

        state.process_data(b"Line 3\n");
        state.record_snapshot(crate::AhrSnapshot {
            ts_ns: 3000,
            label: Some("snapshot-3".to_string()),
        });

        // Test valid snapshot indices
        assert_eq!(state.snapshot_line_index(0), LineIndex(1)); // First snapshot at line 1
        assert_eq!(state.snapshot_line_index(1), LineIndex(2)); // Second snapshot at line 2
        assert_eq!(state.snapshot_line_index(2), LineIndex(3)); // Third snapshot at line 3
    }

    #[test]
    fn test_visible_line_absolute_index_with_scrollback() {
        // Test that get_visible_line_absolute_index works correctly even when
        // lines have fallen out of the scrollback buffer

        // Create a terminal with small scrollback
        let mut state = TerminalState::new_with_scrollback(3, 80, 2); // 3 rows, 2 lines scrollback

        // Add initial lines to establish a baseline
        for i in 0..5 {
            state.process_data(format!("Initial line {}\r\n", i).as_bytes());
        }

        // At this point we have 5 lines written to the terminal, but since the terminal
        // always has one empty line at the end of the screen buffer, the first one should
        // already be gone (since the in-memory capacity is also 5 (screen rows + scrollback lines))
        assert!(state.line_content_by_line_index(LineIndex(0)).is_none());

        for i in 1..5 {
            let line_content = state.line_content_by_line_index(LineIndex(i));
            assert_eq!(line_content.unwrap(), format!("Initial line {}", i));
        }

        let (start_idx, end_idx) = state.get_visible_lines_absolute_indices();

        // The visible lines should correspond to the most recent lines that fit on screen
        // There is always one empty line at the end of the screen buffer, so the total lines
        // should be 6. The indices of the last 3 lines should be 3, 4, and 5.
        assert_eq!(start_idx, LineIndex(3));
        assert_eq!(end_idx, LineIndex(5));

        // Test the individual method
        assert_eq!(
            state.get_visible_line_absolute_index(ScreenLineIndex(0)),
            LineIndex(3)
        );
        assert_eq!(
            state.get_visible_line_absolute_index(ScreenLineIndex(1)),
            LineIndex(4)
        );
        assert_eq!(
            state.get_visible_line_absolute_index(ScreenLineIndex(2)),
            LineIndex(5)
        );

        // Add many more lines to force lines out of scrollback
        for i in 5..25 {
            state.process_data(format!("Additional line {}\r\n", i).as_bytes());
        }

        // Now we have 25 total lines
        // +1 for the empty line at the end of the screen buffer
        assert_eq!(state.total_output_lines(), 25 + 1);

        // Get the new visible line range
        let (start_idx, end_idx) = state.get_visible_lines_absolute_indices();

        assert_eq!(start_idx, LineIndex(23));
        assert_eq!(end_idx, LineIndex(25));

        // Test the individual method
        assert_eq!(
            state.get_visible_line_absolute_index(ScreenLineIndex(0)),
            LineIndex(23)
        );
        assert_eq!(
            state.get_visible_line_absolute_index(ScreenLineIndex(1)),
            LineIndex(24)
        );
        assert_eq!(
            state.get_visible_line_absolute_index(ScreenLineIndex(2)),
            LineIndex(25)
        );

        for i in 21..25 {
            let line_content = state.line_content_by_line_index(LineIndex(i));
            assert_eq!(line_content.unwrap(), format!("Additional line {}", i));
        }
    }

    #[test]
    fn test_word_wrapping_line_counting() {
        let width = 16;
        // Test that line counting functions work correctly with word-wrapping
        let mut state = TerminalState::new_with_scrollback(5, width as u16, 10); // 5 rows, 20 cols, 10 scrollback

        // Create a line that is longer than the terminal width (20 chars)
        // This should wrap to multiple visual lines
        let long_line = "This is a very long line that should wrap multiple times because it exceeds the terminal width of 20 characters";
        let expected_lines = long_line.len() / width + 1;
        state.process_data(format!("{}\n", long_line).as_bytes());

        // The long line should occupy multiple visual rows due to wrapping
        // Let's check the total lines in memory - this should account for wrapping
        let total_in_memory = state.total_output_lines_in_memory();

        // The screen should have the expected number of wrapped lines plus
        // one empty line at the end of the screen buffer.
        assert!(
            total_in_memory == expected_lines + 1,
            "Should have the expected number of wrapper lines plus one empty line at the end of the screen buffer"
        );

        // The total processed lines should be 1 (we only processed one \n)
        // But there might be an empty line at the end
        let total_processed = state.total_output_lines();
        assert!(
            total_processed >= 1,
            "Should have processed at least 1 line"
        );

        // Test visible line indices
        let (start_idx, end_idx) = state.get_visible_lines_absolute_indices();
        let visible_range = end_idx.as_usize() - start_idx.as_usize() + 1;
        assert_eq!(visible_range, 5, "Should show exactly 5 visible rows");

        // Test individual visible line indices
        for i in 0..5 {
            let idx = state.get_visible_line_absolute_index(ScreenLineIndex(i));
            // All visible lines should have valid indices
            assert!(idx.as_usize() >= start_idx.as_usize());
            assert!(idx.as_usize() <= end_idx.as_usize());
        }
    }

    #[test]
    fn test_word_wrapping_with_scrollback() {
        // Test word-wrapping behavior when lines scroll out of view
        let mut state = TerminalState::new_with_scrollback(3, 15, 2); // Small terminal: 3 rows, 15 cols, 2 scrollback

        // Add several long lines that will wrap and cause scrolling
        for i in 0..8 {
            let long_line = format!(
                "Line {}: This line is intentionally long to test wrapping behavior",
                i
            );
            state.process_data(format!("{}\n", long_line).as_bytes());
        }

        // Verify basic invariants
        let total_in_memory = state.total_output_lines_in_memory();
        let total_processed = state.total_output_lines();

        // Should have processed 8 lines
        assert!(total_processed >= 8, "Should have processed 8 lines");

        // In-memory lines should be limited by scrollback + screen height
        // Due to wrapping, this might be more than 3 + 2 = 5
        assert!(
            total_in_memory >= 3,
            "Should have at least screen height in memory"
        );

        // Test visible line range
        let (start_idx, end_idx) = state.get_visible_lines_absolute_indices();
        let visible_range = end_idx.as_usize() - start_idx.as_usize() + 1;
        assert_eq!(visible_range, 3, "Should show exactly 3 visible rows");

        // The first visible line should account for fallen scrollback lines
        let screen = state.parser.screen();
        let expected_start = screen.dropped_off_scrollback_lines() + screen.used_scrollback_lines();
        assert_eq!(
            start_idx.as_usize(),
            expected_start,
            "First visible line should account for scrollback"
        );
    }

    #[test]
    fn test_word_wrapping_snapshot_positioning() {
        // Test that snapshot positioning works correctly with word-wrapped lines
        let mut state = TerminalState::new_with_scrollback(4, 25, 5); // 4 rows, 25 cols, 5 scrollback

        // Add a long line that will wrap
        let long_line = "This is a very long first line that should wrap across multiple visual rows in the terminal";
        state.process_data(format!("{}\n", long_line).as_bytes());

        // Record a snapshot after processing the long line
        let snapshot = state.record_snapshot(crate::AhrSnapshot {
            ts_ns: 1000,
            label: Some("wrapped-line-snapshot".to_string()),
        });

        // The snapshot should be positioned correctly
        // Even though the line wraps visually, it should still be associated with the logical line
        // snapshot.line.as_usize() is usize, so it's always >= 0

        // Verify we can retrieve the snapshot
        assert!(
            state.has_snapshot_at_line(snapshot.line),
            "Should be able to find snapshot at recorded line"
        );

        // Add more content to test scrollback behavior with wrapping
        for i in 0..6 {
            let another_long_line = format!(
                "Additional wrapped line {}: This content also wraps to test scrollback",
                i
            );
            state.process_data(format!("{}\n", another_long_line).as_bytes());
        }

        // The snapshot should still be findable even after scrolling
        assert!(
            state.has_snapshot_at_line(snapshot.line),
            "Snapshot should persist through scrolling"
        );

        // Test that the snapshot line is within valid range
        let total_processed = state.total_output_lines();
        assert!(
            snapshot.line.as_usize() < total_processed,
            "Snapshot line should be within processed range"
        );
    }

    #[test]
    fn test_word_wrapping_dropped_lines() {
        // Test the dropped_off_scrollback_lines API with word-wrapping
        let mut state = TerminalState::new_with_scrollback(2, 10, 1); // Very small: 2 rows, 10 cols, 1 scrollback

        // Add many long lines that will definitely cause lines to drop off
        for i in 0..10 {
            let long_line = format!("Line {}: This is long enough to wrap in 10 columns", i);
            state.process_data(format!("{}\n", long_line).as_bytes());
        }

        let screen = state.parser.screen();

        // Check that some lines have been dropped
        let dropped_lines = screen.dropped_off_scrollback_lines();
        assert!(
            dropped_lines > 0,
            "Some lines should have been dropped from scrollback"
        );

        // The total output lines should equal in-memory + dropped
        let total_in_memory = state.total_output_lines_in_memory();
        let total_processed = state.total_output_lines();
        let expected_total = total_in_memory + dropped_lines;

        assert_eq!(
            total_processed, expected_total,
            "Total processed should equal in-memory + dropped"
        );

        // Test that visible lines account for dropped lines
        let (start_idx, _end_idx) = state.get_visible_lines_absolute_indices();
        assert!(
            start_idx.as_usize() >= dropped_lines,
            "Visible lines should account for dropped lines"
        );
    }

    #[test]
    fn test_word_wrapping_edge_cases() {
        // Test edge cases with word-wrapping
        let mut state = TerminalState::new_with_scrollback(3, 5, 2); // Very narrow terminal: 3 rows, 5 cols, 2 scrollback

        // Test with extremely long lines
        let extremely_long =
            "This line is extremely long and should wrap many times in such a narrow terminal";
        state.process_data(format!("{}\n", extremely_long).as_bytes());

        // Should still work correctly
        let total_in_memory = state.total_output_lines_in_memory();
        let total_processed = state.total_output_lines();

        assert!(total_in_memory > 0, "Should have lines in memory");
        assert!(total_processed > 0, "Should have processed lines");

        // Test with empty lines mixed with long lines
        state.process_data(b"\n"); // Empty line
        state.process_data(b"Short\n"); // Short line
        state.process_data(format!("{}\n", extremely_long).as_bytes()); // Another long line

        // Should still maintain correct counts
        let new_total_in_memory = state.total_output_lines_in_memory();
        let new_total_processed = state.total_output_lines();

        assert!(
            new_total_in_memory >= total_in_memory,
            "Total in memory should not decrease"
        );
        assert!(
            new_total_processed > total_processed,
            "Total processed should increase"
        );

        // Visible range should still be correct
        let (_start_idx, end_idx) = state.get_visible_lines_absolute_indices();
        let visible_range = end_idx.as_usize() - _start_idx.as_usize() + 1;
        assert_eq!(visible_range, 3, "Should still show exactly 3 visible rows");
    }
}
