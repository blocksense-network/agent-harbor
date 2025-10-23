#!/usr/bin/env python3
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

import os
import shlex
import time
import re
import sys
import tempfile
from pathlib import Path

from common import (
    print_heading,
    ensure_tool_available,
    ensure_module_available,
    backup_file,
    trim_jsonl_midpoint,
)


def main() -> None:
    print_heading("Claude Code session experiment")
    ensure_tool_available("claude")
    ensure_module_available("pexpect")

    import pexpect  # type: ignore

    # Prepare a temporary settings file to install a PostToolUse hook that logs the transcript_path
    hook_dir = Path.cwd() / ".claude" / "hooks"
    hook_dir.mkdir(parents=True, exist_ok=True)
    hook_log = Path(tempfile.gettempdir()) / "claude_hook_input.json"
    # Ensure a clean slate for this run
    if hook_log.exists():
        try:
            hook_log.unlink()
        except Exception:
            pass
    hook_script = hook_dir / "dump-hook.sh"
    hook_script.write_text(
        "#!/usr/bin/env bash\n" \
        f"cat > {hook_log}\n",
        encoding="utf-8",
    )
    os.chmod(hook_script, 0o755)

    settings_path = Path(tempfile.gettempdir()) / "claude_settings_experiment.json"
    settings_path.write_text(
        "{"
        "\n  \"hooks\": {\n"
        "    \"PostToolUse\": [\n"
        "      {\n"
        "        \"matcher\": \"*\",\n"
        f"        \"hooks\": [{{ \"type\": \"command\", \"command\": \"{hook_script}\" }}]\n"
        "      }\n"
        "    ]\n"
        "  }\n"
        "}\n",
        encoding="utf-8",
    )

    # Prefer interactive pexpect session to ensure hooks can fire
    cmd = f"claude --allowed-tools Bash --debug hooks --settings {shlex.quote(str(settings_path))}"
    child = pexpect.spawn(cmd, encoding="utf-8", timeout=180)
    try:
        # Give the TUI time to initialize
        time.sleep(2)
        child.sendline("Run 'ls -1' in Bash, show the output.")
        # Approve possible permission prompt(s)
        time.sleep(2)
        child.sendline("y")
        # Let it work for a few seconds
        time.sleep(6)
        # Try to stop cleanly
        child.sendline("/stop")
        child.expect(pexpect.EOF)
    except Exception:
        # Best-effort cleanup; process may exit on its own
        try:
            child.close(force=True)
        except Exception:
            pass

    # Determine transcript_path via hook (preferred) or filesystem fallback
    transcript_path: Path | None = None
    if hook_log.exists():
        text = hook_log.read_text(encoding="utf-8")
        m = re.search(r'"transcript_path"\s*:\s*"([^"]+)"', text)
        if m:
            transcript_path = Path(m.group(1)).expanduser()
        else:
            print("[!] transcript_path not found in hook input; will try filesystem fallback.")
    else:
        print("[!] Hook did not capture input; will try filesystem fallback under ~/.claude/projects .")

    if transcript_path is None:
        projects_dir = Path.home() / ".claude" / "projects"
        sanitized = "-" + str(Path.cwd()).lstrip("/").replace("/", "-")
        candidate_dir = projects_dir / sanitized
        search_base = candidate_dir if candidate_dir.exists() else projects_dir
        candidates = sorted(search_base.rglob("*.jsonl"), key=lambda p: p.stat().st_mtime, reverse=True)
        if not candidates:
            print("[!] No transcript JSONL files found under ~/.claude/projects; cannot continue.")
            sys.exit(1)
        # Prefer the newest transcript with at least 2 JSONL lines
        selected = None
        for cand in candidates:
            try:
                with cand.open("r", encoding="utf-8") as fh:
                    cnt = sum(1 for _ in fh)
                if cnt >= 2:
                    selected = cand
                    break
            except Exception:
                continue
        transcript_path = selected or candidates[0]
        print(f"[i] Fallback selected latest transcript: {transcript_path}")

    assert transcript_path is not None
    print(f"[+] transcript_path: {transcript_path}")
    print(f"[+] transcript_path: {transcript_path}")

    if not transcript_path.exists():
        print("[!] transcript file does not exist on disk.")
        sys.exit(1)

    backup_file(transcript_path)
    try:
        trimmed = trim_jsonl_midpoint(transcript_path)
        print(f"[+] Wrote trimmed transcript: {trimmed}")
    except RuntimeError as e:
        print(f"[!] Skipping trim: {e}")
        print("[i] Tip: add a few follow-up prompts to lengthen the transcript, then re-run.")
        trimmed = None  # type: ignore

    print("[i] To test resume behavior, try: 'claude --resume' and verify the conversation truncation.")


if __name__ == "__main__":
    main()
