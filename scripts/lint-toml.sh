#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail
if ! command -v taplo >/dev/null 2>&1; then
  echo "taplo is not installed. Example to run once: nix shell ~/nixpkgs#taplo -c taplo check" >&2
  exit 127
fi
taplo check
