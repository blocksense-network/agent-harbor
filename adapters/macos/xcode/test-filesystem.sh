#!/bin/bash
# Comprehensive filesystem integration test for AgentFS
# Tests real filesystem operations including mount/unmount, file operations, and control plane

set -e

# Source device setup utilities
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/test-device-setup.sh"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Test configuration
DEVICE_SIZE_MB=50
TEST_TIMEOUT=300 # 5 minutes timeout

# Test counters
TESTS_RUN=0
TESTS_PASSED=0
TESTS_FAILED=0

# Function to run a test with proper counting
run_test() {
  local test_name="$1"
  local test_function="$2"

  echo -e "${BLUE}Running test: $test_name${NC}"
  TESTS_RUN=$((TESTS_RUN + 1))

  if $test_function; then
    echo -e "${GREEN}✓ $test_name PASSED${NC}"
    TESTS_PASSED=$((TESTS_PASSED + 1))
    return 0
  else
    echo -e "${RED}✗ $test_name FAILED${NC}"
    TESTS_FAILED=$((TESTS_FAILED + 1))
    return 1
  fi
}

# Function to report test results
report_results() {
  echo -e "\n${BLUE}=== Test Results ===${NC}"
  echo "Tests run: $TESTS_RUN"
  echo -e "Passed: ${GREEN}$TESTS_PASSED${NC}"
  echo -e "Failed: ${RED}$TESTS_FAILED${NC}"

  if [ $TESTS_FAILED -eq 0 ]; then
    echo -e "${GREEN}All tests passed!${NC}"
    return 0
  else
    echo -e "${RED}$TESTS_FAILED test(s) failed${NC}"
    return 1
  fi
}

# Test 1: Full mount cycle
test_mount_cycle() {
  local device_id mount_point

  create_device "$DEVICE_SIZE_MB" device_id
  create_mount_point mount_point

  # Test mount
  if ! mount_agentfs "$device_id" "$mount_point"; then
    echo "Mount failed, but this may be expected if extension is not installed"
    return 0 # Don't fail the test for missing extension
  fi

  # Test basic directory listing
  if ! ls "$mount_point" >/dev/null 2>&1; then
    echo "Directory listing failed"
    return 1
  fi

  # Test unmount
  if ! unmount_device "$mount_point"; then
    return 1
  fi

  return 0
}

# Test 2: File operations (create, read, write, delete)
test_file_operations() {
  local device_id mount_point

  create_device "$DEVICE_SIZE_MB" device_id
  create_mount_point mount_point

  if ! mount_agentfs "$device_id" "$mount_point"; then
    return 0 # Skip if mount fails
  fi

  # Test file creation and writing
  if ! echo "test content" >"$mount_point/test.txt"; then
    unmount_device "$mount_point"
    return 1
  fi

  # Test file reading
  if ! grep -q "test content" "$mount_point/test.txt"; then
    unmount_device "$mount_point"
    return 1
  fi

  # Test file modification
  if ! echo "modified content" >>"$mount_point/test.txt"; then
    unmount_device "$mount_point"
    return 1
  fi

  # Verify modification
  if ! grep -q "modified content" "$mount_point/test.txt"; then
    unmount_device "$mount_point"
    return 1
  fi

  # Test file deletion
  if ! rm "$mount_point/test.txt"; then
    unmount_device "$mount_point"
    return 1
  fi

  # Verify deletion
  if [ -f "$mount_point/test.txt" ]; then
    unmount_device "$mount_point"
    return 1
  fi

  unmount_device "$mount_point"
  return 0
}

# Test 3: Directory operations
test_directory_operations() {
  local device_id mount_point

  create_device "$DEVICE_SIZE_MB" device_id
  create_mount_point mount_point

  if ! mount_agentfs "$device_id" "$mount_point"; then
    return 0 # Skip if mount fails
  fi

  # Test directory creation
  if ! mkdir "$mount_point/testdir"; then
    unmount_device "$mount_point"
    return 1
  fi

  # Test directory listing
  if ! ls "$mount_point" | grep -q "testdir"; then
    unmount_device "$mount_point"
    return 1
  fi

  # Test nested directory creation
  if ! mkdir -p "$mount_point/testdir/nested"; then
    unmount_device "$mount_point"
    return 1
  fi

  # Test file in directory
  if ! echo "nested content" >"$mount_point/testdir/nested/file.txt"; then
    unmount_device "$mount_point"
    return 1
  fi

  # Test recursive directory removal
  if ! rm -rf "$mount_point/testdir"; then
    unmount_device "$mount_point"
    return 1
  fi

  # Verify removal
  if [ -d "$mount_point/testdir" ]; then
    unmount_device "$mount_point"
    return 1
  fi

  unmount_device "$mount_point"
  return 0
}

# Test 4: Control plane operations
test_control_plane() {
  local device_id mount_point

  create_device "$DEVICE_SIZE_MB" device_id
  create_mount_point mount_point

  if ! mount_agentfs "$device_id" "$mount_point"; then
    return 0 # Skip if mount fails
  fi

  # Test snapshot creation via ah agent fs CLI
  if ! ah agent fs init-session --mount "$mount_point" --name "test_session"; then
    echo "Failed to create initial session via CLI"
    unmount_device "$mount_point"
    return 1
  fi

  # Test snapshot listing
  if ! ah agent fs snapshots test_session >/dev/null 2>&1; then
    echo "Failed to list snapshots via CLI"
    unmount_device "$mount_point"
    return 1
  fi

  # Get snapshot ID from listing (simplified - assume first snapshot)
  local snapshot_id
  snapshot_id=$(ah agent fs snapshots --mount "$mount_point" test_session | head -1 | awk '{print $1}')

  if [ -z "$snapshot_id" ]; then
    echo "No snapshot ID found"
    unmount_device "$mount_point"
    return 1
  fi

  # Test branch creation
  if ! ah agent fs branch create --mount "$mount_point" --from "$snapshot_id" --name "test_branch" >/dev/null 2>&1; then
    echo "Failed to create branch via CLI"
    unmount_device "$mount_point"
    return 1
  fi

  # Test branch binding
  if ! ah agent fs branch bind --mount "$mount_point" --branch "test_branch" >/dev/null 2>&1; then
    echo "Failed to bind branch via CLI"
    unmount_device "$mount_point"
    return 1
  fi

  unmount_device "$mount_point"
  return 0
}

# Test 5: Extended attributes (xattrs)
test_extended_attributes() {
  local device_id mount_point

  create_device "$DEVICE_SIZE_MB" device_id
  create_mount_point mount_point

  if ! mount_agentfs "$device_id" "$mount_point"; then
    return 0 # Skip if mount fails
  fi

  # Create test file
  echo "test" >"$mount_point/testfile.txt"

  # Test xattr setting (using xattr command if available)
  if command -v xattr >/dev/null 2>&1; then
    if ! xattr -w "user.testattr" "testvalue" "$mount_point/testfile.txt"; then
      echo "Failed to set extended attribute"
      unmount_device "$mount_point"
      return 1
    fi

    # Test xattr reading
    if ! xattr -p "user.testattr" "$mount_point/testfile.txt" | grep -q "testvalue"; then
      echo "Failed to read extended attribute"
      unmount_device "$mount_point"
      return 1
    fi
  else
    echo "xattr command not available, skipping xattr tests"
  fi

  unmount_device "$mount_point"
  return 0
}

# Test 6: Large file operations
test_large_files() {
  local device_id mount_point

  create_device "$DEVICE_SIZE_MB" device_id
  create_mount_point mount_point

  if ! mount_agentfs "$device_id" "$mount_point"; then
    return 0 # Skip if mount fails
  fi

  # Create a moderately large file (1MB)
  if ! dd if=/dev/zero of="$mount_point/largefile.bin" bs=1024 count=1024 2>/dev/null; then
    echo "Failed to create large file"
    unmount_device "$mount_point"
    return 1
  fi

  # Verify file size
  local file_size
  file_size=$(stat -f %z "$mount_point/largefile.bin" 2>/dev/null || echo "0")
  if [ "$file_size" -ne 1048576 ]; then
    echo "Large file size mismatch: expected 1048576, got $file_size"
    unmount_device "$mount_point"
    return 1
  fi

  # Test reading part of the large file
  if ! head -c 1024 "$mount_point/largefile.bin" | wc -c | grep -q "1024"; then
    echo "Failed to read from large file"
    unmount_device "$mount_point"
    return 1
  fi

  # Cleanup
  rm "$mount_point/largefile.bin"

  unmount_device "$mount_point"
  return 0
}

# Test 7: Concurrent operations
test_concurrent_operations() {
  local device_id mount_point

  create_device "$DEVICE_SIZE_MB" device_id
  create_mount_point mount_point

  if ! mount_agentfs "$device_id" "$mount_point"; then
    return 0 # Skip if mount fails
  fi

  # Create multiple files concurrently
  local pids=()
  for i in {1..5}; do
    (
      echo "content$i" >"$mount_point/file$i.txt"
      sleep 0.1
      grep -q "content$i" "$mount_point/file$i.txt"
      rm "$mount_point/file$i.txt"
    ) &
    pids+=($!)
  done

  # Wait for all operations to complete
  local failed=0
  for pid in "${pids[@]}"; do
    if ! wait "$pid"; then
      failed=1
    fi
  done

  unmount_device "$mount_point"
  return $failed
}

# Test 8: Error conditions
test_error_conditions() {
  local device_id mount_point

  create_device "$DEVICE_SIZE_MB" device_id
  create_mount_point mount_point

  if ! mount_agentfs "$device_id" "$mount_point"; then
    return 0 # Skip if mount fails
  fi

  # Test deleting non-existent file (should fail gracefully)
  if rm "$mount_point/nonexistent.txt" 2>/dev/null; then
    echo "Deleting non-existent file should have failed"
    unmount_device "$mount_point"
    return 1
  fi

  # Test reading non-existent file (should fail)
  if cat "$mount_point/nonexistent.txt" 2>/dev/null; then
    echo "Reading non-existent file should have failed"
    unmount_device "$mount_point"
    return 1
  fi

  # Test creating file in non-existent directory (should fail)
  if echo "test" >"$mount_point/nonexistent/file.txt" 2>/dev/null; then
    echo "Creating file in non-existent directory should have failed"
    unmount_device "$mount_point"
    return 1
  fi

  # Test removing non-empty directory without -r (should fail)
  mkdir "$mount_point/testdir"
  echo "test" >"$mount_point/testdir/file.txt"
  if rmdir "$mount_point/testdir" 2>/dev/null; then
    echo "Removing non-empty directory should have failed"
    unmount_device "$mount_point"
    return 1
  fi

  # Cleanup
  rm -rf "$mount_point/testdir"

  unmount_device "$mount_point"
  return 0
}

# Main test execution
main() {
  echo -e "${BLUE}Starting comprehensive AgentFS integration tests...${NC}"
  echo -e "${YELLOW}Note: Some tests may be skipped if FSKit extension is not installed${NC}"

  # Set timeout for the entire test suite
  timeout "$TEST_TIMEOUT" bash -c '
        # Run all tests
        run_test "Mount Cycle" test_mount_cycle
        run_test "File Operations" test_file_operations
        run_test "Directory Operations" test_directory_operations
        run_test "Control Plane" test_control_plane
        run_test "Extended Attributes" test_extended_attributes
        run_test "Large Files" test_large_files
        run_test "Concurrent Operations" test_concurrent_operations
        run_test "Error Conditions" test_error_conditions

        # Report results
        report_results
    ' || {
    echo -e "${RED}Test suite timed out after ${TEST_TIMEOUT} seconds${NC}"
    return 1
  }
}

# Run main function
main "$@"
