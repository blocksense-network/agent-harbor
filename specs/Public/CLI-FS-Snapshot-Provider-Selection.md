## CLI Filesystem Snapshot Provider Selection

### Purpose

Defines how the AH CLI selects a filesystem snapshot provider for a repository/workspace path. This document consolidates the logic implemented in the `ah-fs-snapshots` crates for reference across CLI commands.

### Scope

Applies to commands that need to determine the snapshot provider for a path (e.g., `ah agent fs status`, `ah agent fs init-session`, sandbox preparation, and time‑travel operations).

### Inputs

- Target path (typically repository root)
- Platform capabilities (feature flags for providers)

### Providers (supported)

- ZFS provider (feature: `zfs`)
- Btrfs provider (feature: `btrfs`)
- AgentFS provider (future integration, when applicable)
- Git provider (shadow commits / in‑place capture semantics)

### Algorithm

1. Validate destination path

- Reject system directories (`/dev`, `/proc`, `/sys`, `/run`) and root (`/`).
- Ensure parent directories are creatable when preparing workspaces; fail early with descriptive errors.

2. Capability detection per provider

- For each enabled provider, call `detect_capabilities(path)` which returns:
  - `kind`: provider kind (Zfs|Btrfs|AgentFs|Git)
  - `score`: numeric score indicating suitability
  - `supports_cow_overlay`: whether CoW overlays are available
  - `provider_data`: provider-specific properties that were extracted from the detection procedure

3. Selection

- Choose the provider with the highest capability `score` among supported providers (ZFS, then Btrfs, then AgentFS, then Git when applicable). If no provider reports capabilities, return an error: "No suitable provider found".

4. Working copy strategy

- The selected provider integrates with the requested working copy mode (`auto|cow-overlay|worktree|in-place`). Providers may map unsupported modes to supported ones (e.g., in‑place only). The CLI records the resolved `working_copy` and the selected `provider`.

### Error semantics

- Provider detection or selection failures MUST emit actionable errors (e.g., permission issues on mount points, missing datasets/subvolumes, unsupported filesystem). JSON output in CLI should include an `error` field.

### Notes

- Snapshots are provider‑authoritative. The CLI does not duplicate snapshot metadata in SQLite; minimal references may be emitted as session events (see [State Persistence](State-Persistence.md)).
- Provider selection MUST NOT fall back to a copy‑based approach.

### References

- See implementation in `crates/ah-fs-snapshots/` and provider crates (`ah-fs-snapshots-zfs`, `ah-fs-snapshots-btrfs`).
- See CLI behavior in `ah agent fs status` and `ah agent fs init-session` sections of [CLI.md](CLI.md).
