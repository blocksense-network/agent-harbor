## Nix Devcontainer — Cache Sharing Test Suite

### Purpose

Validate cache persistence and correctness for supported package managers and toolchains across container rebuilds and typical workflows.

### Test Matrix

- Platforms: Linux, macOS (Docker Desktop), Windows (WSL2)
- Images: Nix Base, Agents Base, representative project images
- Package Managers: npm/pnpm/yarn, pip/pipx/poetry, cargo, go, maven/gradle, ccache/sccache
- Build Systems: Bazel, Buck2 (local dir/disk caches and HTTP caches)

### Scenarios (step-by-step recipes)

Assumptions for all scenarios:

- Workspace folder is the repository root: `$WS`.
- Dev Containers CLI is installed: `devcontainer --version` works on host.
- Base images are available locally or via GHCR: `ghcr.io/blocksense/agent-harbor-nix-base:latest` and `ghcr.io/blocksense/agent-harbor-agents-base:latest`.
- `devcontainer.json` defines named volumes for caches as in the reference spec.

1) Cold → Warm Install

Preconditions:

- Ensure cache volumes are absent to simulate cold start.

Steps:

```bash
# 1) Remove existing named volumes used by devcontainer.json (idempotent)
for v in ah-nix-store ah-cache-home ah-cargo ah-go-cache ah-go-mod; do docker volume rm "$v" 2>/dev/null || true; done

# 2) Bring up the devcontainer
devcontainer up --workspace-folder "$WS"

# 3) Inside container, prepare minimal sample projects and install deps
# Node (pnpm)
devcontainer exec --workspace-folder "$WS" -- bash -lc '
  set -euo pipefail
  mkdir -p /work/node && cd /work/node
  corepack enable pnpm
  printf "{\"name\":\"node-sample\",\"version\":\"1.0.0\",\"dependencies\":{\"left-pad\":\"^1.3.0\"}}" > package.json
  /usr/bin/time -f "%E %M" -o /tmp/node_cold.time pnpm i
'
# Python (pip)
devcontainer exec --workspace-folder "$WS" -- bash -lc '
  set -euo pipefail
  mkdir -p /work/py && cd /work/py
  printf "requests==2.32.0\n" > requirements.txt
  /usr/bin/time -f "%E %M" -o /tmp/pip_cold.time pip install -r requirements.txt
'
# Rust (cargo)
devcontainer exec --workspace-folder "$WS" -- bash -lc '
  set -euo pipefail
  cargo new --vcs none /work/rs && cd /work/rs
  echo "anyhow = \"1\"" >> Cargo.toml
  /usr/bin/time -f "%E %M" -o /tmp/cargo_cold.time cargo build
'

# 4) Rebuild the container (warm)
devcontainer up --workspace-folder "$WS" --update-remote-user-uid default

# 5) Re-run installs/builds and record timings as warm
# Node (pnpm)
devcontainer exec --workspace-folder "$WS" -- bash -lc '
  cd /work/node
  /usr/bin/time -f "%E %M" -o /tmp/node_warm.time pnpm i --frozen-lockfile --prefer-offline
'
# Python (pip)
devcontainer exec --workspace-folder "$WS" -- bash -lc '
  cd /work/py
  /usr/bin/time -f "%E %M" -o /tmp/pip_warm.time pip install -r requirements.txt --no-input
'
# Rust (cargo)
devcontainer exec --workspace-folder "$WS" -- bash -lc '
  cd /work/rs
  /usr/bin/time -f "%E %M" -o /tmp/cargo_warm.time cargo build
'
```

Expected results:

- Warm timings are significantly faster than cold.
- Network fetches are reduced or eliminated where offline caches apply (pnpm prefer-offline, cargo local index reuse).
- No errors; cache directories exist under the named volumes.

2) Lockfile Change Invalidation

Preconditions:

- Complete “Cold → Warm Install” once.

Steps:

```bash
# Node: add a new dependency and reinstall
devcontainer exec --workspace-folder "$WS" -- bash -lc '
  cd /work/node
  jq ".dependencies += {\"lodash\":\"^4.17.21\"}" package.json > package.json.tmp && mv package.json.tmp package.json
  pnpm i --lockfile-only
  pnpm i --prefer-offline
'

# Python: bump a dependency version and reinstall
devcontainer exec --workspace-folder "$WS" -- bash -lc '
  cd /work/py
  sed -i.bak "s/requests==.*/requests==2.32.3/" requirements.txt
  pip install -r requirements.txt --no-input
'

# Rust: add a new crate and rebuild
devcontainer exec --workspace-folder "$WS" -- bash -lc '
  cd /work/rs
  echo "thiserror = \"1\"" >> Cargo.toml
  cargo build
'
```

Expected results:

- Dependency graphs update correctly; builds succeed.
- Caches do not force stale results; new artifacts are produced as needed.

3) Toolchain Change

Preconditions:

- A project devshell or flake exists that pins tool versions (e.g., Rust toolchain or Node).

Steps:

```bash
# 1) Update devshell tool version (example: Rust stable→beta) in flake.nix
#    Commit the change locally (or produce a patched flake) so devcontainer rebuilds.

# 2) Recreate the container to pick up the new toolchain
devcontainer up --workspace-folder "$WS" --update-remote-user-uid default

# 3) Rebuild sample projects
devcontainer exec --workspace-folder "$WS" -- bash -lc '
  cargo --version && cd /work/rs && cargo clean && cargo build
'
```

Expected results:

- ABI/toolchain changes cause appropriate invalidation; rebuild completes from caches where compatible and recompiles where not.

4) Concurrent Builds

Preconditions:

- Two workspace folders on host: `$WS_A` and `$WS_B` (e.g., `$WS` duplicated), both using the same named cache volumes via the shared `devcontainer.json`.

Steps:

```bash
# 1) Bring up two containers for the two workspaces
devcontainer up --workspace-folder "$WS_A"
devcontainer up --workspace-folder "$WS_B"

# 2) Start concurrent builds that share the same named volumes
(devcontainer exec --workspace-folder "$WS_A" -- bash -lc 'cd /work/rs && cargo clean && cargo build' &)
(devcontainer exec --workspace-folder "$WS_B" -- bash -lc 'cd /work/rs && cargo clean && cargo build' &)
wait
```

Expected results:

- Both builds succeed; no cache corruption; subsequent builds are still fast in either container.

5) Security Hygiene

Preconditions:

- Warm caches exist; no sensitive tokens intentionally placed in the workspace.

Steps:

```bash
# 1) Inspect cache directories for secrets and permissions
# Example: scan for common token patterns and ensure ownership is the devcontainer user

devcontainer exec --workspace-folder "$WS" -- bash -lc '
  set -e
  id
  for d in ~/.cargo ~/.cache/pip ~/.m2 ~/.gradle ~/go/pkg/mod ~/.cache/go-build ~/.cache/pnpm ~/.npm; do
    [ -d "$d" ] || continue
    echo "Checking: $d"
    find "$d" -type f -maxdepth 2 -printf "%u:%g %p\n" | head -n 50
    rg -n --ignore-case --max-columns 200 "(ghp_|glpat-|xox[baprs]-|aws_access_key_id|secret|token)" "$d" || true
  done
'
```

Expected results:

- No secrets are found in caches by pattern scan; cache ownership aligns with the non‑root user; permissions are sane.

6) Offline Build (warm caches)

Preconditions:

- Complete “Cold → Warm Install” to populate caches.

Steps:

```bash
# Use package manager offline modes where supported
# Cargo
devcontainer exec --workspace-folder "$WS" -- bash -lc 'cd /work/rs && cargo build --offline'
# Go (disable network via proxy off)
devcontainer exec --workspace-folder "$WS" -- bash -lc 'mkdir -p /work/go && cd /work/go && go env -w GOPROXY=off && go mod init example.com/off && echo "package main; func main(){}" > main.go && go build ./...'
# pnpm (offline)
devcontainer exec --workspace-folder "$WS" -- bash -lc 'cd /work/node && pnpm i --offline'
# Maven (offline)
devcontainer exec --workspace-folder "$WS" -- bash -lc 'mkdir -p /work/java && cd /work/java && mvn -o -q archetype:generate -DgroupId=ah.test -DartifactId=ah-offline -DarchetypeArtifactId=maven-archetype-quickstart -DinteractiveMode=false && cd ah-offline && mvn -o -q -DskipTests package'
```

Expected results:

- Builds succeed without network when caches permit; unsupported cases are documented per manager.

9) Bazel: Host→Guest local caches (disk/repo)

Preconditions:

- Host has Bazel and pre-populated `--disk_cache` and `--repository_cache` with a small sample workspace.
- `.bazelrc` in the workspace points to container paths: `/home/vscode/.cache/bazel/disk` and `/home/vscode/.cache/bazel/repo`.

Steps:

```bash
# 1) Bind mount host caches into the container paths specified in .bazelrc
devcontainer up --workspace-folder "$WS" \
  --mount type=bind,source=$HOME/.cache/bazel/disk,target=/home/vscode/.cache/bazel/disk \
  --mount type=bind,source=$HOME/.cache/bazel/repo,target=/home/vscode/.cache/bazel/repo

# 2) Warm build inside container (expect cache hits)
devcontainer exec --workspace-folder "$WS" -- bash -lc '
  bazel clean --expunge_async || true
  /usr/bin/time -f "%E %M" -o /tmp/bazel_warm.time bazel build //...
'
```

Expected results:

- Build completes faster vs cold baseline; logs indicate cache hits.

10) Buck2: Host→Guest dir cache

Preconditions:

- Host has Buck2 with dir cache populated at `$HOME/.cache/buck2/cache`; workspace `.buckconfig` contains:

```
[cache]
mode = dir
dir = /home/vscode/.cache/buck2/cache
```

Steps:

```bash
# 1) Bind mount host dir cache into container
devcontainer up --workspace-folder "$WS" \
  --mount type=bind,source=$HOME/.cache/buck2/cache,target=/home/vscode/.cache/buck2/cache

# 2) Warm build inside container (expect cache hits)
devcontainer exec --workspace-folder "$WS" -- bash -lc '
  /usr/bin/time -f "%E %M" -o /tmp/buck2_warm.time buck2 build //...
'
```

Expected results:

- Build completes faster vs cold baseline; cache is consulted with hits.

7) Nix: Host→Guest cache reuse via local binary cache (Linux host)

Preconditions:

- Linux host with Nix installed and access to Dev Containers CLI.
- A sample flake or devshell attribute that brings in noticeable dependencies (e.g., rust toolchain, node).

Steps (host):

```bash
# 1) Build desired flake inputs on host to populate the host store
pushd "$WS"
nix --extra-experimental-features nix-command --extra-experimental-features flakes develop .#default -c true || true

# 2) Export required store paths to a local binary cache dir (nar/narinfo)
BIN_CACHE=$(mktemp -d)
# Discover closure for the devshell (example uses flake default shell)
nix path-info --derivation --recursive .#devShells.x86_64-linux.default | xargs -I{} sh -c 'nix path-info --recursive --json {}' | jq -r '.[].path' | sort -u > /tmp/paths.txt
xargs -a /tmp/paths.txt -n 50 nix copy --to file://$BIN_CACHE
echo "Binary cache at: $BIN_CACHE"
popd

# 3) Start devcontainer with the binary cache mounted read-only
devcontainer up --workspace-folder "$WS" --mount type=bind,source=$BIN_CACHE,target=/mnt/host-bin-cache,readonly
```

Steps (inside container):

```bash
# 4) Add the file:// substituter and trust it for this session
devcontainer exec --workspace-folder "$WS" -- bash -lc '
  set -euo pipefail
  echo "extra-substituters = file:///mnt/host-bin-cache" >> /etc/nix/nix.conf
  echo "trusted-substituters = file:///mnt/host-bin-cache" >> /etc/nix/nix.conf
  nix --version && nix-store --version
  # 5) Resolve the same devshell; expect fast due to local binary cache
  /usr/bin/time -f "%E %M" -o /tmp/devshell_from_host_cache.time nix --extra-experimental-features nix-command --extra-experimental-features flakes develop .#default -c true
'
```

Expected results:

- Most or all store paths are fetched from `file:///mnt/host-bin-cache` with minimal or no remote traffic.
- Time to realize the devshell is significantly lower than a cold run without the binary cache.

Notes:

- This scenario is Linux‑only due to OS/ABI differences; on macOS/Windows, prefer remote caches (Cachix) or container‑persistent volumes.

8) Cache compatibility gating (multi‑manager)

Preconditions:

- A cache compatibility script is available in the devcontainer entrypoint that decides whether to enable a host↔guest cache mount based on compatibility keys.

Compatibility keys (examples):

- Nix: `system` (e.g., x86_64-linux), `nixVersion`, CA‑derivations flag.
- Python: `pythonVersion`, `abiTag` (e.g., cp311‑manylinux_2_28_x86_64), `platform`.
- Node: `os`, `arch`, `nodeVersion`, package manager (`pnpm|yarn|npm`).
- Cargo: `rustcVersion`, `targetTriple`.
- Go: `goVersion`, `GOOS`, `GOARCH`.

Steps:

```bash
# 1) Simulate a mismatch by exporting a fake compatibility key into the environment
devcontainer exec --workspace-folder "$WS" -- bash -lc '
  export AW_CACHE_COMPAT_OVERRIDE="python:cp310-manylinux_2_27_x86_64"  # example mismatch
  # 2) Run health to see which caches are enabled
  ah health --caches | tee /tmp/health_caches.txt
'

# 3) Inspect that python cache mounts are disabled due to mismatch
devcontainer exec --workspace-folder "$WS" -- bash -lc '
  rg -n "python.*compat: mismatch" /tmp/health_caches.txt
'

# 4) Clear override and verify caches enable when compatible
devcontainer exec --workspace-folder "$WS" -- bash -lc '
  unset AW_CACHE_COMPAT_OVERRIDE
  ah health --caches | tee /tmp/health_caches2.txt
  rg -n "python.*compat: ok" /tmp/health_caches2.txt || true
'
```

Expected results:

- With mismatch, affected caches show `compat: mismatch → disabled` and are not mounted/read.
- With compatible settings, caches show `compat: ok` and mounts are active.

### Measurements

- Wall‑clock durations (cold vs warm)
- Network requests count/bytes (where observable)
- Cache sizes before/after
- Hit/miss metrics (cargo, sccache)

### Automation

- Provide `ah health --caches` to print configured mounts and sizes.
- CI jobs per package manager with synthetic sample projects.

Implementation Plan: See [Devcontainer Design.status.md](Devcontainer-Design.status.md) for milestones, success criteria, and CI strategy.
