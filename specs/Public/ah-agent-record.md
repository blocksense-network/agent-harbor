# ah agent record — Recorder for Agent Sessions

**Status:** Draft (approved design direction)
**Audience:** Systems/infra engineers, CLI/TUI developers, agent-integration owners
**Primary goals:**

* Record terminal output of coding agents (e.g., Claude Code, Codex CLI) with **byte-perfect fidelity**.
* Compress **timestamped PTY output bytes immediately** using Brotli.
* Keep all lines effectively **open for modification until session end** (viewer renders live from a vt100 model; final line set is materialized only at export).
* Allow **time-anchored instructions**  targeted at the agent, addressable by **byte offsets** into the PTY stream.
* Produce a **final interleaved report** (lines + instructions/events) after the process exits, via full replay.

---

## 1. Overview

`ah agent record` launches a target command under a PTY, streams bytes into a `vt100` parser for faithful live display, writes a compact **append-only compressed file** of timestamped output. The TUI viewer (Ratatui) renders **directly from the in-memory vt100 model** and overlays annotations; it does **not** tail storage. A post-run exporter replays the recording to compute the **final set of terminal output lines** and interleaves them with moments snapshots were taken for reporting and downstream Agent Time‑Travel flows.

**Why this shape**

* Heavy use of `\r` (carriage return) from agents means lines are frequently overwritten. Live display must track a terminal grid (vt100). Persisting raw bytes preserves maximal fidelity and decouples storage from rendering.
* Byte-offset anchoring lets us map instructions to whatever the final line layout becomes after replay.

---

## 2. Terminology

* **PTY bytes**: Raw output bytes read from the master PTY.
* **Byte offset** (`byte_off`): Cumulative count of PTY bytes observed so far (monotonic, 0-based).
* **Instruction**: A user- or system-authored directive associated with an anchor in the PTY stream (by `anchor_byte`).
* **vt100 model**: The terminal state used by the viewer; tracks scrollback, cursor, and cell contents.
* **`last_write_byte`**: For each terminal row in the vt100 model, the largest `byte_off` that wrote to any cell in that row.

---

## 3. Operating Modes & CLI

```
Usage: ah agent record [OPTIONS] -- <CMD> [ARGS...]
       ah agent replay [--session <session-id|file.ahr>] [--fast] [--no-colors] [--print-meta]
       ah agent branch-points [--session <session-id|file.ahr>]
```

### `record`

* Starts recording and opens the Ratatui viewer immediately.
* Spawns `<CMD ...>` under a PTY; captures output and delivers input transparently.
* Opens an IPC socket for external instruction injection.

**Key options**

* `--out-file <file>`: Optional compressed output file (no records will be stored in the local database).
* `--out-branch-points <file>`: Optional JSON file with the session branch points (the output produced by the `branch-points` command).
* `--brotli-q <0..11>`: Brotli level (default: 4 for fast/compact balance).
* `--cols <n> --rows <n>`: Initial terminal size; resizes are tracked live. By default, preserve the size of the current terminal.
* `--ipc <auto|uds|tcp:host:port>`: Instruction injection server transport.

### `replay`

* Replays the `.ahr` file, simulating the passage of time as it originally happened during the recording. 

#### `--fast`

* Replay the complete `.ahr` file immediately, producing the file state of all lines. Print the final state to the current terminal.

#### `--no-colors`

Don't emit ANSI color codes when replaying or emitting the final state.

#### `--print-meta`

* Prints metadata and basic stats; does not render.

### `branch-points`

* Prints to stdout the final set of terminal output lines in the recorded session, interleaved with the snapshot labels.

---

## 4. Runtime Architecture

```
[PTY bytes] ─▶ [Brotli block writer] ─┐       (append-only .ahr)
                               ┌──────┴───────────────────────┐
[PTY bytes] ─▶ [vt100 parser] ─┤  Ratatui viewer (live only)  ├─► user clicks/keys
                               └──────┬───────────────────────┘
                                      │ overlays
                    [Snapshot notifications server (UDS/TCP)]
```

* **Write to local database.** Storage consists of one compressed recording blob.
* The viewer renders directly from the **vt100** model; storage is not consulted during the run.
* On shutdown (child exit, SIGINT), we flush/finish the last Brotli block and store it in the database and then fsync.

---

## 5. Outputs

### 5.1 Recording blob: `session.ahr`

* **Purpose:** Durable, minimal, replayable stream of PTY output (+ resize markers) with timestamps.
* **Layout:** Sequence of **independent blocks**. Each block has a tiny uncompressed header followed by a standalone **Brotli stream** of an uncompressed **Records Segment**.
* **Crash safety:** Each block is self-describing (lengths included). A truncated final block is detectable and ignorable.

#### 5.1.1 Block Header (little-endian)

```
struct AhrBlockHeader {
  u32 magic;            // 'AHRC' = 0x43524841
  u16 version;          // 1
  u16 header_len;       // sizeof(AhrBlockHeader)
  u64 start_ts_ns;      // wall clock ts of the first record in this block
  u64 start_byte_off;   // PTY byte offset immediately BEFORE the first DATA record
  u32 uncompressed_len; // bytes of the Records Segment before compression
  u32 compressed_len;   // bytes of the Brotli payload that follows
  u32 record_count;     // number of records in the segment
  u8  flags;            // bit 0: is_last_block (best-effort)
  u8  reserved[7];      // zero; room for future
}
```

Immediately after the header, `compressed_len` bytes of Brotli payload follow.

#### 5.1.2 Records Segment (uncompressed, then Brotli-compressed)

Each segment is a concatenation of simple fixed/length-prefixed records. **Absolute timestamps** are used for simplicity; deltas may be added later.

```
// Record type tags
const u8 REC_DATA     = 0; // PTY output bytes
const u8 REC_RESIZE   = 1; // terminal resize
const u8 REC_INPUT    = 2; // (optional) input keystrokes, if enabled
const u8 REC_MARK     = 3; // internal markers (rare)
const u8 REC_SNAPSHOT = 4; // filesystem snapshot notification

// Common prefix for all records
struct RecHeader {
  u8  tag;     // one of REC_*
  u8  pad[3];  // zero (alignment/future)
  u64 ts_ns;   // event timestamp (CLOCK_REALTIME in ns)
}

// REC_DATA
struct RecData {
  RecHeader h;          // tag=REC_DATA
  u64 start_byte_off;   // byte offset for the FIRST byte of payload
  u32 len;              // payload length
  u8  bytes[len];       // raw PTY bytes
}

// REC_RESIZE
struct RecResize {
  RecHeader h;          // tag=REC_RESIZE
  u16 cols;
  u16 rows;
}

// REC_INPUT (optional)
struct RecInput {
  RecHeader h;          // tag=REC_INPUT
  u32 len;
  u8  bytes[len];       // raw input bytes (may be redacted)
}

// REC_MARK (reserved for future; not required by MVP)
struct RecMark {
  RecHeader h;          // tag=REC_MARK
  u32 code;             // semantic sub-type
  u32 val;              // optional value
}

// REC_SNAPSHOT - Filesystem snapshot notification
// Written when `ah agent fs snapshot` notifies the recorder of a new snapshot
struct RecSnapshot {
  RecHeader h;          // tag=REC_SNAPSHOT
  u64 snapshot_id;      // ID of the snapshot created
  u64 anchor_byte;      // PTY byte offset at snapshot time
  u16 label_len;        // length of optional label string
  u8  label[label_len]; // UTF-8 label (if label_len > 0)
}
```

**Notes**

* `start_byte_off` is **monotonic** and equals the global count of previously seen PTY bytes; use it to resolve anchors without scanning earlier blocks.
* Blocks typically rotate at ~256–512 KiB uncompressed or ~250 ms, whichever comes first; this bounds replay latency and loss on crash.
* Brotli preset default: **q=4**, `lgwin` auto.

### 5.2 Snapshots file: `session.snapshots.jsonl`

Append-only NDJSON; each line is a single object written atomically.

```json
// Snapshot created via external IPC
// Optional structured event (moments/snapshots/checkpoints), future-compatible
{"id":45,"ts_ns":1710000000001000000,
 "label":"post-tool","kind":"auto","anchor_byte": 198000}
```

* Writer resolves `anchor_byte` using the **current** PTY `byte_off` at receipt time when callers supply `anchor:"now"`.
* The viewer maintains an in-memory mirror for instant UI.

### 5.3 Metadata file: `session.meta.json`

Static session facts useful for offline tools:

```json
{
  "version": 1,
  "startedAtNs": 1710000000000000000,
  "cmd": ["agent-cli","run","--project","…"],
  "cols": 120, "rows": 40,
  "brotliQ": 4,
  "host": {"os":"linux","arch":"x86_64"}
}
```

---

## 6. Viewer (Ratatui) — Live and post-session rendering with support for injecting agent instructions at every snapshot

* Reads **directly** from a `vt100::Parser` kept current by the PTY reader.
* Maintains for each absolute row index a `last_write_byte` value.
* **Row-change tracking:** After each REC_DATA application, we compute a small **damage band** around the cursor/scroll area and re-hash only those rows; any row hash change updates its `last_write_byte` to `data.start_byte_off + data.len`.
* **Terminal viewport preservation:** The viewer renders the recorded terminal content within a bordered frame that preserves the original terminal dimensions. If the current terminal is larger than the recorded session, the viewport appears as a bordered rectangle of the original size, centered or positioned appropriately. If the current terminal is smaller, the viewport is truncated with scrolling controls.
* **Overlay/annotations:**

  * Click (or key) maps to a vt100 row; we fetch that row's `last_write_byte`.
  * Nearest snapshot is `argmin |event.anchor_byte - row.last_write_byte|` (tie-break by newest `ts_ns`).
  * The standard new draft task UI is inserted after the clicked line and before the ones that follow. The following lines are dimmed to signify that they won't be taken into consideration once the session is branched from the snapshot moment. Additional UI elements expand the viewer's layout while preserving the original terminal viewport.
* **No storage tailing:** The viewer never reads `.ahr` during a live session.

**Key interactions**

* Scroll: PgUp/PgDn / Mouse
* Insert instruction: `i` or mouse click → overlay → submit
* Incremental search with `/`
* Support all standard short-cuts from `less` and `more`
* Navigate nearest instruction: `[`/`]` (prev/next by anchor)
* Quit: `q` (prompts to stop child if still running)

---

## 7. Snapshot IPC

The recorder provides an IPC interface for receiving snapshot notifications from external commands, primarily `ah agent fs snapshot`. When a filesystem snapshot is taken during recording, the snapshot command notifies the recorder, which writes a `REC_SNAPSHOT` record to the `.ahr` file.

* **Transport:**

  * Unix: default **Unix domain socket** at `<out-dir>/ipc.sock`.
  * Windows: TCP `127.0.0.1:<ephemeral>` (logged in meta), or named pipe (later).
* **Protocol:** Little endian length-prefixed raw binary SSZ with tagged unions.

**Integration with `ah agent fs snapshot`:**

When `ah agent fs snapshot` creates a filesystem snapshot during a recorded session, it:
1. Detects the active recording session (via environment variable or session metadata)
2. Connects to the recorder's IPC socket
3. Sends a `Snapshot` notification with the snapshot ID and optional label
4. Receives confirmation with the anchor byte offset

The recorder then:
1. Records the current PTY byte offset
2. Writes a `REC_SNAPSHOT` record to the `.ahr` file
3. Returns the anchor information to the caller

**Request**

* `Snapshot`
  - `snapshot_id`: uint64 - ID of the filesystem snapshot that was created
  - `label`: string (optional) - human-readable label for this snapshot

**Response**

* `SnapshotStatus`
  - `success`: bool
  - `id`: uint64 - snapshot ID (echoed from request)
  - `anchor_byte`: uint64 - PTY byte offset at snapshot time
  - `ts_ns`: uint64 - timestamp when snapshot was recorded

This is a tagged union, with the `success` field acting as a discriminator. In case of failures, `success = false`, `err = "string"`.

---

## 8. Export — Final Interleaving Report

### 8.1 Replay

* Instantiate a fresh `vt100::Parser` with **large scrollback** (configurable; default 1e6 rows) and initial size from `session.meta.json`.
* Stream through all blocks and all records in order (apply resizes); after each REC_DATA, update row hashes and `last_write_byte` as in live mode.

### 8.2 Collect final lines

* Read the final scrollback + screen rows top→bottom.
* For each row, emit `{ idx, text, last_write_byte }`, omitting fully blank tail rows.

### 8.3 Merge with snapshot moments

* Load all JSONL records.
* Stable-merge on `position` where `position(line) = last_write_byte`, `position(event) = anchor_byte`.
* Output formats:

  * **md**: human-friendly table with badges for snapshot rows.
  * **json**: structured array with `{kind:"line"|"snapshot", …}`.
  * **csv**: minimal columns (kind, index/id, position, ts_ns, text/message/label).

### 8.4 Determinism & edge cases

* If multiple events share the same `anchor_byte`, keep input order.
* Lines with identical `last_write_byte` (rare due to scroll) retain visual order.

---

## 9. Performance & Resource Targets

* **Write path:** sub-1 ms per REC_DATA on typical 4–16 KiB buffers; Brotli q=4 keeps CPU under 5–10% on modern cores.
* **Viewer latency:** <30 ms frame-to-frame at 60–120 Hz terminal update rates; damage-band hashing limits per-update work to O(screen rows), typically a few dozen.
* **Export time:** ~1–2× wall time of the session data volume (I/O bound); no random seeks required.

---


## 10. Testing Plan

* **Mock agent** that emits deterministic progress lines with heavy `\r` usage and synthetic moments; drives instruction injections programmatically.
* Unit tests for: block header parse/scan, truncated tail recovery, replay idempotence, anchor mapping, export merge.
* Snapshot tests on exporter outputs (md/json/csv) for stable diffs.

---

## 11. Future Extensions

* Lightweight **frame index** footer for faster mid-session random access.
* Streaming **SSE bridge** to external dashboards.
* Translate the agent-specific output to a normalized JSON stream with event types defined by Agent Harbor

---

## 13. Implementation Notes (Rust crates)

* PTY: `portable-pty`
* Terminal model: `vt100`
* TUI: `ratatui`, `crossterm`
* Compression: `brotli` crate
* JSON: `serde`, `serde_json`
* Binary codec: manual LE writes/reads (`byteorder`), or `bincode` for scaffolding during prototyping
* Time: `time` or `quanta` for ns timestamps

---

## 14. Compatibility & Portability

* Linux, macOS, Windows (PTY availability and UDS vs TCP vary by OS).
* Recording is platform-neutral; replay behaves identically across hosts.

---

## 15. Minimal File Format Validator (pseudo-code)

```
loop read header H:
  if H.magic != 'AHRC' or H.version > 1: error
  read H.compressed_len bytes → brotli decode → buffer B
  if decoded len != H.uncompressed_len: error
  parse B as concatenated records; ensure record_count matches
end
```

---

## 16. Deliverables (MVP)

* `ah agent record` with live viewer, IPC for instructions, `.ahr` writer, `.instructions.jsonl` appends
* `ah agent replay` producing md/json/csv interleaved report
* `ah agent replay --print-meta` showing meta and quick stats
* `ah agent branch-points` exports the session interleaved outputlines with possible branch points (snapshots)

---

**End of v0.1 spec**
