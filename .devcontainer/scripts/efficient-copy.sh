#!/bin/bash
# Utility for efficient copying with fallback mechanisms
# Provides copy-on-write, hard links, and regular copy as fallbacks
#
# Usage: efficient_copy <source_dir> <dest_dir> [--verbose]
#
# Returns:
#   0 - Success
#   1 - Source directory doesn't exist or is empty
#   2 - Destination directory creation failed
#   3 - All copy methods failed

set -euo pipefail

# Function to perform efficient copy with multiple fallback strategies
# Arguments:
#   $1 - Source directory path
#   $2 - Destination directory path
#   $3 - Optional: --verbose for detailed output
efficient_copy() {
    local source_dir="$1"
    local dest_dir="$2"
    local verbose=false

    # Parse optional verbose flag
    if [[ "${3:-}" == "--verbose" ]]; then
        verbose=true
    fi

    # Validate source directory exists and is not empty
    if [[ ! -d "$source_dir" ]] || [[ -z "$(ls -A "$source_dir" 2>/dev/null)" ]]; then
        [[ "$verbose" == true ]] && echo "Source directory '$source_dir' doesn't exist or is empty"
        return 1
    fi

    # Ensure destination directory exists
    if ! mkdir -p "$dest_dir"; then
        [[ "$verbose" == true ]] && echo "Failed to create destination directory '$dest_dir'"
        return 2
    fi

    # Strategy 1: Copy-on-write (fastest, zero disk usage until modification)
    if command -v cp >/dev/null 2>&1 && cp --help 2>/dev/null | grep -q "\--reflink"; then
        [[ "$verbose" == true ]] && echo "Attempting copy-on-write..."
        if cp -r --reflink=auto "$source_dir"/* "$dest_dir/" 2>/dev/null; then
            [[ "$verbose" == true ]] && echo "✓ Copy-on-write successful"
            return 0
        fi
        [[ "$verbose" == true ]] && echo "✗ Copy-on-write failed, trying hard links..."
    fi

    # Strategy 2: Hard links (fast, space-efficient, same filesystem required)
    if cp -rl "$source_dir"/* "$dest_dir/" 2>/dev/null; then
        [[ "$verbose" == true ]] && echo "✓ Hard link copy successful"
        return 0
    fi
    [[ "$verbose" == true ]] && echo "✗ Hard link copy failed, trying rsync..."

    # Strategy 3: Rsync with deduplication (good for incremental updates)
    if command -v rsync >/dev/null 2>&1; then
        if rsync -a --link-dest="$source_dir" "$source_dir"/ "$dest_dir/" 2>/dev/null; then
            [[ "$verbose" == true ]] && echo "✓ Rsync copy successful"
            return 0
        fi
        [[ "$verbose" == true ]] && echo "✗ Rsync failed, trying regular copy..."
    fi

    # Strategy 4: Regular copy (slowest but most compatible)
    if cp -r "$source_dir"/* "$dest_dir/" 2>/dev/null; then
        [[ "$verbose" == true ]] && echo "✓ Regular copy successful"
        return 0
    fi

    [[ "$verbose" == true ]] && echo "✗ All copy methods failed"
    return 3
}

# If script is executed directly (not sourced), call the function with arguments
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    efficient_copy "$@"
fi
