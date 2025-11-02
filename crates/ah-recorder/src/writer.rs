// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

// Block writer for .ahr files with Brotli compression
//
// Accumulates records in memory and flushes them as compressed blocks
// when size or time thresholds are exceeded.

use crate::format::{AhrBlockHeader, Record};
use anyhow::{Context, Result};
use brotli::enc::BrotliEncoderParams;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::{debug, trace};

/// Default Brotli quality level (q=4 for fast/compact balance)
const DEFAULT_BROTLI_QUALITY: u32 = 4;

/// Default maximum uncompressed block size (256 KiB)
const DEFAULT_MAX_BLOCK_SIZE: usize = 256 * 1024;

/// Default maximum time before flushing a block (250ms)
const DEFAULT_MAX_BLOCK_TIME: Duration = Duration::from_millis(250);

/// Configuration for the block writer
#[derive(Debug, Clone)]
pub struct WriterConfig {
    /// Brotli compression quality (0-11)
    pub brotli_quality: u32,
    /// Maximum uncompressed block size in bytes before flushing
    pub max_block_size: usize,
    /// Maximum time before flushing a block
    pub max_block_time: Duration,
}

impl Default for WriterConfig {
    fn default() -> Self {
        Self {
            brotli_quality: DEFAULT_BROTLI_QUALITY,
            max_block_size: DEFAULT_MAX_BLOCK_SIZE,
            max_block_time: DEFAULT_MAX_BLOCK_TIME,
        }
    }
}

impl WriterConfig {
    pub fn with_brotli_quality(mut self, quality: u32) -> Self {
        self.brotli_quality = quality.min(11);
        self
    }

    pub fn with_max_block_size(mut self, size: usize) -> Self {
        self.max_block_size = size;
        self
    }

    pub fn with_max_block_time(mut self, duration: Duration) -> Self {
        self.max_block_time = duration;
        self
    }
}

/// Block writer for .ahr files
///
/// Accumulates records in memory and flushes them as independent Brotli-compressed
/// blocks to provide crash safety and bounded replay latency.
pub struct AhrWriter {
    /// Output file handle
    file: File,
    /// Configuration
    config: WriterConfig,
    /// Current accumulated records
    current_records: Vec<Record>,
    /// Current accumulated uncompressed size
    current_size: usize,
    /// Timestamp of the first record in current block
    current_start_ts: Option<u64>,
    /// Byte offset of the first record in current block
    current_start_byte_off: u64,
    /// Time when current block was started
    block_start_time: Instant,
    /// Global PTY byte offset (cumulative count of DATA bytes written)
    global_byte_off: u64,
    /// Whether this writer has been finalized
    finalized: bool,
}

impl AhrWriter {
    /// Create a new writer with the given file path and config
    pub fn create<P: AsRef<Path>>(path: P, config: WriterConfig) -> Result<Self> {
        let file = File::create(path.as_ref()).context("Failed to create .ahr file")?;

        debug!(
            path = ?path.as_ref(),
            brotli_quality = config.brotli_quality,
            max_block_size = config.max_block_size,
            "Created AHR writer"
        );

        Ok(Self {
            file,
            config,
            current_records: Vec::new(),
            current_size: 0,
            current_start_ts: None,
            current_start_byte_off: 0,
            block_start_time: Instant::now(),
            global_byte_off: 0,
            finalized: false,
        })
    }

    /// Append a record to the current block
    ///
    /// May trigger a block flush if size or time thresholds are exceeded.
    pub fn append_record(&mut self, record: Record) -> Result<()> {
        if self.finalized {
            anyhow::bail!("Cannot append to finalized writer");
        }

        // Update global byte offset for DATA records
        if let Record::Data(ref data) = record {
            self.global_byte_off += data.bytes.len() as u64;
        }

        // Track the first timestamp in this block
        if self.current_start_ts.is_none() {
            self.current_start_ts = Some(record.ts_ns());
        }

        // Estimate serialized size (conservative)
        let record_size = match &record {
            Record::Data(r) => 12 + 8 + 4 + r.bytes.len(),
            Record::Resize(_) => 12 + 4,
            Record::Input(r) => 12 + 4 + r.bytes.len(),
            Record::Mark(_) => 12 + 8,
            Record::Snapshot(r) => 12 + 8 + 8 + 2 + r.label.len(), // header + snapshot_id + anchor_byte + label_len + label bytes
        };

        self.current_records.push(record);
        self.current_size += record_size;

        trace!(
            record_count = self.current_records.len(),
            current_size = self.current_size,
            "Appended record"
        );

        // Check if we should flush based on size or time
        let should_flush_size = self.current_size >= self.config.max_block_size;
        let should_flush_time = self.block_start_time.elapsed() >= self.config.max_block_time;

        if should_flush_size || should_flush_time {
            let reason = if should_flush_size { "size" } else { "time" };
            debug!(
                reason = reason,
                size = self.current_size,
                elapsed_ms = self.block_start_time.elapsed().as_millis(),
                "Flushing block"
            );
            self.flush_block(false)?;
        }

        Ok(())
    }

    /// Flush the current block to disk
    ///
    /// If `is_final` is true, marks the block as the last in the stream.
    fn flush_block(&mut self, is_final: bool) -> Result<()> {
        if self.current_records.is_empty() {
            return Ok(());
        }

        // Serialize all records to a buffer
        let mut uncompressed = Vec::with_capacity(self.current_size);
        for record in &self.current_records {
            record.write_to(&mut uncompressed).context("Failed to serialize record")?;
        }

        let uncompressed_len = uncompressed.len();

        // Compress with Brotli
        let mut compressed = Vec::new();
        let params = BrotliEncoderParams {
            quality: self.config.brotli_quality as i32,
            ..Default::default()
        };

        brotli::BrotliCompress(&mut &uncompressed[..], &mut compressed, &params)
            .context("Brotli compression failed")?;

        let compressed_len = compressed.len();

        // Build and write block header
        let mut header =
            AhrBlockHeader::new(self.current_start_ts.unwrap(), self.current_start_byte_off);
        header.uncompressed_len = uncompressed_len as u32;
        header.compressed_len = compressed_len as u32;
        header.record_count = self.current_records.len() as u32;
        header.set_last_block(is_final);

        header.write_to(&mut self.file).context("Failed to write block header")?;

        // Write compressed payload
        self.file.write_all(&compressed).context("Failed to write compressed block")?;

        debug!(
            record_count = header.record_count,
            uncompressed_len = uncompressed_len,
            compressed_len = compressed_len,
            ratio = (compressed_len as f64 / uncompressed_len as f64),
            is_final = is_final,
            "Flushed block"
        );

        // Reset state for next block
        self.current_start_byte_off = self.global_byte_off;
        self.current_records.clear();
        self.current_size = 0;
        self.current_start_ts = None;
        self.block_start_time = Instant::now();

        Ok(())
    }

    /// Finalize the writer, flushing any remaining data and marking the last block
    pub fn finalize(mut self) -> Result<()> {
        if self.finalized {
            return Ok(());
        }

        self.flush_block(true)?;
        self.file.sync_all().context("Failed to sync file")?;
        self.finalized = true;

        debug!("Finalized AHR writer");
        Ok(())
    }

    /// Get the current global byte offset
    pub fn global_byte_off(&self) -> u64 {
        self.global_byte_off
    }
}

impl Drop for AhrWriter {
    fn drop(&mut self) {
        if !self.finalized {
            // Best-effort flush on drop
            if let Err(e) = self.flush_block(true) {
                eprintln!("Warning: Failed to flush AHR writer on drop: {}", e);
            }
            if let Err(e) = self.file.sync_all() {
                eprintln!("Warning: Failed to sync AHR file on drop: {}", e);
            }
        }
    }
}

/// Get the current system time as nanoseconds since UNIX epoch
pub fn now_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("System time before UNIX epoch")
        .as_nanos() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::{RecData, RecResize};
    use tempfile::NamedTempFile;

    #[ah_test_utils::logged_test]
    fn test_writer_basic() -> Result<()> {
        let temp = NamedTempFile::new()?;
        let path = temp.path().to_path_buf();

        let config = WriterConfig::default();
        let mut writer = AhrWriter::create(&path, config)?;

        // Write some records
        let ts = now_ns();
        writer.append_record(Record::Data(RecData::new(ts, 0, b"Hello, world!".to_vec())))?;
        writer.append_record(Record::Resize(RecResize::new(ts + 1000, 80, 24)))?;

        writer.finalize()?;

        // Verify file was created and has content
        let metadata = std::fs::metadata(&path)?;
        assert!(metadata.len() > 0);

        Ok(())
    }

    #[ah_test_utils::logged_test]
    fn test_writer_large_block() -> Result<()> {
        let temp = NamedTempFile::new()?;
        let path = temp.path().to_path_buf();

        // Small block size to force flushing
        let config = WriterConfig::default().with_max_block_size(1024);
        let mut writer = AhrWriter::create(&path, config)?;

        let ts = now_ns();

        // Write enough data to trigger multiple blocks
        for i in 0..100 {
            let data = format!("Record number {} with some padding text", i);
            writer.append_record(Record::Data(RecData::new(
                ts + i * 1000,
                writer.global_byte_off(),
                data.into_bytes(),
            )))?;
        }

        writer.finalize()?;

        let metadata = std::fs::metadata(&path)?;
        assert!(metadata.len() > 0);

        Ok(())
    }

    #[ah_test_utils::logged_test]
    fn test_writer_byte_offset_tracking() -> Result<()> {
        let temp = NamedTempFile::new()?;
        let path = temp.path().to_path_buf();

        let config = WriterConfig::default();
        let mut writer = AhrWriter::create(&path, config)?;

        let ts = now_ns();

        // Write some data and verify byte offset tracking
        assert_eq!(writer.global_byte_off(), 0);

        writer.append_record(Record::Data(RecData::new(ts, 0, b"hello".to_vec())))?;
        assert_eq!(writer.global_byte_off(), 5);

        writer.append_record(Record::Data(RecData::new(ts + 1000, 5, b" world".to_vec())))?;
        assert_eq!(writer.global_byte_off(), 11);

        writer.finalize()?;

        Ok(())
    }
}
