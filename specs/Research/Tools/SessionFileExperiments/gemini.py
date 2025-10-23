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
    print_heading("Gemini CLI checkpoint experiment")
    ensure_tool_available("gemini")
    ensure_module_available("pexpect")
    import pexpect  # type: ignore

    # pexpect-only interactive run; try to enable auto approvals to ensure edits occur
    try:
        child = pexpect.spawn("gemini --checkpointing --approval-mode yolo", encoding="utf-8", timeout=240)
    except Exception:
        child = pexpect.spawn("gemini", encoding="utf-8", timeout=180)
    try:
        time.sleep(2)
        child.sendline("Say 'hello'; create or modify a small temporary file named experiment.tmp in the current directory using an edit tool; then show its contents.")
        # Give it time to propose + run an edit tool under YOLO
        time.sleep(12)
        # Follow-up to trigger more steps (thinking mode)
        child.sendline("Append another line to experiment.tmp, then print it again.")
        time.sleep(10)
        child.sendline("/stop")
        child.expect(pexpect.EOF)
    except Exception:
        try:
            child.close(force=True)
        except Exception:
            pass

    print("[i] If checkpointing is enabled, use '/restore' in a new gemini session to test rollback.")
    print("[i] Storage paths are platform-specific (e.g., under ~/.config or ~/.local/share). Inspect recent modified files to locate checkpoints.")


if __name__ == "__main__":
    main()
