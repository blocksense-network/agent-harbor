# F10 – Security & Robustness Plan

## Goals

Track deliverables for the FUSE F10 milestone (`specs/Public/AgentFS/FUSE.status.md`), focused on privilege boundaries, input validation, permission matrix coverage, sandbox escapes, and resilience under resource pressure.

## Current State Recap

- F1–F8 harnesses are green in CI (see `logs/fuse-*-20251120-*`), with mount scripts auto-chowning mountpoints.
- `agentfs-core` permission paths live in `crates/agentfs-core/src/vfs.rs`; FUSE adapter entry points are in `crates/agentfs-fuse-host/src/adapter.rs`.
- Mount helper defaults to `--allow-other` unless `AGENTFS_FUSE_ALLOW_OTHER=0`; custom configs are injected via `AGENTFS_FUSE_CONFIG`.

## Harness Strategy

| Track | New Script                                  | Scope                                                                                                                                                                                                                       |
| ----- | ------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| T10.1 | `scripts/test-fuse-security-privileges.sh`  | Negative attempts via `sudo -u nobody` for read/write/rename/delete in sticky dirs; toggles `root_bypass_permissions` on/off through injected config; asserts sticky bit + ownership rules.                                 |
| T10.2 | `scripts/test-fuse-security-input.sh`       | Malicious path handling: `../../` traversal, overlong/invalid UTF-8 names, reserved characters. Ensures FsCore rejects inputs gracefully and adapter returns stable errno (no panics).                                      |
| T10.3 | `scripts/test-fuse-security-permissions.sh` | Permission matrix table `{owner, same-group, other} × {read, write, exec, rename, chmod/chown}` under both `default_permissions` and core-enforced modes; exercises sticky directories and `root_bypass_permissions=false`. |
| T10.4 | `scripts/test-fuse-security-sandbox.sh`     | Boundary escapes: crafted symlinks/reparse to `/`, `/etc/passwd`, and outside backstore; validates adapter refusal and ensures `AGENTFS_FUSE_ALLOW_OTHER=1` does not open cross-mount access.                               |
| T10.5 | `scripts/test-fuse-security-robustness.sh`  | Resource-focused robustness (low `RLIMIT_NOFILE`, small backstore, ENOSPC propagation) extending stress harness; logs error surfaces and ensures no panics.                                                                 |

Shared conventions:

- All harnesses build once (unless `SKIP_FUSE_BUILD=1`) and mount to `/tmp/agentfs-<track>-<ts>` backed by `HostFs`.
- Each run writes `logs/fuse-<track>-<ts>/` with `summary.json` plus per-phase `.log` files; keep summaries small for CI.
- Config injection: prefer small JSON configs under the run dir, flipping `security.enforce_posix_permissions`, `root_bypass_permissions`, and cache TTLs as needed.

## CI Integration

- Add `just test-fuse-security-*` targets for every harness.
- Wire new targets into the `fuse-harness` job in `.github/workflows/ci.yml` after the F8 block; performance remains `continue-on-error`.

## Next Steps

1. Land permission matrix harness (T10.3) and wire it into `just` + CI.
2. Implement privilege-escalation harness (T10.1) with config toggle for `root_bypass_permissions`.
3. Add input validation + sandbox escape harnesses (T10.2/T10.4), reusing negative-ops primitives where possible.
4. Extend stress coverage for robustness (T10.5) and update `notes/fuse_stress_plan.md` if new scenarios are added.
5. Update `specs/Public/AgentFS/FUSE.status.md` with passing log references as each track turns green.

## Verification Snapshot – 2025-11-20

- `just test-fuse-security-permissions` ⇒ `logs/fuse-security-permissions-20251120-053931/summary.json`
- `just test-fuse-security-privileges` ⇒ `logs/fuse-security-privileges-20251120-053944/summary.json`
- `just test-fuse-security-input` ⇒ `logs/fuse-security-input-20251120-053953/summary.json`
- `just test-fuse-security-sandbox` ⇒ `logs/fuse-security-sandbox-20251120-054314/summary.json`
- `just test-fuse-security-robustness` ⇒ `logs/fuse-security-robustness-20251120-054421/summary.json`
