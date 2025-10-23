#!/usr/bin/env python3
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

import sys
from pathlib import Path

from common import (
    print_heading,
    ensure_tool_available,
    ensure_module_available,
    backup_file,
    trim_jsonl_midpoint,
)

SESS_DIR_CANDIDATE = Path.home() / ".local/share/goose/sessions"


def main() -> None:
    print_heading("Goose session experiment")
    ensure_tool_available("goose")
    ensure_module_available("pexpect")
    import pexpect  # type: ignore

    child = pexpect.spawn("goose session", encoding="utf-8", timeout=180)
    try:
        time.sleep(2)
        child.sendline("List files in the current directory and stop.")
        time.sleep(3)
        child.sendline("/stop")
        child.expect(pexpect.EOF)
    except Exception:
        try:
            child.close(force=True)
        except Exception:
            pass

    # Attempt to locate and trim the most recent Goose session JSONL
    if SESS_DIR_CANDIDATE.exists():
        try:
            latest = sorted(SESS_DIR_CANDIDATE.glob("*.jsonl"), key=lambda p: p.stat().st_mtime, reverse=True)[0]
            print(f"[i] Latest Goose session file: {latest}")
            backup_file(latest)
            trimmed = trim_jsonl_midpoint(latest)
            print(f"[+] Wrote trimmed Goose session to: {trimmed}")
        except IndexError:
            print("[!] No Goose session files found to trim.")
        except Exception as e:
            print(f"[!] Goose trim failed: {e}")

    print(f"[i] Check sessions under: {SESS_DIR_CANDIDATE} (format may vary). Back up, trim cautiously, then 'goose session --resume'.")


if __name__ == "__main__":
    main()
