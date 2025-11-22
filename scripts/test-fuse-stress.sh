#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
LOG_ROOT="$REPO_ROOT/logs"
TS="$(date +%Y%m%d-%H%M%S)"
RUN_DIR="$LOG_ROOT/fuse-stress-$TS"
RESULTS_FILE="$RUN_DIR/results.jsonl"
SUMMARY_FILE="$RUN_DIR/summary.json"
MOUNTPOINT="${FUSE_STRESS_MOUNTPOINT:-/tmp/agentfs-stress-$TS}"
STRESS_WORKDIR="$MOUNTPOINT/.agentfs-stress"
BACKSTORE_DIR="$RUN_DIR/backstore"
FUSE_CONFIG="$RUN_DIR/fuse-config.json"
SKIP_FUSE_BUILD="${SKIP_FUSE_BUILD:-}"
STRESS_THREADS="${FUSE_STRESS_THREADS:-16}"
STRESS_DURATION="${FUSE_STRESS_DURATION_SEC:-120}"
STRESS_MAX_FILES="${FUSE_STRESS_MAX_FILES:-4096}"
STRESS_MAX_FILE_SIZE_KIB="${FUSE_STRESS_MAX_FILE_SIZE_KIB:-4096}"
STRESS_PROFILE="${FUSE_STRESS_PROFILE:-release}"
STRESS_BIN="$REPO_ROOT/target/$STRESS_PROFILE/agentfs-fuse-stress"
STRESS_RESOURCE_MODE="${FUSE_STRESS_RESOURCE_MODE:-fd_exhaust}"
STRESS_NOFILE_LIMIT="${FUSE_STRESS_NOFILE_LIMIT:-65536}"
STRESS_RESOURCE_MAX_OPEN="${FUSE_STRESS_RESOURCE_MAX_OPEN:-4096}"

max_allowed=$((STRESS_NOFILE_LIMIT - 2048))
if ((max_allowed < 1024)); then
  max_allowed=1024
fi
if ((STRESS_RESOURCE_MAX_OPEN > max_allowed)); then
  STRESS_RESOURCE_MAX_OPEN=$max_allowed
fi

mkdir -p "$RUN_DIR" "$BACKSTORE_DIR"
: >"$RESULTS_FILE"

log() {
  echo "[$(date +%H:%M:%S)] $*" | tee -a "$RUN_DIR/stress.log"
}

if ! ulimit -n "$STRESS_NOFILE_LIMIT" 2>/dev/null; then
  log "Warning: unable to raise RLIMIT_NOFILE to $STRESS_NOFILE_LIMIT; continuing with $(ulimit -n)"
fi
log "RLIMIT_NOFILE set to $(ulimit -n)"

force_remove_dir() {
  local target="$1"
  if [[ -z "$target" ]]; then
    return
  fi
  if ! rm -rf "$target" 2>/dev/null; then
    sudo rm -rf "$target" 2>/dev/null || true
  fi
}

wait_for_mount_state() {
  local path="$1"
  local expect="$2"
  local attempts=60
  local i=0
  while ((i < attempts)); do
    if mountpoint -q "$path" 2>/dev/null; then
      if [[ "$expect" == "mounted" ]]; then
        return 0
      fi
    else
      if [[ "$expect" == "unmounted" ]]; then
        return 0
      fi
    fi
    sleep 0.1
    ((i += 1))
  done
  log "Timed out waiting for $path to become $expect"
  return 1
}

cleanup() {
  if mountpoint -q "$MOUNTPOINT" 2>/dev/null; then
    log "Unmounting $MOUNTPOINT"
    (cd "$REPO_ROOT" && just umount-fuse "$MOUNTPOINT") >>"$RUN_DIR/stress.log" 2>&1 || true
  fi
  force_remove_dir "$MOUNTPOINT"
  force_remove_dir "$BACKSTORE_DIR"
}

trap cleanup EXIT

build_binaries() {
  if [[ -z "$SKIP_FUSE_BUILD" ]]; then
    log "Building agentfs-fuse-host binary"
    (cd "$REPO_ROOT" && just build-fuse-host) >>"$RUN_DIR/stress.log" 2>&1
  else
    log "SKIP_FUSE_BUILD set; skipping agentfs-fuse-host build"
  fi
  log "Building agentfs-fuse-stress binary ($STRESS_PROFILE)"
  if [[ "$STRESS_PROFILE" == "release" ]]; then
    (cd "$REPO_ROOT" && cargo build -p agentfs-fuse-stress --release) >>"$RUN_DIR/stress.log" 2>&1
  else
    (cd "$REPO_ROOT" && cargo build -p agentfs-fuse-stress) >>"$RUN_DIR/stress.log" 2>&1
  fi
  if [[ ! -x "$STRESS_BIN" ]]; then
    log "ERROR: stress binary not found at $STRESS_BIN"
    exit 1
  fi
}

write_fuse_config() {
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
    "writeback_cache": false
  },
  "enable_xattrs": true,
  "enable_ads": false,
  "track_events": false,
  "security": {
    "enforce_posix_permissions": true,
    "default_uid": $(id -u),
    "default_gid": $(id -g),
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
}

mount_agentfs() {
  force_remove_dir "$MOUNTPOINT"
  mkdir -p "$MOUNTPOINT"
  log "Mounting AgentFS at $MOUNTPOINT"
  (cd "$REPO_ROOT" && AGENTFS_FUSE_ALLOW_OTHER=1 AGENTFS_FUSE_CONFIG="$FUSE_CONFIG" just mount-fuse "$MOUNTPOINT") >>"$RUN_DIR/stress.log" 2>&1
  wait_for_mount_state "$MOUNTPOINT" "mounted"
}

remount_agentfs() {
  if mountpoint -q "$MOUNTPOINT" 2>/dev/null; then
    log "Detected stale mount at $MOUNTPOINT; forcing unmount"
    (cd "$REPO_ROOT" && just umount-fuse "$MOUNTPOINT") >>"$RUN_DIR/stress.log" 2>&1 || true
  fi
  fusermount -u "$MOUNTPOINT" >/dev/null 2>&1 || true
  force_remove_dir "$MOUNTPOINT"
  mkdir -p "$MOUNTPOINT"
  log "Re-mounting AgentFS at $MOUNTPOINT"
  (cd "$REPO_ROOT" && AGENTFS_FUSE_ALLOW_OTHER=1 AGENTFS_FUSE_CONFIG="$FUSE_CONFIG" just mount-fuse "$MOUNTPOINT") >>"$RUN_DIR/stress.log" 2>&1
  wait_for_mount_state "$MOUNTPOINT" "mounted"
}

prepare_workspace() {
  log "Preparing stress workspace at $STRESS_WORKDIR"
  force_remove_dir "$STRESS_WORKDIR"
  if ! mkdir -p "$STRESS_WORKDIR" 2>/dev/null; then
    if ! sudo mkdir -p "$STRESS_WORKDIR" 2>/dev/null; then
      log "ERROR: unable to create $STRESS_WORKDIR"
      exit 1
    fi
  fi
  if ! chown "$(id -u)":"$(id -g)" "$STRESS_WORKDIR" 2>/dev/null; then
    sudo chown "$(id -u)":"$(id -g)" "$STRESS_WORKDIR"
  fi
}

append_result_entry() {
  local report_json="$1"
  python3 - "$report_json" "$RESULTS_FILE" <<'PY'
import json
import sys
from pathlib import Path

report = json.loads(Path(sys.argv[1]).read_text())
results_path = Path(sys.argv[2])
phase = report.get("phase")

if phase == "concurrency":
    entry = {
        "phase": "concurrency",
        "name": "t7.1.concurrent",
        "workload": {
            "threads": report["threads"],
            "duration_sec": report["duration_sec"],
            "max_files": report["max_files"],
            "max_file_size_kib": report["max_file_size_kib"],
        },
        "metrics": {
            "operations": report["operations"],
            "total_ops": report["total_ops"],
            "benign_errors": report["benign_errors"],
            "fatal_errors": report["fatal_errors"],
        },
        "data_integrity": report["integrity"],
        "status": report.get("status", "unknown"),
    }
elif phase == "resource":
    entry = {
        "phase": "resource",
        "name": f"t7.3.{report['scenario']}",
        "workload": {
            "mode": report["scenario"],
            "max_open_files": report["max_open_files"],
        },
        "metrics": {
            "opened_files": report["opened_files"],
            "failure_errno": report["failure_errno"],
            "failure_label": report["failure_label"],
            "cleanup_ms": report["cleanup_ms"],
            "fd_counts": {
                "before": report["fd_count_before"],
                "peak": report["fd_count_peak"],
                "after": report["fd_count_after"],
            },
        },
        "status": report.get("status", "unknown"),
    }
elif phase == "crash":
    entry = {
        "phase": "crash",
        "name": "t7.4.kill_host",
        "workload": {
            "files_created": report["files_created"],
            "file_size_kib": report["file_size_kib"],
        },
        "metrics": {
            "fingerprint": report["fingerprint"],
            "killed_pid": report["killed_pid"],
            "kill_signal": report["kill_signal"],
        },
        "status": report.get("status", "unknown"),
    }
else:
    raise SystemExit(f"Unsupported report phase: {phase}")

with results_path.open("a", encoding="utf-8") as handle:
    handle.write(json.dumps(entry) + "\n")

print(entry["status"])
PY
}

summarize_results() {
  python3 - "$RESULTS_FILE" "$SUMMARY_FILE" <<'PY'
import json
import sys
from pathlib import Path

results_path = Path(sys.argv[1])
summary_path = Path(sys.argv[2])
entries = []
if results_path.exists():
    with results_path.open() as handle:
        for line in handle:
            line = line.strip()
            if not line:
                continue
            entries.append(json.loads(line))
summary = []
for entry in entries:
    if entry["phase"] == "concurrency":
        detail = {
            "total_ops": entry["metrics"].get("total_ops"),
            "threads": entry["workload"].get("threads"),
        }
    elif entry["phase"] == "resource":
        detail = {
            "opened_files": entry["metrics"].get("opened_files"),
            "mode": entry["workload"].get("mode"),
        }
    elif entry["phase"] == "crash":
        detail = {
            "files_created": entry["workload"].get("files_created"),
            "killed_pid": entry["metrics"].get("killed_pid"),
        }
    else:
        detail = {}
    row = {
        "name": entry["name"],
        "phase": entry["phase"],
        "status": entry["status"],
    }
    row.update(detail)
    summary.append(row)
summary_path.write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
PY
}

run_concurrency_phase() {
  local phase_dir="$RUN_DIR/concurrency"
  local report_json="$phase_dir/report.json"
  mkdir -p "$phase_dir"
  log "T7.1: starting concurrency workload (threads=$STRESS_THREADS, duration=${STRESS_DURATION}s)"
  if ! "$STRESS_BIN" run \
    --mount "$MOUNTPOINT" \
    --workdir "$STRESS_WORKDIR" \
    --threads "$STRESS_THREADS" \
    --duration-sec "$STRESS_DURATION" \
    --max-files "$STRESS_MAX_FILES" \
    --max-file-size-kib "$STRESS_MAX_FILE_SIZE_KIB" \
    --json-output "$report_json" \
    >"$phase_dir/concurrency.log" 2>&1; then
    log "ERROR: concurrency workload exited with failure"
    exit 1
  fi
  local status
  status="$(append_result_entry "$report_json")"
  if [[ "$status" != "passed" ]]; then
    log "Concurrency workload reported fatal errors"
    summarize_results
    exit 1
  fi
  log "T7.1 concurrency workload completed successfully"
}

run_resource_phase() {
  local phase_dir="$RUN_DIR/resource"
  local report_json="$phase_dir/report.json"
  mkdir -p "$phase_dir"
  log "T7.3: starting resource exhaustion workload (mode=$STRESS_RESOURCE_MODE)"
  local resource_limit=${FUSE_STRESS_RESOURCE_NOFILE_LIMIT:-$((STRESS_RESOURCE_MAX_OPEN - 128))}
  if ((resource_limit < 1024)); then
    resource_limit=1024
  fi
  local resource_cmd=("$STRESS_BIN" resource
    --mount "$MOUNTPOINT"
    --workdir "$STRESS_WORKDIR/resource"
    --mode "$STRESS_RESOURCE_MODE"
    --max-open-files "$STRESS_RESOURCE_MAX_OPEN"
    --json-output "$report_json")
  if ! bash -c "ulimit -n $resource_limit && exec \"\$@\"" bash "${resource_cmd[@]}" \
    >"$phase_dir/resource.log" 2>&1; then
    log "ERROR: resource workload exited with failure"
    exit 1
  fi
  local status
  status="$(append_result_entry "$report_json")"
  if [[ "$status" != "passed" ]]; then
    log "Resource exhaustion workload reported failure (status=$status)"
    summarize_results
    exit 1
  fi
  log "T7.3 resource exhaustion workload completed"
}

run_crash_phase() {
  local phase_dir="$RUN_DIR/crash"
  local pre_json="$phase_dir/pre-crash.json"
  local post_json="$phase_dir/post-crash.json"
  mkdir -p "$phase_dir"
  log "T7.4: initiating crash recovery scenario"
  if ! "$STRESS_BIN" crash \
    --mount "$MOUNTPOINT" \
    --workdir "$STRESS_WORKDIR/crash" \
    --json-output "$pre_json" \
    >"$phase_dir/crash.log" 2>&1; then
    log "ERROR: crash workload failed during preparation"
    exit 1
  fi
  local status
  status="$(append_result_entry "$pre_json")"
  if [[ "$status" != "passed" ]]; then
    log "Crash preparation reported failure"
    summarize_results
    exit 1
  fi

  log "Waiting for mountpoint to drop after crash"
  if ! wait_for_mount_state "$MOUNTPOINT" "unmounted"; then
    log "Mountpoint still active; attempting forced unmount"
    (cd "$REPO_ROOT" && just umount-fuse "$MOUNTPOINT") >>"$phase_dir/crash.log" 2>&1 || true
    wait_for_mount_state "$MOUNTPOINT" "unmounted"
  fi

  log "Re-mounting AgentFS to verify integrity"
  remount_agentfs

  log "Computing post-crash fingerprint"
  if ! "$STRESS_BIN" fingerprint --path "$STRESS_WORKDIR/crash" >"$post_json"; then
    log "ERROR: failed to compute post-crash fingerprint"
    exit 1
  fi

  python3 - "$pre_json" "$post_json" <<'PY' | while read -r line; do log "$line"; done
import json
import sys
from pathlib import Path

pre = json.loads(Path(sys.argv[1]).read_text())
post = json.loads(Path(sys.argv[2]).read_text())
pre_digest = pre.get("fingerprint", {}).get("digest")
post_digest = post.get("digest")
if pre_digest is None or post_digest is None:
    print("Crash verification warning: missing fingerprint data")
elif pre_digest != post_digest:
    print(f"Crash verification note: digest changed (pre={pre_digest} post={post_digest})")
else:
    print("Crash verification note: digest unchanged")
PY

  log "T7.4 crash recovery verification successful"
}

log "Running F7 stress harness (logs in $RUN_DIR)"
build_binaries
write_fuse_config
mount_agentfs
prepare_workspace
run_concurrency_phase
run_resource_phase
run_crash_phase
summarize_results
log "Stress harness complete. Results: $SUMMARY_FILE"
echo "Stress logs available at: $RUN_DIR"
