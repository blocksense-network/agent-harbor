# ACP Server Integration — Implementation Status

## Overview

Goal: expose the Agent Harbor execution platform through the Agent Client Protocol (ACP) so that any ACP-compliant editor can talk to `ah-rest-server` the same way it already talks to on-device agents. The ACP reference docs in `resources/acp-specs/docs` (notably `overview/architecture.mdx`, `protocol/overview.mdx`, and `protocol/transports.mdx`) define the JSON-RPC methods we must serve. The REST surface from `specs/Public/REST-Service/API.md` already models session/task lifecycles, Command Execution Tracing is specified in `specs/Public/Command-Execution-Tracing.md`, the recorder/user-experience details live in `specs/Public/ah-agent-record.md`, our scenario harness is documented in `specs/Public/Scenario-Format.md`, and the custom ACP/REST extensions are described in `specs/ACP.extensions.md`. Collectively, these references ensure a newcomer understands the end-to-end pipeline we’re wiring together as the ACP server becomes an alternative control plane that maps ACP flows (`initialize → session/new → session/prompt → session/update …`) to the existing REST orchestrator.

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

### Recent Progress (2025-11-26)

- WebSocket and stdio transports now run through the SDK `ValueDispatcher`, preserving raw JSON params via `WrappedRequest` so Harbor-specific fields survive decoding.
- Vendored SDK exports `WrappedRequest` (typed + raw) to propagate `_meta` and extension fields into handlers.
- `session/new` translation now expects Harbor-specific fields (`repoUrl`, `branch`, `labels`, etc.) under `_meta` (prompt/agent stay at the root); ACP tests updated accordingly.
- Added dispatcher compatibility shims for legacy clients (`ping`, `session/cancel` as requests, string-only `session/prompt` messages) and schema defaults for `session/load` so the full ACP test suite passes under the SDK runtime.
- Rolled back the experimental LocalSet/notify refactor; keeping the per-connection loops single-threaded with dispatcher-driven notifications until we have a Send-safe SDK path.
- Added ACP pagination regression test (`acp_session_list_pagination`) to lock in offset/limit slicing on `session/list`.
- Prompt/cancel/pause/resume now require a live `TaskController` and propagate errors instead of silently best-effort delivery; mock dependencies ship a lightweight controller so scenario/integration tests still run.
- Terminal follow now derives follower commands strictly from recorder/tool metadata; when metadata is missing (mock sessions), a synthetic `tool_use` event is recorded instead of trusting raw client strings.

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

**Status**: Completed (2025-11-25)

#### Deliverables

- [x] Extend `ah-rest-server` config (`crates/ah-rest-server/src/config.rs`) with an `acp` section (enable flag, bind address/port, auth policy, transport mode `stdio|websocket`).
- [x] Create `acp` module skeleton (connection manager trait, translator stubs, error types) with exhaustive docs that quote the relevant sections of the ACP spec.
- [x] Wire `server.rs` so that enabling ACP bootstraps the gateway alongside the REST handlers and shares the existing dependency injector (`dependencies.rs`).
- [x] Document the new config knobs in `specs/Public/REST-Service/Tech-Stack.md` and add a short primer pointing here.

#### Implementation Details

- Added `AcpConfig`, `AcpTransportMode`, and `AcpAuthPolicy` to `crates/ah-rest-server/src/config.rs` with defaults that keep ACP disabled (`enabled = false`, local WebSocket bind on `127.0.0.1:3031`, inherited REST auth) while documenting the relevant ACP spec sections.
- Introduced an `acp` module scaffold (`acp::gateway`, `acp::errors`, `acp::translator`) that provides a bindable gateway wrapper, JSON-RPC translator stub, and error taxonomy to be extended in later milestones.
- `server.rs` now boots the optional `AcpGateway` alongside the REST router using the shared `AppState`; when enabled, the gateway binds its own listener and exposes `acp_bind_addr()` for verification without altering REST behavior.
- Tech stack doc now includes an ACP primer and config block illustrating `enabled`, `bind_addr`, `transport`, and `auth_policy` knobs with a pointer back to this status file.

#### Key Source Files

- `crates/ah-rest-server/src/config.rs`
- `crates/ah-rest-server/src/acp/`
- `crates/ah-rest-server/src/server.rs`
- `specs/Public/REST-Service/Tech-Stack.md`

#### Outstanding Tasks

- [x] Fill in transport/authentication logic per Milestone 1.
- [x] Replace the stub router with the real JSON-RPC runtime once the SDK is wired.
- [x] Add `agentclientprotocol/rust-sdk` as a workspace dependency and swap the echo loop for the SDK runtime.
- [x] Implement stdio transport plumbing for `ah agent access-point --stdio-acp`.

#### Verification

- [x] `cargo test -p ah-rest-server acp_config_defaults` ensures defaults match spec and disabling the feature leaves the server behavior unchanged.
- [x] `cargo test -p ah-rest-server acp_flag_enables_gateway` spins up the server in-memory, toggles the flag, and asserts the TCP listener appears only when enabled.
- [x] `just lint-specs` confirms the updated markdown references build cleanly.

---

### Milestone 1: Transport Layer & Authentication Guardrails

**Status**: Completed (2025-11-26)

#### Deliverables

- [x] Add `agentclientprotocol/rust-sdk` (workspace dependency) and wrap its JSON-RPC runtime in a new `AcpTransport` service that supports:
  - stdio pipes (for `ah agent access-point --stdio-acp`) and
  - WebSocket upgrade on `/acp/v1/connect` routed through Axum.
- [x] Implement a pluggable authenticator that maps ACP `authenticate` payloads to Agent Harbor tenants/projects (API key header, JWT, or session token) and reuses `auth.rs`.
- [x] Add connection-level rate limiting and idle timeout policies consistent with REST limits.
- [x] Emit structured tracing spans for handshake, auth, and disconnect events.

#### Implementation Details

- Added `AcpTransportState` with auth config reused from `auth.rs`, connection semaphore, and idle timeout; WebSocket handler validates `Authorization` or `?api_key=` and returns structured Problem+JSON on failure.
- Gateway now mounts a real `/acp/v1/connect` WebSocket endpoint; stdio mode remains a no-op placeholder until SDK stdio plumbing is wired.
- Minimal JSON-RPC echo handler returns `{"id":<id>,"result":<params>}` to keep the transport exercised while higher-level translators land.
- Connection limit and idle timeout made configurable (`connection_limit`, `idle_timeout_secs`).

#### Key Source Files

- `crates/ah-rest-server/src/acp/transport.rs`
- `crates/ah-rest-server/src/acp/gateway.rs`
- `crates/ah-rest-server/tests/acp_transport.rs`

#### Outstanding Tasks

- [ ] Replace echo handler with SDK-backed JSON-RPC runtime and stdio transport plumbing.
- [ ] Expand authentication to JWT claims-to-tenant mapping once tenant metadata is available.
- [x] Handle ACP `authenticate` as an RPC (not just handshake) and advertise `_meta.agent.harbor` capabilities during `initialize`.
- [x] Persist negotiated capabilities using SDK types instead of ad-hoc structs.
- [ ] Wire ACP WebSocket/stdio transports to the SDK dispatcher (frames→Value→dispatcher) and remove the manual handler/echo loop.

#### Verification

- [x] Integration test `cargo test -p ah-rest-server --test acp_transport_smoke` spawns the gateway, dials it via tokio-tungstenite, and validates JSON-RPC frames echo back with proper ids.
- [x] Scenario fixture `tests/acp_bridge/scenarios/auth_failure.yaml` drives an `llmResponse`/`agentActions` timeline where authentication fails, asserting the gateway returns the ACP-standard error code.
- [x] `cargo test -p ah-rest-server acp_rate_limit` fakes rapid connection attempts and ensures the limiter responds with Problem+JSON converted into ACP errors.

---

### Milestone 2: Initialization & Capability Negotiation

**Status**: Completed (2025-11-26) — updated (2025-11-26) with session context + compatibility matrix

#### Deliverables

- [x] Implement ACP `initialize` handling using the SDK’s capability structs to advertise:
  - Supported transports (`stdio`, `websocket`)
  - Session limits, available slash commands, and **default-off** filesystem capabilities (we only flip `fs.readTextFile` / `fs.writeTextFile` on when sessions run in in-place mode)
  - Terminal support derived from Agent Harbor runtime policies
- [x] Implement optional `authenticate` request forwarding to the guardrails built in M1.
- [x] Persist negotiated settings per connection inside an `AcpSessionContext`.
- [x] Add compatibility matrix docs summarizing which ACP features map to Agent Harbor features.
- [x] Parse `initialize`/`authenticate` payloads using the ACP schema (lite structs) so capability negotiation/auth flows align with SDK types; capability state is stored with SDK `AgentCapabilities`.

#### Verification

- [x] Scenario `tests/acp_bridge/scenarios/initialize_and_auth.yaml` + `cargo test -p ah-rest-server --test acp_initialize` now asserts the `initialize` response advertises `_meta.agent.harbor` capabilities and websocket transport before driving the status playback.
- [x] Unit test `cargo test -p ah-rest-server acp_initialize_caps_roundtrip` validates we correctly convert between SDK structs and internal enums (including path normalization to absolute paths per ACP requirements).
- [x] Property + log test ensures unknown capability flags are safely ignored yet surfaced as warnings (`JsonRpcTranslator::ignore_unknown_caps`).
- [x] Unit test `cargo test -p ah-rest-server acp_authenticate_rpc_uses_payload_tokens` exercises the new `authenticate` RPC and checks capability advertisement includes `_meta.agent.harbor`.

#### Compatibility Matrix (Harbor ↔ ACP core)

| ACP feature | Harbor mapping | Notes |
|-------------|----------------|-------|
| `initialize.capabilities.transports` | Derived from `AcpConfig.transport` (`websocket` always, `stdio` when configured) | Persisted per-connection inside `AcpSessionContext` and required before session RPCs |
| `initialize.capabilities.filesystem` | Default `false/false` until in-place mode (Milestone 12) | Guardrails enforced in translator; unknown flags ignored |
| `initialize.capabilities.terminal` | Always `true` (Harbor owns recorder + follower channel) | Terminal replay hooks arrive in Milestones 5–7 |
| Auth forwarding | Uses REST `AuthConfig` (API key/JWT) | Shared problem+JSON errors with REST |

#### Session Context Snapshot

Each WebSocket connection records the negotiated capabilities and a set of session subscriptions in `AcpSessionContext`; all session RPCs are gated on a completed `initialize` call. Event subscriptions are seeded with a synthetic status notification so clients see the current lifecycle immediately after `session/new` or `session/load`.

#### Outstanding Tasks

- [x] Advertise `_meta.agent.harbor` capability blocks (workspace, snapshots, pipelines) during `initialize` per `specs/ACP.extensions.md`.
- [x] Add explicit ACP `authenticate` RPC handling wired to `AuthConfig`, keeping handshake auth for transport setup.
- [x] Persist negotiated capabilities in shared state using SDK structs (remove bespoke capability structs).
- [ ] Port initialize/auth handling to the SDK dispatcher once transport refactor lands.

---

### Milestone 3: Session Catalog & Workspace Binding

**Status**: Completed (2025-11-26)

#### Deliverables

- [x] Implement ACP `session/new`, `session/list`, and `session/load` requests by translating them into existing TaskManager operations (creating REST tasks, enumerating `sessions` table).
- [x] Add mapping between ACP `sessionId` strings and Agent Harbor ULIDs; persist cross-reference table so either entry point (REST or ACP) can locate sessions. *(Currently 1:1 reuse of Harbor session IDs; schema migration for dedicated cross-ref deferred to later milestone).*
- [x] Support loading paused sessions by mounting the existing workspace snapshot read-only and exposing its metadata back to the ACP client (workspace metadata now flags read-only and includes snapshot provider when status=paused).
- [x] Provide `session/update` notifications for lifecycle changes (queued, provisioning, running, paused, completed) using the existing SSE event bus.
- [ ] Extend the Scenario Format with `userActions.pause_session` / `userActions.resume_session` primitives so harnesses can express pausing/resuming via REST or ACP semantics.
#### Implementation Details

- Added JSON-RPC handlers for `session/new`, `session/list`, and `session/load` in `acp::transport`, translating ACP payloads into `CreateTaskRequest` with safe defaults (local runtime, git/none repo detection, Claude Sonnet agent) before delegating to `SessionService`.
- Introduced per-connection session subscriptions: after creation or load the gateway subscribes to `SessionStore::subscribe_session_events`, re-broadcasting status/log/tool/file events as `session/update` notifications over WebSocket with idle-aware flushing.
- In-memory session store now broadcasts lifecycle/log events (with initial queued status) so ACP clients receive immediate updates; log levels map to ACP log events.
- Session list filtering now infers `tenantId` **and** `projectId` from JWT claims when omitted by the client, aligning ACP behavior with REST scoping expectations.
- Added scenario fixture `tests/acp_bridge/scenarios/session_new_and_load.yaml` to drive the mock playback store and keep Scenario Format coverage aligned with the new RPCs.
- Session RPCs are gated on prior `initialize`, reusing the negotiated capabilities captured in `AcpSessionContext`.

#### Outstanding Tasks

- [x] Support loading paused sessions by mounting the existing workspace snapshot read-only and exposing its metadata back to the ACP client.
- [ ] Extend the Scenario Format with `userActions.pause_session` / `userActions.resume_session` primitives so harnesses can express pausing/resuming via REST or ACP semantics.
- [ ] Create an ACP↔REST session ID cross-reference table/migration instead of reusing raw session IDs.
- [x] Implement pagination (offset/limit) in `session/list` responses.
- [ ] Implement full project/tenant parity in `session/list` to mirror REST responses (claim inference completed; REST response shape parity still pending).

#### Verification

- [x] Integration test `cargo test -p ah-rest-server --test acp_sessions acp_session_catalog_end_to_end` dials the ACP gateway, runs `initialize → session/new → session/list → session/load`, and asserts both the response payloads and the streamed `session/update` notification include the created session.
- [x] Integration test `cargo test -p ah-rest-server --test acp_sessions acp_session_load_paused_marks_workspace_read_only` pauses a session, reloads it via ACP, and validates the response flags the workspace as read-only with snapshot metadata.
- [x] Integration test `cargo test -p ah-rest-server --test acp_session_list_pagination` checks `session/list` respects offset/limit and returns total counts.
- [ ] Scenario `tests/acp_bridge/scenarios/session_new_and_load.yaml` creates a session, pauses it, and reloads it through ACP; assertions check that the workspace mount path inside the scenario matches the TOT snapshot provider while the new pause/resume timeline events drive both REST and ACP clients appropriately.
- [ ] Integration test `cargo test -p ah-rest-server --test acp_session_catalog` uses the mock TaskManager backend to create sessions via REST and ensure ACP `session/list` mirrors them (including pagination).
- [ ] Database migration test ensures the cross-reference table enforces foreign keys and cleans up orphaned rows when sessions are deleted.

---

### Milestone 4: Prompt Turn Execution & Streaming Updates

**Status**: In progress (2025-11-26) — core prompt/cancel streaming landed

#### Deliverables

- [x] Implement `session/prompt` so ACP user messages are enqueued as Agent Harbor task instructions (leveraging `TaskManager::inject_message`). *Prompts now require a live TaskController and bubble errors instead of silently logging; 16k-char cap and history seeding remain.*
- [x] Stream Agent Harbor SSE events (`thought`, `tool_use`, `tool_result`, `file_edit`, `log`, `status`) back through ACP `session/update` notifications with correct JSON-RPC ids and `tool_execution_id` correlation. *Current stream forwards SessionStore events; tool correlation is stubbed until recorder bridge lands.*
- [x] Support `session/cancel` (notification) by invoking the REST cancellation path. *Updates session status, emits cancelled status, and best-effort calls `TaskController::stop_task` when available.*
- [x] Ensure prompts obey context window limits and respond with ACP-standard stop reasons.
- [x] Reject over-budget `session/new` prompts with `stopReason: context_limit` and no side effects on the session store.
- [x] Enforce legacy Scenario Format fixtures (`events` + `assertions`) in the mock playback store so ACP scenario tests emit expected status/log/thought events and fail fast when assertions are missing.
- [x] Add ACP `session/pause` / `session/resume` handlers that update session status and broadcast `session/update` events; verified with pause/resume scenario + RPC tests.
- [x] TaskController exposes pause/resume/inject stubs so downstream TaskManager wiring can adopt ACP control paths without API churn.
- [x] Database-backed TaskExecutor now updates session status for pause/resume and records status events, with unit tests covering the controller hooks.
- [x] JWT bearer claims propagate tenantId/projectId into `session/new` when absent from params; ACP test covers claim inference.
- [x] JWT-scoped `session/list` defaults tenant filter from claims and returns tenant/project metadata; covered by bearer + api_key hybrid auth test.

#### Verification

- [x] Scenario `tests/acp_bridge/scenarios/prompt_turn_basic.yaml` + integration test `cargo test -p ah-rest-server --test acp_prompt_scenario_streams_events` replay the timeline and assert running/log updates arrive.
- [x] `cargo test -p ah-rest-server --test acp_prompt_backpressure` simulates a slow ACP client and ensures the gateway applies bounded channels so the REST event bus never blocks.
- [x] Integration test `cargo test -p ah-rest-server --test acp_prompt acp_prompt_round_trip` sends `session/prompt` and asserts the gateway streams the user log back via `session/update`.
- [x] Integration test `cargo test -p ah-rest-server --test acp_cancel acp_session_cancel_streams_update` verifies `session/cancel` emits a cancelled status and acknowledges the request.
- [x] `cargo test -p ah-rest-server --test acp_prompt acp_prompt_rejects_on_context_limit` rejects over-budget prompts with `stopReason: context_limit` and avoids echoing them into session logs.
- [x] `cargo test -p ah-rest-server --test acp_sessions acp_session_new_respects_context_limit` rejects oversized initial prompts with `stopReason: context_limit` and suppresses `session/update` fanout.
- [x] Scenario `tests/acp_bridge/scenarios/initialize_and_auth.yaml` + integration test `cargo test -p ah-rest-server --test acp_initialize_and_auth_scenario_succeeds` validate the initialize/auth handshake and streamed status transitions using legacy `events`/`assertions`.
- [x] Scenario `tests/acp_bridge/scenarios/pause_resume.yaml` + integration tests `cargo test -p ah-rest-server --test acp_pause_resume acp_pause_resume_status_streams` and `cargo test -p ah-rest-server --test acp_pause_resume acp_pause_and_resume_rpcs_emit_status` validate paused → resumed → completed status streaming and the new pause/resume RPCs.
- [x] Unit test `cargo test -p ah-rest-server inject_message_forwards_to_recorder_socket` spins up the task-manager socket, simulates a recorder client, and asserts injected bytes reach the PTY channel.
- [x] Scenario playback assertions are exercised automatically in CI now that ACP fixtures include `assertions:` blocks (mock store evaluates them).

#### Implementation Details (current)

- Added ACP RPCs `session/prompt` and `session/cancel` inside `acp::transport`; they reuse `SessionService` storage, flip queued sessions to running, emit status/log events, and stream them back as `session/update` without blocking the socket.
- Event fanout now de-duplicates subscriptions per connection, seeds historical events on subscription (for fast scenario playback), and continues to flush broadcast events on idle ticks; lagged/closed channels are pruned defensively.
- `session/cancel` best-effort calls `TaskController::stop_task` when available to mirror REST cancellation.
- Backpressure coverage added via `acp_prompt_backpressure` which blasts prompts while delaying reads to ensure the gateway keeps streaming and does not deadlock.
- Scenario fixture `tests/acp_bridge/scenarios/prompt_turn_basic.yaml` added to mirror the prompt turn timeline; harness assertions now execute via the mock playback store’s legacy `events`/`assertions` support.
- Prompt injection now targets the live PTY via the task-manager socket: the socket protocol is a bidirectional SSZ envelope, `ah agent record` listens for `InjectInput` frames and writes them to the PTY, and the REST `TaskExecutor::inject_message` forwards ACP prompts over that channel (with newline termination) in addition to logging. The socket now also carries `PtyData`/`PtyResize` envelopes from the recorder to seed the follower/backlog channel for Milestone 5.3. Recorder-side PTY bytes are now buffered inside `TaskSocketHub` (backlog + broadcast) but no consumer is wired yet; follower hookup remains outstanding.
- Task executor now exposes PTY backlog/live subscription through `TaskController::subscribe_pty`, backed by `TaskSocketHub`; recorder PTY bytes and resizes flow into the hub and tests cover backlog + live delivery. ACP gateway now subscribes to the PTY stream per session, seeds backlog immediately, and emits `session/update` notifications with `terminal`/`terminal_resize` events (base64 payloads) so IDEs can begin rendering live output. IDE follower attachment/command channel remains TODO.
- Added `_ah/terminal/write` ACP extension to inject raw PTY bytes (base64) through the task manager socket without newline. `_ah/terminal/follow` returns the canonical follower command (`ah show-sandbox-execution ...`) so IDEs can spawn a follower terminal with the right execution/session ids. Full IDE follower lifecycle (attach/detach surfacing in UI) is still pending.
- `_ah/terminal/follow` now derives its follower command from recorded tool events when available (tool args/name matching `executionId`), only falling back to the client-supplied command when no history exists. This reduces spoofing risk while preserving compatibility with legacy clients that omit recorder metadata.
- Client-provided follow commands are now length-limited and stripped of newlines before any fallback is accepted, tightening the temporary compatibility path while recorder-derived commands remain the goal.
- Session event fanout now caches executionId→command mappings per connection as tool events stream in, so subsequent follower requests resolve from recorder history instead of trusting the client. This lays the groundwork to drop the fallback entirely once recorder metadata is plumbed.
- Added a notifier scaffold (SDK `AgentSideConnection` holder) so future session/update pushes can switch from the ad-hoc dispatcher to `notify`; dispatcher path remains active until typed session/update payload wiring is complete.
- Vendored SDK now publicly re-exports `MessageHandler`/`RpcConnection`, unblocking a proper notify-based gateway wiring without touching upstream crates.
- Notify migration blocker: `AgentSideConnection::new` spawns `LocalBoxFuture<'static, ()>` tasks that are not `Send`; on our multi-thread tokio runtime we need a `LocalSet` (or a Send-capable shim) to host the agent IO. Until that is in place, we continue to emit `session/update` via the dispatcher path.
- Added REST SSE endpoint `/api/v1/sessions/{id}/pty` streaming PTY backlog + live output (`event: pty`, base64 payloads and resizes) to mirror ACP terminal updates for non-ACP clients. `_ah/terminal/detach` ACP notification emits a `terminal_detach` event on the update stream. Scenario stub `tests/acp_bridge/scenarios/terminal_follow_detach.yaml` documents the happy path for follow/write/detach; UI wiring remains.
- Added REST SSE endpoint `/api/v1/sessions/{id}/pty` streaming PTY backlog + live output (`event: pty`, base64 payloads and resizes) to mirror ACP terminal updates for non-ACP clients. `_ah/terminal/detach` ACP notification emits a `terminal_detach` event on the update stream.

#### Outstanding Tasks

- [x] Wire `session/prompt` / `session/cancel` / pause/resume to guaranteed TaskManager delivery with execution-id correlation instead of best-effort logging/injection. *(Implemented via required TaskController + synthetic tool_use seeding when recorder metadata is missing.)*
- [ ] Derive follower commands and terminal streams directly from recorder metadata (execution stream) instead of falling back to client-supplied strings when history is missing. *(Client-supplied fallback removed; synthetic tool_use is a temporary bridge for mock sessions.)*
- [ ] Move session/prompt/cancel/pause/resume onto the SDK dispatcher path and send updates via `AgentSideConnection::notify`. *(Progress: WebSocket/stdio now use the SDK `ValueDispatcher` with raw params preserved; notifications still emitted manually.)*

#### Key Implementation Files

- `crates/ah-rest-server/src/acp/transport.rs` — prompt/cancel handlers, event flush tweaks.
- `crates/ah-rest-server/tests/acp_prompt.rs` — round-trip prompt coverage.
- `crates/ah-rest-server/tests/acp_prompt_backpressure.rs` — slow-consumer safety.
- `tests/acp_bridge/scenarios/prompt_turn_basic.yaml` — prompt timeline fixture (assertions pending).
- `crates/ah-rest-server/tests/acp_prompt_scenario.rs` — scenario-driven prompt flow validation.
- `crates/ah-rest-server/tests/acp_cancel.rs` — cancel RPC streaming check.

---

### Milestone 5: Command Execution Tracing & Passthrough Recorder

**Status**: In progress (2025-11-26)

#### Deliverables

- [ ] Finalize the new command-trace shim (Linux + macOS builds) that rewrites every agent-launched `exec/posix_spawn` to run under `ah agent record --passthrough --cmd "<command …>" --parent-recorder-socket <sock> --session-socket <sock>`. This includes hooking the shim into all third-party agent launches by default (`ah agent start` exports the correct `LD_PRELOAD`/`DYLD_INSERT_LIBRARIES` and socket descriptors).
- [ ] Ensure the passthrough recorder mirrors the parent TTY (size negotiation, raw byte forwarding) and streams timestamped input/output to the observer socket for `.ahr` persistence.
- [ ] Teach the shim to keep forwarding stdout/stderr from indirect child processes (existing interception path) so nested commands remain visible in the recording.
- [ ] Expose the session socket to followers: implement the `ah show-sandbox-execution` CLI integration plus an ACP hook that tells IDEs how to attach (including execution IDs, backlog replay, and live streaming).
- [ ] Harden the recorder to handle multiple simultaneous followers, late joins (send backlog), and input injection from followers routed through the `inject_message` bridge back to the running process.
- [ ] Add basic regression tests (`ah-command-trace-e2e-tests`) that prove the shim loads, rewrites commands, and fails open when env injection or sockets are unavailable.
- [ ] Automatically create filesystem snapshots after every tool execution and after every file write detected by the shim. For file writes, capture the diff against the previous snapshot for the affected file and emit it as an ACP `diff` content block (or via REST diff endpoints) so clients can show before/after views without re-reading the workspace.
- [ ] Track writes to agent session files (per agent type) and maintain a mapping between the snapshot taken for those writes and the session file update event. This mapping allows restoring session files to the correct state when a snapshot is restored or branched.
- [ ] Derive follower commands from recorded executions (not caller-supplied), replay PTY backlog via recorder sockets, and support multiple simultaneous followers.

#### Implementation Details (current)

- Added recorder bridge scaffold (`acp::recorder`) with a single source of truth for constructing follower commands (`ah show-sandbox-execution ... --id <exec> --session <session> --follow`), matching the design described in this milestone.
- No runtime wiring yet; this helper will be consumed by the upcoming recorder-to-ACP bridge and IDE follower channel.
- Scenario playback now honors a `linger_after_timeline_secs` option (exposed in `MockServerDependencies` and the `ah-rest-server-mock` CLI via `--scenario-linger-secs`) to keep connections open after scripted timelines, preventing premature teardown while follower terminals drain trailing PTY bytes.
- Added an ACP scenario-driven follower test (`acp_prompt_followers`) that replays `tests/acp_bridge/scenarios/terminal_follow_detach.yaml` against the live gateway, asserting both `_ah/terminal/follow` and `_ah/terminal/detach` updates surface as `session/update` notifications. The fixture now uses placeholders so session IDs returned by the server are applied at runtime; the test harness consumes the scenario timeline to drive the WebSocket while letting the server execute normally.
- ACP gateway now streams PTY backlog + live data/resizes from the TaskManager socket to ACP clients after `_ah/terminal/follow`, using the recorder task socket hub (`TaskManagerMessage::PtyData`/`PtyResize`). This wires prompt-injection-followers to the live TTY instead of log-only.
- Recorder socket session events now hydrate the session store: `TaskManagerMessage::SessionEvent` (and legacy SSZ `SessionEvent`) are persisted and fanned out to SSE/ACP subscribers, completing the recorder→ACP bridge for status/log/tool/file events.
- New reusable ACP Scenario driver (`tests/common/acp_scenario.rs`) executes YAML client/server timelines with placeholder substitution; `acp_prompt_followers` now uses it, reducing bespoke harness code. This sets the stage for running ACP scenarios directly via the Scenario Format rather than per-test wiring.

#### Key Implementation Files

- `crates/ah-rest-server/src/acp/recorder.rs` — follower command builder utility and unit test.

##### Sub-milestones

1. **5.1 Shim injection & fail-open** — Cross-platform build, env propagation, safety tests.
2. **5.2 Passthrough recorder core** — PTY mirroring, parent/session sockets, observer mock.
3. **5.3 Follower channel** — Session socket plumbing to `ah show-sandbox-execution`, ACP hooks, multi-follower support.
4. **5.4 Auto snapshot & diff emission** — Snapshot-after-tool/write automation plus SSE/ACP diff events (with truncation).
5. **5.5 Session-file mapping** — Detect session-file writes, store metadata, integrate with time-travel.

#### Verification

- [x] Integration test `cargo test -p ah-rest-server --test acp_prompt_followers` drives `_ah/terminal/follow/write/detach` over ACP and validates the streamed `terminal_follow`/`terminal_detach` updates plus write acknowledgements.
- [ ] Scenario `tests/acp_bridge/scenarios/passthrough_recorder.yaml` launches a tool via the shim, verifies the rewritten command, and asserts that SSE/`.ahr` output matches the original PTY stream.
- [ ] Unit tests cover shim rewrite logic on Linux/macOS, socket negotiation, environment injection from `ah agent start`, and failure modes (recorder unavailable → fail open).
- [ ] Integration test `cargo test -p ah-command-trace-e2e-tests passthrough_follow_mode` spawns the shim, records a command, attaches a follower, injects input (e.g., sudo prompt), and checks that both the running process and `.ahr` capture the interaction faithfully.
- [ ] Scenario `tests/acp_bridge/scenarios/auto_snapshot_diff.yaml` runs multiple file edits, confirms that each write triggers a snapshot + diff, and verifies the diff is delivered to ACP clients (`type: "diff"` content) and REST diff endpoints.
- [ ] Scenario-driven integration test (using the LLM API Proxy + Scenario Format harness) launches a real third-party agent, performs writes to agent session files, and validates that the snapshot/session-file mapping stays consistent when branches/time-travel are exercised. This ensures the feature works end-to-end with an actual agent stack instead of a synthetic shim-only test.

---

### Milestone 6: `ah show-sandbox-execution` Integration Tests

**Status**: Planned

#### Deliverables

- [ ] Build deterministic integration fixtures for `ah show-sandbox-execution` covering: backlog replay, live streaming, concurrent followers, and input injection (including prompts that require user input such as sudo passwords).
- [ ] Add Scenario Format coverage (`tests/acp_bridge/scenarios/show_sandbox_execution.yaml`) that drives a tool command, attaches a follower mid-flight, injects keystrokes, and verifies recorder + ACP output stay consistent.
- [ ] Create CLI-level tests under `tests/tools/show-sandbox-execution/` that run the follower program against a mocked session socket; verify tty resizing, ctrl-c handling, and graceful shutdown when the recorder exits first.
- [ ] Ensure SessionViewer shortcut/modal integration is exercised via an automated UI test (e.g., Ratatui harness) so status-bar indicators map to follower sessions correctly.

##### Sub-milestones

1. **6.1 CLI harness** — Finalize follower CLI behavior, add mocked-socket tests.
2. **6.2 Scenario-driven pipelines** — Use Scenario Format to cover attach/detach, prompts, error handling.
3. **6.3 SessionViewer UX** — Implement modal UI, keyboard shortcuts, snapshot-aware auto-follow suppression.

#### Verification

- [ ] `cargo test -p ah-agent-record --test show_sandbox_execution_backlog` asserts that the follower receives historical bytes plus live updates and terminates cleanly when the PTY closes.
- [ ] `cargo test -p ah-agent-record --test show_sandbox_execution_input` proves injected keystrokes routed through the follower reach the underlying PTY (recorded in `.ahr`) and unblock a scripted sudo prompt.
- [ ] Scenario `tests/acp_bridge/scenarios/show_sandbox_execution.yaml` runs end-to-end (Harbor agent → passthrough recorder → ACP follower) and diff-checks the IDE transcript vs. the `.ahr` output.

---

### Milestone 7: Recorder Bridge, Follower Playback & Workspace Guardrails

**Status**: Planned

#### Deliverables

- [ ] Wire `ah agent record` (see `specs/Public/ah-agent-record.md`) into the ACP gateway so recorder SSE/IPC events are translated into ACP `session/update` messages in real time (tool start, output chunks, completion, snapshot markers).
- [ ] Extend the Command-Execution-Tracing pipeline so intercepted tool launches feed structured metadata into the recorder (cmd/args/env/cwd) and are tagged for downstream playback.
- [ ] Implement an “IDE follower” command channel: Harbor emits ACP instructions that tell the IDE to run `ah show-sandbox-execution "<cmd>" --id <execution_id>` (wrapping `ah agent replay --session <id> --tool <tool_id> --follow`) so the IDE can attach to the `.ahr` stream and display byte-perfect terminal state without spawning its own sandbox.
- [ ] Update SessionViewer to surface the same follower terminals locally: show active executions in the status bar, add a shortcut that opens the follower TTY in a modal, and send keystrokes back through the recorder’s `inject_message` TTY so users can unblock prompts (e.g., sudo passwords) directly from the UI.
- [ ] Ensure recorder outputs always reference sandbox-relative paths and reject any attempt by upstream agents to exec outside the session workspace (guarded via tracer policies).
- [ ] Provide per-session/tenant policies to disable follower playback export or to redact sensitive commands before they’re replayed to IDEs.
- [ ] Stream “last lines” for each ongoing tool execution by piping passthrough-recorder output from the parent recorder to the local task manager over the task-manager socket, then fanning it out to the dashboard UI plus SSE/ACP subscribers. Implement throttling/line-wrapping in the task manager so ACP clients see concise yet timely updates (mirrors `last_lines` panel in the TUI).

##### Sub-milestones

1. **7.1 Recorder → ACP bridge** — Translate SSE/IPC events into ACP `session/update` payloads with execution IDs.
2. **7.2 IDE follower channel** — Emit ACP instructions + policies for follower launch, ensure resilience when IDE disconnects/reconnects.
3. **7.3 SessionViewer UX & policies** — Local modal hookup plus redaction/governance toggles.
4. **7.4 Last-lines streaming** — Task-manager socket plumbing to dashboards + ACP SSE, including throttling tests.

#### Verification

- [ ] Scenario `tests/acp_bridge/scenarios/terminal_only.yaml` records a deterministic tool run, streams live updates to the IDE, issues the `ah show-sandbox-execution` command, and asserts the IDE output matches the `.ahr` playback while the SessionViewer modal reflects the same TTY.
- [ ] Unit tests cover tracer policy enforcement (workspace confinement), recorder-to-ACP translation, replay command construction, and SessionViewer shortcut/modal + input-injection plumbing.
- [ ] Integration test `cargo test -p ah-rest-server --test acp_recorder_follow_mode` launches a mock agent, records an `.ahr`, triggers IDE replay via ACP, and compares the IDE’s rendered output to the original PTY stream (including injected keystrokes routed through `inject_message`).
- [ ] Integration test `cargo test -p ah-task-manager --test last_lines_stream` feeds synthetic passthrough output into the parent recorder socket and asserts the local task manager publishes capped “last line” summaries over both the dashboard channel and the REST SSE endpoint; a companion ACP test validates the same data arrives via `session/update`.

---

### Milestone 8: Pipeline Introspection APIs & UI

**Status**: Planned

#### Deliverables

- [ ] Persist pipeline metadata (pipeline id, steps, per-stream byte counts, timestamps) alongside each execution in the recorder and expose it via new REST endpoints (`GET /api/v1/sessions/{id}/executions/{executionId}/pipelines[...]`).
- [ ] Implement ACP extensions `_ah/tool_pipeline_list` and `_ah/tool_pipeline_stream`, including streaming support for individual steps (stdout/stderr) with backlog + follow semantics.
- [ ] Extend SessionViewer follower modal with the pipeline sidebar/menu (as described in `ah-agent-record.md`) and provide the same data to ACP clients so IDEs can render matching UIs.
- [ ] Teach `ah show-sandbox-execution` to request pipeline metadata and expose a CLI switch (`--step <stepId> --stream stdout`) so terminal users can focus on a single pipeline stage.
- [ ] Add RBAC/policy enforcement so tenants can disable pipeline introspection or redact specific commands/streams before exposing them over REST/ACP.

##### Sub-milestones

1. **8.1 Recorder metadata capture** — Persist pipeline/step details plus byte ranges.
2. **8.2 REST surface** — Expose `/pipelines` endpoints with pagination and RBAC.
3. **8.3 ACP extensions** — Implement `_ah/tool_pipeline_*` methods with backlog + follow support.
4. **8.4 UI/CLI integrations** — SessionViewer sidebar + `ah show-sandbox-execution --step`.

#### Verification

- [ ] Scenario `tests/acp_bridge/scenarios/pipeline_explorer.yaml` records a multi-step pipeline, exercises the REST + ACP endpoints, and asserts that IDE telemetry matches the `.ahr` metadata (including per-step byte counts).
- [ ] `cargo test -p ah-agent-record --test pipeline_metadata_roundtrip` ensures recorder-generated pipeline records survive replay and match the REST responses.
- [ ] UI test `tests/ui/session_viewer_pipeline.rs` drives the SessionViewer shortcut/modal, navigates between steps, and confirms streamed bytes match the selected pipeline step.
- [ ] CLI test `tests/tools/show-sandbox-execution/pipeline.rs` verifies that `ah show-sandbox-execution --step ...` streams only the requested step and handles follow/unfollow correctly.

---

### Milestone 9: Workspace Info & File Diff Extensions (REST + ACP)

**Status**: Planned

#### Deliverables

- [ ] Build a shared workspace-inspection layer (library in `ah-rest-server`) that powers both REST endpoints (`GET /api/v1/sessions/{id}/workspace/info`, `/workspace/files`, `/files/{path}`, `/diff`, etc.) and the new ACP extensions (`_ah/workspace/info`, `_ah/workspace/list`, `_ah/workspace/file`, `_ah/workspace/diff`). The implementation must live in one place so feature parity and RBAC policies remain consistent.
- [ ] Implement the ACP methods from `specs/ACP.extensions.md` so IDEs can browse Harbor’s workspace, fetch metadata/content, and request diffs without relying on `fs/read_text_file`.
- [ ] Ensure the SSE diff events and ACP diff payloads reuse the same diff computation layer, including the truncation rules (64 KiB / 2,000-line thresholds) and `fullDiffUrl` hints described in the REST spec.
- [ ] Add policy/RBAC knobs to restrict workspace browsing (e.g., blacklist directories, disable diff streaming) and verify they apply equally to REST and ACP.
- [ ] Advertise `_meta.agent.harbor` capability blocks during `initialize`, documenting which `_ah/*` methods are available (workspace, snapshots, pipelines) and ensuring capability negotiation gates each extension.

##### Sub-milestones

1. **9.1 Shared workspace library** — Build reusable service exposing list/info/diff APIs with caching.
2. **9.2 REST parity** — Refactor REST endpoints to use the shared layer; add regression tests.
3. **9.3 ACP workspace extensions** — Implement `_ah/workspace/*` methods with pagination + filtering.
4. **9.4 Policy enforcement** — Centralize RBAC toggles (directory allowlists, diff redaction) and ensure both transports honor them.

#### Verification

- [ ] Scenario `tests/acp_bridge/scenarios/workspace_browse.yaml` exercises both REST and ACP workspace endpoints side-by-side and asserts identical results (order, pagination, metadata).
- [ ] Unit tests cover the shared workspace layer (filtering, pagination, diff truncation) and RBAC enforcement.
- [ ] Integration test `cargo test -p ah-rest-server --test workspace_extensions_rest_vs_acp` invokes the REST endpoints and ACP methods in the same run to ensure parity (including error cases).

---

### Milestone 10: Plans, Modes, Permissions, and Slash Commands

**Status**: Planned

#### Deliverables

- [ ] Implement ACP plan support: translate Agent Harbor supervisor summaries into `agent_plan/update` notifications and accept plan acknowledgements.
- [ ] Support `session/set_mode` to switch between plan mode, code mode, and eval mode, mapping them to Agent Harbor runtime profiles.
- [ ] Surface slash commands exposed by Agent Harbor (run tests, open IDE, branch session) via `session/update` command catalog updates.
- [ ] Implement `session/request_permission` round-trips so that potentially destructive tool calls (e.g., `fs.write_text_file`) can request user approval based on tenant policies.

##### Sub-milestones

1. **10.1 Plan plumbing** — Map Harbor supervisor plans to ACP `agent_plan/update`, store acknowledgements.
2. **10.2 Mode transitions** — Bridge `session/set_mode` to Harbor runtime profiles with validation tests.
3. **10.3 Slash command catalog** — Publish actionable commands over ACP and REST with metadata.
4. **10.4 Permission workflow** — Implement policy-driven prompts, integrate with Scenario Format pause/resume controls.

#### Verification

- [ ] Scenario `tests/acp_bridge/scenarios/plan_and_permissions.yaml` models a flow where the agent publishes a plan, switches modes, and requests permission for a filesystem write; harness assertions verify each stage.
- [ ] Unit test `cargo test -p ah-rest-server acp_mode_translation` ensures internal mode enums stay in sync with ACP definitions.
- [ ] Automation test using the SDK sample client triggers slash commands and validates they appear with correct metadata in the ACP stream.

---

### Milestone 11: Resilience, Metrics, and Multi-Client Concurrency

**Status**: Planned

#### Deliverables

- [ ] Add per-connection metrics (latency histograms, active sessions, dropped messages) exported through the existing Prometheus stack.
- [ ] Implement automatic reconnection support so ACP clients can resume sessions after transient network failures (leveraging SDK session tokens).
- [ ] Harden error handling: classify fatal vs. recoverable errors, ensure JSON-RPC errors adhere to ACP codes, and add structured audit logs.
- [ ] Load-test harness that spins up multiple Scenario Format clients concurrently to validate concurrency and tenant isolation.

##### Sub-milestones

1. **11.1 Metrics instrumentation** — Add Prometheus gauges/counters + regression tests.
2. **11.2 Reconnection protocol** — Implement resume tokens, backlog replay, and SDK validation.
3. **11.3 Error taxonomy** — Standardize JSON-RPC errors, audit logs, and fatal vs recoverable handling.
4. **11.4 Load/chaos harness** — Expand Scenario Format runners to orchestrate multi-client stress + fault injection.

#### Verification

- [ ] Benchmark test `cargo test -p ah-rest-server --test acp_load_balancer` spawns 50 simulated ACP clients, each replaying `tests/acp_bridge/scenarios/prompt_turn_basic.yaml`, and asserts no dropped events.
- [ ] Metrics snapshot test scrapes the `/metrics` endpoint after a scenario run and checks for the new gauges/counters with non-zero values.
- [ ] Chaos test (feature-gated) randomly terminates ACP connections mid-turn and ensures reconnection logic replays missed events without duplications (validated via scenario assertions).

---

### Milestone 12: Optional In-Place Filesystem Passthrough

---

**Status**: Planned

#### Deliverables

- [ ] When a session explicitly selects `workingCopy = in-place`, advertise ACP filesystem capabilities and implement `fs/read_text_file` / `fs/write_text_file` by proxying to the editor-owned workspace via the REST task file APIs.
- [ ] Reuse `ah-fs-snapshots` provider metadata to verify absolute paths belong to the opted-in working copy; refuse access to other directories.
- [ ] Provide tenant-level policy controls so administrators must opt into exposing local filesystem state before the capability bit is set.
- [ ] Document the operational caveats (no AgentFS isolation, relies on client to persist edits) inside `specs/Public/Configuration.status.md`.
- [ ] Extend the command-execution shim so that, when the ACP client advertises `fs.readTextFile` / `fs.writeTextFile`, agent-side file reads/writes are intercepted and serviced by remote `fs/*` calls to the client (with caching/fallback when the client is offline). This keeps the Harbor sandbox in lockstep with the editor’s workspace even in in-place mode.

##### Sub-milestones

1. **12.1 Capability negotiation** — Detect client `fs.*` support and only enable in-place mode when both sides agree.
2. **12.2 REST proxying** — Reuse task file APIs to service `fs/*` calls, including diff emission and policy checks.
3. **12.3 Shim redirection** — Intercept agent syscalls and forward to client `fs/*` methods with caching/fallback.
4. **12.4 Policy & docs** — Document operational caveats and enforce tenant opt-in.

#### Verification

- [ ] Scenario `tests/acp_bridge/scenarios/in_place_fs.yaml` runs a session with in-place mode enabled, exercises read/write operations, and confirms the editor receives the expected diffs.
- [ ] Integration test `cargo test -p ah-rest-server --test acp_fs_passthrough` uses a temporary on-disk workspace mounted via the client harness to ensure edits round-trip correctly.
- [ ] Policy test ensures sessions without the opt-in continue to advertise `fs.* = false` even if workloads request it.
- [ ] Shim-level integration test `cargo test -p ah-command-trace-e2e-tests fs_redirect` verifies that `open`/`read`/`write` syscalls inside the third-party agent result in mocked `fs/*` JSON-RPC calls when the capability is enabled, and fall back to local disk when disabled.

---

### Milestone 13: Multimodal Input Staging & Third-Party Agent Support

**Status**: Planned

#### Deliverables

- [ ] Implement a media staging subsystem inside each session workspace (`/workspace/.ah-media/<uuid>`) that stores ACP/REST-uploaded attachments (images, audio, arbitrary binaries) with metadata (mime type, size, timestamps). Ensure staging respects tenant policies and integrates with time-travel snapshots.
- [ ] Extend the ACP gateway & REST API to ingest multimodal content (per `resources/acp-specs/docs/protocol/content.mdx`), persist it in the staging area, and return `harborMediaId` references so downstream components can rehydrate attachments.
- [ ] Update third-party agent launchers (documented in `specs/Public/3rd-Party-Agents/`) to consume staged media: e.g., pass file paths via CLI flags, provide HTTP upload helpers, or call agent-specific APIs as needed.
- [ ] Ensure the recorder/command-tracing stack captures media references so recorded sessions and snapshots include the context required to replay multimodal inputs.
- [ ] Integrate the flow into the LLM API Proxy (Scenario Format) so we can drive real third-party agents with media inputs and verify end-to-end behavior.

##### Sub-milestones

1. **13.1 Media staging service** — Define on-disk layout, metadata, retention policies.
2. **13.2 REST + ACP ingestion** — Extend APIs to upload/stream attachments, return `harborMediaId`.
3. **13.3 Agent adapters** — Update each third-party agent wrapper with media ingestion (e.g., CLI flags, HTTP uploads).
4. **13.4 Recorder + snapshot integration** — Persist media references in `.ahr`, ensure snapshots/time-travel include assets.
5. **13.5 Scenario coverage** — Add Scenario Format fixtures for multimodal inputs driven via the LLM API Proxy.

#### Verification

- [ ] Scenario `tests/acp_bridge/scenarios/multimodal_input.yaml` drives an ACP client that uploads images/audio, ensures the media is staged, and verifies the third-party agent receives and consumes the files.
- [ ] Agent-specific integration tests (one per supported agent) cover media handling (e.g., Claude Code image input, OpenHands audio) and confirm outputs are recorded/replayed correctly.
- [ ] Snapshot restoration test ensures media files referenced by snapshots are restored alongside session files, preserving agent state when branching/time-traveling.

### Milestone 14: SessionViewer UI Enhancements & Regression Suite

**Status**: Planned

#### Deliverables

- [ ] Ship the SessionViewer follower tray/modal: session status-bar indicators, keyboard shortcuts (open/close, switch follower), and embedded PTY panes wired to recorder streams.
- [ ] Add the pipeline explorer sidebar plus workspace/diff panes so users can inspect `_ah/tool_pipeline_*` and `_ah/workspace/*` data without leaving the UI.
- [ ] Integrate time-travel controls (snapshot list, branch selector, “jump to follower”) that leverage recorder anchors, ensuring UI actions mirror REST/ACP time-travel APIs.
- [ ] Surface “last lines” summaries, media attachment previews, and multimodal context hints inside the SessionViewer to align with Milestones 5, 8, 9, and 13.
- [ ] Build a regression harness (Ratatui/TUI) that replays Scenario Format fixtures, captures golden screenshots/logs for key UI states, and runs in CI to prevent regressions.

##### Sub-milestones

1. **14.1 Follower tray & shortcuts** — Implement status bar entries, modal navigation, keystroke passthrough; add golden screenshot tests.
2. **14.2 Pipeline & workspace panes** — Render pipeline steps, diffs, and workspace metadata using the shared workspace layer; include interactive focus/follow controls.
3. **14.3 Timeline & branch UX** — Display snapshots/branches, enable jump-to-snapshot/follower actions, and sync with `_ah/session_seek`.
4. **14.4 Regression harness** — Extend the TUI screenshot harness + Scenario Format playback to exercise the new UI states and enforce thresholds on layout changes.

#### Verification

- [ ] UI test `tests/ui/session_viewer_followers.rs` verifies shortcuts open the follower modal, multiple executions render correctly, and input injection matches `.ahr` output.
- [ ] UI test `tests/ui/session_viewer_pipeline.rs` drives the pipeline sidebar, ensures streamed bytes align with the selected step, and confirms truncation hints render.
- [ ] UI test `tests/ui/session_viewer_workspace.rs` exercises workspace/diff panes, snapshot jump controls, and last-line panels using mocked `_ah/workspace/*` data.
- [ ] Scenario `tests/acp_bridge/scenarios/session_viewer_ui.yaml` replays a full session (followers + pipelines + multimodal inputs) and diff-checks golden screenshots/logs.

## Outstanding Tasks After Milestones

- Define a compatibility matrix for third-party ACP clients (VS Code, Cursor, Zed) once the server reaches beta.
- Extend Scenario fixtures with negative-path coverage (malformed JSON-RPC, outdated schema versions).
- Determine whether to expose ACP over QUIC once the spec finalizes HTTP streaming transport.
- Promote Scenario Format support for ACP RPC timelines (client/server frames) so fixtures like `terminal_follow_detach.yaml` can be executed directly without bespoke harness code; currently the ACP follow/detach scenario is driven via a bespoke test harness rather than the scenario store.

Once all milestones are implemented and verified, update this status document with:

1. Implementation details and source file references per milestone (mirroring other status files).
2. Checklist updates (`[x]`) and remaining outstanding tasks.

## Hand-off Notes (Dec 2025)

- SDK is vendored as a submodule at `vendor/acp-rust-sdk` (remote `git@github.com:blocksense-network/acp-rust-sdk.git`, branch `agent-harbor`). We patched it to expose `AgentSideConnection::notify`, added a `RpcDispatcher` that operates on `serde_json::Value`, and added `StreamBroadcast::outgoing_json`. All SDK unit tests still pass (`cargo test -p agent-client-protocol`).
- The ACP gateway now runs through the SDK dispatcher for **both WebSocket and stdio transports** (`acp/transport.rs`), so frames flow `JSON → ValueDispatcher → RpcDispatcher` with responses/notifications serialized via `outgoing_to_value`. Stdio framing reuses the same dispatcher on stdin/stdout.
- Still needed: an `Agent` trait implementation that maps SDK request types to the existing session/prompt/cancel/pause/resume logic and emits updates via `notify` instead of the ad‑hoc router helpers.
- `session/list` pagination (offset/limit) is implemented; project/tenant parity and ACP↔REST session ID cross-ref remain outstanding. Paused-session load semantics (read-only mounts) are also pending.
- Follower safety: `_ah/terminal/follow` now prefers recorder/tool event history for follower commands and only falls back to client-supplied strings when no execution history exists; long term we still need recorder-derived commands to be the single source of truth (Milestones 5–7).
- Warn lints: the vendored SDK still emits benign warnings (shadowed `Result`, unused dispatcher/broadcast helpers); clean these up or allow in CI.
