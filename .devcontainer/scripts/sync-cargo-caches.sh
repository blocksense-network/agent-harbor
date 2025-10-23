#!/bin/bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

# Script to sync development caches with host system
# This can be run multiple times to pick up new packages downloaded on the host
# Usage: sync-cargo-caches
# Reference: https://doc.rust-lang.org/cargo/guide/cargo-home.html

# Get the directory where this script is located for finding other utilities
SCRIPT_DIR="$(dirname "$0")"

HOST_CARGO_HOME="/host-cargo"
CONTAINER_CARGO_HOME="$HOME/.cargo"

# Always ensure container cargo directories exist
mkdir -p "$CONTAINER_CARGO_HOME"/{bin,registry,git}

# Sync host cache to container cache if available
if [ -d "$HOST_CARGO_HOME" ] && [ "$(ls -A $HOST_CARGO_HOME 2>/dev/null)" ]; then
  echo "Host Cargo cache detected, syncing with container cache..."

  # Sync registry data using efficient copying utility
  if [ -d "$HOST_CARGO_HOME/registry" ]; then
    echo "Syncing registry cache (this may take a moment on first run)..."

    # Execute the efficient copy utility as a standalone script
    if "$SCRIPT_DIR/efficient-copy" "$HOST_CARGO_HOME/registry" "$CONTAINER_CARGO_HOME/registry" --verbose; then
      echo "✓ Registry cache synced from host"

      # Show some stats about what we have
      if [ -d "$CONTAINER_CARGO_HOME/registry/cache" ]; then
        cache_count=$(find "$CONTAINER_CARGO_HOME/registry/cache" -name "*.crate" 2>/dev/null | wc -l)
        echo "  → $cache_count cached crate files available"
      fi
    else
      echo "⚠ Warning: Failed to sync cache from host, using existing container cache"
    fi
  fi

  echo "Container cache is now up-to-date with host. New downloads will be stored in container."
else
  echo "No host Cargo cache found, using container-only mode"
fi

# Ensure Cargo configuration exists (idempotent)
if [ ! -f "$CONTAINER_CARGO_HOME/config.toml" ]; then
  echo "Setting up Cargo configuration..."
  cat >"$CONTAINER_CARGO_HOME/config.toml" <<'EOF'
# Cargo configuration for container environment
# Reference: https://doc.rust-lang.org/cargo/reference/config.html

[net]
# Better network reliability in container environments
git-fetch-with-cli = true

# Uncomment the following section if you want to use a shared build cache
# [build]
# target-dir = "/tmp/cargo-target"
EOF
  echo "✓ Cargo configuration created"
else
  echo "✓ Cargo configuration already exists"
fi

echo "Development cache sync complete!"
