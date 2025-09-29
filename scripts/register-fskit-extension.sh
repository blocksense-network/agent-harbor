#!/usr/bin/env bash
set -euo pipefail

# Attempt to trigger FSKit extension loading by mounting a minimal dummy device
# Assumes AgentsWorkflow.app with AgentFSKitExtension.appex is installed/enabled

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
HELPERS="${REPO_ROOT}/adapters/macos/xcode/test-device-setup.sh"

if [[ ! -f "$HELPERS" ]]; then
  echo "Missing helper: $HELPERS" >&2
  exit 1
fi

echo "Creating dummy device and mountpoint to trigger FSKit load..."
device=$(bash -lc "source '$HELPERS'; create_device 10 dev; echo \$dev")
mp=$(bash -lc "source '$HELPERS'; create_mount_point mp; echo \$mp")

echo "Device: $device"
echo "Mount point: $mp"

if bash -lc "source '$HELPERS'; mount_agentfs '$device' '$mp'"; then
  echo "Mounted successfully; unmounting..."
  bash -lc "source '$HELPERS'; unmount_device '$mp'" || true
else
  echo "Mount failed; extension may still require approval. See systemextensionsctl list."
fi

echo "Done."
