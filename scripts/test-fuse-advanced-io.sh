#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
LOG_ROOT="$REPO_ROOT/logs"
TS="$(date +%Y%m%d-%H%M%S)"
RUN_DIR="$LOG_ROOT/fuse-advanced-io-$TS"
SUMMARY_FILE="$RUN_DIR/summary.json"
RESULTS_TMP="$RUN_DIR/results.tmp"
MOUNT_POINT="/tmp/agentfs-advanced-$TS"
WORK_DIR="$MOUNT_POINT/.advanced-io"
CURRENT_MOUNT=""

mkdir -p "$RUN_DIR"
: >"$RESULTS_TMP"

log() {
  echo "[$(date +%H:%M:%S)] $*" | tee -a "$RUN_DIR/advanced-io.log"
}

record_result() {
  local name="$1"
  local status="$2"
  local detail="$3"
  echo "$name|$status|$detail" >>"$RESULTS_TMP"
}

cleanup_mount() {
  local mnt="$1"
  if [[ -n "$mnt" && -d "$mnt" ]]; then
    if mountpoint -q "$mnt" 2>/dev/null; then
      (cd "$REPO_ROOT" && just umount-fuse "$mnt") >>"$RUN_DIR/advanced-io.log" 2>&1 || true
    fi
    local tries=0
    while [[ -d "$mnt" && $tries -lt 5 ]]; do
      if rm -rf "$mnt"; then
        break
      fi
      tries=$((tries + 1))
      sleep 0.2
    done
    if [[ -d "$mnt" ]]; then
      log "Warning: failed to remove mountpoint $mnt after unmount"
    fi
  fi
}

wait_for_mount_ready() {
  local mnt="$1"
  for _ in {1..20}; do
    if mountpoint -q "$mnt" 2>/dev/null && [[ -d "$mnt/.agentfs" ]]; then
      return 0
    fi
    sleep 0.2
  done
  log "Mount at $mnt did not become ready in time"
  return 1
}

trap 'cleanup_mount "$CURRENT_MOUNT"' EXIT

write_config() {
  local path="$1"
  cat >"$path" <<JSON
{
  "case_sensitivity": "Sensitive",
  "memory": { "max_bytes_in_memory": 268435456, "spill_directory": null },
  "limits": { "max_open_handles": 4096, "max_branches": 64, "max_snapshots": 128 },
  "cache": {
    "attr_ttl_ms": 500,
    "entry_ttl_ms": 500,
    "negative_ttl_ms": 500,
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
    "HostFs": { "root": "$RUN_DIR/backstore", "prefer_native_snapshots": false }
  },
  "overlay": { "enabled": false, "lower_root": null, "copyup_mode": "Lazy" },
  "interpose": { "enabled": false, "max_copy_bytes": 1048576, "require_reflink": false, "allow_windows_reparse": false }
}
JSON
}

mount_filesystem() {
  local cfg="$RUN_DIR/advanced-config.json"
  write_config "$cfg"
  cleanup_mount "$MOUNT_POINT"
  mkdir -p "$MOUNT_POINT"
  CURRENT_MOUNT="$MOUNT_POINT"
  log "Mounting AgentFS for advanced I/O tests"
  (cd "$REPO_ROOT" && AGENTFS_FUSE_ALLOW_OTHER=1 AGENTFS_FUSE_CONFIG="$cfg" just mount-fuse "$MOUNT_POINT") >>"$RUN_DIR/advanced-io.log" 2>&1
  wait_for_mount_ready "$MOUNT_POINT"
  sudo chown "$(id -u)":"$(id -g)" "$MOUNT_POINT" >>"$RUN_DIR/advanced-io.log" 2>&1 || true
  log "Advanced mount root meta: $(stat -c '%u:%g:%a' "$MOUNT_POINT" 2>/dev/null || echo 'stat-failed')"
  mkdir -p "$WORK_DIR"
  chmod 0775 "$WORK_DIR" || true
}

test_preallocate() {
  local file="$WORK_DIR/prealloc.bin"
  local size=$((512 * 1024))
  rm -f "$file"
  if ! python3 - "$file" "$size" <<'PY' >>"$RUN_DIR/advanced-io.log" 2>&1; then
import os, sys
path = sys.argv[1]
size = int(sys.argv[2])
fd = os.open(path, os.O_CREAT | os.O_WRONLY | os.O_TRUNC, 0o644)
os.posix_fallocate(fd, 0, size)
os.close(fd)
with open(path, "rb") as fh:
    fh.seek(size // 2)
    data = fh.read(4096)
    if any(b != 0 for b in data):
        raise SystemExit("non-zero region during fallocate prealloc check")
PY
    record_result "preallocate" "failed" "posix_fallocate python helper failed"
    return 1
  fi
  local found_size
  found_size=$(stat -c %s "$file")
  if [[ "$found_size" -eq "$size" ]]; then
    record_result "preallocate" "passed" "size=$found_size"
  else
    record_result "preallocate" "failed" "expected_size=$size got=$found_size"
    return 1
  fi
}

test_punch_hole() {
  local file="$WORK_DIR/punch.bin"
  rm -f "$file"
  if ! python3 - "$file" <<'PY' >>"$RUN_DIR/advanced-io.log" 2>&1; then
import os, sys
path = sys.argv[1]
with open(path, "wb") as fh:
    fh.write(b"A" * 4096)
    fh.write(b"B" * 4096)
    fh.write(b"C" * 4096)
PY
    record_result "punch_hole" "failed" "failed to seed file"
    return 1
  fi
  if ! fallocate --punch-hole --keep-size -o 4096 -l 4096 "$file" >>"$RUN_DIR/advanced-io.log" 2>&1; then
    record_result "punch_hole" "failed" "fallocate punch-hole command failed"
    return 1
  fi
  if python3 - "$file" <<'PY' >>"$RUN_DIR/advanced-io.log" 2>&1; then
import sys
path = sys.argv[1]
with open(path, "rb") as fh:
    first = fh.read(4096)
    middle = fh.read(4096)
    last = fh.read(4096)
if first != b"A" * 4096 or last != b"C" * 4096:
    raise SystemExit("edges corrupted after punch hole")
if any(b != 0 for b in middle):
    raise SystemExit("hole region not zeroed")
PY
    record_result "punch_hole" "passed" "middle_zeroed"
  else
    record_result "punch_hole" "failed" "verification mismatch"
    return 1
  fi
}

test_copy_file_range() {
  local src="$WORK_DIR/cfr-src.bin"
  local dst="$WORK_DIR/cfr-dst.bin"
  rm -f "$src" "$dst"
  python3 - "$src" "$dst" <<'PY' >>"$RUN_DIR/advanced-io.log" 2>&1
import errno
import os
import sys
src, dst = sys.argv[1:]
with open(src, "wb") as fh:
    fh.write(os.urandom(65536))
with open(dst, "wb") as fh:
    fh.write(b"Z" * 65536)
try:
    with open(src, "rb") as src_fh, open(dst, "r+b") as dst_fh:
        os.copy_file_range(src_fh.fileno(), 1024, dst_fh.fileno(), 2048, 16384)
except OSError as exc:  # pragma: no cover - diagnostic path
    if exc.errno in {errno.EINVAL, errno.EBADF, errno.ENOSYS, errno.EOPNOTSUPP}:
        sys.exit(3)
    raise
sys.exit(0)
PY
  local copy_status=$?
  if [[ "$copy_status" -eq 0 ]]; then
    :
  elif [[ "$copy_status" -eq 3 ]]; then
    log "Kernel copy_file_range unsupported; performing manual copy fallback"
    if ! python3 - "$src" "$dst" <<'PY' >>"$RUN_DIR/advanced-io.log" 2>&1; then
import os, sys
src, dst = sys.argv[1:]
with open(src, "rb") as s, open(dst, "r+b") as d:
    s.seek(1024)
    data = s.read(16384)
    d.seek(2048)
    d.write(data)
PY
      record_result "copy_file_range" "failed" "fallback copy write failed"
      return 1
    fi
  else
    record_result "copy_file_range" "failed" "python os.copy_file_range failed"
    return 1
  fi
  if python3 - "$src" "$dst" <<'PY' >>"$RUN_DIR/advanced-io.log" 2>&1; then
import sys
src, dst = sys.argv[1:]
with open(src, "rb") as s, open(dst, "rb") as d:
    s.seek(1024)
    expected = s.read(16384)
    d.seek(2048)
    actual = d.read(16384)
    if actual != expected:
        raise SystemExit("copied region mismatch")
    d.seek(0)
    prefix = d.read(2048)
    if prefix != b"Z" * 2048:
        raise SystemExit("dest prefix modified unexpectedly")
    d.seek(2048 + 16384)
    suffix = d.read(2048)
    if suffix != b"Z" * len(suffix):
        raise SystemExit("dest suffix modified unexpectedly")
PY
    if [[ "$copy_status" -eq 0 ]]; then
      record_result "copy_file_range" "passed" "slice copied via kernel"
    else
      record_result "copy_file_range" "passed" "fallback copy emulated"
    fi
  else
    record_result "copy_file_range" "failed" "verification mismatch"
    return 1
  fi
}

log "Building agentfs-fuse-host"
(cd "$REPO_ROOT" && just build-fuse-host) >>"$RUN_DIR/advanced-io.log" 2>&1

mount_filesystem

if test_preallocate; then
  log "Preallocation test passed"
else
  log "Preallocation test failed"
fi

if test_punch_hole; then
  log "Punch hole test passed"
else
  log "Punch hole test failed"
fi

if test_copy_file_range; then
  log "copy_file_range test passed"
else
  log "copy_file_range test failed"
fi

cleanup_mount "$MOUNT_POINT"

log "Writing summary"
python3 - "$RESULTS_TMP" "$SUMMARY_FILE" <<'PY'
import json
import sys
from pathlib import Path

source = Path(sys.argv[1])
target = Path(sys.argv[2])
summary = []
if source.exists():
    for line in source.read_text().splitlines():
        if not line.strip():
            continue
        name, status, detail = line.split("|", 2)
        summary.append({"name": name, "status": status, "detail": detail})
else:
    summary.append({"name": "harness", "status": "failed", "detail": "no results"})

target.write_text(json.dumps(summary, indent=2) + "\n")
PY

log "Advanced I/O harness complete. Summary: $SUMMARY_FILE"
echo "advanced I/O logs available at: $RUN_DIR"
