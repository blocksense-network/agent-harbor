#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only
#
# Restart the AgentFS snapshots daemon (uses sudo).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

"$SCRIPT_DIR/stop-ah-fs-snapshots-daemon.sh" || true
"$SCRIPT_DIR/start-ah-fs-snapshots-daemon.sh"
