#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

LOG_DIR="${REPO_ROOT}/target/tmp/e2e-fskit-logs"
mkdir -p "$LOG_DIR"
RUN_ID="$(date +%Y%m%d-%H%M%S)-$$"
RUN_LOG="$LOG_DIR/run-${RUN_ID}.log"

echo "Logs: $RUN_LOG"

cleanup() {
  echo "Cleaning up background processes..." >>"$RUN_LOG" 2>&1 || true
}
trap cleanup EXIT

echo "Verifying macOS FSKit prerequisites..." | tee -a "$RUN_LOG"
"$REPO_ROOT/scripts/verify-macos-fskit-prereqs.sh" | tee -a "$RUN_LOG"

echo "Building FSKit extension (Rust libs + Swift appex)..." | tee -a "$RUN_LOG"
just build-agentfs-extension | tee -a "$RUN_LOG"

EXT_BUNDLE="${REPO_ROOT}/adapters/macos/xcode/AgentFSKitExtension/AgentFSKitExtension.appex"
if [[ ! -d "$EXT_BUNDLE" ]]; then
  echo "Extension bundle not found at $EXT_BUNDLE" | tee -a "$RUN_LOG"
  exit 1
fi
echo "Extension bundle present: $EXT_BUNDLE" | tee -a "$RUN_LOG"

echo "Starting E2E mount/I-O/unmount test via Python..." | tee -a "$RUN_LOG"
PY_SCRIPT="${REPO_ROOT}/tests/tools/e2e_macos_fskit/e2e_io_test.py"
python3 "$PY_SCRIPT" 2>&1 | tee -a "$RUN_LOG"

echo "E2E FSKit test completed successfully." | tee -a "$RUN_LOG"
