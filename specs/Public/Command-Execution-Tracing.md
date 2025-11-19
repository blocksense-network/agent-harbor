---
title: Command Execution Tracing Design Rationale
status: Draft
last_updated: 2025-11-19
---

<!-- cspell:ignore libah -->

# Command Execution Tracing Design Rationale

## Overview

The command-execution tracing subsystem provides end-to-end visibility into every process launched within an Agent Harbor session. It consists of a pre-loadable shim (`ah-command-trace-shim`), a lightweight IPC protocol (`ah-command-trace-proto`), and a recorder service (`ah-command-trace-server`). The shim is injected via `LD_PRELOAD`/`DYLD_INSERT_LIBRARIES`, instruments process creation, streams stdout/stderr in real time, and reports lifecycle events back to the recorder.

This document captures the design motivations and key implementation decisions that emerged while stabilizing milestone **R9 / M2**. It explains why the shim is structured the way it is, the trade-offs we accepted, and the operational constraints that shaped the final behaviour.

## Goals

- Capture every process that participates in a traced session, including shell pipelines, interpreters, and short-lived binaries.
- Attribute stdout/stderr bytes to the correct logical command in chronological order.
- Fail open: when tracing infrastructure is unavailable, the developer workload must continue uninterrupted.
- Stay portable between Linux and macOS while sharing as much POSIX code as possible.
- Keep the recorder simple: a single-threaded Tokio service that accepts multiple shim connections, validates handshakes, and stores streamed events for higher-level consumers.

## Key Challenges

1. **Async-signal-safety**: POSIX forbids most operations between `fork()` and `exec()`. Attempting to acquire `Mutex`es, allocate memory, or open sockets in that window can deadlock the child. Early prototypes instrumented `exec*`, `clone`, and `vfork` directly and immediately tried to connect back to the recorder. Under load, this wedged Python subprocesses in `D` state and caused the `shim_shell_and_interpreter_coverage` test to time out.

2. **Shim availability**: The `LD_PRELOAD`ed cdylib must always exist at a predictable path. Tests run under `cargo nextest` in parallel, so copying `target/debug/deps/libah_command_trace_shim.so` into `target/debug` at runtime was racy and sometimes produced zero-byte files.

3. **Test determinism**: The e2e suites rely on a deterministic handshake between the shim and the recorder. Because we inject the shim into shells and interpreters launched via `posix_spawn` and `fork+exec`, we need a consistent way to report processes regardless of how they were spawned.

## Design Decisions

### 1. Self-Reporting Shim Initialization

- The shim now **self-reports** immediately after `CommandTraceClient::connect` succeeds. We gather PID, PPID, argv, environment, current working directory, and executable path from the already-exec’d process image and send a `CommandStart` message straight away.
- This approach eliminates the need for instrumenting `exec*`/`clone`/`vfork`, keeping the code asynchronous-signal-safe. The only hooks left are the portable POSIX family (`posix_spawn`, `posix_spawnp`, `dup*`, `write*`, etc.) where glibc guarantees a safe calling context.
- Self-reporting is idempotent; the recorder deduplicates `CommandStart` messages with identical PID/executable pairs. We kept parent-side `posix_spawn*` hooks so crashes that occur before the child shim loads are still recorded.

### 2. Fail-Open Connection Strategy

- The shim lazily connects the first time it needs to emit data (`initialize_client()` is called from both `send_command_start` and `send_command_chunk`). We attempt a single handshake; if it fails, we log (when logging is enabled) and downgrade the shim state to `ShimState::Error`.
- We removed the previous “retry 5 times” loop, because queued `UnixStream::connect` calls inside `#[ctor]` caused a thundering-herd stall across shells and interpreters.
- Once in the `Error` state, all hooks become no-ops, ensuring the developer’s workload continues even if the recorder socket never comes up. This aligns with the “fail open” objective.

### 3. Deterministic Artifact Placement

- `find_shim_path()` now builds the cdylib on demand (via `cargo build -p ah-command-trace-shim`) and locates it directly inside `target/<profile>/deps`. We removed the `build.rs` copy step inside `ah-command-trace-e2e-tests` to avoid zero-byte artifacts.
- The e2e harness runs tests serially, ensuring the freshly built shared library is reused across all scenarios without re-copying or symlinking.

### 4. Stream Attribution

- The shim maintains an in-process FD table (stdout/stderr `dup` propagation) so the recorder can attribute every `write`, `writev`, or `sendmsg` payload to the correct logical command stream (`StreamType::Stdout`/`StreamType::Stderr`).
- To avoid recursive loops when the client itself writes to traced FDs, we guard all send paths with a thread-local `IN_TRACE` flag.

### 5. macOS Interpose Reliability

- macOS builds now emit explicit `__DATA,__interpose` records for every hook and mark them `#[used]` so the linker cannot dead-strip them when `-dead_strip` is in effect. Without this change, hooks compiled successfully but never ran on macOS because the interpose section was empty.
- To keep the hook definitions portable, we vendored `redhook` into `crates/stackable-interpose` and updated every call site to use that crate. Its macOS implementation now emits deterministic interpose records with `#[used]`, while the Linux path keeps the familiar `LD_PRELOAD` trampoline behavior. This preserved the Linux workflow while guaranteeing macOS chunk capture parity.
- The macOS build still avoids interposing variadic libc symbols (e.g., `fcntl`) until we can model their call signatures safely; those cases are tracked in the R9 status file as follow-up work.

## Verification Strategy

- **Unit tests**: `ah-command-trace-client` exercises handshake and SSZ serialization. `ah-command-trace-proto` covers message encoding/decoding.
- **E2E tests (`ah-command-trace-e2e-tests`)**:
  - `shim_injection_smoke_with_socket`: ensures the shim can preload, connect, and register a simple helper binary end-to-end.
  - `shim_shell_and_interpreter_coverage`: launches nested shells and Python interpreters, asserting that at least one shell and one interpreter emit `CommandStart`. This test caught the deadlock/regression described above and now serves as the guardrail for the new self-reporting logic.
  - `shim_output_capture`: validates stdout/stderr interleaving across multiple streams.

We run these scenarios via `cargo test -p ah-command-trace-e2e-tests <test_name> -- --nocapture` to preserve the custom log output and to avoid nextest’s parallelization, which doesn’t respect the shim’s serialized requirements yet.

## Operational Considerations

- **Logging**: Controlled by `AH_CMDTRACE_LOG`. When enabled, the shim logs connection events and self-report failures to stderr, but all instrumentation remains silent by default.
- **Environment overrides**: Tests may set `AH_SHELL_TEST_LOG=/tmp/...` to append breadcrumbs from the helper binary, aiding debugging without touching the main code path.
- **Security**: The shim doesn’t attempt to trace setuid binaries. `LD_PRELOAD` is skipped when `AT_SECURE` is set.

## Future Work

- **MacOS parity**: The self-reporting + chunk-capture stack now runs on macOS via `__interpose` hooks, so Linux/macOS have feature parity through M2. Remaining mac gaps (e.g., safe `fcntl` interception for variadic commands) are tracked in `specs/Public/R9.status.md`.
- **Command deduplication**: The server currently treats every `CommandStart` as authoritative. Long term we should merge parent- and child-reported metadata (e.g., keep parent-supplied argv when the child fails before self-reporting).
- **Backpressure-aware streaming**: The shim sends `CommandChunk` messages synchronously. Introducing bounded buffers or batching could reduce syscall overhead without sacrificing attribution fidelity.

## References

- `crates/ah-command-trace-shim/src/posix.rs`
- `crates/ah-command-trace-shim/src/platform/linux.rs`
- `crates/ah-command-trace-client/src/lib.rs`
- `crates/ah-command-trace-e2e-tests/tests/shim_injection_smoke.rs`
- `specs/Public/R9.status.md`
