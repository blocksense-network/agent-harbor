#!/bin/bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

# Stress testing and benchmarking script for AgentFS
# Performs performance benchmarks and stress tests on filesystem operations

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

# Benchmark configuration
LARGE_DEVICE_SIZE_MB=500
SMALL_DEVICE_SIZE_MB=50
BENCHMARK_ITERATIONS=5
STRESS_FILE_COUNT=1000
STRESS_DIR_DEPTH=5
CONCURRENT_WORKERS=4

# Results storage
BENCHMARK_RESULTS=()

# Function to measure execution time
time_operation() {
  local operation_name="$1"
  local command="$2"
  local start_time end_time duration

  echo -e "${BLUE}Benchmarking: $operation_name${NC}"

  start_time=$(date +%s.%N)
  if eval "$command"; then
    end_time=$(date +%s.%N)
    duration=$(echo "$end_time - $start_time" | bc 2>/dev/null || echo "0")
    echo -e "${GREEN}$operation_name completed in ${duration}s${NC}"
    BENCHMARK_RESULTS+=("$operation_name:$duration")
    return 0
  else
    echo -e "${RED}$operation_name failed${NC}"
    return 1
  fi
}

# Benchmark 1: File creation performance
benchmark_file_creation() {
  local device_id mount_point

  create_device "$SMALL_DEVICE_SIZE_MB" device_id
  create_mount_point mount_point

  if ! mount_agentfs "$device_id" "$mount_point"; then
    echo "Skipping file creation benchmark - mount failed"
    return 0
  fi

  # Test creating many small files
  time_operation "Create 100 small files" "
        for i in {1..100}; do
            echo \"content\$i\" > \"$mount_point/file\$i.txt\"
        done
    "

  # Cleanup
  rm -f "$mount_point/file"*.txt
  unmount_device "$mount_point"
}

# Benchmark 2: File reading performance
benchmark_file_reading() {
  local device_id mount_point

  create_device "$SMALL_DEVICE_SIZE_MB" device_id
  create_mount_point mount_point

  if ! mount_agentfs "$device_id" "$mount_point"; then
    echo "Skipping file reading benchmark - mount failed"
    return 0
  fi

  # Create test files first
  for i in {1..100}; do
    echo "content$i" >"$mount_point/file$i.txt"
  done

  # Benchmark reading files
  time_operation "Read 100 small files" "
        for i in {1..100}; do
            cat \"$mount_point/file\$i.txt\" >/dev/null
        done
    "

  # Cleanup
  rm -f "$mount_point/file"*.txt
  unmount_device "$mount_point"
}

# Benchmark 3: Large file operations
benchmark_large_files() {
  local device_id mount_point

  create_device "$LARGE_DEVICE_SIZE_MB" device_id
  create_mount_point mount_point

  if ! mount_agentfs "$device_id" "$mount_point"; then
    echo "Skipping large file benchmark - mount failed"
    return 0
  fi

  # Benchmark creating a large file
  time_operation "Create 10MB file" "
        dd if=/dev/zero of=\"$mount_point/large.bin\" bs=1M count=10 2>/dev/null
    "

  # Benchmark reading the large file
  time_operation "Read 10MB file" "
        dd if=\"$mount_point/large.bin\" of=/dev/null bs=1M 2>/dev/null
    "

  # Cleanup
  rm -f "$mount_point/large.bin"
  unmount_device "$mount_point"
}

# Benchmark 4: Directory operations
benchmark_directory_operations() {
  local device_id mount_point

  create_device "$SMALL_DEVICE_SIZE_MB" device_id
  create_mount_point mount_point

  if ! mount_agentfs "$device_id" "$mount_point"; then
    echo "Skipping directory benchmark - mount failed"
    return 0
  fi

  # Benchmark creating many directories
  time_operation "Create 100 directories" "
        for i in {1..100}; do
            mkdir \"$mount_point/dir\$i\"
        done
    "

  # Benchmark listing directories
  time_operation "List 100 directories" "
        ls \"$mount_point\" >/dev/null
    "

  # Cleanup
  rm -rf "$mount_point/dir"*
  unmount_device "$mount_point"
}

# Stress test 1: Many files in single directory
stress_many_files() {
  local device_id mount_point

  create_device "$LARGE_DEVICE_SIZE_MB" device_id
  create_mount_point mount_point

  if ! mount_agentfs "$device_id" "$mount_point"; then
    echo "Skipping many files stress test - mount failed"
    return 0
  fi

  echo -e "${YELLOW}Creating $STRESS_FILE_COUNT files...${NC}"

  time_operation "Create $STRESS_FILE_COUNT files" "
        for i in {1..$STRESS_FILE_COUNT}; do
            echo \"content\$i\" > \"$mount_point/stress_file_\$i.txt\"
        done
    "

  time_operation "List $STRESS_FILE_COUNT files" "
        ls \"$mount_point\" | wc -l >/dev/null
    "

  time_operation "Read all $STRESS_FILE_COUNT files" "
        for i in {1..$STRESS_FILE_COUNT}; do
            head -1 \"$mount_point/stress_file_\$i.txt\" >/dev/null
        done
    "

  time_operation "Delete $STRESS_FILE_COUNT files" "
        rm -f \"$mount_point/stress_file_\"*.txt
    "

  unmount_device "$mount_point"
}

# Stress test 2: Deep directory hierarchy
stress_deep_directories() {
  local device_id mount_point

  create_device "$SMALL_DEVICE_SIZE_MB" device_id
  create_mount_point mount_point

  if ! mount_agentfs "$device_id" "$mount_point"; then
    echo "Skipping deep directories stress test - mount failed"
    return 0
  fi

  echo -e "${YELLOW}Creating deep directory hierarchy...${NC}"

  # Create deep directory structure
  local current_path="$mount_point"
  for i in {1..$STRESS_DIR_DEPTH}; do
    current_path="$current_path/level$i"
  done

  time_operation "Create $STRESS_DIR_DEPTH level deep directory" "
        mkdir -p \"$current_path\"
    "

  # Create file at deepest level
  time_operation "Create file in deep directory" "
        echo 'deep file content' > \"$current_path/deep_file.txt\"
    "

  time_operation "Read file from deep directory" "
        cat \"$current_path/deep_file.txt\" >/dev/null
    "

  time_operation "Remove deep directory structure" "
        rm -rf \"$mount_point/level1\"
    "

  unmount_device "$mount_point"
}

# Stress test 3: Concurrent access
stress_concurrent_access() {
  local device_id mount_point

  create_device "$LARGE_DEVICE_SIZE_MB" device_id
  create_mount_point mount_point

  if ! mount_agentfs "$device_id" "$mount_point"; then
    echo "Skipping concurrent access stress test - mount failed"
    return 0
  fi

  echo -e "${YELLOW}Testing concurrent access with $CONCURRENT_WORKERS workers...${NC}"

  time_operation "Concurrent file operations ($CONCURRENT_WORKERS workers)" "
        # Create worker processes
        pids=()
        for worker in {1..$CONCURRENT_WORKERS}; do
            (
                # Each worker creates, reads, and deletes files
                for i in {1..50}; do
                    file_id=\$((worker * 1000 + i))
                    echo \"worker\$worker-file\$i\" > \"$mount_point/worker_\${file_id}.txt\"
                    grep \"worker\$worker\" \"$mount_point/worker_\${file_id}.txt\" >/dev/null
                    rm \"$mount_point/worker_\${file_id}.txt\"
                done
            ) &
            pids+=(\$!)
        done

        # Wait for all workers to complete
        for pid in \"\${pids[@]}\"; do
            wait \$pid
        done
    "

  unmount_device "$mount_point"
}

# Stress test 4: Snapshot and branch operations
stress_snapshots_branches() {
  local device_id mount_point

  create_device "$SMALL_DEVICE_SIZE_MB" device_id
  create_mount_point mount_point

  if ! mount_agentfs "$device_id" "$mount_point"; then
    echo "Skipping snapshot/branch stress test - mount failed"
    return 0
  fi

  echo -e "${YELLOW}Testing snapshot and branch operations...${NC}"

  # Create initial files
  for i in {1..10}; do
    echo "initial content $i" >"$mount_point/file$i.txt"
  done

  # Create snapshot
  time_operation "Create snapshot" "
        echo '{\"create\": {\"name\": \"stress_snapshot\"}}' > \"$mount_point/.agentfs/snapshot\"
    "

  # Modify files
  for i in {1..10}; do
    echo "modified content $i" >"$mount_point/file$i.txt"
  done

  # Create branch
  time_operation "Create branch from snapshot" "
        echo '{\"create\": {\"from_snapshot\": \"stress_snapshot\", \"name\": \"stress_branch\"}}' > \"$mount_point/.agentfs/branch\"
    "

  # List snapshots and branches
  time_operation "List snapshots and branches" "
        cat \"$mount_point/.agentfs/snapshot\" >/dev/null 2>&1 || true
        cat \"$mount_point/.agentfs/branch\" >/dev/null 2>&1 || true
    "

  unmount_device "$mount_point"
}

# Function to run all benchmarks multiple times and average results
run_benchmarks() {
  echo -e "${BLUE}Running performance benchmarks...${NC}"

  for iteration in $(seq 1 "$BENCHMARK_ITERATIONS"); do
    echo -e "${YELLOW}Iteration $iteration/$BENCHMARK_ITERATIONS${NC}"

    benchmark_file_creation
    benchmark_file_reading
    benchmark_large_files
    benchmark_directory_operations
  done
}

# Function to run all stress tests
run_stress_tests() {
  echo -e "${BLUE}Running stress tests...${NC}"

  stress_many_files
  stress_deep_directories
  stress_concurrent_access
  stress_snapshots_branches
}

# Function to generate benchmark report
generate_report() {
  echo -e "\n${BLUE}=== Benchmark Results Report ===${NC}"

  # Group results by operation type
  declare -A operation_times
  declare -A operation_counts

  for result in "${BENCHMARK_RESULTS[@]}"; do
    IFS=':' read -r operation time <<<"$result"
    if [ -z "${operation_times[$operation]}" ]; then
      operation_times[$operation]="$time"
      operation_counts[$operation]=1
    else
      # Calculate running average
      current_avg="${operation_times[$operation]}"
      current_count="${operation_counts[$operation]}"
      new_count=$((current_count + 1))
      new_avg=$(echo "scale=3; ($current_avg * $current_count + $time) / $new_count" | bc 2>/dev/null || echo "$time")

      operation_times[$operation]="$new_avg"
      operation_counts[$operation]="$new_count"
    fi
  done

  # Display averaged results
  echo -e "${YELLOW}Operation\t\t\tAverage Time (s)${NC}"
  echo "--------------------------------------------------"

  for operation in "${!operation_times[@]}"; do
    printf "%-30s %.3f\n" "$operation" "${operation_times[$operation]}"
  done

  # Save detailed results to file
  local results_file="/tmp/agentfs-stress-results-$(date +%Y%m%d-%H%M%S).txt"
  {
    echo "AgentFS Stress Test Results"
    echo "Generated: $(date)"
    echo "Iterations: $BENCHMARK_ITERATIONS"
    echo ""
    echo "Detailed Results:"
    printf "%-30s %-10s %-10s\n" "Operation" "Time(s)" "Iteration"
    echo "----------------------------------------------------------"

    local iteration=1
    for result in "${BENCHMARK_RESULTS[@]}"; do
      IFS=':' read -r operation time <<<"$result"
      printf "%-30s %-10s %-10d\n" "$operation" "$time" "$iteration"
      if [ $((iteration % 4)) -eq 0 ]; then
        iteration=1
      else
        iteration=$((iteration + 1))
      fi
    done

    echo ""
    echo "Averaged Results:"
    for operation in "${!operation_times[@]}"; do
      printf "%-30s %.3f (avg of %d runs)\n" "$operation" "${operation_times[$operation]}" "${operation_counts[$operation]}"
    done
  } >"$results_file"

  echo -e "\n${GREEN}Detailed results saved to: $results_file${NC}"
}

# Function to check system resources during stress test
monitor_resources() {
  local mount_point="$1"
  local duration="$2"

  echo -e "${BLUE}Monitoring system resources during stress test...${NC}"

  # Run monitoring in background
  {
    local start_time=$(date +%s)
    while [ $(($(date +%s) - start_time)) -lt "$duration" ]; do
      echo "$(date +%s): CPU=$(ps aux | awk 'NR>1 {sum+=$3} END {print sum}'), Memory=$(vm_stat | grep 'Pages free' | awk '{print $3}')" >>/tmp/agentfs-resource-monitor.log
      sleep 1
    done
  } &
  local monitor_pid=$!

  # Return the monitor PID so caller can wait for it
  echo "$monitor_pid"
}

# Main execution
main() {
  echo -e "${BLUE}AgentFS Stress Testing and Benchmarking Suite${NC}"
  echo -e "${YELLOW}This will create test devices and perform intensive filesystem operations${NC}"

  # Run benchmarks
  run_benchmarks

  # Run stress tests
  run_stress_tests

  # Generate report
  generate_report

  echo -e "\n${GREEN}Stress testing completed successfully!${NC}"
}

# Run main function with timeout
main "$@"
