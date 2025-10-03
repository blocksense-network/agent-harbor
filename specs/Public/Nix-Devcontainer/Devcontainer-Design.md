## Devcontainer Design — Requirements, Rationale, Implementation

### Scope

Define a layered devcontainer architecture for Agent Harbor:

- A lower‑level Nix devcontainer base that standardizes Nix, caches, and host↔guest cache sharing.
- An Agents Base Image that builds on the Nix base and ships all supported agentic CLIs plus the framework glue for task execution, recording, and credential propagation.
- Downstream project images that extend the Agents Base Image by adding a project‑specific Nix devshell for building/testing that codebase.

This document describes requirements, rationale, and implementation details. Nix base specifications live under `docs/nix-devcontainer/`.

### Requirements

- Provide a consistent developer experience across Linux, macOS, and Windows (Docker Desktop/WSL) with minimal host prerequisites.
- Support all agentic CLIs integrated by agents‑workflow (see per‑agent docs under `docs/agents/`).
- Implement credential propagation: when host is authenticated to a provider, the guest resolves the same credentials without re‑login.
- Provide persistent build caches and Nix store layers across container rebuilds; enable optional host↔guest cache sharing for language package managers.
- Integrate execution hooks needed by Agent Time‑Travel (timeline SessionMoments, FsSnapshot triggers) without interfering with normal shell use.
- Keep secrets out of images; use runtime env/secrets and read‑only mounts; be auditable.

### Design Rationale

- Layering separates concerns:
  - The base image (e.g., Nix or other supported systems) focuses on reproducibility and performant builds via shared caches.
  - Agents base concentrates agent tools, shell setup, recording hooks, and credential bridges.
  - Projects remain small: typically just a `devcontainer.json` and a project-specific devshell or environment definition.
- While Nix is very well supported and is a common choice for project dependencies and toolchains, derived projects are not required to use Nix. Other systems that offer efficient cache sharing and reproducibility, such as SPack, may also be supported to a high degree.
- Cache sharing is opt-in and explicit per package manager to avoid accidental leakage and permission issues.
- Credential propagation prioritizes agent-approved mechanisms (env vars, config files, OS agents) and relies on read-only host mounts or forwarded sockets where feasible.

### Layered Images

1. Nix Devcontainer Base (see `docs/nix-devcontainer/`)
   - Installs Nix (flakes enabled), configures substituters/cachix.
   - Declares persistent volumes for `/nix`, Nix DB, and general caches.
   - Provides optional shared cache mounts for common package managers (npm/pnpm/yarn, pip/pipx, cargo, go, maven/gradle, etc.).
   - Exposes a thin entrypoint that sources project devshell when present.

2. Agents Base Image
   - FROM: Nix Devcontainer Base.
   - Installs all supported agentic CLIs using Nix. The list of agents is shared with the agent-harbor Nix package, defined at the root of this repository, ensuring consistency between the devcontainer and the Nix package set.
   - Prepares integration with agent‑provided hook mechanisms (e.g., Claude Code hooks) to emit SessionMoments and trigger FsSnapshots.
   - Prepares netrc/SSH/gh credential bridges (runtime only; nothing baked into the image).

3. Project Image
   - FROM: Agents Base Image.
   - Adds project `flake.nix`/`devshell` and any extra tools.
   - Optionally extends cache mounts for project‑specific package managers.

### devcontainer.json (reference)

Downstream projects consume the base like this (illustrative):

```json
{
  "name": "agent-harbor-dev",
  "image": "ghcr.io/blocksense/agent-harbor-agents-base:latest",
  "features": {},
  "remoteUser": "vscode",
  "updateRemoteUserUID": true,
  "containerEnv": {
    "ZDOTDIR": "/home/vscode/.zsh",
    "NIX_CONFIG": "experimental-features = nix-command flakes"
  },
  "mounts": [
    "source=ah-nix-store,target=/nix,type=volume",
    "source=ah-cache-home,target=/home/vscode/.cache,type=volume",
    "source=ah-cargo,target=/home/vscode/.cargo,type=volume",
    "source=ah-go-cache,target=/home/vscode/.cache/go-build,type=volume",
    "source=ah-go-mod,target=/home/vscode/go/pkg/mod,type=volume"
  ],
  "runArgs": ["--init"],
  "customizations": {
    "vscode": {
      "settings": {
        "terminal.integrated.defaultProfile.linux": "zsh"
      },
      "extensions": ["ms-vscode-remote.remote-containers"]
    }
  },
  "postCreateCommand": "./codex-setup || true"
}
```

### Credential Propagation Framework

Principles:

- Never bake secrets into images. Use runtime environment variables, forwarded agents/sockets, and read‑only mounts of host config where safe.
- Normalize through the shell setup scripts so agent CLIs find credentials predictably.

Mechanisms:

- Env pass‑through: Allowlist known vars (examples: `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `OPENROUTER_API_KEY`, `HUGGING_FACE_HUB_TOKEN`, `GITHUB_TOKEN`, `GITLAB_TOKEN`, `BITBUCKET_TOKEN`).
- Git hosting: generate `~/.netrc` from `GITHUB_TOKEN`/`GITLAB_TOKEN`/`BITBUCKET_TOKEN` (as in `codex-setup`).
- GitHub CLI and Copilot CLI: prefer `gh auth` state by mounting `~/.config/gh/hosts.yml` read‑only or exporting `GH_TOKEN` if org policy allows.
- SSH agent forwarding: mount `SSH_AUTH_SOCK` and pass through `~/.ssh/known_hosts` read‑only; avoid copying private keys.
- Cloud SDKs: allow optional read‑only mounts of provider config (e.g., `~/.aws`, `~/.config/gcloud`) when required by a specific agent; otherwise rely on env‑based credentials.

Each agent’s exact mapping is captured in `docs/agents/<tool>.md` and validated in CI with probe commands (e.g., `gh auth status`, minimal API ping for OpenAI/Anthropic).

### Time‑Travel Event Emission in Devcontainer

- Use agent tool built‑in hooks (e.g., Claude Code hooks) to emit SessionMoment events at tool boundaries and important milestones. Shell‑level hooks are not used.
- Projects may ship hook scripts under `.claude/hooks/` or equivalent per‑agent location. The devcontainer ensures these paths are available and executable.

### Caching and Host↔Guest Cache Sharing

- Persistent volumes for: `/nix`, language caches (Cargo, Go, npm/pnpm/yarn, pip, maven/gradle), and compiler caches (sccache/ccache).
- Build systems: support Bazel (`--disk_cache`, `--repository_cache`) and Buck2 (dir/HTTP cache) via mounted cache paths and project config.
- Host↔guest sharing guidelines and test plans are defined in `docs/nix-devcontainer/cache-guidelines.md` and `test-suite.md`.
- For Windows hosts, prefer Docker volumes over bind mounts to avoid permission/line‑ending pitfalls; on macOS/Linux, bind mounts are acceptable for read‑only config.

### Implementation Details

- User/UID: set `remoteUser` to a non‑root user matching host UID to simplify shared cache permissions.
- Entrypoint: minimal init (tini), run `common-pre-setup`, source project hooks if present `.agents/*-setup`, then `common-post-setup`.
- Health: ship `ah health` checks for snapshot providers, Nix, caches, gh/ssh/auth readiness.
- Security: secrets via env or forwarded sockets; config mounts read‑only; no tokens written to image layers.

### Testing and CI

- Cold/warm build benchmarks with and without caches.
- Credential probes for each agent (non‑destructive): `gh auth status`, short `curl` to model/provider endpoints when keys present.
- Time‑travel hook smoke tests: run a few commands and verify SessionMoments are emitted.
- Bazel/Buck2 cache smoke tests: verify host‑prewarmed cache reuse and warm speedups; gate mounts by compatibility.
- Multi‑OS smoke tests: verify Mutagen sessions, fence latency, and `run_everywhere` execution on tagged followers.
- Cross‑platform matrix: Linux, macOS (Docker Desktop), Windows (WSL2/Hyper‑V).

### Migration Plan

- Phase 1 (this repo): build and publish `agents‑workflow‑nix‑base` and `agents‑workflow‑agents‑base` images to a registry (e.g., GHCR).
- Phase 2 (extraction): split Nix base to a standalone repo; keep Agents base here, pinning base by digest.

### Open Questions

- Exact per‑agent credential files and minimal scopes (document in `docs/agents/`).
- Which package manager caches to enable by default vs opt‑in.
- Windows credential manager integrations (e.g., Git Credential Manager) via bind vs env.
