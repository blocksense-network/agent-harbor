// Snapshots JSONL writer for instruction tracking
//
// Maintains an append-only NDJSON log of snapshot events that mark
// moments in time within the PTY stream for later instruction injection.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use tracing::{debug, trace};

/// A snapshot event marking a moment in the PTY stream
///
/// Snapshots are anchored to byte offsets in the PTY output and can be
/// associated with instructions or checkpoints for later time-travel functionality.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Snapshot {
    /// Unique snapshot ID (monotonically increasing)
    pub id: u64,
    /// Timestamp in nanoseconds since UNIX epoch
    pub ts_ns: u64,
    /// Label for this snapshot (optional, for human readability)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Kind of snapshot (e.g., "auto", "manual", "checkpoint")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Byte offset anchor in the PTY stream
    pub anchor_byte: u64,
}

impl Snapshot {
    /// Create a new snapshot with the given parameters
    pub fn new(id: u64, ts_ns: u64, anchor_byte: u64) -> Self {
        Self {
            id,
            ts_ns,
            label: None,
            kind: None,
            anchor_byte,
        }
    }

    /// Set the label for this snapshot
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Set the kind for this snapshot
    pub fn with_kind(mut self, kind: impl Into<String>) -> Self {
        self.kind = Some(kind.into());
        self
    }
}

/// Writer for snapshots in JSONL format
///
/// Maintains an append-only log of snapshot events with atomic writes.
pub struct SnapshotsWriter {
    /// Buffered file writer
    writer: BufWriter<File>,
    /// Next snapshot ID to assign
    next_id: u64,
}

impl SnapshotsWriter {
    /// Create a new snapshots writer
    pub fn create<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path.as_ref())
            .context("Failed to create snapshots file")?;

        debug!(path = ?path.as_ref(), "Created snapshots writer");

        Ok(Self {
            writer: BufWriter::new(file),
            next_id: 0,
        })
    }

    /// Append a snapshot to the log
    ///
    /// Returns the assigned snapshot ID.
    pub fn append(&mut self, snapshot: &Snapshot) -> Result<u64> {
        // Serialize to JSON
        let json = serde_json::to_string(snapshot)
            .context("Failed to serialize snapshot")?;

        // Write with newline
        writeln!(self.writer, "{}", json)
            .context("Failed to write snapshot")?;

        // Flush to ensure durability (each snapshot is atomic)
        self.writer.flush()
            .context("Failed to flush snapshots file")?;

        trace!(id = snapshot.id, anchor_byte = snapshot.anchor_byte, "Wrote snapshot");

        Ok(snapshot.id)
    }

    /// Create and append a new snapshot with automatic ID assignment
    pub fn create_snapshot(
        &mut self,
        ts_ns: u64,
        anchor_byte: u64,
        label: Option<String>,
        kind: Option<String>,
    ) -> Result<Snapshot> {
        let id = self.next_id;
        self.next_id += 1;

        let mut snapshot = Snapshot::new(id, ts_ns, anchor_byte);
        if let Some(l) = label {
            snapshot.label = Some(l);
        }
        if let Some(k) = kind {
            snapshot.kind = Some(k);
        }

        self.append(&snapshot)?;

        Ok(snapshot)
    }

    /// Get the next ID that will be assigned
    pub fn next_id(&self) -> u64 {
        self.next_id
    }

    /// Finalize the writer, flushing any buffered data
    pub fn finalize(mut self) -> Result<()> {
        self.writer.flush()
            .context("Failed to flush snapshots writer")?;
        self.writer.into_inner()
            .map_err(|e| anyhow::anyhow!("Failed to finalize writer: {}", e))?
            .sync_all()
            .context("Failed to sync snapshots file")?;

        debug!("Finalized snapshots writer");
        Ok(())
    }
}

/// Thread-safe wrapper for SnapshotsWriter
pub type SharedSnapshotsWriter = Arc<Mutex<SnapshotsWriter>>;

/// Helper to create a shared snapshots writer
pub fn create_shared_writer<P: AsRef<Path>>(path: P) -> Result<SharedSnapshotsWriter> {
    Ok(Arc::new(Mutex::new(SnapshotsWriter::create(path)?)))
}

/// Reader for snapshots from JSONL format
pub struct SnapshotsReader {
    snapshots: Vec<Snapshot>,
}

impl SnapshotsReader {
    /// Load all snapshots from a file
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())
            .context("Failed to read snapshots file")?;

        let mut snapshots = Vec::new();
        for (line_num, line) in content.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }

            let snapshot: Snapshot = serde_json::from_str(line)
                .with_context(|| format!("Failed to parse snapshot at line {}", line_num + 1))?;

            snapshots.push(snapshot);
        }

        debug!(count = snapshots.len(), "Loaded snapshots");

        Ok(Self { snapshots })
    }

    /// Get all snapshots
    pub fn snapshots(&self) -> &[Snapshot] {
        &self.snapshots
    }

    /// Find snapshot by ID
    pub fn find_by_id(&self, id: u64) -> Option<&Snapshot> {
        self.snapshots.iter().find(|s| s.id == id)
    }

    /// Find snapshots near a given byte offset
    ///
    /// Returns snapshots within the specified distance of the offset,
    /// sorted by proximity.
    pub fn find_near(&self, byte_off: u64, max_distance: u64) -> Vec<&Snapshot> {
        let mut nearby: Vec<_> = self
            .snapshots
            .iter()
            .filter_map(|s| {
                let dist = if s.anchor_byte > byte_off {
                    s.anchor_byte - byte_off
                } else {
                    byte_off - s.anchor_byte
                };

                if dist <= max_distance {
                    Some((dist, s))
                } else {
                    None
                }
            })
            .collect();

        nearby.sort_by_key(|(dist, _)| *dist);
        nearby.into_iter().map(|(_, s)| s).collect()
    }

    /// Find the snapshot closest to a given byte offset
    pub fn find_closest(&self, byte_off: u64) -> Option<&Snapshot> {
        self.snapshots
            .iter()
            .min_by_key(|s| {
                if s.anchor_byte > byte_off {
                    s.anchor_byte - byte_off
                } else {
                    byte_off - s.anchor_byte
                }
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::writer::now_ns;
    use tempfile::NamedTempFile;

    #[test]
    fn test_snapshots_writer_basic() -> Result<()> {
        let temp = NamedTempFile::new()?;
        let path = temp.path().to_path_buf();

        let mut writer = SnapshotsWriter::create(&path)?;

        let ts = now_ns();
        writer.create_snapshot(ts, 100, Some("test".to_string()), Some("manual".to_string()))?;
        writer.create_snapshot(ts + 1000, 200, None, Some("auto".to_string()))?;

        writer.finalize()?;

        // Verify file contents
        let content = std::fs::read_to_string(&path)?;
        assert!(content.contains("\"anchor_byte\":100"));
        assert!(content.contains("\"anchor_byte\":200"));

        Ok(())
    }

    #[test]
    fn test_snapshots_reader_basic() -> Result<()> {
        let temp = NamedTempFile::new()?;
        let path = temp.path().to_path_buf();

        // Write some snapshots
        let mut writer = SnapshotsWriter::create(&path)?;
        let ts = now_ns();
        writer.create_snapshot(ts, 100, Some("first".to_string()), None)?;
        writer.create_snapshot(ts + 1000, 200, Some("second".to_string()), None)?;
        writer.finalize()?;

        // Read them back
        let reader = SnapshotsReader::load(&path)?;
        assert_eq!(reader.snapshots().len(), 2);

        let first = reader.find_by_id(0).unwrap();
        assert_eq!(first.anchor_byte, 100);
        assert_eq!(first.label, Some("first".to_string()));

        Ok(())
    }

    #[test]
    fn test_find_closest() -> Result<()> {
        let temp = NamedTempFile::new()?;
        let path = temp.path().to_path_buf();

        let mut writer = SnapshotsWriter::create(&path)?;
        let ts = now_ns();
        writer.create_snapshot(ts, 100, None, None)?;
        writer.create_snapshot(ts + 1000, 500, None, None)?;
        writer.create_snapshot(ts + 2000, 1000, None, None)?;
        writer.finalize()?;

        let reader = SnapshotsReader::load(&path)?;

        // Test various positions
        assert_eq!(reader.find_closest(50).unwrap().anchor_byte, 100);
        assert_eq!(reader.find_closest(150).unwrap().anchor_byte, 100);
        // 300 is equidistant from 100 (distance 200) and 500 (distance 200)
        // min_by_key will return the first one it finds, which is 100
        assert_eq!(reader.find_closest(300).unwrap().anchor_byte, 100);
        assert_eq!(reader.find_closest(750).unwrap().anchor_byte, 500);
        assert_eq!(reader.find_closest(1200).unwrap().anchor_byte, 1000);

        Ok(())
    }

    #[test]
    fn test_find_near() -> Result<()> {
        let temp = NamedTempFile::new()?;
        let path = temp.path().to_path_buf();

        let mut writer = SnapshotsWriter::create(&path)?;
        let ts = now_ns();
        writer.create_snapshot(ts, 100, None, None)?;
        writer.create_snapshot(ts + 1000, 200, None, None)?;
        writer.create_snapshot(ts + 2000, 1000, None, None)?;
        writer.finalize()?;

        let reader = SnapshotsReader::load(&path)?;

        // Find snapshots near 150 with distance 100
        let near = reader.find_near(150, 100);
        assert_eq!(near.len(), 2); // Should find 100 and 200

        // Find snapshots near 150 with distance 30
        let near = reader.find_near(150, 30);
        assert_eq!(near.len(), 0); // None within distance

        Ok(())
    }
}
