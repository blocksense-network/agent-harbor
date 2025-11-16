# FUSE Adapter Status – Nov 16 2025 (handoff)

<!-- cSpell:ignore fdatasync conv ENOTCONN writeback Backoff perfdata memmove erms memfd -->

## Current Scope

- Working branch: `feat/agentfs-fuse-f5`.
- Focus: **F6 – Performance Tuning + Benchmarking**. F4/F5 harnesses stay green, but the priority is pushing sequential write throughput toward the host baseline and documenting every `just test-fuse-performance` run.
- Tooling targets: `just test-fuse-performance`, `just test-fuse-control-plane`, and the F1/F3 sweeps (`test-fuse-basic-ops`, `test-fuse-negative-ops`, `test-fuse-overlay-ops`, `sudo -E just test-pjdfs-subset /tmp/agentfs`).

## Updates – Nov 16 2025

1. **FsCore dirty-tracking for write/ftruncate**
   - `StreamState` now remembers whether a branch is sharing snapshot content, so we only clone once per stream after a snapshot instead of before every write. Snapshot creation walks the active branch and marks streams clean; the first post-snapshot write marks them dirty and clones once (`test_snapshot_write_only_clones_once_per_stream`).
2. **HostFs descriptor cache + real fsync path**
   - `HostFsBackend` reuses `Arc<Mutex<File>>` per `ContentId`, only flushes on explicit `sync()`, and `FsCore::fsync` plumbs through the adapter. Sequential `dd … conv=fdatasync` no longer kills the mount, and `write_all` guards against partial writes (`logs/fuse-performance-20251116-110035/summary.json`).
3. **Perf profiling (automated runs)**
   - `perf record` on `just test-fuse-performance` showed ~13 % CPU in `std::io::Write::write_all → libc::write` plus kernel `ksys_write → zfs_write`, so the current IPC path (FUSE host ⇄ FsCore) was identified as the bottleneck; FsCore itself wasn’t cloning excessively.
4. **Manual perf attach (live dd workload)**
   - Mounted `/tmp/agentfs-perf-profile` with `/tmp/agentfs-perf-config.json` to attach `perf` to the live host while `dd if=/dev/zero of=/tmp/agentfs-perf-profile/perf/throughput.bin bs=4M count=64 status=none conv=fdatasync` runs in the foreground (background jobs are blocked on this environment).
   - Latest captures live under `logs/perf-profiles/agentfs-perf-profile-20251116-115242/` (default sampling) and `logs/perf-profiles/agentfs-perf-profile-20251116-115426/` (call stacks + higher frequency). Both reports show the hottest frames in `__memmove_avx_unaligned_erms` (~16 %) and `crossbeam_utils::backoff::Backoff::snooze` (~9 %), while the `HostFsBackend::write → std::io::Write::write_all` chain contributes ~1 %. Conclusion: the write worker pool’s channel + copy, not FsCore itself, dominate sequential writes.
5. **Write tracing + worker pool**
   - Added a lightweight write tracer (`AGENTFS_FUSE_TRACE_WRITES=1`, `RUST_LOG=agentfs::write=debug`) to count inflight writes. Initial traces (`logs/perf-profiles/agentfs-concurrent-20251116-095135/fuse-host.log`) showed only one write in flight; after adding a configurable background worker pool (`AGENTFS_FUSE_WRITE_THREADS`, default `available_parallelism()`), traces hit 3+ inflight writes for multi-writer workloads (`logs/perf-profiles/agentfs-concurrent-20251116-100230/fuse-host.log`).
6. **Kernel max_write/max_background knobs**
   - The adapter now honors `AGENTFS_FUSE_MAX_WRITE` (default 4 MiB, capped at 16 MiB), `AGENTFS_FUSE_MAX_BACKGROUND` (default 64), and derives `congestion_threshold = ¾ * max_background`. Combined with optional `AGENTFS_FUSE_WRITEBACK_CACHE=1`, we can reproduce each tuning run without editing JSON configs.
7. **New HostFs write error logging**
   - `HostFsBackend::write` logs the `ContentId`, offset, size, and errno when `write_all` fails (target `agentfs::storage`). This makes future `Input/output error` reports actionable without rerunning under `strace`.
8. **Performance harness reruns (sequential write ≥ 0.5×)**
   - Fresh mount/backstore plus the knobs above yielded much healthier ratios:
     - `logs/fuse-performance-20251116-120621/summary.json` – default env (seq_write 0.80×, seq_read 1.00×, metadata 0.75×, concurrent 1.00×).
     - `logs/fuse-performance-20251116-120649/summary.json` – second default run to double-check (seq_write 0.80×, seq_read 1.00×, metadata 0.70×, concurrent 2.00× because both agentfs/baseline finished in ~20 ms). The job is now limited by measurement noise.
     - `logs/fuse-performance-20251116-120715/summary.json` – `AGENTFS_FUSE_MAX_BACKGROUND=128`, `AGENTFS_FUSE_WRITE_THREADS=8`, `AGENTFS_FUSE_MAX_WRITE=8388608` (seq_write ~1.25×, metadata 1.18×; effectively identical within timing jitter).
     - `logs/fuse-performance-20251116-122208/summary.json` – sequential workload bumped to 8 GiB and the harness now drops host page caches before each read so seq_read takes ~0.5 s instead of ~0.02 s; ratios stay ~1.0× on this box because both AgentFS and the baseline saturate the same NVMe bandwidth, but we finally have a measurement that lasts long enough to catch regressions.
   - Earlier reference runs remain documented under `logs/fuse-performance-20251116-110756/…-111825/` for regression tracking.
9. **Transport redesign sketch**
   - Since perf shows the crossbeam channel + `Vec::extend_from_slice` as the bottleneck, the next experiment is to split transport responsibilities behind a trait:
     1. Introduce `FsCoreTransport` with methods for `open`, `read`, `write`, `fsync`, etc.
     2. Provide an in-process transport that keeps FsCore in the host but replaces the channel with a lock-free ring buffer (e.g., `crossbeam_deque`) backed by a slab allocator of page-aligned blocks; workers enqueue completions instead of sending replies over a pipe.
     3. Future-proof the trait so we can swap in a shared-memory transport (memfd-backed queue + doorbells) if we _do_ need to move FsCore back into a helper process.
   - This design removes the `ChildStdin`-style IPC entirely for the common case and should cut the memmove/backoff costs highlighted above.

## Pending / Next Steps

1. **Control-plane write semantics** – FsCore still refuses to mutate files after snapshot creation. Fixing that (or adding a worker flow that mutates the HostFs backstore) is required before we can demonstrate true branch-local divergence in the harness.
2. **pjdfstest regression gating (F5)** – Baseline failures live in `specs/Public/AgentFS/pjdfstest.baseline.json`. Start fixing chmod/chown/ftruncate/utimens failures and keep growing the baseline so the diff stays meaningful.
3. **Performance tuning (F6)** – Even though this machine now reports ≥ 0.8× for sequential writes, both AgentFS and the baseline saturate NVMe bandwidth (even with the new 8 GiB workload and cache drops), so measurement noise still hides small deltas. Keep logging every run and try again on a slower host (or larger workloads) while prototyping the shared-buffer transport.
4. **pjdfstest fixes + CI** – With the pjdfstest harness hooked into GitHub Actions, the last unchecked box under F5 is reducing failures (chmod/chown/utimens) and updating `specs/Public/AgentFS/pjdfstest.baseline.json` accordingly.

## Useful Paths & Logs

- Control-plane harness (latest): `logs/fuse-control-plane-20251115-130217/control-plane.log`.
- Performance harness snapshots:
  - `logs/fuse-performance-20251116-120621/{performance.log,results.jsonl,summary.json}` – default env (seq_write 0.80×).
  - `logs/fuse-performance-20251116-120649/{…}` – rerun to confirm stability (ratios ~0.8×/1.0×/0.7×/2.0×).
  - `logs/fuse-performance-20251116-120715/{…}` – background=128, threads=8, max_write=8 MiB.
  - `logs/fuse-performance-20251116-122208/{…}` – sequential workload increased to 8 GiB with explicit cache drops so seq_read runs ~0.5 s instead of ~20 ms.
  - Historical runs: `logs/fuse-performance-20251116-110035/`, `…-110756/`, `…-110953/`, `…-111810/`, etc.
- Manual perf attaches: `logs/perf-profiles/agentfs-perf-profile-20251116-115242/` (base) and `…-115426/` (call stacks) plus prior traces in `logs/perf-profiles/agentfs-concurrent-*/`.
- pjdfstest full suite: `logs/pjdfstest-full-20251115-135821/{pjdfstest.log,summary.json}`.
- Other harnesses (Nov 15): `logs/fuse-mount-cycle-20251115-102831`, `logs/fuse-mount-failures-20251115-104618`, `logs/fuse-mount-concurrent-20251115-104626`, `logs/fuse-basic-ops-20251115-103631`, `logs/fuse-negative-ops-20251115-104635`, `logs/fuse-overlay-ops-20251115-104639`, `logs/pjdfs-subset-20251115-104653`.

## Commands Recap

- Control-plane smoke test: `just test-fuse-control-plane`
- Performance benchmarks: `just test-fuse-performance`
- pjdfstest full suite: `just test-pjdfstest-full`
- Manual CLI usage:
  - `target/debug/agentfs-control-cli --mount /tmp/agentfs-control-plane snapshot-list`
  - `target/debug/agentfs-control-cli --mount /tmp/agentfs-control-plane snapshot-create --name demo`
  - `target/debug/agentfs-control-cli --mount /tmp/agentfs-control-plane branch-create --snapshot <id> --name feature`
  - `target/debug/agentfs-control-cli --mount /tmp/agentfs-control-plane branch-bind --branch <id> --pid $$`
- Other harnesses:
  - `just test-fuse-basic-ops`
  - `just test-fuse-negative-ops`
  - `just test-fuse-overlay-ops`
  - `sudo -E just test-pjdfs-subset /tmp/agentfs`
  - Mount helpers: `AGENTFS_FUSE_ALLOW_OTHER=1 just mount-fuse /tmp/agentfs`, `just umount-fuse /tmp/agentfs`
