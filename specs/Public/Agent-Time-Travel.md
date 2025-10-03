## Agent Time-Travel — Product and Technical Specification

### Summary

Agent Time-Travel lets a user review an agent's coding session and jump back to precise moments in time to intervene by inserting a new chat message. Seeking to a timestamp restores the corresponding filesystem state using filesystem snapshots (FsSnapshots). The feature integrates across CLI, TUI, WebUI, and REST, and builds on the snapshot provider model referenced by other docs (see [FS-Snapshots-Overview.md](FS%20Snapshots/FS-Snapshots-Overview.md)).

### Implementation Phasing

The initial implementation will focus on supporting regular FsSnapshot on copy-on-write (CoW) Linux filesystems (such as ZFS and Btrfs), using a session recorder based on Claude Code hooks. An end-to-end prototype will be developed for the entire Agent Time-Travel system, including session recording, timeline navigation, and snapshot/seek/branch operations, to validate the core workflow and user experience. Once this prototype is functional, we will incrementally add support for additional recording and snapshotting mechanisms, including AgentFS (FSKit/WinFsp) on macOS/Windows and Git-based snapshots as a universal fallback.

First targeted agent: Claude Code. We will leverage its hook system (see [Claude-Code-Hooks.md](3rd-Party%20Agents/Claude-Code-Hooks.md) and [Claude-Code.md](3rd-Party%20Agents/Claude-Code.md)) to emit `SessionMoment`s at tool boundaries (e.g., `PostToolUse`) and to capture transcript paths for resume/trim flows.

Testability strategy from day one:

- A scriptable Mock Agent will be introduced to deterministically produce tool-step boundaries, stdout/stderr patterns, and optional synthetic session artifacts. This enables precise, repeatable tests of time-travel flows (seek → snapshot mount → branch → resume/replay) without depending on external tools.
- For off‑the‑shelf agents that talk to remote APIs, we will provide a Mock API Server to simulate provider responses and force edge cases (timeouts, partial successes, retries). Agents under test will be pointed at the mock via env/config overrides.

### Goals

- Enable scrubbing through an agent session with exact visual terminal playback and consistent filesystem state.
- Allow the user to pause at any moment, inspect the workspace at that time, and create a new SessionBranch with an injected instruction.
- Provide first-class support for ZFS/Btrfs and AgentFS where available; offer a robust Git-based fallback on platforms/filesystems without CoW support.
- Expose a consistent API and UX across WebUI, TUI, and CLI.

### Non-Goals

- Full semantic capture of each application’s internal state (e.g., Vim buffers). We replay terminal output and restore filesystem state.
- Reflowing terminal content to arbitrary sizes. Playback uses a fixed terminal grid with recorded resize events.
- A kernel-level journaling subsystem; we rely on filesystem snapshots and pragmatic fallbacks.

### Concepts and Terminology

- **SessionRecording**: A terminal I/O session timeline (e.g., asciinema v2). Visually faithful to what the user saw; does not encode TUI semantics.
- **SessionMoment**: A labeled point in the session recording timeline (auto or manual). Used for navigation.
- **FsSnapshot**: A SessionMoment that has an associated filesystem snapshot reference (snapshot created near‑synchronously with the moment).
- **SessionFrame**: A visual state at a specific timestamp; the player can seek and render the SessionFrame.
- **SessionTimeline**: The ordered set of events (logs, SessionMoments, FsSnapshots, resizes) across a session.
- **SessionBranch**: A new session created from a SessionMoment and its associated FsSnapshot’s filesystem state with an injected chat message.

### Architecture Overview

- **Recorder**: Captures terminal output as an asciinema session recording (preferred) or ttyrec; emits SessionMoments at logical boundaries (e.g., per-command). The initial prototype will use a recorder based on Claude Code hooks.
- **FsSnapshot Manager**: Creates and tracks filesystem snapshots; maintains mapping {moment → snapshotId}.
- **Snapshot Provider Abstraction**: Chooses provider per host (ZFS → Btrfs → AgentFS → Git). AgentFS provides cow‑overlay (path‑stable CoW) isolation on macOS/Windows. See Provider Matrix below.
- **SessionTimeline Service (REST)**: Lists FsSnapshots/SessionMoments, seeks, and creates SessionBranches; streams session recording events via SSE.
- **Players (WebUI/TUI)**: Embed the session recording; render streaming SessionRecordings in real-time and allows seeking to arbitrary SessionFrames; orchestrate SessionBranch actions.
- **Workspace Manager**: Mounts read-only snapshots for inspection and prepares writable clones/upper layers for SessionBranches.

### SessionRecording and SessionTimeline Model

- **Format**: asciinema v2 JSON with events [time, type, data]; optional input events for richer analysis. Idle compression is configurable.
- **SessionMoments**: Auto moments at tool boundaries via agent‑provided hooks (e.g., Claude Code hooks) and runtime milestones (provisioned, tests passed). Manual moments via UI/CLI.
- **Random Access**: Web player supports `startAt` and moments; for power users/offline analysis we may store a parallel ttyrec to enable IPBT usage.
- **Alternate Screen Semantics**: Full-screen TUIs (vim, less, nano) switch to the alternate screen; scrollback of earlier output is not available while paused on the alternate screen. Navigation uses session timeline seek rather than scrollback.

### FsSnapshots and Providers (multi‑OS)

- **Creation Policy**:

  - Default: Create an FsSnapshot at each shell command boundary and at important runtime milestones.
  - Max frequency controls and deduplication to avoid thrashing during rapid events.
  - FsSnapshots include: id, ts, label, provider, snapshotRef, notes.

- **Provider Preference (host‑specific)**:
  - Linux:
    - ZFS: instantaneous snapshots and cheap writable clones (SessionBranch from snapshot via clone).
    - Btrfs: subvolume snapshots (constant-time), cheap writable snapshots for SessionBranching.
    - Git fallback: capture shadow commits with a temporary index (include untracked optional); materialize branches via `git worktree` when isolation is desired.
    - Copy fallback: Present in early versions of AH, but now removed. Please clean up references to it if noticed in the code or the specs.
    - AgentFS (FSKit, WinFsp): Provide a user-space filesystem with native snapshots/branches for inspection and SessionBranching, with per-process cow-overlay (path-stable CoW) mounts.
- **SessionBranch Semantics**:
  - Writable clones are native on ZFS/Btrfs. On macOS and Windows, SessionBranching is implemented via AgentFS (FSKit/WinFsp) rather than native OS snapshots.
  - SessionBranches are isolated workspaces; original session remains immutable.

### AgentFS on macOS and Windows

- macOS (FSKit): Ship an FSKit filesystem extension implementing a copy‑on‑write overlay over the host filesystem. For each task, mount a per‑task overlay root and `chroot` the agent process into it to preserve original project path layout while writing to the CoW upper. This preserves build and config paths and enables efficient incremental builds.
- Windows (WinFsp): Ship a WinFsp filesystem implementing the same CoW overlay. Mount per‑task at a stable path and map that path to a per‑process drive letter (e.g., `S:`) using per‑process device maps so the agent sees the original project path under `S:`. This provides a chroot‑like illusion on Windows.
- Windows Containers (alternative): Support process‑isolated containers where the container FS view (wcifs overlay) provides the consistent working directory path, analogous to Linux containers.

### Syncing Terminal Time to Filesystem State

- **Runtime Integration**: The agent execution system executes a hook that creates the snapshot in between chat messages, agent thinking streams and tool executions.
- **Advanced (future)**: eBPF capture of PTY I/O and/or FS mutations
- **Multi‑OS Sync Fence**: When multi‑OS testing is enabled, each execution cycle performs `fs_snapshot_and_sync` on the leader (create FsSnapshot, then fence Mutagen sessions to followers) before invoking `ah agent followers run`. See [Multi-OS Testing.md](Multi-OS%20Testing.md).

### Restarting the agent from a SessionMoment

This feature hinges on the ability to reconstruct two distinct state planes at the chosen SessionMoment: (1) the workspace filesystem and (2) the agent’s internal conversation/session state. Filesystem state is restored via FsSnapshots; agent state restoration uses agent‑specific checkpoint/resume mechanisms where available, or a conservative "prompt replay" fallback when not.

Scope and assumptions:

- Filesystem: We always restore an immutable, read‑only mount for inspection and a writable clone for a SessionBranch using the provider preference policy described above. This guarantees deterministic file state at the selected timestamp.
- Agent session: Behavior is agent‑specific. Some agents support first‑class checkpoints; others allow resuming recent sessions; others provide only stateless, prompt‑driven operation. We maintain per‑agent integration notes under `specs/Public/3rd-Party Agents/` to drive precise restore flows.

Baseline flows (ordered by fidelity):

1. Checkpoint restore (preferred when agent supports it)

   - Detect a compatible checkpoint near the target `SessionMoment` via the session timeline metadata emitted during recording (e.g., `timeline.sessionMoment` with `agentCheckpointId`).
   - Create a SessionBranch from the associated FsSnapshot so the workspace matches the checkpoint’s file view.
   - Launch the agent in "resume from checkpoint" mode with the checkpoint ID and the writable branch workspace mounted as its project root. Inject the user’s new message as the first turn after resume.

2. Session resume with trim (when agent persists conversation transcripts but lacks explicit checkpoints)

   - Identify the persisted session artifacts (location and format per agent notes). Examples: JSON/JSONL logs, SQLite stores, or proprietary directories.
   - Create a SessionBranch from the target FsSnapshot.
   - Inside the branch workspace, prepare a "trimmed session view" that logically ends at the selected `SessionMoment`:
     - If the on‑disk session format is append‑only and tolerant to truncation, copy the session file(s) and truncate at the last event ≤ target time.
     - If the format requires index consistency, reconstruct a new minimal session DB/file containing events up to the target (refer to per‑agent schema in the 3rd‑party spec).
     - Never modify the original session files outside the branch. All edits occur in the SessionBranch view to preserve the original session integrity.
   - Relaunch the agent in "resume prior session" mode pointing to the trimmed session artifacts in the branch, then inject the new user message.

3. Prompt replay (fallback for stateless agents)
   - Extract the prompt turns up to the target timestamp from the captured terminal stream and AH task files (initial and follow‑up tasks). Where feasible, prefer fetching structured prompts from agent logs instead of scraping terminal output.
   - Launch the agent fresh in the SessionBranch workspace and replay the concatenated turns to reconstruct approximate context, then inject the new user message.
   - Note: This yields lower fidelity than checkpoint/resume; we annotate the new SessionBranch with `contextReconstruction: "replay"` and surface a UI hint.

Synchronization between terminal time and agent state:

- The recorder emits auto `SessionMoment`s at shell boundaries and important milestones; FsSnapshots are taken at these fences. For agents that emit checkpoint/resume IDs, we capture them as timeline events and cross‑reference them with FsSnapshots.
- When intervening at an arbitrary timestamp that is not precisely on a fence, we snap to the nearest prior `FsSnapshot` and, if needed, fast‑forward the agent session state to the chosen time by trimming (flow 2) or replaying non‑mutating interactions. We do not attempt to mutate file state beyond the chosen snapshot; instead we choose the previous snapshot to maintain filesystem and transcript coherence.

Launch semantics for the new SessionBranch:

- Workspace: Writable clone/branch/worktree per provider semantics; original session remains immutable unless working-copy=in-place.
- Process isolation: The agent process is launched bound to the SessionBranch workspace view (Linux: chroot/container; macOS: AgentFS FSKit; Windows: AgentFS WinFsp) as specified in AgentFS docs.
- Message injection: The REST/TUI/WebUI afford a text box for the injected message. The runner translates this into agent‑specific CLI/IPC arguments.

Safety and validation:

- Before launching, validate agent support level: `checkpoint | resume | stateless` using the per‑agent catalog; emit a clear warning when falling back to replay.
- Verify that the selected FsSnapshot exists and mounts successfully; otherwise propose the nearest valid snapshot.
- For resume/trim, operate only on copied artifacts in the SessionBranch. Maintain a backup of pre‑trim copies in the branch under `.ah/restore/` for diagnostics.

Observability:

- Record restore provenance in the new session: `{ fromSessionId, fromTs, fsSnapshotId, method: checkpoint|resume|replay, agentDetails }`.
- Emit timeline events: `timeline.sessionBranch.created` with the method and any checkpoint IDs.

Agent catalog requirements:

- Each 3rd‑party agent spec must define: how to launch in resume/checkpoint mode, storage paths and formats for sessions, how to safely trim, and how to inject an initial message on resume. See [3rd-Party-Agent-Description-Template.md](3rd-Party%20Agents/3rd-Party-Agent-Description-Template.md) (sections: Checkpointing, Session continuation, Storage format, Reverse‑engineering policy).

Implementation Plan (high‑level, test‑driven):
Phase 0 — Test harness foundation (Mock Agent + Mock API Server)

- Implement a `mock-agent` binary (Rust) driven by a simple scenario DSL (YAML/JSON): steps emit terminal output, tool boundaries, exit codes, and optional synthetic "session artifacts". Provide hooks to signal `SessionMoment`s and to request FsSnapshots at step fences.
- Implement a `mock-agent-api` server (Rust) to emulate remote model/tool providers. Provide deterministic response scripts, latency/failure injection, and record/replay. Agents under test can target it via env/config overrides.
- Tests: repeatable end‑to‑end seek→snapshot→branch flows entirely with mocks; coverage of edge cases (rapid steps, no‑ops, long idle, failures).

Phase 1 — Claude Code as the first real agent

- Recorder: Integrate Claude Code hooks (primarily `PostToolUse`) to emit `SessionMoment`s and capture `transcript_path`. Persist timeline events alongside FsSnapshot IDs.
- Restore: Implement resume/trim flow for Claude Code transcripts (JSONL) inside a SessionBranch. Never mutate originals; operate on copied/trimmed files in the branch.
- Tests: E2E flows using Claude Code with a local project; hook‑driven moments align with FsSnapshots; injected message accepted after resume.

Phase 2 — Drive off‑the‑shelf agents via Mock API Server

- For agents that speak HTTP to their providers, route their API base URL to `mock-agent-api`. Script deterministic tool behaviors and boundary events to align with FsSnapshots.
- Tests: deterministically reproduce multi‑step tasks and faults (rate limit, partial results) and verify seek/branch behavior is coherent.

Phase 3 — Checkpoint agent integration

- Wire a checkpoint‑capable agent; capture checkpoint IDs in the timeline; restore directly by ID.
- Tests: seek to checkpoints, branch, and verify zero replay/trim; performance budgets for restore under N seconds.

Phase 4 — Cross‑platform workspace binding

- macOS FSKit and Windows WinFsp SessionBranch mounting aligned with AgentFS CLI. Ensure agent processes are contained within the branch view.
- Tests: smoke tests for resume/replay inside FSKit/WinFsp mounts; verify isolation and permissions.

Phase 5 — REST/TUI/WebUI integration polish

- REST endpoints finalized as below; TUI/WebUI “Intervene” dialog implements all methods with clear UX annotations and fallbacks.
- Tests: API contract tests and UI smoke flows with mocked agents.

### REST API Extensions

- `GET /api/v1/sessions/{id}/timeline`

  - Returns SessionMoments and FsSnapshots ordered by time.
  - Response:

  ```json
  {
    "sessionId": "...",
    "durationSec": 1234.5,
    "recording": { "format": "cast", "uri": "s3://.../cast.json" },
    "moments": [
      { "id": "m1", "ts": 12.34, "label": "git clone", "kind": "auto" }
    ],
    "fsSnapshots": [
      {
        "id": "s1",
        "ts": 12.4,
        "label": "post-clone",
        "provider": "btrfs",
        "snapshot": { "id": "repo@tt-001", "mount": "/.snapshots/..." }
      }
    ]
  }
  ```

- `POST /api/v1/sessions/{id}/fs-snapshots`

  - Create a manual FsSnapshot near a timestamp; returns snapshot ref.

- `POST /api/v1/sessions/{id}/moments`

  - Create a manual SessionMoment at/near a timestamp.

- `POST /api/v1/sessions/{id}/seek`

  - Parameters: `ts`, or `fsSnapshotId`.
  - Returns a short‑lived read‑only mount (host path and/or container path) for inspection; optionally pauses the session player at `ts`.

- `POST /api/v1/sessions/{id}/session-branch`

  - Parameters: `fromTs` or `fsSnapshotId`, `name`, optional `injectedMessage`.
  - Creates a new session (SessionBranch) with a writable workspace cloned/overlaid from the FsSnapshot.
  - Response includes new `sessionId` and workspace mount info.

- `GET /api/v1/sessions/{id}/fs-snapshots`

  - Lists underlying provider snapshots/checkpoints with metadata (for diagnostics and retention tooling).

- SSE additions on `/sessions/{id}/events`
  - New event types: `timeline.sessionMoment`, `timeline.fsSnapshot.created`, `timeline.sessionBranch.created`.

### CLI Commands

See the `ah agent fs` [commands](./CLI.md).

### WebUI UX

- **Player Panel**: Embed `<asciinema-player>` with SessionMoments and a scrubber. Time cursor shows nearest FsSnapshot and label.
- **Pause & Intervene**: On pause, surface “Inspect snapshot” and “SessionBranch from here”.
- **Inspect Snapshot**: Mounts read‑only view; open a lightweight file browser and offer “Open IDE at this point”.
- **SessionBranch From Here**: Dialog to enter an injected message and name; creates a new session (SessionBranch); link both sessions for side‑by‑side comparison.
- **History View**: SessionTimeline list with filters (auto/manual SessionMoments, FsSnapshots only).

### TUI UX

- **SessionTimeline Bar**: Keyboard scrubbing with SessionMoments (jump prev/next), current time, and FsSnapshot badges.
- **Keys**:
  - Space: pause/resume
  - [ / ]: prev/next SessionMoment; { / }: prev/next FsSnapshot
  - i: Intervene (SessionBranch dialog)
  - s: Seek and open read‑only snapshot in left pane; right pane keeps the player/logs

### Data Model Additions (Session)

- `recording`: `{ format: "cast"|"ttyrec", uri, width, height, hasInput }`
- `sessionTimeline`: `{ durationSec, moments: [...], fsSnapshots: [...] }`
- `fsSnapshots[*]`: `{ id, ts, label, provider, snapshot: { id, mount?, details? } }`
- `sessionBranchOf` (optional): parent session id and fsSnapshot id when branched.

### Security and Privacy

- **Keystrokes**: If input capture is enabled, redact known password prompts (heuristics based on ECHO off and common prompts). Make input capture opt‑in.
- **Access Control**: SessionTimeline/seek/SessionBranch require the same permissions as session access; snapshot mounts use least‑privilege read‑only where applicable.
- **Data Retention**: Separate retention for session recordings vs snapshots; defaults minimize data exposure. Encrypt at rest when stored remotely.

### Performance, Retention, and Limits

- **Snapshot Rate Limits**: Min interval between FsSnapshots; coalesce within a small window (e.g., 250–500 ms) to avoid bursty commands creating many snapshots.
- **Retention**: Policies by count/age/size. Prune unreferenced provider snapshots and expired Git snapshots.
- **Storage**: Session recording files compressed; offload to object storage. Mounts are short‑lived and garbage‑collected.

### Failure Modes and Recovery

- **Snapshot Creation Fails**: Create a SessionMoment with `fsSnapshot=false` and reason; continue session recording; allow manual retry.
- **Seek Failure**: Report provider error and suggest nearest valid FsSnapshot.
- **Provider Degraded**: Fall back per provider preference, with explicit event logged to the session timeline.

### Provider Semantics Matrix (summary)

- **ZFS**: Snapshots and clones — ideal for FsSnapshots and SessionBranches.
- **Btrfs**: Subvolume snapshots — ideal for FsSnapshots and SessionBranches.
- **Git snapshots**: Universal baseline when CoW is unavailable; store content/state deltas in-repo with efficient pruning.

### Open Issues and Future Work

- eBPF PTY and FS hooks for automatic, runner‑independent capture.
- FSKit/AgentFS backend maturation on macOS for robust SessionBranching without kexts.
- Windows containers integration to provide stronger per‑session isolation when SessionBranching.
