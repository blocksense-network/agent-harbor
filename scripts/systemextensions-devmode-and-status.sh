#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

echo "Enabling system extensions developer mode (requires SIP disabled)..."
if sudo -n systemextensionsctl developer on 2>/dev/null; then
  :
else
  echo "sudo password may be required to enable developer mode..."
  sudo systemextensionsctl developer on
fi

echo "Listing system extensions:"
systemextensionsctl list || true
