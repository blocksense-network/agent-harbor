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
mkdir -p "$RUN_DIR"

log() {
  echo "[$(date +%H:%M:%S)] $*" | tee -a "$LOG_FILE"
}

cleanup() {
  if mountpoint -q "$MOUNTPOINT" 2>/dev/null; then
    log "Unmounting $MOUNTPOINT"
    just umount-fuse "$MOUNTPOINT" >>"$LOG_FILE" 2>&1 || true
  fi
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

log "Mounting AgentFS at $MOUNTPOINT"
AGENTFS_FUSE_ALLOW_OTHER=1 just mount-fuse "$MOUNTPOINT" >>"$LOG_FILE" 2>&1

log "Running full pjdfstest suite via prove"
set +e
set +o pipefail
sudo -E bash -c "cd '$MOUNTPOINT' && prove -vr '$PJDFSTEST_DIR/tests'" 2>&1 | tee -a "$LOG_FILE"
PROVE_STATUS=${PIPESTATUS[0]}
set -e
set -o pipefail

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

if diff["result_differs"] or diff["extra_failed_programs"] or diff["missing_failed_programs"] or diff["mismatched_failed_programs"] or diff["extra_skipped_programs"] or diff["missing_skipped_programs"] or diff["mismatched_skipped_programs"]:
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
