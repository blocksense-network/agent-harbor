#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

if [ $# -ne 1 ]; then
  echo "Usage: $0 /path/to/mountpoint"
  exit 1
fi

mountpoint="$1"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

is_mounted() {
  mountpoint -q "$mountpoint" 2>/dev/null
}

if is_mounted; then
  echo "Existing AgentFS mount detected at $mountpoint; attempting to unmount first..."
  if command -v fusermount >/dev/null 2>&1; then
    if ! fusermount -u "$mountpoint" 2>/dev/null; then
      echo "Error: Unable to auto-unmount $mountpoint. Run 'just umount-fuse $mountpoint' and retry."
      exit 1
    fi
  else
    echo "Error: $mountpoint is already mounted and fusermount is unavailable. Unmount manually and retry."
    exit 1
  fi
fi

if [ ! -d "$mountpoint" ]; then
  echo "Creating mount point: $mountpoint"
  if ! mkdir -p "$mountpoint"; then
    echo "Error: Failed to create $mountpoint (insufficient permissions?)"
    exit 1
  fi
fi

# Require sudo for device setup and mount prep
if ! command -v sudo >/dev/null 2>&1; then
  echo "Error: sudo is required to mount AgentFS FUSE (for /dev/fuse setup and mount prep)" >&2
  exit 1
fi

# Ensure the mount point is owned by the current user (best effort)
if [ "$(stat -c %U "$mountpoint")" != "$(whoami)" ]; then
  if ! sudo chown "$(whoami)" "$mountpoint" 2>/dev/null; then
    echo "Warning: Unable to chown $mountpoint to $(whoami); continuing" >&2
  fi
fi

ensure_dev_fuse() {
  if [ -e /dev/fuse ]; then
    return 0
  fi
  if command -v modprobe >/dev/null 2>&1; then
    sudo modprobe fuse 2>/dev/null || true
  fi
  if [ ! -e /dev/fuse ] && command -v mknod >/dev/null 2>&1; then
    # Try to create the device node manually (major 10, minor 229)
    sudo mknod /dev/fuse c 10 229 2>/dev/null || true
    sudo chmod 666 /dev/fuse 2>/dev/null || true
  fi
  if [ -e /dev/fuse ]; then
    return 0
  fi
  echo "Error: /dev/fuse not found (tried modprobe fuse); cannot mount" >&2
  return 1
}

ensure_dev_fuse || exit 1

CONFIG_PATH_DEFAULT="$REPO_ROOT/fuse_config.json"
CONFIG_PATH="${AGENTFS_FUSE_CONFIG:-$CONFIG_PATH_DEFAULT}"
CONFIG_ARGS=()
ALLOW_OTHER="${AGENTFS_FUSE_ALLOW_OTHER:-1}"
FALLBACK_ALLOW_OTHER="${AGENTFS_FUSE_ALLOW_OTHER_FALLBACK:-1}"

if [ -n "${AGENTFS_FUSE_CONFIG:-}" ]; then
  if [ ! -f "$CONFIG_PATH" ]; then
    echo "Error: Custom config '$CONFIG_PATH' (from AGENTFS_FUSE_CONFIG) not found."
    exit 1
  fi
  echo "Using custom FUSE config: $CONFIG_PATH"
  CONFIG_ARGS=(--config "$CONFIG_PATH")
else
  if [ -f "$CONFIG_PATH" ]; then
    echo "Using FUSE config: $CONFIG_PATH"
    CONFIG_ARGS=(--config "$CONFIG_PATH")
  else
    echo "No fuse_config.json supplied; generating default config for current user."
    CONFIG_PATH="$(mktemp /tmp/agentfs-default-config-XXXXXX.json)"
    BACKSTORE_ROOT="$(mktemp -d "/tmp/agentfs-backstore-XXXXXX")"
    cat >"$CONFIG_PATH" <<JSON
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
    "default_uid": 0,
    "default_gid": 0,
    "enable_windows_acl_compat": false,
    "root_bypass_permissions": true
  },
  "backstore": {
    "HostFs": { "root": "$BACKSTORE_ROOT", "prefer_native_snapshots": false }
  },
  "overlay": { "enabled": false, "lower_root": null, "copyup_mode": "Lazy" },
  "interpose": { "enabled": false, "max_copy_bytes": 1048576, "require_reflink": false, "allow_windows_reparse": false }
}
JSON
    CONFIG_ARGS=(--config "$CONFIG_PATH")
    echo "Generated default FUSE config: $CONFIG_PATH (backstore: $BACKSTORE_ROOT)"
  fi
fi

echo "Mounting AgentFS FUSE filesystem at $mountpoint..."
HOST_BIN="${AGENTFS_FUSE_HOST_BIN:-$REPO_ROOT/target/debug/agentfs-fuse-host}"
echo "AgentFS host binary: $HOST_BIN"

attempt_mount() {
  local allow_other_flag="$1"
  local fuse_flags=()
  if [ "$allow_other_flag" = "1" ]; then
    fuse_flags+=("--allow-other")
  fi

  if [ ${#fuse_flags[@]} -gt 0 ]; then
    echo "Additional FUSE flags: ${fuse_flags[*]}"
  fi
  if [ ${#CONFIG_ARGS[@]} -gt 0 ]; then
    echo "Config args: ${CONFIG_ARGS[*]}"
  fi
  echo "Note: This will run in the background. To unmount later: fusermount -u $mountpoint"
  echo ""

  if [ -n "${AGENTFS_FUSE_LOG_FILE:-}" ]; then
    mkdir -p "$(dirname "$AGENTFS_FUSE_LOG_FILE")"
    echo "Logging FUSE host output to $AGENTFS_FUSE_LOG_FILE"
    "$HOST_BIN" "${CONFIG_ARGS[@]}" "${fuse_flags[@]}" "$mountpoint" >>"$AGENTFS_FUSE_LOG_FILE" 2>&1 &
  else
    "$HOST_BIN" "${CONFIG_ARGS[@]}" "${fuse_flags[@]}" "$mountpoint" &
  fi
  local pid=$!
  for _ in {1..30}; do
    if mountpoint -q "$mountpoint" 2>/dev/null; then
      echo "AgentFS FUSE filesystem mounted. PID: $pid"
      return 0
    fi
    if ! ps -p "$pid" >/dev/null 2>&1; then
      break
    fi
    sleep 0.1
  done

  echo "Mount attempt failed (allow_other=$allow_other_flag); cleaning up..." >&2
  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
  return 1
}

if ! attempt_mount "$ALLOW_OTHER"; then
  if [ "$ALLOW_OTHER" = "1" ] && [ "$FALLBACK_ALLOW_OTHER" = "1" ]; then
    echo "Retrying mount without --allow-other (user_allow_other likely missing on this runner)..." >&2
    attempt_mount "0" || {
      echo "Mount failed even without --allow-other" >&2
      exit 1
    }
  else
    echo "Mount failed and fallback is disabled" >&2
    exit 1
  fi
fi
if [[ "${AGENTFS_FUSE_SKIP_AUTO_CHOWN:-0}" != "1" ]]; then
  chown_ok=false
  for _ in {1..30}; do
    if sudo chown "$(id -u)":"$(id -g)" "$mountpoint" 2>/dev/null; then
      chown_ok=true
      break
    fi
    sleep 0.2
  done
  if ! $chown_ok; then
    echo "Warning: failed to chown mountpoint to $(id -u):$(id -g); continuing" >&2
  else
    if command -v stat >/dev/null 2>&1; then
      echo "Mountpoint ownership: $(stat -c '%u:%g %a' "$mountpoint" 2>/dev/null || true)"
    fi
  fi

  for _ in {1..10}; do
    if chmod 0777 "$mountpoint" 2>/dev/null || sudo chmod 0777 "$mountpoint" 2>/dev/null; then
      break
    fi
    sleep 0.1
  done
else
  echo "Skipping auto-chown/chmod (AGENTFS_FUSE_SKIP_AUTO_CHOWN=1)"
fi
