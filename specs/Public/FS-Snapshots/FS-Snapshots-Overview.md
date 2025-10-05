This document specifies the filesystem snapshot abstraction used by Agent Harbor (Rust rewrite) and the provider matrix that implements it across platforms. It supersedes the legacy Ruby‑era overview while preserving the original objectives and context.

## Objectives & Context

- Isolated per‑agent workspaces: each agent runs against an independent working copy to avoid cross‑contamination and enable parallelism.
- Time‑Travel snapshots: capture point‑in‑time filesystem state aligned to SessionMoments so users can seek, inspect, and branch.
- Performance via CoW: prefer native CoW snapshots (ZFS, Btrfs) and lightweight user‑space snapshots (AgentFS). Git‑based snapshots provide a ubiquitous fallback with excellent metadata ergonomics.
- Path preservation: when feasible, mount the agent’s isolated view under the same repo path so incremental builds remain fast and absolute paths remain valid.
- Cross‑platform delivery: unify Linux, macOS, and Windows by selecting the best provider automatically while allowing explicit control.
- Local and remote parity: the same abstractions apply to local machines, devcontainers/VMs, and remote hosts accessed over SSH.

Adaptations vs the legacy plan:

- We drop OverlayFS and direct file‑copy strategies in favor of Git‑based snapshots on platforms/filesystems without CoW support. This simplifies the matrix and keeps behavior consistent.
- AgentFS provides per‑process branch isolation and path‑preserving mounts on macOS/Windows; on Linux we use ZFS/Btrfs for CoW and namespaces/binds for path preservation.

## Abstractions

### Rust Trait: FsSnapshotProvider

The CLI/core selects one concrete provider per session based on config and capability detection. Providers implement the following Rust trait (signatures shown for specification purposes):

```rust
use std::path::{Path, PathBuf};
use std::collections::HashMap;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SnapshotProviderKind { Auto, Zfs, Btrfs, AgentFs, Git }

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkingCopyMode { Auto, CowOverlay, Worktree, InPlace }

#[derive(Clone, Debug)]
pub struct ProviderCapabilities {
    pub kind: SnapshotProviderKind,
    pub score: u8,                // higher is better (0..=100)
    pub supports_cow_overlay: bool, // supports path‑stable CoW view under original repo path
    pub notes: Vec<String>,       // detection notes shown in diagnostics
}

#[derive(Clone, Debug)]
pub struct PreparedWorkspace {
    pub exec_path: PathBuf,       // path where agent processes should run
    pub working_copy: WorkingCopyMode,
    pub provider: SnapshotProviderKind,
    pub cleanup_token: String,    // opaque handle for idempotent teardown across crashes/process boundaries (see rationale below)
}

#[derive(Clone, Debug)]
pub struct SnapshotRef {
    pub id: String,               // provider‑opaque snapshot id/ref
    pub label: Option<String>,
    pub provider: SnapshotProviderKind,
    pub meta: HashMap<String, String>,
}

pub trait FsSnapshotProvider: Send + Sync {
    fn kind(&self) -> SnapshotProviderKind;

    // Capability detection for current host/repo
    fn detect_capabilities(&self, repo: &Path) -> ProviderCapabilities;

    // Create a session workspace (independent or in‑place) for the selected working‑copy mode
    fn prepare_writable_workspace(
        &self,
        repo: &Path,
        mode: WorkingCopyMode,
    ) -> anyhow::Result<PreparedWorkspace>;

    // Snapshot current workspace state; label is optional UI hint
    fn snapshot_now(&self, ws: &PreparedWorkspace, label: Option<&str>) -> anyhow::Result<SnapshotRef>;

    // Read‑only inspection mount for a snapshot (optional)
    fn mount_readonly(&self, snap: &SnapshotRef) -> anyhow::Result<PathBuf>;

    // Create a new writable workspace (branch) from a snapshot
    fn branch_from_snapshot(
        &self,
        snap: &SnapshotRef,
        mode: WorkingCopyMode,
    ) -> anyhow::Result<PreparedWorkspace>;

    // Cleanup/destroy any resources created by this provider (workspaces, mounts)
    fn cleanup(&self, token: &str) -> anyhow::Result<()>;
}
```

Notes:

- Cow‑overlay mode allows executing the agent under the original repo path while keeping isolation (copy‑on‑write semantics). On Linux this is implemented via mount namespaces and bind mounts to a CoW clone; on macOS/Windows via AgentFS adapters (FSKit/WinFsp). When not feasible, `Worktree` runs from a separate directory. `InPlace` executes directly in the user’s working copy with no isolation.
- The trait abstracts both “creating an independently writable working copy” and “recording snapshots/branches,” covering AgentFS’s responsibilities as well as native CoW filesystems and Git‑based snapshots.

Why `cleanup_token`?

- Providers often create multiple ephemeral resources (e.g., ZFS clones, Btrfs subvolumes, AgentFS branches, temporary worktrees, temp directories, per‑process namespace bindings). A single opaque `cleanup_token` lets the orchestrator record one stable handle in the session state and later perform idempotent teardown even if the original process crashed or the workspace path moved.
- The token may encode multiple resource identifiers or reference a provider‑managed lease record. It avoids relying solely on filesystem paths or PIDs and supports remote/VM scenarios where control must be delegated to a helper.
- Recording this token alongside the session in the state DB enables robust leak detection and background garbage collection.

### Provider Matrix

- ZFS snapshots/clones
- Btrfs subvolume snapshots
- AgentFS (user‑space CoW FS with branches; see specs/Public/AgentFS/\*.md)
- Git‑based snapshots (see Git‑based Snapshots doc)

Providers expose the same operations via the trait; the Workspace Manager and Agent Time‑Travel are provider‑agnostic.

## Configuration

Two top‑level options select snapshotting and working‑copy strategies. Both default to `auto`.

- fs-snapshots: `auto|zfs|btrfs|agentfs|git|disable`
- working-copy: `auto|cow-overlay|worktree|in-place`

Mapping:

- CLI flags: `--fs-snapshots` and `--working-copy`
- Env: `AH_FS_SNAPSHOTS`, `AH_WORKING_COPY`
- TOML: under `[fs]` in [Configuration.md](../Configuration.md) and JSON Schema

Rationale: Users may prefer a Git worktree–based flow even when a more efficient CoW setup is available, or enforce `worktree` mounting to avoid per‑process mounts.

## Provider Selection (Auto)

Auto detection evaluates all available providers and picks the highest‑scoring capability for the host repo and requested working‑copy mode:

0. Disabled (explicit): bypass snapshotting entirely; Time‑Travel FsSnapshots disabled
1. ZFS (dataset or ancestor dataset writable): cow‑overlay supported on Linux via clone + bind
2. Btrfs subvolume: cow‑overlay supported on Linux via subvolume snapshot + bind
3. AgentFS (macOS/Windows best‑effort; Linux optional): cow‑overlay supported via FSKit/WinFsp/FUSE
4. Git‑based snapshots: supports Worktree and In‑Place; does not support Cow‑Overlay. When Cow‑Overlay is requested and only Git is available, the orchestrator SHALL either select AgentFS (if available) or fall back to Worktree with a diagnostic.

Working‑copy compatibility:

- When `workingCopy = InPlace`, select providers that support in‑place capture (e.g., Git commit‑tree with a temp index; ZFS/Btrfs native snapshots). Do not select providers that require a separate user‑space mount for isolation.

Diagnostics surface detection notes and the final decision in `ah doctor` and verbose logs.

## Workflow Overview

1. Prepare workspace (independent writable copy or in‑place):
   - ZFS/Btrfs: create snapshot + clone/subvolume snapshot
   - AgentFS: create branch from base snapshot and bind process
   - Git: create a dedicated worktree/branch at a snapshot commit
   - In‑Place: no workspace preparation; run on the original working copy; provider may still capture snapshots in place (Git, ZFS/Btrfs)

2. Working copy mode:
   - Cow‑overlay: mount/bind so the agent sees the workspace at the original repo path with CoW isolation
   - Worktree: run agent in the provider’s workspace directory (isolated)
   - In‑Place: run directly in the original working copy (no isolation), while still allowing snapshots when supported

## Rationale: Typed options vs. “stringly” options

The trait intentionally omits loosely‑typed `opts` maps. Common behavior is captured by strongly‑typed parameters (`WorkingCopyMode`) and provider selection occurs before invoking the trait. Any provider‑specific knobs MUST be configured when constructing the concrete provider (e.g., a `GitProvider::new(GitConfig { include_untracked: bool, worktrees_dir: PathBuf })`). This keeps the cross‑provider trait stable, strongly typed, and testable, while still allowing rich provider‑specific configuration via constructors/builders.

## State Persistence Integration

Before launching a local agent session, AH writes session workspace information to the local state DB (see State Persistence):

- Selected `provider` (zfs|btrfs|agentfs|git|disable) and `workingCopy` mode
- `workspace_path` (when applicable)
- `cleanup_token` (for robust teardown)
- Provider details in metadata JSON; for Git this includes the shadow repo/worktree base directory

This record is created before the agent process starts and updated as snapshots/branches are created (rows in `fs_snapshots`).

3. Snapshot during session: `snapshot_now(label)` records points for Agent Time‑Travel

4. Branch from snapshot (intervene): create a new writable workspace using `branch_from_snapshot`

## Git‑Based Snapshots (Summary)

Git provider captures the working state as a commit without mutating the user’s branch, then uses `git worktree` for writable workspaces. Details are in "[Git-Based-Snapshots.md](Git-Based-Snapshots.md)". Key properties:

- Zero changes to the primary index and branch; uses a separate temporary index (`GIT_INDEX_FILE`) and `git stash create`/`commit‑tree` to capture staged + unstaged changes. Untracked files can be included via an opt‑in mode that enumerates untracked paths into the temporary index.
- Snapshot refs are stored under `refs/ah/sessions/<session>/snapshots/<n>`; writable branches under `refs/ah/branches/<session>/<name>`.
- Path‑preserving requires an AgentFS bind mount or Linux mount namespace tricks; otherwise runs as a worktree.

## Integration with Agent Time‑Travel

Time‑Travel associates `SessionMoment`s with `SnapshotRef { id, provider }`. Seeking mounts the snapshot read‑only; branching calls `branch_from_snapshot`. The provider choice is opaque to the UI. See [specs/Public/Agent-Time-Travel.md](../Agent-Time-Travel.md).

## Platform Strategies (Updated)

- Linux: prefer ZFS/Btrfs; AgentFS optional for unified behavior when CoW is unavailable; Git is the portable fallback.
- macOS/Windows: prefer AgentFS for cow‑overlay; Git provider is always available as a fallback (worktree).
- Remote/VM: the same provider selection runs remotely; persistent sync (Mutagen) recommended for host↔VM file transfer.

## Operational Notes

- Cleanup is explicit via `cleanup(token)`; the CLI ensures cleanup on normal exit and on SIGINT via shutdown hooks.
- Providers must be concurrency‑safe; multiple sessions for the same repo are allowed.
- Providers should emit human‑actionable errors (missing tools, insufficient privileges) and hints.

## Security

- Cow‑overlay mounts isolate processes using mount namespaces (Linux) or AgentFS per‑process binding (macOS/Windows). When unavailable, default to `worktree`.
- Providers must not mutate the original working copy in `CowOverlay` or `Worktree` modes. In `InPlace` mode mutation is explicit and expected; the UI warns and Time‑Travel capabilities depend on the selected provider.
