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
test_paths=()

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

# Privileged test detection and mounting functions
wait_for_mount_state() {
  local mount_path="$1"
  local expect="$2"
  local max_attempts=50
  local attempt=0
  while ((attempt < max_attempts)); do
    if mountpoint -q "$mount_path" 2>/dev/null; then
      if [[ "$expect" == "mounted" ]]; then
        return 0
      fi
    else
      if [[ "$expect" == "unmounted" ]]; then
        return 0
      fi
    fi
    sleep 0.1
    ((attempt += 1))
  done
  echo "Timed out waiting for $mount_path to become $expect" >&2
  return 1
}

mount_agentfs() {
  local privileged="$1"
  local log_path="${2:-}"

  # Check if already mounted
  if mountpoint -q "$mountpoint" 2>/dev/null; then
    echo "Using existing mount at $mountpoint"
    return 0
  fi

  if [[ -n "$log_path" ]]; then
    export AGENTFS_FUSE_LOG_FILE="$log_path"
  else
    unset AGENTFS_FUSE_LOG_FILE
  fi
  export RUST_BACKTRACE="${RUST_BACKTRACE:-1}"
  if [[ "$privileged" == "sudo" ]]; then
    if [[ -n "${AGENTFS_FUSE_LOG_FILE:-}" ]]; then
      sudo env AGENTFS_FUSE_ALLOW_OTHER=1 AGENTFS_FUSE_LOG_FILE="$AGENTFS_FUSE_LOG_FILE" \
        AGENTFS_FUSE_PRIVILEGED=1 just mount-fuse "$mountpoint" >/dev/null 2>&1
    else
      sudo env AGENTFS_FUSE_ALLOW_OTHER=1 AGENTFS_FUSE_PRIVILEGED=1 just mount-fuse "$mountpoint" >/dev/null 2>&1
    fi
  else
    AGENTFS_FUSE_ALLOW_OTHER=1 just mount-fuse "$mountpoint" >/dev/null 2>&1
  fi
  if ! wait_for_mount_state "$mountpoint" "mounted"; then
    echo "Failed to verify mount at $mountpoint; aborting pjdfstest run." >&2
    exit 1
  fi
  mounted_by_us=true
}

unmount_agentfs() {
  local privileged="$1"
  if mountpoint -q "$mountpoint" 2>/dev/null; then
    if [[ "$privileged" == "sudo" ]]; then
      sudo just umount-fuse "$mountpoint" >/dev/null 2>&1 || true
    else
      just umount-fuse "$mountpoint" >/dev/null 2>&1 || true
    fi
    wait_for_mount_state "$mountpoint" "unmounted"
  fi
}

detect_privileged_tests() {
  local test_paths=("$@")
  local sudo_test_list="${PJDFSTEST_SUDO_TESTS:-chmod/12.t}"
  local -a sudo_tests
  read -ra sudo_tests <<<"$sudo_test_list"
  declare -A sudo_lookup=()
  for test in "${sudo_tests[@]}"; do
    [[ -z "$test" ]] && continue
    sudo_lookup["$test"]=1
  done

  # Check if any of the requested test paths need privileged execution
  for test_path in "${test_paths[@]}"; do
    # Handle both file paths (like "chmod/12.t") and directory paths (like "chmod/")
    if [[ "$test_path" == *.t ]]; then
      # Direct file match
      if [[ -n "${sudo_lookup[$test_path]+x}" ]]; then
        return 0 # Found a privileged test
      fi
    else
      # Directory/category match - check if any files in this category need privileges
      local category="${test_path%/}" # Remove trailing slash
      for privileged_test in "${!sudo_lookup[@]}"; do
        if [[ "$privileged_test" == "$category/"* ]]; then
          return 0 # Found a privileged test in this category
        fi
      done
    fi
  done
  return 1 # No privileged tests found
}

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

# Build the FUSE test binaries if requested (do this early)
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

# Setup cleanup for any mounts we create
ORIGINAL_DIR="$(pwd)"
mounted_by_us=false
cleanup() {
  if [[ "$mounted_by_us" == true ]]; then
    echo "Cleaning up: unmounting $mountpoint..."
    cd "$ORIGINAL_DIR"
    just umount-fuse "$mountpoint" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

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
declare -a requested_test_paths=()
if [[ "$ALL_CATEGORIES" == true ]]; then
  # Run all available categories
  echo "Running all pjdfstest categories against $mountpoint..."
  if [ -d "$PJDFSTEST_DIR/tests" ]; then
    for category in $(ls "$PJDFSTEST_DIR/tests" | grep -v misc.sh | sort); do
      requested_test_paths+=("$category/")
    done
  else
    echo "Error: pjdfstest not set up; run 'just setup-pjdfstest-suite'" >&2
    exit 1
  fi
elif [[ ${#test_paths[@]} -gt 0 ]]; then
  echo "Running pjdfstest against $mountpoint for specific tests: ${test_paths[*]}"
  requested_test_paths=("${test_paths[@]}")
else
  echo "Running full pjdfstest suite against $mountpoint..."
  echo "Note: This requires root privileges and may take a long time"
  echo "Press Ctrl+C to interrupt the test suite"
  requested_test_paths=(".")
fi

# Check if any tests require privileged execution
if detect_privileged_tests "${requested_test_paths[@]}"; then
  echo "Detected privileged tests - using two-phase execution"

  # Phase 1: User mount for non-privileged tests
  if [[ "$NO_MOUNT_CHECK" != true ]]; then
    echo "Phase 1: Mounting AgentFS as user for non-privileged tests..."
    mount_agentfs "user"
  fi

  # Separate privileged and non-privileged tests
  sudo_test_list="${PJDFSTEST_SUDO_TESTS:-chmod/12.t}"
  read -ra sudo_tests <<<"$sudo_test_list"
  declare -A sudo_lookup=()
  for test in "${sudo_tests[@]}"; do
    [[ -z "$test" ]] && continue
    sudo_lookup["$test"]=1
  done

  non_privileged_prove_args=()
  privileged_prove_args=()

  for test_path in "${requested_test_paths[@]}"; do
    if [[ "$test_path" == "." ]]; then
      # Full suite - separate all tests
      mapfile -t all_tests < <(cd "$PJDFSTEST_DIR/tests" && find . -name '*.t' | sort)
      for rel in "${all_tests[@]}"; do
        rel="${rel#./}"
        if [[ -n "${sudo_lookup[$rel]+x}" ]]; then
          privileged_prove_args+=("$PJDFSTEST_DIR/tests/$rel")
        else
          non_privileged_prove_args+=("$PJDFSTEST_DIR/tests/$rel")
        fi
      done
    elif [[ "$test_path" == *.t ]]; then
      # Direct file
      if [[ -n "${sudo_lookup[$test_path]+x}" ]]; then
        privileged_prove_args+=("$PJDFSTEST_DIR/tests/$test_path")
      else
        non_privileged_prove_args+=("$PJDFSTEST_DIR/tests/$test_path")
      fi
    else
      # Category - check each file in category
      local category="${test_path%/}"
      mapfile -t category_tests < <(cd "$PJDFSTEST_DIR/tests/$category" && find . -name '*.t' | sort)
      for rel in "${category_tests[@]}"; do
        rel="${rel#./}"
        full_path="$category/$rel"
        if [[ -n "${sudo_lookup[$full_path]+x}" ]]; then
          privileged_prove_args+=("$PJDFSTEST_DIR/tests/$full_path")
        else
          non_privileged_prove_args+=("$PJDFSTEST_DIR/tests/$full_path")
        fi
      done
    fi
  done

  # Run non-privileged tests (if any)
  if [[ ${#non_privileged_prove_args[@]} -gt 0 ]]; then
    echo "Running non-privileged tests..."
    cd "$mountpoint"
    if [[ $(id -u) -eq 0 ]]; then
      prove "${non_privileged_prove_args[@]}"
    else
      sudo -E prove "${non_privileged_prove_args[@]}"
    fi
  fi

  # Phase 2: Privileged mount for privileged tests
  if [[ ${#privileged_prove_args[@]} -gt 0 ]]; then
    echo "Phase 2: Remounting AgentFS with privileges for privileged tests..."

    # Always unmount current mount (whether we created it or it was existing)
    was_mounted_by_us="$mounted_by_us"
    unmount_agentfs "user"
    mounted_by_us=false

    mount_agentfs "sudo"

    echo "Running privileged tests..."
    cd "$mountpoint"
    if [[ $(id -u) -eq 0 ]]; then
      prove "${privileged_prove_args[@]}"
    else
      sudo -E prove "${privileged_prove_args[@]}"
    fi

    # Clean up privileged mount
    unmount_agentfs "sudo"

    # If we originally mounted it ourselves, leave it unmounted
    # If it was already mounted, remount as user for consistency
    if [[ "$was_mounted_by_us" == false ]]; then
      echo "Restoring original user mount..."
      mount_agentfs "user"
    fi
  fi

else
  # Simple single-phase execution for non-privileged tests
  declare -a prove_args=()
  for test_path in "${requested_test_paths[@]}"; do
    if [[ "$test_path" == "." ]]; then
      prove_args+=("$PJDFSTEST_DIR/tests")
    else
      prove_args+=("$PJDFSTEST_DIR/tests/$test_path")
    fi
  done

  # Mount if needed (using existing logic)
  if [[ "$NO_MOUNT_CHECK" != true ]] && ! mountpoint -q "$mountpoint" 2>/dev/null; then
    echo "Mounting AgentFS for tests..."
    mount_agentfs "user"
  fi

  echo "Running tests..."
  cd "$mountpoint"
  if [[ $(id -u) -eq 0 ]]; then
    prove "${prove_args[@]}"
  else
    sudo -E prove "${prove_args[@]}"
  fi
fi
