#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

LOG_DIR="logs"
mkdir -p "${LOG_DIR}"
LOG_FILE="${LOG_DIR}/manual-task-$(date -u +%Y%m%dT%H%M%SZ).log"

echo "Running manual task workflow. Log: ${LOG_FILE}"

# Isolated AH_HOME to avoid polluting user data
export AH_HOME
AH_HOME=$(mktemp -d)
export AH_TASK_FORCE_MOCK_MANAGER=1

REPO_DIR=$(mktemp -d)
cd "${REPO_DIR}"
git init -b main >/dev/null
git config user.email "manual@test.invalid"
git config user.name "Manual Tester"
echo "seed" >README.md
git add README.md
git commit -m "seed" >/dev/null

log_step() {
  echo "## $*" | tee -a "${LOG_FILE}"
}

# Step 1: interactive editor-based prompt
log_step "Interactive prompt with editor template"
export EDITOR="bash -c 'echo \"interactive prompt body\" > \"$1\"'"
ah task create manual-interactive --push-to-remote false >>"${LOG_FILE}" 2>&1

# Step 2: follow-up via prompt file on the task branch
log_step "Follow-up via --prompt-file on task branch"
git checkout manual-interactive >/dev/null
echo "follow-up content" >/tmp/manual-follow.txt
ah task create --prompt-file /tmp/manual-follow.txt --push-to-remote false --non-interactive >>"${LOG_FILE}" 2>&1

# Step 3: patch delivery (no push)
log_step "Patch delivery (no push)"
git checkout main >/dev/null
ah task create manual-patch --prompt "Patch only delivery" --delivery patch --push-to-remote false --non-interactive >>"${LOG_FILE}" 2>&1

# Step 4: follow with notifications disabled
log_step "Follow hand-off with notifications=no"
ah task create manual-follow --prompt "Follow UI hand-off" --notifications no --follow --push-to-remote false --non-interactive >>"${LOG_FILE}" 2>&1

echo "Manual task workflow finished. Inspect ${LOG_FILE} for details."
