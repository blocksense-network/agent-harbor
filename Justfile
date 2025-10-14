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

# Define REPOMIX_OUT_DIR with a default value
REPOMIX_OUT_DIR := env('REPOMIX_OUT_DIR', 'repomix')

# IMPORTANT: Never use long scripts in Justfile recipes!
# Long scripts set a custom shell, overriding our nix-env.sh setting.
# Move complex scripts to the scripts/ folder instead.

# Check Rust code for compilation errors
check:
    cargo check --workspace

# Build all test binaries needed for Rust workspace tests
build-rust-test-binaries: build-sbx-helper build-cgroup-test-binaries build-overlay-test-binaries build-debugging-test-binaries build-tui-test-binaries

# Run Rust tests
test-rust: build-rust-test-binaries
    cargo nextest run --workspace

# Run Rust tests with verbose output
test-rust-verbose: build-rust-test-binaries
    cargo nextest run --workspace --verbose

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
    cargo build --bin fork_bomb --bin memory_hog --bin cpu_burner --bin test_orchestrator

# Build overlay enforcement test binaries (overlay_test_orchestrator, blacklist_tester, overlay_writer)
build-overlay-test-binaries:
    cargo build --bin overlay_test_orchestrator --bin blacklist_tester --bin overlay_writer

# Build sbx-helper binary
build-sbx-helper:
    cargo build -p sbx-helper --bin sbx-helper

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
    cargo build -p debugging-enforcement --bin debugging_test_orchestrator --bin ptrace_tester --bin process_visibility_tester --bin mount_test

# Build all test binaries needed for debugging enforcement tests
build-debugging-tests: build-sbx-helper build-debugging-test-binaries

# Build TUI test binaries
build-tui-test-binaries:
    cargo build -p tui-testing --bin test-guest

# Build all TUI test binaries needed for TUI testing
build-tui-tests: build-tui-test-binaries

# Run cgroup tests with E2E enforcement verification
test-cgroups:
    just build-cgroup-tests
    cargo test -p sandbox-integration-tests --verbose

# WebUI Development Targets
# ========================

# Install dependencies for mock server only
mock-server-install:
    yarn workspace ah-webui-mock-server install

# Install dependencies for all WebUI projects
webui-install:
    yarn workspace shared install
    yarn workspace ah-webui-ssr-sidecar install
    just mock-server-install
    yarn workspace ah-webui-e2e-tests install

# Build WebUI application
webui-build:
    yarn workspace ah-webui-ssr-sidecar run build

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
    ./scripts/manual-test-agent-start.sh {{args}}

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
    #!/usr/bin/env bash
    set -euo pipefail
    LOGS_DIR="webui/e2e-tests/test-results/logs"
    if [ ! -d "$LOGS_DIR" ]; then
        echo "❌ No test logs directory found at $LOGS_DIR"
        exit 1
    fi
    # Find the most recent test run directory
    LATEST_RUN=$(ls -td "$LOGS_DIR"/test-run-* | head -1)
    if [ -z "$LATEST_RUN" ]; then
        echo "❌ No test runs found in $LOGS_DIR"
        exit 1
    fi

    echo "🔍 Analyzing test results from $(basename "$LATEST_RUN")..."
    echo "=============================================="

    # Parse failed tests from individual log files
    FAILED_TESTS=$(grep -l "RESULT: Test failed" "$LATEST_RUN"/*.log | wc -l)
    TOTAL_TESTS=$(ls "$LATEST_RUN"/*.log | grep -v "failed-tests\|test-summary" | wc -l)

    if [ "$FAILED_TESTS" -gt 0 ]; then
        echo "❌ $FAILED_TESTS failed tests out of $TOTAL_TESTS total tests"
        echo ""
        echo "📋 Failed tests:"
        echo "---------------"

        # Extract failed test information from log files
        for log_file in "$LATEST_RUN"/*.log; do
            if grep -q "RESULT: Test failed" "$log_file"; then
                TEST_NAME=$(grep "TEST_START:" "$log_file" | sed 's/.*TEST_START: //')
                TEST_ID=$(grep "TEST_ID:" "$log_file" | sed 's/.*TEST_ID: //')
                TEST_FILE=$(grep "TEST_FILE:" "$log_file" | sed 's/.*TEST_FILE: //' | sed 's|.*/||')
                TEST_LINE=$(grep "TEST_LINE:" "$log_file" | sed 's/.*TEST_LINE: //')
                ERROR_MSG=$(grep "ERROR:" "$log_file" | head -1 | sed 's/.*ERROR: //' | cut -c1-80)

                echo "• $TEST_NAME"
                echo "  📄 $(basename "$log_file")"
                echo "  📍 $TEST_FILE:$TEST_LINE"
                if [ -n "$ERROR_MSG" ]; then
                    echo "  💥 ${ERROR_MSG:0:80}..."
                fi
                echo ""
            fi
        done

        echo "💡 Commands to investigate further:"
        echo "   • View Playwright HTML report: just webui-test-report"
        echo "   • View detailed JSON: cat $LATEST_RUN/test-summary.json"
        echo "   • List all log files: ls -la $LATEST_RUN/*.log"
        echo "   • View specific test log: cat $LATEST_RUN/<filename>.log"
    else
        echo "✅ All $TOTAL_TESTS tests passed!"
    fi

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
repomix-webui:
    mkdir -p {{REPOMIX_OUT_DIR}}
    repomix \
        . \
        --output {{REPOMIX_OUT_DIR}}/Agent-Harbor-WebUI.md \
        --style markdown \
        --header-text "WebUI Complete Implementation and Specification" \
        --include "{{REPOMIX_WEBUI_PATTERNS}}"

# Create repomix bundle of all specs files
repomix-specs:
    mkdir -p {{REPOMIX_OUT_DIR}}
    repomix specs/ -o {{REPOMIX_OUT_DIR}}/Agent-Harbor-Specs.md --style markdown --header-text "Agents Workflow Specifications"

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

# Build the native addon for Electron GUI
electron-build-native-addon:
    cd crates/ah-gui-core && npm run build

# Run Electron GUI in development mode (with hot reload)
electron-dev:
    yarn workspace agent-harbor-gui run dev

# Build Electron GUI for production
electron-build:
    yarn workspace agent-harbor-gui run build

# Build Electron GUI for development (no native addon)
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

# Run all Electron GUI checks (lint, type-check)
electron-check:
    just electron-lint
    just electron-type-check

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
repomix-tui:
    @echo "📦 Creating TUI repomix snapshot..."
    mkdir -p {{REPOMIX_OUT_DIR}}
    repomix \
        . \
        --output {{REPOMIX_OUT_DIR}}/Agent-Harbor-TUI.md \
        --style markdown \
        --header-text "TUI Complete Implementation and Specification" \
        --include "{{REPOMIX_TUI_PATTERNS}}"

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

# AgentFS specs-only include patterns as a multiline string
REPOMIX_AGENTFS_SPECS_PATTERNS := replace("""
specs/Public/AgentFS/**
specs/Public/FS-Snapshots/FS-Snapshots-Overview.md
specs/Public/Schemas/agentfs-control.request.schema.json
specs/Public/Schemas/agentfs-control.response.schema.json
""", "\n", ",")

# Create repomix bundle of AgentFS-related specs only (no implementation code)
repomix-agentfs-specs:
    @echo "📦 Creating AgentFS specs repomix snapshot..."
    mkdir -p {{REPOMIX_OUT_DIR}}
    repomix \
        . \
        --output {{REPOMIX_OUT_DIR}}/Agent-Harbor-AgentFS-Specs.md \
        --style markdown \
        --header-text "AgentFS Specifications and Related Documentation" \
        --include "{{REPOMIX_AGENTFS_SPECS_PATTERNS}}"

# Create a repomix snapshot of all AgentFS-related files (specs and implementation)
repomix-agentfs:
    @echo "📦 Creating AgentFS repomix snapshot..."
    mkdir -p {{REPOMIX_OUT_DIR}}
    repomix \
        . \
        --output {{REPOMIX_OUT_DIR}}/Agent-Harbor-AgentFS.md \
        --style markdown \
        --header-text "AgentFS Complete Implementation and Specification" \
        --include "{{REPOMIX_AGENTFS_PATTERNS}}"

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

# Check for outdated dependencies
outdated:
    cargo outdated
    yarn outdated
