#!/usr/bin/env python3
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

import sys
from pathlib import Path
from common import print_heading, recent_files_under


def main() -> None:
    print_heading("Recent session-related files (last 15 min)")
    candidates = []
    # Claude Code
    candidates += recent_files_under([Path.home() / ".claude"], max_age_sec=900)
    # Goose
    candidates += recent_files_under([Path.home() / ".local/share/goose"], max_age_sec=900)
    # OpenCode
    candidates += recent_files_under([Path.home() / ".config/opencode"], max_age_sec=900)
    # Gemini (heuristics)
    candidates += recent_files_under([Path.home() / ".config/gemini-cli", Path.home() / ".config/gcloud"], max_age_sec=900)

    for p in candidates[:200]:
        try:
            print(f"{p}  (modified {int(p.stat().st_mtime)}s epoch)")
        except Exception:
            pass


if __name__ == "__main__":
    main()

