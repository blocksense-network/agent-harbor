#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

usage() {
  cat <<'EOF'
Usage: scripts/run-fuse-regression.sh [mountpoint]

Runs the full AgentFS FUSE regression stack:
  1. just test-rust
  2. Manual mount + sudo just test-fuse-basic <mountpoint>
  3. just test-fuse-basic-ops
  4. just test-fuse-mount-cycle
  5. just test-fuse-mount-concurrent
  6. just test-pjdfstest-full (verifies against expected failure list)

Defaults:
  mountpoint: /tmp/agentfs

Requires password-less sudo for the pjdfstest + fuse-basic smoke tests.
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

MOUNTPOINT="${1:-/tmp/agentfs}"
LOG_ROOT="$REPO_ROOT/logs/fuse-e2e-$(date +%Y%m%d-%H%M%S)"
mkdir -p "$LOG_ROOT"
E2E_LOG="$LOG_ROOT/e2e.log"
touch "$E2E_LOG"

log() {
  local msg="$*"
  printf '[%s] %s\n' "$(date +%H:%M:%S)" "$msg" | tee -a "$E2E_LOG"
}

slugify() {
  echo "$1" | tr '[:upper:]' '[:lower:]' | tr -cs 'a-z0-9' '-' | sed -e 's/^-*//' -e 's/-*$//'
}

run_step() {
  local desc="$1"
  shift
  local logfile="$LOG_ROOT/$(slugify "$desc").log"
  log "▶ $desc (log: $logfile)"
  if ! (
    set -o pipefail
    "$@" |& tee "$logfile"
  ); then
    log "❌ $desc failed. See $logfile for details."
    exit 1
  fi
}

cleanup_mount() {
  if mountpoint -q "$MOUNTPOINT" 2>/dev/null; then
    log "Unmounting leftover mount at $MOUNTPOINT"
    if ! "$SCRIPT_DIR/umount-fuse.sh" "$MOUNTPOINT" >>"$LOG_ROOT/cleanup.log" 2>&1; then
      log "⚠️  Failed to unmount $MOUNTPOINT during cleanup. Manual intervention may be required."
    fi
  fi
}

trap cleanup_mount EXIT

log "Starting AgentFS FUSE regression run (FUSE suites only)"
log "Repository root: $REPO_ROOT"
log "Logs under: $LOG_ROOT"

mkdir -p "$MOUNTPOINT"
cleanup_mount

run_step "Mount AgentFS at $MOUNTPOINT" env AGENTFS_FUSE_ALLOW_OTHER=1 just mount-fuse "$MOUNTPOINT"
if ! mountpoint -q "$MOUNTPOINT"; then
  log "❌ $MOUNTPOINT is not a mountpoint after mount-fuse"
  exit 1
fi

run_step "FUSE basic smoke at $MOUNTPOINT" sudo just test-fuse-basic "$MOUNTPOINT"
run_step "Unmount manual mount" just umount-fuse "$MOUNTPOINT"

run_step "Comprehensive FUSE basic ops" just test-fuse-basic-ops
run_step "FUSE mount cycle harness" just test-fuse-mount-cycle
run_step "FUSE concurrent mount harness" just test-fuse-mount-concurrent

before_pjdfs_dirs="$(find "$REPO_ROOT/logs" -maxdepth 1 -type d -name 'pjdfstest-full-*' -print 2>/dev/null | wc -l)"
run_step "pjdfstest full suite" just test-pjdfstest-full

latest_pjdfs_dir="$(find "$REPO_ROOT/logs" -maxdepth 1 -type d -name 'pjdfstest-full-*' -printf '%T@ %p\n' | sort -nr | head -n1 | cut -d' ' -f2)"
if [[ -z "${latest_pjdfs_dir:-}" || ! -f "$latest_pjdfs_dir/summary.json" ]]; then
  log "❌ Unable to locate pjdfstest summary.json after run"
  exit 1
fi

log "Latest pjdfstest logs: $latest_pjdfs_dir"

python3 - "$latest_pjdfs_dir/summary.json" <<'PY'
import json
import sys
from pathlib import Path

summary_path = Path(sys.argv[1])
summary = json.loads(summary_path.read_text())
expected = {
    "resources/pjdfstest/tests/chown/00.t",
    "resources/pjdfstest/tests/chown/05.t",
    "resources/pjdfstest/tests/ftruncate/05.t",
    "resources/pjdfstest/tests/open/00.t",
    "resources/pjdfstest/tests/open/06.t",
    "resources/pjdfstest/tests/rename/00.t",
    "resources/pjdfstest/tests/rename/09.t",
    "resources/pjdfstest/tests/rename/10.t",
    "resources/pjdfstest/tests/symlink/06.t",
    "resources/pjdfstest/tests/truncate/05.t",
    "resources/pjdfstest/tests/chmod/12.t",
}
observed = {entry["program"].split()[0] for entry in summary.get("failed_programs", [])}
unexpected = sorted(observed - expected)
missing = sorted(expected - observed)
if unexpected or missing:
    for extra in unexpected:
        print(f"Unexpected pjdfstest failure detected: {extra}")
    for miss in missing:
        print(f"Expected pjdfstest failure missing from summary: {miss}")
    sys.exit(1)
print("pjdfstest summary matches expected failure set:")
for program in sorted(observed):
    print(f"  - {program}")
PY

log "✅ AgentFS FUSE regression run completed successfully."
log "Artifacts:"
for file in "$LOG_ROOT"/*.log; do
  printf '  - %s\n' "$file"
done | tee -a "$E2E_LOG"
log "pjdfstest artifacts: $latest_pjdfs_dir"
