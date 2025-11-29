#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

SCENARIO=${1:-tests/tools/mock-agent-acp/scenarios/acp_echo.yaml}
shift || true

cargo run -p mock-agent -- --scenario "${SCENARIO}" "$@"
