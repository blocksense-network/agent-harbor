#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

# Cleanup function for graceful exit
cleanup() {
  echo "Cleaning up servers..."
  if [ -n "${MOCK_PID:-}" ]; then kill $MOCK_PID 2>/dev/null || true; fi
  if [ -n "${SSR_PID:-}" ]; then kill $SSR_PID 2>/dev/null || true; fi
  echo "Server log files preserved at: $TEST_RUN_DIR"
  ls -la "$TEST_RUN_DIR"/*server.log 2>/dev/null || echo "No server log files found"
}
trap cleanup EXIT

# Create timestamp for this test run
export TEST_RUN_TIMESTAMP=$(date +"%Y-%m-%dT%H-%M-%S")
export TEST_RUN_DIR="$(pwd)/test-results/logs/test-run-${TEST_RUN_TIMESTAMP}"
mkdir -p "$TEST_RUN_DIR"

# Kill any existing server processes
echo "Killing any existing server processes..."
pkill -f "yarn.*dev" || true
sleep 2

# Build and start mock server (port 3001) with quiet mode
(
  yarn workspace ah-webui-mock-server run build
  echo "Starting mock server..."
  QUIET_MODE=true yarn workspace ah-webui-mock-server run dev
) >"$TEST_RUN_DIR/mock-server.log" 2>&1 &
MOCK_PID=$!
echo "Started mock server (PID: $MOCK_PID)"

# Build and start SSR sidecar (port 3002) with quiet mode
(
  yarn workspace ah-webui-ssr-sidecar run build
  # Export PORT to ensure it's available to all child processes
  export PORT=3002
  echo "Starting SSR server..."
  QUIET_MODE=true yarn workspace ah-webui-ssr-sidecar run dev
) >"$TEST_RUN_DIR/ssr-server.log" 2>&1 &
SSR_PID=$!
echo "Started SSR server (PID: $SSR_PID)"

# Wait for ports to be ready (using nc from Nix)
echo "Waiting for servers to be ready..."
for i in {1..30}; do
  if nc -z localhost 3001 && nc -z localhost 3002; then
    echo "Servers ready!"
    break
  fi
  sleep 1
done

if ! nc -z localhost 3001 || ! nc -z localhost 3002; then
  echo "Timeout: Servers not ready"
  echo "Mock server logs:"
  head -20 "$TEST_RUN_DIR/mock-server.log" || echo "No mock server logs available"
  echo "SSR server logs:"
  head -20 "$TEST_RUN_DIR/ssr-server.log" || echo "No SSR server logs available"
  exit 1
fi

# Run tests (back in e2e-tests dir)
cd ../e2e-tests

# Ensure test results directories exist for reporters
echo "Creating test-results/logs directory..."
mkdir -p test-results/logs
echo "Directory created."

# Export test run directory for reporters to access
export TEST_RUN_DIR="$TEST_RUN_DIR"

# Playwright browsers: Use Nix-provided browsers if available, otherwise fall back to local
# The Nix flake sets PLAYWRIGHT_BROWSERS_PATH and PLAYWRIGHT_CHROMIUM_EXECUTABLE
# If these are not set (e.g., running outside Nix shell), use local browsers
if [ -z "${PLAYWRIGHT_BROWSERS_PATH:-}" ]; then
  export PLAYWRIGHT_BROWSERS_PATH="$(pwd)/browsers"
  echo "⚠️  Warning: PLAYWRIGHT_BROWSERS_PATH not set - using local browsers"
  echo "   For reproducible tests, run inside Nix dev shell (direnv allow)"
fi

echo "🧪 Running tests... (server logs captured to $TEST_RUN_DIR)"
echo "   📄 View server logs: cat $TEST_RUN_DIR/*.log"
echo ""

yarn test "$@"

# Cleanup happens via trap
