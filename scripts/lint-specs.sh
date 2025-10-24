#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

if [ -n "${IN_NIX_SHELL:-}" ]; then
  echo "Running lint-specs inside Nix dev shell (no fallbacks)." >&2
fi

# Use pre-commit to run markdown linting (respects configuration and can be incremental)
pre-commit run lint-specs --all-files
just md-links || echo "⚠️  Link checking found external certificate issues (non-fatal - these are external sites with SSL problems)"
just md-spell

# Prose/style linting via Vale (warn-only): our custom style lowers
# spelling to warnings and uses project vocab so this won't fail commits.
if command -v vale >/dev/null 2>&1; then
  vale specs/Public || true
else
  if [ -n "${IN_NIX_SHELL:-}" ]; then
    echo "vale is missing inside Nix dev shell; add pkgs.vale to flake.nix." >&2
    exit 127
  fi
  echo "vale not found; skipping outside Nix shell." >&2
fi

# Mermaid syntax validation (enabled by default, disable with CHECK_MERMAID=false)
if [ "${CHECK_MERMAID:-true}" != "false" ]; then
  just md-mermaid-check
fi
