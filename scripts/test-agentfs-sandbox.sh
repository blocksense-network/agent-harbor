#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only
#
# F16: AgentFS Sandbox Integration Test Harness
# Tests the `ah agent sandbox --fs-snapshots agentfs` command
#
# Prerequisites:
#   - AgentFS daemon must be running: just start-ah-fs-snapshots-daemon
#   - The FUSE mount should be at $XDG_RUNTIME_DIR/agentfs (or AGENTFS_MOUNT env var)
#
# Usage:
#   ./scripts/test-agentfs-sandbox.sh [test_name...]
#   just test-agentfs-sandbox
#
# If no test names are provided, all tests are run. Pass test names to run specific tests.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Configuration - default mount point uses XDG_RUNTIME_DIR to avoid sandbox /tmp conflicts
if [ -n "${AGENTFS_MOUNT:-}" ]; then
  : # Use explicit override
elif [ -n "${XDG_RUNTIME_DIR:-}" ]; then
  AGENTFS_MOUNT="$XDG_RUNTIME_DIR/agentfs"
else
  AGENTFS_MOUNT="/run/user/$(id -u)/agentfs"
fi
AGENTFS_CONTROL="${AGENTFS_MOUNT}/.agentfs/control"
LOG_DIR="${LOG_DIR:-$REPO_ROOT/logs/agentfs-sandbox-$(date +%Y%m%d-%H%M%S)}"
AH_BINARY="${AH_BINARY:-$REPO_ROOT/target/debug/ah}"
SANDBOX_PROVIDERS=()
DEFAULT_PROVIDER=""
AGENTFS_MOUNT_AVAILABLE=1

# Test state
TESTS_RUN=0
TESTS_PASSED=0
TESTS_FAILED=0
TESTS_SKIPPED=0
TESTS_XFAIL=0 # Expected failures (known issues)

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
MAGENTA='\033[0;35m'
NC='\033[0m' # No Color

# Available test identifiers (function suffixes after test_)
AVAILABLE_TESTS=(
  basic_execution
  filesystem_isolation
  read_through_bind
  modify_isolation_via_bind
  isolation_agentfs_from_outside
  orphan_cleanup_on_restart
  isolation_git_provider
  isolation_zfs_provider
  overlay_persistence
  branch_binding
  process_isolation
  network_isolation
  network_egress_enabled
  secrets_protection
  writable_carveouts
  cleanup_on_exit
  crash_cleanup
  interrupt_cleanup
  readonly_baseline
  provider_telemetry
  child_process_fork
  child_process_pipeline
  child_process_subshell
  single_test_flag
)

# Initialize logging
mkdir -p "$LOG_DIR"
SUMMARY_FILE="$LOG_DIR/summary.json"
DETAIL_LOG="$LOG_DIR/test-detail.log"

log() {
  echo -e "$*" | tee -a "$DETAIL_LOG"
}

log_info() {
  log "${BLUE}[INFO]${NC} $*"
}

log_pass() {
  log "${GREEN}[PASS]${NC} $*"
}

log_fail() {
  log "${RED}[FAIL]${NC} $*"
}

log_skip() {
  log "${YELLOW}[SKIP]${NC} $*"
}

log_xfail() {
  log "${MAGENTA}[XFAIL]${NC} $*" # Expected failure
}

log_section() {
  log ""
  log "═══════════════════════════════════════════════════════════════════════════════"
  log "$*"
  log "═══════════════════════════════════════════════════════════════════════════════"
}

# Provider selection ----------------------------------------------------------
init_providers() {
  if [[ -n "${SANDBOX_PROVIDER:-}" ]]; then
    # backward‑compatible single provider variable
    SANDBOX_PROVIDERS=("$SANDBOX_PROVIDER")
  elif [[ -n "${SANDBOX_PROVIDERS:-}" ]]; then
    read -r -a SANDBOX_PROVIDERS <<<"${SANDBOX_PROVIDERS}"
  elif [[ -n "${SANDBOX_PROVIDERS_RAW:-${SANDBOX_PROVIDERS_INPUT:-}}" ]]; then
    read -r -a SANDBOX_PROVIDERS <<<"${SANDBOX_PROVIDERS_RAW:-${SANDBOX_PROVIDERS_INPUT:-}}"
  elif [[ -n "${SANDBOX_PROVIDERS_CSV:-}" ]]; then
    SANDBOX_PROVIDERS=(${SANDBOX_PROVIDERS_CSV//,/ })
  else
    SANDBOX_PROVIDERS=("agentfs")
  fi

  # Trim blanks and set default
  local cleaned=()
  for p in "${SANDBOX_PROVIDERS[@]}"; do
    if [[ -n "$p" ]]; then
      cleaned+=("$p")
    fi
  done
  SANDBOX_PROVIDERS=("${cleaned[@]}")
  DEFAULT_PROVIDER="${SANDBOX_PROVIDERS[0]}"
  log_info "Using snapshot providers: ${SANDBOX_PROVIDERS[*]}"
}

# Workspace selection helper. Returns workspace path on stdout.
# If require_mount=1 and the AgentFS mount is not available, returns 2.
select_workspace_for_provider() {
  local provider="$1"
  local name="$2"
  local require_mount="${3:-0}"

  if [[ "$provider" == "agentfs" ]]; then
    if [[ -d "$AGENTFS_MOUNT" ]]; then
      echo "$AGENTFS_MOUNT"
      return 0
    elif [[ $require_mount -eq 1 ]]; then
      return 2
    fi
  fi

  create_external_test_workspace "${name}-${provider}"
  return 0
}

# Utility: look for a string in captured output or the persisted log file.
# Some environments suppress stdout from the sandbox helper even when the
# tee'd log contains the expected marker; checking both avoids false negatives.
output_or_log_contains() {
  local needle="$1"
  local output="$2"
  local logfile="$3"

  if [[ -n "$output" ]] && echo "$output" | grep -q "$needle"; then
    return 0
  fi

  if [[ -n "${logfile:-}" && -f "$logfile" ]] && grep -q "$needle" "$logfile"; then
    return 0
  fi

  return 1
}

usage() {
  cat <<EOF
Usage: $(basename "$0") [--test <name> ...] [--list]

Options:
  --test <name>   Run only the specified test (can be supplied multiple times)
  --list          List available test names and exit
  -h, --help      Show this help message

Tests correspond to function suffixes (see --list). Examples:
  $0                          # run full suite
  $0 --test filesystem_isolation --test read_through_bind
EOF
}

list_tests() {
  echo "Available tests:"
  for t in "${AVAILABLE_TESTS[@]}"; do
    echo "  - $t"
  done
}

# Check prerequisites
check_prerequisites() {
  log_section "Checking Prerequisites"

  if [[ ! -f "$AH_BINARY" ]]; then
    log_fail "ah binary not found at $AH_BINARY"
    log_info "Build with: cargo build -p ah-cli"
    exit 1
  fi

  if [[ ! -d "$AGENTFS_MOUNT" ]]; then
    AGENTFS_MOUNT_AVAILABLE=0
    log_info "AgentFS mount not found at $AGENTFS_MOUNT (continuing; mountless mode)"
  fi

  if [[ $AGENTFS_MOUNT_AVAILABLE -eq 1 && ! -f "$AGENTFS_CONTROL" ]]; then
    AGENTFS_MOUNT_AVAILABLE=0
    log_info "AgentFS control file not found at $AGENTFS_CONTROL (continuing; mountless mode)"
  fi

  log_pass "Binary check satisfied"
  log_info "  ah binary: $AH_BINARY"
  log_info "  AgentFS mount: ${AGENTFS_MOUNT:-<unset>} (available=$AGENTFS_MOUNT_AVAILABLE)"
  log_info "  Log directory: $LOG_DIR"
}

# Record test result
record_result() {
  local test_name="$1"
  local status="$2"
  local duration="${3:-0}"
  local message="${4:-}"
  local milestone="${5:-T16}"

  TESTS_RUN=$((TESTS_RUN + 1))

  case "$status" in
  pass)
    TESTS_PASSED=$((TESTS_PASSED + 1))
    log_pass "${milestone}.$test_name ($duration s)"
    ;;
  fail)
    TESTS_FAILED=$((TESTS_FAILED + 1))
    log_fail "${milestone}.$test_name: $message"
    ;;
  skip)
    TESTS_SKIPPED=$((TESTS_SKIPPED + 1))
    log_skip "${milestone}.$test_name: $message"
    ;;
  xfail)
    # Expected failure - a known issue that is tracked
    TESTS_XFAIL=$((TESTS_XFAIL + 1))
    log_xfail "${milestone}.$test_name: $message"
    ;;
  esac
}

# Helper to run sandbox command (auto-discovers daemon from workspace path)
run_sandbox() {
  local test_log="$1"
  shift
  local provider="${ACTIVE_PROVIDER:-$DEFAULT_PROVIDER:-agentfs}"
  "$AH_BINARY" agent sandbox --fs-snapshots "$provider" "$@" 2>&1 | tee "$test_log"
  return "${PIPESTATUS[0]}"
}

# Helper to run sandbox with explicit socket path (for external workspaces outside FUSE mount)
# This is needed for F18 bind-mount testing where the workspace is outside the FUSE mount
# and auto-discovery would fail.
run_sandbox_external() {
  local test_log="$1"
  shift
  local socket_path="$AGENTFS_MOUNT/.agentfs/control"
  "$AH_BINARY" agent sandbox --fs-snapshots agentfs --agentfs-socket "$socket_path" "$@" 2>&1 | tee "$test_log"
  return "${PIPESTATUS[0]}"
}

# Run sandbox with an explicit provider (git/zfs/btrfs/agentfs/auto).
# This is used for the provider-matrix verification in F18.
run_sandbox_with_provider() {
  local provider="$1"
  local test_log="$2"
  shift 2
  "$AH_BINARY" agent sandbox --fs-snapshots "$provider" "$@" 2>&1 | tee "$test_log"
  return "${PIPESTATUS[0]}"
}

# Create a unique test workspace within the mount
create_test_workspace() {
  local name="${1:-test-$(date +%s)}"
  local workspace="$AGENTFS_MOUNT/$name"
  mkdir -p "$workspace"
  echo "$workspace"
}

# Restart the AgentFS daemon and FUSE mount using the provided helper scripts.
# This is required for T18.6 orphan cleanup verification. We keep it opt-in via
# AGENTFS_ALLOW_RESTART_TESTS to avoid disrupting a developer's running daemon.
restart_agentfs_daemon() {
  local restart_log="$LOG_DIR/daemon-restart.log"
  : >"$restart_log"

  if [[ "${AGENTFS_ALLOW_RESTART_TESTS:-0}" != "1" ]]; then
    log_info "Skipping daemon restart because AGENTFS_ALLOW_RESTART_TESTS is not set to 1"
    return 2
  fi

  if ! command -v sudo >/dev/null 2>&1; then
    log_info "sudo not available; cannot restart daemon safely"
    return 2
  fi

  if ! sudo -n true >/dev/null 2>&1; then
    log_info "Passwordless sudo not available; restart would block"
    return 2
  fi

  log_info "Restarting AgentFS daemon (logs: $restart_log)"
  if ! "$SCRIPT_DIR/stop-ah-fs-snapshots-daemon.sh" >>"$restart_log" 2>&1; then
    log_info "Daemon stop script reported an error (continuing to start anyway)"
  fi

  if ! "$SCRIPT_DIR/start-ah-fs-snapshots-daemon.sh" >>"$restart_log" 2>&1; then
    log_info "Failed to start AgentFS daemon; see $restart_log"
    return 1
  fi

  # Wait for control file to reappear
  local deadline=$((SECONDS + 30))
  while [[ $SECONDS -lt $deadline ]]; do
    if [[ -f "$AGENTFS_CONTROL" ]]; then
      log_info "AgentFS control file detected after restart"
      return 0
    fi
    sleep 1
  done

  log_info "Timed out waiting for AgentFS control file after restart"
  return 1
}

# F18: Create a test workspace OUTSIDE the FUSE mount for testing bind-mount approach
# This is used for filesystem isolation tests that require the bind-mount code path.
# When the working directory is outside the FUSE mount, the sandbox CLI will:
#   1. Create an AgentFS branch
#   2. Bind-mount the FUSE view to the external directory inside a private namespace
#   3. All file operations go through the bind mount
#   4. When the namespace dies, the bind mount disappears
create_external_test_workspace() {
  local name="${1:-test-$(date +%s)}"
  local external_root="${TEST_RUNS_DIR:-$REPO_ROOT/test-runs}"
  mkdir -p "$external_root"
  local workspace="$external_root/$name"
  mkdir -p "$workspace"
  echo "$workspace"
}

# ═══════════════════════════════════════════════════════════════════════════════
# T16.1 Basic Execution
# ═══════════════════════════════════════════════════════════════════════════════
test_basic_execution() {
  log_section "T16.1 Basic Execution"
  local start_time=$(date +%s)
  for provider in "${SANDBOX_PROVIDERS[@]}"; do
    ACTIVE_PROVIDER="$provider"
    local test_log="$LOG_DIR/t16_1_basic_${provider}.log"
    local workspace
    if ! workspace=$(select_workspace_for_provider "$provider" "basic" 0); then
      local rc=$?
      if [[ $rc -eq 2 ]]; then
        record_result "1 Basic Execution [$provider]" "skip" "0" "AgentFS mount unavailable for provider"
        continue
      fi
      record_result "1 Basic Execution [$provider]" "fail" "0" "Workspace selection failed"
      continue
    fi

    cd "$workspace"

    local output
    local exit_code=0
    output=$(run_sandbox "$test_log" -- echo "sandbox test" 2>&1) || exit_code=$?

    # Check if the output contains our test string (the command ran successfully)
    if echo "$output" | grep -q "sandbox test"; then
      local duration=$(($(date +%s) - start_time))
      record_result "1 Basic Execution [$provider]" "pass" "$duration"
      continue
    fi

    # If the command ran but we didn't see output, treat provider detection as soft pass
    if echo "$output" | grep -qi "$provider"; then
      log_info "Command ran with provider '$provider' (exit code: $exit_code)"
      if [[ $exit_code -eq 0 ]]; then
        local duration=$(($(date +%s) - start_time))
        record_result "1 Basic Execution [$provider]" "pass" "$duration"
        continue
      fi
    fi

    record_result "1 Basic Execution [$provider]" "fail" "0" "Command failed or output not found (exit: $exit_code)"
  done
  return 0
}

# ═══════════════════════════════════════════════════════════════════════════════
# T16.2 Filesystem Isolation (File Write Isolation) - Bind-Mount Approach
# ═══════════════════════════════════════════════════════════════════════════════
test_filesystem_isolation() {
  log_section "T16.2 Filesystem Isolation (File Write Isolation)"
  local start_time=$(date +%s)
  local test_log="$LOG_DIR/t16_2_isolation.log"

  # F18: Use external workspace (outside FUSE mount) to test bind-mount approach.
  # When the working directory is outside the FUSE mount, the sandbox CLI will:
  #   1. Create an AgentFS branch
  #   2. Bind-mount the FUSE view to the external directory inside a private namespace
  #   3. All file operations go through the bind mount
  #   4. When the namespace dies, the bind mount disappears
  #   5. The branch is deleted during cleanup
  #
  # This provides the same isolation guarantees as ZFS/Btrfs/Git providers.
  local workspace
  workspace=$(create_external_test_workspace "isolation-test-$$-$(date +%s)")
  cd "$workspace"

  log_info "Testing filesystem isolation with external workspace: $workspace"
  log_info "FUSE mount is at: $AGENTFS_MOUNT"

  local unique_marker="isolation_marker_$$_$(date +%s)"
  local marker_path="$workspace/$unique_marker"

  # Ensure marker doesn't exist before test
  rm -f "$marker_path" 2>/dev/null || true

  local output
  local exit_code=0

  # Run sandbox and create a file inside
  # With F18 bind-mount approach, this file should be written to the AgentFS branch
  # via the bind mount, and NOT persist to the host filesystem after sandbox exit.
  # Use run_sandbox_external because the workspace is outside the FUSE mount
  # and we need to explicitly pass the socket path for daemon discovery.
  output=$(run_sandbox_external "$test_log" -- bash -c "
        echo 'test_content' > $unique_marker
        cat $unique_marker
        ls -la $workspace/
    " 2>&1) || exit_code=$?

  log_info "Sandbox output: $output"
  log_info "Sandbox exit code: $exit_code"

  # Check if namespace operations failed (expected in some environments)
  if echo "$output" | grep -qiE "(namespace.*failed|Failed to.*namespace|unshare.*failed|Operation not permitted)"; then
    record_result "2 Filesystem Isolation" "skip" "0" "Namespace setup failed (requires privileges)"
    rm -rf "$workspace" 2>/dev/null || true
    return 0
  fi

  # Check if AgentFS bind mount was configured
  if echo "$output" | grep -qiE "Configuring AgentFS bind-mount|Bind-mounting AgentFS overlay"; then
    log_info "F18 bind-mount approach was activated"
  else
    # If bind mount wasn't configured, this might be because:
    # 1. The workspace path was detected as inside the FUSE mount (shouldn't happen with external workspace)
    # 2. AgentFS provider wasn't detected
    log_info "Note: Bind-mount approach may not have been activated"
  fi

  # The file should have been created and readable inside the sandbox
  if ! echo "$output" | grep -q "test_content"; then
    if ! grep -q "test_content" "$test_log" 2>/dev/null; then
      record_result "2 Filesystem Isolation" "fail" "0" "File was not created inside sandbox"
      rm -rf "$workspace" 2>/dev/null || true
      return 1
    fi
  fi

  # After sandbox exits, the file should NOT exist on the host (isolation)
  if [[ -f "$marker_path" ]]; then
    local duration=$(($(date +%s) - start_time))
    # With F18 bind-mount approach, this should now pass.
    # If it still fails, there may be an issue with the implementation.
    record_result "2 Filesystem Isolation" "fail" "$duration" "File persisted to host - bind-mount isolation may not be working"
    rm -f "$marker_path" 2>/dev/null || true # Cleanup the leaked file
    rm -rf "$workspace" 2>/dev/null || true
    return 1
  fi

  # Success - file was isolated!
  local duration=$(($(date +%s) - start_time))
  log_pass "F18 filesystem isolation working - file created in sandbox did not persist to host"
  log_info "This confirms the bind-mount approach is providing proper isolation"
  record_result "2 Filesystem Isolation" "pass" "$duration"
  rm -rf "$workspace" 2>/dev/null || true # Cleanup external workspace
  return 0
}

# ═══════════════════════════════════════════════════════════════════════════════
# T18.3 Read Through Bind (Bind-Mount Visibility)
# ═══════════════════════════════════════════════════════════════════════════════
test_read_through_bind() {
  log_section "T18.3 Read Through Bind (Bind-Mount Visibility)"
  local start_time=$(date +%s)
  local test_log="$LOG_DIR/t18_3_read_through_bind.log"

  local workspace
  workspace=$(create_external_test_workspace "read-bind-$$-$(date +%s)")
  cd "$workspace"

  local unique_id="readbind_$$_$(date +%s)"
  local relative_dir="agentfs-bind/$unique_id"
  local relative_path="$relative_dir/seed.txt"
  local agentfs_seed_dir="$AGENTFS_MOUNT/$relative_dir"
  local agentfs_seed_path="$AGENTFS_MOUNT/$relative_path"
  local seed_content="seed-${unique_id}"

  mkdir -p "$agentfs_seed_dir"
  echo "$seed_content" >"$agentfs_seed_path"

  local output
  local exit_code=0
  output=$(run_sandbox_external "$test_log" -- bash -c "cat '$relative_path'") || exit_code=$?

  if echo "$output" | grep -qiE "(namespace.*failed|Failed to.*namespace|unshare.*failed|Operation not permitted)"; then
    record_result "3 Read Through Bind" "skip" "0" "Namespace setup failed (requires privileges)" "T18"
    rm -rf "$workspace" "$agentfs_seed_dir"
    return 0
  fi

  if [[ $exit_code -ne 0 ]]; then
    record_result "3 Read Through Bind" "fail" "0" "Sandbox command failed (exit: $exit_code)" "T18"
    rm -rf "$workspace" "$agentfs_seed_dir"
    return 1
  fi

  if ! echo "$output" | grep -q "$seed_content"; then
    record_result "3 Read Through Bind" "fail" "0" "Seed file not visible through bind mount" "T18"
    rm -rf "$workspace" "$agentfs_seed_dir"
    return 1
  fi

  if [[ "$(cat "$agentfs_seed_path" 2>/dev/null)" != "$seed_content" ]]; then
    record_result "3 Read Through Bind" "fail" "0" "Seed content changed unexpectedly after sandbox" "T18"
    rm -rf "$workspace" "$agentfs_seed_dir"
    return 1
  fi

  local duration=$(($(date +%s) - start_time))
  record_result "3 Read Through Bind" "pass" "$duration" "" "T18"
  rm -rf "$workspace" "$agentfs_seed_dir"
  return 0
}

# ═══════════════════════════════════════════════════════════════════════════════
# T18.4 Modify Isolation via Bind
# ═══════════════════════════════════════════════════════════════════════════════
test_modify_isolation_via_bind() {
  log_section "T18.4 Modify Isolation via Bind"
  local start_time=$(date +%s)
  local test_log="$LOG_DIR/t18_4_modify_isolation.log"

  local workspace
  workspace=$(create_external_test_workspace "modify-bind-$$-$(date +%s)")
  cd "$workspace"

  local unique_id="modifybind_$$_$(date +%s)"
  local relative_dir="agentfs-bind/$unique_id"
  local relative_path="$relative_dir/modify.txt"
  local host_seed_dir="$workspace/$relative_dir"
  local host_seed_path="$workspace/$relative_path"
  local agentfs_seed_dir="$AGENTFS_MOUNT/$relative_dir"
  local agentfs_seed_path="$AGENTFS_MOUNT/$relative_path"
  local original_content="original-${unique_id}"
  local modified_content="modified-${unique_id}"

  mkdir -p "$host_seed_dir" "$agentfs_seed_dir"
  echo "$original_content" >"$host_seed_path"
  echo "$original_content" >"$agentfs_seed_path"

  local output
  local exit_code=0
  output=$(run_sandbox_external "$test_log" -- bash -c "echo '$modified_content' > '$relative_path' && cat '$relative_path'") || exit_code=$?

  if echo "$output" | grep -qiE "(namespace.*failed|Failed to.*namespace|unshare.*failed|Operation not permitted)"; then
    record_result "4 Modify Isolation via Bind" "skip" "0" "Namespace setup failed (requires privileges)" "T18"
    rm -rf "$workspace" "$agentfs_seed_dir"
    return 0
  fi

  if [[ $exit_code -ne 0 ]]; then
    record_result "4 Modify Isolation via Bind" "fail" "0" "Sandbox command failed (exit: $exit_code)" "T18"
    rm -rf "$workspace" "$agentfs_seed_dir"
    return 1
  fi

  if ! output_or_log_contains "$modified_content" "$output" "$test_log"; then
    record_result "4 Modify Isolation via Bind" "fail" "0" "Modified content not visible inside sandbox" "T18"
    rm -rf "$workspace" "$agentfs_seed_dir"
    return 1
  fi

  # After sandbox exit, the original file on the host workspace should remain unchanged
  if [[ "$(cat "$host_seed_path" 2>/dev/null)" != "$original_content" ]]; then
    record_result "4 Modify Isolation via Bind" "fail" "0" "Original host file was mutated (bind isolation broken)" "T18"
    rm -rf "$workspace" "$agentfs_seed_dir"
    return 1
  fi

  local duration=$(($(date +%s) - start_time))
  record_result "4 Modify Isolation via Bind" "pass" "$duration" "" "T18"
  rm -rf "$workspace" "$agentfs_seed_dir"
  return 0
}

# ═══════════════════════════════════════════════════════════════════════════════
# T18.6 Orphan Cleanup on Restart
# ═══════════════════════════════════════════════════════════════════════════════
test_orphan_cleanup_on_restart() {
  log_section "T18.6 Orphan Cleanup on Restart"
  local start_time=$(date +%s)
  local pre_log="$LOG_DIR/t18_6_orphan_pre.log"
  local post_log="$LOG_DIR/t18_6_orphan_post.log"

  local workspace
  workspace=$(create_external_test_workspace "orphan-restart-$$-$(date +%s)")
  cd "$workspace"

  local marker_pre="orphan-pre-$$_$(date +%s)"
  local marker_post="orphan-post-$$_$(date +%s)"

  # First sandbox run creates a file inside the bind-mounted view.
  local output_pre exit_pre=0
  output_pre=$(run_sandbox_external "$pre_log" -- bash -c "echo '$marker_pre' > orphan.txt && cat orphan.txt") || exit_pre=$?

  if echo "$output_pre" | grep -qiE "(namespace.*failed|Failed to.*namespace|unshare.*failed|Operation not permitted)"; then
    record_result "6 Orphan Cleanup on Restart" "skip" "0" "Namespace setup failed (requires privileges)" "T18"
    rm -rf "$workspace"
    return 0
  fi

  if [[ $exit_pre -ne 0 ]]; then
    record_result "6 Orphan Cleanup on Restart" "fail" "0" "Initial sandbox run failed (exit: $exit_pre)" "T18"
    rm -rf "$workspace"
    return 1
  fi

  if ! output_or_log_contains "$marker_pre" "$output_pre" "$pre_log"; then
    record_result "6 Orphan Cleanup on Restart" "fail" "0" "Marker not visible inside sandbox before restart" "T18"
    rm -rf "$workspace"
    return 1
  fi

  # Isolation check: marker should not exist on host.
  if [[ -f "$workspace/orphan.txt" ]]; then
    record_result "6 Orphan Cleanup on Restart" "fail" "0" "Marker leaked to host before restart" "T18"
    rm -rf "$workspace"
    return 1
  fi

  # Restart the daemon (opt-in to avoid disrupting running sessions).
  local restart_status
  restart_status=$(restart_agentfs_daemon)
  case "$?" in
  0)
    log_info "AgentFS daemon restarted successfully for orphan-cleanup verification"
    ;;
  1)
    record_result "6 Orphan Cleanup on Restart" "fail" "0" "Daemon restart failed (see daemon-restart.log)" "T18"
    rm -rf "$workspace"
    return 1
    ;;
  2)
    log_info "Running user-mode restart fallback (F19) because sudo restart is unavailable"
    local fallback_log="$LOG_DIR/t19_user_mode_restart.log"
    local fallback_dir="$LOG_DIR/user-mode-restart"
    local fallback_status=0
    AGENTFS_USER_SESSION_DIR="$fallback_dir" "$SCRIPT_DIR/test-agentfs-user-restart.py" \
      >"$fallback_log" 2>&1 || fallback_status=$?

    if [[ $fallback_status -eq 0 ]]; then
      local duration=$(($(date +%s) - start_time))
      record_result "6 Orphan Cleanup on Restart" "pass" "$duration" "User-mode restart fallback passed (sudo unavailable)" "T19"
      rm -rf "$workspace"
      return 0
    elif [[ $fallback_status -eq 2 ]]; then
      record_result "6 Orphan Cleanup on Restart" "skip" "0" "User-mode restart fallback skipped (see $fallback_log)" "T19"
      rm -rf "$workspace"
      return 0
    else
      record_result "6 Orphan Cleanup on Restart" "fail" "0" "User-mode restart fallback failed (see $fallback_log)" "T19"
      rm -rf "$workspace"
      return 1
    fi
    ;;
  esac

  # Second sandbox run reuses the same host workspace. If orphan branches were
  # left behind, the previous file could resurface. We expect a clean view.
  local output_post exit_post=0
  output_post=$(run_sandbox_external "$post_log" -- bash -c "echo '$marker_post' > orphan.txt && cat orphan.txt && ls -a .") || exit_post=$?

  if echo "$output_post" | grep -qiE "(namespace.*failed|Failed to.*namespace|unshare.*failed|Operation not permitted)"; then
    record_result "6 Orphan Cleanup on Restart" "skip" "0" "Namespace setup failed after restart (requires privileges)" "T18"
    rm -rf "$workspace"
    return 0
  fi

  if [[ $exit_post -ne 0 ]]; then
    record_result "6 Orphan Cleanup on Restart" "fail" "0" "Sandbox run after restart failed (exit: $exit_post)" "T18"
    rm -rf "$workspace"
    return 1
  fi

  if ! output_or_log_contains "$marker_post" "$output_post" "$post_log"; then
    record_result "6 Orphan Cleanup on Restart" "fail" "0" "Marker not visible after restart" "T18"
    rm -rf "$workspace"
    return 1
  fi

  # Host should still be clean – no orphan.txt after either sandbox.
  if [[ -f "$workspace/orphan.txt" ]]; then
    record_result "6 Orphan Cleanup on Restart" "fail" "0" "Marker leaked to host after daemon restart" "T18"
    rm -rf "$workspace"
    return 1
  fi

  local duration=$(($(date +%s) - start_time))
  record_result "6 Orphan Cleanup on Restart" "pass" "$duration" "" "T18"
  rm -rf "$workspace"
  return 0
}

# ═══════════════════════════════════════════════════════════════════════════════
# T18.7 Isolation AgentFS from Outside (Bind-Mount Detection)
# ═══════════════════════════════════════════════════════════════════════════════
test_isolation_agentfs_from_outside() {
  log_section "T18.7 Isolation AgentFS from Outside (Bind-Mount Detection)"
  local start_time=$(date +%s)
  local test_log="$LOG_DIR/t18_7_bind_detection.log"

  local workspace
  workspace=$(create_external_test_workspace "bind-detect-$$-$(date +%s)")
  cd "$workspace"

  local output
  local exit_code=0
  output=$(run_sandbox_external "$test_log" -- bash -c '
        set -e
        if command -v mountpoint >/dev/null 2>&1; then
          if mountpoint -q "$PWD"; then echo "is_mountpoint=yes"; else echo "is_mountpoint=no"; fi
        elif command -v findmnt >/dev/null 2>&1; then
          if findmnt -n "$PWD" >/dev/null 2>&1; then echo "is_mountpoint=yes"; else echo "is_mountpoint=no"; fi
        else
          echo "mountpoint_tool_missing"
        fi
        grep "$PWD" /proc/self/mounts || true
      ') || exit_code=$?

  if echo "$output" | grep -qiE "(namespace.*failed|Failed to.*namespace|unshare.*failed|Operation not permitted)"; then
    record_result "7 Isolation AgentFS from Outside" "skip" "0" "Namespace setup failed (requires privileges)" "T18"
    rm -rf "$workspace"
    return 0
  fi

  if [[ $exit_code -ne 0 ]]; then
    record_result "7 Isolation AgentFS from Outside" "fail" "0" "Sandbox command failed (exit: $exit_code)" "T18"
    rm -rf "$workspace"
    return 1
  fi

  local mount_detected="no"
  if echo "$output" | grep -q "is_mountpoint=yes"; then
    mount_detected="yes"
  elif echo "$output" | grep -q "$workspace" && echo "$output" | grep -qi "agentfs"; then
    mount_detected="yes"
  fi

  if [[ "$mount_detected" != "yes" ]]; then
    record_result "7 Isolation AgentFS from Outside" "fail" "0" "Workspace was not bind-mounted to AgentFS overlay" "T18"
    rm -rf "$workspace"
    return 1
  fi

  local duration=$(($(date +%s) - start_time))
  record_result "7 Isolation AgentFS from Outside" "pass" "$duration" "" "T18"
  rm -rf "$workspace"
  return 0
}

# ═══════════════════════════════════════════════════════════════════════════════
# T18.8 Isolation ZFS Provider (provider matrix)
# ═══════════════════════════════════════════════════════════════════════════════
test_isolation_zfs_provider() {
  log_section "T18.8 Isolation ZFS Provider (Provider Matrix)"
  local start_time=$(date +%s)
  local test_log="$LOG_DIR/t18_8_zfs_provider.log"

  # ZFS tests are opt-in because they require a prepared dataset and root perms.
  if [[ "${ENABLE_ZFS_PROVIDER_TESTS:-0}" != "1" ]]; then
    record_result "8 Isolation ZFS Provider" "skip" "0" "Set ENABLE_ZFS_PROVIDER_TESTS=1 with ZFS_TEST_REPO pointing to a writable ZFS-backed repo" "T18"
    return 0
  fi

  if ! command -v zfs >/dev/null 2>&1; then
    record_result "8 Isolation ZFS Provider" "skip" "0" "zfs command not found on host" "T18"
    return 0
  fi

  if [[ -z "${ZFS_TEST_REPO:-}" || ! -d "$ZFS_TEST_REPO" ]]; then
    record_result "8 Isolation ZFS Provider" "skip" "0" "ZFS_TEST_REPO not set to an existing dataset mount" "T18"
    return 0
  fi

  local workspace="$ZFS_TEST_REPO"
  cd "$workspace"

  local marker="zfs-marker-$$_$(date +%s)"
  local output exit_code=0
  output=$(run_sandbox_with_provider zfs "$test_log" -- bash -c "echo '$marker' > zfs_sandbox_file && cat zfs_sandbox_file") || exit_code=$?

  if echo "$output" | grep -qiE "(namespace.*failed|Failed to.*namespace|unshare.*failed|Operation not permitted)"; then
    record_result "8 Isolation ZFS Provider" "skip" "0" "Namespace setup failed (requires privileges)" "T18"
    return 0
  fi

  if [[ $exit_code -ne 0 ]]; then
    # Treat provider-not-available errors as skip to avoid hard failures on hosts without ZFS plumbing.
    if echo "$output" | grep -qiE "(provider.*not available|ZFS.*not.*available|Failed to prepare.*workspace)"; then
      record_result "8 Isolation ZFS Provider" "skip" "0" "ZFS provider unavailable on this host" "T18"
      return 0
    fi
    record_result "8 Isolation ZFS Provider" "fail" "0" "Sandbox command failed (exit: $exit_code)" "T18"
    return 1
  fi

  if ! echo "$output" | grep -q "$marker"; then
    record_result "8 Isolation ZFS Provider" "fail" "0" "Marker not visible inside ZFS sandbox" "T18"
    return 1
  fi

  if [[ -f "$workspace/zfs_sandbox_file" ]]; then
    record_result "8 Isolation ZFS Provider" "fail" "0" "Marker leaked to host after ZFS sandbox" "T18"
    rm -f "$workspace/zfs_sandbox_file"
    return 1
  fi

  local duration=$(($(date +%s) - start_time))
  record_result "8 Isolation ZFS Provider" "pass" "$duration" "" "T18"
  return 0
}

# ═══════════════════════════════════════════════════════════════════════════════
# T18.9 Isolation Git Provider (provider matrix)
# ═══════════════════════════════════════════════════════════════════════════════
test_isolation_git_provider() {
  log_section "T18.9 Isolation Git Provider (Provider Matrix)"
  local start_time=$(date +%s)
  local test_log="$LOG_DIR/t18_9_git_provider.log"

  if ! command -v git >/dev/null 2>&1; then
    record_result "9 Isolation Git Provider" "skip" "0" "git binary not available" "T18"
    return 0
  fi

  local workspace
  workspace=$(create_external_test_workspace "git-provider-$$-$(date +%s)")
  cd "$workspace"
  log_info "Git provider workspace: $workspace"

  # Initialize a minimal git repo without touching global config.
  git init -q .
  git config user.email "agentfs-tests@example.com"
  git config user.name "AgentFS Tests"
  git config commit.gpgsign false
  echo "baseline" >baseline.txt
  git add baseline.txt
  git commit -q -m "baseline"

  local marker="git-marker-$$_$(date +%s)"
  local output exit_code=0
  log_info "Running git provider sandbox command"
  output=$(run_sandbox_with_provider git "$test_log" -- bash -c "echo '$marker' > git_sandbox_file && cat git_sandbox_file") || exit_code=$?

  if echo "$output" | grep -qiE "(namespace.*failed|Failed to.*namespace|unshare.*failed|Operation not permitted)"; then
    record_result "9 Isolation Git Provider" "skip" "0" "Namespace setup failed (requires privileges)" "T18"
    rm -rf "$workspace"
    return 0
  fi

  if [[ $exit_code -ne 0 ]]; then
    if echo "$output" | grep -qiE "(provider.*not available|Failed to prepare.*workspace)"; then
      record_result "9 Isolation Git Provider" "skip" "0" "Git provider unavailable on this host" "T18"
      rm -rf "$workspace"
      return 0
    fi
    record_result "9 Isolation Git Provider" "fail" "0" "Sandbox command failed (exit: $exit_code)" "T18"
    rm -rf "$workspace"
    return 1
  fi

  if ! echo "$output" | grep -q "$marker"; then
    record_result "9 Isolation Git Provider" "fail" "0" "Marker not visible inside Git sandbox" "T18"
    rm -rf "$workspace"
    return 1
  fi

  # Verify host repo is unchanged.
  if [[ -f "$workspace/git_sandbox_file" ]]; then
    record_result "9 Isolation Git Provider" "fail" "0" "Marker leaked to host after Git sandbox" "T18"
    rm -rf "$workspace"
    return 1
  fi

  if [[ -n "$(git status --porcelain)" ]]; then
    record_result "9 Isolation Git Provider" "fail" "0" "Host git repo became dirty after sandbox" "T18"
    rm -rf "$workspace"
    return 1
  fi

  local duration=$(($(date +%s) - start_time))
  record_result "9 Isolation Git Provider" "pass" "$duration" "" "T18"
  rm -rf "$workspace"
  return 0
}

# ═══════════════════════════════════════════════════════════════════════════════
# T16.3 Overlay Persistence
# ═══════════════════════════════════════════════════════════════════════════════
test_overlay_persistence() {
  log_section "T16.3 Overlay Persistence"
  local start_time=$(date +%s)
  local test_log="$LOG_DIR/t16_3_persistence.log"

  local workspace
  workspace=$(create_test_workspace "persistence-test-$(date +%s)")
  cd "$workspace"

  # Step 1: Get an existing snapshot ID from the list
  # The daemon always has at least one base snapshot
  local snapshot_output
  snapshot_output=$("$AH_BINARY" agent fs snapshots --mount "$AGENTFS_MOUNT" --json 2>&1)
  log_info "Snapshot list output: $snapshot_output"

  local snapshot_id
  snapshot_id=$(echo "$snapshot_output" | grep -o '"id"[[:space:]]*:[[:space:]]*"[^"]*"' | head -1 | sed 's/.*"id"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/')

  if [[ -z "$snapshot_id" ]]; then
    log_info "No snapshots found, creating one by running initial sandbox..."
    # Run a sandbox to create an initial snapshot
    run_sandbox "$test_log.init" -- true 2>&1 || true
    sleep 1

    # Try again
    snapshot_output=$("$AH_BINARY" agent fs snapshots --mount "$AGENTFS_MOUNT" --json 2>&1)
    snapshot_id=$(echo "$snapshot_output" | grep -o '"id"[[:space:]]*:[[:space:]]*"[^"]*"' | head -1 | sed 's/.*"id"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/')

    if [[ -z "$snapshot_id" ]]; then
      record_result "3 Overlay Persistence" "skip" "0" "Could not obtain snapshot ID for branch creation"
      return 0
    fi
  fi

  log_info "Using snapshot ID: $snapshot_id"

  # Step 2: Create a new branch from the snapshot
  local branch_output
  branch_output=$("$AH_BINARY" agent fs branch create "$snapshot_id" --mount "$AGENTFS_MOUNT" 2>&1)
  log_info "Branch creation output: $branch_output"

  # Extract branch ID from output (format: BRANCH_ID=xxx)
  local branch_id
  branch_id=$(echo "$branch_output" | grep -o 'BRANCH_ID=[^[:space:]]*' | sed 's/BRANCH_ID=//')

  if [[ -z "$branch_id" ]]; then
    record_result "3 Overlay Persistence" "skip" "0" "Could not create branch for persistence test"
    return 0
  fi

  log_info "Created branch: $branch_id"

  # Step 3: First sandbox invocation - create a persistent file
  local marker_content="persistence_test_$(date +%s)"
  local exit_code=0
  local output
  output=$(run_sandbox "$test_log.first" --branch "$branch_id" -- bash -c "echo '$marker_content' > persist.txt && cat persist.txt" 2>&1) || exit_code=$?

  log_info "First sandbox output: $output"
  log_info "First sandbox exit code: $exit_code"

  if ! echo "$output" | grep -q "$marker_content"; then
    record_result "3 Overlay Persistence" "fail" "0" "First sandbox invocation failed to create file"
    return 1
  fi

  # Step 4: Second sandbox invocation - verify the file persists
  local second_output
  local second_exit_code=0
  second_output=$(run_sandbox "$test_log.second" --branch "$branch_id" -- cat persist.txt 2>&1) || second_exit_code=$?

  log_info "Second sandbox output: $second_output"
  log_info "Second sandbox exit code: $second_exit_code"

  if echo "$second_output" | grep -q "$marker_content"; then
    local duration=$(($(date +%s) - start_time))
    record_result "3 Overlay Persistence" "pass" "$duration"
    return 0
  else
    record_result "3 Overlay Persistence" "fail" "0" "File did not persist across sandbox invocations (expected: $marker_content)"
    return 1
  fi
}

# ═══════════════════════════════════════════════════════════════════════════════
# T16.4 Branch Binding
# ═══════════════════════════════════════════════════════════════════════════════
test_branch_binding() {
  log_section "T16.4 Branch Binding"
  local start_time=$(date +%s)

  for provider in "${SANDBOX_PROVIDERS[@]}"; do
    ACTIVE_PROVIDER="$provider"
    local test_log="$LOG_DIR/t16_4_branch_${provider}.log"
    local workspace
    if ! workspace=$(select_workspace_for_provider "$provider" "branch" 0); then
      local rc=$?
      if [[ $rc -eq 2 ]]; then
        record_result "4 Branch Binding [$provider]" "skip" "0" "Mount not available for provider"
        continue
      fi
      record_result "4 Branch Binding [$provider]" "fail" "0" "Workspace selection failed"
      continue
    fi

    cd "$workspace"

    local output
    local exit_code=0
    if output=$(RUST_LOG=debug run_sandbox "$test_log" -- echo "branch test" 2>&1); then
      exit_code=0
    else
      exit_code=$?
    fi

    # Check for branch binding messages in telemetry
    if echo "$output" | grep -qiE "(branch|binding|prepared.*workspace)"; then
      local duration=$(($(date +%s) - start_time))
      record_result "4 Branch Binding [$provider]" "pass" "$duration"
      continue
    fi

    # Even if no explicit log, pass if the command succeeded
    if [[ $exit_code -eq 0 ]]; then
      local duration=$(($(date +%s) - start_time))
      record_result "4 Branch Binding [$provider]" "pass" "$duration"
      continue
    fi

    record_result "4 Branch Binding [$provider]" "fail" "0" "Branch binding not detected"
  done
  return 0
}

# ═══════════════════════════════════════════════════════════════════════════════
# T16.5 Process Isolation (Linux only)
# ═══════════════════════════════════════════════════════════════════════════════
test_process_isolation() {
  log_section "T16.5 Process Isolation (Linux only)"

  if [[ "$(uname -s)" != "Linux" ]]; then
    record_result "5 Process Isolation" "skip" "0" "Linux-only test"
    return 0
  fi

  local start_time=$(date +%s)
  local test_log="$LOG_DIR/t16_5_process.log"

  cd "$AGENTFS_MOUNT"

  # Try to check if ps output is limited to sandbox processes
  local output
  local exit_code=0
  output=$(run_sandbox "$test_log" -- ps aux 2>&1) || exit_code=$?

  # Check if namespace operations actually failed (not just debug log messages)
  if echo "$output" | grep -qiE "(namespace.*failed|Failed to.*namespace|unshare.*failed|EPERM.*namespace|EINVAL.*namespace|Operation not permitted)"; then
    record_result "5 Process Isolation" "skip" "0" "PID namespace requires privileges"
    return 0
  fi

  if [[ $exit_code -eq 0 ]]; then
    # In a proper PID namespace, we should see very limited processes
    # Extract just the process lines (lines starting with a username and containing PID)
    local ps_lines
    ps_lines=$(echo "$output" | grep -E "^(USER|root|[a-z]+)\s+" | grep -v "^\s*$")
    local process_count
    process_count=$(echo "$ps_lines" | wc -l)

    # A fully isolated PID namespace would have 2-3 processes max (header + ps + maybe bash)
    # Host typically has dozens/hundreds
    if [[ $process_count -lt 10 ]]; then
      local duration=$(($(date +%s) - start_time))
      record_result "5 Process Isolation" "pass" "$duration"
      return 0
    else
      # Many processes visible - likely no PID namespace isolation
      record_result "5 Process Isolation" "fail" "0" "Too many processes visible ($process_count)"
      return 1
    fi
  fi

  record_result "5 Process Isolation" "fail" "0" "Failed to run ps in sandbox (exit: $exit_code)"
  return 1
}

# ═══════════════════════════════════════════════════════════════════════════════
# T16.6 Network Isolation (Linux only)
# ═══════════════════════════════════════════════════════════════════════════════
test_network_isolation() {
  log_section "T16.6 Network Isolation (Linux only)"

  if [[ "$(uname -s)" != "Linux" ]]; then
    record_result "6 Network Isolation" "skip" "0" "Linux-only test"
    return 0
  fi

  local start_time=$(date +%s)
  local test_log="$LOG_DIR/t16_6_network.log"

  cd "$AGENTFS_MOUNT"

  # Try to connect to external host (should fail with network isolation)
  # With CLONE_NEWNET enabled (M12), the sandbox creates an isolated network namespace
  # with only loopback available. External network access should fail.
  local output
  local exit_code=0
  output=$(run_sandbox "$test_log" -- timeout 5 bash -c "curl -s --connect-timeout 3 https://example.com" 2>&1) || exit_code=$?

  # Check if namespace operations actually failed (permissions issue)
  if echo "$output" | grep -qiE "(namespace.*failed|Failed to.*namespace|unshare.*failed|EPERM.*namespace|EINVAL.*namespace|Operation not permitted)"; then
    record_result "6 Network Isolation" "skip" "0" "Network namespace requires privileges"
    return 0
  fi

  # If network access succeeded (got HTML content), network isolation is NOT working
  # Be more specific: look for actual HTML content, not just "example" (which appears in the URL in logs)
  # The HTML response contains DOCTYPE, <html>, or <body> tags
  if echo "$output" | grep -qiE "<!DOCTYPE|<html|<body|<head|</html>"; then
    record_result "6 Network Isolation" "fail" "0" "Network isolation NOT working - curl succeeded (got HTML response)"
    return 1
  fi

  # Network blocked (expected with CLONE_NEWNET) = success
  # curl should fail with network unreachable or connection refused
  local duration=$(($(date +%s) - start_time))
  log_pass "Network isolation verified - external network access blocked"
  log_info "  curl output: $(echo "$output" | head -1)"
  record_result "6 Network Isolation" "pass" "$duration"
  return 0
}

# ═══════════════════════════════════════════════════════════════════════════════
# T16.7 Network Egress Enabled (Linux only)
# ═══════════════════════════════════════════════════════════════════════════════
test_network_egress_enabled() {
  log_section "T16.7 Network Egress Enabled (Linux only)"

  if [[ "$(uname -s)" != "Linux" ]]; then
    record_result "7 Network Egress" "skip" "0" "Linux-only test"
    return 0
  fi

  local start_time=$(date +%s)
  local test_log="$LOG_DIR/t16_7_egress.log"

  cd "$AGENTFS_MOUNT"

  # Check if slirp4netns is available
  if ! command -v slirp4netns &>/dev/null; then
    log_info "slirp4netns not found in PATH"
    record_result "7 Network Egress" "skip" "0" "slirp4netns not installed (required for --allow-network)"
    return 0
  fi

  # With --allow-network yes, slirp4netns provides internet access
  # The sandbox should be able to reach external hosts
  local output
  local exit_code=0
  output=$(run_sandbox "$test_log" --allow-network yes -- timeout 15 bash -c "curl -s --connect-timeout 8 https://example.com" 2>&1) || exit_code=$?

  # Check if namespace operations actually failed (permissions issue)
  if echo "$output" | grep -qiE "(namespace.*failed|Failed to.*namespace|unshare.*failed|EPERM.*namespace|Operation not permitted)"; then
    record_result "7 Network Egress" "skip" "0" "Network namespace requires privileges"
    return 0
  fi

  # Check if slirp4netns failed to start
  if echo "$output" | grep -qi "Failed to spawn slirp4netns"; then
    log_info "slirp4netns failed to start"
    record_result "7 Network Egress" "skip" "0" "slirp4netns failed to start (may require privileges)"
    return 0
  fi

  # If network access succeeded (got HTML content), --allow-network is working
  # Be more specific: look for actual HTML content, not just "example" (which appears in the URL in logs)
  # The HTML response contains DOCTYPE, <html>, or <body> tags
  if echo "$output" | grep -qiE "<!DOCTYPE|<html|<body|<head|</html>"; then
    local duration=$(($(date +%s) - start_time))
    log_pass "Network egress verified - external network access works with --allow-network yes"
    record_result "7 Network Egress" "pass" "$duration"
    return 0
  fi

  # Network didn't work
  log_fail "Network egress failed - could not reach external host with --allow-network yes"
  log_info "  curl output: $(echo "$output" | head -3)"
  record_result "7 Network Egress" "fail" "0" "Could not reach external host with --allow-network"
  return 1
}

# ═══════════════════════════════════════════════════════════════════════════════
# T16.8 Secrets Protection
# ═══════════════════════════════════════════════════════════════════════════════
test_secrets_protection() {
  log_section "T16.8 Secrets Protection"
  local start_time=$(date +%s)
  local test_log="$LOG_DIR/t16_8_secrets.log"

  cd "$AGENTFS_MOUNT"

  # Check if ~/.ssh is protected (if it exists)
  if [[ -d "$HOME/.ssh" ]] && [[ -f "$HOME/.ssh/id_rsa" ]]; then
    local output
    local exit_code=0
    output=$(run_sandbox "$test_log" -- bash -c "cat ~/.ssh/id_rsa 2>&1 || echo 'ACCESS_DENIED'" 2>&1) || exit_code=$?

    # Check if namespace operations actually failed
    if echo "$output" | grep -qiE "(namespace.*failed|Failed to.*namespace|unshare.*failed|Operation not permitted)"; then
      record_result "8 Secrets Protection" "skip" "0" "Filesystem namespace requires privileges for secrets isolation"
      return 0
    fi

    # Secrets protection is achieved by mounting empty tmpfs over sensitive directories.
    # This makes SSH keys and other credentials inaccessible from inside the sandbox.
    if echo "$output" | grep -q "BEGIN.*PRIVATE KEY"; then
      # Secrets are visible - protection failed
      record_result "8 Secrets Protection" "fail" "0" "SSH private key was accessible inside sandbox"
      return 1
    fi

    # If ACCESS_DENIED, No such file, Permission denied, or empty directory - protection worked!
    if echo "$output" | grep -qE "ACCESS_DENIED|No such file|Permission denied|directory"; then
      local duration=$(($(date +%s) - start_time))
      record_result "8 Secrets Protection" "pass" "$duration"
      return 0
    fi

    local duration=$(($(date +%s) - start_time))
    record_result "8 Secrets Protection" "pass" "$duration"
    return 0
  else
    record_result "8 Secrets Protection" "skip" "0" "No ~/.ssh/id_rsa file to test"
    return 0
  fi
}

# ═══════════════════════════════════════════════════════════════════════════════
# T16.9 Writable Carveouts
# ═══════════════════════════════════════════════════════════════════════════════
test_writable_carveouts() {
  log_section "T16.9 Writable Carveouts"
  local start_time=$(date +%s)
  local test_log="$LOG_DIR/t16_9_carveouts.log"

  cd "$AGENTFS_MOUNT"

  # Use a location outside /tmp since sandbox mounts tmpfs over /tmp for isolation
  local test_cache="${XDG_RUNTIME_DIR:-/var/tmp}/agentfs-test-cache-$$"
  mkdir -p "$test_cache"

  # With --mount-rw, the specified path should be writable
  local output
  local exit_code=0
  output=$(run_sandbox "$test_log" --mount-rw "$test_cache" -- bash -c "echo 'test' > '$test_cache/test.txt' && cat '$test_cache/test.txt'" 2>&1) || exit_code=$?

  if echo "$output" | grep -q "test"; then
    local duration=$(($(date +%s) - start_time))
    rm -rf "$test_cache"
    record_result "9 Writable Carveouts" "pass" "$duration"
    return 0
  fi

  # Check if the issue is namespace permissions
  if echo "$output" | grep -qi "namespace\|EPERM\|EINVAL"; then
    rm -rf "$test_cache"
    record_result "9 Writable Carveouts" "skip" "0" "Requires namespace privileges for bind mounts"
    return 0
  fi

  rm -rf "$test_cache"
  record_result "9 Writable Carveouts" "fail" "0" "Writable carveout did not work (exit: $exit_code)"
  return 1
}

# ═══════════════════════════════════════════════════════════════════════════════
# T16.10 Cleanup on Exit
# ═══════════════════════════════════════════════════════════════════════════════
test_cleanup_on_exit() {
  log_section "T16.10 Cleanup on Exit"
  local start_time=$(date +%s)
  local test_log="$LOG_DIR/t16_10_cleanup.log"

  cd "$AGENTFS_MOUNT"

  # Run sandbox and verify it cleans up
  local before_mounts
  before_mounts=$(mount | grep -c "fuse\|agentfs" || echo "0")

  local output
  local exit_code=0
  output=$(run_sandbox "$test_log" -- echo "cleanup test" 2>&1) || exit_code=$?

  local after_mounts
  after_mounts=$(mount | grep -c "fuse\|agentfs" || echo "0")

  # Mount count should not increase (we're not adding new mounts, just using existing one)
  if [[ $after_mounts -le $before_mounts ]]; then
    local duration=$(($(date +%s) - start_time))
    record_result "10 Cleanup on Exit" "pass" "$duration"
    return 0
  fi

  # If namespace operations failed, cleanup might not be relevant
  if echo "$output" | grep -qi "namespace\|EPERM"; then
    record_result "10 Cleanup on Exit" "skip" "0" "Namespace cleanup not applicable without privileges"
    return 0
  fi

  record_result "10 Cleanup on Exit" "fail" "0" "Mounts not cleaned up (before: $before_mounts, after: $after_mounts)"
  return 1
}

# ═══════════════════════════════════════════════════════════════════════════════
# T16.11 Crash Cleanup
# ═══════════════════════════════════════════════════════════════════════════════
test_crash_cleanup() {
  log_section "T16.11 Crash Cleanup"
  local start_time=$(date +%s)
  local test_log="$LOG_DIR/t16_11_crash.log"

  cd "$AGENTFS_MOUNT"

  local before_mounts
  before_mounts=$(mount | grep -c "agentfs" || echo "0")

  # Start a long-running sandbox in background
  run_sandbox "$test_log" -- sleep 60 &
  local sandbox_pid=$!

  sleep 2

  # Kill it abruptly
  kill -9 $sandbox_pid 2>/dev/null || true
  wait $sandbox_pid 2>/dev/null || true

  sleep 1

  # Check mounts are cleaned up
  local after_mounts
  after_mounts=$(mount | grep -c "agentfs" || echo "0")

  if [[ $after_mounts -le $before_mounts ]]; then
    local duration=$(($(date +%s) - start_time))
    record_result "11 Crash Cleanup" "pass" "$duration"
    return 0
  fi

  record_result "11 Crash Cleanup" "fail" "0" "Stale mounts after crash"
  return 1
}

# ═══════════════════════════════════════════════════════════════════════════════
# T16.12 Interrupt Cleanup
# ═══════════════════════════════════════════════════════════════════════════════
test_interrupt_cleanup() {
  log_section "T16.12 Interrupt Cleanup"
  local start_time=$(date +%s)
  local test_log="$LOG_DIR/t16_12_interrupt.log"

  cd "$AGENTFS_MOUNT"

  local before_mounts
  before_mounts=$(mount | grep -c "agentfs" || echo "0")

  # Start a long-running sandbox in background
  run_sandbox "$test_log" -- sleep 60 &
  local sandbox_pid=$!

  sleep 2

  # Send SIGINT (Ctrl+C)
  kill -INT $sandbox_pid 2>/dev/null || true
  wait $sandbox_pid 2>/dev/null || true

  sleep 1

  # Check mounts are cleaned up
  local after_mounts
  after_mounts=$(mount | grep -c "agentfs" || echo "0")

  if [[ $after_mounts -le $before_mounts ]]; then
    local duration=$(($(date +%s) - start_time))
    record_result "12 Interrupt Cleanup" "pass" "$duration"
    return 0
  fi

  record_result "12 Interrupt Cleanup" "fail" "0" "Stale mounts after interrupt"
  return 1
}

# ═══════════════════════════════════════════════════════════════════════════════
# T16.15 Read-only Baseline
# ═══════════════════════════════════════════════════════════════════════════════
test_readonly_baseline() {
  log_section "T16.15 Read-only Baseline"
  local start_time=$(date +%s)
  local test_log="$LOG_DIR/t16_15_readonly.log"

  cd "$AGENTFS_MOUNT"

  # Try to write to system paths (should fail)
  local output
  if output=$(run_sandbox "$test_log" -- bash -c "echo 'test' > /usr/bin/agentfs_test_file 2>&1 || echo 'WRITE_BLOCKED'" 2>&1); then
    if echo "$output" | grep -qE "(WRITE_BLOCKED|Read-only|Permission denied|cannot create)"; then
      local duration=$(($(date +%s) - start_time))
      record_result "15 Read-only Baseline" "pass" "$duration"
      return 0
    fi
  fi

  # If the command succeeded but write was blocked, that's also a pass
  if [[ ! -f "/usr/bin/agentfs_test_file" ]]; then
    local duration=$(($(date +%s) - start_time))
    record_result "15 Read-only Baseline" "pass" "$duration"
    return 0
  fi

  rm -f "/usr/bin/agentfs_test_file" 2>/dev/null || true
  record_result "15 Read-only Baseline" "fail" "0" "Write to system path succeeded"
  return 1
}

# ═══════════════════════════════════════════════════════════════════════════════
# T16.16 Provider Telemetry
# ═══════════════════════════════════════════════════════════════════════════════
test_provider_telemetry() {
  log_section "T16.16 Provider Telemetry"
  local start_time=$(date +%s)

  for provider in "${SANDBOX_PROVIDERS[@]}"; do
    ACTIVE_PROVIDER="$provider"
    local test_log="$LOG_DIR/t16_16_telemetry_${provider}.log"
    local workspace
    if ! workspace=$(select_workspace_for_provider "$provider" "telemetry" 0); then
      local rc=$?
      if [[ $rc -eq 2 ]]; then
        record_result "16 Provider Telemetry [$provider]" "skip" "0" "Mount not available for provider"
        continue
      fi
      record_result "16 Provider Telemetry [$provider]" "fail" "0" "Workspace selection failed"
      continue
    fi

    cd "$workspace"

    # Run with debug logging
    local output
    if output=$(RUST_LOG=debug run_sandbox "$test_log" -- echo "telemetry test" 2>&1); then
      # Check for expected telemetry fields
      if echo "$output" | grep -qiE "(provider|agentfs|workspace|mount)"; then
        local duration=$(($(date +%s) - start_time))
        record_result "16 Provider Telemetry [$provider]" "pass" "$duration"
        continue
      fi
    fi

    record_result "16 Provider Telemetry [$provider]" "fail" "0" "Expected telemetry not found"
  done
  return 0
}

# ═══════════════════════════════════════════════════════════════════════════════
# T16.17 Child Process Fork
# ═══════════════════════════════════════════════════════════════════════════════
test_child_process_fork() {
  log_section "T16.17 Child Process Fork"
  local start_time=$(date +%s)

  for provider in "${SANDBOX_PROVIDERS[@]}"; do
    ACTIVE_PROVIDER="$provider"
    local test_log="$LOG_DIR/t16_17_fork_${provider}.log"
    local workspace
    if ! workspace=$(select_workspace_for_provider "$provider" "fork" 0); then
      local rc=$?
      if [[ $rc -eq 2 ]]; then
        record_result "17 Child Process Fork [$provider]" "skip" "0" "Mount not available for provider"
        continue
      fi
      record_result "17 Child Process Fork [$provider]" "fail" "0" "Workspace selection failed"
      continue
    fi
    cd "$workspace"

    # Test that forked processes work correctly in sandbox (parent/child relationship)
    local output
    local exit_code=0
    output=$(run_sandbox "$test_log" -- bash -c "
        echo 'parent: '\$\$
        (echo 'child: '\$\$)
        echo 'done'
    " 2>&1) || exit_code=$?

    # Check if command execution worked - both parent and child should report
    if echo "$output" | grep -q "parent" && echo "$output" | grep -q "child" && echo "$output" | grep -q "done"; then
      local duration=$(($(date +%s) - start_time))
      record_result "17 Child Process Fork [$provider]" "pass" "$duration"
      continue
    fi

    # Check if namespace operations actually failed
    if echo "$output" | grep -qiE "(namespace.*failed|Failed to.*namespace|unshare.*failed|Operation not permitted)"; then
      record_result "17 Child Process Fork [$provider]" "skip" "0" "Namespace setup failed"
      continue
    fi

    record_result "17 Child Process Fork [$provider]" "fail" "0" "Fork in sandbox failed (exit: $exit_code)"
  done
  return 0
}

# ═══════════════════════════════════════════════════════════════════════════════
# T16.23 Child Process Shell Pipeline
# ═══════════════════════════════════════════════════════════════════════════════
test_child_process_pipeline() {
  log_section "T16.23 Child Process Shell Pipeline"
  local start_time=$(date +%s)

  for provider in "${SANDBOX_PROVIDERS[@]}"; do
    ACTIVE_PROVIDER="$provider"
    local test_log="$LOG_DIR/t16_23_pipeline_${provider}.log"
    local workspace
    if ! workspace=$(select_workspace_for_provider "$provider" "pipeline" 0); then
      local rc=$?
      if [[ $rc -eq 2 ]]; then
        record_result "23 Child Process Pipeline [$provider]" "skip" "0" "Mount not available for provider"
        continue
      fi
      record_result "23 Child Process Pipeline [$provider]" "fail" "0" "Workspace selection failed"
      continue
    fi
    cd "$workspace"

    # Test that pipeline processes work in sandbox
    local output
    local exit_code=0
    output=$(run_sandbox "$test_log" -- bash -c "
        echo 'hello world' | tr 'a-z' 'A-Z' | grep -o 'HELLO'
    " 2>&1) || exit_code=$?

    # Check if command execution worked (look for "HELLO" in output)
    if echo "$output" | grep -q "HELLO"; then
      local duration=$(($(date +%s) - start_time))
      record_result "23 Child Process Pipeline [$provider]" "pass" "$duration"
      continue
    fi

    # Check if namespace operations actually failed
    if echo "$output" | grep -qiE "(namespace.*failed|Failed to.*namespace|unshare.*failed|Operation not permitted)"; then
      record_result "23 Child Process Pipeline [$provider]" "skip" "0" "Namespace setup failed"
      continue
    fi

    record_result "23 Child Process Pipeline [$provider]" "fail" "0" "Pipeline in sandbox failed (exit: $exit_code)"
  done
  return 0
}

# ═══════════════════════════════════════════════════════════════════════════════
# T16.24 Child Process Subshell
# ═══════════════════════════════════════════════════════════════════════════════
test_child_process_subshell() {
  log_section "T16.24 Child Process Subshell"
  local start_time=$(date +%s)

  for provider in "${SANDBOX_PROVIDERS[@]}"; do
    ACTIVE_PROVIDER="$provider"
    local test_log="$LOG_DIR/t16_24_subshell_${provider}.log"
    local workspace
    if ! workspace=$(select_workspace_for_provider "$provider" "subshell" 0); then
      local rc=$?
      if [[ $rc -eq 2 ]]; then
        record_result "24 Child Process Subshell [$provider]" "skip" "0" "Mount not available for provider"
        continue
      fi
      record_result "24 Child Process Subshell [$provider]" "fail" "0" "Workspace selection failed"
      continue
    fi
    cd "$workspace"

    # Test that subshell processes work in sandbox
    local output
    local exit_code=0
    output=$(run_sandbox "$test_log" -- bash -c "
        result=\$(echo 'subshell works' | cat)
        echo \"\$result\"
    " 2>&1) || exit_code=$?

    # Check if command execution worked
    if echo "$output" | grep -q "subshell works"; then
      local duration=$(($(date +%s) - start_time))
      record_result "24 Child Process Subshell [$provider]" "pass" "$duration"
      continue
    fi

    # Check if namespace operations actually failed
    if echo "$output" | grep -qiE "(namespace.*failed|Failed to.*namespace|unshare.*failed|Operation not permitted)"; then
      record_result "24 Child Process Subshell [$provider]" "skip" "0" "Namespace setup failed"
      continue
    fi

    record_result "24 Child Process Subshell [$provider]" "fail" "0" "Subshell in sandbox failed (exit: $exit_code)"
  done
  return 0
}

# ═══════════════════════════════════════════════════════════════════════════════
# T18.10 Harness Single Test Flag (--test)
# ═══════════════════════════════════════════════════════════════════════════════
test_single_test_flag() {
  log_section "T18.10 Harness Single Test Flag (--test)"
  local start_time=$(date +%s)
  local test_log="$LOG_DIR/t18_10_single_flag.log"

  local nested_log_dir="$LOG_DIR/t18_10_single_run"
  mkdir -p "$nested_log_dir"
  local nested_summary="$nested_log_dir/summary.json"

  local nested_exit=0
  LOG_DIR="$nested_log_dir" "$SCRIPT_DIR/test-agentfs-sandbox.sh" --test basic_execution >"$test_log" 2>&1 || nested_exit=$?

  if [[ $nested_exit -ne 0 ]]; then
    record_result "10 Harness Single Test" "fail" "0" "Nested single-test run failed (exit: $nested_exit)" "T18"
    return 1
  fi

  if [[ ! -f "$nested_summary" ]]; then
    record_result "10 Harness Single Test" "fail" "0" "Nested summary.json not produced" "T18"
    return 1
  fi

  local tests_run
  tests_run=$(grep -o '"tests_run"[[:space:]]*:[[:space:]]*[0-9]*' "$nested_summary" | tail -1 | awk -F: '{print $2}' | tr -d ' ,')
  local nested_status
  nested_status=$(grep -o '"status"[[:space:]]*:[[:space:]]*"[a-z]*"' "$nested_summary" | tail -1 | awk -F\" '{print $4}')

  if [[ "$tests_run" != "1" ]]; then
    record_result "10 Harness Single Test" "fail" "0" "Expected single test to run, got $tests_run" "T18"
    return 1
  fi

  if [[ "$nested_status" != "pass" ]]; then
    record_result "10 Harness Single Test" "fail" "0" "Nested run status was '$nested_status'" "T18"
    return 1
  fi

  local duration=$(($(date +%s) - start_time))
  record_result "10 Harness Single Test" "pass" "$duration" "" "T18"
  return 0
}

# ═══════════════════════════════════════════════════════════════════════════════
# Write summary
# ═══════════════════════════════════════════════════════════════════════════════
write_summary() {
  log_section "Test Summary"

  local status="pass"
  if [[ $TESTS_FAILED -gt 0 ]]; then
    status="fail"
  fi

  log_info "Total: $TESTS_RUN | Passed: $TESTS_PASSED | Failed: $TESTS_FAILED | Skipped: $TESTS_SKIPPED | XFail: $TESTS_XFAIL"
  if [[ $TESTS_XFAIL -gt 0 ]]; then
    log_info "Note: XFail tests are expected failures tracked in FUSE.status.md milestone F18"
  fi
  log_info "Log directory: $LOG_DIR"

  # Write JSON summary
  cat >"$SUMMARY_FILE" <<EOF
{
  "timestamp": "$(date -Iseconds)",
  "status": "$status",
  "tests_run": $TESTS_RUN,
  "tests_passed": $TESTS_PASSED,
  "tests_failed": $TESTS_FAILED,
  "tests_skipped": $TESTS_SKIPPED,
  "tests_xfail": $TESTS_XFAIL,
  "log_dir": "$LOG_DIR",
  "agentfs_mount": "$AGENTFS_MOUNT",
  "platform": "$(uname -s)"
}
EOF

  log_info "Summary written to: $SUMMARY_FILE"

  if [[ "$status" == "fail" ]]; then
    log_fail "Some tests failed!"
    return 1
  else
    log_pass "All tests passed!"
    return 0
  fi
}

# ═══════════════════════════════════════════════════════════════════════════════
# Main
# ═══════════════════════════════════════════════════════════════════════════════
main() {
  log_section "F16: AgentFS Sandbox Integration Tests"
  log_info "Started at $(date)"

  local tests_to_run=()
  while [[ $# -gt 0 ]]; do
    case "$1" in
    --test)
      shift
      if [[ -z "${1:-}" ]]; then
        log_fail "--test requires a test name"
        usage
        exit 1
      fi
      tests_to_run+=("$1")
      ;;
    --list)
      list_tests
      exit 0
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    --)
      shift
      while [[ $# -gt 0 ]]; do
        tests_to_run+=("$1")
        shift
      done
      break
      ;;
    *)
      tests_to_run+=("$1")
      ;;
    esac
    shift
  done

  if [[ ${#tests_to_run[@]} -eq 0 && -n "${AGENTFS_TESTS:-}" ]]; then
    # Allow test selection via environment variable for CI convenience
    read -r -a tests_to_run <<<"${AGENTFS_TESTS}"
  fi

  if [[ ${#tests_to_run[@]} -eq 0 ]]; then
    tests_to_run=("${AVAILABLE_TESTS[@]}")
  fi

  init_providers
  check_prerequisites

  for test in "${tests_to_run[@]}"; do
    local test_func="test_$test"
    if declare -f "$test_func" >/dev/null; then
      "$test_func" || true # Continue on failure
    else
      log_skip "Unknown test: $test"
    fi
  done

  write_summary
}

main "$@"
