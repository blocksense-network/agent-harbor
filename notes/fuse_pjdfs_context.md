# FUSE Adapter / pjdfstest Status – Nov 15 2025 (handoff)

## Current Scope

- Branch `feat/agentfs-fuse-pjdfstest` rebased; we're 4 commits ahead, 74 behind `origin/main`.
- Goal: finish **F1** by getting the pjdfstest unlink/rename/mkdir/rmdir subsets green on the FUSE mount at `/tmp/agentfs`.
- Tooling: `AGENTFS_FUSE_ALLOW_OTHER=1 just mount-fuse /tmp/agentfs`, run subsets with `sudo -E just test-pjdfs-subset /tmp/agentfs`, unmount via `just umount-fuse /tmp/agentfs`.

## Current State

- `cargo test -p agentfs-core` is green (91 tests) after latest sticky enforcement changes.
- pjdfstest unlink suite passes. Rename suite still fails on tests 2266–2301 in `rename/09.t` (sticky directories + cross-parent moves) even after remounting with the new binary. Latest failure logs: `logs/pjdfs-subset-20251115-044909/rename.log`, `/tmp/agentfs-fuse-rename.log`, `/tmp/agentfs-sticky.log`.
- Sticky log now records both per-directory unlink checks and explicit `allow_cross_parent`/`deny_cross_parent` entries for cross-parent directory moves, but the failing cases still show the FUSE layer returning success for prohibited renames.

## What Changed This Session

1. **Sticky rename enforcement**
   - Strengthened `check_dir_cross_parent_permissions`: still requires the caller to own directories during cross-parent renames, and now always logs `allow_cross_parent` vs `deny_cross_parent` in `/tmp/agentfs-sticky.log`. Only uid 0 bypasses via `root_bypass_permissions`.
   - Added helper `test_core_posix_with_root_bypass` plus new unit test `test_sticky_directory_blocks_cross_parent_dir_move_with_root_bypass_enabled` to pin behaviour when root bypass is enabled (mirrors the FUSE host config).
   - Extra logging ensures we can tie each cross-parent attempt back to inode ownership with absolute paths.
2. **Tests**
   - `cargo test -p agentfs-core` now runs 91 tests (all green).
   - Rebuilt `agentfs-fuse-host`, remounted `/tmp/agentfs`, and re-ran `sudo -E just test-pjdfs-subset /tmp/agentfs`; rename/09 failures persist (see `logs/pjdfs-subset-20251115-044909`).

## Pending / Next Steps

1. **Root cause remaining rename/09 failures**
   - Even after remounting, pjdfstest can cross-parent move root-owned directories out of sticky parents as uid 65534; `check_dir_cross_parent_permissions` apparently isn’t firing (no `deny_cross_parent` entries for these paths). Need to trace control flow in `FsCore::rename` to ensure we still call the helper before deleting the destination, and confirm the source node we hand into the check retains the original uid/gid after earlier successful renames.
   - Investigate whether the directory ownership mutates mid-test (pjdfstest expects uid/gid to remain 0/0). Compare inode ownership stored in `self.nodes` vs what pjdfstest expects (see failed assertions expecting inode 485 owned by root). Maybe the user-level `chown` inside pjdfstest actually succeeded because FUSE host forced root bypass? Need to gather per-step inode metadata via debug instrumentation or `agentfs-core` tracing.
   - Confirm whether `root_bypass_permissions` being enabled inside the FUSE adapter is inadvertently short-circuiting permission checks before they reach `agentfs-core`. (Adapter always registers processes with their real uid/gid, but the config toggles root bypass globally.)
2. **Re-run pjdfstest after adjustments**
   - Each iteration: user mounts via `AGENTFS_FUSE_ALLOW_OTHER=1 just mount-fuse /tmp/agentfs`, then `sudo -E just test-pjdfs-subset /tmp/agentfs`, inspect `/tmp/agentfs-sticky.log` and `/tmp/agentfs-fuse-rename.log`.
   - Archive each failing run under `logs/pjdfs-subset-<timestamp>` for reference.
3. **Rebase/cleanup**
   - Once rename suite is green, rebase onto latest `origin/main`, rerun `cargo fmt`, `cargo test -p agentfs-core`, pjdfstest subset.
   - Update `specs/Public/AgentFS/FUSE.status.md` and README/notes when F1 criteria are met.

## Useful Paths & Logs

- Latest logs: `logs/pjdfs-subset-20251115-044909/rename.log`, `/tmp/agentfs-fuse-rename.log`, `/tmp/agentfs-sticky.log`.
- Older references: `logs/pjdfs-subset-20251114-163136`, `logs/pjdfs-subset-20251114-170151`, etc.
- Sticky path examples: `/pjdfstest_2340aa526a954f8114fc4535596ce2aa/pjdfstest_9dd361e62c47469c752cd6f3d0f97c0c`.

## Commands Recap

- Mount: `AGENTFS_FUSE_ALLOW_OTHER=1 just mount-fuse /tmp/agentfs`
- Run subset: `sudo -E just test-pjdfs-subset /tmp/agentfs`
- Unmount: `just umount-fuse /tmp/agentfs`
- Inspect sticky log: `sudo tail -n 100 /tmp/agentfs-sticky.log`
- Inspect rename log: `sudo tail -n 100 /tmp/agentfs-fuse-rename.log`
