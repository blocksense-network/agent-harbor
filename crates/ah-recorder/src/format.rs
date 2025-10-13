// File format definitions for .ahr (Agent Harbor Recording) files
//
// See: specs/Public/ah-agent-record.md for complete specification
// Format: Sequence of independent Brotli-compressed blocks with timestamped PTY records

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::{self, Read, Write};

/// Magic bytes for AHR block header: 'AHRC' = 0x43524841
pub const AHR_MAGIC: u32 = 0x43524841;

/// Current AHR format version
pub const AHR_VERSION: u16 = 1;

/// Size of the block header in bytes
pub const BLOCK_HEADER_SIZE: usize = 48;

/// Record type tags
pub const REC_DATA: u8 = 0;
pub const REC_RESIZE: u8 = 1;
pub const REC_INPUT: u8 = 2;
pub const REC_MARK: u8 = 3;
pub const REC_SNAPSHOT: u8 = 4;

/// Block header (48 bytes, little-endian)
/// Describes a single Brotli-compressed block of records
#[derive(Debug, Clone, PartialEq)]
pub struct AhrBlockHeader {
    /// Magic bytes: 'AHRC' = 0x43524841
    pub magic: u32,
    /// Format version (currently 1)
    pub version: u16,
    /// Size of this header structure in bytes
    pub header_len: u16,
    /// Wall clock timestamp (ns) of the first record in this block
    pub start_ts_ns: u64,
    /// PTY byte offset immediately BEFORE the first DATA record
    pub start_byte_off: u64,
    /// Bytes of the Records Segment before compression
    pub uncompressed_len: u32,
    /// Bytes of the Brotli payload that follows this header
    pub compressed_len: u32,
    /// Number of records in the segment
    pub record_count: u32,
    /// Flags: bit 0 = is_last_block (best-effort)
    pub flags: u8,
    /// Reserved for future use (must be zero)
    pub reserved: [u8; 7],
}

impl AhrBlockHeader {
    /// Create a new block header with default values
    pub fn new(start_ts_ns: u64, start_byte_off: u64) -> Self {
        Self {
            magic: AHR_MAGIC,
            version: AHR_VERSION,
            header_len: BLOCK_HEADER_SIZE as u16,
            start_ts_ns,
            start_byte_off,
            uncompressed_len: 0,
            compressed_len: 0,
            record_count: 0,
            flags: 0,
            reserved: [0; 7],
        }
    }

    /// Mark this as the last block in the stream
    pub fn set_last_block(&mut self, is_last: bool) {
        if is_last {
            self.flags |= 0x01;
        } else {
            self.flags &= !0x01;
        }
    }

    /// Check if this is marked as the last block
    pub fn is_last_block(&self) -> bool {
        self.flags & 0x01 != 0
    }

    /// Write the header to a writer
    pub fn write_to<W: Write>(&self, mut w: W) -> io::Result<()> {
        w.write_u32::<LittleEndian>(self.magic)?;
        w.write_u16::<LittleEndian>(self.version)?;
        w.write_u16::<LittleEndian>(self.header_len)?;
        w.write_u64::<LittleEndian>(self.start_ts_ns)?;
        w.write_u64::<LittleEndian>(self.start_byte_off)?;
        w.write_u32::<LittleEndian>(self.uncompressed_len)?;
        w.write_u32::<LittleEndian>(self.compressed_len)?;
        w.write_u32::<LittleEndian>(self.record_count)?;
        w.write_u8(self.flags)?;
        w.write_all(&self.reserved)?;
        Ok(())
    }

    /// Read a header from a reader
    pub fn read_from<R: Read>(mut r: R) -> io::Result<Self> {
        let magic = r.read_u32::<LittleEndian>()?;
        if magic != AHR_MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid AHR magic: expected 0x{:08X}, got 0x{:08X}", AHR_MAGIC, magic),
            ));
        }

        let version = r.read_u16::<LittleEndian>()?;
        if version > AHR_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Unsupported AHR version: {} (max supported: {})", version, AHR_VERSION),
            ));
        }

        let header_len = r.read_u16::<LittleEndian>()?;
        let start_ts_ns = r.read_u64::<LittleEndian>()?;
        let start_byte_off = r.read_u64::<LittleEndian>()?;
        let uncompressed_len = r.read_u32::<LittleEndian>()?;
        let compressed_len = r.read_u32::<LittleEndian>()?;
        let record_count = r.read_u32::<LittleEndian>()?;
        let flags = r.read_u8()?;
        let mut reserved = [0u8; 7];
        r.read_exact(&mut reserved)?;

        Ok(Self {
            magic,
            version,
            header_len,
            start_ts_ns,
            start_byte_off,
            uncompressed_len,
            compressed_len,
            record_count,
            flags,
            reserved,
        })
    }
}

/// Common record header (12 bytes)
#[derive(Debug, Clone, PartialEq)]
pub struct RecHeader {
    /// Record type tag (REC_DATA, REC_RESIZE, REC_INPUT, REC_MARK)
    pub tag: u8,
    /// Padding for alignment (must be zero)
    pub pad: [u8; 3],
    /// Event timestamp (CLOCK_REALTIME in nanoseconds)
    pub ts_ns: u64,
}

impl RecHeader {
    pub fn new(tag: u8, ts_ns: u64) -> Self {
        Self {
            tag,
            pad: [0; 3],
            ts_ns,
        }
    }

    pub fn write_to<W: Write>(&self, mut w: W) -> io::Result<()> {
        w.write_u8(self.tag)?;
        w.write_all(&self.pad)?;
        w.write_u64::<LittleEndian>(self.ts_ns)?;
        Ok(())
    }

    pub fn read_from<R: Read>(mut r: R) -> io::Result<Self> {
        let tag = r.read_u8()?;
        let mut pad = [0u8; 3];
        r.read_exact(&mut pad)?;
        let ts_ns = r.read_u64::<LittleEndian>()?;
        Ok(Self { tag, pad, ts_ns })
    }
}

/// PTY output data record
#[derive(Debug, Clone, PartialEq)]
pub struct RecData {
    pub header: RecHeader,
    /// Byte offset for the FIRST byte of payload in the global PTY stream
    pub start_byte_off: u64,
    /// Payload length in bytes
    pub len: u32,
    /// Raw PTY bytes
    pub bytes: Vec<u8>,
}

impl RecData {
    pub fn new(ts_ns: u64, start_byte_off: u64, bytes: Vec<u8>) -> Self {
        Self {
            header: RecHeader::new(REC_DATA, ts_ns),
            start_byte_off,
            len: bytes.len() as u32,
            bytes,
        }
    }

    pub fn write_to<W: Write>(&self, mut w: W) -> io::Result<()> {
        self.header.write_to(&mut w)?;
        w.write_u64::<LittleEndian>(self.start_byte_off)?;
        w.write_u32::<LittleEndian>(self.len)?;
        w.write_all(&self.bytes)?;
        Ok(())
    }

    pub fn read_from<R: Read>(mut r: R, header: RecHeader) -> io::Result<Self> {
        let start_byte_off = r.read_u64::<LittleEndian>()?;
        let len = r.read_u32::<LittleEndian>()?;
        let mut bytes = vec![0u8; len as usize];
        r.read_exact(&mut bytes)?;
        Ok(Self {
            header,
            start_byte_off,
            len,
            bytes,
        })
    }
}

/// Terminal resize record
#[derive(Debug, Clone, PartialEq)]
pub struct RecResize {
    pub header: RecHeader,
    /// Terminal columns after resize
    pub cols: u16,
    /// Terminal rows after resize
    pub rows: u16,
}

impl RecResize {
    pub fn new(ts_ns: u64, cols: u16, rows: u16) -> Self {
        Self {
            header: RecHeader::new(REC_RESIZE, ts_ns),
            cols,
            rows,
        }
    }

    pub fn write_to<W: Write>(&self, mut w: W) -> io::Result<()> {
        self.header.write_to(&mut w)?;
        w.write_u16::<LittleEndian>(self.cols)?;
        w.write_u16::<LittleEndian>(self.rows)?;
        Ok(())
    }

    pub fn read_from<R: Read>(mut r: R, header: RecHeader) -> io::Result<Self> {
        let cols = r.read_u16::<LittleEndian>()?;
        let rows = r.read_u16::<LittleEndian>()?;
        Ok(Self { header, cols, rows })
    }
}

/// Input keystroke record (optional)
#[derive(Debug, Clone, PartialEq)]
pub struct RecInput {
    pub header: RecHeader,
    /// Input length in bytes
    pub len: u32,
    /// Raw input bytes (may be redacted)
    pub bytes: Vec<u8>,
}

impl RecInput {
    pub fn new(ts_ns: u64, bytes: Vec<u8>) -> Self {
        Self {
            header: RecHeader::new(REC_INPUT, ts_ns),
            len: bytes.len() as u32,
            bytes,
        }
    }

    pub fn write_to<W: Write>(&self, mut w: W) -> io::Result<()> {
        self.header.write_to(&mut w)?;
        w.write_u32::<LittleEndian>(self.len)?;
        w.write_all(&self.bytes)?;
        Ok(())
    }

    pub fn read_from<R: Read>(mut r: R, header: RecHeader) -> io::Result<Self> {
        let len = r.read_u32::<LittleEndian>()?;
        let mut bytes = vec![0u8; len as usize];
        r.read_exact(&mut bytes)?;
        Ok(Self { header, len, bytes })
    }
}

/// Internal marker record (reserved for future use)
#[derive(Debug, Clone, PartialEq)]
pub struct RecMark {
    pub header: RecHeader,
    /// Semantic sub-type code
    pub code: u32,
    /// Optional value
    pub val: u32,
}

impl RecMark {
    pub fn new(ts_ns: u64, code: u32, val: u32) -> Self {
        Self {
            header: RecHeader::new(REC_MARK, ts_ns),
            code,
            val,
        }
    }

    pub fn write_to<W: Write>(&self, mut w: W) -> io::Result<()> {
        self.header.write_to(&mut w)?;
        w.write_u32::<LittleEndian>(self.code)?;
        w.write_u32::<LittleEndian>(self.val)?;
        Ok(())
    }

    pub fn read_from<R: Read>(mut r: R, header: RecHeader) -> io::Result<Self> {
        let code = r.read_u32::<LittleEndian>()?;
        let val = r.read_u32::<LittleEndian>()?;
        Ok(Self { header, code, val })
    }
}

/// Filesystem snapshot record
///
/// Written when `ah agent fs snapshot` notifies the recorder of a new snapshot.
/// Links the snapshot ID to a specific PTY byte offset for time-travel integration.
#[derive(Debug, Clone, PartialEq)]
pub struct RecSnapshot {
    pub header: RecHeader,
    /// ID of the filesystem snapshot that was created
    pub snapshot_id: u64,
    /// PTY byte offset at snapshot time (for anchoring)
    pub anchor_byte: u64,
    /// Optional UTF-8 label for the snapshot
    pub label: String,
}

impl RecSnapshot {
    pub fn new(ts_ns: u64, snapshot_id: u64, anchor_byte: u64, label: String) -> Self {
        Self {
            header: RecHeader::new(REC_SNAPSHOT, ts_ns),
            snapshot_id,
            anchor_byte,
            label,
        }
    }

    pub fn write_to<W: Write>(&self, mut w: W) -> io::Result<()> {
        self.header.write_to(&mut w)?;
        w.write_u64::<LittleEndian>(self.snapshot_id)?;
        w.write_u64::<LittleEndian>(self.anchor_byte)?;

        // Write label length and bytes
        let label_bytes = self.label.as_bytes();
        w.write_u16::<LittleEndian>(label_bytes.len() as u16)?;
        w.write_all(label_bytes)?;

        Ok(())
    }

    pub fn read_from<R: Read>(mut r: R, header: RecHeader) -> io::Result<Self> {
        let snapshot_id = r.read_u64::<LittleEndian>()?;
        let anchor_byte = r.read_u64::<LittleEndian>()?;

        // Read label
        let label_len = r.read_u16::<LittleEndian>()? as usize;
        let mut label_bytes = vec![0u8; label_len];
        r.read_exact(&mut label_bytes)?;
        let label = String::from_utf8(label_bytes).map_err(|e| {
            io::Error::new(io::ErrorKind::InvalidData, format!("Invalid UTF-8 in snapshot label: {}", e))
        })?;

        Ok(Self {
            header,
            snapshot_id,
            anchor_byte,
            label,
        })
    }
}

/// Unified record type for all record kinds
#[derive(Debug, Clone, PartialEq)]
pub enum Record {
    Data(RecData),
    Resize(RecResize),
    Input(RecInput),
    Mark(RecMark),
    Snapshot(RecSnapshot),
}

impl Record {
    /// Get the timestamp of this record
    pub fn ts_ns(&self) -> u64 {
        match self {
            Record::Data(r) => r.header.ts_ns,
            Record::Resize(r) => r.header.ts_ns,
            Record::Input(r) => r.header.ts_ns,
            Record::Mark(r) => r.header.ts_ns,
            Record::Snapshot(r) => r.header.ts_ns,
        }
    }

    /// Write the record to a writer
    pub fn write_to<W: Write>(&self, w: W) -> io::Result<()> {
        match self {
            Record::Data(r) => r.write_to(w),
            Record::Resize(r) => r.write_to(w),
            Record::Input(r) => r.write_to(w),
            Record::Mark(r) => r.write_to(w),
            Record::Snapshot(r) => r.write_to(w),
        }
    }

    /// Read a record from a reader (requires reading the header first)
    pub fn read_from<R: Read>(mut r: R) -> io::Result<Self> {
        let header = RecHeader::read_from(&mut r)?;
        match header.tag {
            REC_DATA => Ok(Record::Data(RecData::read_from(r, header)?)),
            REC_RESIZE => Ok(Record::Resize(RecResize::read_from(r, header)?)),
            REC_INPUT => Ok(Record::Input(RecInput::read_from(r, header)?)),
            REC_MARK => Ok(Record::Mark(RecMark::read_from(r, header)?)),
            REC_SNAPSHOT => Ok(Record::Snapshot(RecSnapshot::read_from(r, header)?)),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Unknown record tag: {}", header.tag),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_block_header_roundtrip() {
        let mut header = AhrBlockHeader::new(1234567890, 5000);
        header.uncompressed_len = 1024;
        header.compressed_len = 512;
        header.record_count = 10;
        header.set_last_block(true);

        let mut buf = Vec::new();
        header.write_to(&mut buf).unwrap();

        let mut cursor = Cursor::new(buf);
        let decoded = AhrBlockHeader::read_from(&mut cursor).unwrap();

        assert_eq!(header, decoded);
        assert!(decoded.is_last_block());
    }

    #[test]
    fn test_rec_data_roundtrip() {
        let data = RecData::new(9876543210, 1000, b"hello world".to_vec());

        let mut buf = Vec::new();
        data.write_to(&mut buf).unwrap();

        let mut cursor = Cursor::new(buf);
        let header = RecHeader::read_from(&mut cursor).unwrap();
        let decoded = RecData::read_from(&mut cursor, header).unwrap();

        assert_eq!(data, decoded);
        assert_eq!(decoded.bytes, b"hello world");
    }

    #[test]
    fn test_rec_resize_roundtrip() {
        let resize = RecResize::new(1111111111, 120, 40);

        let mut buf = Vec::new();
        resize.write_to(&mut buf).unwrap();

        let mut cursor = Cursor::new(buf);
        let header = RecHeader::read_from(&mut cursor).unwrap();
        let decoded = RecResize::read_from(&mut cursor, header).unwrap();

        assert_eq!(resize, decoded);
        assert_eq!(decoded.cols, 120);
        assert_eq!(decoded.rows, 40);
    }

    #[test]
    fn test_rec_snapshot_roundtrip() {
        let snapshot = RecSnapshot::new(
            1234567890,
            42,
            1000,
            "checkpoint-after-build".to_string(),
        );

        let mut buf = Vec::new();
        snapshot.write_to(&mut buf).unwrap();

        let mut cursor = Cursor::new(buf);
        let header = RecHeader::read_from(&mut cursor).unwrap();
        let decoded = RecSnapshot::read_from(&mut cursor, header).unwrap();

        assert_eq!(snapshot, decoded);
        assert_eq!(decoded.snapshot_id, 42);
        assert_eq!(decoded.anchor_byte, 1000);
        assert_eq!(decoded.label, "checkpoint-after-build");
    }

    #[test]
    fn test_rec_snapshot_empty_label() {
        let snapshot = RecSnapshot::new(1111111111, 99, 5000, String::new());

        let mut buf = Vec::new();
        snapshot.write_to(&mut buf).unwrap();

        let mut cursor = Cursor::new(buf);
        let header = RecHeader::read_from(&mut cursor).unwrap();
        let decoded = RecSnapshot::read_from(&mut cursor, header).unwrap();

        assert_eq!(snapshot, decoded);
        assert_eq!(decoded.label, "");
    }

    #[test]
    fn test_record_enum_roundtrip() {
        let records = vec![
            Record::Data(RecData::new(100, 0, b"test".to_vec())),
            Record::Resize(RecResize::new(200, 80, 24)),
            Record::Input(RecInput::new(300, b"abc".to_vec())),
            Record::Mark(RecMark::new(400, 1, 2)),
            Record::Snapshot(RecSnapshot::new(
                500,
                1,
                2000,
                "test-snapshot".to_string(),
            )),
        ];

        for record in records {
            let mut buf = Vec::new();
            record.write_to(&mut buf).unwrap();

            let mut cursor = Cursor::new(buf);
            let decoded = Record::read_from(&mut cursor).unwrap();

            assert_eq!(record, decoded);
        }
    }
}
