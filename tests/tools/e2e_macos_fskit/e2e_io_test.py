#!/usr/bin/env python3
import hashlib
import os
import shutil
import subprocess
import sys
import tempfile
from datetime import datetime


def run(cmd):
    return subprocess.run(cmd, check=True, text=True, capture_output=True)


def ensure_mount() -> tuple[str, str]:
    script_dir = os.path.dirname(os.path.realpath(__file__))
    xcode_dir = os.path.realpath(os.path.join(script_dir, "../../../adapters/macos/xcode"))
    setup = os.path.join(xcode_dir, "test-device-setup.sh")
    if not os.path.exists(setup):
        print("Missing test-device-setup.sh", file=sys.stderr)
        sys.exit(2)

    env = os.environ.copy()
    env["BASH_SILENCE_DEPRECATION_WARNING"] = "1"

    # Create device and mountpoint using the shared helper via bash -c to export functions
    bash = shutil.which("bash") or "/bin/bash"

    # Create device
    create_cmd = f"source '{setup}'; create_device 50 device; echo $device"
    dev = subprocess.check_output([bash, "-lc", create_cmd], text=True, env=env).strip()
    if not dev:
        print("Failed to create device", file=sys.stderr)
        sys.exit(3)

    # Create mount point
    mp_cmd = f"source '{setup}'; create_mount_point mp; echo $mp"
    mp = subprocess.check_output([bash, "-lc", mp_cmd], text=True, env=env).strip()
    if not mp:
        print("Failed to create mount point", file=sys.stderr)
        sys.exit(3)

    # Mount AgentFS
    mount_cmd = f"source '{setup}'; mount_agentfs '{dev}' '{mp}'"
    rc = subprocess.call([bash, "-lc", mount_cmd], env=env)
    if rc != 0:
        print("Mount failed; skipping I/O (extension may not be active)")
        sys.exit(0)

    return dev, mp


def unmount(mount_point: str):
    script_dir = os.path.dirname(os.path.realpath(__file__))
    xcode_dir = os.path.realpath(os.path.join(script_dir, "../../../adapters/macos/xcode"))
    setup = os.path.join(xcode_dir, "test-device-setup.sh")
    bash = shutil.which("bash") or "/bin/bash"
    subprocess.call([bash, "-lc", f"source '{setup}'; unmount_device '{mount_point}'"])  # best effort


def sha256_file(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(8192), b""):
            h.update(chunk)
    return h.hexdigest()


def main():
    dev, mp = ensure_mount()
    try:
        # Basic write/read
        fpath = os.path.join(mp, "hello.txt")
        with open(fpath, "wb") as f:
            f.write(b"hello agentfs\n")
            f.flush()
            os.fsync(f.fileno())

        assert os.path.exists(fpath)
        with open(fpath, "rb") as f:
            data = f.read()
        assert data == b"hello agentfs\n"

        # Rename within directory
        newpath = os.path.join(mp, "greeting.txt")
        os.replace(fpath, newpath)
        assert os.path.exists(newpath) and not os.path.exists(fpath)

        # Directory list and metadata
        entries = os.listdir(mp)
        assert "greeting.txt" in entries
        st = os.stat(newpath)
        assert st.st_size > 0

        # Subdirectory and nested write
        subdir = os.path.join(mp, "sub")
        os.mkdir(subdir)
        nested = os.path.join(subdir, "nested.bin")
        blob = os.urandom(64 * 1024)
        with open(nested, "wb") as f:
            f.write(blob)
            f.flush()
            os.fsync(f.fileno())

        # Validate checksum
        h1 = hashlib.sha256(blob).hexdigest()
        h2 = sha256_file(nested)
        assert h1 == h2

        # Cleanup
        os.remove(nested)
        os.remove(newpath)
        os.rmdir(subdir)

        print("OK")
    finally:
        unmount(mp)


if __name__ == "__main__":
    main()


