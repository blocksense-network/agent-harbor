#
# Nix Dev Shell Policy (reproducibility)
# -------------------------------------
# When running inside the Nix dev shell (environment variable `IN_NIX_SHELL` is set),
# Just tasks and helper scripts MUST NOT use fallbacks such as `npx`, brew installs,
# network downloads, or any ad-hoc tool bootstrap. If a required command is missing
# in that context, the correct fix is to add it to `flake.nix` (devShell.buildInputs)
# and re-enter the shell, not to fall back. Outside of the Nix shell, tasks may use
# best-effort fallbacks for convenience, but scripts should gate them like:
#   if [ -n "$IN_NIX_SHELL" ]; then echo "missing <tool>; fix flake.nix" >&2; exit 127; fi
# This keeps `nix develop` fully reproducible and prevents hidden network variability.

root-dir := justfile_directory()

set shell := ["./scripts/nix-env.sh", "-c"]

# Define REPOMIX_OUT_DIR with a default value
REPOMIX_OUT_DIR := env('REPOMIX_OUT_DIR', 'repomix')

# Cargo Build Variables
#
# These variables define cargo build flags for individual components and combined builds.
# The goal is to allow parallel cargo builds by consolidating multiple build commands
# into a single cargo invocation, which leverages cargo's internal parallelism instead
# of relying on just's sequential target execution.
#
# Individual component variables allow for selective builds, while the combined
# CARGO_BUILD_RUST_TEST_BINARIES enables building all test binaries in parallel.
CARGO_BUILD_SBX_HELPER := "--bin sbx-helper -p sbx-helper"
CARGO_BUILD_AGENTFS_DAEMON := "--bin agentfs-daemon -p agentfs-daemon"
CARGO_BUILD_AGENTFS_INTERPOSE_TEST := "--bin agentfs-interpose-test-helper -p agentfs-interpose-e2e-tests"
CARGO_BUILD_DEBUGGING_ENFORCEMENT := "-p debugging-enforcement"
CARGO_BUILD_TUI_TESTING := "-p tui-testing"
CARGO_BUILD_AGENTFS_INTERPOSE_SHIM := "-p agentfs-interpose-shim"
CARGO_BUILD_E2E_CALL_REAL_SHIM := "-p e2e-call-real-shim"
CARGO_BUILD_E2E_SHIM_A := "-p e2e-shim-a"
CARGO_BUILD_E2E_SHIM_B := "-p e2e-shim-b"
CARGO_BUILD_E2E_STACKABLE_HOOKS := "-p e2e-stackable-hooks --bins"
CARGO_BUILD_E2E_AUTO_PROPAGATION_SHIM := "-p e2e-auto-propagation-shim"
CARGO_BUILD_AH_COMMAND_TRACE_SHIM := "-p ah-command-trace-shim"
CARGO_BUILD_CGROUP_ENFORCEMENT_TESTS := "-p cgroup-enforcement-tests"
CARGO_BUILD_OVERLAY_ENFORCEMENT_TESTS := "-p overlay-enforcement-tests"
CARGO_BUILD_FS_SNAPSHOTS_HARNESS := "-p fs-snapshots-test-harness --bin fs-snapshots-harness-driver"
CARGO_BUILD_AGENTFS_FUSE_HOST := "--package agentfs-fuse-host --features fuse"
CARGO_RELEASE_BUILD_AGENTFS_FUSE_STRESS := "--package agentfs-fuse-stress --release"

# Combined cargo flags for test binaries
# This concatenation expresses dependencies between named sets of binaries
CARGO_BUILD_RUST_TEST_BINARIES := \
  CARGO_BUILD_FS_SNAPSHOTS_HARNESS + " " + \
  CARGO_BUILD_SBX_HELPER + " " + \
  CARGO_BUILD_AGENTFS_DAEMON + " " + \
  CARGO_BUILD_AGENTFS_INTERPOSE_TEST + " " + \
  CARGO_BUILD_DEBUGGING_ENFORCEMENT + " " + \
  CARGO_BUILD_TUI_TESTING + " " + \
  CARGO_BUILD_AGENTFS_INTERPOSE_SHIM + " " + \
  CARGO_BUILD_E2E_CALL_REAL_SHIM + " " + \
  CARGO_BUILD_E2E_SHIM_A + " " + \
  CARGO_BUILD_E2E_SHIM_B + " " + \
  CARGO_BUILD_E2E_AUTO_PROPAGATION_SHIM + " " + \
  CARGO_BUILD_E2E_STACKABLE_HOOKS + " " + \
  CARGO_BUILD_AH_COMMAND_TRACE_SHIM + " " + \
  CARGO_BUILD_CGROUP_ENFORCEMENT_TESTS + " " + \
  CARGO_BUILD_OVERLAY_ENFORCEMENT_TESTS

# IMPORTANT: Never use long scripts in Justfile recipes!
# Long scripts set a custom shell, overriding our nix-env.sh setting.
# Move complex scripts to the scripts/ folder instead.

# List all available Just tasks
default:
  @just --list

# Run the full set of checks and builds that CI executes across workflows
ci-all:
    just ci-lint
    just ci-build-devshell
    just ci-build-package
    just ci-test
    just ci-webui
    just ci-fuse-harness
    just ci-pjdfstest
    just ci-backstore-macos

# Match the lint job in .github/workflows/ci.yml
ci-lint:
    yarn install
    pre-commit run --all-files

# Build the dev shell cache the same way CI does
ci-build-devshell:
    system=$(nix eval --impure --expr 'builtins.currentSystem' --raw)
    nix run nixpkgs#nix-fast-build -L -- --no-nom --skip-cached --flake .#devShells.${system}.default

# Build the main Agent Harbor package as in CI
ci-build-package:
    system=$(nix eval --impure --expr 'builtins.currentSystem' --raw)
    nix run nixpkgs#nix-fast-build -L -- --no-nom --skip-cached --flake .#packages.${system}.default

# Reproduce the Rust test job from CI
ci-test:
    cargo test -p sandbox-fs
    just check
    just build-sbx-helper-release
    just build-cgroup-tests
    just test-rust-verbose

# Mirror the WebUI job steps
ci-webui:
    yarn install
    just webui-lint
    just webui-type-check
    just webui-build
    just webui-build-mock

# Replicate the FUSE harness workflow (Linux only)
ci-fuse-harness:
    if [ "$(uname -s)" != "Linux" ]; then echo "Skipping FUSE harness: requires Linux"; exit 0; fi
    cargo build -p agentfs-fuse-host --features fuse
    cargo build -p agentfs-control-cli
    export SKIP_FUSE_BUILD=1 SKIP_CONTROL_CLI_BUILD=1
    just test-fuse-mount-cycle
    just test-fuse-mount-failures
    just test-fuse-mount-concurrent
    just test-fuse-basic-ops
    just test-fuse-negative-ops
    just test-fuse-overlay-ops
    just test-fuse-control-plane

# Replicate the pjdfstest workflow (Linux only)
ci-pjdfstest:
    if [ "$(uname -s)" != "Linux" ]; then echo "Skipping pjdfstest suite: requires Linux"; exit 0; fi
    just build-fuse-test-binaries
    SKIP_FUSE_BUILD=1 just test-pjdfstest-full

# Mirror the macOS backstore workflow (noop on non-macOS)
ci-backstore-macos:
    if [ "$(uname -s)" != "Darwin" ]; then echo "Skipping macOS backstore checks: requires macOS"; exit 0; fi
    just fmt-rust-check
    just lint-rust
    just check
    cargo nextest run -p agentfs-backstore-macos

[doc('Run a command to clean the repository of untracked files')]
clean:
  git clean -fdx \
    -e .jj \
    -e .env \
    -e .direnv \
    -e .vscode \
    -e .pre-commit-config.yaml \
    -- {{root-dir}}

reinstall-pre-commit-hooks:
    git config --unset-all core.hooksPath
    pre-commit install -f --hook-type pre-commit

# Check Rust code for compilation errors
check:
    cargo check --workspace

# Build all test binaries needed for Rust workspace tests
build-rust-test-binaries: build-fuse-test-binaries
    cargo build {{CARGO_BUILD_RUST_TEST_BINARIES}}

# Run Rust tests
test-rust *args: build-rust-test-binaries
    cargo nextest run --workspace {{args}}

test-rust-single *args: build-rust-test-binaries
    cargo nextest run --workspace --profile single {{args}}

# Run Rust tests with verbose output
test-rust-verbose *args: build-rust-test-binaries
    cargo nextest run --workspace --verbose {{args}}

# Run ACP client-focused tests (Milestone 1 scaffold)
test-acp-client:
    cargo nextest run -p ah-agents -E 'test(acp_client)'

# Build mock TUI dashboard binary
build-mock-tui-dashboard:
    cd tests/tools/mock-tui-dashboard && cargo build --bin mock-tui-dashboard

# Run mock TUI dashboard (new MVVM architecture)
run-mock-tui-dashboard: build-mock-tui-dashboard
    cd tests/tools/mock-tui-dashboard && cargo run --bin mock-tui-dashboard

# Snapshot Testing with Insta
# ===========================

# Accept all pending snapshot changes (use when snapshots have legitimately changed)
# This is the most common command when developing - it accepts the current test output
# as the new expected snapshots. Always review changes first with 'just insta-review'
# to ensure the changes are correct and not regressions.
insta-accept:
    cargo insta accept

# Interactively review snapshot changes before accepting them
# This opens a terminal UI where you can see diffs between old and new snapshots,
# and choose which ones to accept, reject, or skip. Use this instead of blindly
# accepting all changes to avoid missing regressions.
insta-review:
    cargo insta review

# Reject all pending snapshot changes (reverts to previous snapshots)
# Useful when you've made changes that shouldn't affect snapshots, or when
# you want to undo accidental snapshot updates.
insta-reject:
    cargo insta reject

# Run tests and check snapshots without updating them
# This verifies that current snapshots match expected state. Use this in CI
# or when you want to ensure no unexpected snapshot changes occurred.
insta-test:
    cargo insta test

# Show all pending snapshots that need to be reviewed
# Useful for getting an overview of what snapshots have changed without
# opening the interactive review interface.
insta-pending:
    cargo insta pending-snapshots

# Run snapshot tests for specific packages (useful for focused testing)
# Example: just insta-test-pkg ah-mux
insta-test-pkg pkg:
    cargo insta test -p {{pkg}}

# Accept snapshots for specific packages (useful when only one package changed)
# Example: just insta-accept-pkg ah-mux
insta-accept-pkg pkg:
    cargo insta accept -p {{pkg}}

# Quick workflow: test snapshots, show status
# This runs snapshot tests and reports whether snapshots are up to date or need review
# Note: Some snapshots (like tmux golden snapshots) capture dynamic terminal output
# and may need periodic acceptance due to timing/environment differences.
insta-check:
    ./scripts/insta-test.sh

# Lint Rust code
lint-rust:
    cargo clippy --workspace

# Format Rust code
fmt-rust:
    pre-commit run rustfmt --all-files || true

# Check Rust code formatting (used by CI)
fmt-rust-check:
    cargo fmt --all --check

# Build release binary for sbx-helper
build-sbx-helper-release:
    cargo build --release --bin sbx-helper

legacy-test:
    export RUBYLIB=legacy/ruby/lib && ruby -Ilegacy/ruby/test legacy/ruby/test/run_tests_shell.rb

# Run codex-setup integration tests (Docker-based)
legacy-test-codex-setup-integration:
    ./setup-tests/test-runner.sh

# Run only snapshot-related tests (ZFS, Btrfs, and Git providers)
legacy-test-snapshot:
    RUBYLIB=legacy/ruby/lib ruby scripts/run_snapshot_tests.rb

# Lint the Ruby codebase
legacy-lint:
    rubocop legacy/ruby

# Auto-fix lint issues where possible
legacy-lint-fix:
    rubocop --autocorrect-all legacy/ruby

# Build and publish the gem
legacy-publish-gem:
    gem build legacy/ruby/agent-task.gemspec && gem push agent-task-*.gem

# Validate all JSON Schemas with ajv (meta-schema compile)
conf-schema-validate:
    scripts/conf-schema-validate.sh

# Check TOML files with Taplo (uses schema mapping if configured)
conf-schema-taplo-check:
    ./scripts/lint-toml.sh

# Serve schema docs locally with Docson (opens http://localhost:3000)
conf-schema-docs:
    docson -d specs/schemas

# Validate Mermaid diagrams in Markdown with mermaid-cli (mmdc)
md-mermaid-check:
    bash scripts/md-mermaid-validate.sh specs/**/*.md

# Lint Markdown structure/style in specs with markdownlint-cli2
md-lint:
    markdownlint-cli2 "specs/**/*.md"

# Check external links in Markdown with lychee
md-links:
    lychee --config .lychee.toml --accept "200..=299,403" --no-progress --require-https --max-concurrency 8 "specs/**/*.md"

# Spell-check Markdown with cspell (uses default dictionaries unless configured)
md-spell:
    cspell "specs/**/*.md"

# Add words to the shared cspell allow-list
allow-words *words:
    ./scripts/allow_words.py {{words}}
    @git add .cspell.json

# Sync spell-check dictionaries (cspell + vale) without adding new words
sync-spell-dicts:
    python3 scripts/allow_words.py --sync

# Test that spell checking tools (cspell and vale) work correctly
test-spell-checking:
    ./scripts/test_spell_checking.py

# Create reusable file-backed filesystems for testing ZFS and Btrfs providers
# This sets up persistent test environments in ~/.cache/agent-harbor
create-test-filesystems:
    scripts/create-test-filesystems.sh

# Check the status of test filesystems
check-test-filesystems:
    scripts/check-test-filesystems.sh

# Clean up test filesystems created by create-test-filesystems
cleanup-test-filesystems:
    scripts/cleanup-test-filesystems.sh

# Launch the AH filesystem snapshots daemon for testing (requires sudo)
legacy-start-ah-fs-snapshots-daemon:
    legacy/scripts/launch-ah-fs-snapshots-daemon.sh

# Stop the AH filesystem snapshots daemon
legacy-stop-ah-fs-snapshots-daemon:
    legacy/scripts/stop-ah-fs-snapshots-daemon.sh

# Check status of AH filesystem snapshots daemon
legacy-check-ah-fs-snapshots-daemon:
    ruby legacy/scripts/check-ah-fs-snapshots-daemon.rb

# Build ah-fs-snapshots-daemon binary
build-daemon:
    cargo build --package ah-fs-snapshots-daemon

# Build all binaries needed for daemon integration tests
build-daemon-tests: build-daemon

# Launch the new Rust AH filesystem snapshots daemon for testing (requires sudo)
start-ah-fs-snapshots-daemon:
    scripts/start-ah-fs-snapshots-daemon.sh

# Stop the new Rust AH filesystem snapshots daemon
stop-ah-fs-snapshots-daemon:
    scripts/stop-ah-fs-snapshots-daemon.sh

# Restart the new Rust AH filesystem snapshots daemon
restart-ah-fs-snapshots-daemon:
    scripts/restart-ah-fs-snapshots-daemon.sh

# Check status of the new Rust AH filesystem snapshots daemon
check-ah-fs-snapshots-daemon:
    scripts/check-ah-fs-snapshots-daemon.sh

# Run comprehensive daemon integration tests (requires test filesystems)
test-daemon-integration: build-daemon-tests
    cargo test --package ah-fs-snapshots-daemon -- --nocapture integration

# Run filesystem snapshot provider integration tests (requires root for ZFS/Btrfs operations)
test-fs-snapshots: build-daemon-tests
    cargo test --package ah-fs-snapshots -- --nocapture integration

# Run filesystem snapshot provider unit tests only (no root required)
test-fs-snapshots-unit:
    cargo test --package ah-fs-snapshots

# Run all spec linting/validation in one go
lint-specs:
    scripts/lint-specs.sh

# Build cgroup enforcement test binaries (fork_bomb, memory_hog, cpu_burner, test_orchestrator)
build-cgroup-test-binaries:
    cargo build {{CARGO_BUILD_CGROUP_ENFORCEMENT_TESTS}}

# Build overlay enforcement test binaries (overlay_test_orchestrator, blacklist_tester, overlay_writer)
build-overlay-test-binaries:
    cargo build {{CARGO_BUILD_OVERLAY_ENFORCEMENT_TESTS}}

# Build interpose shim test binaries (agentfs-interpose-test-helper)
build-interpose-test-binaries:
    cargo build \
      {{CARGO_BUILD_AGENTFS_INTERPOSE_TEST}} \
      {{CARGO_BUILD_AGENTFS_DAEMON}} \
      {{CARGO_BUILD_AGENTFS_INTERPOSE_SHIM}}

# Build stackable-interpose e2e test binaries (test-program, call_real_demo) and shim libraries
build-stackable-interpose-test-binaries:
    cargo build \
      {{CARGO_BUILD_E2E_CALL_REAL_SHIM}} \
      {{CARGO_BUILD_E2E_SHIM_A}} \
      {{CARGO_BUILD_E2E_SHIM_B}} \
      {{CARGO_BUILD_E2E_AUTO_PROPAGATION_SHIM}}
    cargo build {{CARGO_BUILD_E2E_STACKABLE_HOOKS}} --bins

# Build command trace shim library needed for e2e tests
build-command-trace-shim:
    cargo build {{CARGO_BUILD_AH_COMMAND_TRACE_SHIM}}

build-fs-snapshots-harness:
    cargo build {{CARGO_BUILD_FS_SNAPSHOTS_HARNESS}}

# Build sbx-helper binary
build-sbx-helper:
    cargo build {{CARGO_BUILD_SBX_HELPER}}

# Build macOS sandbox launcher (ah-macos-launcher)
build-ah-macos-launcher:
    cargo build --bin ah-macos-launcher

# Build all test binaries needed for cgroup enforcement tests
build-cgroup-tests: build-sbx-helper build-cgroup-test-binaries

# Build all test binaries needed for overlay enforcement tests
build-overlay-tests: build-sbx-helper build-overlay-test-binaries

# Build network enforcement test binaries (network_test_orchestrator, curl_tester, port_collision_tester)
build-network-test-binaries:
    cargo build --bin network_test_orchestrator --bin curl_tester --bin port_collision_tester

# Build all test binaries needed for network enforcement tests
build-network-tests: build-sbx-helper build-network-test-binaries

# Build debugging enforcement test binaries (debugging_test_orchestrator, ptrace_tester, process_visibility_tester, mount_test)
build-debugging-test-binaries:
    cargo build {{CARGO_BUILD_DEBUGGING_ENFORCEMENT}}

# Build all test binaries needed for debugging enforcement tests
build-debugging-tests: build-sbx-helper build-debugging-test-binaries

# Build TUI test binaries
build-tui-test-binaries:
    cargo build {{CARGO_BUILD_TUI_TESTING}}

# Build FUSE test binaries (requires FUSE support)
build-fuse-test-binaries:
    ./scripts/build-fuse-test-binaries.sh

# Build FUSE host binary (requires FUSE support)
build-fuse-host:
    ./scripts/build-fuse-host.sh

# Run basic filesystem smoke tests against a mounted FUSE filesystem
# Usage: just test-fuse-basic /mnt/agentfs
# Note: Mount the filesystem first with: just mount-fuse /mnt/agentfs
test-fuse-basic mountpoint:
    ./scripts/test-fuse-basic.sh "{{mountpoint}}"

# Mount the AgentFS FUSE filesystem at a given mount point
# Usage: just mount-fuse /tmp/agentfs  (recommended for user mounting)
mount-fuse mountpoint:
    ./scripts/mount-fuse.sh "{{mountpoint}}"

# Unmount the AgentFS FUSE filesystem from a given mount point
# Usage: just umount-fuse /tmp/agentfs
umount-fuse mountpoint:
    ./scripts/umount-fuse.sh "{{mountpoint}}"

# Automated mount/unmount cycle test harness (T2.1)
test-fuse-mount-cycle:
    ./scripts/test-fuse-mount-cycle.sh

# Mount failure handling scenarios (T2.2)
test-fuse-mount-failures:
    ./scripts/test-fuse-mount-failures.sh

# Concurrent mount harness (T2.4)
test-fuse-mount-concurrent:
    ./scripts/test-fuse-mount-concurrent.sh

# Basic filesystem operations (F3.1–T3.4)
test-fuse-basic-ops:
    ./scripts/test-fuse-basic-ops.sh

# Negative path/errno validation (F3.2)
test-fuse-negative-ops:
    ./scripts/test-fuse-negative-ops.sh

# Overlay semantics validation (F3.5)
test-fuse-overlay-ops:
    ./scripts/test-fuse-overlay-ops.sh

# Control plane integration (F4)
test-fuse-control-plane:
    ./scripts/test-fuse-control-plane.sh

# Extended attributes harness (F8.1)
test-fuse-xattrs:
    ./scripts/test-fuse-xattrs.sh

# Special file creation harness (F8.2)
test-fuse-mknod:
    ./scripts/test-fuse-mknod.sh

# Mount option harness (F8.3)
test-fuse-mount-options:
    ./scripts/test-fuse-mount-options.sh

# Advanced I/O harness (F8.4)
test-fuse-advanced-io:
    ./scripts/test-fuse-advanced-io.sh

# Performance benchmarks (F6)
test-fuse-performance:
    ./scripts/test-fuse-performance.sh

test-fuse-performance-release:
    FUSE_BUILD_PROFILE=release AGENTFS_FUSE_HOST_BIN="target/release/agentfs-fuse-host" ./scripts/test-fuse-performance.sh

# Stress + fault-injection harness (F7)
test-fuse-stress:
    ./scripts/test-fuse-stress.sh

# Security permission matrix harness (F10.3)
test-fuse-security-permissions:
    ./scripts/test-fuse-security-permissions.sh

# Security privilege escalation harness (F10.1)
test-fuse-security-privileges:
    ./scripts/test-fuse-security-privileges.sh

# Security input validation harness (F10.2)
test-fuse-security-input:
    ./scripts/test-fuse-security-input.sh

# Security sandbox boundary harness (T10.4)
test-fuse-security-sandbox:
    ./scripts/test-fuse-security-sandbox.sh

# Security robustness harness (T10.5)
test-fuse-security-robustness:
    ./scripts/test-fuse-security-robustness.sh

# Cross-version compatibility harness (F9)
test-fuse-compat:
    ./scripts/test-fuse-compat.sh

# CLI control plane parity harness (F15/T15.1)
# Requires: AgentFS mounted with --allow-other, or run with sudo
test-agentfs-cli-parity:
    ./scripts/test-agentfs-cli-control-plane.sh

# CLI failure injection harness (F15/T15.2)
# Tests error handling when daemon stops mid-run or is unavailable
test-agentfs-cli-failure-injection:
    ./scripts/test-agentfs-cli-failure-injection.sh

# CLI SSZ schema validation tests (F15/T15.3)
# Validates CLI request builders against agentfs-control.request.logical.json
test-agentfs-cli-schema:
    cargo test -p ah-cli --test cli_request_builders_test --features agentfs

# Run all F15 CLI control plane tests
test-agentfs-cli-all: test-agentfs-cli-parity test-agentfs-cli-failure-injection test-agentfs-cli-schema

# F16 AgentFS sandbox integration tests
# Requires: AgentFS daemon running (just start-ah-fs-snapshots-daemon)
test-agentfs-sandbox:
    cargo build -p ah-cli --features agentfs
    ./scripts/test-agentfs-sandbox.sh

# F19 user-mode restart/orphan cleanup harness (no sudo required)
test-agentfs-user-restart:
    ./scripts/test-agentfs-user-restart.py

# Run all F16 sandbox tests
test-agentfs-sandbox-all: test-agentfs-sandbox

# Run all FUSE tests (requires sudo)
# This runs all FUSE harness tests and the full pjdfstest suite
test-fuse-all:
    @echo "🧪 Running comprehensive FUSE test suite..."
    @echo "📋 Running FUSE harness tests..."
    just test-fuse-mount-cycle
    just test-fuse-mount-failures
    just test-fuse-mount-concurrent
    just test-fuse-basic-ops
    just test-fuse-negative-ops
    just test-fuse-overlay-ops
    just test-fuse-control-plane
    just test-fuse-xattrs
    just test-fuse-mknod
    just test-fuse-mount-options
    just test-fuse-advanced-io
    just test-fuse-security-permissions
    just test-fuse-security-privileges
    just test-fuse-security-input
    just test-fuse-security-sandbox
    just test-fuse-security-robustness
    just test-fuse-stress
    just test-fuse-compat
    @echo "📋 Running full pjdfstest suite..."
    just test-pjdfstest-full
    @echo "✅ All FUSE tests completed!"

# Setup comprehensive pjdfstest suite with test files
# Usage: just setup-pjdfstest-suite
# See docs/PJDFSTest-Guide.md for detailed usage instructions
setup-pjdfstest-suite:
    ./scripts/setup-pjdfstest.sh

# List available pjdfstest categories
# Usage: just list-pjdfstest-categories
list-pjdfstest-categories:
    ./scripts/run-pjdfstest.sh --list

# Advanced pjdfstest targets (for manual filesystem mounting)

# Run pjdfstest suite against a mounted FUSE filesystem (advanced usage)
# Usage: just run-pjdfstest [options] <mountpoint> [test-paths...]
# Prerequisites: Mount filesystem first with just mount-fuse (or use auto-mount options)
# Examples:
#   just run-pjdfstest /tmp/agentfs                    # Run all tests
#   just run-pjdfstest /tmp/agentfs unlink/            # Run unlink category
#   just run-pjdfstest -q /tmp/agentfs chmod/ chown/   # Run multiple categories quietly
#   just run-pjdfstest --auto-mount /tmp/agentfs       # Auto-mount and run all tests
run-pjdfstest *all_args:
    #!/usr/bin/env bash
    # Pass all arguments directly to the script (it handles the parsing)
    ./scripts/run-pjdfstest.sh {{all_args}}

# Essential pjdfstest targets (auto-mount by default)

# Run individual test file (auto-mounts if needed, most common for debugging)
# Usage: just pjdfs-file <test-file> [mountpoint]
# Example: just pjdfs-file unlink/00.t
pjdfs-file test_file mountpoint="/tmp/agentfs":
    ./scripts/run-pjdfstest.sh "{{mountpoint}}" "{{test_file}}"

# Run test category (auto-mounts if needed, for category-level debugging)
# Usage: just pjdfs-cat <category> [mountpoint]
# Example: just pjdfs-cat unlink
pjdfs-cat category mountpoint="/tmp/agentfs":
    ./scripts/run-pjdfstest.sh "{{mountpoint}}" "{{category}}/"

# Dedicated harness that sets up pjdfstest, mounts AgentFS, runs the
# entire suite with logging/JSON summary, and unmounts on completion.
# Usage: just test-pjdfstest-full [/tmp/agentfs]
test-pjdfstest-full mountpoint="/tmp/agentfs":
    ./scripts/test-pjdfstest-full.sh "{{mountpoint}}"

# Run complete pjdfstest workflow: setup (if needed), mount, test, unmount
# Usage: just test-pjdfstest-suite [mountpoint]
#   mountpoint: Mount point for the filesystem (default: /tmp/agentfs)
# This runs the full pjdfstest suite (all test categories)
test-pjdfstest-suite mountpoint="/tmp/agentfs":
    ./scripts/run-pjdfstest.sh --auto-setup --auto-mount --build-binaries "{{mountpoint}}"

# Build all TUI test binaries needed for TUI testing
build-tui-tests: build-tui-test-binaries

# Run cgroup tests with E2E enforcement verification
test-cgroups:
    just build-cgroup-tests
    cargo test -p sandbox-integration-tests --verbose

# Agent Harbor Dev Site Development Targets
# ==========================================

# Build Agent Harbor Dev site (Next.js static export)
website-build:
    yarn workspace @agent-harbor/agent-harbor.dev run build

# Run Agent Harbor Dev site development server
website-dev:
    yarn workspace @agent-harbor/agent-harbor.dev run dev

# Lint Agent Harbor Dev site
website-lint:
    yarn workspace @agent-harbor/agent-harbor.dev run lint

# WebUI Development Targets
# ========================

# Build WebUI application (SSR mode - default for development/testing)
webui-build:
    yarn workspace ah-webui-ssr-sidecar run build

# Build WebUI application in static mode for Electron embedding
webui-build-static:
    WEBUI_BUILD_MODE=static yarn workspace ah-webui-ssr-sidecar run build

# Build mock server
webui-build-mock:
    yarn workspace ah-webui-mock-server run build

# Run WebUI development server
webui-dev:
    yarn workspace ah-webui-ssr-sidecar run dev

# Run mock REST API server
webui-mock-server:
    yarn workspace ah-webui-mock-server run dev

# Lint all WebUI projects
webui-lint:
    yarn workspace ah-webui-ssr-sidecar run lint
    yarn workspace ah-webui-mock-server run lint
    yarn workspace ah-webui-e2e-tests run lint

# Type check all WebUI projects
webui-type-check:
    yarn workspace ah-webui-ssr-sidecar run type-check
    yarn workspace ah-webui-mock-server run type-check

# Check for unused files/exports/dependencies in WebUI projects
webui-knip:
    yarn workspace ah-webui-ssr-sidecar run knip
    yarn workspace shared run knip

# Check TypeScript type coverage in WebUI projects
webui-type-coverage:
    yarn workspace ah-webui-ssr-sidecar run type-coverage
    yarn workspace shared run type-coverage

# Format all WebUI projects
webui-format:
    yarn workspace ah-webui-ssr-sidecar run format
    yarn workspace ah-webui-mock-server run format
    yarn workspace ah-webui-e2e-tests run format

# Run WebUI E2E tests
webui-test-unit:
    yarn workspace ah-webui-ssr-sidecar run test:run

webui-test *args:
    cd webui/e2e-tests && ./start-servers.sh {{args}}

# Run only API contract tests (mock server only)
webui-test-api: webui-build-mock
    process-compose up --tui=false --no-server api-tests

# Build WebUI SSR server (SolidStart)
webui-build-ssr:
    yarn workspace ah-webui-ssr-sidecar run build

# Start WebUI with mock server for manual testing (cycles through 5 scenarios)
manual-test-webui:
    ./scripts/manual-test-webui.sh

# Launch manual agent start script for testing agent integration
manual-test-agent-start *args:
    ./scripts/manual-test-agent-start.py {{args}}

# Launch manual agent record script for testing recording functionality
manual-test-ah-agent-record *args:
    ./scripts/manual-test-agent-start.py --record {{args}}

# Run manual task workflow (interactive + non-interactive variants)
manual-test-task:
    ./scripts/manual-test-task.sh

# Launch manual TUI test script for testing TUI functionality
# Usage: just manual-test-tui [--repo NAME] [--fs TYPE]
#   --repo NAME: Repository name to create (default: example-repo)
#   --fs TYPE: Filesystem type - zfs, btrfs, apfs, or tmp (default: zfs)
manual-test-tui *args:
    ./scripts/manual-test-tui.py {{args}}

# Launch remote-mode TUI manual testing workflow (REST or mock server)
manual-test-tui-remote *args:
    ./scripts/manual-test-remote.py {{args}}

# Convenience wrapper for mock server remote testing (loads default scenario)
manual-test-tui-remote-mock *args:
    ./scripts/manual-test-remote.py --mode mock {{args}}

# Automated smoke test used by CI to validate remote manual-test harness
test-manual-remote-smoke:
    ./scripts/manual-test-remote.py --mode mock --smoke

# Run WebUI E2E tests in headed mode (visible browser)
webui-test-headed:
    yarn workspace ah-webui-e2e-tests run test:headed

# Run WebUI E2E tests in debug mode
webui-test-debug:
    yarn workspace ah-webui-e2e-tests run test:debug

# Run WebUI E2E tests in UI mode
webui-test-ui:
    yarn workspace ah-webui-e2e-tests run test:ui

# Show WebUI test reports
webui-test-report:
    yarn workspace ah-webui-e2e-tests run report

# Show failed WebUI tests from the most recent run
webui-test-failed:
    ./scripts/webui-test-failed.sh

# WebUI include patterns as a multiline string
REPOMIX_WEBUI_PATTERNS := replace("""
specs/Public/WebUI-PRD.md
specs/Public/WebUI.status.md
specs/Public/REST-Service.md
specs/Public/REST-Service.status.md
specs/Public/Configuration.md
specs/Public/Agent-Workflow-GUI.md
specs/Public/Schemas/session-events.schema.json
webui/**
""", "\n", ",")

# Create repomix bundle of all WebUI-related files (specs + implementation)
repomix-webui *args:
    mkdir -p {{REPOMIX_OUT_DIR}}
    repomix \
        . \
        --output {{REPOMIX_OUT_DIR}}/Agent-Harbor-WebUI.md \
        --style markdown \
        --header-text "WebUI Complete Implementation and Specification" \
        --include "{{REPOMIX_WEBUI_PATTERNS}}" \
        {{args}}

# Create repomix bundle of all specs files
repomix-specs *args:
    mkdir -p {{REPOMIX_OUT_DIR}}
    repomix specs/ -o {{REPOMIX_OUT_DIR}}/Agent-Harbor-Specs.md --style markdown --header-text "Agent Harbor Specifications" {{args}}

# Create repomix bundle of all Third-Party-Agents files
repomix-agent-questions *args:
    mkdir -p {{REPOMIX_OUT_DIR}}
    repomix \
        specs/Public/3rd-Party-Agents/ \
        --output {{REPOMIX_OUT_DIR}}/Agent-Questions.md \
        --style markdown \
        --header-text "Third-Party Agent Integration Questions and Research" \
        {{args}}

# Create repomix bundle of Multi-OS fleet specs and related status/plans
REPOMIX_MULTI_OS_FLEET_PATTERNS := replace("""
specs/Public/Multi-OS-Testing.md
specs/Public/Multi-OS-Testing.status.md
specs/Public/FS-Snapshots/FS-Snapshots-Overview.md
specs/Public/Agent-Time-Travel.md
specs/Public/Lima-VM-Images.md
specs/Public/Lima-VM-Images.status.md
specs/Public/Executor-Enrollment.md
specs/Public/CLI.md
specs/Public/Repository-Layout.md
specs/Public/Logging-Guidelines.md
specs/Public/Configuration.md
specs/Research/How-to-persistent-SSH-connections.md
specs/Research/Intro-to-Mutagen-Projects.md
specs/Research/Can-SSH-work-over-HTTPS.md
""", "\n", ",")

repomix-multi-os-fleets *args:
    mkdir -p {{REPOMIX_OUT_DIR}}
    repomix \
        . \
        --output {{REPOMIX_OUT_DIR}}/Agent-Harbor-Multi-OS-Fleets.md \
        --style markdown \
        --header-text "Multi-OS Fleets Specifications and Plans" \
        --include "{{REPOMIX_MULTI_OS_FLEET_PATTERNS}}" \
        {{args}}

# Install Playwright browsers for E2E tests
webui-install-browsers:
    yarn workspace ah-webui-e2e-tests run install-browsers

# Run all WebUI checks (lint, type-check, build, test)
webui-check:
    just webui-knip
    just webui-type-check
    just webui-lint
    just webui-type-coverage
    just webui-build
    just webui-build-ssr
    just webui-build-mock
    just webui-test

# Electron GUI Development Targets
# ================================

# Install dependencies for Electron GUI
electron-install:
    yarn workspace agent-harbor-gui install

# Build WebUI in static mode and copy to Electron resources
electron-build-webui:
    @echo "Building WebUI in static mode for Electron..."
    @echo "Note: Using WEBUI_BUILD_MODE=static to build pure CSR (no SSR/hydration)"
    WEBUI_BUILD_MODE=static yarn workspace ah-webui-ssr-sidecar run build:client
    @echo "Copying WebUI client bundle to Electron resources..."
    rm -rf electron-app/resources/webui-static
    mkdir -p electron-app/resources/webui-static
    cp -r webui/app/.vinxi/build/client/_build electron-app/resources/webui-static/
    @echo "Generating index.html with correct asset hashes..."
    ./scripts/generate-electron-index.sh
    @echo "WebUI client bundle ready for Electron"

# Build native addon for Electron
electron-build-native-addon:
    @echo "Building native addon..."
    cd crates/ah-gui-core && npm install && npm run build
    @echo "Copying native addon to node_modules..."
    cp crates/ah-gui-core/*.node node_modules/@agent-harbor/gui-core/ 2>/dev/null || true
    @echo "Native addon ready"

# Run Electron GUI in development mode (with hot reload)
# Note: This expects a WebUI server running on localhost:3001
# Use 'just electron-manual-test' for a full automated setup
electron-dev: electron-build-native-addon
    @echo "⚠️  Make sure a WebUI server is running on http://localhost:3001"
    @echo "   You can start one with: cd webui/mock-server && yarn dev"
    @echo ""
    yarn workspace agent-harbor-gui run dev

# Build Electron GUI for production (includes WebUI static build and native addon)
electron-build: electron-build-webui electron-build-native-addon
    yarn workspace agent-harbor-gui run build

# Build Electron GUI for development (no native addon, no WebUI build)
electron-build-dev:
    yarn workspace agent-harbor-gui run build:dev

# Lint Electron GUI code
electron-lint:
    yarn workspace agent-harbor-gui run lint

# Format Electron GUI code
electron-format:
    yarn workspace agent-harbor-gui run format

# Type check Electron GUI code
electron-type-check:
    yarn workspace agent-harbor-gui run type-check

# Test Electron GUI with Playwright
electron-test: electron-build-webui electron-build-native-addon
    @echo "Building Electron app for testing..."
    yarn workspace agent-harbor-gui run build:dev
    @echo "Running Playwright tests..."
    yarn workspace agent-harbor-gui run test

# Test Electron GUI with Playwright (headed mode for debugging)
electron-test-headed: electron-build-webui electron-build-native-addon
    @echo "Building Electron app for testing..."
    yarn workspace agent-harbor-gui run build:dev
    @echo "Running Playwright tests in headed mode..."
    yarn workspace agent-harbor-gui run test:headed

# Run all Electron GUI checks (lint, type-check)
electron-check:
    just electron-lint
    just electron-type-check

# Build WebUI in CSR mode (static build for Electron/subprocess architecture)
webui-build-csr:
    @echo "Building WebUI in CSR mode..."
    cd webui/app && ./scripts/build-csr.sh

# Manual test: Build CSR, start mock server, launch Electron
manual-test-electron:
    @echo "🔨 Building CSR static files..."
    just webui-build-csr
    @echo "🏗️  Building mock server..."
    cd webui/mock-server && yarn build
    @echo "🔨 Building Electron app..."
    cd electron-app && yarn build:dev
    @echo "🚀 Starting mock server and Electron..."
    @echo "   Mock server: http://localhost:3001"
    @echo "   Press Ctrl+C to stop"
    @echo ""
    @# Start mock server in background, then launch Electron directly
    @# Use 'yarn node' to enable Yarn PnP module resolution for mock server
    @# Launch Electron via helper script that resolves binary path through PnP
    @trap 'kill %1' EXIT; \
      cd webui/mock-server && yarn node dist/index.js & \
      sleep 2 && \
      cd electron-app && yarn node scripts/launch-electron.mjs dist-electron/index.js

# macOS / Xcode Targets
# ====================

# Build the AgentFSKitExtension from adapters directory (release mode)
build-agentfs-extension:
    cd adapters/macos/xcode/AgentFSKitExtension && ./build.sh

# Build the AgentFSKitExtension in debug mode
build-agentfs-extension-debug:
    cd adapters/macos/xcode/AgentFSKitExtension && CONFIGURATION=debug ./build.sh

# Build the AgentFSKitExtension in release mode
build-agentfs-extension-release:
    cd adapters/macos/xcode/AgentFSKitExtension && CONFIGURATION=release ./build.sh

# Build the AgentHarbor Xcode project (includes embedded AgentFSKitExtension)
build-agent-harbor-xcode:
    @echo "🔨 Building AgentHarbor macOS app..."
    just build-agentfs-extension
    cd apps/macos/AgentHarbor && (test -d AgentHarbor.xcodeproj || (echo "❌ Xcode project not found at apps/macos/AgentHarbor/AgentHarbor.xcodeproj" && echo "💡 Run 'just setup-agent-harbor-xcode' to create it" && exit 1))
    cd apps/macos/AgentHarbor && xcodebuild build -project AgentHarbor.xcodeproj -scheme AgentHarbor -configuration Debug -arch x86_64 CODE_SIGN_IDENTITY="" CODE_SIGNING_REQUIRED=NO

# Set up the Xcode project for AgentHarbor (run once after cloning)
setup-agent-harbor-xcode:
    @echo "🔧 Setting up AgentHarbor Xcode project..."
    @echo "Generating Xcode project from project.yml using XcodeGen..."
    cd apps/macos/AgentHarbor && xcodegen generate
    @echo ""
    @echo "✅ Xcode project generated successfully!"
    @echo "You can now open AgentHarbor.xcodeproj in Xcode or run:"
    @echo "  just build-agent-harbor"

# Build the complete AgentHarbor macOS app (debug build for development)
build-agent-harbor:
    just build-agentfs-extension-debug
    @echo "🔨 Building AgentHarbor macOS app with Swift Package Manager (debug)..."
    cd apps/macos/AgentHarbor && swift build --configuration debug
    @echo "📦 Ensuring extension is properly embedded in app bundle..."
    # Swift PM creates the app bundle, we just need to ensure the extension is copied
    mkdir -p "apps/macos/AgentHarbor/.build/arm64-apple-macosx/debug/AgentHarbor.app/Contents/PlugIns"
    cp -R "adapters/macos/xcode/AgentFSKitExtension/AgentFSKitExtension.appex" "apps/macos/AgentHarbor/.build/arm64-apple-macosx/debug/AgentHarbor.app/Contents/PlugIns/"
    @echo "✅ AgentHarbor (debug) built successfully!"

# Build the complete AgentHarbor macOS app (release build)
build-agent-harbor-release:
    just build-agentfs-extension-release
    @echo "🔨 Building AgentHarbor macOS app with Swift Package Manager (release)..."
    cd apps/macos/AgentHarbor && swift build --configuration release
    @echo "📦 Ensuring extension is properly embedded in app bundle..."
    # Swift PM creates the app bundle, we just need to ensure the extension is copied
    mkdir -p "apps/macos/AgentHarbor/.build/arm64-apple-macosx/release/AgentHarbor.app/Contents/PlugIns"
    cp -R "adapters/macos/xcode/AgentFSKitExtension/AgentFSKitExtension.appex" "apps/macos/AgentHarbor/.build/arm64-apple-macosx/release/AgentHarbor.app/Contents/PlugIns/"
    @echo "✅ AgentHarbor (release) built successfully!"

# Test the AgentHarbor macOS app (builds and validates extension)
test-agent-harbor:
    @echo "🧪 Testing AgentHarbor macOS app..."
    just build-agent-harbor-xcode || (echo "⚠️  Xcode build failed (likely environment issue), checking for existing app..." && find ~/Library/Developer/Xcode/DerivedData/AgentHarbor-*/Build/Products/Debug -name "AgentHarbor.app" -type d -exec echo "📱 Using existing built app at {}" \;)

# Launch the debug build of AgentHarbor
launch-agent-harbor-debug:
    @echo "🚀 Launching AgentHarbor (debug build)..."
    open "apps/macos/AgentHarbor/.build/arm64-apple-macosx/debug/AgentHarbor.app"

# Launch the release build of AgentHarbor
launch-agent-harbor-release:
    @echo "🚀 Launching AgentHarbor (release build)..."
    open "apps/macos/AgentHarbor/.build/arm64-apple-macosx/release/AgentHarbor.app"

# TUI include patterns as a multiline string
REPOMIX_TUI_PATTERNS := replace("""
specs/Public/AGENTS.md
specs/Public/TUI-PRD.md
specs/Public/TUI.status.md
specs/Public/TUI-Testing-Architecture.md
specs/Public/REST-Service.md
specs/Public/REST-Service.status.md
specs/Public/Terminal-Multiplexers/TUI-Multiplexers-Overview.md
specs/Public/Terminal-Multiplexers/tmux.md
specs/Public/Terminal-Multiplexers/Kitty.md
specs/Public/Terminal-Multiplexers/Multiplexer-Description-Template.md
crates/ah-rest-api-contract/**
crates/ah-rest-client/**
crates/ah-tui/**
crates/ah-tui-multiplexer/**
crates/ah-tui-test/**
crates/ah-mux-core/**
crates/ah-mux/**
crates/ah-test-scenarios/**
crates/ah-rest-client-mock/**
crates/ah-client-api/**
test_scenarios/**
""", "\n", ",")

# Create repomix bundle of all TUI-related files (specs + implementation)
repomix-tui *args:
    @echo "📦 Creating TUI repomix snapshot..."
    mkdir -p {{REPOMIX_OUT_DIR}}
    repomix \
        . \
        --output {{REPOMIX_OUT_DIR}}/Agent-Harbor-TUI.md \
        --style markdown \
        --header-text "TUI Complete Implementation and Specification" \
        --include "{{REPOMIX_TUI_PATTERNS}}" \
        {{args}}


# AgentFS include patterns as a multiline string
REPOMIX_AGENTFS_PATTERNS := replace("""
specs/Public/AgentFS/**
resources/fskit/**
crates/agentfs-*/**
apps/macos/AgentHarbor/**
adapters/**
crates/ah-cli/src/agent/fs.rs
crates/ah-cli/src/agent/mod.rs
tests/tools/e2e_macos_fskit/**
""", "\n", ",")

# FUSE adapter include patterns as a multiline string
REPOMIX_AGENTFS_FUSE_PATTERNS := replace("""
specs/Public/AgentFS/**
specs/Research/Compiling-and-Testing-FUSE-File-Systems.md
crates/agentfs-core/**
crates/agentfs-proto/**
crates/agentfs-fuse-host/**
crates/ah-cli/src/agent/fs.rs
crates/ah-cli/src/agent/mod.rs
tests/agentfs-daemon-backstore-integration/**
Justfile
flake.nix
.github/workflows/ci.yml
""", "\n", ",")

# AgentFS interpose-specific include patterns as a multiline string
REPOMIX_AGENTFS_INTERPOSE_PATTERNS := replace("""
specs/Public/AgentFS/AgentFS.md
specs/Public/AgentFS/AgentFS-Core.md
specs/Public/AgentFS/AgentFS-Per-process-FS-mounts.md
specs/Public/AgentFS/macOS-FS-Hooks.md
crates/agentfs-core/**
crates/agentfs-proto/**
crates/agentfs-interpose-shim/**
crates/agentfs-interpose-e2e-tests/**
""", "\n", ",")

# Cloud/Browser automation include patterns as a multiline string
REPOMIX_CLOUD_AUTOMATION_PATTERNS := replace("""
specs/Public/Agent-Browsers/Agent-Browser-Profiles.md
specs/Public/Browser-Automation/**
specs/Public/Cloud-Automation.status.md
""", "\n", ",")

# AgentFS specs-only include patterns as a multiline string
REPOMIX_AGENTFS_SPECS_PATTERNS := replace("""
specs/Public/AgentFS/**
specs/Public/FS-Snapshots/FS-Snapshots-Overview.md
specs/Public/Schemas/agentfs-control.request.schema.json
specs/Public/Schemas/agentfs-control.response.schema.json
""", "\n", ",")

# Create repomix bundle of AgentFS-related specs only (no implementation code)
repomix-agentfs-specs *args:
    @echo "📦 Creating AgentFS specs repomix snapshot..."
    mkdir -p {{REPOMIX_OUT_DIR}}
    repomix \
        . \
        --output {{REPOMIX_OUT_DIR}}/Agent-Harbor-AgentFS-Specs.md \
        --style markdown \
        --header-text "AgentFS Specifications and Related Documentation" \
        --include "{{REPOMIX_AGENTFS_SPECS_PATTERNS}}" \
        {{args}}

# Create a repomix snapshot of all AgentFS-related files (specs and implementation)
repomix-agentfs *args:
    @echo "📦 Creating AgentFS repomix snapshot..."
    mkdir -p {{REPOMIX_OUT_DIR}}
    repomix \
        . \
        --output {{REPOMIX_OUT_DIR}}/Agent-Harbor-AgentFS.md \
        --style markdown \
        --header-text "AgentFS Complete Implementation and Specification" \
        --include "{{REPOMIX_AGENTFS_PATTERNS}}" \
        {{args}}

# Create repomix bundle of AgentFS interpose-specific files (core, proto, shim, server)
repomix-agentfs-interpose:
    @echo "📦 Creating AgentFS interpose repomix snapshot..."
    mkdir -p {{REPOMIX_OUT_DIR}}
    repomix \
        . \
        --output {{REPOMIX_OUT_DIR}}/AgentFS-Interpose.md \
        --style markdown \
        --header-text "AgentFS Interpose Implementation (Core, Proto, Shim, Server)" \
        --include "{{REPOMIX_AGENTFS_INTERPOSE_PATTERNS}}"

# Create repomix bundle of browser automation plans/specs
repomix-cloud-automation *args:
    @echo "📦 Creating Cloud/Browser Automation repomix snapshot..."
    mkdir -p {{REPOMIX_OUT_DIR}}
    repomix \
        . \
        --output {{REPOMIX_OUT_DIR}}/Agent-Harbor-Cloud-Automation.md \
        --style markdown \
        --header-text "Cloud & Browser Automation Specifications and Status" \
        --include "{{REPOMIX_CLOUD_AUTOMATION_PATTERNS}}" \
        {{args}}

# Record/Replay include patterns as a multiline string
REPOMIX_RECORD_REPLAY_PATTERNS := replace("""
specs/Public/ah-agent-record.md
crates/ah-recorder/**
crates/ah-cli/src/agent/record.rs
crates/ah-cli/src/agent/replay.rs
""", "\n", ",")

# Create repomix bundle of all record/replay functionality (spec + implementation)
repomix-record-replay:
    @echo "📦 Creating Record/Replay repomix snapshot..."
    mkdir -p {{REPOMIX_OUT_DIR}}
    repomix \
        . \
        --output {{REPOMIX_OUT_DIR}}/Agent-Harbor-Record-Replay.md \
        --style markdown \
        --header-text "Agent Harbor Record/Replay - Complete Implementation and Specification" \
        --include "{{REPOMIX_RECORD_REPLAY_PATTERNS}}" \
        --ignore ".direnv/**"

# Create a repomix snapshot of the LLM API Proxy crate
repomix-llm-api-proxy *args:
    @echo "📦 Creating LLM API Proxy repomix snapshot..."
    mkdir -p {{REPOMIX_OUT_DIR}}
    repomix \
        . \
        --output {{REPOMIX_OUT_DIR}}/Agent-Harbor-LLM-API-Proxy.md \
        --style markdown \
        --header-text "LLM API Proxy - Complete Implementation and Specification" \
        --include "crates/llm-api-proxy/**,specs/Public/Scenario-Format.md" \
        {{args}}

# Create repomix bundle of all FUSE adapter-related files (specs + implementation)
repomix-agentfs-fuse *args:
    @echo "📦 Creating AgentFS FUSE adapter repomix snapshot..."
    mkdir -p {{REPOMIX_OUT_DIR}}
    repomix \
        . \
        --output {{REPOMIX_OUT_DIR}}/AgentFS-FUSE-Adapter.md \
        --style markdown \
        --header-text "AgentFS FUSE Adapter: Complete Implementation and Specification" \
        --include "{{REPOMIX_AGENTFS_FUSE_PATTERNS}}" \
        {{args}}

# Run overlay tests with E2E enforcement verification
test-overlays:
    just build-overlay-tests
    cargo test -p sandbox-integration-tests --verbose

test-mock-agent-simple:
    cd tests/tools/mock-agent && python3 tests/test_agent_simple.py

# Run mock-agent integration tests
test-mock-agent-integration:
    cd tests/tools/mock-agent && python3 tests/test_agent_integration.py

# Replay mock-agent session recordings (shows menu)
replay-mock-agent-sessions:
    tests/tools/mock-agent/replay-recording.sh

# Replay the most recent mock-agent session recording
replay-last-mock-agent-session:
    tests/tools/mock-agent/replay-recording.sh --latest

# Clear all mock-agent session recordings
clear-mock-agent-recordings:
    rm -rf tests/tools/mock-agent/recordings/*.json

# Run ACP mock-agent demo (scenario overridable)
run-mock-agent-acp scenario="tests/tools/mock-agent-acp/scenarios/acp_echo.yaml":
    ./tests/tools/mock-agent-acp/run.sh {{scenario}}

# Run Agent Activity TUI mock mode (driven by mock-agent transcript)
run-mock-agent-session scenario="tests/tools/mock-agent-acp/scenarios/acp_realistic_session.yaml":
    cargo run -p ah-tui --bin mock_agent_session -- --scenario {{scenario}}

# Quick ACP client+agent smoke (non-interactive; feeds EOF to stdin)
run-mock-agent-acp-smoke:
    printf '' | cargo run -p mock-agent --example acp_client -- --scenario tests/tools/mock-agent-acp/scenarios/acp_echo.yaml --prompt "smoke echo"
    printf '' | cargo run -p mock-agent --example acp_client -- --scenario tests/tools/mock-agent-acp/scenarios/acp_loadsession_meta.yaml --session-id ls-meta --prompt "load session smoke"

# Run network tests with E2E enforcement verification
test-networks:
    just build-network-tests
    cargo test -p sandbox-integration-tests --verbose

# Run debugging enforcement tests with E2E verification
test-debugging:
    just build-debugging-tests
    cargo test -p sandbox-integration-tests --verbose
    ./target/debug/debugging_test_orchestrator

# Build VM enforcement test binaries (qemu_vm_tester, kvm_device_tester, vm_test_orchestrator)
build-vm-test-binaries:
    cargo build -p vm-enforcement --bin qemu_vm_tester --bin kvm_device_tester --bin vm_test_orchestrator

# Build all test binaries needed for VM enforcement tests
build-vm-tests: build-sbx-helper build-vm-test-binaries

# Run VM tests with E2E enforcement verification
test-vms:
    just build-vm-tests
    ./target/debug/vm_test_orchestrator

# Build container enforcement test binaries (podman_container_tester, container_resource_tester, docker_socket_tester, container_test_orchestrator)
build-container-test-binaries:
    cargo build -p container-enforcement --bin podman_container_tester --bin container_resource_tester --bin docker_socket_tester --bin container_test_orchestrator

# Build all test binaries needed for container enforcement tests
build-container-tests: build-sbx-helper build-container-test-binaries

# Run container tests with E2E enforcement verification
test-containers:
    just build-container-tests
    ./target/debug/container_test_orchestrator

# Run simple mount test to verify CAP_SYS_ADMIN availability in user namespaces
test-mount-capability:
    just build-debugging-test-binaries
    ./target/debug/mount_test

regen-ansi-logo:
    chafa --format=symbols --view-size=80x50 assets/agent-harbor-logo.png | tee assets/agent-harbor-logo-80.ansi

# macOS FSKit E2E (requires SIP/AMFI disabled)
verify-macos-fskit-prereqs:
    bash scripts/verify-macos-fskit-prereqs.sh

e2e-fskit:
    bash scripts/e2e-fskit.sh

# macOS FSKit provisioning helpers (refer to Research doc)
install-agent-harbor-app:
    bash scripts/install-agent-harbor-app.sh

systemextensions-devmode-and-status:
    bash scripts/systemextensions-devmode-and-status.sh

register-fskit-extension:
    bash scripts/register-fskit-extension.sh

# Download macOS sandbox documentation
download-mac-sandbox-docs:
    nix run .#sosumi-docs-downloader -- fskit endpointsecurity -o resources

# Run a program with MITM proxy capturing all HTTP(S) traffic
# Usage: just mitm claude
#        just mitm curl https://api.anthropic.com/v1/models
mitm *args:
    ./scripts/with_mitmproxy.py {{args}}

# Check for outdated dependencies
outdated:
    cargo outdated
    yarn outdated

# Command Tracing include patterns as a multiline string
REPOMIX_COMMAND_TRACING_PATTERNS := replace("""
specs/Public/R9.status.md
specs/Public/R9.md
crates/ah-command-trace-shim/**
crates/ah-command-trace-client/**
crates/ah-command-trace-proto/**
crates/ah-command-trace-e2e-tests/**
""", "\n", ",")

# Create repomix bundle of all command tracing functionality (specs + implementation)
repomix-command-tracing *args:
    @echo "📦 Creating Command Tracing repomix snapshot..."
    mkdir -p {{REPOMIX_OUT_DIR}}
    repomix \
        . \
        --output {{REPOMIX_OUT_DIR}}/Agent-Harbor-Command-Tracing.md \
        --style markdown \
        --header-text "Command Tracing - Complete Implementation and Specification" \
        --include "{{REPOMIX_COMMAND_TRACING_PATTERNS}}" \
        {{args}}

# Inspect AHR recording files
# Usage: just inspect-ahr <path/to/recording.ahr>
inspect-ahr *args:
    @cargo build --quiet --bin inspect_ahr --package ah-recorder --message-format=json \
      | jq -c 'select(.reason=="compiler-message" and .message.level=="error")' \
      # The above shows only build errors from cargo
    @./target/debug/inspect_ahr {{args}}

# Run Proof-of-Concept usage limits verification scripts
# Shows current usage limits for AI coding assistants (Claude, Codex, Cursor, Replit)
poc-show-usage-limits:
    @python PoC/agent-limits/claude_usage_verifier.py
    @python PoC/agent-limits/codex_usage_verifier.py
    @CURSOR_AUTH_TOKEN="$(cat PoC/agent-limits/cursor_token.txt 2>/dev/null || echo '')" python PoC/agent-limits/cursor_usage_verifier.py
    @python PoC/agent-limits/replit_usage_verifier.py
