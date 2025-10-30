# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

import json
import os
import sys
import uuid
import subprocess
import shutil
import time
import threading
from typing import Dict, Any, List, Optional

try:
    import yaml
    YAML_AVAILABLE = True
except ImportError:
    YAML_AVAILABLE = False

try:
    import zmq
    ZMQ_AVAILABLE = True
except ImportError:
    ZMQ_AVAILABLE = False

from .session_io import RolloutRecorder, SessionLogger, ClaudeSessionRecorder, _now_iso_ms
from .tools import call_tool, ToolError

# ANSI color codes matching Agent Harbor TUI theme (Catppuccin Mocha)
class Theme:
    # Base colors
    BG = "\033[48;2;17;17;27m"         # Base background
    SURFACE = "\033[48;2;24;24;37m"    # Card/surface background
    TEXT = "\033[38;2;205;214;244m"    # Main text
    MUTED = "\033[38;2;127;132;156m"   # Secondary text
    PRIMARY = "\033[38;2;137;180;250m" # Blue for primary actions
    ACCENT = "\033[38;2;166;218;149m"  # Green for success/accent
    SUCCESS = "\033[38;2;166;218;149m" # Green
    WARNING = "\033[38;2;250;179;135m" # Orange/yellow
    ERROR = "\033[38;2;243;139;168m"   # Red/pink
    BORDER = "\033[38;2;69;71;90m"     # Border color
    MAGENTA = "\033[38;2;245;194;231m" # Magenta for agent branding

    # Control codes
    RESET = "\033[0m"
    BOLD = "\033[1m"
    DIM = "\033[2m"
    REVERSE = "\033[7m"
    UNDERLINE = "\033[4m"

    # Cursor control
    HIDE_CURSOR = "\033[?25l"
    SHOW_CURSOR = "\033[?25h"
    CLEAR_LINE = "\033[2K"
    MOVE_TO_START = "\033[G"

    # Spinner frames
    SPINNER_FRAMES = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]

class RichOutput:
    """Rich terminal output formatting for mock-agent"""

    def __init__(self, no_colors: bool = False):
        self.no_colors = no_colors
        self.spinner_active = False
        self.spinner_thread = None
        self.current_spinner_text = ""

    def _print_raw(self, text: str) -> None:
        """Print text without newline"""
        sys.stdout.write(text)
        sys.stdout.flush()

    def _println(self, text: str = "") -> None:
        """Print text with newline"""
        print(text)

    def _colorize(self, text: str, color: str) -> str:
        """Apply color to text if colors are enabled"""
        if self.no_colors:
            return text
        return f"{color}{text}{Theme.RESET}"

    def _format_box(self, title: str, content: str, color: str = Theme.BORDER) -> str:
        """Create a framed box with rounded borders"""
        lines = content.split('\n')
        max_len = max(len(line) for line in lines) if lines else 0
        title_len = len(title)

        # Calculate total width (content + padding + borders)
        width = max(max_len + 4, title_len + 6)  # At least title + brackets + borders

        # Top border with title
        top = f"╭─ {title} " + "─" * (width - title_len - 5) + "╮"

        # Content lines with padding
        formatted_lines = []
        for line in lines:
            padded = f"│ {line:<{max_len}} │"
            formatted_lines.append(padded)

        # Bottom border
        bottom = "╰" + "─" * (width - 2) + "╯"

        if self.no_colors:
            return f"{top}\n" + "\n".join(formatted_lines) + f"\n{bottom}"
        else:
            return f"{color}{top}{Theme.RESET}\n" + \
                   f"{color}" + f"{color}\n".join(formatted_lines) + f"{Theme.RESET}\n" + \
                   f"{color}{bottom}{Theme.RESET}"

    def print_header(self, text: str) -> None:
        """Print a header with Agent Harbor branding"""
        logo = "🚀 Agent Harbor"
        header = self._colorize(f"{Theme.BOLD}{logo}", Theme.MAGENTA) + " " + self._colorize(text, Theme.MUTED)
        self._println(header)

    def print_thinking(self, text: str) -> None:
        """Print agent thinking with styled output"""
        icon = "🤔"
        styled = self._colorize(f"{icon} Thinking:", Theme.PRIMARY) + " " + self._colorize(text, Theme.TEXT)
        self._println(styled)

    def print_tool_start(self, tool_name: str, args: Dict[str, Any]) -> None:
        """Print tool execution start"""
        icon = "🔧"
        tool_display = f"{Theme.ACCENT}{icon} {tool_name}{Theme.RESET}"
        args_display = f"{Theme.MUTED}{self._format_args(args)}{Theme.RESET}"

        # Create a framed box for tool execution
        content = f"Executing: {tool_name}\nArguments: {self._format_args(args)}"
        box = self._format_box(f"🔧 {tool_name}", content, Theme.ACCENT)
        self._println(box)

    def print_tool_result(self, tool_name: str, result: Any, status: str = "ok") -> None:
        """Print tool execution result"""
        if status == "ok":
            icon = "✅"
            color = Theme.SUCCESS
        else:
            icon = "❌"
            color = Theme.ERROR

        # Format result for display
        if isinstance(result, dict):
            if "path" in result and "content" in result:
                # File read result
                content = result["content"]
                if len(content) > 200:
                    content = content[:200] + "..."
                result_display = f"File content ({len(result['content'])} chars):\n{content}"
            else:
                result_display = json.dumps(result, indent=2)
        elif isinstance(result, str):
            result_display = result
        else:
            result_display = str(result)

        content = f"Result: {result_display}"
        box = self._format_box(f"{icon} {tool_name}", content, color)
        self._println(box)

    def print_file_edit(self, path: str, lines_added: int, lines_removed: int) -> None:
        """Print file edit notification"""
        icon = "📝"
        changes = []
        if lines_added > 0:
            changes.append(f"{Theme.SUCCESS}+{lines_added}{Theme.RESET}")
        if lines_removed > 0:
            changes.append(f"{Theme.ERROR}-{lines_removed}{Theme.RESET}")

        change_display = " ".join(changes) if changes else "no changes"

        content = f"Modified: {path}\nChanges: {change_display}"
        box = self._format_box(f"{icon} File Edit", content, Theme.WARNING)
        self._println(box)

    def print_error(self, message: str, error_type: str = "Error") -> None:
        """Print error message"""
        icon = "❌"
        content = f"Type: {error_type}\nMessage: {message}"
        box = self._format_box(f"{icon} {error_type}", content, Theme.ERROR)
        self._println(box)

    def print_success(self, message: str) -> None:
        """Print success message"""
        icon = "🎉"
        box = self._format_box(f"{icon} Success", message, Theme.SUCCESS)
        self._println(box)

    def print_user_input_prompt(self, prompt: str) -> None:
        """Print user input prompt"""
        icon = "👤"
        styled = self._colorize(f"{icon} User Input:", Theme.TEXT) + f" {prompt}"
        self._println(styled)

    def start_spinner(self, text: str) -> None:
        """Start a spinner animation"""
        if self.spinner_active:
            self.stop_spinner()

        self.current_spinner_text = text
        self.spinner_active = True

        def spinner_loop():
            frame_idx = 0
            while self.spinner_active:
                frame = Theme.SPINNER_FRAMES[frame_idx % len(Theme.SPINNER_FRAMES)]
                self._print_raw(f"\r{Theme.PRIMARY}{frame}{Theme.RESET} {self.current_spinner_text}")
                frame_idx += 1
                time.sleep(0.1)

        self.spinner_thread = threading.Thread(target=spinner_loop, daemon=True)
        self.spinner_thread.start()

    def stop_spinner(self) -> None:
        """Stop the spinner animation"""
        if self.spinner_active:
            self.spinner_active = False
            if self.spinner_thread and self.spinner_thread.is_alive():
                self.spinner_thread.join(timeout=0.2)
            self._print_raw(f"\r{Theme.CLEAR_LINE}{Theme.MOVE_TO_START}")
            self.spinner_thread = None

    def print_progress(self, current: int, total: int, message: str = "") -> None:
        """Print progress bar"""
        if total == 0:
            return

        width = 30
        filled = int(width * current / total)
        bar = "█" * filled + "░" * (width - filled)
        percent = int(100 * current / total)

        progress = f"{Theme.PRIMARY}[{bar}]{Theme.RESET} {percent}% {message}"
        self._print_raw(f"\r{Theme.CLEAR_LINE}{progress}")

        if current >= total:
            self._println()  # New line when complete

    def _format_args(self, args: Dict[str, Any]) -> str:
        """Format tool arguments for display"""
        formatted = []
        for key, value in args.items():
            if isinstance(value, str) and len(value) > 50:
                value = value[:47] + "..."
            formatted.append(f"{key}={repr(value)}")
        return ", ".join(formatted)

# Global rich output instance (will be replaced in run_scenario with proper no_colors setting)
_rich_output = RichOutput()

def _display_command_output(lines: List[str]) -> None:
    """Display command output in a framed box showing last 6 lines."""
    if not lines:
        return

    # Create content showing the last 6 lines
    display_lines = lines[-6:] if len(lines) > 6 else lines
    content = "\n".join(display_lines)

    # Show total lines if truncated
    if len(lines) > 6:
        content = f"... ({len(lines) - 6} more lines)\n{content}"

    # Display in a framed box
    box = _rich_output._format_box("💻 Command Output", content, Theme.TEXT)
    print(box)
    sys.stdout.flush()

def _print_trace(kind: str, msg: str) -> None:
    """Enhanced print function with rich formatting"""
    if kind == "thinking":
        _rich_output.print_thinking(msg)
    elif kind == "tool":
        # Tool messages are handled separately in the event processing
        sys.stdout.write(f"[{kind}] {msg}\n")
        sys.stdout.flush()
    elif kind == "assistant":
        # Assistant messages with styled output
        styled_msg = f"{Theme.ACCENT}💬 Assistant:{Theme.RESET} {Theme.TEXT}{msg}{Theme.RESET}"
        print(styled_msg)
    elif kind == "tool_progress":
        # Progress messages
        print(f"{Theme.MUTED}⏳ {msg}{Theme.RESET}")
    elif kind == "user_input":
        # User input display
        print(f"{Theme.PRIMARY}👤 User:{Theme.RESET} {Theme.TEXT}{msg}{Theme.RESET}")
    elif kind == "error":
        _rich_output.print_error(msg, "Error")
    else:
        # Default fallback
        print(f"[{kind}] {msg}")

class TuiTestClient:
    """ZeroMQ client for TUI testing IPC communication."""

    def __init__(self, uri: str):
        if not ZMQ_AVAILABLE:
            raise ImportError("ZeroMQ not available. Install with: pip install pyzmq")
        self.uri = uri
        self.context = zmq.Context()
        self.socket = self.context.socket(zmq.REQ)

        # Set connection timeout - try to connect with a short timeout
        self.socket.setsockopt(zmq.CONNECT_TIMEOUT, 2000)  # 2 second connection timeout
        self.socket.setsockopt(zmq.RCVTIMEO, 5000)  # 5 second receive timeout
        self.socket.setsockopt(zmq.SNDTIMEO, 5000)  # 5 second send timeout

        try:
            self.socket.connect(uri)
            # Test the connection by sending a ping
            print(f"[tui-test] Testing connection to {uri}")
            self.socket.send_string("ping")
            print("[tui-test] Ping sent, waiting for response")

            # Use poller with short timeout to test connection
            poller = zmq.Poller()
            poller.register(self.socket, zmq.POLLIN)

            socks = dict(poller.poll(2000))  # 2 second timeout
            if self.socket in socks and socks[self.socket] == zmq.POLLIN:
                response_str = self.socket.recv_string()
                print(f"[tui-test] Received response: {repr(response_str)}")
                if response_str != "ok":
                    raise ConnectionError(f"TUI testing server responded with error: {response_str}")
                print("[tui-test] Connection test successful")
            else:
                raise ConnectionError(
                    f"Cannot connect to TUI testing server at {uri}. "
                    f"This usually means the --tui-testing-uri flag was specified outside of a tui-testing test case. "
                    f"The TUI_TESTING_URI environment variable should only be set by tui-testing framework."
                )
        except zmq.ZMQError as e:
            raise ConnectionError(f"Failed to connect to TUI testing server at {uri}: {e}") from e
        except Exception as e:
            raise ConnectionError(
                f"Failed to connect to TUI testing server at {uri}. "
                f"This usually means the --tui-testing-uri flag was specified outside of a tui-testing test case."
            ) from e

    def request_screenshot(self, label: str) -> bool:
        """Request a screenshot capture with the given label."""
        print(f"[tui-test] Requesting screenshot: {label}")
        try:
            # Send simple string message for compatibility
            message = f"screenshot:{label}"
            print(f"[tui-test] Sending message: {message}")
            self.socket.send_string(message)
            print(f"[tui-test] Message sent, waiting for response...")
            response_str = self.socket.recv_string()
            print(f"[tui-test] Received response: {response_str}")
            return response_str == "ok"
        except zmq.ZMQError as e:
            if e.errno == zmq.EFSM:
                # Socket is in wrong state, try to reset it
                print(f"[tui-test] Screenshot request failed (socket state error), skipping: {e}")
                _print_trace("tui-test", f"Screenshot request failed (socket state error), skipping: {e}")
            else:
                print(f"[tui-test] Screenshot request failed: {e}")
                _print_trace("tui-test", f"Screenshot request failed: {e}")
            return False
        except Exception as e:
            print(f"[tui-test] Screenshot request failed: {e}")
            _print_trace("tui-test", f"Screenshot request failed: {e}")
            return False

    def ping(self) -> bool:
        """Send a ping to check connectivity."""
        try:
            self.socket.send_string("ping")
            response_str = self.socket.recv_string()
            return response_str == "ok"
        except Exception as e:
            _print_trace("tui-test", f"Ping failed: {e}")
            return False

    def close(self):
        """Close the ZeroMQ connection."""
        try:
            self.socket.close()
            self.context.term()
        except Exception:
            pass

def _print_trace(kind: str, msg: str) -> None:
    sys.stdout.write(f"[{kind}] {msg}\n")
    sys.stdout.flush()

def _as_json(obj: Any) -> str:
    return json.dumps(obj, ensure_ascii=False)

def _execute_checkpoint_cmd(checkpoint_cmd: str, workspace: str) -> None:
    """Execute the checkpoint command after agentToolUse or agentEdits events."""
    if not checkpoint_cmd:
        return

    # Debug: check if AH_RECORDER_IPC_SOCKET is set
    ipc_socket = os.environ.get('AH_RECORDER_IPC_SOCKET')
    if ipc_socket:
        _print_trace("checkpoint", f"AH_RECORDER_IPC_SOCKET is set: {ipc_socket}")
    else:
        _print_trace("checkpoint", "AH_RECORDER_IPC_SOCKET is NOT set")

    try:
        # Execute the checkpoint command in the workspace directory
        process = subprocess.Popen(
            checkpoint_cmd,
            shell=True,
            cwd=workspace,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True
        )

        stdout, stderr = process.communicate(timeout=60)  # 60 second timeout

        if stdout.strip():
            _print_trace("checkpoint", f"stdout: {stdout.strip()}")
        if stderr.strip():
            _print_trace("checkpoint", f"stderr: {stderr.strip()}")

        if process.returncode != 0:
            _print_trace("checkpoint", f"Command failed with exit code {process.returncode}")
        else:
            _print_trace("checkpoint", f"Executed checkpoint command: {checkpoint_cmd}")

    except subprocess.TimeoutExpired:
        _print_trace("checkpoint", f"Checkpoint command timeout: {checkpoint_cmd}")
        process.kill()
    except Exception as e:
        _print_trace("checkpoint", f"Checkpoint command execution failed: {checkpoint_cmd} - {e}")

def _initialize_repository(scenario: Dict[str, Any], workspace: str, scenario_path: str) -> None:
    """Initialize repository based on scenario.repo section."""
    repo_config = scenario.get("repo", {})
    if not repo_config:
        return

    # Initialize git repo if requested
    if repo_config.get("init", False):
        _print_trace("repo", "Initializing git repository")
        try:
            subprocess.run(["git", "init"], cwd=workspace, check=True, capture_output=True)
            subprocess.run(["git", "config", "user.name", "Mock Agent"], cwd=workspace, check=True)
            subprocess.run(["git", "config", "user.email", "mock@agent.test"], cwd=workspace, check=True)
        except subprocess.CalledProcessError as e:
            _print_trace("repo", f"Failed to initialize git repo: {e}")
            return

    # Switch to or create branch if specified
    branch = repo_config.get("branch")
    if branch:
        try:
            # Try to checkout existing branch
            result = subprocess.run(["git", "checkout", branch], cwd=workspace, capture_output=True)
            if result.returncode != 0:
                # Branch doesn't exist, create it
                subprocess.run(["git", "checkout", "-b", branch], cwd=workspace, check=True)
            _print_trace("repo", f"Switched to branch: {branch}")
        except subprocess.CalledProcessError as e:
            _print_trace("repo", f"Failed to switch to branch {branch}: {e}")

    # Copy seed directory if specified
    seed_dir = repo_config.get("dir")
    if seed_dir:
        scenario_dir = os.path.dirname(os.path.abspath(scenario_path))
        source_dir = os.path.join(scenario_dir, seed_dir)
        if os.path.exists(source_dir):
            _print_trace("repo", f"Copying seed directory: {seed_dir}")
            try:
                for item in os.listdir(source_dir):
                    src = os.path.join(source_dir, item)
                    dst = os.path.join(workspace, item)
                    if os.path.isdir(src):
                        shutil.copytree(src, dst, dirs_exist_ok=True)
                    else:
                        shutil.copy2(src, dst)
            except Exception as e:
                _print_trace("repo", f"Failed to copy seed directory: {e}")
        else:
            _print_trace("repo", f"Seed directory not found: {source_dir}")

    # Create inline seed files
    seed_files = repo_config.get("files", [])
    for file_spec in seed_files:
        path = file_spec.get("path", "")
        if not path:
            continue

        content = file_spec.get("contents", "")
        # Handle base64 encoded content
        if isinstance(file_spec.get("contents"), dict) and "base64" in file_spec["contents"]:
            import base64
            content = base64.b64decode(file_spec["contents"]["base64"]).decode("utf-8")

        try:
            full_path = os.path.join(workspace, path)
            os.makedirs(os.path.dirname(full_path), exist_ok=True)
            with open(full_path, "w", encoding="utf-8") as f:
                f.write(content)
            _print_trace("repo", f"Created seed file: {path}")
        except Exception as e:
            _print_trace("repo", f"Failed to create seed file {path}: {e}")

    # Commit initial files if git repo was initialized
    if repo_config.get("init", False):
        try:
            subprocess.run(["git", "add", "."], cwd=workspace, check=True, capture_output=True)
            result = subprocess.run(["git", "commit", "-m", "Initial commit", "--allow-empty"], cwd=workspace, capture_output=True)
            if result.returncode == 0:
                _print_trace("repo", "Created initial commit")
            else:
                _print_trace("repo", "No files to commit or commit failed")
        except subprocess.CalledProcessError as e:
            _print_trace("repo", f"Failed to create initial commit: {e}")

def _check_expectations(expect: Dict[str, Any], workspace: str) -> None:
    """Check scenario expectations."""
    # Check exit code if specified (mock scenarios typically don't have real exit codes)
    if "exitCode" in expect:
        expected_code = expect["exitCode"]
        _print_trace("expect", f"Expected exit code: {expected_code} (not enforced in mock mode)")

    # Check artifacts
    if "artifacts" in expect:
        for artifact in expect["artifacts"]:
            artifact_type = artifact.get("type", "file")
            pattern = artifact.get("pattern", "")

            if artifact_type == "taskFile":
                # Check for task files using glob pattern
                import glob
                matches = glob.glob(os.path.join(workspace, pattern))
                if matches:
                    _print_trace("expect", f"✓ Found artifact: {pattern} ({len(matches)} matches)")
                else:
                    _print_trace("expect", f"✗ Missing artifact: {pattern}")
            else:
                _print_trace("expect", f"Unknown artifact type: {artifact_type}")

def _run_assertions(assertion: Dict[str, Any], workspace: str) -> bool:
    """Run assertions and return True if all pass, False otherwise."""
    all_passed = True

    # Filesystem assertions
    if "fs" in assertion:
        fs_assert = assertion["fs"]
        if "exists" in fs_assert:
            for path in fs_assert["exists"]:
                full_path = os.path.join(workspace, path)
                if os.path.exists(full_path):
                    _print_trace("assert", f"✓ fs.exists: {path}")
                else:
                    _print_trace("assert", f"✗ fs.exists: {path} (not found)")
                    all_passed = False

        if "notExists" in fs_assert:
            for path in fs_assert["notExists"]:
                full_path = os.path.join(workspace, path)
                if not os.path.exists(full_path):
                    _print_trace("assert", f"✓ fs.notExists: {path}")
                else:
                    _print_trace("assert", f"✗ fs.notExists: {path} (exists)")
                    all_passed = False

    # Text content assertions (simplified - checking files since we don't have terminal buffer)
    if "text" in assertion and "contains" in assertion["text"]:
        for text_pattern in assertion["text"]["contains"]:
            # In mock mode, we can't check terminal buffer, so we'll check if any files contain the text
            found = False
            for root, dirs, files in os.walk(workspace):
                for file in files:
                    if file.endswith(('.txt', '.md', '.py', '.json', '.log')):  # Common text files
                        try:
                            with open(os.path.join(root, file), 'r', encoding='utf-8') as f:
                                if text_pattern in f.read():
                                    found = True
                                    break
                        except:
                            pass
                if found:
                    break

            if found:
                _print_trace("assert", f"✓ text.contains: {repr(text_pattern)}")
            else:
                _print_trace("assert", f"✗ text.contains: {repr(text_pattern)} (not found in files)")
                all_passed = False

    # JSON file assertions
    if "json" in assertion and "file" in assertion["json"]:
        json_assert = assertion["json"]["file"]
        path = json_assert.get("path")
        pointer = json_assert.get("pointer", "")
        expected = json_assert.get("equals")

        if path:
            full_path = os.path.join(workspace, path)
            try:
                with open(full_path, 'r', encoding='utf-8') as f:
                    data = json.load(f)

                # Simple JSON pointer implementation (only supports root level and basic paths)
                if pointer == "":
                    actual = data
                elif pointer.startswith("/"):
                    # Basic pointer support - split by / and navigate
                    parts = pointer.strip("/").split("/")
                    actual = data
                    for part in parts:
                        if part in actual:
                            actual = actual[part]
                        else:
                            raise KeyError(f"Pointer {pointer} not found")
                else:
                    actual = data.get(pointer)

                if actual == expected:
                    _print_trace("assert", f"✓ json.file: {path} {pointer} == {expected}")
                else:
                    _print_trace("assert", f"✗ json.file: {path} {pointer} expected {expected}, got {actual}")
                    all_passed = False

            except Exception as e:
                _print_trace("assert", f"✗ json.file: {path} - {e}")
                all_passed = False

    # Git commit assertions
    if "git" in assertion and "commit" in assertion["git"]:
        git_assert = assertion["git"]["commit"]
        if "messageContains" in git_assert:
            expected_text = git_assert["messageContains"]
            try:
                # Get the latest commit message
                result = subprocess.run(["git", "log", "-1", "--pretty=%B"],
                                      cwd=workspace, capture_output=True, text=True, check=True)
                commit_msg = result.stdout.strip()

                if expected_text in commit_msg:
                    _print_trace("assert", f"✓ git.commit: message contains {repr(expected_text)}")
                else:
                    _print_trace("assert", f"✗ git.commit: message {repr(commit_msg)} doesn't contain {repr(expected_text)}")
                    all_passed = False

            except subprocess.CalledProcessError as e:
                _print_trace("assert", f"✗ git.commit: failed to get commit message - {e}")
                all_passed = False

    return all_passed

def _execute_hooks(hooks_config: Dict[str, Any], event_name: str, hook_input: Dict[str, Any], workspace: str) -> None:
    """Execute hooks for a given event."""
    if not hooks_config or event_name not in hooks_config:
        return

    session_id = hooks_config.get("session_id", "mock-session-123")
    cwd = workspace

    for matcher_config in hooks_config[event_name]:
        matcher = matcher_config.get("matcher", "*")
        hooks = matcher_config.get("hooks", [])

        # For simplicity, we'll execute hooks for all matchers in this mock implementation
        # In a real implementation, you'd check the matcher against the tool name
        for hook in hooks:
            if hook.get("type") == "command":
                command = hook.get("command", "")
                timeout = hook.get("timeout", 60)

                # Prepare hook input JSON
                hook_input_data = {
                    "session_id": session_id,
                    "transcript_path": "/tmp/mock-transcript.jsonl",  # Mock path
                    "cwd": cwd,
                    "hook_event_name": event_name,
                    **hook_input
                }

                try:
                    # Execute the hook command with JSON input via stdin
                    process = subprocess.Popen(
                        command,
                        shell=True,
                        cwd=cwd,
                        stdin=subprocess.PIPE,
                        stdout=subprocess.PIPE,
                        stderr=subprocess.PIPE,
                        text=True,
                        env={**os.environ, "CLAUDE_PROJECT_DIR": workspace}
                    )

                    # Send JSON input
                    stdout, stderr = process.communicate(
                        input=json.dumps(hook_input_data),
                        timeout=timeout
                    )

                    _print_trace("hook", f"Executed {event_name} hook: {command}")
                    if stdout.strip():
                        _print_trace("hook", f"Hook stdout: {stdout.strip()}")
                    if stderr.strip():
                        _print_trace("hook", f"Hook stderr: {stderr.strip()}")

                except subprocess.TimeoutExpired:
                    _print_trace("hook", f"Hook timeout: {command}")
                    process.kill()
                except Exception as e:
                    _print_trace("hook", f"Hook execution failed: {command} - {e}")

def run_scenario(scenario_path: str, workspace: str, codex_home: str = os.path.expanduser("~/.codex"), format: str = "codex", checkpoint_cmd: str = None, fast_mode: bool = False, tui_testing_uri: str = None, interactive: bool = False, no_colors: bool = False) -> str:
    # Set up rich output with color settings
    global _rich_output
    _rich_output = RichOutput(no_colors=no_colors)

    os.makedirs(workspace, exist_ok=True)
    with open(scenario_path, "r", encoding="utf-8") as f:
        content = f.read()

    # Parse scenario file (YAML or JSON)
    try:
        if YAML_AVAILABLE:
            scenario = yaml.safe_load(content)
        else:
            # Fall back to JSON parsing if YAML is not available
            scenario = json.loads(content)
    except (yaml.YAMLError if YAML_AVAILABLE else json.JSONDecodeError) as e:
        error_type = "YAML" if YAML_AVAILABLE else "JSON"
        _print_trace("error", f"Failed to parse {error_type} file {scenario_path}: {e}")
        raise

    # Print Agent Harbor header
    scenario_name = scenario.get("name", "Unknown Scenario")
    _rich_output.print_header(f"Running scenario: {scenario_name}")

    # Initialize repository based on scenario.repo section
    _initialize_repository(scenario, workspace, scenario_path)

    # Extract hooks configuration
    hooks_config = scenario.get("hooks", {})

    # Initialize TUI testing client only if URI is explicitly provided (not from environment)
    tui_client = None
    print(f"[tui-test] tui_testing_uri parameter: {tui_testing_uri}")
    if tui_testing_uri:
        print(f"[tui-test] Initializing TUI test client with URI: {tui_testing_uri}")
        try:
            tui_client = TuiTestClient(tui_testing_uri)
            _print_trace("tui-test", f"Connected to TUI testing server at {tui_testing_uri}")
            print(f"[tui-test] TUI test client initialized successfully")
        except Exception as e:
            print(f"[tui-test] Failed to initialize TUI test client: {e}")
            _print_trace("tui-test", f"Failed to connect to TUI testing server: {e}")
            # If TUI testing is requested but server is not available, exit with error
            raise RuntimeError(f"TUI testing server not available at {tui_testing_uri}: {e}")

    # Run the scenario
    result_path = None
    if fast_mode:
        result_path = _run_scenario_fast_mode(scenario, workspace, codex_home, format, hooks_config, checkpoint_cmd, scenario_path, tui_client, interactive)
    elif format == "claude":
        result_path = _run_scenario_claude(scenario, workspace, codex_home, hooks_config, checkpoint_cmd, scenario_path, tui_client, interactive)
    else:
        result_path = _run_scenario_codex(scenario, workspace, codex_home, hooks_config, checkpoint_cmd, scenario_path, tui_client, interactive)

    # Clean up TUI client
    if tui_client:
        tui_client.close()

    # Check expectations if present
    expect = scenario.get("expect", {})
    if expect:
        _check_expectations(expect, workspace)

    return result_path


def _run_scenario_fast_mode(scenario: Dict[str, Any], workspace: str, codex_home: str, format: str, hooks_config: Dict[str, Any], checkpoint_cmd: str = None, scenario_path: str = None, tui_client: TuiTestClient = None, interactive: bool = False) -> str:
    """Run scenario in fast mode: sort events by time and execute sequentially."""
    if format == "claude":
        recorder = ClaudeSessionRecorder(codex_home=codex_home, cwd=workspace)

        # Record initial user message if present in meta
        instructions = scenario.get("meta", {}).get("instructions")
        if instructions:
            recorder.record_user_message(instructions, is_meta=True)
    else:
        recorder = RolloutRecorder(codex_home=codex_home, cwd=workspace, instructions=scenario.get("meta",{}).get("instructions"))
        logger = SessionLogger(codex_home=codex_home)

        tc = scenario.get("meta",{}).get("turn_context", {
            "cwd": workspace,
            "approval_policy": "on_failure",
            "sandbox_policy": "workspace_write",
            "model": "mock-model",
            "effort": "medium",
            "summary": "concise"
        })
        recorder.record_turn_context(tc)
        logger.to_tui("insert_history", lines=1)

    # Collect all events with their timestamps
    timeline_events = []

    current_time = 0
    timeline = scenario.get("timeline", [])

    for step in timeline:
        # Handle llmResponse grouped events
        if "llmResponse" in step:
            llm_response_elements = step["llmResponse"]
            for element in llm_response_elements:
                if "think" in element:
                    # Think events: array of [ms, text] pairs
                    for ms, text in element["think"]:
                        timeline_events.append({
                            "time": current_time,
                            "type": "think",
                            "data": (ms, text),
                            "step": step
                        })
                        current_time += ms
                elif "assistant" in element:
                    # Assistant events: array of [ms, text] pairs
                    for ms, text in element["assistant"]:
                        timeline_events.append({
                            "time": current_time,
                            "type": "assistant",
                            "data": (ms, text),
                            "step": step
                        })
                        current_time += ms
                elif "agentToolUse" in element:
                    # Agent tool use within llmResponse
                    timeline_events.append({
                        "time": current_time,
                        "type": "agentToolUse",
                        "data": element["agentToolUse"],
                        "step": step
                    })
                elif "agentEdits" in element:
                    # Agent edits within llmResponse
                    timeline_events.append({
                        "time": current_time,
                        "type": "agentEdits",
                        "data": element["agentEdits"],
                        "step": step
                    })
                elif "error" in element:
                    # Error response
                    error_data = element["error"]
                    timeline_events.append({
                        "time": current_time,
                        "type": "error",
                        "data": error_data,
                        "step": step
                    })
                elif "toolResult" in element:
                    # Tool result (for multi-turn conversations)
                    timeline_events.append({
                        "time": current_time,
                        "type": "toolResult",
                        "data": element["toolResult"],
                        "step": step
                    })

        # Handle agentActions grouped events
        elif "agentActions" in step:
            agent_actions = step["agentActions"]
            for action in agent_actions:
                if "agentToolUse" in action:
                    timeline_events.append({
                        "time": current_time,
                        "type": "agentToolUse",
                        "data": action["agentToolUse"],
                        "step": step
                    })
                elif "agentEdits" in action:
                    timeline_events.append({
                        "time": current_time,
                        "type": "agentEdits",
                        "data": action["agentEdits"],
                        "step": step
                    })

        # Handle userActions grouped events
        elif "userActions" in step:
            user_actions = step["userActions"]
            for action in user_actions:
                if "userInputs" in action:
                    user_inputs = action["userInputs"]
                    target = action.get("target", "tui")
                    for ms, input_text in user_inputs:
                        timeline_events.append({
                            "time": current_time,
                            "type": "userInputs",
                            "data": (ms, input_text, target),
                            "step": step
                        })
                        current_time += ms
                elif "userEdits" in action:
                    timeline_events.append({
                        "time": current_time,
                        "type": "userEdits",
                        "data": action["userEdits"],
                        "step": step
                    })
                elif "userCommand" in action:
                    timeline_events.append({
                        "time": current_time,
                        "type": "userCommand",
                        "data": action["userCommand"],
                        "step": step
                    })

        # Handle legacy individual events for backward compatibility
        elif "think" in step and isinstance(step["think"], list):
            # Legacy format: think is array of [ms, text] pairs
            for ms, text in step["think"]:
                timeline_events.append({
                    "time": current_time,
                    "type": "think",
                    "data": (ms, text),
                    "step": step
                })
                current_time += ms
        elif "agentToolUse" in step:
            # Legacy individual agentToolUse
            timeline_events.append({
                "time": current_time,
                "type": "agentToolUse",
                "data": step["agentToolUse"],
                "step": step
            })
        elif "agentEdits" in step:
            # Legacy individual agentEdits
            timeline_events.append({
                "time": current_time,
                "type": "agentEdits",
                "data": step["agentEdits"],
                "step": step
            })
        elif "assistant" in step and isinstance(step["assistant"], list):
            # Legacy format: assistant is array of [ms, text] pairs
            for ms, text in step["assistant"]:
                timeline_events.append({
                    "time": current_time,
                    "type": "assistant",
                    "data": (ms, text),
                    "step": step
                })
                current_time += ms

        # Handle tool-specific events (mapped to agentToolUse internally)
        elif any(tool in step for tool in ["runCmd", "grep", "readFile", "listDir", "find", "sed", "editFile", "writeFile", "task", "webFetch", "webSearch", "todoWrite", "notebookEdit", "exitPlanMode", "bashOutput", "killShell", "slashCommand"]):
            tool_name = next((tool for tool in ["runCmd", "grep", "readFile", "listDir", "find", "sed", "editFile", "writeFile", "task", "webFetch", "webSearch", "todoWrite", "notebookEdit", "exitPlanMode", "bashOutput", "killShell", "slashCommand"] if tool in step), None)
            if tool_name:
                timeline_events.append({
                    "time": current_time,
                    "type": "agentToolUse",
                    "data": {"toolName": tool_name, "args": step[tool_name]},
                    "step": step
                })

        # Handle control events
        elif "advanceMs" in step:
            ms = step["advanceMs"]
            current_time += ms
        elif "screenshot" in step:
            timeline_events.append({
                "time": current_time,
                "type": "screenshot",
                "data": step["screenshot"],
                "step": step
            })
        elif "assert" in step:
            timeline_events.append({
                "time": current_time,
                "type": "assert",
                "data": step["assert"],
                "step": step
            })
        elif "complete" in step:
            timeline_events.append({
                "time": current_time,
                "type": "complete",
                "data": step.get("complete", {}),
                "step": step
            })
        elif "merge" in step:
            timeline_events.append({
                "time": current_time,
                "type": "merge",
                "data": step.get("merge", {}),
                "step": step
            })

        # Handle legacy user input events
        elif "userInputs" in step:
            user_inputs = step["userInputs"]
            target = step.get("target", "tui")
            for ms, input_text in user_inputs:
                timeline_events.append({
                    "time": current_time,
                    "type": "userInputs",
                    "data": (ms, input_text, target),
                    "step": step
                })
                current_time += ms
        elif "userEdits" in step:
            timeline_events.append({
                "time": current_time,
                "type": "userEdits",
                "data": step["userEdits"],
                "step": step
            })
        elif "userCommand" in step:
            timeline_events.append({
                "time": current_time,
                "type": "userCommand",
                "data": step["userCommand"],
                "step": step
            })

    # Sort events by time
    timeline_events.sort(key=lambda x: x["time"])

    # Execute events in order (no timing delays)
    for event in timeline_events:
        if event["type"] == "think":
            ms, text = event["data"]
            _rich_output.print_thinking(text)
            if format == "claude":
                recorder.record_assistant_message(f"I need to think about this: {text}")
            else:
                recorder.record_reasoning(summary_text=text)
                recorder.record_event("agent_message", {"message": text, "id": f"msg_{uuid.uuid4().hex[:6]}"})
        elif event["type"] == "agentToolUse":
            tool_use = event["data"]
            tool_name = tool_use["toolName"]
            tool_args = tool_use.get("args", {})
            call_id = f"call_{uuid.uuid4().hex[:8]}"

            if format == "claude":
                tool_call_id = recorder.record_assistant_tool_use(tool_name, tool_args)
            else:
                recorder.record_function_call(name=tool_name, arguments=_as_json(tool_args), call_id=call_id)

            # Use rich output for tool execution
            _rich_output.print_tool_start(tool_name, tool_args)

            # Simulate progress if provided
            progress_events = tool_use.get("progress", [])
            for progress_ms, progress_msg in progress_events:
                _print_trace("tool_progress", progress_msg)

            try:
                # Start spinner for tool execution
                _rich_output.start_spinner(f"Executing {tool_name}...")

                # Pass display callback for runCmd to enable real-time output
                if tool_name == "runCmd":
                    tool_args = tool_args.copy()
                    tool_args["display_callback"] = _display_command_output
                result = call_tool(tool_name, workspace, **tool_args)
                status = tool_use.get("status", "ok")

                # Stop spinner
                _rich_output.stop_spinner()

                # Show result with rich formatting
                _rich_output.print_tool_result(tool_name, result, status)

                if format == "claude":
                    # Create tool result data based on the tool type
                    tool_result_data = _create_tool_result_data(tool_name, result, tool_args)
                    recorder.record_tool_result(tool_call_id, str(result), is_error=False, tool_result_data=tool_result_data)
                else:
                    recorder.record_event("agent_message", {"message": f"tool {tool_name} {status}", "id": call_id})

                # Execute PostToolUse hooks
                hook_input = {
                    "tool_name": tool_name,
                    "tool_input": tool_args,
                    "tool_response": {"success": True, "result": result}
                }
                _execute_hooks(hooks_config, "PostToolUse", hook_input, workspace)

                # Execute checkpoint command
                _execute_checkpoint_cmd(checkpoint_cmd, workspace)

            except ToolError as e:
                # Stop spinner on error
                _rich_output.stop_spinner()

                # Show error with rich formatting
                _rich_output.print_error(str(e), f"{tool_name} Error")

                if format == "claude":
                    recorder.record_tool_result(tool_call_id, str(e), is_error=True, tool_result_data=f"Error: {e}")
                else:
                    recorder.record_event("agent_message", {"message": f"tool {tool_name} error: {e}", "id": call_id})

                # Execute PostToolUse hooks for failed tools too
                hook_input = {
                    "tool_name": tool_name,
                    "tool_input": tool_args,
                    "tool_response": {"success": False, "error": str(e)}
                }
                _execute_hooks(hooks_config, "PostToolUse", hook_input, workspace)

                # Execute checkpoint command for failed tools too
                _execute_checkpoint_cmd(checkpoint_cmd, workspace)
        elif event["type"] == "agentEdits":
            edits = event["data"]
            path = edits["path"]
            lines_added = edits.get("linesAdded", 0)
            lines_removed = edits.get("linesRemoved", 0)
            _rich_output.print_file_edit(path, lines_added, lines_removed)

            # Execute checkpoint command for file edits
            _execute_checkpoint_cmd(checkpoint_cmd, workspace)
        elif event["type"] == "assistant":
            ms, text = event["data"]
            _print_trace("assistant", text)  # This uses the enhanced _print_trace
            if format == "claude":
                recorder.record_assistant_message(text)
            else:
                recorder.record_message("assistant", text)
        elif event["type"] == "screenshot":
            label = event["data"]
            _print_trace("screenshot", f"[FAST] Capturing screenshot: {label}")
            if tui_client:
                success = tui_client.request_screenshot(label)
                if success:
                    _print_trace("screenshot", f"[FAST] Screenshot '{label}' captured successfully")
                else:
                    _print_trace("screenshot", f"[FAST] Failed to capture screenshot '{label}'")
        elif event["type"] == "assert":
            assertion = event["data"]
            if not _run_assertions(assertion, workspace):
                _print_trace("assert", "[FAST] Some assertions failed")
        elif event["type"] == "error":
            error_data = event["data"]
            error_type = error_data.get("errorType", "unknown_error")
            message = error_data.get("message", "An error occurred")
            status_code = error_data.get("statusCode", 400)
            _print_trace("error", f"Error ({error_type}, {status_code}): {message}")
            if format == "claude":
                recorder.record_assistant_message(f"Error: {message}")
            else:
                recorder.record_message("assistant", f"Error: {message}")
        elif event["type"] == "toolResult":
            tool_result_data = event["data"]
            tool_call_id = tool_result_data.get("toolCallId", "")
            content = tool_result_data.get("content", "")
            is_error = tool_result_data.get("is_error", False)
            _print_trace("tool_result", f"Tool result for {tool_call_id}: {content} (error: {is_error})")
            # Tool results are typically handled by the recorder in the tool execution above
        elif event["type"] == "complete":
            complete_data = event["data"]
            _rich_output.print_success(f"Scenario completed successfully")
        elif event["type"] == "merge":
            merge_data = event["data"]
            _print_trace("merge", f"Scenario merged: {merge_data}")
        elif event["type"] == "userInputs":
            ms, input_text, target = event["data"]
            if interactive:
                # Interactive mode: prompt for actual user input
                _rich_output.print_user_input_prompt(input_text)
                try:
                    user_response = input().strip()
                    _print_trace("user_input", f"[{target}] User provided: {repr(user_response)}")
                    _rich_output.print_success(f"Received input: {user_response}")
                except (EOFError, KeyboardInterrupt):
                    _print_trace("user_input", f"[{target}] Input cancelled")
                    user_response = ""
            else:
                # Non-interactive mode: just log the expected input
                _print_trace("user_input", f"[{target}] {repr(input_text)} (non-interactive)")
        elif event["type"] == "userEdits":
            user_edit = event["data"]
            patch_path = user_edit["patch"]
            _print_trace("user_edit", f"[FAST] Applying user edit patch: {patch_path}")

            # Apply the patch file
            full_patch_path = os.path.join(os.path.dirname(os.path.abspath(scenario_path)), patch_path)
            try:
                result = subprocess.run(["git", "apply", full_patch_path], cwd=workspace,
                                      capture_output=True, text=True)
                if result.returncode != 0:
                    with open(full_patch_path, 'r') as f:
                        patch_content = f.read()
                    result = subprocess.run(["patch", "-p1"], cwd=workspace,
                                          input=patch_content, capture_output=True, text=True)
                    if result.returncode != 0:
                        raise subprocess.CalledProcessError(result.returncode, "patch", result.stderr)
                _print_trace("user_edit", f"[FAST] Successfully applied patch: {patch_path}")
            except subprocess.CalledProcessError as e:
                _print_trace("user_edit", f"[FAST] Failed to apply patch {patch_path}: {e}")
            except FileNotFoundError:
                _print_trace("user_edit", f"[FAST] Patch file not found: {full_patch_path}")
        elif event["type"] == "userCommand":
            user_cmd = event["data"]
            cmd = user_cmd["cmd"]
            cwd = user_cmd.get("cwd", ".")
            _print_trace("user_command", f"[FAST] Executing user command: {cmd} (cwd: {cwd})")

            try:
                result = subprocess.run(cmd, shell=True, cwd=os.path.join(workspace, cwd),
                                      capture_output=True, text=True, timeout=30)
                if result.stdout:
                    _print_trace("user_command", f"[FAST] stdout: {result.stdout.strip()}")
                if result.stderr:
                    _print_trace("user_command", f"[FAST] stderr: {result.stderr.strip()}")
                _print_trace("user_command", f"[FAST] exit code: {result.returncode}")
            except subprocess.TimeoutExpired:
                _print_trace("user_command", f"[FAST] Command timeout: {cmd}")
            except Exception as e:
                _print_trace("user_command", f"[FAST] Command failed: {cmd} - {e}")

    # Flush recorders
    recorder.flush()
    if format == "claude":
        recorder.close()
        return recorder.session_path
    else:
        logger.close()
        return recorder.rollout_path

def _run_scenario_codex(scenario: Dict[str, Any], workspace: str, codex_home: str, hooks_config: Dict[str, Any], checkpoint_cmd: str = None, scenario_path: str = None, tui_client: TuiTestClient = None, interactive: bool = False) -> str:
    """Run scenario using Codex format."""
    recorder = RolloutRecorder(codex_home=codex_home, cwd=workspace, instructions=scenario.get("meta",{}).get("instructions"))
    logger = SessionLogger(codex_home=codex_home)

    tc = scenario.get("meta",{}).get("turn_context", {
        "cwd": workspace,
        "approval_policy": "on_failure",
        "sandbox_policy": "workspace_write",
        "model": "mock-model",
        "effort": "medium",
        "summary": "concise"
    })
    recorder.record_turn_context(tc)
    logger.to_tui("insert_history", lines=1)

    timeline = scenario["timeline"]
    for step in timeline:
        # Handle llmResponse grouped events
        if "llmResponse" in step:
            llm_response_elements = step["llmResponse"]
            for element in llm_response_elements:
                if "think" in element:
                    # Think events: array of [ms, text] pairs
                    for ms, text in element["think"]:
                        _rich_output.print_thinking(text)
                        _rich_output.start_spinner("Thinking...")
                        time.sleep(ms / 1000.0)  # Sleep for the specified milliseconds
                        _rich_output.stop_spinner()
                        recorder.record_reasoning(summary_text=text)
                        recorder.record_event("agent_message", {"message": text, "id": f"msg_{uuid.uuid4().hex[:6]}"})
                elif "assistant" in element:
                    # Assistant events: array of [ms, text] pairs
                    for ms, text in element["assistant"]:
                        _print_trace("assistant", text)  # Uses enhanced _print_trace
                        time.sleep(ms / 1000.0)
                        recorder.record_message("assistant", text)
                elif "agentToolUse" in element:
                    # Agent tool use within llmResponse
                    tool_use = element["agentToolUse"]
                    tool_name = tool_use["toolName"]
                    tool_args = tool_use.get("args", {})
                    call_id = f"call_{uuid.uuid4().hex[:8]}"

                    recorder.record_function_call(name=tool_name, arguments=_as_json(tool_args), call_id=call_id)
                    _rich_output.print_tool_start(tool_name, tool_args)

                    # Simulate progress if provided
                    progress_events = tool_use.get("progress", [])
                    for progress_ms, progress_msg in progress_events:
                        _print_trace("tool_progress", progress_msg)
                        time.sleep(progress_ms / 1000.0)

                    try:
                        _rich_output.start_spinner(f"Executing {tool_name}...")

                        # Pass display callback for runCmd to enable real-time output
                        if tool_name == "runCmd":
                            tool_args = tool_args.copy()
                            tool_args["display_callback"] = _display_command_output
                        result = call_tool(tool_name, workspace, **tool_args)
                        status = tool_use.get("status", "ok")
                        expected_result = tool_use.get("result")

                        _rich_output.stop_spinner()
                        _rich_output.print_tool_result(tool_name, result, status)

                        # Validate result if expected result is provided
                        if expected_result is not None and str(result) != str(expected_result):
                            _print_trace("tool", f"{tool_name} -> result mismatch. Expected: {expected_result}, Got: {result}")

                        recorder.record_event("agent_message", {"message": f"tool {tool_name} {status}", "id": call_id})

                        # Execute PostToolUse hooks
                        hook_input = {
                            "tool_name": tool_name,
                            "tool_input": tool_args,
                            "tool_response": {"success": True, "result": result}
                        }
                        _execute_hooks(hooks_config, "PostToolUse", hook_input, workspace)

                        # Execute checkpoint command
                        _execute_checkpoint_cmd(checkpoint_cmd, workspace)

                    except ToolError as e:
                        _rich_output.stop_spinner()
                        _rich_output.print_error(str(e), f"{tool_name} Error")
                        recorder.record_event("agent_message", {"message": f"tool {tool_name} error: {e}", "id": call_id})

                        # Execute PostToolUse hooks for failed tools too
                        hook_input = {
                            "tool_name": tool_name,
                            "tool_input": tool_args,
                            "tool_response": {"success": False, "error": str(e)}
                        }
                        _execute_hooks(hooks_config, "PostToolUse", hook_input, workspace)

                        # Execute checkpoint command for failed tools too
                        _execute_checkpoint_cmd(checkpoint_cmd, workspace)
                elif "agentEdits" in element:
                    # Agent edits within llmResponse
                    edits = element["agentEdits"]
                    path = edits["path"]
                    lines_added = edits.get("linesAdded", 0)
                    lines_removed = edits.get("linesRemoved", 0)
                    _rich_output.print_file_edit(path, lines_added, lines_removed)

                    # Execute checkpoint command for file edits
                    _execute_checkpoint_cmd(checkpoint_cmd, workspace)
                elif "error" in element:
                    # Error response
                    error_data = element["error"]
                    error_type = error_data.get("errorType", "unknown_error")
                    message = error_data.get("message", "An error occurred")
                    status_code = error_data.get("statusCode", 400)
                    _print_trace("error", f"Error ({error_type}, {status_code}): {message}")
                    recorder.record_message("assistant", f"Error: {message}")
                elif "toolResult" in element:
                    # Tool result (for multi-turn conversations)
                    tool_result_data = element["toolResult"]
                    tool_call_id = tool_result_data.get("toolCallId", "")
                    content = tool_result_data.get("content", "")
                    is_error = tool_result_data.get("is_error", False)
                    _print_trace("tool_result", f"Tool result for {tool_call_id}: {content} (error: {is_error})")

        # Handle legacy individual events for backward compatibility
        elif "think" in step and isinstance(step["think"], list):
            # Legacy format: think is array of [ms, text] pairs
            for ms, text in step["think"]:
                _rich_output.print_thinking(text)
                _rich_output.start_spinner("Thinking...")
                time.sleep(ms / 1000.0)  # Sleep for the specified milliseconds
                _rich_output.stop_spinner()
                recorder.record_reasoning(summary_text=text)
                recorder.record_event("agent_message", {"message": text, "id": f"msg_{uuid.uuid4().hex[:6]}"})
        elif "agentToolUse" in step:
            tool_use = step["agentToolUse"]
            tool_name = tool_use["toolName"]
            tool_args = tool_use.get("args", {})
            call_id = f"call_{uuid.uuid4().hex[:8]}"

            # Record the tool call
            recorder.record_function_call(name=tool_name, arguments=_as_json(tool_args), call_id=call_id)
            _rich_output.print_tool_start(tool_name, tool_args)

            # Simulate progress if provided
            progress_events = tool_use.get("progress", [])
            for progress_ms, progress_msg in progress_events:
                _print_trace("tool_progress", progress_msg)
                time.sleep(progress_ms / 1000.0)

            try:
                _rich_output.start_spinner(f"Executing {tool_name}...")
                result = call_tool(tool_name, workspace, **tool_args)
                status = tool_use.get("status", "ok")
                expected_result = tool_use.get("result")

                _rich_output.stop_spinner()
                _rich_output.print_tool_result(tool_name, result, status)

                # Validate result if expected result is provided
                if expected_result is not None and str(result) != str(expected_result):
                    _print_trace("tool", f"{tool_name} -> result mismatch. Expected: {expected_result}, Got: {result}")

                recorder.record_event("agent_message", {"message": f"tool {tool_name} {status}", "id": call_id})

                # Execute PostToolUse hooks
                hook_input = {
                    "tool_name": tool_name,
                    "tool_input": tool_args,
                    "tool_response": {"success": True, "result": result}
                }
                _execute_hooks(hooks_config, "PostToolUse", hook_input, workspace)

                # Execute checkpoint command
                _execute_checkpoint_cmd(checkpoint_cmd, workspace)

            except ToolError as e:
                _rich_output.stop_spinner()
                _rich_output.print_error(str(e), f"{tool_name} Error")
                recorder.record_event("agent_message", {"message": f"tool {tool_name} error: {e}", "id": call_id})

                # Execute PostToolUse hooks for failed tools too
                hook_input = {
                    "tool_name": tool_name,
                    "tool_input": tool_args,
                    "tool_response": {"success": False, "error": str(e)}
                }
                _execute_hooks(hooks_config, "PostToolUse", hook_input, workspace)

                # Execute checkpoint command for failed tools too
                _execute_checkpoint_cmd(checkpoint_cmd, workspace)
        elif "agentEdits" in step:
            edits = step["agentEdits"]
            path = edits["path"]
            lines_added = edits.get("linesAdded", 0)
            lines_removed = edits.get("linesRemoved", 0)
            _rich_output.print_file_edit(path, lines_added, lines_removed)

            # Execute checkpoint command for file edits
            _execute_checkpoint_cmd(checkpoint_cmd, workspace)
        elif "assistant" in step and isinstance(step["assistant"], list):
            # Legacy format: assistant is array of [ms, text] pairs
            for ms, text in step["assistant"]:
                _print_trace("assistant", text)  # Uses enhanced _print_trace
                time.sleep(ms / 1000.0)  # Sleep for the specified milliseconds
            recorder.record_message("assistant", text)
        elif "advanceMs" in step:
            ms = step["advanceMs"]
            _print_trace("timing", f"Advancing timeline by {ms}ms")
            time.sleep(ms / 1000.0)  # Actually sleep for the specified milliseconds
        elif "screenshot" in step:
            label = step["screenshot"]
            _print_trace("screenshot", f"Capturing screenshot: {label}")
            if tui_client:
                success = tui_client.request_screenshot(label)
                if success:
                    _print_trace("screenshot", f"Screenshot '{label}' captured successfully")
                else:
                    _print_trace("screenshot", f"Failed to capture screenshot '{label}'")
            # In a real implementation, this would capture and store a screenshot
        elif "assert" in step:
            assertion = step["assert"]
            if not _run_assertions(assertion, workspace):
                _print_trace("assert", "Some assertions failed")
        elif "userInputs" in step:
            user_inputs = step["userInputs"]
            target = step.get("target", "tui")  # target is at the step level
            for ms, input_text in user_inputs:
                if interactive:
                    # Interactive mode: prompt for actual user input
                    _rich_output.print_user_input_prompt(input_text)
                    try:
                        user_response = input().strip()
                        _print_trace("user_input", f"[{target}] User provided: {repr(user_response)}")
                        _rich_output.print_success(f"Received input: {user_response}")
                    except (EOFError, KeyboardInterrupt):
                        _print_trace("user_input", f"[{target}] Input cancelled")
                        user_response = ""
                else:
                    # Non-interactive mode: just log the expected input
                    _print_trace("user_input", f"[{target}] [{ms}ms] {repr(input_text)} (non-interactive)")
                time.sleep(ms / 1000.0)  # Sleep for the specified milliseconds
        elif "userEdits" in step:
            user_edit = step["userEdits"]
            patch_path = user_edit["patch"]
            _print_trace("user_edit", f"Applying user edit patch: {patch_path}")

            # Apply the patch file
            scenario_dir = os.path.dirname(os.path.abspath(scenario_path))
            full_patch_path = os.path.join(scenario_dir, patch_path)
            try:
                # Use git apply if available, otherwise try patch command
                result = subprocess.run(["git", "apply", full_patch_path], cwd=workspace,
                                      capture_output=True, text=True)
                if result.returncode != 0:
                    # Try patch command as fallback
                    with open(full_patch_path, 'r') as f:
                        patch_content = f.read()
                    result = subprocess.run(["patch", "-p1"], cwd=workspace,
                                          input=patch_content, capture_output=True, text=True)
                    if result.returncode != 0:
                        raise subprocess.CalledProcessError(result.returncode, "patch", result.stderr)

                _print_trace("user_edit", f"Successfully applied patch: {patch_path}")
            except subprocess.CalledProcessError as e:
                _print_trace("user_edit", f"Failed to apply patch {patch_path}: {e}")
            except FileNotFoundError:
                _print_trace("user_edit", f"Patch file not found: {full_patch_path}")
        elif "userCommand" in step:
            user_cmd = step["userCommand"]
            cmd = user_cmd["cmd"]
            cwd = user_cmd.get("cwd", ".")
            _print_trace("user_command", f"Executing user command: {cmd} (cwd: {cwd})")

            try:
                result = subprocess.run(cmd, shell=True, cwd=os.path.join(workspace, cwd),
                                      capture_output=True, text=True, timeout=30)
                if result.stdout:
                    _print_trace("user_command", f"stdout: {result.stdout.strip()}")
                if result.stderr:
                    _print_trace("user_command", f"stderr: {result.stderr.strip()}")
                _print_trace("user_command", f"exit code: {result.returncode}")
            except subprocess.TimeoutExpired:
                _print_trace("user_command", f"Command timeout: {cmd}")
            except Exception as e:
                _print_trace("user_command", f"Command failed: {cmd} - {e}")

        else:
            _print_trace("warn", f"Unknown step: {step}")
    recorder.flush()
    logger.close()
    return recorder.rollout_path


def _run_scenario_claude(scenario: Dict[str, Any], workspace: str, codex_home: str, hooks_config: Dict[str, Any], checkpoint_cmd: str = None, scenario_path: str = None, tui_client: TuiTestClient = None, interactive: bool = False) -> str:
    """Run scenario using Claude format."""
    recorder = ClaudeSessionRecorder(codex_home=codex_home, cwd=workspace)

    # Record initial user message if present in meta
    instructions = scenario.get("meta", {}).get("instructions")
    if instructions:
        recorder.record_user_message(instructions, is_meta=True)

    timeline = scenario["timeline"]
    for step in timeline:
        # Handle llmResponse grouped events
        if "llmResponse" in step:
            llm_response_elements = step["llmResponse"]
            for element in llm_response_elements:
                if "think" in element:
                    # Think events: array of [ms, text] pairs
                    for ms, text in element["think"]:
                        _rich_output.print_thinking(text)
                        _rich_output.start_spinner("Thinking...")
                        time.sleep(ms / 1000.0)  # Sleep for the specified milliseconds
                        _rich_output.stop_spinner()
                        recorder.record_assistant_message(f"I need to think about this: {text}")
                elif "assistant" in element:
                    # Assistant events: array of [ms, text] pairs
                    for ms, text in element["assistant"]:
                        _print_trace("assistant", text)  # Uses enhanced _print_trace
                        time.sleep(ms / 1000.0)
                        recorder.record_assistant_message(text)
                elif "agentToolUse" in element:
                    # Agent tool use within llmResponse
                    tool_use = element["agentToolUse"]
                    tool_name = tool_use["toolName"]
                    tool_args = tool_use.get("args", {})

                    # Record tool use
                    tool_call_id = recorder.record_assistant_tool_use(tool_name, tool_args)
                    _rich_output.print_tool_start(tool_name, tool_args)

                    # Simulate progress if provided
                    progress_events = tool_use.get("progress", [])
                    for progress_ms, progress_msg in progress_events:
                        _print_trace("tool_progress", progress_msg)
                        time.sleep(progress_ms / 1000.0)

                    try:
                        _rich_output.start_spinner(f"Executing {tool_name}...")

                        # Pass display callback for runCmd to enable real-time output
                        if tool_name == "runCmd":
                            tool_args = tool_args.copy()
                            tool_args["display_callback"] = _display_command_output
                        result = call_tool(tool_name, workspace, **tool_args)
                        status = tool_use.get("status", "ok")
                        expected_result = tool_use.get("result")

                        _rich_output.stop_spinner()
                        _rich_output.print_tool_result(tool_name, result, status)

                        # Validate result if expected result is provided
                        if expected_result is not None and str(result) != str(expected_result):
                            _print_trace("tool", f"{tool_name} -> result mismatch. Expected: {expected_result}, Got: {result}")

                        # Create tool result data based on the tool type
                        tool_result_data = _create_tool_result_data(tool_name, result, tool_args)
                        recorder.record_tool_result(tool_call_id, str(result), is_error=False, tool_result_data=tool_result_data)

                        # Execute PostToolUse hooks
                        hook_input = {
                            "tool_name": tool_name,
                            "tool_input": tool_args,
                            "tool_response": {"success": True, "result": result}
                        }
                        _execute_hooks(hooks_config, "PostToolUse", hook_input, workspace)

                        # Execute checkpoint command
                        _execute_checkpoint_cmd(checkpoint_cmd, workspace)

                    except ToolError as e:
                        _rich_output.stop_spinner()
                        _rich_output.print_error(str(e), f"{tool_name} Error")
                        recorder.record_tool_result(tool_call_id, str(e), is_error=True, tool_result_data=f"Error: {e}")

                        # Execute PostToolUse hooks for failed tools too
                        hook_input = {
                            "tool_name": tool_name,
                            "tool_input": tool_args,
                            "tool_response": {"success": False, "error": str(e)}
                        }
                        _execute_hooks(hooks_config, "PostToolUse", hook_input, workspace)

                        # Execute checkpoint command for failed tools too
                        _execute_checkpoint_cmd(checkpoint_cmd, workspace)
                elif "agentEdits" in element:
                    # Agent edits within llmResponse
                    edits = element["agentEdits"]
                    path = edits["path"]
                    lines_added = edits.get("linesAdded", 0)
                    lines_removed = edits.get("linesRemoved", 0)
                    _rich_output.print_file_edit(path, lines_added, lines_removed)

                    # Execute checkpoint command for file edits
                    _execute_checkpoint_cmd(checkpoint_cmd, workspace)
                elif "error" in element:
                    # Error response
                    error_data = element["error"]
                    error_type = error_data.get("errorType", "unknown_error")
                    message = error_data.get("message", "An error occurred")
                    status_code = error_data.get("statusCode", 400)
                    _print_trace("error", f"Error ({error_type}, {status_code}): {message}")
                    recorder.record_assistant_message(f"Error: {message}")
                elif "toolResult" in element:
                    # Tool result (for multi-turn conversations)
                    tool_result_data = element["toolResult"]
                    tool_call_id = tool_result_data.get("toolCallId", "")
                    content = tool_result_data.get("content", "")
                    is_error = tool_result_data.get("is_error", False)
                    _print_trace("tool_result", f"Tool result for {tool_call_id}: {content} (error: {is_error})")

        # Handle legacy individual events for backward compatibility
        elif "think" in step and isinstance(step["think"], list):
            # Legacy format: think is array of [ms, text] pairs
            for ms, text in step["think"]:
                _rich_output.print_thinking(text)
                _rich_output.start_spinner("Thinking...")
                time.sleep(ms / 1000.0)  # Sleep for the specified milliseconds
                _rich_output.stop_spinner()
                recorder.record_assistant_message(f"I need to think about this: {text}")
        elif "agentToolUse" in step:
            tool_use = step["agentToolUse"]
            tool_name = tool_use["toolName"]
            tool_args = tool_use.get("args", {})

            # Record tool use
            tool_call_id = recorder.record_assistant_tool_use(tool_name, tool_args)
            _print_trace("tool", f"{tool_name}({tool_args}) -> executing")

            # Simulate progress if provided
            progress_events = tool_use.get("progress", [])
            for progress_ms, progress_msg in progress_events:
                _print_trace("tool_progress", f"[{progress_ms}ms] {progress_msg}")

            try:
                result = call_tool(tool_name, workspace, **tool_args)
                status = tool_use.get("status", "ok")
                expected_result = tool_use.get("result")

                # Validate result if expected result is provided
                if expected_result is not None and str(result) != str(expected_result):
                    _print_trace("tool", f"{tool_name} -> result mismatch. Expected: {expected_result}, Got: {result}")
                else:
                    _print_trace("tool", f"{tool_name} -> {status} {result}")

                # Create tool result data based on the tool type
                tool_result_data = _create_tool_result_data(tool_name, result, tool_args)
                recorder.record_tool_result(tool_call_id, str(result), is_error=False, tool_result_data=tool_result_data)

                # Execute PostToolUse hooks
                hook_input = {
                    "tool_name": tool_name,
                    "tool_input": tool_args,
                    "tool_response": {"success": True, "result": result}
                }
                _execute_hooks(hooks_config, "PostToolUse", hook_input, workspace)

                # Execute checkpoint command
                _execute_checkpoint_cmd(checkpoint_cmd, workspace)

            except ToolError as e:
                _print_trace("tool", f"{tool_name} -> error {e}")
                recorder.record_tool_result(tool_call_id, str(e), is_error=True, tool_result_data=f"Error: {e}")

                # Execute PostToolUse hooks for failed tools too
                hook_input = {
                    "tool_name": tool_name,
                    "tool_input": tool_args,
                    "tool_response": {"success": False, "error": str(e)}
                }
                _execute_hooks(hooks_config, "PostToolUse", hook_input, workspace)

                # Execute checkpoint command for failed tools too
                _execute_checkpoint_cmd(checkpoint_cmd, workspace)
        elif "agentEdits" in step:
            edits = step["agentEdits"]
            path = edits["path"]
            lines_added = edits.get("linesAdded", 0)
            lines_removed = edits.get("linesRemoved", 0)
            _print_trace("agent_edits", f"{path}: +{lines_added} -{lines_removed} lines")

            # Execute checkpoint command for file edits
            _execute_checkpoint_cmd(checkpoint_cmd, workspace)
        elif "assistant" in step and isinstance(step["assistant"], list):
            # New format: assistant is array of [ms, text] pairs
            for ms, text in step["assistant"]:
                _print_trace("assistant", f"[{ms}ms] {text}")
                time.sleep(ms / 1000.0)  # Sleep for the specified milliseconds
            recorder.record_assistant_message(text)
        elif "advanceMs" in step:
            ms = step["advanceMs"]
            _print_trace("timing", f"Advancing timeline by {ms}ms")
            time.sleep(ms / 1000.0)  # Actually sleep for the specified milliseconds
        elif "screenshot" in step:
            label = step["screenshot"]
            _print_trace("screenshot", f"Capturing screenshot: {label}")
            if tui_client:
                success = tui_client.request_screenshot(label)
                if success:
                    _print_trace("screenshot", f"Screenshot '{label}' captured successfully")
                else:
                    _print_trace("screenshot", f"Failed to capture screenshot '{label}'")
            # In a real implementation, this would capture and store a screenshot
        elif "assert" in step:
            assertion = step["assert"]
            if not _run_assertions(assertion, workspace):
                _print_trace("assert", "Some assertions failed")
        elif "userInputs" in step:
            user_inputs = step["userInputs"]
            target = step.get("target", "tui")  # target is at the step level
            for ms, input_text in user_inputs:
                if interactive:
                    # Interactive mode: prompt for actual user input
                    _rich_output.print_user_input_prompt(input_text)
                    try:
                        user_response = input().strip()
                        _print_trace("user_input", f"[{target}] User provided: {repr(user_response)}")
                        _rich_output.print_success(f"Received input: {user_response}")
                    except (EOFError, KeyboardInterrupt):
                        _print_trace("user_input", f"[{target}] Input cancelled")
                        user_response = ""
                else:
                    # Non-interactive mode: just log the expected input
                    _print_trace("user_input", f"[{target}] [{ms}ms] {repr(input_text)} (non-interactive)")
                time.sleep(ms / 1000.0)  # Sleep for the specified milliseconds
        elif "userEdits" in step:
            user_edit = step["userEdits"]
            patch_path = user_edit["patch"]
            _print_trace("user_edit", f"Applying user edit patch: {patch_path}")

            # Apply the patch file
            scenario_dir = os.path.dirname(os.path.abspath(scenario_path))
            full_patch_path = os.path.join(scenario_dir, patch_path)
            try:
                # Use git apply if available, otherwise try patch command
                result = subprocess.run(["git", "apply", full_patch_path], cwd=workspace,
                                      capture_output=True, text=True)
                if result.returncode != 0:
                    # Try patch command as fallback
                    with open(full_patch_path, 'r') as f:
                        patch_content = f.read()
                    result = subprocess.run(["patch", "-p1"], cwd=workspace,
                                          input=patch_content, capture_output=True, text=True)
                    if result.returncode != 0:
                        raise subprocess.CalledProcessError(result.returncode, "patch", result.stderr)

                _print_trace("user_edit", f"Successfully applied patch: {patch_path}")
            except subprocess.CalledProcessError as e:
                _print_trace("user_edit", f"Failed to apply patch {patch_path}: {e}")
            except FileNotFoundError:
                _print_trace("user_edit", f"Patch file not found: {full_patch_path}")
        elif "userCommand" in step:
            user_cmd = step["userCommand"]
            cmd = user_cmd["cmd"]
            cwd = user_cmd.get("cwd", ".")
            _print_trace("user_command", f"Executing user command: {cmd} (cwd: {cwd})")

            try:
                result = subprocess.run(cmd, shell=True, cwd=os.path.join(workspace, cwd),
                                      capture_output=True, text=True, timeout=30)
                if result.stdout:
                    _print_trace("user_command", f"stdout: {result.stdout.strip()}")
                if result.stderr:
                    _print_trace("user_command", f"stderr: {result.stderr.strip()}")
                _print_trace("user_command", f"exit code: {result.returncode}")
            except subprocess.TimeoutExpired:
                _print_trace("user_command", f"Command timeout: {cmd}")
            except Exception as e:
                _print_trace("user_command", f"Command failed: {cmd} - {e}")

        else:
            _print_trace("warn", f"Unknown step: {step}")

    recorder.flush()
    recorder.close()
    return recorder.session_path


def _create_tool_result_data(tool_name: str, result: Any, args: Dict[str, Any]) -> Any:
    """Create appropriate tool result data based on tool type."""
    if tool_name == "write_file":
        return {
            "type": "text",
            "file": {
                "filePath": args.get("path", "unknown"),
                "content": args.get("text", ""),
                "numLines": len(str(args.get("text", "")).split("\n")),
                "startLine": 1,
                "totalLines": len(str(args.get("text", "")).split("\n"))
            }
        }
    elif tool_name == "read_file":
        return {
            "type": "text",
            "file": {
                "filePath": args.get("path", "unknown"),
                "content": str(result),
                "numLines": len(str(result).split("\n")) if result else 0,
                "startLine": 1,
                "totalLines": len(str(result).split("\n")) if result else 0
            }
        }
    elif tool_name in ["append_file", "replace_in_file"]:
        return {"path": args.get("path", "unknown"), "operation": tool_name}
    else:
        # Generic result for other tools
        return str(result) if result else "Operation completed"

def demo_scenario(workspace: str) -> Dict[str, Any]:
    return {
        "name": "demo_scenario",
    "repo": {
        "init": True
    },
        "timeline": [
            {
                "think": [
                    [500, "Analyzing the user's request"],
                    [300, "I need to create a Python file"]
                ]
            },
            {
                "agentToolUse": {
                    "toolName": "write_file",
                    "args": { "path": "hello.py", "text": "print('Hello, World!')\n" },
                    "result": "File created",
                    "status": "ok"
                }
            },
            {
                "assistant": [
                    [200, "Created hello.py. Run with: python hello.py"]
                ]
            },
            {
                "agentToolUse": {
                    "toolName": "read_file",
                    "args": { "path": "hello.py" },
                    "result": "File read successfully",
                    "status": "ok"
                }
            },
            {
                "assistant": [
                    [300, "Confirmed content of hello.py."]
                ]
            }
        ],
        "expect": {
            "exitCode": 0,
            "artifacts": [
                { "type": "taskFile", "pattern": "hello.py" }
            ]
        }
    }
