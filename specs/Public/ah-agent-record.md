# ah agent record — Recorder for Agent Sessions

**Status:** Draft (approved design direction)
**Audience:** Systems/infra engineers, CLI/TUI developers, agent-integration owners
**Primary goals:**

- Record terminal output of coding agents (e.g., Claude Code, Codex CLI) with **byte-perfect fidelity**.
- Compress **timestamped PTY output bytes immediately** using Brotli.
- Keep all lines effectively **open for modification until session end** (viewer renders live from a vt100 model; final line set is materialized only at export).
- Allow **time-anchored instructions** targeted at the agent, addressable by **byte offsets** into the PTY stream.
- Produce a **final interleaved report** (lines + instructions/events) after the process exits, via full replay.

---

## 1. Overview

`ah agent record` launches a target command under a PTY, streams bytes into a `vt100` parser for faithful live display, and optionally writes a compact **append-only compressed file** of timestamped output. The TUI viewer (Ratatui) renders **directly from the in-memory vt100 model** and overlays annotations; it does **not** tail storage. Always provides real-time IPC communication for snapshot detection and UI interaction, enabling users to start agent tasks from detected snapshots even when no output file is saved. A post-run exporter replays the recording to compute the **final set of terminal output lines** and interleaves them with moments snapshots were taken for reporting and downstream Agent Time‑Travel flows.

**Why this shape**

- Heavy use of `\r` (carriage return) from agents means lines are frequently overwritten. Live display must track a terminal grid (vt100). Persisting raw bytes preserves maximal fidelity and decouples storage from rendering.
- Byte-offset anchoring lets us map instructions to whatever the final line layout becomes after replay.

---

## 2. Terminology

- **PTY bytes**: Raw output bytes read from the master PTY.
- **Byte offset** (`byte_off`): Cumulative count of PTY bytes observed so far (monotonic, 0-based).
- **Instruction**: A user- or system-authored directive associated with an anchor in the PTY stream (by `anchor_byte`).
- **vt100 model**: The terminal state used by the viewer; tracks scrollback, cursor, and cell contents.

---

## 3. Operating Modes & CLI

```
Usage: ah agent record [OPTIONS] -- <CMD> [ARGS...]
       ah agent replay [--session <session-id|file.ahr>] [--fast] [--no-colors] [--print-meta]
       ah agent branch-points [--session <session-id|file.ahr>]
       ah show-sandbox-execution "<CMD...>" --id <execution-id> [--session <session-id|file.ahr>] [--follow]
```

### `record`

- Starts recording and opens the Ratatui viewer immediately.
- Spawns `<CMD ...>` under a PTY; captures output and delivers input transparently.
- Always opens an IPC socket for external instruction injection and real-time snapshot detection, enabling UI interaction even when no output file is saved.
- When launched by the TUI or CLI for task monitoring, establishes an unnamed pipe for streaming SSE-like events to the parent process.

**Key options**

- `--out-file <file>`: Optional compressed output file. When not specified, no .ahr file is created but IPC communication remains active for UI interaction and snapshot detection. When launched by the local task manager (e.g., via `ah agent start`) and the user has selected "Record session" in the launch options, the file is automatically stored at `{AH_HOME}/recordings/YYYY/MM/{session_id}.ahr` where:
  - `{AH_HOME}` follows the same location logic as the local SQLite database (see State-Persistence.md)
  - `YYYY/MM` is the UTC year/month when recording started
  - `{session_id}` is the stable session ULID/UUID
- `--out-branch-points <file>`: Optional JSON file with the session branch points (the output produced by the `branch-points` command).
- `--brotli-q <0..11>`: Brotli level (default: 4 for fast/compact balance).
- `--cols <n> --rows <n>`: Initial terminal size; resizes are tracked live. When specified, attempts to resize the current terminal window using Window Ops escape sequences. By default, preserve the size of the current terminal.
- `--ipc <auto|uds|tcp:host:port>`: Instruction injection server transport.
- `--gutter <left|right|none>`: Position of the snapshot indicator gutter column (default: right).
- `--events-pipe-fd <FD>`: File descriptor of an unnamed pipe for streaming SSE-like events to parent process (used by TUI/CLI for real-time task monitoring).
- **Input injection (`inject_message`)**: every recording session exposes a PTY-backed control channel that can simulate keystrokes inside the launched third-party agent. SessionViewer, the supervisor CLI, or ACP bridges call `inject_message` to feed bytes into the recorder; those bytes travel through the same TTY, are captured in the `.ahr`, and reach the child process exactly as if the user had typed them live (useful for responding to prompts such as sudo passwords).

### `replay`

- Replays the `.ahr` file using the same unified pipeline as recording, but sources events from the stored AHR file instead of live PTY/IPC input. Events are first fed through TerminalState (containing the single vt100 parser instance), then to the viewer for display. This ensures identical behavior between live recording and replay.

#### `--fast`

- Replay the complete `.ahr` file immediately, producing the file state of all lines. Print the final state to the current terminal.

#### `--no-colors`

Don't emit ANSI color codes when replaying or emitting the final state.

#### `--print-meta`

- Prints metadata and basic stats; does not render.

### `branch-points`

- Prints to stdout the final set of terminal output lines in the recorded session, interleaved with the snapshot labels.

### `show-sandbox-execution`

- Opens a read/write view of a specific tool execution captured in `.ahr`, similar to attaching to a tmux pane. Usage: `ah show-sandbox-execution "<original command pipeline>" --id <execution-id> [--session <session-id|file.ahr>] [--follow]`.
- The quoted command is exactly what the embedded agent ran (pipelines included) so operators and IDEs can display the true shell invocation; Harbor-aware clients may hide the `ah` wrapper when rendering.
- `--id` matches the execution identifier emitted in recorder events/SSE and stays stable across replays so ACP clients can reconcile telemetry.
- Any keystrokes typed while the command is streaming are forwarded through the recorder’s `inject_message` TTY back into the running process, allowing users to unblock password prompts or interactive menus without leaving the follower view.
- With `--follow`, the command remains open after the process exits so users can scroll recent output; otherwise it exits once the PTY reaches EOF.

---

## 4. Runtime Architecture

```
Live Recording Pipeline:
[PTY bytes + IPC snapshots] ─▶ [Ordered processing] ─▶ [TerminalState (vt100 parser)] ─▶ [AHR writer] ─▶ .ahr file
                                                                │
                                                                ▼
                                                         [Ratatui viewer] ─► user clicks/keys

Replay Pipeline:
[AHR file events] ─▶ [TerminalState (vt100 parser)] ─▶ [Ratatui viewer] ─► user clicks/keys
```

**Data Pipeline Algorithm:**

1. **Terminal output data and snapshots are received over IPC.** PTY bytes arrive asynchronously from the child process, while snapshot notifications come via IPC from external commands (e.g., `ah agent fs snapshot`).

2. **We order them by time in a best effort way.** When a snapshot IPC notification is received, we drain all pending terminal data buffers to ensure that all recorded app output before the snapshot has been processed. This creates a best-effort chronological ordering of events.

3. **Once they are ordered, they are processed in the same order by the AHR file writer, then by the TerminalState and finally by the viewer.** Events flow through a unified pipeline:
   - First to the **TerminalState** (which contains the single vt100 parser instance)
   - Then to the **AHR file writer** for persistent storage
   - Finally to the **viewer** for real-time display

4. **The viewer uses the information in TerminalState to render the final screen output with the correct colors, snapshot indicators at the correct lines, etc.** The viewer queries the TerminalState for current terminal content and snapshot positions.

- **Single vt100 parser instance:** There is only one vt100 parser, stored in TerminalState, ensuring consistency between recording and display.
- **Unified state:** TerminalState maintains both the terminal display state and snapshot positioning information.
- **Live updates:** The viewer renders directly from TerminalState; storage is not consulted during live recording.
- **Write to local database:** Storage consists of one compressed recording blob.
- On shutdown (child exit, SIGINT), we flush/finish the last Brotli block and store it in the database and then fsync.

---

## 5. Outputs

### 5.1 Recording blob: `session.ahr`

- **Purpose:** Durable, minimal, replayable stream of PTY output (+ resize markers) with timestamps.
- **Layout:** Sequence of **independent blocks**. Each block has a tiny uncompressed header followed by a standalone **Brotli stream** of an uncompressed **Records Segment**.
- **Crash safety:** Each block is self-describing (lengths included). A truncated final block is detectable and ignorable.

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

- `start_byte_off` is **monotonic** and equals the global count of previously seen PTY bytes; use it to resolve anchors without scanning earlier blocks.
- Blocks typically rotate at ~256–512 KiB uncompressed or ~250 ms, whichever comes first; this bounds replay latency and loss on crash.
- Brotli preset default: **q=4**, `lgwin` auto.

### 5.2 Snapshots file: `session.snapshots.jsonl`

Append-only NDJSON; each line is a single object written atomically.

```json
// Snapshot created via external IPC
// Optional structured event (moments/snapshots/checkpoints), future-compatible
{
  "id": 45,
  "ts_ns": 1710000000001000000,
  "label": "post-tool",
  "kind": "auto",
  "anchor_byte": 198000
}
```

- Writer resolves `anchor_byte` using the **current** PTY `byte_off` at receipt time when callers supply `anchor:"now"`.
- The viewer maintains an in-memory mirror for instant UI.

### 5.3 Metadata file: `session.meta.json`

Static session facts useful for offline tools:

```json
{
  "version": 1,
  "startedAtNs": 1710000000000000000,
  "cmd": ["agent-cli", "run", "--project", "…"],
  "cols": 120,
  "rows": 40,
  "brotliQ": 4,
  "host": { "os": "linux", "arch": "x86_64" }
}
```

---

## 6. SessionViewer UI — Live and post-session rendering with support for injecting agent instructions at every snapshot

The **SessionViewer UI** is the unified interface displayed by `ah agent record` during live agent sessions and when examining AHR recordings after sessions have ended. It supports both **live session tracking mode** (during active recording) and **post-facto session examination** (when reviewing completed recordings).

- Uses **TerminalState** (which contains the single vt100 parser instance) to render terminal content and maintain snapshot positioning information.
- **TerminalState state machine:** Processes PTY output and snapshot events in chronological order through the unified pipeline, maintaining accurate terminal state at all times.
- **Snapshot positioning:** When a snapshot is recorded, TerminalState associates the snapshot with the line that was active at that moment in the vt100 parser.
- **Terminal viewport preservation:** During replay mode, when the current terminal dimensions differ from the recorded session's terminal dimensions, the viewer renders the recorded terminal content within a bordered frame that preserves the original terminal dimensions. If the current terminal is larger than the recorded session, the viewport appears as a bordered rectangle of the original size, centered or positioned appropriately. If the current terminal is smaller, the viewport is truncated with scrolling controls.

During live recording mode, the terminal content is displayed without a frame to provide an immersive, full-screen experience.

- **Gutter system:** The viewer supports an optional gutter column (`agent.record.gutter: <left|right|none>`) that displays snapshot indicators. Snapshot markers appear in the gutter at positions corresponding to when snapshots were taken during recording. The position is determined by the TerminalState's vt100 parser state.
- **Task Entry UI:** The instruction entry UI (task_entry) behavior differs between live and replay modes:
  - **Live mode:** The instruction entry UI is **not shown by default** while the recording UI is being displayed. It is activated by pressing `Ctrl+Shift+Up`, which creates the instruction entry UI at the position of the latest snapshot. Subsequent presses of `Ctrl+Shift+Up` (or just `Up`) move the instruction entry dialog to the previous snapshot, while `Ctrl+Shift+Down` (or just `Down`) moves it to the next snapshot.
  - **Replay mode:** When the SessionViewer UI is launched on a completed session, the task entry UI is shown immediately at the very bottom of the session. When the session ends, this automatically creates a final snapshot, allowing users to immediately add instructions for branching from the session's end state.
- **Task Entry Movement Rules:**
  - If the target snapshot was already visible on screen before the move and there is enough room to fit the task entry box (i.e., the snapshot line is visible and there are sufficient lines after it in the viewport to accommodate the task entry height), the screen should not scroll and the snapshot position in the gutter should stay the same.
  - If the target snapshot was not visible on screen or there is not enough room to fit the task entry box, the screen centers around the snapshot. The task entry is displayed such that the spans of lines above it (before_task_entry) and below it (after_task_entry) are roughly balanced, with the height difference between them at most 1 (the span below may have one more line than the one above).
  - **Auto-follow suppression:** When the task entry UI is displayed over a snapshot, auto-follow behavior is disabled. New terminal output lines do not cause automatic scrolling, allowing the user to maintain their view of the snapshot and task entry. Manual scrolling (keyboard or mouse) will move the entire display including the task entry widget attached to its snapshot.
- **Dynamic UI injection:** When snapshots are created during recording, the viewer can dynamically inject instruction UI elements. Clicking on a gutter marker inserts the instruction overlay between the relevant lines of the original program output. The gutter indicator remains visible next to the inserted UI.
- **Overlay/annotations:**
  - Click (or key) maps to a terminal line; TerminalState provides the snapshot associated with that line (if any).
  - If a snapshot exists for the clicked line, the instruction UI is inserted at that position.
  - The standard new draft task UI is inserted after the clicked line and before the ones that follow. The following lines are dimmed to signify that they won't be taken into consideration once the session is branched from the snapshot moment. Additional UI elements expand the viewer's layout while preserving the original terminal viewport.

- **No storage tailing:** The viewer never reads `.ahr` during a live session.
- **Keyboard handling:** The SessionViewer processes keyboard input through the `settings.rs` approach, allowing all shortcuts to be remapped. It handles `KeyboardOperations` defined in the settings system, ensuring consistent and customizable key bindings across both live and replay modes.
- **Tool execution followers:** The status bar lists active `show-sandbox-execution` sessions (identified by `execution-id`). Pressing `Ctrl+Shift+T` (default, configurable) opens a modal that renders the follower TTY exactly as recorded; the modal behaves like a tmux pane, letting the user scroll, copy output, or type input. Any keystrokes typed inside the modal are sent through the recorder’s `inject_message` bridge so the underlying third-party agent sees them immediately, which is critical for unblocking commands waiting on interactive prompts (e.g., sudo). Completed executions remain accessible until dismissed, enabling re-watch without leaving the viewer.

**Key interactions**

- Scroll: PgUp/PgDn / Mouse
- Task Entry UI: `Ctrl+Shift+Up` activates instruction entry at latest snapshot, `Ctrl+Shift+Up`/`Up (when upwards movement within the task entry lines is not possible)` (previous snapshot), `Ctrl+Shift+Down`/`Down (when downwards movement within the task entry lines is not possible)` (next snapshot)
- Insert instruction: `i` or mouse click → overlay → submit
- Gutter interaction: Click on gutter snapshot markers to insert instruction UI between program output lines
- Incremental search with `/`
- Support all standard short-cuts from `less` and `more`
- Navigate nearest instruction: `[`/`]` (prev/next by anchor)
- Quit: `Esc` (double-press to exit)

---

## 7. TerminalState — Unified State Machine for Recording and Display

**TerminalState** is the central component that maintains accurate terminal state and snapshot positioning. It contains the single vt100 parser instance and processes all events (PTY data and snapshots) through the unified pipeline in chronological order to ensure consistency between recording and display.

### 7.1 Architecture

```rust
/// A newtype for terminal line indices to prevent accidental assignments
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LineIndex(usize);

pub struct TerminalState {
    parser: vt100::Parser,           // Single vt100 parser instance
    snapshots: Vec<LineWithSnapshot>, // Sorted by line index
}

pub struct LineWithSnapshot {
    line: LineIndex,    // Line that was active when snapshot was taken (comes first for sorting)
    snapshot: Snapshot,
}
```

### 7.2 Event Processing

TerminalState processes events in chronological order through the unified pipeline:

- **Data events**: PTY output bytes fed to the vt100 parser, updating terminal state
- **Snapshot events**: Recorded when snapshots occur, associating them with the line that was active at that moment

### 7.3 Key Methods

- **`process_data(bytes)`**: Feeds PTY output through the vt100 parser, updating terminal state
- **`record_snapshot(snapshot)`**: Associates a snapshot with the current active line (cursor position)
- **`line_count()`**: Returns current number of lines in terminal
- **`line_content(line_idx)`**: Returns ANSI-formatted content of a specific line
- **`has_snapshot_at_line(line_idx)`**: Returns true if the line has an associated snapshot (uses binary search on sorted snapshots)
- **`get_snapshot_for_line(line_idx)`**: Returns the snapshot associated with a line (if any, uses binary search on sorted snapshots)
- **`last_snapshot_before_line(line_idx)`**: Returns the last snapshot that occurred before the given line index (uses binary search with partition_point)
- **`next_snapshot_after_line(line_idx)`**: Returns the first snapshot that occurred after the given line index (uses binary search with partition_point)

### 7.4 Usage in Different Contexts

#### Live Recording

```rust
// Terminal size is adjusted for UI elements (gutter width subtracted)
let effective_cols = cols - gutter_width;
let mut recording_state = TerminalState::new(rows, effective_cols);

// As PTY data arrives through the unified pipeline
recording_state.process_data(pty_bytes);

// When snapshots are created via IPC (after draining buffers)
recording_state.record_snapshot(snapshot);

// Viewer queries for rendering
for line_idx in 0..recording_state.line_count() {
    if recording_state.has_snapshot_at_line(line_idx) {
        // Show ▶ indicator
    }
    let content = recording_state.line_content(line_idx);
    // Render line
}
```

#### Replay from AHR File

Replay uses the same unified pipeline as live recording, but sources events from the stored AHR file instead of live PTY/IPC input:

```rust
// Terminal size is adjusted for UI elements (gutter width subtracted from recorded size)
let effective_cols = recorded_cols - gutter_width;
let mut recording_state = TerminalState::new(recorded_rows, effective_cols);

// Replay all events from AHR file through the same pipeline as recording
// Events are processed in chronological order: TerminalState first, then viewer
for event in ahr_events {
    match event {
        Data { bytes, .. } => recording_state.process_data(bytes),
        Snapshot { snapshot, .. } => recording_state.record_snapshot(snapshot),
        Resize { cols, rows, .. } => recording_state.resize(cols, rows),
    }
}

// Viewer queries the TerminalState for display and snapshot information
// This ensures identical behavior between live recording and replay
```

#### Branch-Points Generation

```rust
// Same replay process as above, then:
let mut result = Vec::new();
for line_idx in 0..recording_state.line_count() {
    result.push(InterleavedItem::Line(recording_state.line_content(line_idx)));
    if let Some(snapshot) = recording_state.get_snapshot_for_line(line_idx) {
        result.push(InterleavedItem::Snapshot(snapshot));
    }
}
```

### 7.5 Algorithm Deep Dive

The core algorithm ensures that terminal output data and snapshots are received over IPC, ordered by time in a best-effort way (with buffer draining for snapshots), and processed in the same order by the AHR file writer, then by TerminalState, and finally by the viewer.

**Key insight**: Terminal lines are not immutable - they can be overwritten by subsequent output (especially with carriage returns `\r`). A snapshot taken at byte position X may end up associated with different content after more output arrives.

**Solution**: Process all events (PTY data and snapshots) in the exact chronological order they occurred through a single vt100 parser instance. When a snapshot is recorded, capture which line was currently active (cursor position) in the vt100 model. This association remains valid even if the line content changes later, because we're tracking the line index that was active at snapshot time.

**Unified pipeline flow**:

1. PTY bytes and IPC snapshots arrive → ordered with buffer draining for snapshots
2. Events processed chronologically through TerminalState (vt100 parser)
3. Same events also processed by AHR writer for persistent storage
4. Viewer renders from TerminalState for real-time display

**Live recording flow**:

1. PTY bytes arrive asynchronously from child process
2. Snapshot notifications arrive via IPC from external commands
3. When snapshot received: drain all pending PTY data buffers first
4. Process events in chronological order: TerminalState → AHR writer → viewer
5. Viewer renders from TerminalState with correct colors and snapshot indicators

**Replay flow**:

1. Start with fresh TerminalState at recorded dimensions
2. Source events from AHR file instead of live PTY/IPC input
3. Process events through the same unified pipeline as live recording: TerminalState (vt100 parser) → viewer
4. Final state contains accurate line-to-snapshot associations identical to live recording
5. Viewer renders using TerminalState, ensuring identical display behavior

This approach ensures that snapshot positioning reflects the actual terminal state at capture time, with consistency between recording and display through the single vt100 parser instance.

### 7.6 Correctness Guarantees

- **Single vt100 parser**: Only one parser instance ensures consistency between recording and display
- **Chronological processing**: Events processed in exact order they occurred
- **vt100 accuracy**: Terminal state reflects actual program output interpretation
- **Snapshot timing**: Snapshots associated with the line active at capture time
- **Unified pipeline**: Same events processed by recording, storage, and display components

---

## 8. Snapshot IPC

The recorder provides an IPC interface for receiving snapshot notifications from external commands, primarily `ah agent fs snapshot`. When a filesystem snapshot is taken during recording, the snapshot command notifies the recorder, which writes a `REC_SNAPSHOT` record to the `.ahr` file.

- **Transport:**
  - Unix: default **Unix domain socket** at `<out-dir>/ipc.sock`.
  - Windows: TCP `127.0.0.1:<ephemeral>` (logged in meta), or named pipe (later).

- **Protocol:** Little endian length-prefixed raw binary SSZ with tagged unions.

**Integration with `ah agent fs snapshot`:**

When `ah agent fs snapshot` creates a filesystem snapshot during a recorded session, it:

1. Detects the active recording session (via environment variable or session metadata)
2. **Notifies the recorder via IPC and waits for confirmation** that the `REC_SNAPSHOT` record has been written to the AHR file
3. **Only after receiving confirmation**, creates the actual filesystem snapshot through the selected provider
4. Returns the snapshot information

The recorder's confirmation process:

1. Records the current PTY byte offset at the time of the notification
2. Writes a `REC_SNAPSHOT` record to the `.ahr` file (snapshot_id may be 0 as placeholder)
3. **Ensures the write is durable** (fsync if necessary)
4. Returns confirmation to unblock the snapshot creation

**Synchronization guarantee:** The `REC_SNAPSHOT` record exists in the AHR file before the filesystem snapshot is taken, ensuring perfect temporal alignment between recording and filesystem state.

**Request**

- `Snapshot`
  - `snapshot_id`: uint64 - ID of the filesystem snapshot that was created
  - `label`: string (optional) - human-readable label for this snapshot

**Response**

- `SnapshotStatus`
  - `success`: bool
  - `id`: uint64 - snapshot ID (echoed from request)
  - `anchor_byte`: uint64 - PTY byte offset at snapshot time
  - `ts_ns`: uint64 - timestamp when snapshot was recorded

This is a tagged union, with the `success` field acting as a discriminator. In case of failures, `success = false`, `err = "string"`.

---

## 9. SSE Event Streaming IPC

The recorder provides an optional IPC mechanism for streaming SSE-like events to parent processes, enabling real-time task monitoring in both local and remote modes. This provides a unified event streaming interface that mirrors the REST API's SSE endpoint.

- **Transport:** Unnamed pipe (inherited file descriptor passed via `--events-pipe-fd`)
- **Protocol:** Length-prefixed SSZ-encoded events, sharing the same type/schema as the JSON events sent through the REST API SSE mechanism

**Usage Pattern:**

When launched by the TUI or CLI for task monitoring:

1. Parent process creates an unnamed pipe using `interprocess::unnamed_pipe::tokio::pipe()`
2. Parent passes the sender file descriptor to child via `--events-pipe-fd <FD>`
3. Child establishes `Sender` from the inherited file descriptor
4. Child streams SSZ-encoded events to parent as agent activity occurs
5. Parent receives events via `Receiver`, decodes them, and forwards them to TUI for display

**Event Types:**

The recorder emits events matching the REST API SSE taxonomy (see REST-Service/API.md), but encoded in SSZ format for efficient binary transport:

```rust
// SSZ-encoded equivalent of JSON events
{ type: "status", status: "running", ts: "2024-01-01T12:00:00.000Z" }
{ type: "log", level: "info", message: "Agent started", ts: "2024-01-01T12:00:01.000Z" }
{ type: "thought", content: "Analyzing codebase structure", ts: "2024-01-01T12:00:02.000Z" }
{ type: "tool_start", name: "search_codebase", args: ["pattern"], ts: "2024-01-01T12:00:03.000Z" }
{ type: "tool_output", name: "search_codebase", output: "Found 42 matches", ts: "2024-01-01T12:00:04.000Z" }
{ type: "file_edit", path: "src/main.rs", lines_added: 5, lines_removed: 2, ts: "2024-01-01T12:00:05.000Z" }
```

**Integration with TUI:**

- **Local Mode**: TUI launches `ah agent record` with `--events-pipe-fd`, receives SSZ-encoded events via pipe, decodes them, and displays in task cards
- **Remote Mode**: TUI connects to REST API SSE endpoint, receives JSON-encoded events, and displays in task cards
- **Unified Display**: Same activity display logic processes events from either source after decoding to internal format

**Implementation Notes:**

- Events are SSZ-encoded for efficient binary transport over the pipe (not JSON-encoded like SSE)
- Parent must drain the pipe continuously to prevent blocking the child process
- Pipe is unidirectional (child → parent only)
- Events are sent immediately as agent activity occurs, providing real-time updates
- SSZ decoding in parent process converts binary events to the same internal format as JSON SSE events

---

## 10. Export — Final Interleaving Report

### 10.1 Replay

- Instantiate a `TerminalState` with **large scrollback** (configurable; default 1e6 rows) and initial size from `session.meta.json`.
- Stream through all blocks and all records in chronological order (apply resizes and process data through the vt100 parser).

### 10.2 Collect final lines

- Read the final scrollback + screen rows top→bottom.
- For each row, emit `{ idx, text }`, omitting fully blank tail rows.

### 10.3 Interleave lines with snapshots

- After replaying all events through `TerminalState`, iterate through lines in order.
- For each line, check if it has associated snapshots using `TerminalState::has_snapshot_at_line()`.
- Output formats:
  - **md**: human-friendly table with badges for snapshot rows.
  - **json**: structured array with `{kind:"line"|"snapshot", …}`.
  - **csv**: minimal columns (kind, index/id, ts_ns, text/message/label).

### 10.4 Determinism & edge cases

- Events are processed in chronological order during replay, ensuring deterministic output.
- Multiple snapshots on the same line are preserved in the order they were recorded.

---

## 11. Performance & Resource Targets

- **Write path:** sub-1 ms per REC_DATA on typical 4–16 KiB buffers; Brotli q=4 keeps CPU under 5–10% on modern cores.
- **Viewer latency:** <30 ms frame-to-frame at 60–120 Hz terminal update rates; damage-band hashing limits per-update work to O(screen rows), typically a few dozen.
- **Export time:** ~1–2× wall time of the session data volume (I/O bound); no random seeks required.

---

## 12. Testing Plan

- **Mock agent** that emits deterministic progress lines with heavy `\r` usage and synthetic moments; drives instruction injections programmatically.
- Unit tests for: block header parse/scan, truncated tail recovery, replay idempotence, anchor mapping, export merge.
- Snapshot tests on exporter outputs (md/json/csv) for stable diffs.
- **inspect_ahr** tool (`tests/tools/inspect_ahr.rs`) for manually inspecting .ahr file contents and debugging recording issues. Build with: `cargo build --bin inspect_ahr`, run with: `just inspect-ahr <ahr_file>`.
- **Synchronous snapshot write verification test:** Integration test that verifies the `ah agent fs snapshot` command blocks until the `REC_SNAPSHOT` record is written to the AHR file. The test spawns `ah agent record` with a mock agent, triggers snapshot creation via IPC, and immediately after `ah agent fs snapshot` returns, parses the AHR file to verify the snapshot record exists with correct metadata. Test must handle race conditions by ensuring the AHR file is fully flushed before verification.
- **SSE event streaming test:** Integration test that verifies the unnamed pipe IPC mechanism works correctly. Test creates a pipe, launches `ah agent record` with `--events-pipe-fd`, and verifies that SSE-like events are received in real-time as the mock agent produces activity.

---

## 14. Future Extensions

- Lightweight **frame index** footer for faster mid-session random access.
- Streaming **SSE bridge** to external dashboards.
- Translate the agent-specific output to a normalized JSON stream with event types defined by Agent Harbor

---

## 15. Implementation Notes (Rust crates)

- PTY: `portable-pty`
- Terminal model: `vt100`
- TUI: `ratatui`, `crossterm`
- Compression: `brotli` crate
- JSON: `serde`, `serde_json`
- Binary codec: manual LE writes/reads (`byteorder`), or `bincode` for scaffolding during prototyping
- Time: `time` or `quanta` for ns timestamps
- IPC: `interprocess` crate for unnamed pipes and cross-platform handle transfer

---

## 16. Compatibility & Portability

- Linux, macOS, Windows (PTY availability and UDS vs TCP vary by OS).
- Recording is platform-neutral; replay behaves identically across hosts.
- **Terminal resizing**: When `--cols` and `--rows` are specified, the command attempts to resize the terminal window using xterm-compatible Window Ops escape sequences (`\e[8;<rows>;<cols>t`). This works in xterm, iTerm2, GNOME Terminal, and other VTE-based terminals, but may be disabled in some configurations or unsupported in terminals like tmux, screen, or macOS Terminal. The command will log a warning if the resize attempt doesn't succeed.

---

## 17. Minimal File Format Validator (pseudo-code)

```
loop read header H:
  if H.magic != 'AHRC' or H.version > 1: error
  read H.compressed_len bytes → brotli decode → buffer B
  if decoded len != H.uncompressed_len: error
  parse B as concatenated records; ensure record_count matches
end
```

---

## 18. Deliverables (MVP)

- `ah agent record` with live viewer, IPC for instructions, `.ahr` writer, `.instructions.jsonl` appends
- `ah agent record` with SSE event streaming IPC for real-time task monitoring
- `ah agent replay` producing md/json/csv interleaved report
- `ah agent replay --print-meta` showing meta and quick stats
- `ah agent branch-points` exports the session interleaved outputlines with possible branch points (snapshots)

---

**End of v0.1 spec**
