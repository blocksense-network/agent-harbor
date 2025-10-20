// AHR file replay functionality
//
// Replays AHR recordings to reconstruct the final terminal state
// using vt100 terminal emulation.

use crate::reader::{AhrReadEvent, AhrReader};
use crate::snapshots::{Snapshot, SnapshotsReader};
use std::collections::HashMap;
use std::path::Path;

/// Represents a final terminal line after replay
#[derive(Debug, Clone)]
pub struct TerminalLine {
    /// Line index (0-based from top of scrollback)
    pub index: usize,
    /// The text content of the line
    pub text: String,
    /// The highest byte offset that wrote to any cell in this line
    pub last_write_byte: u64,
}

/// Result of replaying an AHR file
#[derive(Debug)]
pub struct ReplayResult {
    /// All terminal lines in final state (top to bottom)
    pub lines: Vec<TerminalLine>,
    /// All snapshots from the recording
    pub snapshots: Vec<Snapshot>,
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
    Line(TerminalLine),
    /// A snapshot event
    Snapshot(Snapshot),
}

/// Result of creating interleaved branch points
#[derive(Debug)]
pub struct BranchPointsResult {
    /// Interleaved items sorted by position (anchor_byte/last_write_byte)
    pub items: Vec<InterleavedItem>,
    /// Total bytes processed
    pub total_bytes: u64,
}

/// Replay an AHR file to reconstruct the final terminal state
pub fn replay_ahr_file<P: AsRef<Path>>(path: P) -> std::io::Result<ReplayResult> {
    let mut reader = AhrReader::new(path)?;

    // Read all events from the recording
    let events = reader.read_all_events()?;

    // Initialize vt100 parser with large scrollback (1M rows as per spec)
    let mut parser = vt100::Parser::new(200, 50, 1_000_000);

    // Track last_write_byte for each row
    let mut row_last_write: HashMap<usize, u64> = HashMap::new();

    // Collect snapshots from the recording
    let mut snapshots = Vec::new();

    // Track previous screen state for change detection
    let mut prev_screen_hashes: HashMap<usize, u64> = HashMap::new();

    // Process all events in chronological order
    let mut total_bytes = 0u64;

    for event in events {
        match event {
            AhrReadEvent::Data {
                data,
                start_byte_off,
                ..
            } => {
                // Process the data through vt100 parser
                parser.process(&data);

                // Calculate hashes for screen state after processing
                let screen = parser.screen();
                let (screen_cols, screen_rows) = screen.size();

                let mut current_hashes = HashMap::new();
                for row_idx in 0..screen_rows as usize {
                    let mut row_hash = 0u64;
                    for col in 0..screen_cols as usize {
                        if let Some(cell) = screen.cell(row_idx as u16, col as u16) {
                            // Simple hash of cell contents
                            for byte in cell.contents().as_bytes() {
                                row_hash = row_hash.wrapping_mul(31).wrapping_add(*byte as u64);
                            }
                        }
                    }
                    current_hashes.insert(row_idx, row_hash);
                }

                // Update row tracking based on changes from previous state
                let current_byte_off = start_byte_off + data.len() as u64;
                update_row_tracking_changed(
                    &mut row_last_write,
                    &prev_screen_hashes,
                    &current_hashes,
                    current_byte_off,
                );

                // Update previous hashes for next iteration
                prev_screen_hashes = current_hashes;

                total_bytes = current_byte_off;
            }
            AhrReadEvent::Resize { cols, rows, .. } => {
                // Resize the terminal
                parser.set_size(rows, cols);
            }
            AhrReadEvent::Snapshot {
                ts_ns,
                snapshot_id,
                anchor_byte,
                label,
            } => {
                // Collect snapshot metadata
                snapshots.push(Snapshot {
                    id: snapshot_id,
                    ts_ns,
                    label,
                    anchor_byte,
                    kind: Some("ipc".to_string()),
                });
            }
        }
    }

    // Extract final terminal lines
    let lines = extract_terminal_lines(&parser, &row_last_write);

    // Get initial terminal size (use final size if no resize events)
    let (rows, cols) = parser.screen().size();
    let initial_cols = cols;
    let initial_rows = rows;

    Ok(ReplayResult {
        lines,
        snapshots,
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
    // First replay the AHR file to get terminal lines and snapshots
    let replay_result = replay_ahr_file(&ahr_path)?;

    // Interleave lines and snapshots by position
    let result = interleave_by_position(
        &replay_result.lines,
        &replay_result.snapshots,
        replay_result.total_bytes,
    );

    Ok(result)
}

/// Interleave terminal lines and snapshots by their position values
pub fn interleave_by_position(
    lines: &[TerminalLine],
    snapshots: &[Snapshot],
    total_bytes: u64,
) -> BranchPointsResult {
    // Create a combined list of items with their positions
    let mut items_with_positions: Vec<(u64, InterleavedItem)> = Vec::new();

    // Add all lines
    for line in lines {
        let position = line.last_write_byte;
        items_with_positions.push((position, InterleavedItem::Line(line.clone())));
    }

    // Add all snapshots
    for snapshot in snapshots {
        let position = snapshot.anchor_byte;
        items_with_positions.push((position, InterleavedItem::Snapshot(snapshot.clone())));
    }

    // Sort by position, then by type (lines before snapshots at same position)
    items_with_positions.sort_by(|a, b| {
        a.0.cmp(&b.0).then_with(|| match (&a.1, &b.1) {
            (InterleavedItem::Line(_), InterleavedItem::Snapshot(_)) => std::cmp::Ordering::Less,
            (InterleavedItem::Snapshot(_), InterleavedItem::Line(_)) => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        })
    });

    // Extract just the items
    let items: Vec<InterleavedItem> =
        items_with_positions.into_iter().map(|(_, item)| item).collect();

    BranchPointsResult { items, total_bytes }
}

/// Update row tracking after processing data by detecting which rows changed
fn update_row_tracking_changed(
    row_last_write: &mut HashMap<usize, u64>,
    prev_hashes: &HashMap<usize, u64>,
    current_hashes: &HashMap<usize, u64>,
    data_end_byte: u64,
) {
    // Compare current hashes with previous hashes to detect changes
    for (row_idx, &current_hash) in current_hashes {
        let prev_hash = prev_hashes.get(row_idx).unwrap_or(&0);
        if current_hash != *prev_hash {
            // Row changed, update its last write byte
            row_last_write.insert(*row_idx, data_end_byte);
        }
    }
}

/// Legacy function for backward compatibility
fn update_row_tracking(
    _parser: &vt100::Parser,
    row_last_write: &mut HashMap<usize, u64>,
    data_start_byte: u64,
    data_end_byte: u64,
) {
    // This function is no longer used with the new change-tracking approach
    // Keep it for any external callers, but it uses the old conservative approach
    // For now, just update a placeholder - this should not be called
    row_last_write.insert(0, data_end_byte);
}

/// Extract terminal lines from the final vt100 state
fn extract_terminal_lines(
    parser: &vt100::Parser,
    row_last_write: &HashMap<usize, u64>,
) -> Vec<TerminalLine> {
    let screen = parser.screen();
    let (_, screen_rows) = screen.size();

    let mut lines = Vec::new();

    // Extract lines from scrollback + screen (top to bottom)
    let (screen_cols, screen_rows) = screen.size();

    for row_idx in 0..screen_rows as usize {
        let mut line_content = String::new();

        // Collect all cells in this row
        for col in 0..screen_cols as usize {
            if let Some(cell) = screen.cell(row_idx as u16, col as u16) {
                let contents = cell.contents();
                if contents.is_empty() {
                    // Check if we've reached the end of content
                    if col > 0 {
                        break; // Stop at first empty cell after content
                    }
                } else {
                    line_content.push_str(&contents);
                }
            } else {
                break; // No more cells in this row
            }
        }

        // Only include non-empty lines
        if !line_content.trim().is_empty() {
            let last_write_byte = row_last_write.get(&row_idx).copied().unwrap_or(0);

            lines.push(TerminalLine {
                index: row_idx,
                text: line_content,
                last_write_byte,
            });
        }
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_replay_empty_file() {
        let temp_file = NamedTempFile::new().unwrap();
        let result = replay_ahr_file(temp_file.path());

        // Empty file should not error but have empty results
        assert!(result.is_ok());
        let replay = result.unwrap();
        assert_eq!(replay.lines.len(), 0);
        assert_eq!(replay.total_bytes, 0);
    }

    #[test]
    fn test_replay_invalid_file() {
        // Try to replay a non-existent file
        let result = replay_ahr_file("/non/existent/file.ahr");
        assert!(result.is_err());
    }
}
