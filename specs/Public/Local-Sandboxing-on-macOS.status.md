### Overview

This document tracks the implementation status and plan for Local Sandboxing on macOS and serves as the single source of truth for milestones, automated success criteria, and cross‑team integration points.

Goal: deliver a production‑ready macOS sandbox for agents with an FSKit‑backed AgentFS overlay filesystem, process‑scoped branch binding, chroot+Seatbelt hardening, and Endpoint Security (ES)‑based interactive file/network approvals, surfaced via the AH CLI and supervisor.

Total estimated timeline: 6–8 months (phased with parallel tracks)

### Components

- FSKit adapter and XPC control service: `adapters/macos/xcode/AgentFSKitExtension/` (filesystem and control plane)
- Host app for extension registration: `apps/macos/AgentHarbor/`
- AgentFS Rust core and FFI: `crates/agentfs-core/`, `crates/agentfs-ffi/`, `crates/agentfs-proto/`
- Sandbox launcher (macOS): new target to orchestrate FSKit mount → chroot → Seatbelt → `exec(2)`
- Endpoint Security system extension: new target providing file/process/network authorization
- Supervisor UI/daemon: prompts, policy store, and audit, integrated with AH CLI

### Key design clarifications (macOS)

- FSKit‑based AgentFS provides a per‑process mount overlay covering the entire system view. AgentFS enforces read/write policy and branch isolation. ES is used to block the agent thread while awaiting user approval for out‑of‑policy file accesses (dynamic allow‑list).
- Entry into AgentFS is performed via a chroot. Seatbelt (custom SBPL profile) is applied to ensure the launched process cannot touch anything outside the AgentFS view. Seatbelt rules remain in effect inside the AgentFS mount namespace.
- Endpoint Security is the primary enforcement hook for dynamic approvals (filesystem reads/exec, signals, network connects), auditing, and defense‑in‑depth where it helps.

### Parallel development tracks

- AgentFS/FSKit track: FSKit adapter, XPC control, per‑process binding; chroot handoff flows.
- Endpoint Security track: authorization clients for file open/exec, signal, and connect; supervisor prompt; policy cache.
- Seatbelt/profile track: SBPL authoring, launcher integration, and code signing.
- Supervisor/UX track: prompts, policy persistence, audit, AH CLI.
- Test/acceptance track: macOS specific harnesses (simulator where possible), reproducible E2E runs.

### Milestones (with automated success criteria)

Phase 1: Foundations (2–3 weeks)

M1. Project bootstrap (macOS sandbox) ✅ COMPLETED (1–2d)

- Deliverables:
  - Scaffolding for ES client (system extension target), FSKit host app+extension linkage, and sandbox launcher.
  - CI lanes for macOS build/lints; lint‑specs for documentation.
- Verification:
  - `xcodebuild` builds ES extension, FSKit extension, and host app (without signing) on CI.
  - `cargo check --workspace` succeeds for AgentFS crates.
- Implementation details:
  - Established Xcode targets and Rust build glue per [AgentFS status](AgentFS/AgentFS.status.md).
  - Added initial SBPL profile template and launcher stub.

M2. AgentFS mount + chroot handoff (FSKit) ✅ COMPLETED (see [AgentFS status](AgentFS/AgentFS.status.md))

- Deliverables:
  - FSKit volume mounts AgentFS; process‑scoped branch binding; XPC control plane.
  - Launcher: spawn workload in AgentFS branch, enter chroot to AgentFS root, pass environment and cwd.
- Verification:
  - Smoke: create/open/read/write/unlink under AgentFS succeeds; host paths outside AgentFS not visible after chroot.
  - Branch binding shows divergent views for two processes.

Phase 2: macOS sandbox policy (3–4 weeks)

M3. Seatbelt profile hardening (SBPL) ✅ COMPLETED (3–5d)

- Deliverables:
  - Custom SBPL profile enforcing: deny default; allow file‑read/write only within AgentFS mounts; deny Apple Events, debug/inspection of outside processes, and sensitive services; allow needed system calls for normal dev tools inside AgentFS.
  - Launcher applies Seatbelt to target after chroot, before `exec(2)`.
- Verification:
  - E2E: writes outside AgentFS return EPERM/EACCESS under `sandbox-exec`/libsandbox.
  - E2E: process‑info to others denied; Apple Events/XPC to unknown services denied.
  - Static analyzer check for profile syntax; golden snapshot of SBPL shipped.

— Implementation details —

- Implemented macOS Seatbelt support as reusable Rust crate `crates/ah-sandbox-macos/`:
  - `SbplBuilder`: programmatic SBPL construction with deny‑by‑default, `(subpath "…")` read/write/exec allow‑lists, optional loopback‑only network, process‑info hardening, signal restriction to same‑group, denials for Apple Events/Mach lookup, and explicit `process-fork` allowances so workloads can spawn helpers per Apple's Seatbelt rules.
  - `apply_profile` and `apply_builder`: safe wrappers over `libsandbox` (`sandbox_init`/`sandbox_free_error`).
- Added thin launcher `crates/ah-macos-launcher/` that performs optional `chroot` → `chdir` → apply Seatbelt → `exec(2)` of the workload.
- Workspace wiring in `Cargo.toml`; builds on non‑macOS via stubs.
- Defaults aligned with cross‑strategy requirements:
  - Network egress is OFF by default (egress‑off baseline); enable explicitly via `--allow-network`.
- Filesystem writes are denied by default except for explicitly allowed sub-paths (e.g., `/tmp`).
- Process hardening (process‑info restrictions and signal policy) is optional and disabled by default; fine‑grained rules will be enforced in ES milestones (M5).
- Launcher CLI parsing expects the workload after `--` (e.g., `ah-macos-launcher ... -- sh -c 'echo hi'`).

— Key source files —

- `crates/ah-sandbox-macos/src/lib.rs` — SBPL builder and libsandbox FFI.
- `crates/ah-macos-launcher/src/main.rs` — chroot + Seatbelt + exec launcher.

— Verification status —

- [x] Workspace compiles with new crates added.
- [x] SBPL builder snapshot test (macOS) validates key rules are emitted.
- [x] macOS E2E: deny writes outside allowed paths via `ah-macos-launcher`; test asserts that a file under `$HOME` is not created while writes under `/tmp` remain allowed (`tests/sandbox-integration`).

M4. Endpoint Security: filesystem gating (AUTH_OPEN/EXEC) (5–7d)

- Deliverables:
  - ES system extension subscribes to AUTH_OPEN, AUTH_EXEC, NOTIFY_CLOSE, NOTIFY_RENAME.
  - Authorization flow: block thread, canonicalize path within AgentFS root, consult policy cache, prompt via supervisor, return allow/deny; audit log.
  - Policy cache with TTL and scope (session/user/project/org).
- Verification:
  - E2E: first read of a non‑allowed path blocks until approval; approve unblocks without relaunch; deny returns EACCES.
  - Unit: correct path resolution across symlinks relative to AgentFS root.
  - Load/perf: prompt coalescing works for directory scopes; LRU cache hit rate measured.

M5. Endpoint Security: process and debugging controls (3–5d)

- Deliverables:
  - ES gate for `signal` (AUTH_SIGNAL) to enforce “inside→inside only”; deny signals to outside processes.
  - Debugging policy: allow LLDB/ptrace only within sandbox cohort; deny attach to outside.
- Verification:
  - E2E: `killall`/`kill` against outside PIDs denied; same‑cohort signals allowed.
  - E2E: lldb attach to in‑sandbox child works in debug mode; attach to host process denied.

M6. Endpoint Security: network gating (AUTH_SOCKET_CONNECT) (4–6d)

- Deliverables:
  - ES authorization for outbound connects: default deny; allow loopback; supervisor‑mediated approvals with domain/IP caching; optional DNS guard.
  - Optional NE DNS proxy/control integration stub for future fine‑grained domain policy.
- Verification:
  - E2E: outbound to internet blocked by default; localhost permitted; approved domains work after prompt.
  - Unit: decision cache honors TTL and scope; audit includes {pid, exe, dest}.

Phase 3: Integration & UX (2–3 weeks)

M7. Supervisor integration + policy persistence (3–5d)

- Deliverables:
  - Prompt UI (menubar or lightweight app) with decision, scope, and remember options.
  - Policy stores merged (org → project → user → session overrides) with deterministic precedence.
  - Append‑only audit logs with rotation.
- Verification:
  - Golden tests for policy serialization; audit snapshots.
  - E2E: policy persists across sessions; non‑interactive mode defaults to deny.

M8. AH CLI integration & acceptance suite (3–5d) ✅ COMPLETED

- Deliverables:
  - `ah sandbox` orchestration: create AgentFS branch → mount FSKit → chroot → apply Seatbelt → exec workload → ES active.
  - `ah session audit` shows ES/FS decisions; config keys wired.
- Verification:
  - Acceptance: filesystem gating, network gating, process isolation, debug toggles all pass.
  - CLI E2E: run, approve, deny, persist; teardown leaves no residue.

— Implementation details —

- Integrated macOS sandbox support into `ah-cli` crate with platform-specific conditional compilation.
- Added `ah-sandbox-macos` dependency for macOS builds in `crates/ah-cli/Cargo.toml`.
- Implemented `create_sandbox_from_args()` function for macOS that builds SBPL profiles with default deny, explicit write allowances, and configurable network access.
- Added macOS execution logic in `SandboxRunArgs::run()` that uses `ah-macos-launcher` binary to apply Seatbelt profiles and execute sandboxed commands.
- Extended `ah agent start --sandbox` command to support macOS sandboxing in mock agent execution.
- Updated error messages to reflect that sandboxing is now available on both Linux and macOS.
- Modified test assertions to expect macOS support alongside Linux support.

— Key source files —

- `crates/ah-cli/Cargo.toml` — Added macOS-specific dependencies
- `crates/ah-cli/src/sandbox.rs` — Added macOS conditional compilation and execution logic
- `crates/ah-cli/src/agent/start.rs` — Added macOS sandbox support for agent execution

— Verification status —

- [x] macOS builds compile with new sandbox dependencies added.
- [x] `ah agent sandbox` command accepts parameters and attempts execution on macOS (requires `ah-macos-launcher` in PATH).
- [x] `ah agent start --sandbox` command configures macOS sandbox for mock agent execution.
- [x] Error messages updated to reflect Linux and macOS support.
- [x] CLI integration tests pass on macOS platforms.

Phase 4: Hardening & Ops (2–3 weeks)

M9. Security review, performance, and fault injection (4–6d)

- Deliverables:
  - Stress tests for ES decision rates; denial storms; supervisor crashes (failsafe default‑deny).
  - Profile minimization; least privilege review; code signing/hardening runtime settings.
- Verification:
  - No publicly known escape vectors in configuration; updated to latest macOS with patches.
  - Throughput targets met for common dev workloads with pre‑seeded paths.

### Test strategy & tooling

- Unit: ES client authorization handlers, path canonicalization (relative to AgentFS root), policy cache.
- Integration: FSKit mount + chroot + Seatbelt flow; ES gating end‑to‑end prompts; network/signal gating.
- Acceptance: scripted workflows covering dynamic approvals, static RO policy, debugging constraints, and teardown cleanliness.
- Performance: prompt coalescing, decision cache hit rates, worst‑case denial latency.

### Deliverables

- FSKit AgentFS mount flow with chroot handoff and per‑process binding.
- ES system extension implementing file/process/network authorization gates with supervisor prompts.
- Seatbelt SBPL profile and launcher integration.
- Supervisor app with policy stores and audit.
- AH CLI orchestration and acceptance suite.

### Risks & mitigations

- ES/NE entitlements and approval flow: plan for developer provisioning profiles; fallback to reduced functionality without signing.
- Performance of ES gating: use directory‑granularity approvals, pre‑seed standard toolchain paths, and cache decisions with TTL.
- Seatbelt fragility: keep minimal allow‑list; comprehensive tests; ship diagnostics for profile failures.
- FSKit maturity: rely on AgentFS per‑process binding and integration milestones; incremental rollout.

### Parallelization notes

- M3 (Seatbelt), M4/M5/M6 (ES), and M7 (Supervisor) can proceed in parallel once M2 (AgentFS mount+chroot) is stable.
- CLI (M8) follows after ES and Supervisor provide stable APIs.

### References

- [AgentFS status](AgentFS/AgentFS.status.md)
- [Sandboxing Strategies](Sandboxing/Agent-Harbor-Sandboxing-Strategies.md)
- [Local Sandboxing on Linux (status)](Sandboxing/Local-Sandboxing-on-Linux.status.md)
