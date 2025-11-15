# FUSE Adapter Control-Plane Status – Nov 15 2025 (handoff)

## Current Scope

- Working branch: `feat/agentfs-fuse-f4`.
- Focus: **F4 – Control Plane Integration Testing**. We need automated coverage that opens `<MOUNT>/.agentfs/control`, issues snapshot/branch/bind IOCTLs, and proves per-process binding isolation on a live mount.
- Tooling targets: `just test-fuse-control-plane` (new); also keep the F1/F3 harnesses runnable (`test-fuse-basic-ops`, `test-fuse-negative-ops`, `test-fuse-overlay-ops`, `sudo -E just test-pjdfs-subset /tmp/agentfs`).

## Current State

- Fresh control-plane harness (`scripts/test-fuse-control-plane.sh`) now green. Latest run: `logs/fuse-control-plane-20251115-113550`. It mounts `/tmp/agentfs-control-plane`, hits `.agentfs/control`, creates a snapshot + branch, binds the current shell to that branch via IOCTL, and logs all interactions.
- `agentfs-control-cli` (new crate) is the helper binary that speaks the SSZ control protocol from the CLI. It’s invoked by both the harness and can be used manually (`target/debug/agentfs-control-cli --mount /tmp/agentfs snapshot-list` etc.).
- IOCTL framing changed: both requests and responses now carry a 4-byte little-endian length prefix so generic buffers (4 KiB) can safely decode replies. Adapter and AH CLI transport updated accordingly.
- Regression check: after these changes, all existing FUSE harnesses still pass (`test-fuse-mount-cycle`, `test-fuse-mount-failures`, `test-fuse-mount-concurrent`, `test-fuse-basic-ops`, `test-fuse-negative-ops`, `test-fuse-overlay-ops`, `sudo -E just test-pjdfs-subset /tmp/agentfs`). Logs for this rerun:
  - `logs/fuse-mount-cycle-20251115-104613`
  - `logs/fuse-mount-failures-20251115-104618`
  - `logs/fuse-mount-concurrent-20251115-104626`
  - `logs/fuse-basic-ops-20251115-103631`
  - `logs/fuse-negative-ops-20251115-104635`
  - `logs/fuse-overlay-ops-20251115-104639`
  - `logs/pjdfs-subset-20251115-104653`

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

1. **Control-plane coverage expansion**
   - Harness currently exercises snapshot create/list and a single branch bind. Next steps: add negative cases (bind invalid branch, snapshot list after unmount/remount) and cover branch isolation by actually reading/writing files under different PIDs (requires reintroducing a worker flow once the simpler smoke test is solid).
2. **Hook into CI**
   - Wire `just test-fuse-control-plane` into the same pipeline that already runs the mount/negative/overlay harnesses so control-plane regressions get caught automatically.
3. **Manual CLI verification**
   - Use `target/debug/agentfs-control-cli --mount /tmp/agentfs-control-plane snapshot-list` etc. to manually inspect control state while reproducing issues.

## Useful Paths & Logs

- Control-plane harness log (latest): `logs/fuse-control-plane-20251115-113550/control-plane.log`
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
