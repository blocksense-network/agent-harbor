# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

import os
import re
import subprocess
import glob
import threading
import time
from typing import Dict, Any, Tuple, List

try:
    from ptyprocess import PtyProcessUnicode
except ImportError:
    PtyProcessUnicode = None

class ToolError(Exception):
    pass

def _safe_join(root: str, path: str) -> str:
    new_path = os.path.normpath(os.path.join(root, path))
    root = os.path.abspath(root)
    if not os.path.commonpath([root, new_path]).startswith(root):
        raise ToolError(f"Unsafe path: {path}")
    return new_path

def read_file(workspace: str, path: str) -> Dict[str, Any]:
    abspath = _safe_join(workspace, path)
    with open(abspath, "r", encoding="utf-8") as f:
        data = f.read()
    return {"path": path, "content": data}

def write_file(workspace: str, path: str, text: str, mkdirs: bool = True) -> Dict[str, Any]:
    abspath = _safe_join(workspace, path)
    d = os.path.dirname(abspath)
    if mkdirs:
        os.makedirs(d, exist_ok=True)
    with open(abspath, "w", encoding="utf-8") as f:
        f.write(text)
    return {"path": path, "bytes": len(text)}

def append_file(workspace: str, path: str, text: str) -> Dict[str, Any]:
    abspath = _safe_join(workspace, path)
    with open(abspath, "a", encoding="utf-8") as f:
        f.write(text)
    return {"path": path, "appended": len(text)}

def replace_text(workspace: str, path: str, pattern: str, replacement: str, count: int = 0) -> Dict[str, Any]:
    abspath = _safe_join(workspace, path)
    with open(abspath, "r", encoding="utf-8") as f:
        data = f.read()
    new, n = re.subn(pattern, replacement, data, count=count, flags=re.MULTILINE)
    with open(abspath, "w", encoding="utf-8") as f:
        f.write(new)
    return {"path": path, "replaced": n}

def list_dir(workspace: str, path: str = ".") -> Dict[str, Any]:
    abspath = _safe_join(workspace, path)
    entries = []
    for name in sorted(os.listdir(abspath)):
        full = os.path.join(abspath, name)
        entries.append({
            "name": name,
            "is_dir": os.path.isdir(full),
            "size": os.path.getsize(full)
        })
    return {"path": path, "entries": entries}

def runCmd(workspace: str, command: str, cwd: str = ".", timeout: int = 30, description: str = "", run_in_background: bool = False, display_callback=None) -> Dict[str, Any]:
    """Execute a shell command."""
    cwd_abs = _safe_join(workspace, cwd)

    if run_in_background:
        # For background execution, just start the process and return immediately
        process = subprocess.Popen(command, shell=True, cwd=cwd_abs,
                                 stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        return {"command": command, "pid": process.pid, "background": True}

    # Use PTY for real-time output display if available and display_callback provided
    if PtyProcessUnicode and display_callback:
        return _run_cmd_with_pty(command, cwd_abs, timeout, display_callback)
    else:
        # Fall back to regular subprocess execution
        try:
            result = subprocess.run(command, shell=True, cwd=cwd_abs,
                                  capture_output=True, text=True, timeout=timeout)
            return {
                "command": command,
                "stdout": result.stdout,
                "stderr": result.stderr,
                "returncode": result.returncode,
                "success": result.returncode == 0
            }
        except subprocess.TimeoutExpired:
            raise ToolError(f"Command timed out after {timeout} seconds: {command}")

def _run_cmd_with_pty(command: str, cwd_abs: str, timeout: int, display_callback) -> Dict[str, Any]:
    """Execute command with PTY and display output in a frame."""
    try:
        # Start PTY process with shell to execute the command
        proc = PtyProcessUnicode.spawn(['sh', '-c', command], cwd=cwd_abs)

        # Wait for process to complete or timeout
        end_time = time.time() + timeout
        while proc.isalive() and time.time() < end_time:
            time.sleep(0.1)

        # Read all output at once
        try:
            output = proc.read()
            lines = output.replace('\r\n', '\n').replace('\r', '\n').split('\n')
            all_output_lines = [line.strip() for line in lines if line.strip()]
        except Exception as e:
            all_output_lines = []

        # Display the last 6 lines in a frame
        if all_output_lines:
            display_lines = all_output_lines[-6:] if len(all_output_lines) > 6 else all_output_lines
            display_callback(display_lines)

        # Ensure process is terminated
        if proc.isalive():
            try:
                proc.terminate()
                time.sleep(0.1)
                if proc.isalive():
                    proc.kill(9)
            except:
                pass

        returncode = proc.exitstatus if hasattr(proc, 'exitstatus') and proc.exitstatus is not None else 0

        # Combine all output for return value
        stdout = '\n'.join(all_output_lines)
        stderr = ""  # PTY combines stdout/stderr

        return {
            "command": command,
            "stdout": stdout,
            "stderr": stderr,
            "returncode": returncode,
            "success": returncode == 0
        }

    except Exception as e:
        raise ToolError(f"PTY execution failed: {e}")

def grep(workspace: str, pattern: str, path: str = ".", glob_pattern: str = "", output_mode: str = "content",
         before_context: int = 0, after_context: int = 0, context: int = 0,
         case_insensitive: bool = False, file_type: str = "", head_limit: int = 0, multiline: bool = False) -> Dict[str, Any]:
    """Search for patterns in files using grep-like functionality."""
    search_path = _safe_join(workspace, path)

    flags = []
    if case_insensitive:
        flags.append("-i")
    if multiline:
        flags.append("-U")  # Enable multiline mode in ripgrep
    if before_context > 0:
        flags.extend(["-B", str(before_context)])
    if after_context > 0:
        flags.extend(["-A", str(after_context)])
    if context > 0:
        flags.extend(["-C", str(context)])

    # Build the command
    cmd_parts = ["rg", "--line-number"]
    if flags:
        cmd_parts.extend(flags)
    if glob_pattern:
        cmd_parts.extend(["--glob", glob_pattern])
    if file_type:
        cmd_parts.extend(["--type", file_type])
    cmd_parts.extend([pattern, search_path])

    try:
        result = subprocess.run(cmd_parts, capture_output=True, text=True, timeout=30)
        lines = result.stdout.strip().split('\n') if result.stdout.strip() else []

        if output_mode == "files_with_matches":
            files = set()
            for line in lines:
                if ':' in line:
                    files.add(line.split(':')[0])
            return {"files": sorted(list(files)), "count": len(files)}
        elif output_mode == "count":
            return {"count": len(lines)}
        else:  # content mode
            if head_limit > 0:
                lines = lines[:head_limit]
            return {"matches": lines, "count": len(lines)}

    except FileNotFoundError:
        raise ToolError("ripgrep (rg) not found. Please install ripgrep to use the grep tool.")
    except subprocess.TimeoutExpired:
        raise ToolError("Grep search timed out")

def find(workspace: str, path: str = ".", name: str = "", file_type: str = "") -> Dict[str, Any]:
    """Find files by name pattern."""
    search_path = _safe_join(workspace, path)

    if file_type == "file":
        if name:
            pattern = f"**/{name}"
        else:
            pattern = "**/*"
        files = glob.glob(pattern, root_dir=search_path, recursive=True)
        return {"files": [f for f in files if os.path.isfile(os.path.join(search_path, f))]}
    elif file_type == "dir":
        if name:
            pattern = f"**/{name}"
        else:
            pattern = "**/*"
        files = glob.glob(pattern, root_dir=search_path, recursive=True)
        return {"dirs": [f for f in files if os.path.isdir(os.path.join(search_path, f))]}
    else:
        if name:
            pattern = f"**/{name}"
        else:
            pattern = "**/*"
        files = glob.glob(pattern, root_dir=search_path, recursive=True)
        result = {"files": [], "dirs": []}
        for f in files:
            full_path = os.path.join(search_path, f)
            if os.path.isfile(full_path):
                result["files"].append(f)
            elif os.path.isdir(full_path):
                result["dirs"].append(f)
        return result

def sed(workspace: str, expression: str, path: str, inplace: bool = False) -> Dict[str, Any]:
    """Stream editor operations."""
    abspath = _safe_join(workspace, path)

    # Parse sed expression (basic support for s/pattern/replacement/flags)
    if not expression.startswith('s/'):
        raise ToolError(f"Unsupported sed expression: {expression}")

    parts = expression.split('/')
    if len(parts) < 4:
        raise ToolError(f"Invalid sed expression: {expression}")

    pattern = parts[1]
    replacement = parts[2]
    flags = parts[3] if len(parts) > 3 else ""

    count = 0 if 'g' in flags else 1

    with open(abspath, "r", encoding="utf-8") as f:
        data = f.read()

    new_data, n = re.subn(pattern, replacement, data, count=count, flags=re.MULTILINE)

    if inplace:
        with open(abspath, "w", encoding="utf-8") as f:
            f.write(new_data)

    return {"path": path, "replaced": n, "inplace": inplace}

def editFile(workspace: str, path: str, old_string: str, new_string: str, replace_all: bool = False) -> Dict[str, Any]:
    """Edit file with exact string replacements."""
    abspath = _safe_join(workspace, path)

    with open(abspath, "r", encoding="utf-8") as f:
        data = f.read()

    if replace_all:
        new_data, count = re.subn(re.escape(old_string), new_string, data, flags=re.MULTILINE)
    else:
        new_data = data.replace(old_string, new_string, 1)
        count = 1 if old_string in data else 0

    with open(abspath, "w", encoding="utf-8") as f:
        f.write(new_data)

    return {"path": path, "replaced": count}

def writeFile(workspace: str, path: str, text: str) -> Dict[str, Any]:
    """Write content to file."""
    abspath = _safe_join(workspace, path)
    d = os.path.dirname(abspath)
    os.makedirs(d, exist_ok=True)

    with open(abspath, "w", encoding="utf-8") as f:
        f.write(text)

    return {"path": path, "bytes": len(text)}

# Stub implementations for more complex tools that would need external dependencies
def task(workspace: str, description: str, prompt: str, subagent_type: str = "mock") -> Dict[str, Any]:
    """Launch specialized agent for complex tasks."""
    # For mock agent, just return a placeholder response
    return {"task": description, "subagent": subagent_type, "status": "completed"}

def webFetch(workspace: str, url: str, prompt: str = "") -> Dict[str, Any]:
    """Fetch content from URL with AI analysis."""
    raise ToolError("webFetch tool requires HTTP client implementation")

def webSearch(workspace: str, query: str, allowed_domains: List[str] = None, blocked_domains: List[str] = None) -> Dict[str, Any]:
    """Search the web for information."""
    raise ToolError("webSearch tool requires search API implementation")

def todoWrite(workspace: str, todos: List[Dict[str, Any]]) -> Dict[str, Any]:
    """Manage structured task lists."""
    # For mock agent, just log the todos
    return {"todos_written": len(todos), "todos": todos}

def notebookEdit(workspace: str, notebook_path: str, cell_id: str = None, new_source: str = "", cell_type: str = "", edit_mode: str = "replace") -> Dict[str, Any]:
    """Edit Jupyter notebook cells."""
    raise ToolError("notebookEdit tool requires Jupyter notebook parsing implementation")

def exitPlanMode(workspace: str, plan: str) -> Dict[str, Any]:
    """Exit plan mode with summary."""
    return {"plan": plan, "exited_plan_mode": True}

def bashOutput(workspace: str, bash_id: str, filter: str = "") -> Dict[str, Any]:
    """Retrieve output from background bash shell."""
    raise ToolError("bashOutput tool requires bash session management")

def killShell(workspace: str, shell_id: str) -> Dict[str, Any]:
    """Kill running background bash shell."""
    raise ToolError("killShell tool requires process management")

def slashCommand(workspace: str, command: str) -> Dict[str, Any]:
    """Execute slash command."""
    # For mock agent, just return a placeholder response
    return {"slash_command": command, "executed": True}

REGISTRY = {
    # File operations (scenario format names)
    "readFile": read_file,
    "writeFile": writeFile,
    "editFile": editFile,

    # Legacy file operations (for backward compatibility)
    "read_file": read_file,
    "write_file": write_file,
    "append_file": append_file,
    "replace_text": replace_text,
    "list_dir": list_dir,

    # Shell and system operations
    "runCmd": runCmd,
    "sed": sed,

    # Search and find operations
    "grep": grep,
    "find": find,

    # Complex operations (stub implementations)
    "task": task,
    "webFetch": webFetch,
    "webSearch": webSearch,
    "todoWrite": todoWrite,
    "notebookEdit": notebookEdit,
    "exitPlanMode": exitPlanMode,
    "bashOutput": bashOutput,
    "killShell": killShell,
    "slashCommand": slashCommand,
}

def call_tool(name: str, workspace: str, **kwargs) -> Dict[str, Any]:
    if name not in REGISTRY:
        raise ToolError(f"Unknown tool: {name}")
    return REGISTRY[name](workspace, **kwargs)
