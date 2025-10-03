## CLI Repository Selection

### Purpose

Defines how the AH CLI selects a repository (and working directory) for commands that operate on source code. This document provides a canonical, self‑contained description for reuse across commands.

### Scope

Applies to commands that:

- Need a repository root to operate (e.g., `ah task`, `ah agent fs init-session`, repo utilities)
- Accept `--repo <PATH|URL>` and optionally `--workspace <NAME>`

### Inputs

- `--repo <PATH|URL>` (optional)
- Current working directory (CWD)
- Supported VCS types: Git, Mercurial (hg), Bazaar (bzr), Fossil

### Algorithm

1. Resolve execution mode

- If `--remote-server` is configured/provided, repository path resolution may be delegated to the server for server‑side operations (see each command’s spec and [Remote Mode](Remote-Mode.md)). When running locally (see [Local Mode](Local-Mode.md)), continue below.

2. If `--repo` is provided

- If `--repo` is a local path:
  - Expand and canonicalize the path.
  - If path points inside a supported VCS working copy, walk up to find the VCS root directory.
  - If not a repository, abort with a descriptive error: include the provided path and reason (e.g.,
    "Not a supported VCS repository: <path>").
- If `--repo` is a URL:
  - Local mode: Attempt to resolve the repository by consulting the local state database (see [State Persistence](State-Persistence.md)) for a matching known repository record (by canonical remote URL). If found, use its `root_path` as the repository root. If not found and the command does not explicitly support cloning/fetching, abort with an actionable error that suggests adding the repo locally or using a supported command.
  - Remote mode: pass the URL to the server according to the command’s contract.

3. If `--repo` is not provided (local mode)

- Start from CWD and walk parent directories until a VCS root is found.
- A VCS root is identified using standard markers per VCS:
  - Git: a directory containing `.git/` (worktrees are supported via Git’s worktree metadata);
  - Mercurial: `.hg/`;
  - Bazaar: `.bzr/`;
  - Fossil: `_FOSSIL_`/`.fslckout` (platform/tooling dependent).
- If a supported VCS root is found:
  - Select that directory as the repository root and record the VCS type.
- If no VCS root is found:
  - Abort with a clear error explaining that a repository is required and suggest using `--repo`.

4. Validate repository state

- Determine the default branch name for the VCS (e.g., Git: `main`/`master` resolution; see the `ah-repo` crate).
- Commands that prohibit running on primary branches MUST enforce protection (`main`, `master`, `trunk`, `default`) when applicable.
- Ensure required tools for the detected VCS are available in PATH when the command needs them; otherwise abort with an actionable error.

5. Multi‑repo and workspaces

- When `--workspace` is specified in local mode, commands that require a single repository MUST reject the option with a clear message (workspaces are defined for server mode unless otherwise specified by the command’s own doc).
- When a command explicitly supports multi‑repo, it MUST enumerate member repositories as defined by that command.

### Error semantics

- Repository detection failures MUST include:
  - The starting path (either CWD or `--repo` path)
  - The list of VCS types checked or the condition that failed
  - A remediation hint (e.g., "Run inside a repository or pass --repo <PATH>")

### Non‑interactive behavior

- When `--non-interactive` is set and a repository cannot be resolved, commands MUST return the interactive‑required exit code (10) with a message indicating that `--repo` is required.

### Examples

- Inside a Git worktree:
  - CWD = `/work/project/subdir` → Repository root = `/work/project`
- With `--repo ../project`:
  - Path resolves to `/work/project`; detection walks up to find the VCS root.
- Not inside a repo, no `--repo`:
  - Fails with: "Repository required. Run inside a supported VCS repository or pass --repo <PATH>".

### References

- See `ah task` behavior and flow for branch protection and task‑file behaviors that depend on repository selection.
- See `ah-repo` crate for VCS operations and root detection helpers.
