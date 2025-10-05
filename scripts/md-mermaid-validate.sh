#!/usr/bin/env bash
set -euo pipefail

# Resolve Mermaid CLI (mmdc)
if command -v mmdc >/dev/null 2>&1; then
  MMDC_CMD="mmdc"
else
  # Inside a Nix dev shell we must not fallback; require fixing flake.nix
  if [[ -n "${IN_NIX_SHELL:-}" ]]; then
    echo "mmdc (mermaid-cli) not found in Nix dev shell. Fix flake.nix devShell inputs; no fallbacks allowed." >&2
    exit 127
  fi
  # Outside Nix shell: allow best-effort yarn fallback with system Chrome/Chromium
  if command -v yarn >/dev/null 2>&1; then
    MMDC_CMD="yarn dlx @mermaid-js/mermaid-cli"
    for bin in chromium chromium-browser google-chrome google-chrome-stable; do
      if command -v "$bin" >/dev/null 2>&1; then
        export PUPPETEER_EXECUTABLE_PATH="$(command -v "$bin")"
        export PUPPETEER_PRODUCT=chrome
        export PUPPETEER_SKIP_CHROMIUM_DOWNLOAD=1
        break
      fi
    done
  else
    echo "mmdc (mermaid-cli) not found. Install mermaid-cli or run inside Nix dev shell." >&2
    exit 127
  fi
fi

TMPDIR_ROOT="${TMPDIR:-/tmp}"
FAILED=0

validate_file() {
  local file="$1"
  local tmpdir
  tmpdir="$(mktemp -d "$TMPDIR_ROOT/mmdc.$$.$(basename "$file").XXXX")"
  trap "rm -rf \"$tmpdir\"" RETURN

  local in_block=0
  local block_no=0
  local line_no=0
  while IFS= read -r line; do
    line_no=$((line_no + 1))
    case "$line" in
    '```mermaid'*)
      if [[ $in_block -eq 0 ]]; then
        in_block=1
        block_no=$((block_no + 1))
        : >"$tmpdir/block_${block_no}.mmd"
        continue
      fi
      ;;
    '```'*)
      if [[ $in_block -eq 1 ]]; then
        in_block=0
        # validate the block by attempting render
        local in_file="$tmpdir/block_${block_no}.mmd"
        local out_file="$tmpdir/block_${block_no}.svg"
        if ! $MMDC_CMD -i "$in_file" -o "$out_file" --quiet >/dev/null 2>&1; then
          echo "Mermaid error: $file: block $block_no (see $in_file)" >&2
          FAILED=1
        fi
        continue
      fi
      ;;
    esac
    if [[ $in_block -eq 1 ]]; then
      printf '%s\n' "$line" >>"$tmpdir/block_${block_no}.mmd"
    fi
  done <"$file"
}

if [[ $# -eq 0 ]]; then
  set -- specs/Public/*.md specs/Public/**/*.md 2>/dev/null || true
fi

shopt -s nullglob
for f in "$@"; do
  validate_file "$f"
done

exit $FAILED
