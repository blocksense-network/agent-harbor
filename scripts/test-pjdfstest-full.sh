#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
LOG_ROOT="$REPO_ROOT/logs"
TS="$(date +%Y%m%d-%H%M%S)"
RUN_DIR="$LOG_ROOT/pjdfstest-full-$TS"
LOG_FILE="$RUN_DIR/pjdfstest.log"
SUMMARY_FILE="$RUN_DIR/summary.json"
MOUNTPOINT="${1:-/tmp/agentfs}"
PJDFSTEST_DIR="$REPO_ROOT/resources/pjdfstest"
SKIP_FUSE_BUILD="${SKIP_FUSE_BUILD:-}"
HARNESS_OPTIONS="${HARNESS_OPTIONS:-j32}"
mkdir -p "$RUN_DIR"
export HARNESS_OPTIONS

log() {
  echo "[$(date +%H:%M:%S)] $*" | tee -a "$LOG_FILE"
}

wait_for_mount_state() {
  local mount_path="$1"
  local expect="$2"
  local max_attempts=50
  local attempt=0
  while ((attempt < max_attempts)); do
    if mountpoint -q "$mount_path" 2>/dev/null; then
      if [[ "$expect" == "mounted" ]]; then
        return 0
      fi
    else
      if [[ "$expect" == "unmounted" ]]; then
        return 0
      fi
    fi
    sleep 0.1
    ((attempt += 1))
  done
  log "Timed out waiting for $mount_path to become $expect"
  return 1
}

CURRENT_MOUNT_PRIV="none"

cleanup() {
  unmount_agentfs "$CURRENT_MOUNT_PRIV"
}
trap cleanup EXIT

if [[ ! -d "$PJDFSTEST_DIR/tests" ]]; then
  log "pjdfstest suite missing; running setup"
  just setup-pjdfstest-suite >>"$LOG_FILE" 2>&1
fi

if [[ -z "$SKIP_FUSE_BUILD" ]]; then
  log "Building FUSE host for pjdfstest run"
  just build-fuse-test-binaries >>"$LOG_FILE" 2>&1
else
  log "SKIP_FUSE_BUILD set; skipping build"
fi

mount_agentfs() {
  local privileged="$1"
  local log_path="$2"
  export AGENTFS_FUSE_LOG_FILE="$log_path"
  export RUST_BACKTRACE="${RUST_BACKTRACE:-1}"
  log "FUSE host log: $AGENTFS_FUSE_LOG_FILE"
  if [[ "$privileged" == "sudo" ]]; then
    sudo env AGENTFS_FUSE_ALLOW_OTHER=1 AGENTFS_FUSE_LOG_FILE="$AGENTFS_FUSE_LOG_FILE" \
      AGENTFS_FUSE_PRIVILEGED=1 AGENTFS_FUSE_SKIP_AUTO_CHOWN=1 \
      just mount-fuse "$MOUNTPOINT" >>"$LOG_FILE" 2>&1
  else
    AGENTFS_FUSE_ALLOW_OTHER=1 AGENTFS_FUSE_SKIP_AUTO_CHOWN=1 \
      just mount-fuse "$MOUNTPOINT" >>"$LOG_FILE" 2>&1
  fi
  if ! wait_for_mount_state "$MOUNTPOINT" "mounted"; then
    log "Failed to verify mount at $MOUNTPOINT; aborting pjdfstest run."
    exit 1
  fi
}

unmount_agentfs() {
  local privileged="$1"
  if mountpoint -q "$MOUNTPOINT" 2>/dev/null; then
    if [[ "$privileged" == "sudo" ]]; then
      sudo just umount-fuse "$MOUNTPOINT" >>"$LOG_FILE" 2>&1 || true
    else
      just umount-fuse "$MOUNTPOINT" >>"$LOG_FILE" 2>&1 || true
    fi
    wait_for_mount_state "$MOUNTPOINT" "unmounted"
  fi
}

log "Mounting AgentFS at $MOUNTPOINT"
mount_agentfs "user" "$RUN_DIR/fuse-host.log"
CURRENT_MOUNT_PRIV="user"

log "Selecting pjdfstest subsets"
SUDO_TEST_LIST="${PJDFSTEST_SUDO_TESTS:-chmod/12.t}"
read -ra SUDO_TESTS <<<"$SUDO_TEST_LIST"
declare -A SUDO_LOOKUP=()
for test in "${SUDO_TESTS[@]}"; do
  [[ -z "$test" ]] && continue
  SUDO_LOOKUP["$test"]=1
done

mapfile -t ALL_TESTS < <(cd "$PJDFSTEST_DIR/tests" && find . -name '*.t' | sort)
NON_SUDO_FILES=()
SUDO_FILES=()
for rel in "${ALL_TESTS[@]}"; do
  rel="${rel#./}"
  abs="$PJDFSTEST_DIR/tests/$rel"
  if [[ -n "${SUDO_LOOKUP[$rel]+x}" ]]; then
    SUDO_FILES+=("$abs")
  else
    NON_SUDO_FILES+=("$abs")
  fi
done

if [[ -n ${SUDO_LOOKUP["chmod/12.t"]+x} ]]; then
  log "NOTE: chmod/12.t stays out of the main run because the Linux kernel rejects writes to SUID files on user-mounted FUSE before AgentFS can clear the bits; it still runs in a dedicated privileged pass."
fi

run_prove_suite() {
  local label="$1"
  local use_sudo="$2"
  shift 2
  local -a tests=("$@")
  if ((${#tests[@]} == 0)); then
    log "No tests to run for ${label}; skipping."
    return 0
  fi
  log "Running ${label} via prove (count ${#tests[@]}; HARNESS_OPTIONS=${HARNESS_OPTIONS})"
  set +e
  set +o pipefail
  if [[ "$use_sudo" == "sudo" ]]; then
    sudo -E env HARNESS_OPTIONS="$HARNESS_OPTIONS" bash -c '
      mountpoint="$1"
      shift
      cd "$mountpoint"
      prove -vr "$@"
    ' bash "$MOUNTPOINT" "${tests[@]}" 2>&1 | tee -a "$LOG_FILE"
  else
    env HARNESS_OPTIONS="$HARNESS_OPTIONS" bash -c '
      mountpoint="$1"
      shift
      cd "$mountpoint"
      prove -vr "$@"
    ' bash "$MOUNTPOINT" "${tests[@]}" 2>&1 | tee -a "$LOG_FILE"
  fi
  local status=${PIPESTATUS[0]}
  set -e
  set -o pipefail
  return $status
}

MAIN_STATUS=0
if ! run_prove_suite "sudo pjdfstest subset (sans privileged list)" "sudo" "${NON_SUDO_FILES[@]}"; then
  MAIN_STATUS=$?
fi

SUDO_STATUS=0
if ((${#SUDO_FILES[@]} > 0)); then
  log "Preparing privileged mount for sudo-only subset"
  unmount_agentfs "$CURRENT_MOUNT_PRIV"
  CURRENT_MOUNT_PRIV="none"
  mount_agentfs "sudo" "$RUN_DIR/fuse-host-priv.log"
  CURRENT_MOUNT_PRIV="sudo"
  if ! run_prove_suite "sudo-only privileged subset" "sudo" "${SUDO_FILES[@]}"; then
    SUDO_STATUS=$?
  fi
  log "Unmounting privileged mount"
  unmount_agentfs "$CURRENT_MOUNT_PRIV"
  CURRENT_MOUNT_PRIV="none"
else
  log "No privileged pjdfstest files configured; skipping privileged mount."
fi

PROVE_STATUS=0
if [[ $MAIN_STATUS -ne 0 || $SUDO_STATUS -ne 0 ]]; then
  PROVE_STATUS=1
fi

log "Parsing pjdfstest output into $SUMMARY_FILE"
python3 - "$LOG_FILE" "$SUMMARY_FILE" "$REPO_ROOT" <<'PY'
import json
import re
import sys
from pathlib import Path

log_path = Path(sys.argv[1])
out_path = Path(sys.argv[2])
repo_root = Path(sys.argv[3])
lines = log_path.read_text().splitlines()
summary = {
    "result": "UNKNOWN",
    "files": None,
    "tests": None,
    "failed_programs": [],
    "skipped_programs": [],
    "failed_program_count": 0,
    "failed_subtests": None,
    "log_file": str(log_path.relative_to(repo_root)),
}
report_entries = []
collect_report = False
current = None
for line in lines:
    if line.startswith("Test Summary Report"):
        collect_report = True
        continue
    if collect_report:
        if line.startswith("Files=") or line.startswith("Result:"):
            if current:
                report_entries.append(current)
                current = None
            collect_report = False
        elif not line.strip() or set(line.strip()) == {"-"}:
            continue
        elif not line.startswith(" "):
            if current:
                report_entries.append(current)
            program = line.strip()
            if program.startswith(str(repo_root)):
                try:
                    program = str(Path(program).relative_to(repo_root))
                except ValueError:
                    pass
            current = {"program": program, "details": []}
        else:
            if current:
                current["details"].append(line.strip())
    if line.startswith("Files="):
        m = re.search(r"Files=(\d+), Tests=(\d+)", line)
        if m:
            summary["files"] = int(m.group(1))
            summary["tests"] = int(m.group(2))
    elif line.startswith("Failed "):
        m = re.search(r"Failed (\d+)/(\d+) test programs\. (\d+)/(\d+) subtests failed", line)
        if m:
            summary["failed_program_count"] = int(m.group(1))
            summary["failed_subtests"] = {
                "failed": int(m.group(3)),
                "total": int(m.group(4)),
            }
    elif line.startswith("Result:"):
        summary["result"] = line.split(":", 1)[1].strip()

if current:
    report_entries.append(current)

for entry in report_entries:
    program = entry["program"]
    detail_text = " ".join(entry["details"]).lower()
    if "skipped" in detail_text:
        summary["skipped_programs"].append({"program": program, "details": entry["details"]})
    else:
        summary["failed_programs"].append({"program": program, "details": entry["details"]})

summary["failed_program_count"] = len(summary["failed_programs"]) or summary["failed_program_count"]
out_path.write_text(json.dumps(summary, indent=2))
PY

log "Summary written to $SUMMARY_FILE"

BASELINE_PATH="${PJDFSTEST_BASELINE:-$REPO_ROOT/specs/Public/AgentFS/pjdfstest.baseline.json}"
DIFF_FILE="$RUN_DIR/baseline_diff.json"
BASELINE_MATCHED=0

if [[ -f "$BASELINE_PATH" ]]; then
  log "Comparing results with baseline $BASELINE_PATH"
  if python3 - "$SUMMARY_FILE" "$BASELINE_PATH" "$DIFF_FILE" <<'PY'; then
import json
import sys
from pathlib import Path

summary_path = Path(sys.argv[1])
baseline_path = Path(sys.argv[2])
diff_path = Path(sys.argv[3])
summary = json.loads(summary_path.read_text())
baseline = json.loads(baseline_path.read_text())

def normalize(entries):
    normalized = []
    for entry in entries:
        normalized.append(
            {
                "program": entry["program"],
                "details": entry.get("details", []),
            }
        )
    normalized.sort(key=lambda e: e["program"])
    return normalized

result_differs = summary.get("result") != baseline.get("result")
failed_expected = normalize(baseline.get("failed_programs", []))
failed_observed = normalize(summary.get("failed_programs", []))
skipped_expected = normalize(baseline.get("skipped_programs", []))
skipped_observed = normalize(summary.get("skipped_programs", []))

def diff_lists(expected, observed):
    expected_map = {item["program"]: item["details"] for item in expected}
    observed_map = {item["program"]: item["details"] for item in observed}
    extra = []
    missing = []
    mismatched = []
    for program, details in observed_map.items():
        if program not in expected_map:
            extra.append({"program": program, "details": details})
        elif expected_map[program] != details:
            mismatched.append(
                {
                    "program": program,
                    "expected": expected_map[program],
                    "observed": details,
                }
            )
    for program, details in expected_map.items():
        if program not in observed_map:
            missing.append({"program": program, "details": details})
    return extra, missing, mismatched

extra_failed, missing_failed, mismatched_failed = diff_lists(failed_expected, failed_observed)
extra_skipped, missing_skipped, mismatched_skipped = diff_lists(skipped_expected, skipped_observed)

diff = {
    "result_differs": result_differs,
    "extra_failed_programs": extra_failed,
    "missing_failed_programs": missing_failed,
    "mismatched_failed_programs": mismatched_failed,
    "extra_skipped_programs": extra_skipped,
    "missing_skipped_programs": missing_skipped,
    "mismatched_skipped_programs": mismatched_skipped,
}

diff_path.write_text(json.dumps(diff, indent=2))

has_unexpected = (
    diff["result_differs"]
    or diff["extra_failed_programs"]
    or diff["mismatched_failed_programs"]
    or diff["extra_skipped_programs"]
    or diff["mismatched_skipped_programs"]
)
if diff["missing_failed_programs"] or diff["missing_skipped_programs"]:
    print("Note: some expected pjdfstest failures/skips were not observed in this run.")
    if diff["missing_failed_programs"]:
        print("  Missing failed programs:")
        for entry in diff["missing_failed_programs"]:
            print(f"    - {entry['program']}")
    if diff["missing_skipped_programs"]:
        print("  Missing skipped programs:")
        for entry in diff["missing_skipped_programs"]:
            print(f"    - {entry['program']}")
if has_unexpected:
    sys.exit(1)
PY
    log "Baseline comparison successful"
    BASELINE_MATCHED=1
  else
    log "Baseline mismatch detected; see $DIFF_FILE"
    exit 1
  fi
else
  log "Baseline $BASELINE_PATH not found; skipping comparison"
fi

if [[ $PROVE_STATUS -ne 0 ]]; then
  if [[ $BASELINE_MATCHED -eq 1 ]]; then
    log "pjdfstest suite failed (exit $PROVE_STATUS) but matches baseline; continuing."
  else
    log "pjdfstest suite failed (exit $PROVE_STATUS). See $SUMMARY_FILE for details."
    exit $PROVE_STATUS
  fi
fi

log "pjdfstest suite completed successfully"
