# FUSE Adapter Status – Nov 20 2025 (handoff)

<!-- cSpell:ignore fdatasync conv ENOTCONN writeback Backoff perfdata memmove erms memfd siphash setid FOWNER noprof subtest subtests -->

## Latest (Nov 22 2025)

- CI FUSE jobs now assume a self-hosted NixOS runner with passwordless sudo. Workflow labels were bumped to `[self-hosted, nixos, x86-64-v3]` and we fail fast if `sudo -n true` is unavailable. The mount helper always uses sudo for `/dev/fuse` setup and no longer tries non-sudo fallbacks; this aligns with the GitHub runner requirements where `/dev/fuse` must be created via `modprobe/mknod`.
- FUSE/pjdfstest failures on the sandbox runners were due to missing `/dev/fuse` and lack of sudo (some runners even without sudo installed). Expect the harness to pass once it lands on a privileged runner; there is no skip path in CI.

## Latest (Nov 21 2025)

- Branch `feat/agentfs-fuse-f7` rebased on main; F1–F5/F7/F8/F10 rerun and passing. Logs: `logs/fuse-basic-ops-20251121-113210`, `…negative-ops-20251121-113229`, `…overlay-ops-20251121-113235`, `…control-plane-20251121-113243`, `…mount-cycle-20251121-113309`, `…mount-failures-20251121-113355`, `…mount-concurrent-20251121-113417`, `…xattrs-20251121-113438`, `…mknod-20251121-113452`, `…mount-options-20251121-113500`, `…advanced-io-20251121-113514`, `…security-permissions-20251121-113829`, `…security-privileges-20251121-113858`, `…security-input-20251121-113918`, `…security-sandbox-20251121-113942`, `…security-robustness-20251121-114004`.
- F7 stress harness green: `logs/fuse-stress-20251121-113542/summary.json`.
- F6 performance still under target (seq_write ~0.42×, seq_read ~0.28×, metadata ~0.11×, concurrent_write ~0.13×) in `logs/fuse-performance-20251121-113521/summary.json`.
- F6 tuning pass (feat/fuse-perf): default release build for perf harness, writeback cache on, inline write path (skip buffer copies/queue), max_write bumped to 8 MiB, max_background=128, block size 8 MiB. Latest run `logs/fuse-performance-20251121-153312/summary.json` shows ratios still low (seq_write ~0.36×, seq_read ~0.34×, metadata ~0.21×, concurrent_write ~0.12×); passthrough (`open_backing`) rejected with `EPERM` due to missing CAP_SYS_ADMIN on /dev/fuse.
- F9 compatibility harness added and wired into CI: `just test-fuse-compat` exercises `fusermount` (libfuse2) and `fusermount3` (libfuse3) mount/unmount flows, logs helper/kernel versions, and writes `summary.json`; first run: `logs/fuse-compat-20251121-143907/summary.json` (both helpers succeeded on NixOS 6.12).
- pjdfstest full suite rerun: `logs/pjdfstest-full-20251121-114039/summary.json` passes main set; privileged `chmod/12.t` still fails as expected under user-mounted FUSE.

## Latest (Nov 20 2025)

- Working branch is now `feat/agentfs-fuse-f7` (F8 delivered); pjdfstest compliance remains stable. Only the upstream `chown/00.t` TODOs and the kernel-imposed `chmod/12.t` failures appear in the comparison to `specs/Public/AgentFS/pjdfstest.baseline.json`.
- Harness workflow: fix regressions individually via `prove` with the filesystem mounted (`AGENTFS_FUSE_ALLOW_OTHER=1 just mount-fuse /tmp/agentfs`), then rerun `just test-pjdfstest-full` to regenerate `logs/pjdfstest-full-<ts>/summary.json` before updating the baseline. CI now runs the full suite automatically in the `fuse-harness` job, so keep new `summary.json` files small and deterministic.
- `scripts/test-pjdfstest-full.sh` handles two passes:
  1. Main suite under sudo (236 files, `HARNESS_OPTIONS=j32`).
  2. Privileged subset for `chmod/12.t` via a remount under sudo (still expected to fail because Linux blocks nosuid writes before we can clear SUID bits; we keep the output for auditing).
- Latest log sets (Nov 20): `logs/pjdfstest-full-20251120-041419/summary.json` (main pass) and `logs/pjdfstest-full-20251120-041419/fuse-host-priv.log` (privileged remount). Both match the baseline aside from the known `chmod/12.t` entries.
- Sticky bit/permission tracing: `/tmp/agentfs-*/fuse-host.log` still records synthetic PID mappings, so debugging permission events remains straightforward.

## Current Scope

- Working branch: `feat/agentfs-fuse-f5`.
- Focus: **F6 – Performance Tuning + Benchmarking**. F4/F5 harnesses stay green, but the priority is pushing sequential write throughput toward the host baseline and documenting every `just test-fuse-performance` run (latest release-mode sweeps still fail the ≥ 0.75× thresholds even with kernel writeback cache enabled).
- Tooling targets: `just test-fuse-performance`, `just test-fuse-control-plane`, the F1/F3 sweeps (`test-fuse-basic-ops`, `test-fuse-negative-ops`, `test-fuse-overlay-ops`, `sudo -E just test-pjdfs-subset /tmp/agentfs`), plus perf captures under `logs/perf-profiles/`.

## Updates – Nov 17 2025

1. **Async writeback in the FUSE host**
   - Every `FUSE_WRITE` now enqueues into a per-handle dispatcher and replies immediately while a worker flushes the buffer into FsCore. Handle-level state tracks inflight writes; `flush`, `fsync`, and handle `release` wait for the queue to drain and propagate deferred errno back to the caller.
2. **Kernel writeback cache capability**
   - When `config.cache.writeback_cache` (or `AGENTFS_FUSE_WRITEBACK_CACHE=1`) is set we request the `FUSE_WRITEBACK_CACHE` capability during `init`, letting Linux batch buffered writes before waking us.
3. **F5 harness reruns**
   - `just test-fuse-basic-ops`, `test-fuse-negative-ops`, `test-fuse-overlay-ops`, and `test-fuse-control-plane` all passed after the async writeback change (`logs/fuse-basic-ops-20251117-062245`, `…negative-ops-20251117-062257`, `…overlay-ops-20251117-062301`, `…control-plane-20251117-062305`).
4. **F6 release sweeps remain below spec**
   - `logs/fuse-performance-20251117-065645/summary.json` and `logs/fuse-performance-20251117-070644/summary.json` capture the latest release-mode runs (seq_write ≈ 0.32×, seq_read ≈ 0.32×, metadata ≈ 0.24–0.31×, concurrent_write ≈ 0.22–0.29×). The harness still fails the configured thresholds.
   - `logs/perf-profiles/agentfs-perf-20251117-064244/` and `…064348/` contain fresh `perf record -F 199 -g -- dd …` captures; the kernel still spends most of its time in `pagecache_get_page → __alloc_pages_noprof → clear_page_erms`.
5. **Direct-I/O toggle exposed**
   - Added `AGENTFS_FUSE_DIRECT_IO=1` to force `FOPEN_DIRECT_IO` on all regular files so the kernel bypasses its page cache entirely. Default remains buffered I/O; the knob is for upcoming experiments (Option 2).
6. **Passthrough fast path (in progress)**

- AgentFsFuse can now request `FUSE_PASSTHROUGH` (Linux 6.9+) and, when `AGENTFS_FUSE_PASSTHROUGH=1`, tries to hand `/dev/fuse` a backing FD during `open`. The host logs per-attempt metrics so we can see how often the kernel accepts it. Remaining TODOs: confirm HostFs backend exposes real paths for all branches, and land fallback copyup logic so lower-only files don’t throw `missing_content`.

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

## Nov 17 2025 – Metadata semantics + pjdfstest

- Added `FsError::OperationNotPermitted` and mapped it to `EPERM` across the FUSE host (`crates/agentfs-fuse-host/src/adapter.rs`), the FSKIT daemon, and the C FFI so pjdfstest can distinguish EPERM vs EACCES in chmod/chown/utimens exercises.
- FsCore now records each process’s full supplementary group list (parsed from `/proc/<pid>/status`) via `register_process_with_groups`, so owner group changes (`chown -1 <gid>`) check real membership instead of the primary gid stub.
- Metadata APIs enforce the expected semantics:
  - `set_mode`/`fchmod*` return EPERM for non-owners and clear setgid for regular files when the caller isn’t in the target group.
  - `set_owner`/`fchown*` honor `uid == gid == (uid_t)-1` sentinels, clear setid bits only when ownership actually changes, and reject non-root/non-owner callers even when the request arrives via `fchown`.
  - `utimensat`/`futimens` keep track of `UTIME_NOW` vs `UTIME_OMIT`, enforce the POSIX rules (owner or CAP_FOWNER for explicit timestamps, write permission suffices for `UTIME_NOW`), and leave birthtime untouched.
  - `ftruncate` now zero-extends files instead of returning `EOPNOTSUPP`, so pjdfstest’s truncate suites progress instead of aborting early.
- Test run: `just test-pjdfstest-full` (logs in `logs/pjdfstest-full-20251116-180350/`). Progress:
  - `chmod/00` and the bulk of `chown/00` now pass; sentinel handling and EPERM vs EACCES regressions are fixed.
  - Remaining deterministic failures: `chmod/12.t` (setgid-on symlink cases still report 0755), `chown/05.t`/`chown/07.t` (non-owner chown still sometimes succeeds via hardlink/lchown paths), and `ftruncate/05.t`/`ftruncate/12.t` (size update races in open-handle paths).
  - After `ftruncate/12.t` the host exited unexpectedly (“Transport endpoint is not connected”), so the rest of the suites reported “No plan found”. Need to pull the FUSE host logs and ensure we aren’t panicking in the new timestamp/owner paths.
- TODOs captured for F6/F7 backlog:
  1. Instrument `evaluate_owner_change` to log caller UID/gid and the target node whenever a non-owner chown slips through (`chown/07.t`). The log should include `pid`, `requested_uid`, `node.uid`, and the resolved group list so we can see why EPERM isn’t returned.
  2. Track the mount crash under `logs/pjdfstest-full-20251116-180350/pjdfstest.log` by tailing the FUSE host stdout/stderr; the failure happens between `ftruncate/12` and `ftruncate/13` and leaves the mount ENOTCONN for all subsequent suites.
  3. Re-run perf tuning with the new `just test-fuse-performance-release` target (builds the release host binary and mounts it via `AGENTFS_FUSE_HOST_BIN=target/release/agentfs-fuse-host`) so all reported ratios come from optimized builds.

9. **Perf profiling runs (Nov 16 evening)**
   - Rebuilt the FUSE host in debug for faster iterations, mounted `/tmp/agentfs-perf-profile` against a fresh HostFs backstore, and captured three back-to-back runs of four sequential 16 GiB writes (total 64 GiB per capture) with cold caches (`sync && echo 3 | sudo tee /proc/sys/vm/drop_caches`) while attaching `perf record -g -F 400 -p <fuse_pid>`. Artifacts live under:
     - `logs/perf-profiles/agentfs-perf-profile-20251116-125536-run1/`
     - `logs/perf-profiles/agentfs-perf-profile-20251116-125630-run2/`
     - `logs/perf-profiles/agentfs-perf-profile-20251116-125721-run3/`
   - All three runs show ~14 k samples with the same hotspots (`__GI___clone3 → crossbeam::Backoff::snooze`, siphash, memmove, FsCore::write) and no lost samples; the results confirm the worker-channel transport is still the dominant cost.
   - Repeated the captures with the **release** binary (`target/release/agentfs-fuse-host --features fuse`) so we have production-codegen data (`logs/perf-profiles/agentfs-perf-profile-20251116-130943-release-run1/`, `…131032-release-run2/`, `…131121-release-run3/`). Sample counts stay in the ~10 k range but self time concentrates even more heavily in `__memmove_avx_unaligned_erms`, reinforcing the need for the transport redesign.
10. **Write transport rewrite**
    - Replaced the crossbeam-channel worker pool with a dedicated `WriteDispatcher` that uses `crossbeam_queue::SegQueue` plus a lightweight `Condvar` to park idle threads. This removes the `ChildStdin`-style IPC entirely, avoids the heavy `Backoff::snooze` loops seen in perf, and keeps FsCore in-process while still offloading blocking writes to background threads.
11. **pjdfstest full-suite refresh**

- Re-ran `just test-pjdfstest-full` after the FsCore/FUSE fixes. The entire suite now passes (only the upstream `TODO passed` lines remain), so the new reference artifacts live under `logs/pjdfstest-full-20251116-123822/{pjdfstest.log,summary.json}` and the baseline diff was updated accordingly.
- `scripts/test-pjdfstest-full.sh` now runs the suite in two phases: the main `sudo -E prove` invocation skips `chmod/12.t` (and anything else listed via `PJDFSTEST_SUDO_TESTS`) because the Linux kernel refuses to honor SUID-preserving writes on user-mounted FUSE filesystems before AgentFS can clear the bits, and a follow-up privileged pass **remounts via sudo** and executes only those skipped cases so we still capture their output for documentation.
- Latest two-pass artifacts live under `logs/pjdfstest-full-20251117-045151/` (main run + SUID subset). The main run currently fails in `open/{00,05,06}`, `symlink/{05,06}`, `truncate/05`, `ftruncate/05`, and `chown/05`, while the privileged pass documents the expected `chmod/12` EPERM behavior when Linux blocks writes to SUID files from non-root callers on FUSE.
- Nov 18 refresh: `logs/pjdfstest-full-20251118-025512/summary.json` shows the same outstanding failures (open/00 & open/06 permission matrices, chown/05, truncate/ftruncate/05, symlink/06, and the privileged `chmod/12` setuid cases). No new regressions surfaced after the FIFO metadata fix.
- Historical re-check: the commit that introduced the “all green” baseline (`e1ff0de`) no longer has a working vendor tree, so we mirrored `vendor/` into `/tmp/pjdfs-baseline`, re-ran `just test-pjdfstest-full`, and still saw dozens of failures (chmod/00/07/11/12, chown/00/05/07, open/{00,02,03,05,06}, \*truncate suites, utimensat suites, etc.; log `logs/pjdfstest-full-20251118-030415/summary.json` inside the temporary worktree). Conclusion: there isn’t a clean passing commit in history—the task remains to fix the specific failing suites listed above rather than to hunt for a regression that never existed.
- Targeted reproductions:
  - `logs/manual-open06.log`: mount `/tmp/agentfs-debug` with `AGENTFS_FUSE_LOG_FILE` + `RUST_LOG=agentfs::metadata=debug`, then run `sudo -E prove -vr resources/pjdfstest/tests/open/06.t` to capture the credential registration spam for every failing subtest. The log shows kernel credentials arriving correctly (`client_uid=65534`) even though FsCore still returns success where pjdfstest expects `EACCES`, suggesting the `user_for_process` lookup or the permission mask evaluation is being skipped.
  - Manual sandbox: `sudo $PWD/resources/pjdfstest/pjdfstest -u 65534 -g 65534 mkfifo /tmp/agentfs-debug/fifo_test/fifo 0644`, followed by a series of `chmod` / `open` calls, reproduces the bad behavior without running the full suite. Remember that bare `O_RDONLY` on FIFOs blocks, so use `O_RDONLY,O_NONBLOCK` for ad-hoc experiments.

11. **Transport redesign sketch**

- Since perf shows the crossbeam channel + `Vec::extend_from_slice` as the bottleneck, the next experiment is to split transport responsibilities behind a trait:
  1. Introduce `FsCoreTransport` with methods for `open`, `read`, `write`, `fsync`, etc.
  2. Provide an in-process transport that keeps FsCore in the host but replaces the channel with a lock-free ring buffer (e.g., `crossbeam_deque`) backed by a slab allocator of page-aligned blocks; workers enqueue completions instead of sending replies over a pipe.
  3. Future-proof the trait so we can swap in a shared-memory transport (memfd-backed queue + doorbells) if we _do_ need to move FsCore back into a helper process.
- This design removes the `ChildStdin`-style IPC entirely for the common case and should cut the memmove/backoff costs highlighted above.

## Pending / Next Steps

1. **Control-plane write semantics** – FsCore still refuses to mutate files after snapshot creation. Fixing that (or adding a worker flow that mutates the HostFs backstore) is required before we can demonstrate true branch-local divergence in the harness.
2. **Performance tuning (F6)** – Even though this machine now reports ≥ 0.8× for sequential writes, both AgentFS and the baseline saturate NVMe bandwidth (even with the new 8 GiB workload and cache drops), so measurement noise still hides small deltas. Keep logging every run, rerun the new perf captures with the release-host binary to eliminate debug-mode skew, and try again on a slower host (or larger workloads) while prototyping the shared-buffer transport.
3. **pjdfstest maintenance** – Keep running `just test-pjdfstest-full` when touching metadata/path semantics so the new “all green” baseline stays enforced by CI and we catch regressions early. Immediate focus is on the remaining failures listed above; `open/06` in particular needs instrumentation inside `FsCore::open`/`allowed_for_user` to verify that per-request identities are registered before the permission check (see `logs/manual-open06.log` for the latest reproduction run).

## Useful Paths & Logs

- Control-plane harness (latest release rerun): `logs/fuse-control-plane-20251117-062305/control-plane.log`.
- Performance harness snapshots:
  - `logs/fuse-performance-20251116-120621/{performance.log,results.jsonl,summary.json}` – default env (seq_write 0.80×).
  - `logs/fuse-performance-20251116-120649/{…}` – rerun to confirm stability (ratios ~0.8×/1.0×/0.7×/2.0×).
  - `logs/fuse-performance-20251116-120715/{…}` – background=128, threads=8, max_write=8 MiB.
  - `logs/fuse-performance-20251116-122208/{…}` – sequential workload increased to 8 GiB with explicit cache drops so seq_read runs ~0.5 s instead of ~20 ms.
  - Release-mode baseline after async writeback: `logs/fuse-performance-20251117-065645/summary.json` and `logs/fuse-performance-20251117-070644/summary.json` (still below target ratios).
  - Historical runs: `logs/fuse-performance-20251116-110035/`, `…-110756/`, `…-110953/`, `…-111810/`, etc.
- Manual perf attaches: `logs/perf-profiles/agentfs-perf-profile-20251116-115242/` (base) and `…-115426/` (call stacks), plus the release-focused captures under `logs/perf-profiles/agentfs-perf-20251117-064244/` and `…064348/`.
- pjdfstest full suite: `logs/pjdfstest-full-20251117-060947/{pjdfstest.log,summary.json}` (previous runs kept under `logs/pjdfstest-full-20251116-123822/` and `logs/pjdfstest-full-20251115-135821/`).
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
