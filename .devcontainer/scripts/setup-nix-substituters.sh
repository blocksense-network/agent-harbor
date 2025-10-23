#!/bin/bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

# Script to conditionally add host store substituter for Nix
# This prevents errors if the host system doesn't have Nix installed
# References:
# - https://nixos.org/manual/nix/stable/command-ref/conf-file.html#conf-substituters

# Check if we've already configured the substituters
if grep -q "extra-substituters.*file:///nix/host-store" ~/.config/nix/nix.conf 2>/dev/null; then
  # Already configured, exit silently
  exit 0
fi

if [ -d "/nix/host-store" ] && [ "$(ls -A /nix/host-store 2>/dev/null)" ]; then
  echo "Host Nix store detected, enabling substituter..."
  echo "extra-substituters = file:///nix/host-store" >>~/.config/nix/nix.conf
  echo "trusted-substituters = file:///nix/host-store" >>~/.config/nix/nix.conf
else
  echo "No host Nix store found, using container-only mode"
fi
