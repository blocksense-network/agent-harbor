// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

// AHR file reader for replay functionality
//
// Reads compressed AHR blocks, decompresses them, and yields individual records
// for replay or analysis.

use crate::format::{AhrBlockHeader, Record};
// use byteorder::{LittleEndian, ReadBytesExt}; // Unused
use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::Path;

use crate::ahr_events::*;

/// AHR file reader that decompresses blocks and yields records
pub struct AhrReader {
    file: BufReader<File>,
}

impl AhrReader {
    /// Create a new AHR reader for the given file path
    pub fn new<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let file = File::open(path)?;
        let file = BufReader::new(file);
        Ok(Self { file })
    }

    /// Read all events from the AHR file in chronological order
    pub fn read_all_events(&mut self) -> io::Result<Vec<AhrEvent>> {
        let mut events = Vec::new();

        while let Some(block_events) = self.read_next_block()? {
            events.extend(block_events);
        }

        Ok(events)
    }

    /// Read the next block and return all events from it
    pub fn read_next_block(&mut self) -> io::Result<Option<Vec<AhrEvent>>> {
        // Try to read the next block header
        match self.read_block_header() {
            Ok(header) => {
                let events = self.read_block_events(&header)?;
                Ok(Some(events))
            }
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                // End of file reached
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    /// Read a single block header
    fn read_block_header(&mut self) -> io::Result<AhrBlockHeader> {
        AhrBlockHeader::read_from(&mut self.file)
    }

    /// Read all events from a block given its header
    fn read_block_events(&mut self, header: &AhrBlockHeader) -> io::Result<Vec<AhrEvent>> {
        // Read the compressed payload
        let mut compressed_data = vec![0u8; header.compressed_len as usize];
        self.file.read_exact(&mut compressed_data)?;

        // Decompress the data
        let decompressed =
            self.decompress_block(&compressed_data, header.uncompressed_len as usize)?;

        // Parse records from decompressed data
        self.parse_records(&decompressed)
    }

    /// Decompress a block's data
    fn decompress_block(&self, compressed: &[u8], expected_len: usize) -> io::Result<Vec<u8>> {
        // use std::io::Write; // Unused
        let mut decompressed = Vec::with_capacity(expected_len);

        // Create Brotli decoder
        let mut decoder = brotli::Decompressor::new(compressed, 4096);

        // Read all decompressed data
        io::copy(&mut decoder, &mut decompressed)?;

        if decompressed.len() != expected_len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Decompressed size mismatch: expected {}, got {}",
                    expected_len,
                    decompressed.len()
                ),
            ));
        }

        Ok(decompressed)
    }

    /// Parse records from decompressed block data
    fn parse_records(&self, data: &[u8]) -> io::Result<Vec<AhrEvent>> {
        let mut events = Vec::new();
        let mut offset = 0;

        while offset < data.len() {
            let (record, consumed) = Record::parse_from_bytes(&data[offset..])?;
            offset += consumed;

            let event = match record {
                Record::Data(rec) => AhrEvent::Data {
                    ts_ns: rec.header.ts_ns,
                    start_byte_off: rec.start_byte_off,
                    data: rec.bytes,
                },
                Record::Resize(rec) => AhrEvent::Resize {
                    ts_ns: rec.header.ts_ns,
                    cols: rec.cols,
                    rows: rec.rows,
                },
                Record::Snapshot(rec) => AhrEvent::Snapshot(AhrSnapshot {
                    ts_ns: rec.header.ts_ns,
                    label: if !rec.label.is_empty() {
                        Some(rec.label)
                    } else {
                        None
                    },
                }),
                Record::Input(_) | Record::Mark(_) => {
                    // Skip input and mark records for now
                    continue;
                }
            };

            events.push(event);
        }

        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_ahr_reader_creation() {
        let temp_file = NamedTempFile::new().unwrap();
        let reader = AhrReader::new(temp_file.path());
        assert!(reader.is_ok());
    }

    #[test]
    fn test_empty_file() {
        let temp_file = NamedTempFile::new().unwrap();
        let mut reader = AhrReader::new(temp_file.path()).unwrap();

        let events = reader.read_all_events();
        assert!(events.is_ok());
        assert_eq!(events.unwrap().len(), 0);
    }
}
