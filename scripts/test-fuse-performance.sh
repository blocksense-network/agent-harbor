#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
LOG_ROOT="$REPO_ROOT/logs"
TS="$(date +%Y%m%d-%H%M%S)"
RUN_DIR="$LOG_ROOT/fuse-performance-$TS"
RESULTS_FILE="$RUN_DIR/results.jsonl"
SUMMARY_FILE="$RUN_DIR/summary.json"
DEFAULT_MOUNTPOINT="/tmp/agentfs-perf-$TS"
MOUNTPOINT="${1:-$DEFAULT_MOUNTPOINT}"
BASELINE_DIR="$RUN_DIR/baseline"
SKIP_FUSE_BUILD="${SKIP_FUSE_BUILD:-}"
SKIP_THRESHOLD_CHECK="${SKIP_THRESHOLD_CHECK:-}"
NOFILE_LIMIT="${PERF_NOFILE_LIMIT:-8192}"
TIME_BIN="$(command -v time || command -v /usr/bin/time || true)"
AGENTFS_WORKDIR="$MOUNTPOINT/perf"
FUSE_CONFIG="$RUN_DIR/fuse-config.json"
BACKSTORE_DIR="$RUN_DIR/backstore"
USER_ID="$(id -u)"
GROUP_ID="$(id -g)"
SEQ_BLOCK_SIZE_BYTES=${SEQ_BLOCK_SIZE_BYTES:-$((8 * 1024 * 1024))}
SEQ_TOTAL_BYTES=${SEQ_TOTAL_BYTES:-$((8 * 1024 * 1024 * 1024))}
MIN_SEQ_WRITE_RATIO=${MIN_SEQ_WRITE_RATIO:-0.75}
MIN_SEQ_READ_RATIO=${MIN_SEQ_READ_RATIO:-0.75}
MIN_METADATA_RATIO=${MIN_METADATA_RATIO:-0.5}
MIN_CONCURRENT_RATIO=${MIN_CONCURRENT_RATIO:-0.5}
: "${FUSE_BUILD_PROFILE:=release}"
# Perf-oriented FUSE tuning defaults (overridable by env)
: "${AGENTFS_FUSE_MAX_WRITE:=16777216}"
: "${AGENTFS_FUSE_MAX_BACKGROUND:=256}"
: "${AGENTFS_FUSE_WRITE_THREADS:=8}"
: "${AGENTFS_FUSE_PASSTHROUGH:=1}"
: "${AGENTFS_FUSE_INLINE_WRITES:=1}"
: "${AGENTFS_FUSE_HOSTFS_DIRECT:=1}"
: "${AGENTFS_FUSE_SUDO:=1}"
: "${AGENTFS_FUSE_HOST_BIN:=$REPO_ROOT/target/${FUSE_BUILD_PROFILE}/agentfs-fuse-host}"
: "${RUST_LOG:=agentfs::fuse=info}"
export AGENTFS_FUSE_MAX_WRITE AGENTFS_FUSE_MAX_BACKGROUND AGENTFS_FUSE_WRITE_THREADS
export AGENTFS_FUSE_PASSTHROUGH AGENTFS_FUSE_INLINE_WRITES AGENTFS_FUSE_HOSTFS_DIRECT AGENTFS_FUSE_SUDO
export FUSE_BUILD_PROFILE AGENTFS_FUSE_HOST_BIN RUST_LOG
mkdir -p "$RUN_DIR" "$BASELINE_DIR" "$BACKSTORE_DIR"

log() {
  echo "[$(date +%H:%M:%S)] $*" | tee -a "$RUN_DIR/performance.log"
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

if ! ulimit -n "$NOFILE_LIMIT" 2>/dev/null; then
  log "Warning: unable to raise RLIMIT_NOFILE to $NOFILE_LIMIT; continuing with $(ulimit -n)"
fi

drop_caches() {
  local reason="$1"
  if [[ "${SKIP_DROP_CACHES:-}" == "1" ]]; then
    log "Skipping cache drop (${reason})"
    return
  fi
  if ! command -v sudo >/dev/null 2>&1; then
    log "sudo unavailable; cannot drop caches (${reason})"
    return
  fi
  if sudo sh -c 'sync' >>"$RUN_DIR/performance.log" 2>&1 && echo 3 | sudo tee /proc/sys/vm/drop_caches >/dev/null; then
    log "Dropped host page cache (${reason})"
  else
    log "Failed to drop host page cache (${reason}); continuing"
  fi
}

if [[ -z "$TIME_BIN" ]]; then
  log "Required 'time' binary not found in PATH"
  exit 1
fi

force_remove_dir() {
  local target="$1"
  if [[ -z "$target" ]]; then
    return
  fi
  if ! rm -rf "$target" 2>/dev/null; then
    sudo rm -rf "$target" 2>/dev/null || true
  fi
}

cleanup() {
  if mountpoint -q "$MOUNTPOINT" 2>/dev/null; then
    log "Unmounting $MOUNTPOINT"
    (cd "$REPO_ROOT" && just umount-fuse "$MOUNTPOINT") >>"$RUN_DIR/performance.log" 2>&1 || true
  fi
  force_remove_dir "$MOUNTPOINT"
  force_remove_dir "$BASELINE_DIR"
  force_remove_dir "$BACKSTORE_DIR"
}
trap cleanup EXIT

build_fuse_host() {
  if [[ -z "$SKIP_FUSE_BUILD" ]]; then
    log "Building agentfs-fuse-host (performance suite)"
    (cd "$REPO_ROOT" && just build-fuse-host) >>"$RUN_DIR/performance.log" 2>&1
  else
    log "SKIP_FUSE_BUILD set; skipping build"
  fi
}

mount_agentfs() {
  force_remove_dir "$MOUNTPOINT"
  mkdir -p "$MOUNTPOINT"
  cat >"$FUSE_CONFIG" <<JSON
{
  "case_sensitivity": "Sensitive",
  "memory": {
    "max_bytes_in_memory": 268435456,
    "spill_directory": null
  },
  "limits": {
    "max_open_handles": 4096,
    "max_branches": 64,
    "max_snapshots": 128
  },
  "cache": {
    "attr_ttl_ms": 250,
    "entry_ttl_ms": 250,
    "negative_ttl_ms": 250,
    "enable_readdir_plus": true,
    "auto_cache": true,
    "writeback_cache": true
  },
  "enable_xattrs": true,
  "enable_ads": false,
  "track_events": false,
  "security": {
    "enforce_posix_permissions": true,
    "default_uid": $USER_ID,
    "default_gid": $GROUP_ID,
    "enable_windows_acl_compat": false,
    "root_bypass_permissions": true
  },
  "backstore": {
    "HostFs": {
      "root": "$BACKSTORE_DIR",
      "prefer_native_snapshots": false
    }
  },
  "overlay": {
    "enabled": false,
    "lower_root": null,
    "copyup_mode": "Lazy"
  },
  "interpose": {
    "enabled": false,
    "max_copy_bytes": 1048576,
    "require_reflink": false,
    "allow_windows_reparse": false
  }
}
JSON
  log "Mounting AgentFS at $MOUNTPOINT"
  (cd "$REPO_ROOT" && AGENTFS_FUSE_CONFIG="$FUSE_CONFIG" AGENTFS_FUSE_ALLOW_OTHER=1 just mount-fuse "$MOUNTPOINT") >>"$RUN_DIR/performance.log" 2>&1
  if ! wait_for_mount_state "$MOUNTPOINT" "mounted"; then
    log "Failed to verify mount at $MOUNTPOINT; aborting performance run."
    exit 1
  fi
}

prepare_agentfs_workspace() {
  log "Preparing AgentFS workspace $AGENTFS_WORKDIR"
  sleep 1
  if ! mkdir -p "$AGENTFS_WORKDIR" 2>/dev/null; then
    sudo mkdir -p "$AGENTFS_WORKDIR"
  fi
  if ! chown "$(id -u)":"$(id -g)" "$AGENTFS_WORKDIR" 2>/dev/null; then
    sudo chown "$(id -u)":"$(id -g)" "$AGENTFS_WORKDIR"
  fi
}

run_time_dd() {
  local label="$1"
  local test="$2"
  local cmd="$3"
  local bytes="$4"
  local output="$RUN_DIR/${label}_${test}.time"
  "$TIME_BIN" --verbose bash -c "$cmd" >>"$RUN_DIR/${label}_${test}.log" 2>"$output"
  python3 - "$output" "$bytes" "$label" "$test" "$RESULTS_FILE" <<'PY'
import json
import sys
from pathlib import Path

log_path = Path(sys.argv[1])
bytes_total = int(sys.argv[2])
label = sys.argv[3]
test = sys.argv[4]
results_path = Path(sys.argv[5])
fields = {}
for line in log_path.read_text().splitlines():
    line = line.strip()
    if line.startswith("User time (seconds):"):
        fields["user_sec"] = float(line.split(":", 1)[1])
    elif line.startswith("System time (seconds):"):
        fields["sys_sec"] = float(line.split(":", 1)[1])
    elif line.startswith("Percent of CPU this job got:"):
        fields["cpu_percent"] = float(line.split(":", 1)[1].strip().rstrip("%"))
    elif line.startswith("Elapsed (wall clock) time"):
        if ": " in line:
            value = line.split(": ", 1)[1].strip()
        else:
            value = line.rsplit(":", 1)[1].strip()
        parts = value.split(":")
        if len(parts) == 3:
            h, m, s = parts
        elif len(parts) == 2:
            h = 0
            m, s = parts
        else:
            h = 0
            m = 0
            s = parts[0]
        elapsed = float(h) * 3600 + float(m) * 60 + float(s)
        fields["elapsed_sec"] = elapsed
    elif line.startswith("Maximum resident set size (kbytes):"):
        fields["max_rss_kb"] = int(line.split(":", 1)[1])
if "elapsed_sec" not in fields or fields["elapsed_sec"] == 0:
    fields["elapsed_sec"] = None
throughput_mb_s = None
if fields.get("elapsed_sec"):
    throughput_mb_s = bytes_total / fields["elapsed_sec"] / (1024 * 1024)
result = {
    "label": label,
    "test": test,
    "bytes": bytes_total,
    "elapsed_sec": fields.get("elapsed_sec"),
    "user_sec": fields.get("user_sec"),
    "sys_sec": fields.get("sys_sec"),
    "cpu_percent": fields.get("cpu_percent"),
    "max_rss_kb": fields.get("max_rss_kb"),
    "throughput_mb_s": throughput_mb_s,
}
with results_path.open("a") as fh:
    fh.write(json.dumps(result) + "\n")
PY
}

run_metadata_test() {
  local label="$1"
  local target_dir="$2"
  local entries="$3"
  python3 - "$label" "$target_dir" "$entries" "$RESULTS_FILE" <<'PY'
import json
import os
import sys
import time

label = sys.argv[1]
target_dir = sys.argv[2]
entries = int(sys.argv[3])
results_path = sys.argv[4]
os.makedirs(target_dir, exist_ok=True)
start = time.perf_counter()
for i in range(entries):
    sub = os.path.join(target_dir, f"dir_{i}")
    os.makedirs(sub, exist_ok=True)
    with open(os.path.join(sub, "file.txt"), "w") as handle:
        handle.write("metadata-test\n")
elapsed = time.perf_counter() - start
ops = entries * 2  # mkdir + file write
ops_per_sec = ops / elapsed if elapsed else None
result = {
    "label": label,
    "test": "metadata",
    "entries": entries,
    "elapsed_sec": elapsed,
    "ops_per_sec": ops_per_sec,
}
with open(results_path, "a") as fh:
    fh.write(json.dumps(result) + "\n")
PY
  rm -rf "$target_dir"
}

run_concurrent_writes() {
  local label="$1"
  local target_path="$2"
  local jobs="$3"
  local file_mb="$4"
  local bytes=$((jobs * file_mb * 1024 * 1024))
  local logfile="$RUN_DIR/${label}_concurrent.time"
  "$TIME_BIN" --verbose bash -c '
set -euo pipefail
TARGET="$1"
JOBS=$2
SIZE_MB=$3
for i in $(seq 1 "$JOBS"); do
  dd if=/dev/zero "of=$TARGET/concurrent_$i.bin" bs=1M "count=$SIZE_MB" status=none &
  PIDS[$i]=$!
done
exit_code=0
for pid in "${PIDS[@]}"; do
  if ! wait "$pid"; then
    exit_code=1
  fi
done
exit "$exit_code"
' bash "$target_path" "$jobs" "$file_mb" >>"$RUN_DIR/${label}_concurrent.log" 2>"$logfile"
  python3 - "$logfile" "$bytes" "$label" "$RESULTS_FILE" <<'PY'
import json
import sys
from pathlib import Path

log_path = Path(sys.argv[1])
bytes_total = int(sys.argv[2])
label = sys.argv[3]
results_path = Path(sys.argv[4])
fields = {}
for line in log_path.read_text().splitlines():
    line = line.strip()
    if line.startswith("Percent of CPU this job got:"):
        fields["cpu_percent"] = float(line.split(":", 1)[1].strip().rstrip("%"))
    elif line.startswith("Elapsed (wall clock) time"):
        if ": " in line:
            value = line.split(": ", 1)[1].strip()
        else:
            value = line.rsplit(":", 1)[1].strip()
        parts = value.split(":")
        if len(parts) == 3:
            h, m, s = parts
        elif len(parts) == 2:
            h = 0
            m, s = parts
        else:
            h = 0
            m = 0
            s = parts[0]
        fields["elapsed_sec"] = float(h) * 3600 + float(m) * 60 + float(s)
result = {
    "label": label,
    "test": "concurrent_write",
    "bytes": bytes_total,
    "elapsed_sec": fields.get("elapsed_sec"),
    "throughput_mb_s": bytes_total / fields["elapsed_sec"] / (1024 * 1024) if fields.get("elapsed_sec") else None,
    "cpu_percent": fields.get("cpu_percent"),
}
with results_path.open("a") as fh:
    fh.write(json.dumps(result) + "\n")
PY
  rm -f "$target_path"/concurrent_*.bin
}

run_throughput_tests() {
  local label="$1"
  local path="$2"
  local test_file="$path/throughput.bin"
  local write_bytes=$SEQ_TOTAL_BYTES
  local block_bytes=$SEQ_BLOCK_SIZE_BYTES
  local count=$((write_bytes / block_bytes))
  run_time_dd "$label" "seq_write" "dd if=/dev/zero of=$test_file bs=${block_bytes} count=$count status=none conv=fdatasync" "$write_bytes"
  drop_caches "$label seq_read"
  run_time_dd "$label" "seq_read" "dd if=$test_file of=/dev/null bs=${block_bytes} count=$count status=none" "$write_bytes"
  rm -f "$test_file"
}

run_suite() {
  build_fuse_host
  mount_agentfs
  prepare_agentfs_workspace
  run_throughput_tests "agentfs" "$AGENTFS_WORKDIR"
  run_metadata_test "agentfs" "$AGENTFS_WORKDIR/metadata" "${METADATA_ENTRIES:-2000}"
  run_concurrent_writes "agentfs" "$AGENTFS_WORKDIR" 4 64
  log "Preparing baseline directory $BASELINE_DIR"
  mkdir -p "$BASELINE_DIR"
  run_throughput_tests "baseline" "$BASELINE_DIR"
  run_metadata_test "baseline" "$BASELINE_DIR/metadata" "${METADATA_ENTRIES:-2000}"
  run_concurrent_writes "baseline" "$BASELINE_DIR" 4 64
}

summarize_results() {
  python3 - "$RESULTS_FILE" "$SUMMARY_FILE" <<'PY'
import json
import sys
from collections import defaultdict
from pathlib import Path

results_path = Path(sys.argv[1])
out_path = Path(sys.argv[2])
entries = [json.loads(line) for line in results_path.read_text().splitlines() if line.strip()]
grouped = defaultdict(dict)
for entry in entries:
    grouped[entry["test"]][entry["label"]] = entry
summary = []
for test, data in grouped.items():
    agent = data.get("agentfs")
    base = data.get("baseline")
    record = {
        "test": test,
        "agentfs": agent,
        "baseline": base,
    }
    if agent and base:
        key = "ops_per_sec" if test == "metadata" else "throughput_mb_s"
        if agent.get(key) and base.get(key) and base.get(key) != 0:
            record["ratio"] = agent[key] / base[key]
    summary.append(record)
out_path.write_text(json.dumps(summary, indent=2))
PY
  log "Performance summary written to $SUMMARY_FILE"
}

check_thresholds() {
  python3 - "$SUMMARY_FILE" "$MIN_SEQ_WRITE_RATIO" "$MIN_SEQ_READ_RATIO" "$MIN_METADATA_RATIO" "$MIN_CONCURRENT_RATIO" <<'PY'
import json
import sys

summary = json.loads(open(sys.argv[1]).read())
thresholds = {
    "seq_write": float(sys.argv[2]),
    "seq_read": float(sys.argv[3]),
    "metadata": float(sys.argv[4]),
    "concurrent_write": float(sys.argv[5]),
}
failures = []
for entry in summary:
    test = entry["test"]
    required = thresholds.get(test)
    if required is None:
        continue
    ratio = entry.get("ratio")
    if ratio is None or ratio < required:
        failures.append(f"{test}: ratio={ratio} < required {required}")

if failures:
    print("Performance regression detected:", file=sys.stderr)
    for item in failures:
        print(f"  - {item}", file=sys.stderr)
    sys.exit(1)
PY
  log "Performance ratios cleared minimum thresholds"
}

run_suite
summarize_results
if [[ -z "$SKIP_THRESHOLD_CHECK" ]]; then
  check_thresholds
else
  log "SKIP_THRESHOLD_CHECK set; skipping ratio enforcement"
fi
log "Performance benchmarks complete. Results stored in $RUN_DIR"
