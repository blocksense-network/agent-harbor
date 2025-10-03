### Overview

This document tracks the implementation plan and status for the Devcontainer functionality described across the files in `specs/Public/Nix Devcontainer/`.

Goals:

- Deliver layered images (Nix Base → Agents Base → Project) that provide a consistent developer experience across Linux, macOS (Docker Desktop), and Windows (WSL2/Hyper‑V).
- Implement credential propagation, host↔guest cache sharing, and time‑travel hooks as specified in [Devcontainer-Design.md](Devcontainer-Design.md).
- Provide a standard `.devcontainer/ah-healthcheck` contract invoked by `ah repo check`.
- Ship a robust, automated test matrix validating cache behavior, credentials, health, and hooks per [Devcontainer-Test-Suite.md](Devcontainer-Test-Suite.md).

References:

- [Devcontainer Design.md](Devcontainer-Design.md)
- [Devcontainer Cache Guidelines.md](Devcontainer-Cache-Guidelines.md)
- [Devcontainer Test Suite.md](Devcontainer-Test-Suite.md)
- [Devcontainer User Setup.md](Devcontainer-User-Setup.md)

### Deliverables

1. `agent-harbor-nix-base` image (GHCR)

- Nix with flakes enabled, substituters/cachix configured.
- Declared persistent volumes for `/nix` and common caches.
- Minimal init and entrypoint that sources project devshell if present.

2. `agent-harbor-agents-base` image (GHCR)

- FROM nix base, installs supported agentic CLIs via Nix; aligns versions with repository’s Nix package set.
- Integrations with agent‑provided hook mechanisms (e.g., Claude Code hooks) to emit SessionMoments and trigger FsSnapshots. Shell‑level hooks are out of scope.
- Credential bridges via env pass‑through and read‑only mounts; no secrets in layers.

3. Reference project devcontainer

- `devcontainer.json` consuming Agents Base; named volumes for caches; `postCreateCommand` hooks.
- `.devcontainer/ah-healthcheck` implementing the health contract with `--json` output.

4. AH CLI integration

- `ah repo check` invokes `.devcontainer/ah-healthcheck` via Dev Containers CLI when available; falls back to `just health`/`make health` on host.
- `ah health --caches` prints configured cache mounts and sizes.

5. CI pipeline

- Image builds, cache benchmarks (cold vs warm), credential probes, hook smoke tests, and cross‑platform matrix.

### Success criteria (acceptance)

- Cross‑platform: Linux, macOS (Docker Desktop), Windows (WSL2) flows work without manual fixes.
- Caches persist across rebuilds and speed up builds measurably; offline warm builds succeed where expected.
- Credential propagation makes common agent tools usable without re‑auth inside the container.
- `.devcontainer/ah-healthcheck --json` passes and is stable/parseable; `ah repo check` reports clear diagnostics on failure.
- Built‑in agent hooks emit SessionMoments reliably at tool boundaries; are opt‑out via agent configuration.

### Milestones and tasks (with automated tests)

M0. Bootstrap and scaffolding (1–2d)

- Create image build structure (Nix flake + Dockerfile if needed), GHCR workflows, and CI skeleton.
- Smoke test: build base image; `nix --version` prints; minimal `devcontainer.json` opens a shell.

M1. Nix base image (2–3d)

- Install Nix with flakes; configure substituters/cachix.
- Declare volumes: `/nix`, `~/.cache` subset as needed; user UID/GID alignment.
- Tests: cold/warm Nix store reuse; permission sanity for shared volumes.

M2. Agents base image (3–4d)

- Install agent CLIs via Nix; align with repo’s package list.
- Add common setup scripts (`common-pre-setup`, `common-post-setup`) and ensure agent hook locations are mounted/available.
- Tests: `gh --version`, agent CLI probe commands available; no secrets in image layers.

M3. Credential propagation (2–3d)

- Env allowlist (`OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `OPENROUTER_API_KEY`, `HUGGING_FACE_HUB_TOKEN`, `GITHUB_TOKEN`, etc.).
- Read‑only mounts for `~/.netrc`, `~/.config/gh/hosts.yml`, SSH agent forwarding (`SSH_AUTH_SOCK`), and known hosts.
- Tests: `gh auth status` when host authenticated; short API probes when keys are present (non‑destructive).

M4. Cache sharing mounts (3–4d)

- Named volumes for cargo/go/npm/pnpm/yarn/pip/maven/gradle; optional sccache/ccache.
- Documented in [Devcontainer-Cache-Guidelines.md](Devcontainer-Cache-Guidelines.md); mount defaults and opt‑ins implemented.
- Tests: per‑manager cold→warm speedups; lockfile change invalidation; concurrent builds; offline warm builds.

M5. Healthcheck contract and AH integration (2–3d)

- Provide `.devcontainer/ah-healthcheck` with `--json`; checks for nix, devshell, task‑runner, git, disk, optionally network.
- Implement `ah repo check` flow that prefers devcontainer exec, with fallbacks as documented.
- Tests: golden JSON output, exit codes (0/1/2) behavior, CLI fallback paths.

M6. Time‑travel hooks via agent integrations (2–3d)

- Configure Claude Code (and other supported tools) hooks to emit structured events to FIFO/log; provide sample hook scripts under `.claude/hooks/`.
- Ensure hooks are packaged or generated in the devcontainer workspace and enabled by default (agent‑specific opt‑out supported).
- Tests: run a representative tool action and verify event emission; verify opt‑out disables emission.

M7. Cross‑platform validation (3–5d)

- macOS and Windows host runs with Docker Desktop/WSL2; volume/permission adjustments.
- Tests: healthcheck passes; caches function; known Windows/macOS path quirks handled.

M8. Publish and versioning (1–2d)

- Publish images to GHCR with content digests; pin downstream by digest.
- SBOM generation and vulnerability scanning as part of publish.

M9. Documentation and examples (1–2d)

- Update user setup docs; provide a reference `devcontainer.json` snippet and sample project.
- How‑to for adding project‑specific devshells; troubleshooting guide.

### Test strategy & automation

- Follow scenarios and measurements in [Devcontainer Test Suite.md](Devcontainer-Test-Suite.md).
- CI matrix: Linux (ubuntu‑latest), macOS runners, Windows runners using WSL2 for Dev Containers CLI where possible.
- Cache metrics collection; golden JSON for healthcheck; artifact upload of logs and timing tables.
- Security hygiene checks that ensure no secrets are present in cache volumes or images.

### Risks & mitigations

- Host/OS divergence (permissions, line endings, UID/GID): prefer named Docker volumes; align `remoteUser` UID when feasible.
- Windows credential managers: rely on `GH_TOKEN`/env where mounts are unreliable; document limitations.
- Performance regressions with hooks: keep hook commands lightweight; allow opt‑out; benchmark in CI.

### Status tracking

- M0: pending
- M1: pending
- M2: pending
- M3: pending
- M4: pending
- M5: pending
- M6: pending
- M7: pending
- M8: pending
- M9: pending

### Change log
