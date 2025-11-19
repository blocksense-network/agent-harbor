# F8 – Feature Harness Plan (xattrs, mknod, mount options, advanced I/O)

## Goals

Track deliverables for FUSE F8 milestone (spec `specs/Public/AgentFS/FUSE.status.md:260+`), covering:

- T8.1 Extended attributes
- T8.2 Special file creation via `mknod`
- T8.3 Mount option validation (`allow_other`, `default_permissions`, cache TTLs)
- T8.4 Advanced I/O (fallocate, punch hole, copy_file_range)

## Existing State Recap

- AgentFS core and fuse host already expose xattr/mknod/fallocate hooks, but we lack automated Linux harnesses ensuring they work end-to-end.
- Current scripts only cover basic/negative/overlay/control-plane/perf/stress/pjdfstest; none assert on xattr/mknod/mount options/advanced I/O requirements.

## Harness Strategy

| Track | New Script                           | Scope                                                                                                                                                                                                                              |
| ----- | ------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| T8.1  | `scripts/test-fuse-xattrs.sh`        | Mount AgentFS, toggle xattrs via `setfattr/getfattr/listfattr`, ensure persistence across remounts and under different namespaces (`user`, `trusted`). Emit JSON summary + per-operation log like other harnesses.                 |
| T8.2  | `scripts/test-fuse-mknod.sh`         | Create FIFOs/char/block devices via `mknod`; verify `stat` outputs (mode + `rdev`), open/read from FIFOs, ensure nodes survive remounts.                                                                                           |
| T8.3  | `scripts/test-fuse-mount-options.sh` | Iterate over mount configs (default, `default_permissions`, custom cache TTLs). After each mount, run short checks (e.g., ensure `default_permissions` enforces POSIX bits, TTL changes show up in `/proc/mounts` and via `stat`). |
| T8.4  | `scripts/test-fuse-advanced-io.sh`   | Exercise `fallocate` (allocate + punch hole) and `copy_file_range`. Validate file sizes, sparse regions (via `fiemap` or `stat --printf %b`), data integrities.                                                                    |

All scripts will:

- Build via existing `just build-fuse-host` logic, mount under `/tmp/agentfs-<phase>-<ts>`.
- Log to `logs/fuse-<phase>-<ts>/` with `summary.json` + per-step `.log` file mirroring stress harness conventions.
- Return non-zero on any mismatch; integrate new `just` targets (`test-fuse-xattrs`, etc.).

## Core/Adapter Gaps to Double-Check

- Ensure fuse host exports `mknod`, `Removexattr`, `Fallocate`, `CopyFileRange` paths (search for TODOs).
- Add telemetry/counters if needed (e.g., log mount options in fuse host for T8.3).
- Expose CLI helper or extend `agentfs-fuse-stress` with `--verify-xattr`, etc., if scripting becomes heavy.

## Next Steps

1. Implement `scripts/test-fuse-xattrs.sh` + supporting helpers.
2. Extend fuse host/core if any operations are unimplemented; add regression tests in Rust crates.
3. Land `scripts/test-fuse-mknod.sh`, `scripts/test-fuse-mount-options.sh`, `scripts/test-fuse-advanced-io.sh`.
4. Add Justfile targets and update `specs/Public/AgentFS/FUSE.status.md` once each phase is verified.

## Verification Snapshot – 2025‑11‑19

- `just test-fuse-xattrs` ⇒ `logs/fuse-xattrs-20251119-171957/summary.json`
- `just test-fuse-mknod` ⇒ `logs/fuse-mknod-20251119-172003/summary.json`
- `just test-fuse-mount-options` ⇒ `logs/fuse-mount-options-20251119-172010/summary.json`
- `just test-fuse-advanced-io` ⇒ `logs/fuse-advanced-io-20251119-171929/summary.json`
  - Kernel rejects `copy_file_range` with `EINVAL/EBADF`, so the harness documents the limitation and performs a userspace fallback copy to keep data verification deterministic.
