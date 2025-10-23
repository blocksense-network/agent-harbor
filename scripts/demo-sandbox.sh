#!/bin/bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

# Demonstration script for the sandbox functionality
# This shows how the sbx-helper binary would be used in practice

set -e

echo "=== Sandbox Demo ==="
echo

# Build the sandbox helper if not already built
if ! command -v ./target/debug/sbx-helper >/dev/null 2>&1; then
  echo "Building sbx-helper..."
  cargo build --bin sbx-helper
  echo
fi

echo "The sbx-helper binary provides command-line access to sandboxing functionality."
echo "It supports various options for controlling isolation levels:"
echo
echo "Available options:"
echo "  --debug              Enable debug logging"
echo "  --no-user-ns         Disable user namespace isolation"
echo "  --no-mount-ns        Disable mount namespace isolation"
echo "  --no-pid-ns          Disable PID namespace isolation"
echo "  --rw-dir DIR         Allow read-write access to directory DIR"
echo "  -C DIR               Set working directory"
echo
echo "Example usage:"
echo "  ./target/debug/sbx-helper echo 'Hello from sandbox!'"
echo "  ./target/debug/sbx-helper --debug --rw-dir /tmp /bin/ls -la"
echo

echo "Note: Privilege requirements depend on system configuration:"
echo "  - On systems allowing unprivileged user namespaces (Linux 3.8+), non-root users can"
echo "    create user namespaces and within them become 'root' to create other namespaces"
echo "  - On systems with kernel.unprivileged_userns_clone=0, root privileges are required"
echo "  - Mount operations require CAP_SYS_ADMIN (typically root)"
echo "In a test environment, operations will fail gracefully but demonstrate the structure."
echo

echo "The sandbox provides:"
echo "✅ User namespace isolation (maps current user to root in sandbox)"
echo "✅ Mount namespace isolation (separate filesystem view)"
echo "✅ PID namespace isolation (separate process tree)"
echo "✅ UTS namespace isolation (separate hostname)"
echo "✅ IPC namespace isolation (separate System V IPC)"
echo "✅ Read-only sealing of system directories (/etc, /usr, /bin, etc.)"
echo "✅ Read-write access to specified working directories"
echo "✅ Proper /proc mounting for PID namespace"
echo "✅ PID 1 process execution with correct signal handling"
echo

echo "=== Demo Complete ==="
