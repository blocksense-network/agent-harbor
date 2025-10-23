#!/usr/bin/env python3
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

import json
import os
import shutil
import sys
import time
from pathlib import Path
from typing import Optional, List
import subprocess


def print_heading(title: str) -> None:
    border = "=" * len(title)
    print(f"\n{border}\n{title}\n{border}")


def ensure_tool_available(binary_name: str) -> None:
    from shutil import which
    if which(binary_name) is None:
        print(f"[!] Required tool '{binary_name}' not found in PATH. Please install it and re-run.")
        sys.exit(1)


def ensure_module_available(module_name: str) -> None:
    try:
        __import__(module_name)
    except ImportError:
        print(f"[!] Required Python module '{module_name}' is missing. Install with: pip install {module_name}")
        sys.exit(1)


def find_most_recent(paths: List[Path]) -> Optional[Path]:
    existing = [p for p in paths if p.exists()]
    if not existing:
        return None
    return max(existing, key=lambda p: p.stat().st_mtime)


def glob_candidates(patterns: List[str]) -> List[Path]:
    candidates: List[Path] = []
    for pattern in patterns:
        for match in Path.home().glob(pattern):
            candidates.append(match)
    return candidates


def backup_file(file_path: Path) -> Path:
    timestamp = time.strftime("%Y%m%d-%H%M%S")
    backup_path = file_path.with_suffix(file_path.suffix + f".bak-{timestamp}")
    backup_path.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(str(file_path), str(backup_path))
    print(f"[+] Backed up '{file_path}' -> '{backup_path}'")
    return backup_path


def trim_jsonl_midpoint(file_path: Path) -> Path:
    with file_path.open("r", encoding="utf-8") as fh:
        lines = fh.readlines()
    if len(lines) < 2:
        raise RuntimeError("Not enough lines to trim safely (need >= 2)")
    midpoint = len(lines) // 2
    trimmed_lines = lines[:midpoint]
    trimmed_path = file_path.with_suffix(file_path.suffix + ".trimmed")
    with trimmed_path.open("w", encoding="utf-8") as out:
        out.writelines(trimmed_lines)
    print(f"[+] Wrote trimmed JSONL to '{trimmed_path}' ({midpoint}/{len(lines)} lines)")
    return trimmed_path


def is_json_file(path: Path) -> bool:
    try:
        with path.open("r", encoding="utf-8") as fh:
            json.load(fh)
        return True
    except Exception:
        return False


def trim_json_array_midpoint(file_path: Path) -> Path:
    with file_path.open("r", encoding="utf-8") as fh:
        data = json.load(fh)
    if not isinstance(data, list) or len(data) < 4:
        raise RuntimeError("JSON is not an array or too short to trim safely (need >= 4)")
    midpoint = len(data) // 2
    trimmed = data[:midpoint]
    trimmed_path = file_path.with_suffix(file_path.suffix + ".trimmed.json")
    with trimmed_path.open("w", encoding="utf-8") as out:
        json.dump(trimmed, out, ensure_ascii=False, indent=2)
    print(f"[+] Wrote trimmed JSON array to '{trimmed_path}' ({midpoint}/{len(data)} items)")
    return trimmed_path


# ---------- tmux helpers ----------

def tmux_run(
    session: str,
    command: str,
    sends: List[str],
    wait_secs: float = 2.0,
    capture_lines: int = 500,
    kill: bool = True,
) -> str:
    """
    Run `command` inside a detached tmux session, send provided keystrokes (each followed by Enter),
    capture the pane contents, and optionally kill the session.

    Returns the captured pane text.
    """
    ensure_tool_available("tmux")

    # Start session
    subprocess.run(
        [
            "tmux",
            "new-session",
            "-d",
            "-s",
            session,
            command,
        ],
        check=True,
    )

    # Give program time to initialize
    time.sleep(wait_secs)

    # Send scripted inputs
    for s in sends:
        subprocess.run(["tmux", "send-keys", "-t", session, s, "Enter"], check=True)
        time.sleep(wait_secs)

    # Capture pane output
    start = f"-S -{max(1, capture_lines)}"
    result = subprocess.run(
        ["bash", "-lc", f"tmux capture-pane -t {session} -p {start}"],
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    output = result.stdout

    # Cleanup
    if kill:
        subprocess.run(["tmux", "kill-session", "-t", session], check=False)

    return output


def recent_files_under(paths: List[Path], max_age_sec: int = 900) -> List[Path]:
    now = time.time()
    out: List[Path] = []
    for base in paths:
        if not base.exists():
            continue
        for p in base.rglob("*"):
            try:
                if p.is_file() and (now - p.stat().st_mtime) <= max_age_sec:
                    out.append(p)
            except FileNotFoundError:
                # File might disappear between walk and stat; ignore
                continue
    return sorted(out, key=lambda p: p.stat().st_mtime, reverse=True)
