#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

# Verify macOS FSKit E2E pre-requisites: SIP and AMFI disabled
# This script is best-effort; it prints actionable guidance and exits non-zero on failure.

red() { printf "\033[0;31m%s\033[0m\n" "$*"; }
yellow() { printf "\033[1;33m%s\033[0m\n" "$*"; }
green() { printf "\033[0;32m%s\033[0m\n" "$*"; }

if [[ "${IN_NIX_SHELL:-}" != "" ]]; then
  yellow "Running inside Nix dev shell. This check only queries system state; proceeding."
fi

fail=0

echo "Checking SIP (csrutil) status..."
csr_status=$(csrutil status 2>&1 || true)
echo "$csr_status"
if ! grep -qi "disabled" <<<"$csr_status"; then
  red "SIP does not appear to be disabled. FSKit unsigned extensions typically require SIP disabled."
  fail=1
fi

echo "Checking AMFI boot-args..."
nv_out=$(nvram boot-args 2>&1 || true)
echo "$nv_out"
if grep -qi "Error getting variable" <<<"$nv_out"; then
  yellow "Unable to read nvram boot-args. This may require sudo or Recovery changes."
fi

amfi_flags=(
  "amfi_get_out_of_my_way=1"
  "amfi_allow_any_signature=1"
  "amfi_allow_unsigned_code=1"
)

has_amfi_flag=0
for flag in "${amfi_flags[@]}"; do
  if grep -q "$flag" <<<"$nv_out"; then
    has_amfi_flag=1
    echo "Found AMFI flag: $flag"
    break
  fi
done

if [[ $has_amfi_flag -eq 0 ]]; then
  red "No known AMFI-disabling boot-args were detected in nvram."
  yellow "If you have AMFI disabled via a different mechanism, update this check to recognize it."
  fail=1
fi

if [[ $fail -ne 0 ]]; then
  echo
  red "Environment not ready for macOS FSKit E2E."
  cat <<'HELP'
Actions to enable unsigned FSKit extensions for development:
  1) Disable SIP from Recovery: csrutil disable
  2) Set AMFI boot-args from Recovery, e.g.: nvram boot-args="amfi_get_out_of_my_way=1"
  3) Reboot and re-run: just verify-macos-fskit-prereqs

Note: macOS versions differ; flags may vary. Proceed only on a dedicated dev machine.
HELP
  exit 1
fi

green "macOS FSKit prerequisites satisfied (SIP disabled and AMFI flags detected)."
