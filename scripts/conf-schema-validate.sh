#!/usr/bin/env bash
set -euo pipefail

if command -v ajv >/dev/null 2>&1; then
  AJV=ajv
else
  if [ -n "${IN_NIX_SHELL:-}" ]; then
    echo "Error: 'ajv' is missing inside Nix dev shell. Add pkgs.nodePackages.\"ajv-cli\" to flake.nix devShell inputs." >&2
    exit 127
  fi
  echo "ajv not found; falling back to 'yarn dlx ajv-cli' outside Nix shell (requires network)" >&2
  AJV='yarn dlx ajv-cli'
fi

for f in specs/schemas/*.json; do
  echo Validating $f
  $AJV compile -s "$f"
done

echo All schemas valid.
