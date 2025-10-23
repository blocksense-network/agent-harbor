#!/usr/bin/env python3
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

import sys
from pathlib import Path

from common import (
    print_heading,
    ensure_tool_available,
    ensure_module_available,
)


def main() -> None:
    print_heading("OpenCode session experiment")
    ensure_tool_available("opencode")
    ensure_module_available("pexpect")
    import pexpect  # type: ignore

    child = pexpect.spawn("opencode", encoding="utf-8", timeout=180)
    try:
        time.sleep(2)
        child.sendline("Print the current working directory and then stop.")
        time.sleep(3)
        child.sendline("/stop")
        child.expect(pexpect.EOF)
    except Exception:
        try:
            child.close(force=True)
        except Exception:
            pass

    print("[i] Try 'opencode export' to locate the latest session JSON. Backup, trim mid-array, then attempt resume with --session <id>.")


if __name__ == "__main__":
    main()
