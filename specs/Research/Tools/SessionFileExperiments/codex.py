#!/usr/bin/env python3
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

from common import print_heading, ensure_tool_available


def main() -> None:
    print_heading("Codex CLI experiment (no persistent sessions)")
    ensure_tool_available("codex")
    print("[i] Codex CLI does not document persistent sessions. Generate a diff and apply it manually:")
    print("    codex exec \"Edit README.md title\"")
    print("    # then, if supported by your build, apply last diff:")
    print("    codex apply")


if __name__ == "__main__":
    main()
