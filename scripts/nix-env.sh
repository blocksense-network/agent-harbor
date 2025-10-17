#!/usr/bin/env bash

set -euo pipefail

# If direnv or an existing dev-shell already exported the marker, skip re-entering
if [[ -n "${IN_NIX_SHELL:-}" ]]; then
    exec bash -euo pipefail "$@"
fi

# Flags you want to keep in one place
flags=(
    --accept-flake-config
)

[[ "${NIX_IMPURE:-}" == 1 ]] && flags+=(--impure)

# Jump into the flake’s dev-shell, then run the script handed over by `just`
exec nix develop . "${flags[@]}" --command bash -euo pipefail "$@"
