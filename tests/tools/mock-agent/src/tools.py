# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

import os
import re
from typing import Dict, Any, Tuple

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

REGISTRY = {
    "read_file": read_file,
    "write_file": write_file,
    "append_file": append_file,
    "replace_text": replace_text,
    "list_dir": list_dir,
}

def call_tool(name: str, workspace: str, **kwargs) -> Dict[str, Any]:
    if name not in REGISTRY:
        raise ToolError(f"Unknown tool: {name}")
    return REGISTRY[name](workspace, **kwargs)
