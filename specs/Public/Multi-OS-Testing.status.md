# Multi-OS Testing — Implementation Plan and Status

This status file tracks implementation progress for the Multi‑OS Testing and run‑everywhere workflow described in `Multi-OS-Testing.md`. Milestones are ordered to front-load connectivity and simulation harnesses before filesystem and synchronization layers. Each milestone lists deliverables and automated verification criteria. When milestones complete, add Implementation Details, Key Source Files, and Outstanding Tasks subsections with checkboxes in Verification updated to `[x]`.

## M0 — Domain Types Baseline

- **Deliverables**
  - `ah-domain-types` crate defining fleet/host/tag selectors, transport method enums (direct/connect/relay), session/fence metadata, fileset descriptors, and run-everywhere result envelopes; serde + validation helpers.
  - JSON schema (if applicable) generated from the types for CLI/REST configuration validation.
- **Verification**
  - Unit tests: serde round-trip for all types; selector validation (host vs tag precedence); transport method enum accepts/ rejects unknowns.
  - Schema tests: generate schema, load sample config fixtures, fail on invalid fixtures (missing tag, unknown method).

## M1 — Thin Driver Programs (mock-capable)

- **Deliverables**
  - New crates: `ah-followerd` (lib + bin), `ah-access-pointd` (lib + bin), `ah-fleetctl` (bin that depends on crates below); all lib-first under `crates/` with thin launchers in `bin/`.
  - These drivers wrap connectivity and scheduler crates to expose enroll, fleet info, and exec RPCs; runtime flag to switch mock exec vs no-op.
  - Logging CLI conforms to `Logging-Guidelines.md` using `ah_logging::CliLoggingArgs`; defaults per binary type and platform-standard log locations.
  - Config parsing uses `config-core` + `ah-config-types`, no Clap defaults that shadow config precedence (see `Configuration.md`); supports `--config`, env `AH_*`, and repo scopes for any new flags.
  - Structured JSON lines logging for harness consumption; stable CLI contracts.
- **Verification**
  - Golden-output tests: CLIs emit expected JSON envelopes for enroll, fleet list, exec request/response (mock mode).
  - Contract tests: malformed args produce exit code 2 with error on stderr; non-interactive mode respected.
  - Logging tests: `--log-level/--log-file/--log-dir/--log-format` behave per Logging-Guidelines; TUI/non-TUI defaults honored; `RUST_LOG` override honored.
  - Config precedence test: CLI flag overrides env overrides repo-user/repo/user/system; no Clap default leaks (checked via fixture configs).

## M2 — Connectivity Core (Control + Tunnel, mock exec)

- **Deliverables**
  - Crate `ah-connectivity` providing QUIC control-plane + HTTP CONNECT bridge; path-selection state machine (direct → connect → relay) with RTT capture and events (`connect.path.fixed/degraded/restored`).
  - Crate `ah-ssh-bridge` (feature-gated) defining pluggable SSH channel interface (mock stub for now) with liveness probes; used by drivers and scheduler.
- **Verification**
  - Integration test (in-process): start mock access point and two executors; assert path fixation order, RTT recorded, and events sequence.
  - Degradation test: inject disconnect → expect reroute attempt and `connect.path.degraded` then recovery.
  - Timeout test: connect timeout returns defined error code and event payload.

## M3 — Simulation Harness (Local Processes)

- **Deliverables**
  - Crate `ah-harness-local` providing test utilities to spawn access point + followers as local processes running M1 drivers; scripted scenarios for enroll → fleet formation → fan-out mock exec.
  - Helpers to assert event timelines and aggregated run-everywhere outputs.
- **Verification**
  - Scenario test: three followers (linux tags) return distinct mock results; aggregate exit non-zero if any fails.
  - Selector test: `--host` and `--tag` filters select correct subset; empty selection returns defined error.
  - Concurrency test: multiple execs in flight reuse control connection (mock SSH) without race (no lost events).

## M4 — Simulation Harness (Containers: incus/docker)

- **Deliverables**
  - Crate `ah-harness-containers` extending harness to launch followers in incus/docker; network bridge configuration; host-key persistence between runs.
  - Path mapping configuration seeds for later sync work.
- **Verification**
  - Container scenario: enroll all followers, execute mock commands, collect outputs; asserts CONNECT path used.
  - Network failure test: stop one container mid-run → expect per-host failure, aggregate non-zero, remaining hosts succeed.
  - Restart test: container restart triggers re-enroll and new path fixation within timeout.

## M5 — Simulation Harness (Cross-Platform VMs)

- **Deliverables**
  - Crate `ah-harness-vm` with profiles for macOS/Windows/Lima followers, host-key and path mapping configs; hybrid client-relay exercised.
  - Smoke driver to verify Proxy-Authorization header propagation.
- **Verification**
  - Cross-OS scenario: one macOS, one Windows, one Linux follower → all return mock exec outputs; relay path recorded where direct not allowed.
  - Host-key mismatch test: wrong key rejected with explicit error and telemetry.
  - Token expiry test: expired JWT causes CONNECT denial; harness asserts error surface and no executor side effects.

## M6 — Mutagen Project Skeleton (mock flush)

- **Deliverables**
  - Crate `ah-mutagen-project` generating per-session `mutagen.yml` with sync entries per follower and ignores; start/flush/terminate mocked via harness hook emitting fence events.
  - Integration with drivers to call mock flush before exec.
- **Verification**
  - Unit tests: generated YAML matches snapshots for sample fleets (linux-only, mixed OS); ignores list honored.
  - Harness test: fence event emitted and observed before exec; fence timeout produces defined error path.
  - Config override test: repo-specific ignores override defaults.

## M7 — run-everywhere Scheduler (mock exec)

- **Deliverables**
  - Crate `ah-run-everywhere` that shards multiple `--command` entries across followers (per-OS once, per-host concurrency=1); aggregates stdout/stderr/log handles and exit codes; plugs into adapters.
  - Adapter selection hooks (per-OS) stubbed but invoked.
- **Verification**
  - Sharding test: commands distributed round-robin across OS sets; each command runs on exactly one follower per OS.
  - Concurrency guard test: attempts to oversubscribe a follower are queued; event timeline shows serialized execution.
  - Partial failure test: one host returns non-zero → aggregate non-zero; per-host results preserved.

## M8 — Real SSH + Mutagen Sync + FsSnapshot Fence

- **Deliverables**
  - Upgrade `ah-ssh-bridge` to real ControlMaster SSH; extend `ah-mutagen-project` to run real `mutagen project start/flush`; integrate with FsSnapshot provider selection.
  - Selector options preserved across crates and drivers.
  - Cleanup hooks for Mutagen terminate and snapshot cleanup tokens.
- **Verification**
  - End-to-end test (Linux-only): edit file, run fence+exec, followers receive updated file via Mutagen, command sees change, aggregate success.
  - Cross-OS smoke: macOS/Windows followers with path mapping run a real no-op command via SSH; fence completes before exec.
  - Failure injections: fence timeout surfaces actionable error; sync divergence induces rescan; cleanup idempotency after forced process kill.

## M9 — Observability & REST Surfacing

- **Deliverables**
  - REST endpoints `/api/v1/sessions/{id}/info` and SSE stream relaying `fence*`, `host*`, `connect.path.*` events; per-host log/artifact metadata for run-everywhere.
  - Server built on Axum/Tokio with rustls TLS, tower-http middleware (CORS/trace/compression), jsonwebtoken auth where enabled; OpenAPI surfaced via utoipa + Swagger UI.
  - CLI/TUI wiring to display fence status and per-host summaries (read-only for this milestone).
- **Verification**
  - API contract tests: JSON schema validation of responses with sample fleets; SSE ordering guaranteed (monotonic timestamps).
  - TLS/auth test: rustls-served endpoint accepts valid JWT, rejects expired/invalid; requires rustls feature (no OpenSSL).
  - Harness test: running a scenario produces expected REST info payload; SSE stream includes path fixation and fence events in order; reqwest client (rustls-tls) consumes SSE.
  - Mocking test: wiremock-based regression catches breaking changes to response shape and required headers.
  - Backpressure test: SSE client disconnect/reconnect resumes without losing subsequent events.

## M10 — Status Maintenance

- **Deliverables**
  - This file kept in sync: completed milestones annotated with Implementation Details, Key Source Files, Outstanding Tasks, and `[x]` verification checkboxes.
  - CI check to ensure `lint-specs` passes for updates to this file.
- **Verification**
  - `just lint-specs` includes this file and passes.
  - Review hook: PR template requires verification checkboxes per milestone touched.
