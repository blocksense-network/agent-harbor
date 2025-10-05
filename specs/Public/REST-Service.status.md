# Agent Harbor REST Service — Status and Plan

Spec: See [REST-Service/API.md](REST-Service/API.md) for the API behavior. This file tracks the implementation plan, success criteria, and an automated test strategy per specs/AGENTS.md.

## Goal

Deliver a reliable REST + SSE service that the CLI, TUI, and WebUI can use to create and manage agent sessions. Optimize for testability by sharing a transport crate between the mock and real server and by validating all flows through black‑box HTTP tests and CLI‑level E2E tests.

This plan is expanded to explicitly cover multi‑OS execution and connectivity constraints per Multi‑OS Testing specs. The near‑term priority is to validate the communication topology on a single Linux host using Incus or Docker containers to simulate multiple machines. Subsequent phases verify true cross‑OS synchronization (Linux/macOS/Windows) using QUIC control plane and SSH over HTTP CONNECT (with optional client‑side relay in hybrid fleets).

## Milestones and Tasks

1. Shared transport crate

- Crate `ah-rest-api-contract` defining request/response types, error schema, SSE event enums, idempotency headers, and OpenAPI derives.
- JSON compatibility tests and golden samples; ensure OpenAPI can be generated without running the full server.

2. Mock server (deterministic)

- Crate `ah-rest-mock` using `axum` with in‑memory stores and seedable RNG.
- Endpoints: `/healthz`, `/readyz`, `/version`, `/api/v1/openapi.json`.
- Minimal tasks/sessions implementation with synthetic SSE events.

3. CLI first loop

- Point `ah` CLI to the mock via `AH_REST_BASE`/`--rest-base`.
- Implement `ah task`, `ah session list|get|logs|events`, and IDE open helpers against transport types.

4. Black‑box transport tests

- Start `ah-rest-mock` in‑process during tests; exercise HTTP and SSE flows using the shared types.
- Scenarios: create task (idempotent), poll session, read logs, stream events, stop/cancel, and workspace summary.

5. Capability discovery

- `GET /api/v1/agents`, `/runtimes`, `/executors`; optional `/git/refs` cache.
- Tests assert schemas and stable field names for CLI completions.

6. Event ingestion for multi‑OS (mock)

- Implement followers listing and leader‑originated event ingestion (`followersCatalog`, `fence*`, `host*`, `summary`).
- Tests validate per‑host log/event framing and summary events via session SSE.

7. Connectivity

- CONNECT handler and QUIC control plane stubs; strict type contracts.
- Tests cover error paths, timeouts, and validation.

8. Persistence + RBAC (real server only)

- Enable SQLx feature‑gated persistence; API keys/JWT; tenant scoping; rate limiting.
- Contract tests run against a temporary DB; mock continues to use memory.

9. E2E with mock agents and mock cloud APIs

- Use the Mock Agent and Mock API Server described in Agent Time Travel to produce deterministic moments and timeline events.
- Validate timeline, delivery events, and snapshot‑related metadata through the REST API.

10. OpenAPI and docs

- Serve `/api/v1/openapi.json`; check into repo as a generated artifact.
- Contract tests to detect breaking changes; PR bot comments with diffs.

Success criteria

- CLI → Mock server flows pass on Windows, macOS, and Linux.
- SSE streams maintain heartbeats and never block tests; reconnection logic verified.
- Idempotency works with `Idempotency-Key` across retries (no duplicate sessions created).
- Capability discovery endpoints provide stable schemas that feed CLI completions.
- Followers and run‑everywhere events are well‑formed and stream per‑host data.

11. Linux‑only topology bring‑up (Incus/Docker)

- Fixture a single Linux host running: one REST server, one leader, N followers as separate containers/VMs (Incus preferred; Docker acceptable).
- Provide a host catalog for the fleet (`.agents/hosts.json`) surfaced by `/api/v1/followers`.
- Prove end‑to‑end flows over container networking via SSH: `sync-fence` and `ah agent followers run` with per‑host SSE.
- Add failure‑injection knobs (pause a follower, simulate packet loss/latency with `tc netem`, drop ports with iptables/Incus profiles).

12. Connectivity layer E2E (Linux host)

- Validate HTTP CONNECT handler and QUIC control channel with local simulators.
- Exercise client‑hosted relay across two access points for SSH/Mutagen.

13. Cross‑OS executors (true multi‑OS)

- Bring up at least one follower per OS: Linux, macOS, Windows (physical/VM). Use SSH over CONNECT; no dynamic VPNs.
- Verify Mutagen leader→followers sync fence semantics and `ah agent followers run` adapters (POSIX shells vs PowerShell) per Multi‑OS Testing.

14. Resilience and partition tolerance

- Inject link flaps, high latency, follower restarts, and partial outages. Assert fences timeout cleanly, per‑host failures are surfaced, and log streaming remains coherent.

15. Scale and performance

- Validate N followers (e.g., 1, 3, 5, 10) for log multiplexing, fence latency distributions, and summary event correctness.

## Test Plan (precise)

Harness components

- Rust integration tests using `tokio::test` and in‑process `axum` server instances.
- CLI E2E tests invoking `ah` with `--rest-base=http://127.0.0.1:<port>` and capturing stdout/stderr.
- SSE test helper that reads `EventSource` lines, decodes with transport enums, and asserts sequence and liveness heartbeats.

Fixtures

- In‑memory stores (sessions, logs, events) with a configurable seed for deterministic timelines.
- Synthetic session generator producing `status`, `log`, `moment`, and `delivery` events.

Scenarios

1. Create task (happy path)

- POST tasks with idempotency key K; expect `201` with `sessionId`.
- Retry with K; expect `201` or `200` with same `sessionId`.

2. Session lifecycle + SSE

- Subscribe to events; assert ordered statuses (provisioning → running → completed) and periodic heartbeats.
- Cancel mid‑run; assert `stopping → cancelled`.

3. Logs and workspace

- Append synthetic logs; assert tail fetch returns last N entries.
- Workspace summary returns provider and mount info.

4. Capability discovery

- Agents, runtimes, runners schemas match snapshots; `runners[*].snapshotCapabilities` includes `git` when applicable.

5. Event ingestion for multi‑OS

- Simulate/emit leader‑originated events via `events/ingest`; assert per‑host `hostStarted`, `hostLog`, `hostExited`, and `summary` events are delivered over session SSE.

6. Connectivity stubs

- Request keys; assert validation errors for unsupported providers and shape for supported ones.

7. Negative tests

- Validation errors follow Problem+JSON; rate limit returns `429` with `Retry-After`.

CI wiring

- GitHub Actions matrix: `windows-latest`, `macos-latest`, `ubuntu-latest`.
- Run unit/integration tests; run CLI E2E tests against mock server; publish logs on failure.

Exit criteria

- All scenarios pass on CI; OpenAPI diff guard is clean for non‑breaking changes; manual spot‑checks confirm CLI usability against the mock.

---

## Multi‑OS and Connectivity Test Plan (expanded)

This section elaborates concrete topologies, milestones, and automated scenarios that align with:

- CLI.md (fleet orchestration, local vs remote)
- [Multi-OS Testing.md](Multi-OS Testing.md) (leader/followers, sync‑fence, run‑everywhere)
- [Multi-OS Testing.md](Multi-OS Testing.md) (QUIC control plane, SSH over CONNECT, client‑side relay)

### Topologies Under Test

- T0 — Single host (no followers): baseline REST/SSE behavior.
- T1 — Linux leader + 1 follower (same host, containers): SSH over container network.
- T2 — Linux leader + N followers (same host, containers): star topology.
- T3 — Cross‑host Linux leader + Linux followers over CONNECT.
- T4 — Cross‑OS: Linux leader + macOS + Windows followers over CONNECT; hybrid relay across multiple access points.

### Milestone A — Linux Containers Topology (first goal)

Objective: Prove the communication topology and APIs work on a single Linux box using Incus or Docker containers to simulate separate machines.

- Provisioning
  - Incus: profiles for leader/followers with bridged NICs and per‑container hostnames.
  - Docker: `docker compose` network with static names; expose SSH inside each follower.
  - Host catalog emitted via REST (`/api/v1/followers`) from `.agents/hosts.json` seeded by the harness.

- Scenarios (A1–A8)
- A1: Connectivity — executors connected over QUIC; leader reaches followers via CONNECT.
  - A2: Mutagen session up — leader→followers one‑way sync sessions established; ignore rules applied; report health.
  - A3: Sync fence happy path — agent runs `fs_snapshot_and_sync`; leader ingests `fenceStarted/fenceResult`; SSE reflects consistent across hosts < 5s.
  - A4: fleet run fan‑out — invoke `ah agent followers run` on leader; leader ingests `host*` and `summary`; SSE aggregates match CLI output.
  - A5: Fence timeout — `tc netem delay 1500ms loss 20%` on follower‑2; `fenceResult` shows timeout for that host only.
  - A6: Partial failure — stop SSH on follower‑3; run‑everywhere `summary` marks that host failed; others succeed.
  - A7: SSE liveness — long‑running runs stream heartbeats/logs without blocking.
  - A8: Path mapping — verify working directory translation per follower container path.

- Assertions
  - REST responses match transport types; SSE ordering preserved per host; leader exit code reflects aggregate failure.

### Milestone B — Connectivity E2E on Linux (CONNECT and relay)

Objective: Validate Connectivity Layer choices and fallbacks without leaving Linux.

- Scenarios (B1–B7)
- B1: CONNECT only — verify SSH/Mutagen via ProxyCommand; fence + run‑everywhere succeed.
- B2: Client relay — two access points, client relays across CONNECT streams; fence + run‑everywhere succeed.

### Milestone C — Cross‑OS Synchronization

Objective: Verify real multi‑OS synchronization and command adapters.

- Setup
  - macOS follower with FSKit mount path; Windows follower with `S:` mapping (even without WinFsp overlay in follower mode); Linux follower native.
  - Overlay networking preferred (Tailscale/NetBird/ZeroTier) else SSH‑only.

- Scenarios (C1–C8)
  - C1: Sync fence across OSes — leader snapshot → `fenceResult` shows consistent on macOS and Windows.
  - C2: Command adapters — `ah agent followers run -- yarn workspace <package> run test` executes via zsh (macOS) and PowerShell (Windows) with correct quoting.
  - C3: Env normalization — required env vars and PATH present; toolchain discovery succeeds on all followers.
  - C4: Large tree — sync efficiency across ignores; no runaway CPU on followers.
  - C5: File rename edge cases — case sensitivity mismatch (Windows/macOS) handled; no oscillation.
  - C6: Line endings — CRLF/LF preserved as intended by project; tests still pass.
  - C7: Binary artifacts — ensure ignores prevent copying large build outputs back to leader.
  - C8: Failure surfaces — Windows PowerShell non‑zero exit propagates; summary reflects per‑host status.

### Milestone D — Resilience and Partitions

- Scenarios (D1–D6)
  - D1: Follower restart mid‑run — reconnect, resume logging; leader receives `hostExited` appropriately.
  - D2: Network partition — drop overlay routes; handshake degrades to relay if enabled; otherwise errors are explicit.
  - D3: Slow follower quarantine — configurable selector lets fast hosts proceed while laggards are excluded.
  - D4: Log integrity — ordered per‑host streams under jitter; SSE heartbeats prevent idle timeouts.
  - D5: Cancel/stop — mid‑run cancellation propagates, all transports tear down cleanly.
  - D6: Cleanup — CONNECT sessions are torn down cleanly at session end.

### Milestone E — Scale and CI

- N followers matrix (1/3/5/10) with sampled latencies; assert fence p95 and p99 bounds; ensure API and SSE throughput stable.
- CI jobs dedicate one lane for containers (Milestone A/B) and one for real OS runners (Milestone C smoke).

### Harness Components (automation)

- `just` targets
  - `just e2e-topology up [n=3] backend=incus|docker`
  - `just e2e-topology netem host=follower-2 delay=1500ms loss=20%`
- `just e2e-topology relay two-aps=true`
  - `just e2e-topology down`

- Artifacts
  - Generated `.agents/hosts.json` for the fleet and REST fixture for `/api/v1/followers`.
  - Logs and timing summaries for fences and run‑everywhere per host.

### Assertions (cross‑cutting)

- Contracts
  - All new endpoints remain covered by black‑box tests using shared transport types.
- SSE event taxonomies for followers run are stable and documented.

- Security
  - Non‑privileged SSH users; API keys/JWT/mTLS honored where configured.
  - Overlay ACL tags (when used) constrain leader↔followers reachability.

### Exit Criteria per Milestone

- A: All A‑series scenarios pass on Linux with containers (Incus or Docker).
- B: B‑series scenarios pass; client‑side relay can carry SSH/Mutagen reliably.
- C: Cross‑OS smoke suite passes on at least one Windows and one macOS follower.
- D: Resilience tests demonstrate graceful degradation and clear error reporting.
- E: Scale tests meet agreed p95/p99 targets; metrics exported for dashboards.
