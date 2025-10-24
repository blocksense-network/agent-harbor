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
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Optional, List


def find_repo_root() -> Path:
    """Find the repository root directory."""
    try:
        result = subprocess.run(
            ["git", "rev-parse", "--show-toplevel"],
            capture_output=True,
            text=True,
            check=True,
            cwd=os.getcwd()
        )
        return Path(result.stdout.strip())
    except (subprocess.CalledProcessError, FileNotFoundError):
        return Path(os.getcwd())


def wait_for_port(port: int, timeout: float = 12.0) -> bool:
    """Wait for a port to become available."""
    import socket
    start_time = time.time()
    while time.time() - start_time < timeout:
        try:
            with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
                sock.settimeout(0.1)
                result = sock.connect_ex(('127.0.0.1', port))
                if result == 0:
                    return True
        except:
            pass
        time.sleep(0.2)
    return False


def main():
    parser = argparse.ArgumentParser(
        prog='with-mitmproxy',
        description='Launch a program with all HTTP(S) traffic routed through mitmproxy and dump every request/response to files.'
    )

    parser.add_argument(
        '-p', '--port',
        type=int,
        default=int(os.environ.get('MITM_PORT', '8080')),
        help='Proxy port (default: 8080 or $MITM_PORT)'
    )

    parser.add_argument(
        '--out',
        type=str,
        default=os.environ.get('WITH_MITM_OUT', ''),
        help='Dump output directory'
    )

    parser.add_argument(
        '--confdir',
        type=str,
        default=os.environ.get('WITH_MITM_CONFDIR', ''),
        help='mitmproxy confdir (stores CA & state)'
    )

    parser.add_argument(
        '--no-proxy',
        type=str,
        default=os.environ.get('WITH_MITM_NO_PROXY', ''),
        help='Comma list for NO_PROXY (e.g. "localhost,127.0.0.1")'
    )

    parser.add_argument(
        'program',
        help='Program to run'
    )

    parser.add_argument(
        'args',
        nargs='*',
        help='Arguments for the program'
    )

    args = parser.parse_args()

    # Find repo root
    repo_root = find_repo_root()

    # Set defaults
    confdir = Path(args.confdir) if args.confdir else repo_root / '.mitmproxy' / 'state'
    if not args.out:
        timestamp = time.strftime('%Y-%m-%dT%H:%M:%S', time.localtime()).replace(':', '_')
        program_name = Path(args.program).name
        out_dir = repo_root / '.mitmproxy' / program_name / f'{timestamp}-event-logs'
    else:
        out_dir = Path(args.out)

    # Create directories
    confdir.mkdir(parents=True, exist_ok=True)
    out_dir.mkdir(parents=True, exist_ok=True)

    ca_pem = confdir / 'mitmproxy-ca-cert.pem'

    print(f"Starting mitmproxy on port {args.port}...")
    print(f"Dumping traffic to: {out_dir}")
    print(f"CA certificate will be at: {ca_pem}")

    # Start mitmdump with our addon
    mitmdump_cmd = [
        'mitmdump',
        '-p', str(args.port),
        '--set', f'confdir={confdir}',
        '-s', str(Path(__file__).parent / 'mitm_dump_addon.py'),
        '--quiet'
    ]

    env = os.environ.copy()
    env['MITM_DUMP_OUTDIR'] = str(out_dir)

    stderr_log = out_dir / 'mitmdump.stderr.log'

    try:
        with open(stderr_log, 'w') as stderr_file:
            mitmdump_proc = subprocess.Popen(
                mitmdump_cmd,
                env=env,
                stderr=stderr_file,
                stdout=stderr_file
            )

        # Wait for proxy to be ready
        if not wait_for_port(args.port):
            print(f"Error: mitmproxy failed to start (port {args.port} not open)", file=sys.stderr)
            mitmdump_proc.terminate()
            try:
                mitmdump_proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                mitmdump_proc.kill()
            return 1

        print(f"mitmproxy is ready. Launching: {' '.join([args.program] + args.args)}")

        # Prepare environment for the child process
        child_env = os.environ.copy()

        # Proxy environment variables
        proxy_url = f'http://127.0.0.1:{args.port}'
        child_env['HTTP_PROXY'] = proxy_url
        child_env['HTTPS_PROXY'] = proxy_url
        child_env['ALL_PROXY'] = proxy_url

        if args.no_proxy:
            child_env['NO_PROXY'] = args.no_proxy
            child_env['no_proxy'] = args.no_proxy

        # CA trust environment variables (if CA exists)
        if ca_pem.exists():
            child_env['SSL_CERT_FILE'] = str(ca_pem)
            child_env['REQUESTS_CA_BUNDLE'] = str(ca_pem)
            child_env['CURL_CA_BUNDLE'] = str(ca_pem)
            child_env['NODE_EXTRA_CA_CERTS'] = str(ca_pem)
            child_env['GIT_SSL_CAINFO'] = str(ca_pem)
            print("CA certificate found, TLS interception enabled.")
        else:
            print("CA certificate not found yet - it will be created on first HTTPS connection.")

        # Launch the target program
        try:
            result = subprocess.run(
                [args.program] + args.args,
                env=child_env
            )
            return result.returncode
        finally:
            # Clean up mitmproxy
            print("Shutting down mitmproxy...")
            mitmdump_proc.terminate()
            try:
                mitmdump_proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                print("Force killing mitmproxy...")
                mitmdump_proc.kill()
                mitmdump_proc.wait()

    except KeyboardInterrupt:
        print("\nInterrupted by user. Shutting down...")
        mitmdump_proc.terminate()
        try:
            mitmdump_proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            mitmdump_proc.kill()
        return 130
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        mitmdump_proc.terminate()
        try:
            mitmdump_proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            mitmdump_proc.kill()
        return 1


if __name__ == '__main__':
    sys.exit(main())
