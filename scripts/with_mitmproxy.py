#!/usr/bin/env python3
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

"""
with-mitmproxy: Run programs behind mitmproxy with full HTTP(S) traffic dumps

This script launches a headless mitmproxy that captures all HTTP(S) traffic
from the specified program and dumps request/response pairs to files.
"""

import argparse
import os
import shlex
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path


def parse_args():
    p = argparse.ArgumentParser(
        description="Run a program behind mitmproxy, dump all HTTP(S) traffic, and set universal CA envs."
    )
    p.add_argument("-p", "--port", default=os.getenv("MITM_PORT", "8080"))
    p.add_argument("--out", default=os.getenv("WITH_MITM_OUT"))
    p.add_argument("--confdir", default=os.getenv("WITH_MITM_CONFDIR"))
    p.add_argument("--no-proxy", default=os.getenv("WITH_MITM_NO_PROXY", ""))
    p.add_argument("program", help="Program to run")
    p.add_argument("args", nargs=argparse.REMAINDER)
    return p.parse_args()


def repo_root():
    try:
        r = subprocess.run(["git", "rev-parse", "--show-toplevel"], check=True, capture_output=True, text=True)
        return Path(r.stdout.strip())
    except Exception:
        return Path.cwd()


def run(cmd, **kw):
    return subprocess.run(cmd, **kw)


def check_port(port, tries=60, pause=0.2):
    import socket
    for _ in range(tries):
        with socket.socket() as s:
            s.settimeout(0.2)
            try:
                s.connect(("127.0.0.1", port))
                return True
            except Exception:
                time.sleep(pause)
    return False


def ensure_ca_exists(port: int, confdir: Path, outdir: Path):
    ca_pem = confdir / "mitmproxy-ca-cert.pem"
    # Quick path: already created.
    if ca_pem.exists():
        return ca_pem

    # Force a TLS handshake through the proxy so mitmproxy generates the CA & leaf.
    # We allow insecure (-k) here because we only care about triggering mitmproxy.
    # If curl isn't present, we skip; most devs have it.
    curl = shutil.which("curl")
    if curl:
        try:
            run([curl, "-sS", "-k",
                 "-x", f"http://127.0.0.1:{port}",
                 "https://example.com"],
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        except Exception:
            pass

    # Wait until the CA file appears.
    for _ in range(100):  # ~10s
        if ca_pem.exists():
            return ca_pem
        time.sleep(0.1)
    # As a fallback we still return the path; caller can decide to proceed.
    return ca_pem


def main():
    args = parse_args()
    root = repo_root()
    program_name = Path(args.program).name

    confdir = Path(args.confdir) if args.confdir else (root / ".mitmproxy" / "state")
    ts = time.strftime("%Y-%m-%dT%H_%M_%S")
    outdir = Path(args.out) if args.out else (root / ".mitmproxy" / program_name / f"{ts}-event-logs")
    confdir.mkdir(parents=True, exist_ok=True)
    outdir.mkdir(parents=True, exist_ok=True)

    addon_path = Path(__file__).with_name("mitm_dump_addon.py")
    if not addon_path.exists():
        print(f"ERROR: {addon_path} not found (put the addon next to this script).", file=sys.stderr)
        sys.exit(1)

    port = int(args.port)
    env = os.environ.copy()
    env["MITM_DUMP_OUTDIR"] = str(outdir)

    print(f"Starting mitmproxy on port {port}...")
    print(f"Dumping traffic to: {outdir}")
    ca_pem = confdir / "mitmproxy-ca-cert.pem"
    print(f"CA certificate will be at: {ca_pem}")

    mitmdump_bin = shutil.which("mitmdump") or "/usr/bin/mitmdump"
    # Start mitmdump (headless) with our addon
    mitm = subprocess.Popen(
        [mitmdump_bin, "-p", str(port), "--set", f"confdir={confdir}", "-s", str(addon_path), "--quiet"],
        stdout=open(outdir / "mitmdump.stdout.log", "wb"),
        stderr=open(outdir / "mitmdump.stderr.log", "wb"),
        env=env
    )

    def shutdown(_sig=None, _frm=None):
        try:
            mitm.terminate()
            try:
                mitm.wait(timeout=2)
            except subprocess.TimeoutExpired:
                mitm.kill()
        finally:
            pass

    signal.signal(signal.SIGINT, shutdown)
    signal.signal(signal.SIGTERM, shutdown)

    if not check_port(port):
        print(f"mitmproxy failed to open port {port}", file=sys.stderr)
        shutdown()
        sys.exit(1)

    # Warm-up: force CA generation and wait for it
    ca_path = ensure_ca_exists(port, confdir, outdir)
    if ca_path.exists():
        print("mitmproxy is ready. CA certificate present.")
        # Universal CA envs for the child:
        env["SSL_CERT_FILE"]       = str(ca_path)   # OpenSSL consumers
        env["REQUESTS_CA_BUNDLE"]  = str(ca_path)   # Python requests
        env["CURL_CA_BUNDLE"]      = str(ca_path)   # curl
        env["NODE_EXTRA_CA_CERTS"] = str(ca_path)   # Node
        env["GIT_SSL_CAINFO"]      = str(ca_path)   # git
    else:
        print("WARNING: CA certificate not found after warm-up; TLS may fail until it appears.", file=sys.stderr)

    # Proxy envs for the child only:
    env["HTTP_PROXY"]  = f"http://127.0.0.1:{port}"
    env["HTTPS_PROXY"] = env["HTTP_PROXY"]
    env["ALL_PROXY"]   = env["HTTP_PROXY"]
    if args.no_proxy:
        env["NO_PROXY"] = args.no_proxy
        env["no_proxy"] = args.no_proxy

    # Exec the target program
    print(f"Launching: {args.program} {' '.join(map(shlex.quote, args.args))}")
    try:
        proc = subprocess.Popen([args.program, *args.args], env=env)
        rc = proc.wait()
    finally:
        shutdown()

    sys.exit(rc)


if __name__ == "__main__":
    main()
