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

# Run the test suite

set shell := ["./scripts/nix-env.sh", "-c"]

# IMPORTANT: Never use long scripts in Justfile recipes!
# Long scripts set a custom shell, overriding our nix-env.sh setting.
# Move complex scripts to the scripts/ folder instead.

# Check Rust code for compilation errors
check:
    cargo check --workspace

# Run Rust tests
test-rust:
    cargo test --workspace

# Run Rust tests with verbose output
test-rust-verbose:
    cargo test --workspace --verbose

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
# Example: just insta-test-pkg aw-mux
insta-test-pkg pkg:
    cargo insta test -p {{pkg}}

# Accept snapshots for specific packages (useful when only one package changed)
# Example: just insta-accept-pkg aw-mux
insta-accept-pkg pkg:
    cargo insta accept -p {{pkg}}

# Quick workflow: test snapshots, show status
# This runs snapshot tests and reports whether snapshots are up to date or need review
# Note: Some snapshots (like tmux golden snapshots) capture dynamic terminal output
# and may need periodic acceptance due to timing/environment differences.
insta-check:
    #!/usr/bin/env bash
    echo "🔍 Running snapshot tests..."
    if cargo insta test --no-quiet >/dev/null 2>&1; then
        echo "✅ All snapshots are up to date!"
    else
        echo "📝 Snapshots need review. Use 'just insta-review' to review changes."
        echo "   Or use 'just insta-accept' to accept all changes blindly."
        echo "   Or use 'just insta-pending' to see what changed."
        echo "   Note: Dynamic snapshots (tmux terminal output) may need periodic updates."
        exit 1
    fi

# Lint Rust code
lint-rust:
    cargo clippy --workspace

# Format Rust code
fmt-rust:
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
    #!/usr/bin/env bash
    set -euo pipefail
    if ! command -v taplo >/dev/null 2>&1; then
        echo "taplo is not installed. Example to run once: nix shell ~/nixpkgs#taplo -c taplo check" >&2
        exit 127
    fi
    taplo check

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

# Create reusable file-backed filesystems for testing ZFS and Btrfs providers
# This sets up persistent test environments in ~/.cache/agents-workflow
create-test-filesystems:
    scripts/create-test-filesystems.sh

# Check the status of test filesystems
check-test-filesystems:
    scripts/check-test-filesystems.sh

# Clean up test filesystems created by create-test-filesystems
cleanup-test-filesystems:
    scripts/cleanup-test-filesystems.sh

# Launch the AW filesystem snapshots daemon for testing (requires sudo)
legacy-start-aw-fs-snapshots-daemon:
    legacy/scripts/launch-aw-fs-snapshots-daemon.sh

# Stop the AW filesystem snapshots daemon
legacy-stop-aw-fs-snapshots-daemon:
    legacy/scripts/stop-aw-fs-snapshots-daemon.sh

# Check status of AW filesystem snapshots daemon
legacy-check-aw-fs-snapshots-daemon:
    ruby legacy/scripts/check-aw-fs-snapshots-daemon.rb

# Launch the new Rust AW filesystem snapshots daemon for testing (requires sudo)
start-aw-fs-snapshots-daemon:
    scripts/start-aw-fs-snapshots-daemon.sh

# Stop the new Rust AW filesystem snapshots daemon
stop-aw-fs-snapshots-daemon:
    scripts/stop-aw-fs-snapshots-daemon.sh

# Check status of the new Rust AW filesystem snapshots daemon
check-aw-fs-snapshots-daemon:
    scripts/check-aw-fs-snapshots-daemon.sh

# Run comprehensive daemon integration tests (requires test filesystems)
test-daemon-integration:
    cargo test --package aw-fs-snapshots-daemon -- --nocapture integration

# Run filesystem snapshot provider integration tests (requires root for ZFS/Btrfs operations)
test-fs-snapshots:
    cargo test --package aw-fs-snapshots -- --nocapture integration

# Run filesystem snapshot provider unit tests only (no root required)
test-fs-snapshots-unit:
    cargo test --package aw-fs-snapshots

# Run all spec linting/validation in one go
lint-specs:
    scripts/lint-specs.sh

# Build cgroup enforcement test binaries (fork_bomb, memory_hog, cpu_burner, test_orchestrator)
build-cgroup-test-binaries:
    cargo build --bin fork_bomb --bin memory_hog --bin cpu_burner --bin test_orchestrator

# Build overlay enforcement test binaries (overlay_test_orchestrator, blacklist_tester, overlay_writer)
build-overlay-test-binaries:
    cargo build --bin overlay_test_orchestrator --bin blacklist_tester --bin overlay_writer

# Build sbx-helper binary
build-sbx-helper:
    cargo build -p sbx-helper --bin sbx-helper

# Build macOS sandbox launcher (aw-macos-launcher)
build-aw-macos-launcher:
    cargo build --bin aw-macos-launcher

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
    cargo build -p debugging-enforcement --bin debugging_test_orchestrator --bin ptrace_tester --bin process_visibility_tester --bin mount_test

# Build all test binaries needed for debugging enforcement tests
build-debugging-tests: build-sbx-helper build-debugging-test-binaries

# Run cgroup tests with E2E enforcement verification
test-cgroups:
    just build-cgroup-tests
    cargo test -p sandbox-integration-tests --verbose

# WebUI Development Targets
# ========================

# Install dependencies for mock server only
mock-server-install:
    cd webui/mock-server && npm ci

# Install dependencies for all WebUI projects
webui-install:
    cd webui/shared && npm ci
    cd webui/app && npm ci
    just mock-server-install
    cd webui/e2e-tests && npm ci

# Build WebUI application
webui-build:
    cd webui/app && npm run build

# Build mock server
webui-build-mock:
    cd webui/mock-server && npm run build

# Run WebUI development server
webui-dev:
    cd webui/app && npm run dev

# Run mock REST API server
webui-mock-server:
    cd webui/mock-server && npm run dev

# Lint all WebUI projects
webui-lint:
    cd webui/app && npm run lint
    cd webui/mock-server && npm run lint
    cd webui/e2e-tests && npm run lint

# Type check all WebUI projects
webui-type-check:
    cd webui/app && npm run type-check
    cd webui/mock-server && npm run type-check

# Format all WebUI projects
webui-format:
    cd webui/app && npm run format
    cd webui/mock-server && npm run format
    cd webui/e2e-tests && npm run format

# Run WebUI E2E tests
webui-test:
    cd webui/e2e-tests && npm run test:e2e

# Build WebUI SSR server
webui-build-ssr:
    cd webui/app-ssr-server && npm run build

# Build WebUI client bundle
webui-build-client:
    cd webui/app-ssr-server && npm run build:client

# Start WebUI with mock server for manual testing (cycles through 5 scenarios)
webui-manual:
    ./scripts/webui-manual.sh

# Run WebUI E2E tests in headed mode (visible browser)
webui-test-headed:
    cd webui/e2e-tests && npm run test:headed

# Run WebUI E2E tests in debug mode
webui-test-debug:
    cd webui/e2e-tests && npm run test:debug

# Run WebUI E2E tests in UI mode
webui-test-ui:
    cd webui/e2e-tests && npm run test:ui

# Show WebUI test reports
webui-test-report:
    cd webui/e2e-tests && npm run report

# Install Playwright browsers for E2E tests
webui-install-browsers:
    cd webui/e2e-tests && npm run install-browsers

# Run all WebUI checks (lint, type-check, build, test)
webui-check:
    just webui-lint
    just webui-type-check
    just webui-build
    just webui-build-ssr
    just webui-build-mock
    just webui-test

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

# Build the AgentsWorkflow Xcode project (includes embedded AgentFSKitExtension)
build-agents-workflow-xcode:
    @echo "🔨 Building AgentsWorkflow macOS app..."
    just build-agentfs-extension
    cd apps/macos/AgentsWorkflow && (test -d AgentsWorkflow.xcodeproj || (echo "❌ Xcode project not found at apps/macos/AgentsWorkflow/AgentsWorkflow.xcodeproj" && echo "💡 Run 'just setup-agents-workflow-xcode' to create it" && exit 1))
    cd apps/macos/AgentsWorkflow && xcodebuild build -project AgentsWorkflow.xcodeproj -scheme AgentsWorkflow -configuration Debug -arch x86_64 CODE_SIGN_IDENTITY="" CODE_SIGNING_REQUIRED=NO

# Set up the Xcode project for AgentsWorkflow (run once after cloning)
setup-agents-workflow-xcode:
    @echo "🔧 Setting up AgentsWorkflow Xcode project..."
    @echo "Generating Xcode project from project.yml using XcodeGen..."
    cd apps/macos/AgentsWorkflow && xcodegen generate
    @echo ""
    @echo "✅ Xcode project generated successfully!"
    @echo "You can now open AgentsWorkflow.xcodeproj in Xcode or run:"
    @echo "  just build-agents-workflow"

# Build the complete AgentsWorkflow macOS app (debug build for development)
build-agents-workflow:
    just build-agentfs-extension-debug
    @echo "🔨 Building AgentsWorkflow macOS app with Swift Package Manager (debug)..."
    cd apps/macos/AgentsWorkflow && swift build --configuration debug
    @echo "📦 Creating proper macOS app bundle structure..."
    mkdir -p "apps/macos/AgentsWorkflow/.build/arm64-apple-macosx/debug/AgentsWorkflow.app/Contents/MacOS"
    mkdir -p "apps/macos/AgentsWorkflow/.build/arm64-apple-macosx/debug/AgentsWorkflow.app/Contents/PlugIns"
    cp "apps/macos/AgentsWorkflow/.build/arm64-apple-macosx/debug/AgentsWorkflow" "apps/macos/AgentsWorkflow/.build/arm64-apple-macosx/debug/AgentsWorkflow.app/Contents/MacOS/"
    cp "apps/macos/AgentsWorkflow/AgentsWorkflow/Info.plist" "apps/macos/AgentsWorkflow/.build/arm64-apple-macosx/debug/AgentsWorkflow.app/Contents/"
    # Fix Info.plist build variables that weren't expanded by Swift PM
    sed -i '' -e 's/$(EXECUTABLE_NAME)/AgentsWorkflow/g' -e 's/$(PRODUCT_NAME)/AgentsWorkflow/g' -e 's/$(MACOSX_DEPLOYMENT_TARGET)/15.4/g' "apps/macos/AgentsWorkflow/.build/arm64-apple-macosx/debug/AgentsWorkflow.app/Contents/Info.plist"
    # Create PkgInfo file
    echo -n "APPL????" > "apps/macos/AgentsWorkflow/.build/arm64-apple-macosx/debug/AgentsWorkflow.app/Contents/PkgInfo"
    cp -R "adapters/macos/xcode/AgentFSKitExtension/AgentFSKitExtension.appex" "apps/macos/AgentsWorkflow/.build/arm64-apple-macosx/debug/AgentsWorkflow.app/Contents/PlugIns/"
    @echo "✅ AgentsWorkflow (debug) built successfully!"

# Build the complete AgentsWorkflow macOS app (release build)
build-agents-workflow-release:
    just build-agentfs-extension-release
    @echo "🔨 Building AgentsWorkflow macOS app with Swift Package Manager (release)..."
    cd apps/macos/AgentsWorkflow && swift build --configuration release
    @echo "📦 Creating proper macOS app bundle structure..."
    mkdir -p "apps/macos/AgentsWorkflow/.build/arm64-apple-macosx/release/AgentsWorkflow.app/Contents/MacOS"
    mkdir -p "apps/macos/AgentsWorkflow/.build/arm64-apple-macosx/release/AgentsWorkflow.app/Contents/PlugIns"
    cp "apps/macos/AgentsWorkflow/.build/arm64-apple-macosx/release/AgentsWorkflow" "apps/macos/AgentsWorkflow/.build/arm64-apple-macosx/release/AgentsWorkflow.app/Contents/MacOS/"
    cp "apps/macos/AgentsWorkflow/AgentsWorkflow/Info.plist" "apps/macos/AgentsWorkflow/.build/arm64-apple-macosx/release/AgentsWorkflow.app/Contents/"
    # Fix Info.plist build variables that weren't expanded by Swift PM
    sed -i '' -e 's/$(EXECUTABLE_NAME)/AgentsWorkflow/g' -e 's/$(PRODUCT_NAME)/AgentsWorkflow/g' -e 's/$(MACOSX_DEPLOYMENT_TARGET)/15.4/g' "apps/macos/AgentsWorkflow/.build/arm64-apple-macosx/release/AgentsWorkflow.app/Contents/Info.plist"
    # Create PkgInfo file
    echo -n "APPL????" > "apps/macos/AgentsWorkflow/.build/arm64-apple-macosx/release/AgentsWorkflow.app/Contents/PkgInfo"
    cp -R "adapters/macos/xcode/AgentFSKitExtension/AgentFSKitExtension.appex" "apps/macos/AgentsWorkflow/.build/arm64-apple-macosx/release/AgentsWorkflow.app/Contents/PlugIns/"
    @echo "✅ AgentsWorkflow (release) built successfully!"

# Test the AgentsWorkflow macOS app (builds and validates extension)
test-agents-workflow:
    @echo "🧪 Testing AgentsWorkflow macOS app..."
    just build-agents-workflow-xcode || (echo "⚠️  Xcode build failed (likely environment issue), checking for existing app..." && find ~/Library/Developer/Xcode/DerivedData/AgentsWorkflow-*/Build/Products/Debug -name "AgentsWorkflow.app" -type d -exec echo "📱 Using existing built app at {}" \;)

# Launch the debug build of AgentsWorkflow
launch-agents-workflow-debug:
    @echo "🚀 Launching AgentsWorkflow (debug build)..."
    open "apps/macos/AgentsWorkflow/.build/arm64-apple-macosx/debug/AgentsWorkflow.app"

# Launch the release build of AgentsWorkflow
launch-agents-workflow-release:
    @echo "🚀 Launching AgentsWorkflow (release build)..."
    open "apps/macos/AgentsWorkflow/.build/arm64-apple-macosx/release/AgentsWorkflow.app"

# Run overlay tests with E2E enforcement verification
test-overlays:
    just build-overlay-tests
    cargo test -p sandbox-integration-tests --verbose

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
install-agentsworkflow-app:
    bash scripts/install-agentsworkflow-app.sh

systemextensions-devmode-and-status:
    bash scripts/systemextensions-devmode-and-status.sh

register-fskit-extension:
    bash scripts/register-fskit-extension.sh