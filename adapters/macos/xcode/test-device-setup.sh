#!/bin/bash
# Block device creation and cleanup utilities for AgentFS testing
# Provides functions for creating test block devices and cleaning them up

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Global variables
CREATED_DEVICES=()
CREATED_FILES=()
MOUNT_POINTS=()

# Cleanup function - called on script exit
cleanup_devices() {
  echo -e "${YELLOW}Cleaning up test devices...${NC}"

  # Unmount any mounted filesystems first
  for mount_point in "${MOUNT_POINTS[@]}"; do
    if mount | grep -q "$mount_point"; then
      echo -e "${BLUE}Unmounting $mount_point...${NC}"
      umount "$mount_point" 2>/dev/null || true
    fi
  done

  # Detach block devices
  for device_id in "${CREATED_DEVICES[@]}"; do
    if hdiutil info | grep -q "$device_id"; then
      echo -e "${BLUE}Detaching device $device_id...${NC}"
      hdiutil detach "$device_id" 2>/dev/null || true
    fi
  done

  # Remove dummy files
  for dummy_file in "${CREATED_FILES[@]}"; do
    if [ -f "$dummy_file" ]; then
      echo -e "${BLUE}Removing file $dummy_file...${NC}"
      rm -f "$dummy_file" 2>/dev/null || true
    fi
  done

  # Remove mount points
  for mount_point in "${MOUNT_POINTS[@]}"; do
    if [ -d "$mount_point" ]; then
      echo -e "${BLUE}Removing mount point $mount_point...${NC}"
      rmdir "$mount_point" 2>/dev/null || true
    fi
  done

  echo -e "${GREEN}Cleanup completed${NC}"
}

# Setup cleanup trap
trap cleanup_devices EXIT

# Function to create a dummy block device
# Usage: create_device <size> <output_var>
#   size: Size in MB (e.g., 10, 100, 1024)
#   output_var: Variable name to store the device ID
create_device() {
  local size_mb="$1"
  local output_var="$2"
  local dummy_file
  local device_id

  if [ -z "$size_mb" ] || [ -z "$output_var" ]; then
    echo -e "${RED}Usage: create_device <size_mb> <output_var>${NC}" >&2
    return 1
  fi

  echo -e "${BLUE}Creating ${size_mb}MB dummy block device...${NC}"

  # Create unique filename
  dummy_file="/tmp/agentfs-test-$(date +%s)-$$.img"

  # Create dummy file
  if ! mkfile -n "${size_mb}m" "$dummy_file"; then
    echo -e "${RED}Failed to create dummy file $dummy_file${NC}" >&2
    return 1
  fi

  echo -e "${GREEN}Created dummy file: $dummy_file${NC}"
  CREATED_FILES+=("$dummy_file")

  # Attach as block device
  local attach_output
  if ! attach_output=$(hdiutil attach -imagekey diskimage-class=CRawDiskImage -nomount "$dummy_file" 2>&1); then
    echo -e "${RED}Failed to attach device: $attach_output${NC}" >&2
    rm -f "$dummy_file"
    return 1
  fi

  # Extract device ID from output
  device_id=$(echo "$attach_output" | head -1 | awk '{print $1}')

  if [ -z "$device_id" ]; then
    echo -e "${RED}Failed to get device ID from hdiutil output${NC}" >&2
    hdiutil detach "$dummy_file" 2>/dev/null || true
    rm -f "$dummy_file"
    return 1
  fi

  echo -e "${GREEN}Created block device: $device_id${NC}"
  CREATED_DEVICES+=("$device_id")

  # Set output variable
  eval "$output_var=\"$device_id\""
}

# Function to create a mount point directory
# Usage: create_mount_point <output_var>
create_mount_point() {
  local output_var="$1"
  local mount_point

  if [ -z "$output_var" ]; then
    echo -e "${RED}Usage: create_mount_point <output_var>${NC}" >&2
    return 1
  fi

  # Create unique mount point
  mount_point="/tmp/agentfs-mount-$(date +%s)-$$"

  if ! mkdir -p "$mount_point"; then
    echo -e "${RED}Failed to create mount point $mount_point${NC}" >&2
    return 1
  fi

  echo -e "${GREEN}Created mount point: $mount_point${NC}"
  MOUNT_POINTS+=("$mount_point")

  # Set output variable
  eval "$output_var=\"$mount_point\""
}

# Function to mount a device using AgentFS
# Usage: mount_agentfs <device_id> <mount_point>
mount_agentfs() {
  local device_id="$1"
  local mount_point="$2"

  if [ -z "$device_id" ] || [ -z "$mount_point" ]; then
    echo -e "${RED}Usage: mount_agentfs <device_id> <mount_point>${NC}" >&2
    return 1
  fi

  echo -e "${BLUE}Mounting AgentFS on $device_id at $mount_point...${NC}"

  if ! sudo -n mount -F -t AgentFS "$device_id" "$mount_point" 2>/dev/null && ! mount -F -t AgentFS "$device_id" "$mount_point" 2>/dev/null; then
    echo -e "${YELLOW}Mount failed - this may be expected if FSKit extension is not properly installed${NC}"
    echo -e "${YELLOW}or if running on macOS < 15.4. Check system requirements.${NC}"
    return 1
  fi

  echo -e "${GREEN}AgentFS mounted successfully${NC}"
}

# Function to unmount a device
# Usage: unmount_device <mount_point>
unmount_device() {
  local mount_point="$1"

  if [ -z "$mount_point" ]; then
    echo -e "${RED}Usage: unmount_device <mount_point>${NC}" >&2
    return 1
  fi

  echo -e "${BLUE}Unmounting $mount_point...${NC}"

  if ! sudo -n umount "$mount_point" 2>/dev/null && ! umount "$mount_point"; then
    echo -e "${RED}Failed to unmount $mount_point${NC}" >&2
    return 1
  fi

  echo -e "${GREEN}Unmounted successfully${NC}"
}

# Function to get device information
# Usage: get_device_info <device_id>
get_device_info() {
  local device_id="$1"

  if [ -z "$device_id" ]; then
    echo -e "${RED}Usage: get_device_info <device_id>${NC}" >&2
    return 1
  fi

  echo -e "${BLUE}Device information for $device_id:${NC}"
  hdiutil info | grep -A 5 -B 5 "$device_id" || echo "Device not found in hdiutil info"
}

# Function to list all created devices
list_created_devices() {
  echo -e "${BLUE}Created devices:${NC}"
  for device in "${CREATED_DEVICES[@]}"; do
    echo "  $device"
  done

  echo -e "${BLUE}Created files:${NC}"
  for file in "${CREATED_FILES[@]}"; do
    echo "  $file"
  done

  echo -e "${BLUE}Mount points:${NC}"
  for mount in "${MOUNT_POINTS[@]}"; do
    echo "  $mount"
  done
}

# Function to validate device is accessible
# Usage: validate_device <device_id>
validate_device() {
  local device_id="$1"

  if [ -z "$device_id" ]; then
    echo -e "${RED}Usage: validate_device <device_id>${NC}" >&2
    return 1
  fi

  echo -e "${BLUE}Validating device $device_id...${NC}"

  # Check if device exists
  if ! diskutil info "$device_id" >/dev/null 2>&1; then
    echo -e "${RED}Device $device_id is not accessible${NC}" >&2
    return 1
  fi

  echo -e "${GREEN}Device $device_id is accessible${NC}"
}

# Function to create multiple devices for parallel testing
# Usage: create_multiple_devices <count> <size_mb> <output_array_var>
create_multiple_devices() {
  local count="$1"
  local size_mb="$2"
  local output_array_var="$3"
  local devices=()

  if [ -z "$count" ] || [ -z "$size_mb" ] || [ -z "$output_array_var" ]; then
    echo -e "${RED}Usage: create_multiple_devices <count> <size_mb> <output_array_var>${NC}" >&2
    return 1
  fi

  echo -e "${BLUE}Creating $count devices of ${size_mb}MB each...${NC}"

  for i in $(seq 1 "$count"); do
    local device_var="device_$i"
    if ! create_device "$size_mb" "$device_var"; then
      echo -e "${RED}Failed to create device $i${NC}" >&2
      return 1
    fi

    # Get the device ID from the variable
    eval "devices+=(\"\$$device_var\")"
  done

  echo -e "${GREEN}Created $count devices successfully${NC}"

  # Set output array variable
  eval "$output_array_var=(\"\${devices[@]}\")"
}

# Export functions for use in other scripts
export -f cleanup_devices
export -f create_device
export -f create_mount_point
export -f mount_agentfs
export -f unmount_device
export -f get_device_info
export -f list_created_devices
export -f validate_device
export -f create_multiple_devices
