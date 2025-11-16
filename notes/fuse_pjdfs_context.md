# FUSE Adapter Control-Plane Status – Nov 15 2025 (handoff)

<!-- cSpell:ignore fdatasync conv ENOTCONN -->

## Current Scope

- Working branch: `feat/agentfs-fuse-f4`.
- Focus: **F4 – Control Plane Integration Testing**. We need automated coverage that opens `<MOUNT>/.agentfs/control`, issues snapshot/branch/bind IOCTLs, and proves per-process binding isolation on a live mount.
- Tooling targets: `just test-fuse-control-plane` (new); also keep the F1/F3 harnesses runnable (`test-fuse-basic-ops`, `test-fuse-negative-ops`, `test-fuse-overlay-ops`, `sudo -E just test-pjdfs-subset /tmp/agentfs`).

## Updates – Nov 16 2025

- HostFs backstore now exposes a real `sync_data`/`sync_all` path via `StorageBackend::sync`, and `FsCore::fsync` is wired all the way through the FUSE adapter. FUSE `F_SYNC`/`F_FSYNC` requests now flush the backing file instead of being acknowledged as a no-op, so the sequential `dd … conv=fdatasync` profile no longer leaves the mount in `ENOTCONN`. The HostFs write path also switched to `write_all` to stop partial-write truncation during large sequential writes. Re-run `scripts/test-fuse-performance.sh` and the manual `dd` profile in `logs/perf-profiles/` to confirm the host process stays alive before tightening performance thresholds.
- Added a lightweight write-latency tracer in the FUSE host (gate it with `AGENTFS_FUSE_TRACE_WRITES=1` and set `RUST_LOG=agentfs::write=debug`) so we can see how many write requests are in flight and how long each chunk spends in the host. Use this while rerunning the strace/perf captures to verify whether the kernel feeds us requests serially or whether FsCore/HostFs is the bottleneck.
- First trace with the new hooks (`logs/perf-profiles/agentfs-concurrent-20251116-095135/fuse-host.log`) shows `max_inflight` never exceeds 1, which means fuser is currently dispatching writes serially regardless of how many dd writers we spawn. 128 KiB chunks normally finish in ~0.5 ms but every few hundred ops the host spends 20–40 ms inside `write()`, so throughput is effectively capped around 4–5 MB/s.
- Added a background write worker pool (size controlled via `AGENTFS_FUSE_WRITE_THREADS`, defaulting to available CPUs) so the FUSE dispatcher can enqueue writes into FsCore without blocking the main kernel loop. Each worker grabs jobs from a crossbeam channel, performs the actual `FsCore::write`, and only replies to the kernel once the storage backend finishes so durability semantics stay intact. Tracing (`logs/perf-profiles/agentfs-concurrent-20251116-100230/fuse-host.log`) now shows `max_inflight` climbing to 3+ for multi-writer workloads, but sequential `dd conv=fdatasync` remains ~60 s because the kernel still feeds us 128 KiB chunks in a strictly ordered stream. Latest harness run `logs/fuse-performance-20251116-095946/summary.json` captures the unchanged throughput, so we’ll need to dig deeper into kernel queue limits (e.g., raising `writeback_cache`, `max_write`) or FsCore COW overheads for the next tuning pass.
- Follow-up tuning now requests larger write blocks/background queues during FUSE init: we honor `AGENTFS_FUSE_MAX_WRITE` (default 4 MiB), `AGENTFS_FUSE_MAX_BACKGROUND` (default 64), and derive a congestion threshold at ¾ of the background slots. With those knobs the kernel finally sends 4 MiB requests and sequential `dd … conv=fdatasync` regained host-level throughput (`logs/fuse-performance-20251116-100856/summary.json`). These values are overridable per host via env vars without touching the JSON config, and the tracing hooks still surface the true inflight depth.

## Current State

- Control-plane harness now exercises the entire happy path plus the negative cases we identified. Latest log: `logs/fuse-control-plane-20251115-130217/control-plane.log`. Each run builds the FUSE host + control CLI, mounts with a HostFs backstore config, validates `.agentfs/control` access, creates a snapshot + branch, deliberately rejects an invalid branch ID, binds two independent PIDs to the same branch, and proves the default PID can continue reading while the branch-bound readers stay on the snapshot view. We also added an explicit unmount/remount cycle that asserts `snapshot-list` fails while unmounted and recovers afterwards (persistence still TODO—see below).
- `agentfs-control-cli` remains the helper binary for direct SSZ IOCTLs. The harness now respects `SKIP_FUSE_BUILD`/`SKIP_CONTROL_CLI_BUILD` so CI can build the binaries once and reuse them.
- IOCTL framing (length-prefixed request/response) is unchanged; both the CLI and AH client still share the transport.
- Regression sweep: mount-cycle, mount-failures, mount-concurrent, basic-ops, negative-ops, overlay-ops, and the pjdfstest subset are still green (see the Nov 15 logs under `logs/` listed below).
- `scripts/test-pjdfstest-full.sh` + `just test-pjdfstest-full` now automate the **entire** pjdfstest suite: they set up the resources, build/mount with `--allow-other`, stream the full `prove -vr` output into `logs/pjdfstest-full-<ts>/pjdfstest.log`, emit a machine-readable `summary.json`, and compare the results against `specs/Public/AgentFS/pjdfstest.baseline.json`. Latest run (still FAIL because of chmod/chown/utimens gaps, but matching the baseline) lives under `logs/pjdfstest-full-20251115-135821/`.
- Performance harness (`scripts/test-fuse-performance.sh` via `just test-fuse-performance`) now mounts AgentFS with a HostFs backstore, runs sequential read/write, metadata, and 4-way concurrent write benchmarks on both AgentFS and a host baseline, and emits `results.jsonl` + `summary.json` under `logs/fuse-performance-<ts>/`. Latest run: `logs/fuse-performance-20251115-161415/`.

## What Changed This Session

1. **Control-plane CLI + harness**
   - Added `crates/agentfs-control-cli`: small Clap binary that opens `<mount>/.agentfs/control`, frames SSZ requests/responses, and prints snapshot/branch/bind results. Mirrors the AH CLI transport but without the rest of the agent stack.
   - Added `scripts/test-fuse-control-plane.sh` and wired it into the `Justfile` (`just test-fuse-control-plane`). Script builds the FUSE host + CLI, mounts `/tmp/agentfs-control-plane`, runs snapshot-create/list + branch-create/bind, and stores logs under `logs/fuse-control-plane-<timestamp>`.
2. **Adapter framing tweaks**
   - `AgentFsFuse::handle_control_ioctl` now expects the first 4 bytes of the ioctl buffer to encode payload length; responses are framed the same way to keep the CLI agnostic of actual response sizes.
   - AH CLI’s transport (used by `ah agent fs …`) now writes/reads the same framed format, so the new CLI and the existing one share the protocol.
3. **Documentation + status**
   - `specs/Public/AgentFS/FUSE.status.md` now marks T4.1–T4.4 as ✅ with a link to `logs/fuse-control-plane-20251115-113550`.
   - `notes/fuse_pjdfs_context.md` (this file) now reflects the F4 work instead of the old pjdfstest debugging notes.

## Pending / Next Steps

1. **Control-plane write semantics**
   - Negative samples + remount behaviour are covered, but FsCore still refuses to mutate files after snapshot creation. Fixing that (or adding a worker flow that mutates the HostFs backstore) is required before we can demonstrate true branch-local divergence in the harness.
2. **pjdfstest regression gating (F5)**
   - Baseline failures are captured in `specs/Public/AgentFS/pjdfstest.baseline.json`, and the harness now diffs `summary.json` against it. Next steps are (a) start tackling the chmod/chown/ftruncate/utimens issues and (b) keep expanding the baseline as fixes land so the diff stays meaningful.
3. **Hook pjdfstest-full into CI**
   - Add a CI job (similar to the FUSE harness job) that runs `just test-pjdfstest-full` on a privileged runner, archives `pjdfstest.log` + `summary.json`, and relies on the baseline diff for pass/fail.
4. **Performance tuning (F6)**
   - The initial benchmark shows AgentFS is still dramatically slower than the host baseline for sequential writes and concurrent workloads (see `logs/fuse-performance-20251116-060606/summary.json`). Strace profiling (`strace -ff -tt dd if=/dev/zero of=/tmp/agentfs-profile/perf/throughput.bin bs=4M count=64 status=none conv=fdatasync`) routinely ends with `Transport endpoint is not connected`, which means the fuse host crashes mid-write (`logs/perf-profiles/agentfs-dd.strace`). The host filesystem completes the same workload in ~30 ms (`logs/perf-profiles/baseline-dd.strace`). We need to debug why sequential writes wedge the HostFs backstore (fsync path?) before we can enforce regression thresholds.

## Useful Paths & Logs

- Control-plane harness log (latest): `logs/fuse-control-plane-20251115-130217/control-plane.log`
- Performance harness: `logs/fuse-performance-20251115-161415/{performance.log,results.jsonl,summary.json}`.
- pjdfstest full-suite harness: `logs/pjdfstest-full-20251115-135821/{pjdfstest.log,summary.json}` (many chmod/chown/ftruncate failures; baseline diff keeps the noise contained).
- Other harness logs (Nov 15 runs):
  - `logs/fuse-mount-cycle-20251115-102831`
  - `logs/fuse-mount-failures-20251115-104618`
  - `logs/fuse-mount-concurrent-20251115-104626`
  - `logs/fuse-basic-ops-20251115-103631`
  - `logs/fuse-negative-ops-20251115-104635`
  - `logs/fuse-overlay-ops-20251115-104639`
  - `logs/pjdfs-subset-20251115-104653`
- Source additions: `crates/agentfs-control-cli`, `scripts/test-fuse-control-plane.sh`, FUSE adapter framing changes.

## Commands Recap

- Control-plane smoke test: `just test-fuse-control-plane`
- Performance benchmarks (F6): `just test-fuse-performance`
- pjdfstest full suite: `just test-pjdfstest-full`
- Manual CLI usage:
  - `target/debug/agentfs-control-cli --mount /tmp/agentfs-control-plane snapshot-list`
  - `target/debug/agentfs-control-cli --mount /tmp/agentfs-control-plane snapshot-create --name demo`
  - `target/debug/agentfs-control-cli --mount /tmp/agentfs-control-plane branch-create --snapshot <id> --name feature`
  - `target/debug/agentfs-control-cli --mount /tmp/agentfs-control-plane branch-bind --branch <id> --pid $$`
- Other harnesses (unchanged):
  - `just test-fuse-basic-ops`
  - `just test-fuse-negative-ops`
  - `just test-fuse-overlay-ops`
  - `sudo -E just test-pjdfs-subset /tmp/agentfs`
  - Mount/unmount helpers: `AGENTFS_FUSE_ALLOW_OTHER=1 just mount-fuse /tmp/agentfs`, `just umount-fuse /tmp/agentfs`
