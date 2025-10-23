# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

import os
import io
import json
import uuid
import subprocess
from dataclasses import dataclass, field
from typing import Any, Dict, Optional, List
from datetime import datetime, timezone

ISOZ = "%Y-%m-%dT%H:%M:%S.%fZ"

def _now_iso_ms() -> str:
    dt = datetime.now(timezone.utc)
    return dt.strftime("%Y-%m-%dT%H:%M:%S.") + f"{int(dt.microsecond/1000):03d}Z"

def _now_stamp_for_filename() -> str:
    dt = datetime.now(timezone.utc)
    return dt.strftime("%Y-%m-%dT%H-%M-%S")

def _date_parts():
    dt = datetime.now(timezone.utc)
    return dt.strftime("%Y"), dt.strftime("%m"), dt.strftime("%d")

@dataclass
class RolloutRecorder:
    codex_home: str = os.path.expanduser("~/.codex")
    originator: str = "mock-agent"
    cli_version: str = "0.1.0"
    instructions: Optional[str] = None
    cwd: Optional[str] = None
    git: Optional[Dict[str, Any]] = None

    rollout_dir: str = field(init=False)
    rollout_path: str = field(init=False)
    session_id: str = field(init=False)
    _fh: io.TextIOWrapper = field(init=False, repr=False)

    def __post_init__(self):
        year, month, day = _date_parts()
        sess_root = os.path.join(self.codex_home, "sessions", year, month, day)
        os.makedirs(sess_root, exist_ok=True)
        self.session_id = str(uuid.uuid4())
        fname = f"rollout-{_now_stamp_for_filename()}-{self.session_id}.jsonl"
        self.rollout_dir = sess_root
        self.rollout_path = os.path.join(sess_root, fname)
        self._fh = open(self.rollout_path, "w", encoding="utf-8")
        try:
            os.chmod(self.rollout_path, 0o600)
        except PermissionError:
            pass
        self._write_session_meta()

    def _write_jsonl(self, obj: Dict[str, Any]) -> None:
        self._fh.write(json.dumps(obj, ensure_ascii=False) + "\n")
        self._fh.flush()

    def _write_session_meta(self) -> None:
        line = {
            "timestamp": _now_iso_ms(),
            "type": "session_meta",
            "payload": {
                "meta": {
                    "id": self.session_id,
                    "timestamp": _now_iso_ms(),
                    "cwd": self.cwd or os.getcwd(),
                    "originator": self.originator,
                    "cli_version": self.cli_version,
                    "instructions": self.instructions,
                },
                "git": self.git or {}
            }
        }
        self._write_jsonl(line)

    def record_turn_context(self, payload: Dict[str, Any]) -> None:
        self._write_jsonl({
            "timestamp": _now_iso_ms(),
            "type": "turn_context",
            "payload": payload
        })

    def record_message(self, role: str, text: str) -> None:
        self._write_jsonl({
            "timestamp": _now_iso_ms(),
            "type": "message",
            "payload": {
                "role": role,
                "content": [ { "type": "text", "text": text } ]
            }
        })

    def record_reasoning(self, summary_text: str, content: Optional[List[Dict[str, Any]]] = None) -> None:
        self._write_jsonl({
            "timestamp": _now_iso_ms(),
            "type": "reasoning",
            "payload": {
                "summary": [ { "type": "reasoning_text", "text": summary_text } ],
                "content": content or []
            }
        })

    def record_function_call(self, name: str, arguments: str, call_id: Optional[str] = None) -> None:
        self._write_jsonl({
            "timestamp": _now_iso_ms(),
            "type": "function_call",
            "payload": {
                "name": name,
                "arguments": arguments,
                "call_id": call_id or f"call_{uuid.uuid4().hex[:6]}"
            }
        })

    def record_local_shell_call(self, command: str, cwd: Optional[str] = None, env: Optional[Dict[str, str]] = None, status: str = "in_progress", call_id: Optional[str] = None) -> str:
        cid = call_id or f"call_{uuid.uuid4().hex[:6]}"
        self._write_jsonl({
            "timestamp": _now_iso_ms(),
            "type": "local_shell_call",
            "payload": {
                "call_id": cid,
                "status": status,
                "action": {
                    "command": command,
                    "cwd": cwd or (self.cwd or os.getcwd()),
                    "env": env or {}
                }
            }
        })
        return cid

    def record_event(self, event_type: str, payload: Dict[str, Any]) -> None:
        self._write_jsonl({
            "timestamp": _now_iso_ms(),
            "type": event_type,
            "payload": payload
        })

    def record_compacted(self, message: str) -> None:
        self._write_jsonl({
            "timestamp": _now_iso_ms(),
            "type": "compacted",
            "payload": { "message": message }
        })

    def flush(self) -> None:
        self._fh.flush()

    def close(self) -> None:
        try:
            self._fh.flush()
            self._fh.close()
        except Exception:
            pass


class SessionLogger:
    """
    Optional UI session logger controlled by env var CODEX_TUI_RECORD_SESSION=1.
    Writes to ~/.codex/logs/session-YYYYMMDDTHHMMSSZ.jsonl
    """
    def __init__(self, codex_home: str = os.path.expanduser("~/.codex")):
        self.codex_home = codex_home
        self.enabled = os.getenv("CODEX_TUI_RECORD_SESSION", "0") == "1"
        self._fh = None
        if self.enabled:
            log_dir = os.path.join(self.codex_home, "logs")
            os.makedirs(log_dir, exist_ok=True)
            ts = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
            self.path = os.path.join(log_dir, f"session-{ts}.jsonl")
            self._fh = open(self.path, "w", encoding="utf-8")
            try:
                os.chmod(self.path, 0o600)
            except PermissionError:
                pass
            self._write_meta("session_start")

    def _now(self) -> str:
        dt = datetime.now(timezone.utc)
        return dt.strftime("%Y-%m-%dT%H:%M:%S.") + f"{int(dt.microsecond/1000):03d}Z"

    def _write(self, obj: dict) -> None:
        if not self.enabled:
            return
        self._fh.write(json.dumps(obj, ensure_ascii=False) + "\n")
        self._fh.flush()

    def _write_meta(self, kind: str, **extra) -> None:
        self._write({
            "ts": self._now(),
            "dir": "meta",
            "kind": kind,
            **extra
        })

    def to_tui(self, kind: str, **payload) -> None:
        self._write({
            "ts": self._now(),
            "dir": "to_tui",
            "kind": kind,
            **payload
        })

    def from_tui(self, kind: str, **payload) -> None:
        self._write({
            "ts": self._now(),
            "dir": "from_tui",
            "kind": kind,
            **payload
        })

    def close(self) -> None:
        if self.enabled and self._fh:
            self._write_meta("session_end")
            self._fh.flush()
            self._fh.close()


@dataclass 
class ClaudeSessionRecorder:
    """
    Claude Code session format recorder.
    Stores sessions in ~/.claude/projects/<encoded-project-path>/<session-uuid>.jsonl
    """
    codex_home: str = os.path.expanduser("~/.claude")
    originator: str = "mock-agent"
    cli_version: str = "1.0.98"  # Mock Claude Code version
    cwd: Optional[str] = None
    
    session_dir: str = field(init=False)
    session_path: str = field(init=False)
    session_id: str = field(init=False)
    git_branch: str = field(init=False)
    _fh: io.TextIOWrapper = field(init=False, repr=False)
    _message_counter: int = field(init=False, default=0)
    _last_parent_uuid: Optional[str] = field(init=False, default=None)

    def __post_init__(self):
        # Get git info
        self.git_branch = self._get_git_branch()
        
        # Encode project path for Claude's directory structure
        project_path = self.cwd or os.getcwd()
        encoded_path = project_path.replace("/", "-")
        
        # Create Claude-style project directory
        self.session_dir = os.path.join(self.codex_home, "projects", encoded_path)
        os.makedirs(self.session_dir, exist_ok=True)
        
        # Generate session ID and file path
        self.session_id = str(uuid.uuid4())
        self.session_path = os.path.join(self.session_dir, f"{self.session_id}.jsonl")
        
        # Open file with restrictive permissions
        self._fh = open(self.session_path, "w", encoding="utf-8")
        try:
            os.chmod(self.session_path, 0o600)
        except PermissionError:
            pass

    def _get_git_branch(self) -> str:
        """Get current git branch, fallback to 'main' if not a git repo."""
        try:
            result = subprocess.run(
                ["git", "branch", "--show-current"],
                cwd=self.cwd or os.getcwd(),
                capture_output=True,
                text=True,
                timeout=5
            )
            if result.returncode == 0 and result.stdout.strip():
                return result.stdout.strip()
        except (subprocess.TimeoutExpired, subprocess.SubprocessError, FileNotFoundError):
            pass
        return "main"

    def _write_jsonl(self, obj: Dict[str, Any]) -> None:
        """Write a JSON object as a line to the session file."""
        self._fh.write(json.dumps(obj, ensure_ascii=False) + "\n")
        self._fh.flush()

    def _create_entry(self, entry_type: str, message: Dict[str, Any], 
                     is_meta: bool = False, tool_use_result: Any = None) -> Dict[str, Any]:
        """Create a Claude session entry with common fields."""
        entry_uuid = str(uuid.uuid4())
        
        entry = {
            "parentUuid": self._last_parent_uuid,
            "isSidechain": False,
            "userType": "external",
            "cwd": self.cwd or os.getcwd(),
            "sessionId": self.session_id,
            "version": self.cli_version,
            "gitBranch": self.git_branch,
            "type": entry_type,
            "message": message,
            "uuid": entry_uuid,
            "timestamp": _now_iso_ms()
        }
        
        if is_meta:
            entry["isMeta"] = True
            
        if tool_use_result is not None:
            entry["toolUseResult"] = tool_use_result
            
        # Update parent UUID for threading
        self._last_parent_uuid = entry_uuid
        return entry

    def record_user_message(self, content: str, is_meta: bool = False) -> None:
        """Record a user message."""
        message = {
            "role": "user",
            "content": content
        }
        entry = self._create_entry("user", message, is_meta=is_meta)
        self._write_jsonl(entry)

    def record_assistant_message(self, content: str, model: str = "claude-sonnet-4-20250514") -> None:
        """Record an assistant text message."""
        message = {
            "id": f"msg_{uuid.uuid4().hex[:8]}",
            "type": "message", 
            "role": "assistant",
            "model": model,
            "content": [
                {
                    "type": "text",
                    "text": content
                }
            ],
            "stop_reason": None,
            "stop_sequence": None,
            "usage": {
                "input_tokens": 10,  # Mock values
                "output_tokens": len(content.split()),
                "cache_creation_input_tokens": 0,
                "cache_read_input_tokens": 0,
                "server_tool_use": {"web_search_requests": 0},
                "service_tier": "standard"
            }
        }
        entry = self._create_entry("assistant", message)
        entry["requestId"] = f"req_{uuid.uuid4().hex[:8]}"
        self._write_jsonl(entry)

    def record_assistant_tool_use(self, tool_name: str, tool_input: Dict[str, Any], 
                                model: str = "claude-sonnet-4-20250514") -> str:
        """Record an assistant tool use and return the tool call ID."""
        tool_call_id = f"toolu_{uuid.uuid4().hex[:8]}"
        
        message = {
            "id": f"msg_{uuid.uuid4().hex[:8]}",
            "type": "message",
            "role": "assistant", 
            "model": model,
            "content": [
                {
                    "type": "tool_use",
                    "id": tool_call_id,
                    "name": tool_name,
                    "input": tool_input
                }
            ],
            "stop_reason": None,
            "stop_sequence": None,
            "usage": {
                "input_tokens": 10,
                "output_tokens": 50,
                "cache_creation_input_tokens": 0,
                "cache_read_input_tokens": 0,
                "server_tool_use": {"web_search_requests": 0},
                "service_tier": "standard"
            }
        }
        
        entry = self._create_entry("assistant", message)
        entry["requestId"] = f"req_{uuid.uuid4().hex[:8]}"
        self._write_jsonl(entry)
        return tool_call_id

    def record_tool_result(self, tool_call_id: str, content: str, 
                          is_error: bool = False, tool_result_data: Any = None) -> None:
        """Record a tool result."""
        message = {
            "role": "user",
            "content": [
                {
                    "tool_use_id": tool_call_id,
                    "type": "tool_result",
                    "content": content,
                    "is_error": is_error
                }
            ]
        }
        
        entry = self._create_entry("user", message, tool_use_result=tool_result_data)
        self._write_jsonl(entry)

    def flush(self) -> None:
        """Flush the file buffer."""
        self._fh.flush()

    def close(self) -> None:
        """Close the session file."""
        try:
            self._fh.flush()
            self._fh.close()
        except Exception:
            pass
