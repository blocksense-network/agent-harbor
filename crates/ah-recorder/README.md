# ah-recorder

Agent Harbor recording and replay functionality for capturing terminal sessions with byte-perfect fidelity.

## Overview

This crate implements the core functionality for `ah agent record` command, providing:

- **PTY Management**: Spawn commands under a PTY and capture raw output using `portable-pty`
- **Terminal State Tracking**: Faithful terminal emulation using `vt100` parser
- **Compressed Recording Format**: Brotli-compressed `.ahr` file format with timestamped PTY records
- **Snapshots**: JSONL-based snapshot tracking for instruction anchoring
- **Byte-perfect Fidelity**: Preserves all terminal output including control sequences

## File Format

### `.ahr` Files (Agent Harbor Recording)

The `.ahr` format consists of a sequence of independent Brotli-compressed blocks:

```
[Block Header] [Compressed Records] [Block Header] [Compressed Records] ...
```

Each block contains:

- **Block Header** (48 bytes): Magic, version, timestamps, byte offsets, compression info
- **Compressed Records**: Brotli-compressed sequence of timestamped records

Record types:

- `REC_DATA` (0): PTY output bytes with byte offset anchoring
- `REC_RESIZE` (1): Terminal resize events
- `REC_INPUT` (2): Input keystrokes (optional)
- `REC_MARK` (3): Internal markers (reserved)

### `.snapshots.jsonl` Files

Append-only NDJSON log of snapshot events:

```json
{"id":0,"ts_ns":1234567890,"label":"checkpoint","kind":"auto","anchor_byte":1000}
{"id":1,"ts_ns":1234568000,"label":"user-mark","kind":"manual","anchor_byte":2500}
```

## Usage

### As a Library

```rust
use ah_recorder::{
    PtyRecorder, PtyRecorderConfig, RecordingSession,
    AhrWriter, WriterConfig, create_shared_writer,
};

// Configure PTY
let config = PtyRecorderConfig {
    cols: 80,
    rows: 24,
    ..Default::default()
};

// Create writer
let writer = AhrWriter::create("session.ahr", WriterConfig::default())?;
let snapshots_writer = create_shared_writer("session.snapshots.jsonl")?;

// Spawn command in PTY
let (recorder, rx) = PtyRecorder::spawn("bash", &[], config.clone())?;

// Start recording
let handle = recorder.start_capture();
let mut session = RecordingSession::new(handle, rx, writer, &config);

// Process events
while let Some(event) = session.process_event().await {
    // Handle PTY events
}

// Finalize
session.finalize().await?;
```

### Via CLI

```bash
# Record a session
ah agent record -- bash -c "echo 'Hello, world!'"

# Specify output file
ah agent record --out-file my-session.ahr -- python script.py

# Custom terminal size
ah agent record --cols 120 --rows 40 -- vim file.txt
```

## Architecture

### Core Components

1. **format.rs**: `.ahr` file format definitions and serialization
2. **writer.rs**: Block-based writer with Brotli compression
3. **pty.rs**: PTY management and vt100 terminal state tracking
4. **snapshots.rs**: JSONL snapshot writer and reader

### Design Principles

- **Crash Safety**: Each block is independent and self-describing
- **Bounded Latency**: Blocks flush at configurable size/time thresholds (default: 256KB / 250ms)
- **Deterministic Replay**: Absolute timestamps and byte offsets enable exact reproduction
- **Byte-offset Anchoring**: All snapshots reference PTY byte offsets for precise time-travel

## Testing

The crate includes comprehensive unit tests:

```bash
cargo test -p ah-recorder
```

Tests cover:

- File format serialization/deserialization
- Block header validation
- Brotli compression/decompression
- PTY byte offset tracking
- Terminal state updates
- Snapshot storage and querying

## Specification

See [specs/Public/ah-agent-record.md](../../specs/Public/ah-agent-record.md) for the complete specification.

## Future Work

The following features are planned for future iterations:

- **Live Ratatui Viewer**: Real-time TUI rendering from vt100 model
- **IPC Server**: Unix domain socket for external instruction injection
- **Replay Command**: `ah agent replay` with export functionality
- **Interleaved Reports**: Export final lines merged with snapshots (md/json/csv)
- **Frame Index**: Optional footer for faster random access
- **SSE Bridge**: Streaming events to external dashboards

## License

MIT OR Apache-2.0
