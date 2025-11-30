#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only
#
# T15.1 CLI Parity Harness
#
# This script validates that `ah agent fs` commands produce output compatible
# with the reference `agentfs-control-cli` implementation. It tests:
# - snapshot list
# - branch create
# - branch bind
#
# Prerequisites:
# - AgentFS FUSE filesystem mounted at $AGENTFS_MOUNT (default: /tmp/agentfs)
# - Start the daemon with: just start-ah-fs-snapshots-daemon
# - Build with: `cargo build --release -p ah-cli -p agentfs-control-cli --features agentfs`
#
# Usage:
#   ./scripts/test-agentfs-cli-control-plane.sh [--mount /path/to/mount]
#
# Note: The daemon spawns the FUSE host as the requesting user, so no sudo or
# --allow-other is required for control file access.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

# Configuration
AGENTFS_MOUNT="${AGENTFS_MOUNT:-/tmp/agentfs}"
LOG_DIR="${PROJECT_ROOT}/logs/cli-parity-$(date +%Y%m%d-%H%M%S)"
VERBOSE="${VERBOSE:-0}"

# Parse arguments
while [[ $# -gt 0 ]]; do
  case $1 in
  --mount)
    AGENTFS_MOUNT="$2"
    shift 2
    ;;
  --verbose | -v)
    VERBOSE=1
    shift
    ;;
  *)
    echo "Unknown option: $1"
    exit 1
    ;;
  esac
done

# Binaries
AH_CLI="${PROJECT_ROOT}/target/release/ah"
CONTROL_CLI="${PROJECT_ROOT}/target/release/agentfs-control-cli"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log() {
  echo -e "${GREEN}[INFO]${NC} $*"
}

warn() {
  echo -e "${YELLOW}[WARN]${NC} $*"
}

error() {
  echo -e "${RED}[ERROR]${NC} $*"
}

debug() {
  if [[ "$VERBOSE" == "1" ]]; then
    echo -e "[DEBUG] $*"
  fi
}

# Create log directory
mkdir -p "${LOG_DIR}"
log "Log directory: ${LOG_DIR}"

# Check prerequisites
check_prerequisites() {
  log "Checking prerequisites..."

  # Check mount point
  local control_file="${AGENTFS_MOUNT}/.agentfs/control"
  if [[ ! -e "$control_file" ]]; then
    error "AgentFS control file not found at ${control_file}"
    error "Please ensure the AgentFS filesystem is mounted at ${AGENTFS_MOUNT}"
    error "Start the daemon with: just start-ah-fs-snapshots-daemon"
    exit 1
  fi

  # Test that the control file is accessible (daemon is running)
  if ! stat "$control_file" &>/dev/null; then
    error "AgentFS control file exists but is not accessible (I/O error)"
    error "The FUSE daemon may not be running. Start it with: just start-ah-fs-snapshots-daemon"
    exit 1
  fi

  # Check we have permission to access the control file
  if [[ ! -r "$control_file" ]] || [[ ! -w "$control_file" ]]; then
    local file_owner
    file_owner=$(stat -c '%U' "$control_file" 2>/dev/null || stat -f '%Su' "$control_file" 2>/dev/null)
    error "Cannot access AgentFS control file at ${control_file}"
    error "File is owned by '${file_owner}' but you are running as '$(whoami)'"
    error "The daemon should spawn the FUSE host as the requesting user."
    error "Try restarting the daemon: just stop-ah-fs-snapshots-daemon && just start-ah-fs-snapshots-daemon"
    exit 1
  fi

  # Check binaries exist
  if [[ ! -x "$AH_CLI" ]]; then
    warn "ah CLI not found at ${AH_CLI}, building..."
    (cd "${PROJECT_ROOT}" && nix develop . --accept-flake-config --command cargo build --release -p ah-cli --features agentfs) || {
      error "Failed to build ah CLI"
      exit 1
    }
  fi

  if [[ ! -x "$CONTROL_CLI" ]]; then
    warn "agentfs-control-cli not found at ${CONTROL_CLI}, building..."
    (cd "${PROJECT_ROOT}" && nix develop . --accept-flake-config --command cargo build --release -p agentfs-control-cli) || {
      error "Failed to build agentfs-control-cli"
      exit 1
    }
  fi

  log "Prerequisites OK"
}

# Test counter
TESTS_PASSED=0
TESTS_FAILED=0

test_pass() {
  local test_name="$1"
  log "${GREEN}PASS${NC}: ${test_name}"
  TESTS_PASSED=$((TESTS_PASSED + 1))
}

test_fail() {
  local test_name="$1"
  local reason="$2"
  error "${RED}FAIL${NC}: ${test_name} - ${reason}"
  TESTS_FAILED=$((TESTS_FAILED + 1))
}

# Test: Snapshot List Parity
test_snapshot_list_parity() {
  local test_name="snapshot_list_parity"
  log "Running test: ${test_name}"

  # Create a snapshot first to ensure there's something to list
  local snapshot_output
  debug "Creating snapshot with: $CONTROL_CLI --mount ${AGENTFS_MOUNT} snapshot-create --name test-parity-$$"
  if ! snapshot_output=$("$CONTROL_CLI" --mount "${AGENTFS_MOUNT}" snapshot-create --name "test-parity-$$" 2>&1); then
    test_fail "${test_name}" "Failed to create test snapshot: ${snapshot_output}"
    return
  fi
  debug "Created snapshot: ${snapshot_output}"
  echo "$snapshot_output" >"${LOG_DIR}/snapshot-create.txt"

  # Get reference output from agentfs-control-cli
  local ref_output
  debug "Listing snapshots with: $CONTROL_CLI --mount ${AGENTFS_MOUNT} snapshot-list"
  ref_output=$("$CONTROL_CLI" --mount "${AGENTFS_MOUNT}" snapshot-list 2>&1) || true
  echo "$ref_output" >"${LOG_DIR}/snapshot-list-ref.txt"
  debug "Reference output: ${ref_output}"

  # Get ah agent fs output
  local ah_output
  debug "Listing snapshots with: $AH_CLI agent fs snapshots --mount ${AGENTFS_MOUNT}"
  ah_output=$("$AH_CLI" agent fs snapshots --mount "${AGENTFS_MOUNT}" 2>&1) || true
  echo "$ah_output" >"${LOG_DIR}/snapshot-list-ah.txt"
  debug "AH CLI output: ${ah_output}"

  # Both should contain SNAPSHOT entries
  if echo "$ref_output" | grep -q "SNAPSHOT" && echo "$ah_output" | grep -q "SNAPSHOT\|Snapshots:"; then
    # Verify the snapshot we just created appears in both
    if echo "$ref_output" | grep -q "test-parity-$$" && echo "$ah_output" | grep -q "test-parity-$$"; then
      test_pass "${test_name}"
    else
      test_fail "${test_name}" "Created snapshot not found in output"
    fi
  else
    test_fail "${test_name}" "No snapshot entries found in output (ref has SNAPSHOT: $(echo "$ref_output" | grep -c SNAPSHOT || echo 0), ah has: $(echo "$ah_output" | grep -c "SNAPSHOT\|Snapshots:" || echo 0))"
  fi
}

# Test: Branch Create Parity
test_branch_create_parity() {
  local test_name="branch_create_parity"
  log "Running test: ${test_name}"

  # First create a snapshot to branch from
  local snapshot_output
  snapshot_output=$("$CONTROL_CLI" --mount "${AGENTFS_MOUNT}" snapshot-create --name "branch-test-$$" 2>&1)
  local snapshot_id
  snapshot_id=$(echo "$snapshot_output" | grep "SNAPSHOT_ID=" | sed 's/SNAPSHOT_ID=//' | cut -f1)

  if [[ -z "$snapshot_id" ]]; then
    test_fail "${test_name}" "Failed to create snapshot for branch test"
    return
  fi
  debug "Created snapshot: ${snapshot_id}"

  # Create branch with reference CLI
  local ref_output
  ref_output=$("$CONTROL_CLI" --mount "${AGENTFS_MOUNT}" branch-create --snapshot "${snapshot_id}" --name "ref-branch-$$" 2>&1) || true
  echo "$ref_output" >"${LOG_DIR}/branch-create-ref.txt"

  # Create branch with ah agent fs
  local ah_output
  ah_output=$("$AH_CLI" agent fs branch create "${snapshot_id}" --name "ah-branch-$$" --mount "${AGENTFS_MOUNT}" 2>&1) || true
  echo "$ah_output" >"${LOG_DIR}/branch-create-ah.txt"

  # Both should contain BRANCH_ID
  if echo "$ref_output" | grep -q "BRANCH_ID=" && echo "$ah_output" | grep -q "BRANCH_ID="; then
    test_pass "${test_name}"
  else
    test_fail "${test_name}" "BRANCH_ID not found in output (ref: $(grep -c BRANCH_ID= "${LOG_DIR}/branch-create-ref.txt" || echo 0), ah: $(grep -c BRANCH_ID= "${LOG_DIR}/branch-create-ah.txt" || echo 0))"
  fi
}

# Test: Branch Bind Parity
test_branch_bind_parity() {
  local test_name="branch_bind_parity"
  log "Running test: ${test_name}"

  # Create a snapshot and branch first
  local snapshot_output
  snapshot_output=$("$CONTROL_CLI" --mount "${AGENTFS_MOUNT}" snapshot-create --name "bind-test-$$" 2>&1)
  local snapshot_id
  snapshot_id=$(echo "$snapshot_output" | grep "SNAPSHOT_ID=" | sed 's/SNAPSHOT_ID=//' | cut -f1)

  if [[ -z "$snapshot_id" ]]; then
    test_fail "${test_name}" "Failed to create snapshot for bind test"
    return
  fi

  local branch_output
  branch_output=$("$CONTROL_CLI" --mount "${AGENTFS_MOUNT}" branch-create --snapshot "${snapshot_id}" --name "bind-branch-$$" 2>&1)
  local branch_id
  branch_id=$(echo "$branch_output" | grep "BRANCH_ID=" | sed 's/BRANCH_ID=//' | cut -f1)

  if [[ -z "$branch_id" ]]; then
    test_fail "${test_name}" "Failed to create branch for bind test"
    return
  fi
  debug "Created branch: ${branch_id}"

  # Bind with reference CLI
  local ref_output
  ref_output=$("$CONTROL_CLI" --mount "${AGENTFS_MOUNT}" branch-bind --branch "${branch_id}" --pid $$ 2>&1) || true
  echo "$ref_output" >"${LOG_DIR}/branch-bind-ref.txt"

  # Note: We can't easily test ah agent fs branch bind directly since it binds the current process
  # Instead, we verify the command accepts the same parameters and returns success
  local ah_output
  ah_output=$("$AH_CLI" agent fs branch bind "${branch_id}" --pid $$ --mount "${AGENTFS_MOUNT}" 2>&1) || true
  echo "$ah_output" >"${LOG_DIR}/branch-bind-ah.txt"

  # Both should indicate success (BRANCH_BIND_OK)
  if echo "$ref_output" | grep -q "BRANCH_BIND_OK" && echo "$ah_output" | grep -q "BRANCH_BIND_OK"; then
    test_pass "${test_name}"
  else
    test_fail "${test_name}" "BRANCH_BIND_OK not found in output"
  fi
}

# Test: Error Handling Parity
test_error_handling() {
  local test_name="error_handling"
  log "Running test: ${test_name}"

  # Try to create a branch from a non-existent snapshot
  local ref_output
  ref_output=$("$CONTROL_CLI" --mount "${AGENTFS_MOUNT}" branch-create --snapshot "nonexistent-snapshot-$$" --name "should-fail" 2>&1) || true
  echo "$ref_output" >"${LOG_DIR}/error-ref.txt"

  local ah_output
  ah_output=$("$AH_CLI" agent fs branch create "nonexistent-snapshot-$$" --name "should-fail" --mount "${AGENTFS_MOUNT}" 2>&1) || true
  echo "$ah_output" >"${LOG_DIR}/error-ah.txt"

  # Both should indicate an error
  if echo "$ref_output" | grep -qi "error\|fail" && echo "$ah_output" | grep -qi "error\|fail"; then
    test_pass "${test_name}"
  else
    test_fail "${test_name}" "Error not properly reported"
  fi
}

# Test: Mount Not Found
test_mount_not_found() {
  local test_name="mount_not_found"
  log "Running test: ${test_name}"

  local ah_output
  ah_output=$("$AH_CLI" agent fs snapshots --mount "/nonexistent/mount/point" 2>&1) || true
  echo "$ah_output" >"${LOG_DIR}/mount-not-found.txt"

  if echo "$ah_output" | grep -qi "not found\|error"; then
    test_pass "${test_name}"
  else
    test_fail "${test_name}" "Missing mount point not properly reported"
  fi
}

# Main test runner
main() {
  log "AgentFS CLI Parity Test - T15.1"
  log "Mount point: ${AGENTFS_MOUNT}"
  log ""

  check_prerequisites

  log ""
  log "Running tests..."
  log ""

  test_snapshot_list_parity
  test_branch_create_parity
  test_branch_bind_parity
  test_error_handling
  test_mount_not_found

  log ""
  log "=========================================="
  log "Test Results:"
  log "  Passed: ${TESTS_PASSED}"
  log "  Failed: ${TESTS_FAILED}"
  log "  Log directory: ${LOG_DIR}"
  log "=========================================="

  # Create summary JSON
  cat >"${LOG_DIR}/summary.json" <<EOF
{
  "test_name": "cli_parity_harness",
  "timestamp": "$(date -Iseconds)",
  "mount_point": "${AGENTFS_MOUNT}",
  "tests_passed": ${TESTS_PASSED},
  "tests_failed": ${TESTS_FAILED},
  "status": "$([ ${TESTS_FAILED} -eq 0 ] && echo "pass" || echo "fail")"
}
EOF

  if [[ ${TESTS_FAILED} -gt 0 ]]; then
    error "Some tests failed. Check logs in ${LOG_DIR}"
    exit 1
  fi

  log "All tests passed!"
  exit 0
}

main "$@"
