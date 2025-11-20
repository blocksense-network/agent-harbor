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

# Ensure the mount point is owned by the current user
if [ "$(stat -c %U "$mountpoint")" != "$(whoami)" ]; then
  if ! sudo chown "$(whoami)" "$mountpoint"; then
    echo "Error: Unable to chown $mountpoint; aborting mount"
    exit 1
  fi
fi

FUSE_FLAGS=()
ALLOW_OTHER="${AGENTFS_FUSE_ALLOW_OTHER:-1}"
if [ "$ALLOW_OTHER" = "1" ]; then
  FUSE_FLAGS+=("--allow-other")
fi

CONFIG_PATH_DEFAULT="$REPO_ROOT/fuse_config.json"
CONFIG_PATH="${AGENTFS_FUSE_CONFIG:-$CONFIG_PATH_DEFAULT}"
CONFIG_ARGS=()

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
if [ ${#FUSE_FLAGS[@]} -gt 0 ]; then
  echo "Additional FUSE flags: ${FUSE_FLAGS[*]}"
fi
if [ ${#CONFIG_ARGS[@]} -gt 0 ]; then
  echo "Config args: ${CONFIG_ARGS[*]}"
fi
echo "Note: This will run in the background. To unmount later: fusermount -u $mountpoint"
echo ""
HOST_BIN="${AGENTFS_FUSE_HOST_BIN:-$REPO_ROOT/target/debug/agentfs-fuse-host}"
echo "AgentFS host binary: $HOST_BIN"
if [ -n "${AGENTFS_FUSE_LOG_FILE:-}" ]; then
  mkdir -p "$(dirname "$AGENTFS_FUSE_LOG_FILE")"
  echo "Logging FUSE host output to $AGENTFS_FUSE_LOG_FILE"
  "$HOST_BIN" "${CONFIG_ARGS[@]}" "${FUSE_FLAGS[@]}" "$mountpoint" >>"$AGENTFS_FUSE_LOG_FILE" 2>&1 &
else
  "$HOST_BIN" "${CONFIG_ARGS[@]}" "${FUSE_FLAGS[@]}" "$mountpoint" &
fi
echo "AgentFS FUSE filesystem mounted. PID: $!"
for _ in {1..30}; do
  if mountpoint -q "$mountpoint" 2>/dev/null; then
    break
  fi
  sleep 0.1
done
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
