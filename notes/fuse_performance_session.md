<!-- cSpell:ignore setcap -->

# FUSE Performance Session – feat/fuse-perf (handoff)

## Goal

- Push F6 performance ratios above 0.5× baseline while keeping F1–F10 harnesses green.

## Current code state

- Branch: `feat/fuse-perf`.
- Tunings in tree:
  - `agentfs-fuse-host` defaults: `max_write=16 MiB`, `max_background=256`, inline write path env (`AGENTFS_FUSE_INLINE_WRITES`), direct HostFs I/O env (`AGENTFS_FUSE_HOSTFS_DIRECT`).
  - Perf harness (`scripts/test-fuse-performance.sh`): runs release host by default, writeback cache enabled, 8 MiB blocks, env exports for the above tunings; still fails thresholds locally.
  - Docs updated to reflect latest perf run and passthrough limitation.
- Latest perf run on this runner: `logs/fuse-performance-20251121-160145/summary.json` (seq_write ~0.34×, seq_read ~0.35×, metadata ~0.16×, concurrent_write ~0.12×). Passthrough attempts still hit `open_backing` → `EPERM`.
- All F1–F10 harnesses were rerun post-tuning and passed (basic/negative/overlay/control, mount cycle/fail/concurrent, stress, xattrs/mknod/mount-options/advanced-io, security suites).

## Blocker

- Passthrough cannot engage: kernel returns `EPERM` on `open_backing` (needs `CAP_SYS_ADMIN` on the /dev/fuse FD). Without passthrough, the FUSE copy path caps ratios ~0.34× even with direct HostFs I/O and inline writes.

## What was attempted for privileged mount

- Built release host: `FUSE_BUILD_PROFILE=release just build-fuse-host`.
- Prepared config (example at `/tmp/fuse-config.json`): HostFs backstore under `/tmp/agentfs-backstore`, writeback_cache=true, defaults uid=0/gid=0 (adjust if needed).
- Ensured `/dev/fuse`: `sudo modprobe fuse || true`; if missing, `sudo mknod -m 666 /dev/fuse c 10 229`.
- Launch attempt (no stdout; logs expected via `AGENTFS_FUSE_LOG_FILE`):

  ```
  LOG=/tmp/agentfs-fuse-$(date +%s).log
  sudo -E AGENTFS_FUSE_PASSTHROUGH=1 AGENTFS_FUSE_ALLOW_OTHER=1 \
    AGENTFS_FUSE_HOST_BIN=target/release/agentfs-fuse-host \
    AGENTFS_FUSE_LOG_FILE=$LOG RUST_LOG=agentfs::fuse=info \
    target/release/agentfs-fuse-host --config /tmp/fuse-config.json /tmp/agentfs
  ```

  - Process seen: root-owned `agentfs-fuse-host` (PID ~3818155 earlier), but no fresh `/tmp/agentfs-fuse-*.log` located; likely exited or logged elsewhere. Old `/tmp/agentfs-fuse.log` is from Nov 18 and irrelevant.
  - Mountpoint sometimes in “Transport endpoint is not connected” state; unmount with `sudo fusermount3 -u /tmp/agentfs` and remove `/tmp/agentfs*` to reset.

- To debug a launch: use strace to catch ENOENT/EPERM:
  ```
  sudo strace -f -o /tmp/agentfs-host.strace \
    AGENTFS_FUSE_PASSTHROUGH=1 AGENTFS_FUSE_ALLOW_OTHER=1 \
    AGENTFS_FUSE_LOG_FILE=$LOG RUST_LOG=agentfs::fuse=info \
    target/release/agentfs-fuse-host --config /tmp/fuse-config.json /tmp/agentfs
  tail -n 40 /tmp/agentfs-host.strace
  ```

## Next steps for the next developer

1. Get a _privileged_ mount where passthrough succeeds:
   - Ensure clean mountpoint/backstore, `/dev/fuse` present.
   - Launch host directly under sudo (or with `setcap cap_sys_admin+ep target/release/agentfs-fuse-host`) so CAP_SYS_ADMIN stays on the fuse FD.
   - Confirm mount via `mount | grep agentfs` and log(output) shows `passthrough` success (no `open_backing EPERM`).
2. Rerun perf harness with release host and passthrough:
   ```
   AGENTFS_FUSE_PASSTHROUGH=1 AGENTFS_FUSE_HOSTFS_DIRECT=1 \
   AGENTFS_FUSE_INLINE_WRITES=1 FUSE_BUILD_PROFILE=release \
   AGENTFS_FUSE_HOST_BIN=target/release/agentfs-fuse-host \
   just test-fuse-performance
   ```
   Capture `logs/fuse-performance-<ts>/summary.json`.
3. If >0.5× achieved, keep F1–F10 harnesses green (rerun the suites) and update status/notes. If still low, consider deeper transport changes (but likely the privileged passthrough is the key).

## Cleanup hints

- To reset a wedged mount: `sudo fusermount3 -u /tmp/agentfs 2>/dev/null || sudo umount -l /tmp/agentfs 2>/dev/null || true`.
- Remove stale dirs: `sudo rm -rf /tmp/agentfs /tmp/agentfs-backstore`.
- Logs of interest: `logs/fuse-performance-*/summary.json`, `/tmp/agentfs-fuse-*.log` (if produced), `logs/fuse-performance-*/*.log`.
