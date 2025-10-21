# Git‑Based Snapshots — Provider Specification

## Purpose

Define a portable snapshot provider that works anywhere Git is available. It captures the current working state as Git commits without mutating the user’s branch, and provisions writable workspaces using `git worktree`.

This provider complements CoW filesystem and AgentFS providers and participates in the same `FsSnapshotProvider` trait.

## Design Goals

- Zero changes to the user’s working tree and index
- Capture staged and unstaged changes; optional inclusion of untracked files
- Fast provisioning via a persistent shadow repository and `git worktree`
- Namespaced refs for snapshots and branches; explicit cleanup
- Compatible with Time‑Travel seek/branch, and path‑preserving mounting via AgentFS when requested

## Terminology

- Base commit: `HEAD` of the user’s current branch at session start
- Snapshot commit: a commit object that records the working state at a moment in time
- Session branch: a writable branch created from a snapshot commit, materialized as a worktree
- Provider ref namespace:
  - Snapshots: `refs/ah/sessions/<sid>/snapshots/<n>`
  - Branches: `refs/ah/branches/<sid>/<name>`

## Shadow Repository (Performance Backbone)

To avoid mutating the user’s repository and to keep each snapshot O(changes), the provider maintains a persistent bare “shadow repo” per project that shares objects with the primary repo.

- Location: `git.shadowRepoDir/<repo-id>.git` (configurable); if unset, default to the main repo’s `.git` directory.
- Creation (idempotent):
  - `git init --bare --shared=group <shadow>`
  - Configure alternates so the shadow reuses the primary object store:
    - Write `<primary>/.git/objects` to `<shadow>/objects/info/alternates`
  - Set knobs: `gc.auto=0`, `receive.denyCurrentBranch=ignore`, `core.logAllRefUpdates=true`.
- Index per session: maintain a persistent index file at `<shadow>/index-<sid>` to seed subsequent snapshots from the last commit quickly.
- Object locality: new/changed blobs may be written into the primary repo (seen via alternates) or directly into the shadow; both are acceptable. The default writes to the primary via `git hash-object -w` for maximal reuse.

Benefits:

- Seeding from the last snapshot avoids re‑enumerating the entire tree.
- Alternates guarantee zero duplication for unchanged content.
- Using a persistent per‑session index has predictable O(changes) behavior.

## Capturing a Snapshot

The provider uses the session’s shadow index file to avoid touching the primary index and branch. Each snapshot produces a single commit representing the working state at capture time.

First snapshot of a session (tracked changes only):

1. Resolve repo root and ensure no in‑progress operations (`rebase`, `merge`, `bisect`) that would cause ambiguous state.
2. Ensure the shadow repo exists and the session index path `<shadow>/index-<sid>` is created.
3. Seed the session index from HEAD: `GIT_INDEX_FILE=<shadow>/index-<sid> git -C <shadow> read-tree -m <primary-HEAD>`.
4. Overlay staged+unstaged tracked changes efficiently:
   - Fast path: enumerate changed tracked files: `git -C <primary> ls-files -m -z`.
   - For each path, compute blob OIDs in the primary: `git -C <primary> hash-object -w -- <path>` (batch with `--stdin-paths`).
   - Update the shadow index entries directly: `git -C <shadow> update-index --index-info` (lines: `MODE OID\tPATH`).
   - Simpler (slower) alternative: `GIT_INDEX_FILE=<shadow>/index-<sid> git -C <primary> add -A`.
5. Write a tree: `GIT_INDEX_FILE=<shadow>/index-<sid> git -C <shadow> write-tree` → `TREE_OID`.
6. Create a commit object parented to `<primary-HEAD>`: `git -C <shadow> commit-tree TREE_OID -p <primary-HEAD> -m "ah: snapshot <sid>/<n> <label?> <ts>"` → `COMMIT_OID`.
7. Atomically set the session ref: `git -C <shadow> update-ref --create-reflog refs/ah/sessions/<sid>/snapshots/<n> COMMIT_OID`.

Including untracked files (opt‑in):

- Enumerate untracked files (excluding ignored): `git -C <primary> ls-files --others --exclude-standard -z`.
- Batch hash and write them into the primary object store: `git -C <primary> hash-object -w --stdin-paths -z`.
- Add to the shadow index with `git -C <shadow> update-index --add --index-info` before the write‑tree step.

Properties:

- No changes to the user’s working directory or primary index; temporary index and objects are garbage‑collectable.
- Snapshot commit parents `HEAD` to preserve history context for diffs.

Subsequent snapshots in the same session (incremental):

1. Seed index from the last session snapshot tree instead of `<primary-HEAD>`:
   - Read last commit: `PREV=$(git -C <shadow> rev-parse --verify refs/ah/sessions/<sid>/snapshots/<n-1>)`
   - `GIT_INDEX_FILE=<shadow>/index-<sid> git -C <shadow> read-tree -m $PREV^{tree}`
2. Overlay only changed tracked files since the last capture (same as steps above, but the set to update is still computed vs the primary working copy). This minimizes re‑hashing unchanged paths.
3. Continue with write‑tree, commit (parent to PREV), and update the snapshot ref `<n>`.

## Writable Workspaces (Session Branches)

1. Create a namespaced branch from a snapshot commit: `git branch --force refs/ah/branches/<sid>/<name> <COMMIT_OID>`.
2. Materialize a worktree: `git worktree add --detach <worktrees_dir>/<sid>/<name> <COMMIT_OID>`.
   - Optionally check out the branch instead of detached HEAD: `git worktree add <dir> refs/ah/branches/<sid>/<name>`.
3. Return `exec_path = <dir>` when `WorkingCopyMode::Worktree`.
4. Cow‑overlay is not supported by the Git provider. If cow‑overlay is requested, the orchestrator SHALL select a provider that supports it (ZFS/Btrfs on Linux, AgentFS on macOS/Windows) or fall back to Worktree with a diagnostic.

In‑Place compatibility:

- When `WorkingCopyMode::InPlace`, the provider does not create a workspace. The agent runs in the original working copy, and snapshots are captured by producing commits using the temporary‑index method. Branching from a snapshot may still create a worktree for the branch (mode may differ from the main session mode).

Notes:

- Multiple concurrent branches/worktrees per session are supported.
- The provider MAY set `core.worktree` in a worktree‑local config for clarity; global repo state is never altered.

## Read‑Only Mount for Seek

For read‑only inspection, the provider uses `git worktree add --detach --locked` to create a read‑only materialization. The provider returns a filesystem path to the materialized tree; the caller manages lifetime.

Cow‑overlay (path‑stable) read‑only views are not supported by the Git provider; select AgentFS when a path‑stable view is required on macOS/Windows.

## Cleanup

When a session ends or `cleanup(token)` is invoked:

- Remove worktrees created by the session: `git worktree remove --force <dir>`.
- Delete namespaced branches and snapshot refs:
  - `git update-ref -d refs/ah/branches/<sid>/<name>`
  - `git update-ref -d refs/ah/sessions/<sid>/snapshots/<n>` (or prune the whole `refs/ah/sessions/<sid>`)
- Optionally run `git gc --prune=now` when all refs are gone (guarded by a heuristic to avoid heavy GC during active work).
  - Shadow repo GC is deferred; a scheduled maintenance task MAY repack shared objects when the project is idle.

## Edge Cases and Safeguards

- Reentrancy: A session ID is unique; provider uses it to scope all refs and directories.
- Large repos: Snapshot cost is proportional to the number of changed paths; tree writing is O(changes). The provider MAY parallelize blob hashing across CPU cores.
- Submodules: By default, record the superproject gitlink; deep capture of submodules is out of scope in v1.
- Unmerged entries: Abort snapshot with a clear error when index contains conflicts (or capture the current index as‑is on opt‑in).
- Line ending filters and smudge/clean: The temporary index respects repository attributes, matching the behavior of a real commit.
- Permissions: Executable bit captured per Git semantics; xattrs are not represented.
- LFS: Untracked inclusion may store large binaries as loose objects; default `includeUntracked = false`. Future work: LFS‑aware capture.

## Configuration Keys (Provider‑specific)

- `git.includeUntracked`: boolean (default false) — include untracked files during snapshot capture.
- `git.worktreesDir`: path (optional) — base directory for worktrees; default `.git/worktrees-ah/` under repo `.git` directory, falling back to an OS temp directory when `.git` is bare or not writable.
- `git.shadowRepoDir`: path (optional) — location of the shadow bare repository used for namespaced refs and the per‑session index. By default this is the main repo `.git` directory; when isolation is desired, a separate bare repository may be created under the project’s cache directory (recorded in session metadata).

Keys live under the `[fs]` section; see [Configuration.md](../Configuration.md).

## Mapping to FsSnapshotProvider Trait

- kind: `SnapshotProviderKind::Git`
- detect_capabilities: check `git` binary presence, worktree support, `.git` writability; path‑preserving = false.
- prepare_writable_workspace(repo, Worktree, ..) → create worktree from the selected snapshot commit (first snapshot uses `<primary-HEAD>`; later snapshots use the latest session commit).
- snapshot_now(ws, label) → commit under namespaced ref (tracked only or with untracked per config).
- branch_from_snapshot(snap, Worktree, ..) → worktree at snapshot commit; CowOverlay flow is handled by the orchestrator using AgentFS or namespaces.
- cleanup(token) → remove worktrees and delete refs for the session.

## CLI Diagnostics

- `ah doctor` prints detection results and the selected provider.
- `ah session info` shows `{ provider, workingCopy, execPath }` and latest snapshot ref.

## Security Considerations

- Provider never amends or force‑moves the user’s branches; all refs live under `refs/ah/...`.
- Worktrees are owned by the current user; no privilege escalation required.

## Future Work

- Optional content‑addressed asset cache to speed up first materialization of large generated trees
- Deep submodule capture and restoration
- Integration with LFS lock/status for better user feedback
