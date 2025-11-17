#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
PJDFSTEST_DIR="$REPO_ROOT/resources/pjdfstest"

show_help() {
  cat <<EOF
Usage: $0 [OPTIONS] <mountpoint> [test-paths...]

Run pjdfstest suite against a filesystem (auto-mounts if not already mounted).

Arguments:
  mountpoint        Mount point for the filesystem
  test-paths        Optional: Specific test paths (categories, files, or patterns)
                    If not specified, runs all tests

Options:
  -h, --help        Show this help message
  -q, --quiet       Suppress verbose output (run prove without -v)
  -l, --list        List all available test categories
  --all             Run all available test categories
  --auto-mount      Force mount even if already mounted (with --no-mount-check)
  --no-mount-check  Skip all mount validation (advanced usage)
  --auto-setup      Check if pjdfstest is set up and set it up if needed
  --build-binaries  Build FUSE test binaries before auto-mounting

Behavior:
  - Auto-mounts the filesystem if mountpoint is not already mounted
  - Uses existing mount if mountpoint is already a mount point
  - Pre-mount for multiple tests to avoid mounting overhead

Examples:
  $0 /tmp/agentfs                          # Auto-mount and run all tests
  $0 /tmp/agentfs unlink/                   # Auto-mount and run unlink category
  $0 /tmp/agentfs unlink/00.t               # Auto-mount and run specific test
  $0 /tmp/agentfs --all                     # Auto-mount and run all categories
  $0 --list                                 # List available categories
  $0 --auto-setup --build-binaries /tmp/agentfs  # Full suite workflow
EOF
}

list_categories() {
  echo "Available pjdfstest categories:"
  if [ -d "$PJDFSTEST_DIR/tests" ]; then
    ls "$PJDFSTEST_DIR/tests" | grep -v misc.sh | sort
  else
    echo "pjdfstest not set up - run 'just setup-pjdfstest-suite' first"
  fi
}

# Parse arguments
VERBOSE=true
ALL_CATEGORIES=false
LIST_CATEGORIES=false
AUTO_MOUNT=false
NO_MOUNT_CHECK=false
AUTO_SETUP=false
BUILD_BINARIES=false

while [[ $# -gt 0 ]]; do
  case $1 in
  -h | --help)
    show_help
    exit 0
    ;;
  -q | --quiet)
    VERBOSE=false
    shift
    ;;
  -l | --list)
    LIST_CATEGORIES=true
    shift
    ;;
  --all)
    ALL_CATEGORIES=true
    shift
    ;;
  --auto-mount)
    AUTO_MOUNT=true
    shift
    ;;
  --no-mount-check)
    NO_MOUNT_CHECK=true
    shift
    ;;
  --auto-setup)
    AUTO_SETUP=true
    shift
    ;;
  --build-binaries)
    BUILD_BINARIES=true
    shift
    ;;
  -*)
    echo "Unknown option: $1" >&2
    echo "Use '$0 --help' for usage information." >&2
    exit 1
    ;;
  *)
    # First non-option argument is mountpoint
    if [[ -z "${mountpoint:-}" ]]; then
      mountpoint="$1"
    else
      # Subsequent arguments are test paths
      test_paths+=("$1")
    fi
    shift
    ;;
  esac
done

# Handle list option (doesn't require mountpoint)
if [[ "$LIST_CATEGORIES" == true ]]; then
  list_categories
  exit 0
fi

# Handle auto-setup
if [[ "$AUTO_SETUP" == true ]]; then
  if [ ! -d "$PJDFSTEST_DIR/tests" ]; then
    echo "Setting up pjdfstest suite..."
    just setup-pjdfstest-suite
  else
    echo "pjdfstest suite already set up, skipping setup..."
  fi
fi

# Validate mountpoint
if [[ -z "${mountpoint:-}" ]]; then
  echo "Error: mountpoint is required" >&2
  echo "Use '$0 --help' for usage information." >&2
  exit 1
fi

# Mount logic: auto-mount if not already mounted (unless --no-mount-check)
if [[ "$NO_MOUNT_CHECK" != true ]]; then
  if ! mountpoint -q "$mountpoint" 2>/dev/null; then
    # Not mounted - auto-mount
    echo "Auto-mounting AgentFS at $mountpoint..."

    # Store original directory for cleanup
    ORIGINAL_DIR="$(pwd)"
    # Ensure we unmount on exit (success or failure)
    cleanup() {
      echo "Cleaning up: unmounting $mountpoint..."
      cd "$ORIGINAL_DIR"
      just umount-fuse "$mountpoint" || true
    }
    trap cleanup EXIT

    # Build the FUSE test binaries if requested
    if [[ "$BUILD_BINARIES" == true ]]; then
      echo "Building FUSE test binaries..."
      just build-fuse-test-binaries >/dev/null 2>&1
    fi

    # Create mount point if it doesn't exist
    if [[ ! -d "$mountpoint" ]]; then
      echo "Creating mount point: $mountpoint"
      mkdir -p "$mountpoint" || {
        echo "Error: Failed to create $mountpoint" >&2
        exit 1
      }
    fi

    # Mount the filesystem with allow-other for root access
    AGENTFS_FUSE_ALLOW_OTHER=1 just mount-fuse "$mountpoint"

    # Give the mount a moment to settle
    sleep 1
  else
    # Already mounted - validate access
    echo "Using existing mount at $mountpoint"

    # Ensure we can access the mount (requires mounting with --allow-other)
    if ! ls "$mountpoint" >/dev/null 2>&1; then
      echo "Error: Unable to access $mountpoint. Mount may need 'AGENTFS_FUSE_ALLOW_OTHER=1'" >&2
      exit 1
    fi
  fi
elif [[ "$AUTO_MOUNT" == true ]]; then
  # Explicit --auto-mount with --no-mount-check (force mount regardless)
  ORIGINAL_DIR="$(pwd)"
  cleanup() {
    echo "Cleaning up: unmounting $mountpoint..."
    cd "$ORIGINAL_DIR"
    just umount-fuse "$mountpoint" || true
  }
  trap cleanup EXIT

  # Build the FUSE test binaries if requested
  if [[ "$BUILD_BINARIES" == true ]]; then
    echo "Building FUSE test binaries..."
    just build-fuse-test-binaries >/dev/null 2>&1
  fi

  # Mount the filesystem with allow-other for root access
  echo "Force-mounting AgentFS at $mountpoint..."
  AGENTFS_FUSE_ALLOW_OTHER=1 just mount-fuse "$mountpoint"

  # Give the mount a moment to settle
  sleep 1
fi

if [[ ! -d "$PJDFSTEST_DIR" ]]; then
  echo "Error: pjdfstest suite not set up. Run 'just setup-pjdfstest-suite' first" >&2
  exit 1
fi

# Build prove arguments
PROVE_ARGS=()
if [[ "$VERBOSE" == true ]]; then
  PROVE_ARGS+=("-v")
fi
PROVE_ARGS+=("-r")

# Determine which tests to run
if [[ "$ALL_CATEGORIES" == true ]]; then
  # Run all available categories
  echo "Running all pjdfstest categories against $mountpoint..."
  if [ -d "$PJDFSTEST_DIR/tests" ]; then
    for category in $(ls "$PJDFSTEST_DIR/tests" | grep -v misc.sh | sort); do
      PROVE_ARGS+=("$PJDFSTEST_DIR/tests/$category")
    done
  else
    echo "Error: pjdfstest not set up; run 'just setup-pjdfstest-suite'" >&2
    exit 1
  fi
elif [[ ${#test_paths[@]} -gt 0 ]]; then
  echo "Running pjdfstest against $mountpoint for specific tests: ${test_paths[*]}"
  for path in "${test_paths[@]}"; do
    PROVE_ARGS+=("$PJDFSTEST_DIR/tests/$path")
  done
else
  echo "Running full pjdfstest suite against $mountpoint..."
  echo "Note: This requires root privileges and may take a long time"
  echo "Press Ctrl+C to interrupt the test suite"
  PROVE_ARGS+=("$PJDFSTEST_DIR/tests")
fi

cd "$mountpoint"
if [[ $(id -u) -eq 0 ]]; then
  prove "${PROVE_ARGS[@]}"
else
  sudo -E prove "${PROVE_ARGS[@]}"
fi
