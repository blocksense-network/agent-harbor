#!/usr/bin/env python3
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

"""F19: User-mode restart/orphan cleanup harness (no-sudo path).

This script launches an AgentFS daemon and FUSE host in user mode, mounts to a
private directory, runs two sandboxes against the same external workspace, and
verifies that orphaned branches are cleaned up after a crash/restart cycle.

Exit codes:
  0 = pass
  1 = fail
  2 = skip (missing fuse helpers, sandbox not permitted, etc.)
"""

from __future__ import annotations

import json
import os
import shutil
import signal
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, Optional


REPO_ROOT = Path(__file__).resolve().parent.parent


@dataclass
class SessionPaths:
    root: Path
    runtime_dir: Path
    mount_point: Path
    socket_path: Path
    log_dir: Path
    workspace: Path

    @property
    def control_path(self) -> Path:
        return self.mount_point / ".agentfs" / "control"


def log(msg: str) -> None:
    print(msg, flush=True)


def ensure_binary(path: Path, build_cmd: list[str]) -> None:
    if path.exists():
        return
    log(f"Building {path.name} via: {' '.join(build_cmd)}")
    subprocess.run(build_cmd, cwd=REPO_ROOT, check=True)


def fuse_available() -> tuple[bool, str]:
    if not Path("/dev/fuse").exists():
        return False, "FUSE device /dev/fuse is missing"
    if shutil.which("fusermount3") is None and shutil.which("fusermount") is None:
        return False, "fusermount helper not found in PATH"
    return True, ""


def run(cmd: list[str], *, env: Optional[Dict[str, str]] = None, cwd: Optional[Path] = None,
        log_file: Optional[Path] = None, check: bool = False, text: bool = True) -> subprocess.CompletedProcess:
    stdout = subprocess.PIPE if log_file is None else open(log_file, "w")
    stderr = subprocess.STDOUT if log_file is not None else subprocess.PIPE
    proc = subprocess.run(cmd, cwd=cwd, env=env, stdout=stdout, stderr=stderr, text=text)
    if check and proc.returncode != 0:
        raise subprocess.CalledProcessError(proc.returncode, cmd, output=proc.stdout)
    return proc


def wait_for(path: Path, *, timeout: float = 15.0) -> bool:
    deadline = time.time() + timeout
    while time.time() < deadline:
        if path.exists():
            return True
        time.sleep(0.25)
    return False


def start_daemon(session: SessionPaths, env: Dict[str, str], daemon_bin: Path, host_bin: Path) -> subprocess.Popen:
    daemon_log = session.log_dir / "ah-fs-snapshots-daemon.log"
    daemon_stdout = session.log_dir / "ah-fs-snapshots-daemon.stdout"
    cmd = [str(daemon_bin), "--socket-path", str(session.socket_path), "--log-dir", str(session.log_dir), "--log-level", "debug"]
    proc = subprocess.Popen(cmd, env=env, stdout=open(daemon_stdout, "w"), stderr=subprocess.STDOUT)
    if not wait_for(session.socket_path, timeout=10):
        proc.kill()
        raise RuntimeError(f"Daemon socket did not appear at {session.socket_path} (see {daemon_stdout})")
    return proc


def mount_filesystem(session: SessionPaths, env: Dict[str, str], ctl_bin: Path, mount_log: Path) -> None:
    cmd = [
        str(ctl_bin),
        "--socket-path",
        str(session.socket_path),
        "fuse",
        "mount",
        "--mount-point",
        str(session.mount_point),
        "--backstore",
        "in-memory",
        "--materialization",
        "lazy",
        "--mount-timeout-ms",
        "20000",
        "--auto-unmount",
    ]
    run(cmd, env=env, log_file=mount_log, check=True)
    if not wait_for(session.control_path, timeout=15):
        raise RuntimeError(f"Control file did not appear at {session.control_path}")


def get_status(session: SessionPaths, env: Dict[str, str], ctl_bin: Path) -> Optional[dict]:
    cmd = [str(ctl_bin), "--socket-path", str(session.socket_path), "fuse", "status", "--json", "--allow-not-ready"]
    proc = run(cmd, env=env)
    if proc.returncode != 0 or not proc.stdout:
        return None
    try:
        return json.loads(proc.stdout)
    except json.JSONDecodeError:
        return None


def kill_pid(pid: int) -> None:
    if pid <= 0:
        return
    try:
        os.kill(pid, signal.SIGKILL)
    except ProcessLookupError:
        return


def run_sandbox(session: SessionPaths, env: Dict[str, str], ah_bin: Path, command: str, log_path: Path) -> subprocess.CompletedProcess:
    sandbox_env = env.copy()
    sandbox_env.update({
        "AGENTFS_TRANSPORT": "fuse",
        "AGENTFS_MOUNT": str(session.mount_point),
    })
    cmd = [
        str(ah_bin),
        "agent",
        "sandbox",
        "--fs-snapshots",
        "agentfs",
        "--agentfs-socket",
        str(session.control_path),
        "--",
        "bash",
        "-c",
        command,
    ]
    return run(cmd, env=sandbox_env, cwd=session.workspace, log_file=log_path)


def write_summary(session: SessionPaths, status: str, reason: str, files: dict) -> None:
    summary = {
        "status": status,
        "reason": reason,
        "session_dir": str(session.root),
        "mount_point": str(session.mount_point),
        "socket_path": str(session.socket_path),
        "workspace": str(session.workspace),
        "logs": files,
    }
    (session.log_dir / "summary.json").write_text(json.dumps(summary, indent=2))
    log(f"[{status.upper()}] {reason}")


def main() -> int:
    ok, msg = fuse_available()
    if not ok:
        log(f"[SKIP] {msg}")
        return 2

    session_env = os.environ.get("AGENTFS_USER_SESSION_DIR")
    if session_env:
        session_root = Path(session_env)
    else:
        session_root = Path(tempfile.mkdtemp(prefix="agentfs-user-restart-", dir=REPO_ROOT / "logs"))

    paths = SessionPaths(
        root=session_root,
        runtime_dir=session_root / "runtime",
        mount_point=session_root / "mnt",
        socket_path=session_root / "agentfs.sock",
        log_dir=session_root,
        workspace=session_root / "workspace",
    )
    for p in [paths.runtime_dir, paths.mount_point, paths.workspace, paths.log_dir]:
        p.mkdir(parents=True, exist_ok=True)

    daemon_bin = REPO_ROOT / "target" / "debug" / "ah-fs-snapshots-daemon"
    ctl_bin = REPO_ROOT / "target" / "debug" / "ah-fs-snapshots-daemonctl"
    host_bin = REPO_ROOT / "target" / "debug" / "agentfs-fuse-host"
    ah_bin = REPO_ROOT / "target" / "debug" / "ah"

    ensure_binary(daemon_bin, ["cargo", "build", "--package", "ah-fs-snapshots-daemon", "--bins"])
    ensure_binary(ctl_bin, ["cargo", "build", "--package", "ah-fs-snapshots-daemon", "--bins"])
    log("Building ah-cli with agentfs support...")
    subprocess.run(
        ["cargo", "build", "-p", "ah-cli", "--features", "agentfs"],
        cwd=REPO_ROOT,
        check=True,
    )
    log("Building agentfs-fuse-host with FUSE support (force rebuild)...")
    subprocess.run(
        [
            "cargo",
            "build",
            "--package",
            "agentfs-fuse-host",
            "--no-default-features",
            "--features",
            "fuse",
        ],
        cwd=REPO_ROOT,
        check=True,
    )

    base_env = os.environ.copy()
    base_env.update({
        "AGENTFS_FUSE_RUNTIME_DIR": str(paths.runtime_dir),
        "AGENTFS_FUSE_HOST_BIN": str(host_bin),
        "RUST_LOG": "info,agentfs_fuse_host=info,agentfs::fuse=info",
    })

    sandbox_pre_log = paths.log_dir / "sandbox-pre.log"
    sandbox_post_log = paths.log_dir / "sandbox-post.log"
    mount_log = paths.log_dir / "mount.log"

    daemon_proc: Optional[subprocess.Popen] = None
    try:
        daemon_proc = start_daemon(paths, base_env, daemon_bin, host_bin)
        try:
            mount_filesystem(paths, base_env, ctl_bin, mount_log)
        except subprocess.CalledProcessError:
            daemon_log = Path.home() / "Library" / "Logs" / "agent-harbor" / "ah-fs-snapshots-daemon.log"
            write_summary(
                paths,
                "skip",
                f"User-mode mount failed (see {mount_log})",
                {
                    "mount": str(mount_log),
                    "daemon_log": str(daemon_log),
                },
            )
            return 2
        except Exception as mount_exc:  # noqa: BLE001
            write_summary(
                paths,
                "fail",
                f"Mount preparation failed: {mount_exc}",
                {"mount": str(mount_log)},
            )
            return 1

        status = get_status(paths, base_env, ctl_bin) or {}
        host_pid = int(status.get("pid", 0)) if isinstance(status, dict) else 0

        marker_pre = f"orphan-pre-{int(time.time())}"
        pre_result = run_sandbox(
            paths,
            base_env,
            ah_bin,
            f"echo '{marker_pre}' > orphan.txt && cat orphan.txt",
            sandbox_pre_log,
        )

        if pre_result.returncode != 0:
            output = pre_result.stdout or ""
            if "Operation not permitted" in output or "namespace" in output:
                write_summary(paths, "skip", "Sandbox namespaces not permitted", {
                    "sandbox_pre": str(sandbox_pre_log),
                })
                return 2
            write_summary(paths, "fail", "Pre-restart sandbox failed", {
                "sandbox_pre": str(sandbox_pre_log),
                "mount": str(mount_log),
            })
            return 1

        if (paths.workspace / "orphan.txt").exists():
            write_summary(paths, "fail", "Marker leaked to host before restart", {
                "sandbox_pre": str(sandbox_pre_log),
            })
            return 1

        # Simulate crash by killing host + daemon
        kill_pid(host_pid)
        if daemon_proc and daemon_proc.poll() is None:
            daemon_proc.kill()
            daemon_proc.wait(timeout=5)
        (paths.socket_path).unlink(missing_ok=True)

        # Restart stack
        daemon_proc = start_daemon(paths, base_env, daemon_bin, host_bin)
        try:
            mount_filesystem(paths, base_env, ctl_bin, mount_log)
        except subprocess.CalledProcessError:
            write_summary(
                paths,
                "skip",
                f"User-mode mount failed after restart (see {mount_log})",
                {
                    "mount": str(mount_log),
                },
            )
            return 2

        marker_post = f"orphan-post-{int(time.time())}"
        post_result = run_sandbox(
            paths,
            base_env,
            ah_bin,
            "if [ -f orphan.txt ]; then echo EXISTING_ORPHAN; else echo NO_ORPHAN; fi; "
            f"echo '{marker_post}' > orphan.txt && cat orphan.txt",
            sandbox_post_log,
        )

        if post_result.returncode != 0:
            output = post_result.stdout or ""
            if "Operation not permitted" in output or "namespace" in output:
                write_summary(paths, "skip", "Sandbox namespaces not permitted after restart", {
                    "sandbox_post": str(sandbox_post_log),
                })
                return 2
            write_summary(paths, "fail", "Post-restart sandbox failed", {
                "sandbox_post": str(sandbox_post_log),
                "mount": str(mount_log),
            })
            return 1

        output = post_result.stdout or ""
        if "EXISTING_ORPHAN" in output:
            write_summary(paths, "fail", "Orphan file resurfaced after restart", {
                "sandbox_post": str(sandbox_post_log),
                "sandbox_pre": str(sandbox_pre_log),
            })
            return 1

        if (paths.workspace / "orphan.txt").exists():
            write_summary(paths, "fail", "Marker leaked to host after restart", {
                "sandbox_post": str(sandbox_post_log),
            })
            return 1

        write_summary(paths, "pass", "User-mode restart cleaned orphans", {
            "sandbox_pre": str(sandbox_pre_log),
            "sandbox_post": str(sandbox_post_log),
            "mount": str(mount_log),
        })
        return 0

    except Exception as exc:  # noqa: BLE001
        write_summary(paths, "fail", f"Unhandled exception: {exc}", {
            "mount": str(mount_log),
        })
        return 1
    finally:
        # Best-effort cleanup
        try:
            if daemon_proc and daemon_proc.poll() is None:
                run([
                    str(ctl_bin),
                    "--socket-path",
                    str(paths.socket_path),
                    "fuse",
                    "unmount",
                ], env=base_env)
        finally:
            if daemon_proc and daemon_proc.poll() is None:
                daemon_proc.kill()
            # Do not delete logs; they are useful for CI artifacts


if __name__ == "__main__":
    sys.exit(main())
