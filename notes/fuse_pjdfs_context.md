# FUSE Adapter Control-Plane Status – Nov 15 2025 (handoff)

## Current Scope

- Working branch: `feat/agentfs-fuse-f4`.
- Focus: **F4 – Control Plane Integration Testing**. We need automated coverage that opens `<MOUNT>/.agentfs/control`, issues snapshot/branch/bind IOCTLs, and proves per-process binding isolation on a live mount.
- Tooling targets: `just test-fuse-control-plane` (new); also keep the F1/F3 harnesses runnable (`test-fuse-basic-ops`, `test-fuse-negative-ops`, `test-fuse-overlay-ops`, `sudo -E just test-pjdfs-subset /tmp/agentfs`).

## Current State

- Control-plane harness now exercises the entire happy path plus the negative cases we identified. Latest log: `logs/fuse-control-plane-20251115-130217/control-plane.log`. Each run builds the FUSE host + control CLI, mounts with a HostFs backstore config, validates `.agentfs/control` access, creates a snapshot + branch, deliberately rejects an invalid branch ID, binds two independent PIDs to the same branch, and proves the default PID can continue reading while the branch-bound readers stay on the snapshot view. We also added an explicit unmount/remount cycle that asserts `snapshot-list` fails while unmounted and recovers afterwards (persistence still TODO—see below).
- `agentfs-control-cli` remains the helper binary for direct SSZ IOCTLs. The harness now respects `SKIP_FUSE_BUILD`/`SKIP_CONTROL_CLI_BUILD` so CI can build the binaries once and reuse them.
- IOCTL framing (length-prefixed request/response) is unchanged; both the CLI and AH client still share the transport.
- Regression sweep: mount-cycle, mount-failures, mount-concurrent, basic-ops, negative-ops, overlay-ops, and the pjdfstest subset are still green (see the Nov 15 logs under `logs/` listed below).
- `scripts/test-pjdfstest-full.sh` + `just test-pjdfstest-full` now automate the **entire** pjdfstest suite: they set up the resources, build/mount with `--allow-other`, stream the full `prove -vr` output into `logs/pjdfstest-full-<ts>/pjdfstest.log`, emit a machine-readable `summary.json`, and compare the results against `specs/Public/AgentFS/pjdfstest.baseline.json`. Latest run (still FAIL because of chmod/chown/utimens gaps, but matching the baseline) lives under `logs/pjdfstest-full-20251115-135821/`.

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

## Useful Paths & Logs

- Control-plane harness log (latest): `logs/fuse-control-plane-20251115-130217/control-plane.log`
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
