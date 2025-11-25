# ACP Server Integration — Implementation Status

## Overview

Goal: expose the Agent Harbor execution platform through the Agent Client Protocol (ACP) so that any ACP-compliant editor can talk to `ah-rest-server` the same way it already talks to on-device agents. The ACP reference docs in `resources/acp-specs/docs` (notably `overview/architecture.mdx`, `protocol/overview.mdx`, and `protocol/transports.mdx`) define the JSON-RPC methods we must serve. The REST surface from `specs/Public/REST-Service/API.md` already models session/task lifecycles, so the ACP server becomes an alternative control plane that maps ACP flows (`initialize → session/new → session/prompt → session/update …`) to the existing REST orchestrator.

Target crate: `crates/ah-rest-server`. We will add an `acp` module and reuse `agentclientprotocol/rust-sdk` for:

- JSON-RPC framing, method enums, and capability negotiation
- Formal request/response structs for both Agent and Client sides
- Integration test helpers (the SDK ships working example agents/clients we can embed)

## Execution Strategy

1. Build a dedicated ACP gateway inside `ah-rest-server` that can run either as an in-process Axum upgrade (WebSocket) or as a sidecar stdio bridge launched by `ah agent access-point`.
2. Translate ACP state into existing `TaskManager` primitives so we inherit provisioning, recording, and snapshot behavior.
3. Use Scenario Format fixtures (`specs/Public/Scenario-Format.md`) to deterministically drive ACP conversations and verify end-to-end behavior without real agents.
4. Gate everything behind a config flag until all verification milestones pass.
5. Treat Agent Harbor as the ACP **agent** that owns the authoritative workspace; only advertise `fs.readTextFile` / `fs.writeTextFile` when a session is explicitly configured for in-place editing.

## Test Strategy

- **Unit tests** for configuration, capability negotiation, and JSON-RPC translation live under `crates/ah-rest-server/src/acp`.
- **Scenario-driven integration tests** go in `tests/acp_bridge/` and reuse `crates/ah-scenario-format` to stream scripted ACP messages through the gateway while the mock TaskManager simulates agent execution.
- **SDK interoperability tests** spin up the Rust SDK sample client/agent from `agentclientprotocol/rust-sdk/examples` inside `cargo test` to prove real ACP implementations can connect.
- All new tests run via `just test-rust` and a dedicated shortcut `just test-acp-server`.

## Filesystem Capability Strategy

- Agent Harbor plays the **agent** role in ACP: sessions run inside Harbor-managed workspaces (ZFS/Btrfs/AgentFS/Git per [FS-Snapshots-Overview](Public/FS-Snapshots/FS-Snapshots-Overview.md)), so Harbor—not the editor—owns canonical filesystem state.
- Therefore we **do not request** `fs.readTextFile` / `fs.writeTextFile` for the default sandboxed modes. Editors simply render status/log events while Harbor performs edits inside its isolated branch.
- When tenants intentionally opt into `workingCopy = in-place`, Harbor can advertise and use the ACP filesystem methods. That integration is tracked as an optional milestone so we only surface it after the core ACP gateway ships.

## Terminal / Playback Strategy

- Agent Harbor runs third-party agents itself using `ah agent record`, which already captures byte-perfect PTY output into `.ahr` recordings and streams live events via SSE pipes.
- For every tool invocation emitted by the third-party agent, Harbor immediately records the run inside its sandbox (via `ah agent record`) and then asks the IDE—acting as the ACP client—to **launch a follower terminal** that replays that exact command in real time.
- The replay command we send down is `ah show-sandbox-execution "<original command>" --id <execution_id>` (a thin wrapper around `ah agent replay --session <id> --tool <tool_id> --follow`). The follower behaves exactly like attaching to a tmux pane: it maintains a live connection to the recorder-owned TTY, streams fresh output as soon as it is produced, and forwards any keystrokes back to the running process. It also carries the full shell pipeline so users see precisely what ran, and the execution ID lets Harbor-aware IDEs correlate status/logs; IDEs may strip the `ah show-sandbox-execution` prefix when rendering.
- Live recorder SSE events are mapped onto ACP `session/update` payloads (thought/log/tool sections) while the IDE-owned follower terminal runs the replay command to stream the same PTY bytes with millisecond accuracy. SessionViewer mirrors those followers locally: active executions appear in the status bar, a shortcut opens the follower TTY in a modal, and input typed there is injected back into the third-party agent via the recorder’s `inject_message` TTY (enabling users to respond to prompts such as sudo passwords).
- This design keeps tool execution entirely inside Harbor, reuses the Command-Execution-Tracing + Recorder stack, requires no IDE-specific sandbox knowledge, and still honors ACP’s client-owned terminal semantics.

---

---

### Milestone 0: ACP Gateway Architecture & Config Scaffolding

**Status**: Planned

#### Deliverables

- [ ] Extend `ah-rest-server` config (`crates/ah-rest-server/src/config.rs`) with an `acp` section (enable flag, bind address/port, auth policy, transport mode `stdio|websocket`).
- [ ] Create `acp` module skeleton (connection manager trait, translator stubs, error types) with exhaustive docs that quote the relevant sections of the ACP spec.
- [ ] Wire `server.rs` so that enabling ACP bootstraps the gateway alongside the REST handlers and shares the existing dependency injector (`dependencies.rs`).
- [ ] Document the new config knobs in `specs/Public/REST-Service/Tech-Stack.md` and add a short primer pointing here.

#### Verification

- [ ] `cargo test -p ah-rest-server acp_config_defaults` ensures defaults match spec and disabling the feature leaves the server behavior unchanged.
- [ ] `cargo test -p ah-rest-server acp_flag_enables_gateway` spins up the server in-memory, toggles the flag, and asserts the TCP listener appears only when enabled.
- [ ] `just lint-specs` confirms the updated markdown references build cleanly.

---

### Milestone 1: Transport Layer & Authentication Guardrails

**Status**: Planned

#### Deliverables

- [ ] Add `agentclientprotocol/rust-sdk` (workspace dependency) and wrap its JSON-RPC runtime in a new `AcpTransport` service that supports:
  - stdio pipes (for `ah agent access-point --stdio-acp`) and
  - WebSocket upgrade on `/acp/v1/connect` routed through Axum.
- [ ] Implement a pluggable authenticator that maps ACP `authenticate` payloads to Agent Harbor tenants/projects (API key header, JWT, or session token) and reuses `auth.rs`.
- [ ] Add connection-level rate limiting and idle timeout policies consistent with REST limits.
- [ ] Emit structured tracing spans for handshake, auth, and disconnect events.

#### Verification

- [ ] Integration test `cargo test -p ah-rest-server --test acp_transport_smoke` spawns the gateway, dials it via tokio-tungstenite, and validates JSON-RPC frames echo back with proper ids.
- [ ] Scenario fixture `tests/acp_bridge/scenarios/auth_failure.yaml` drives an `llmResponse`/`agentActions` timeline where authentication fails, asserting the gateway returns the ACP-standard error code.
- [ ] `cargo test -p ah-rest-server acp_rate_limit` fakes rapid connection attempts and ensures the limiter responds with Problem+JSON converted into ACP errors.

---

### Milestone 2: Initialization & Capability Negotiation

**Status**: Planned

#### Deliverables

- [ ] Implement ACP `initialize` handling using the SDK’s capability structs to advertise:
  - Supported transports (`stdio`, `websocket`)
  - Session limits, available slash commands, and **default-off** filesystem capabilities (we only flip `fs.readTextFile` / `fs.writeTextFile` on when sessions run in in-place mode)
  - Terminal support derived from Agent Harbor runtime policies
- [ ] Implement optional `authenticate` request forwarding to the guardrails built in M1.
- [ ] Persist negotiated settings per connection inside an `AcpSessionContext`.
- [ ] Add compatibility matrix docs summarizing which ACP features map to Agent Harbor features.

#### Verification

- [ ] Scenario `tests/acp_bridge/scenarios/initialize_and_auth.yaml` replays the full handshake and asserts (via harness assertions) that `session/update` advertises the negotiated capabilities.
- [ ] Unit test `cargo test -p ah-rest-server acp_initialize_caps_roundtrip` validates we correctly convert between SDK structs and internal enums (including path normalization to absolute paths per ACP requirements).
- [ ] Property test (proptest) ensures unknown capability flags are safely ignored yet logged.

---

### Milestone 3: Session Catalog & Workspace Binding

**Status**: Planned

#### Deliverables

- [ ] Implement ACP `session/new`, `session/list`, and `session/load` requests by translating them into existing TaskManager operations (creating REST tasks, enumerating `sessions` table).
- [ ] Add mapping between ACP `sessionId` strings and Agent Harbor ULIDs; persist cross-reference table so either entry point (REST or ACP) can locate sessions.
- [ ] Support loading paused sessions by mounting the existing workspace snapshot read-only and exposing its metadata back to the ACP client.
- [ ] Provide `session/update` notifications for lifecycle changes (queued, provisioning, running, paused, completed) using the existing SSE event bus.
- [ ] Extend the Scenario Format with `userActions.pause_session` / `userActions.resume_session` primitives so harnesses can express pausing/resuming via REST or ACP semantics.

#### Verification

- [ ] Scenario `tests/acp_bridge/scenarios/session_new_and_load.yaml` creates a session, pauses it, and reloads it through ACP; assertions check that the workspace mount path inside the scenario matches the TOT snapshot provider while the new pause/resume timeline events drive both REST and ACP clients appropriately.
- [ ] Integration test `cargo test -p ah-rest-server --test acp_session_catalog` uses the mock TaskManager backend to create sessions via REST and ensure ACP `session/list` mirrors them (including pagination).
- [ ] Database migration test ensures the cross-reference table enforces foreign keys and cleans up orphaned rows when sessions are deleted.

---

### Milestone 4: Prompt Turn Execution & Streaming Updates

**Status**: Planned

#### Deliverables

- [ ] Implement `session/prompt` so ACP user messages are enqueued as Agent Harbor task instructions (leveraging `TaskManager::inject_message`).
- [ ] Stream Agent Harbor SSE events (`thought`, `tool_use`, `tool_result`, `file_edit`, `log`, `status`) back through ACP `session/update` notifications with correct JSON-RPC ids and `tool_execution_id` correlation.
- [ ] Support `session/cancel` (notification) by invoking the REST cancellation path.
- [ ] Ensure prompts obey context window limits and respond with ACP-standard stop reasons.

#### Verification

- [ ] Scenario `tests/acp_bridge/scenarios/prompt_turn_basic.yaml` reproduces a deterministic timeline from the Scenario Format document, verifying each streamed event (captured by the harness) matches the expected ordering and payload schema.
- [ ] `cargo test -p ah-rest-server --test acp_prompt_backpressure` simulates a slow ACP client and ensures the gateway applies bounded channels so the REST event bus never blocks.

---

### Milestone 5: Recorder Bridge, Follower Playback & Workspace Guardrails

**Status**: Planned

#### Deliverables

- [ ] Wire `ah agent record` (see `specs/Public/ah-agent-record.md`) into the ACP gateway so recorder SSE/IPC events are translated into ACP `session/update` messages in real time (tool start, output chunks, completion, snapshot markers).
- [ ] Extend the Command-Execution-Tracing pipeline so intercepted tool launches feed structured metadata into the recorder (cmd/args/env/cwd) and are tagged for downstream playback.
- [ ] Implement an “IDE follower” command channel: Harbor emits ACP instructions that tell the IDE to run `ah show-sandbox-execution "<cmd>" --id <execution_id>` (wrapping `ah agent replay --session <id> --tool <tool_id> --follow`) so the IDE can attach to the `.ahr` stream and display byte-perfect terminal state without spawning its own sandbox.
- [ ] Update SessionViewer to surface the same follower terminals locally: show active executions in the status bar, add a shortcut that opens the follower TTY in a modal, and send keystrokes back through the recorder’s `inject_message` TTY so users can unblock prompts (e.g., sudo passwords) directly from the UI.
- [ ] Ensure recorder outputs always reference sandbox-relative paths and reject any attempt by upstream agents to exec outside the session workspace (guarded via tracer policies).
- [ ] Provide per-session/tenant policies to disable follower playback export or to redact sensitive commands before they’re replayed to IDEs.

#### Verification

- [ ] Scenario `tests/acp_bridge/scenarios/terminal_only.yaml` records a deterministic tool run, streams live updates to the IDE, issues the `ah show-sandbox-execution` command, and asserts the IDE output matches the `.ahr` playback while the SessionViewer modal reflects the same TTY.
- [ ] Unit tests cover tracer policy enforcement (workspace confinement), recorder-to-ACP translation, replay command construction, and SessionViewer shortcut/modal + input-injection plumbing.
- [ ] Integration test `cargo test -p ah-rest-server --test acp_recorder_follow_mode` launches a mock agent, records an `.ahr`, triggers IDE replay via ACP, and compares the IDE’s rendered output to the original PTY stream (including injected keystrokes routed through `inject_message`).

---

### Milestone 6: Plans, Modes, Permissions, and Slash Commands

**Status**: Planned

#### Deliverables

- [ ] Implement ACP plan support: translate Agent Harbor supervisor summaries into `agent_plan/update` notifications and accept plan acknowledgements.
- [ ] Support `session/set_mode` to switch between plan mode, code mode, and eval mode, mapping them to Agent Harbor runtime profiles.
- [ ] Surface slash commands exposed by Agent Harbor (run tests, open IDE, branch session) via `session/update` command catalog updates.
- [ ] Implement `session/request_permission` round-trips so that potentially destructive tool calls (e.g., `fs.write_text_file`) can request user approval based on tenant policies.

#### Verification

- [ ] Scenario `tests/acp_bridge/scenarios/plan_and_permissions.yaml` models a flow where the agent publishes a plan, switches modes, and requests permission for a filesystem write; harness assertions verify each stage.
- [ ] Unit test `cargo test -p ah-rest-server acp_mode_translation` ensures internal mode enums stay in sync with ACP definitions.
- [ ] Automation test using the SDK sample client triggers slash commands and validates they appear with correct metadata in the ACP stream.

---

### Milestone 7: Resilience, Metrics, and Multi-Client Concurrency

**Status**: Planned

#### Deliverables

- [ ] Add per-connection metrics (latency histograms, active sessions, dropped messages) exported through the existing Prometheus stack.
- [ ] Implement automatic reconnection support so ACP clients can resume sessions after transient network failures (leveraging SDK session tokens).
- [ ] Harden error handling: classify fatal vs. recoverable errors, ensure JSON-RPC errors adhere to ACP codes, and add structured audit logs.
- [ ] Load-test harness that spins up multiple Scenario Format clients concurrently to validate concurrency and tenant isolation.

#### Verification

- [ ] Benchmark test `cargo test -p ah-rest-server --test acp_load_balancer` spawns 50 simulated ACP clients, each replaying `tests/acp_bridge/scenarios/prompt_turn_basic.yaml`, and asserts no dropped events.
- [ ] Metrics snapshot test scrapes the `/metrics` endpoint after a scenario run and checks for the new gauges/counters with non-zero values.
- [ ] Chaos test (feature-gated) randomly terminates ACP connections mid-turn and ensures reconnection logic replays missed events without duplications (validated via scenario assertions).

---

### Milestone 8: Optional In-Place Filesystem Passthrough

**Status**: Planned

#### Deliverables

- [ ] When a session explicitly selects `workingCopy = in-place`, advertise ACP filesystem capabilities and implement `fs/read_text_file` / `fs/write_text_file` by proxying to the editor-owned workspace via the REST task file APIs.
- [ ] Reuse `ah-fs-snapshots` provider metadata to verify absolute paths belong to the opted-in working copy; refuse access to other directories.
- [ ] Provide tenant-level policy controls so administrators must opt into exposing local filesystem state before the capability bit is set.
- [ ] Document the operational caveats (no AgentFS isolation, relies on client to persist edits) inside `specs/Public/Configuration.status.md`.

#### Verification

- [ ] Scenario `tests/acp_bridge/scenarios/in_place_fs.yaml` runs a session with in-place mode enabled, exercises read/write operations, and confirms the editor receives the expected diffs.
- [ ] Integration test `cargo test -p ah-rest-server --test acp_fs_passthrough` uses a temporary on-disk workspace mounted via the client harness to ensure edits round-trip correctly.
- [ ] Policy test ensures sessions without the opt-in continue to advertise `fs.* = false` even if workloads request it.

---

## Outstanding Tasks After Milestones

- Define a compatibility matrix for third-party ACP clients (VS Code, Cursor, Zed) once the server reaches beta.
- Extend Scenario fixtures with negative-path coverage (malformed JSON-RPC, outdated schema versions).
- Determine whether to expose ACP over QUIC once the spec finalizes HTTP streaming transport.

Once all milestones are implemented and verified, update this status document with:

1. Implementation details and source file references per milestone (mirroring other status files).
2. Checklist updates (`[x]`) and remaining outstanding tasks.
