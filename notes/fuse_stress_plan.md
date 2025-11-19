# FUSE Stress & Fault Injection (F7) Plan – Nov 2025

This note captures the actionable blueprint for implementing **F7 – Stress Testing and Fault Injection** from `specs/Public/AgentFS/FUSE.status.md:260-279`. It inventories the existing harness conventions, documents the observability/data artifacts we must produce, and breaks down the concrete components that each F7 sub-track (T7.1–T7.4) will extend. The goal is to keep the implementation aligned with the pjdfstest/performance harness patterns documented in `notes/fuse_pjdfs_context.md` so new tests integrate cleanly with CI and future agents.

## Existing Harness Inventory

- `scripts/test-fuse-basic-ops.sh`, `test-fuse-negative-ops.sh`, `test-fuse-overlay-ops.sh`, and `test-fuse-control-plane.sh` — All share the same structure: timestamped run directories under `logs/`, tee'ed console logs, and JSON summaries for machine comparison.
- `scripts/test-fuse-performance.sh` — Reference implementation for richer telemetry: creates `$RUN_DIR/results.jsonl` + `summary.json`, captures `fuse-config.json`, and mirrors each subtest with per-target `.log` + `.time` files (see `logs/fuse-performance-20251117-070644`). This harness will be cloned for the stress suite.
- `scripts/test-fuse-mount-concurrent.sh` — Provides ready-made helpers for orchestrating multiple mounts, verifying mount state (`wait_for_mount_state`), PID checks, and cleanup logic; its concurrency scaffolding will be reused inside the stress orchestrator.
- `notes/fuse_pjdfs_context.md` — Establishes the logging policy (concise console output, detailed log files under `logs/…`), unique run directories, and how we archive structured summaries for regressions. The stress suite will follow the exact same conventions.

## Harness + Artifact Layout

We will introduce a dedicated orchestration script plus a reusable stress workload binary:

| Component                                                               | Responsibility                                                                                                                                                                                                                                                                                 |
| ----------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `scripts/test-fuse-stress.sh`                                           | Shell entry point invoked via `just test-fuse-stress`. Handles building binaries, preparing timestamped run directories `logs/fuse-stress-<ts>/`, generating FUSE configs, mounting/unmounting, applying resource limits, invoking workload/fault injection clients, and collecting artifacts. |
| `crates/agentfs-fuse-stress/`                                           | Rust stress driver library + CLI (`agentfs-fuse-stress run`). Spawns concurrent workers (T7.1), tracks per-op metrics, toggles control-plane knobs (fault policies, crash triggers), and emits structured status via stdout/JSON.                                                              |
| `scripts/lib/fuse_stress_parser.py` (or extend `scripts/test_utils.py`) | Parses individual sub-test outputs, writes `results.jsonl` entries, aggregates into `summary.json`, and enforces success criteria thresholds.                                                                                                                                                  |

Each run directory will contain:

- `stress.log` — Tee'ed console log from the shell orchestrator.
- `fuse-config.json` — Exact FsConfig (mirrors performance harness for reproducibility).
- `results.jsonl` — Line-delimited JSON records for every sub-workload and injection phase (see schema below).
- `summary.json` — Aggregated list summarizing pass/fail ratios + resource stats for CI gating.
- Subdirectories per phase (`concurrency/`, `fault_injection/`, `resource/`, `crash/`) with workload-specific raw logs and pidstats (e.g., `<phase>/worker-<n>.log`, `<phase>/proc_fd_snapshot.json`).

`scripts/test-fuse-stress.sh` now executes two automated phases:

1. **T7.1 Concurrency** – `agentfs-fuse-stress run …` (existing multi-thread mix) generating `concurrency/report.json` and integrity fingerprints.
2. **T7.3 Resource Exhaustion** – `agentfs-fuse-stress resource --mode fd_exhaust …` keeps opening files until the kernel returns `EMFILE`/`ENFILE`, records peak `/proc/self/fd` counts, cleanup latencies, and emits `resource/report.json`. The harness temporarily lowers `RLIMIT_NOFILE` for the resource phase so we deterministically hit the limit even though the global limit is raised to 65 536 for the rest of the run.
3. **T7.4 Crash Recovery** – `agentfs-fuse-stress crash --mount …` creates a deterministic dataset, fingerprints it, kills the active `agentfs-fuse-host` (either via autodetection or explicit PID), and writes `crash/pre-crash.json` containing the digest + metadata. The shell harness remarshals AgentFS, re-fingerprints (`post-crash.json`), and logs any digest mismatch (expected today because the in-memory FsCore is rebuilt) while still failing the run if remount/fingerprint steps break.

### Fault injection control-plane schema

`agentfs-core` now exposes a JSON-serializable `FaultPolicy` (control-plane wiring lands in T7.2) that the CLI can push via `agentfs-control-cli --mount <mnt> fault-policy-set --file policy.json`. The schema mirrors:

```jsonc
{
  "enabled": true,
  "rules": [
    {
      "op": "write", // read|write|truncate|allocate|clone_cow|sync
      "errno": "eio", // eio|enospc
      "start_after": 0, // optional warmup count (defaults to 0)
      "max_faults": 1, // optional max faults for this rule
    },
  ],
}
```

When `enabled` is true and at least one rule exists, storage ops consult the shared `FaultInjector`. `fault-policy-clear` drops back to the empty policy, and both commands return a `FaultPolicyStatus` response so harnesses can assert on the active rule count.

## Proposed JSON Schema (results.jsonl)

Each record is a JSON object with the following structure so downstream automation can diff runs:

```jsonc
{
  "phase": "concurrency|fault_injection|resource|crash_recovery",
  "name": "t7.1.concurrent_mix",
  "workload": {
    "threads": 32,
    "duration_sec": 120,
    "ops": {
      "create": 15000,
      "write": 9800,
      "rename": 3200,
      "unlink": 8700,
    },
  },
  "fault_policy": {
    "enabled": true,
    "inject": [
      { "op": "write", "error": "EIO", "probability": 0.02 },
      { "op": "fsync", "error": "ENOSPC", "probability": 0.01 },
    ],
  },
  "resource_limits": {
    "nofile": 4096,
    "memory_mb": 1024,
    "cgroup_path": "/sys/fs/cgroup/agentfs-stress",
  },
  "metrics": {
    "ops_per_sec": 5231.4,
    "errors": {
      "EIO": 32,
      "ENOSPC": 17,
    },
    "max_latency_ms": 184,
    "mean_latency_ms": 12.4,
    "fuse_queue_depth_max": 64,
    "writeback_queue_depth_max": 18,
  },
  "data_integrity": {
    "tree_hash_before": "sha256:…",
    "tree_hash_after": "sha256:…",
    "mismatch_paths": [],
  },
  "status": "passed|failed",
  "notes": "Any human-readable diagnostics",
}
```

`summary.json` will be an array of `{ "phase": "t7.1", "status": "passed", ... }` objects capturing only the high-level verdict plus key ratios so CI log review is easy.

## Observability + Instrumentation delta

To support the schema and automate assertions we need extra telemetry from the runtime:

1. **FUSE host counters** (in `agentfs-fuse-host/src/adapter.rs`)
   - Export max/avg writeback queue depth and pending request counts per handle; expose snapshots via the existing tracing channel (`target: "agentfs::fuse"`) and, if feasible, a debugfs-style control file under `.agentfs/metrics` that the stress driver can read.
   - Surface crash markers (e.g., last fatal signal, outstanding handles) so crash-recovery tests can verify cleanup.
2. **FsCore fault injection hook**
   - Wrap the active backstore (`HostFsBackstore` by default) with a `FaultInjectingBackstore` that consults a shared `FaultPolicy`. The policy will be updateable through the control plane (`agentfs-control-cli fault-policy set`), satisfying T7.2.
   - Provide tracing events for injected errors so the harness can correlate expected vs. observed errno.
3. **Resource telemetry**
   - Lightweight monitor (Python helper) that samples `/proc/<pid>/{stat,fd}` for the fuse host and emits JSON snapshots. This reuses the script infrastructure already shipping for performance tracking.

## Harness Workflow Outline

1. **Setup** — `test-fuse-stress.sh` raises `ulimit -n`, prepares backstore dirs, writes FsConfig, mounts AgentFS via `just mount-fuse`, and ensures clean teardown via traps (mirroring the performance script).
2. **T7.1 (Concurrent Operations)** — Invoke `agentfs-fuse-stress run --phase concurrent` with configurable `--threads`, `--duration`, `--tree fanout`. Capture per-worker logs and record aggregate ops/sec + integrity hashes.
3. **T7.2 (Fault Injection)** — Use the new control-plane RPCs to arm fault policies before running targeted workloads (e.g., repeated `fsync`, snapshot churn). Verify returned errno matches the injected value and invariants hold (no stuck handles, metadata consistent).
4. **T7.3 (Resource Exhaustion)** — Apply env vars (`STRESS_NOFILE`, `STRESS_MEM_LIMIT_MB`, `STRESS_BACKSTORE_QUOTA_MB`) so CI can run lighter constraints while burn-in uses aggressive ones. For each resource scenario record the limit, detection of expected errno (EMFILE, ENOSPC, OOM signal), and cleanup time.
5. **T7.4 (Crash Recovery)** — The orchestrator spawns a watchdog that periodically `kill -9`’s `agentfs-fuse-host`, then remounts via `just mount-fuse` and runs integrity checkers (`agentfs-fuse-stress verify-tree` + `agentfs-control-cli branch-verify`). Results stored under `crash/` with `before.json`/`after.json` reports.

## Configuration & Interfaces

- `just test-fuse-stress` — New target in `Justfile` that runs the shell orchestrator with sane defaults (`concurrency_duration=120s`, `resource_nofile=4096`, etc.). Accepts env overrides (`STRESS_DURATION_SEC`, `STRESS_THREADS`, `STRESS_FAULT_SPEC`, `STRESS_RESOURCE_MODE`, `STRESS_CRASH_INTERVAL`) so CI/nightly/local runs can scale.
- Control-plane additions:
  - `agentfs-control-cli fault-policy set --branch <id> --spec @policy.json`
  - `agentfs-control-cli fault-policy clear --branch <id>`
  - The spec document matches the JSON snippet shown earlier; exact schema will be formalized in the CLI help output and `specs/Public/AgentFS/AgentFS-Control-Messages.md` once implemented.
- Stress driver CLI examples:
  - `agentfs-fuse-stress run --phase concurrent --mount /tmp/agentfs --threads 32 --duration 120`
  - `agentfs-fuse-stress run --phase resource --mode nofile --limit 2048`
  - `agentfs-fuse-stress verify-tree --mount /tmp/agentfs --log crash/tree-check.json`

## Implementation Sequencing

1. Land the scaffolding (`test-fuse-stress.sh`, crate workspace entry, run-dir logging helpers) + JSON schema definitions.
2. Flesh out T7.1 by building the concurrent workload generator + integrity verifier.
3. Implement fault-injection plumbing in `agentfs_core` + control plane (T7.2), then teach the harness to toggle policies mid-run.
4. Add resource exhaustion helpers (ulimit enforcement, cgroup wrappers) + monitoring hooks (T7.3).
5. Build crash-recovery watchdog + tree verifier (T7.4).
6. Wire everything into CI + `FUSE.status` deliverables summary once stable.

This document will stay in `notes/` and be updated as each sub-track lands, ensuring future agents have a single source for the stress harness architecture.

## Runtime knobs

- `FUSE_STRESS_NOFILE_LIMIT` (default `65536`) raises the global descriptor limit before mounting AgentFS so the concurrency workload can keep thousands of handles open without tripping `EMFILE`.
- `FUSE_STRESS_RESOURCE_MAX_OPEN` (default `4096`) controls how aggressively the fd-exhaustion phase runs; the harness further lowers `ulimit -n` for the resource subprocess so we deterministically hit `EMFILE/ENFILE` even though the outer shell still has a high limit.
- `FUSE_STRESS_DURATION_SEC`, `FUSE_STRESS_THREADS`, and `FUSE_STRESS_MAX_FILES` let CI/downstream scripts trade runtime for coverage. The defaults (120 s, 16 threads, 4096 files) match the verification run recorded under `logs/fuse-stress-20251119-151555/`.
