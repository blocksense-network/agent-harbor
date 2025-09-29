#!/usr/bin/env bash
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
