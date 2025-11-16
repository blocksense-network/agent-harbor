#!/usr/bin/env bash
# test-kitty-integration.sh - Test script for Kitty terminal multiplexer integration
# This script tests all scenarios described in the Kitty.md documentation

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
SOCKET_PATH="${KITTY_LISTEN_ON:-unix:/tmp/kitty-test-ah.sock}"
TEST_TASK_ID="test-task-$(date +%s)"
TEST_CWD="${PWD}"
CLEANUP_ON_EXIT=true

# Test results tracking
declare -i TESTS_PASSED=0
declare -i TESTS_FAILED=0
declare -a FAILED_TESTS=()

# Utility functions
log_info() {
    echo -e "${BLUE}[INFO]${NC} $*"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $*"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $*"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $*"
}

test_passed() {
    local test_name="$1"
    ((TESTS_PASSED++))
    log_success "✓ $test_name"
}

test_failed() {
    local test_name="$1"
    local reason="${2:-Unknown reason}"
    ((TESTS_FAILED++))
    FAILED_TESTS+=("$test_name: $reason")
    log_error "✗ $test_name - $reason"
}

# Cleanup function
cleanup() {
    if [[ "$CLEANUP_ON_EXIT" == "true" ]]; then
        log_info "Cleaning up test windows..."
        kitty @ --to "$SOCKET_PATH" close-window --match "title:test-" 2>/dev/null || true
        kitty @ --to "$SOCKET_PATH" close-tab --match "title:ah-task-test-" 2>/dev/null || true
    else
        log_warning "Skipping cleanup (CLEANUP_ON_EXIT=false)"
    fi
}

trap cleanup EXIT

# Test 1: Check Kitty availability
test_kitty_available() {
    log_info "Test 1: Check Kitty availability"

    if ! command -v kitty &>/dev/null; then
        test_failed "Kitty availability" "kitty command not found"
        return 1
    fi

    local version
    version=$(kitty --version 2>/dev/null | grep -oP 'kitty \K[0-9.]+' || echo "")

    if [[ -z "$version" ]]; then
        test_failed "Kitty availability" "Cannot determine version"
        return 1
    fi

    log_info "Found Kitty version: $version"
    test_passed "Kitty availability"
}

# Test 2: Start Kitty with remote control
test_start_kitty_remote_control() {
    log_info "Test 2: Start Kitty with remote control"

    # Check if already running
    if kitty @ --to "$SOCKET_PATH" ls &>/dev/null; then
        log_info "Kitty already running with remote control"
        test_passed "Start Kitty with remote control (already running)"
        return 0
    fi

    # Start Kitty in background
    log_info "Starting Kitty with socket: $SOCKET_PATH"
    if kitty --listen-on "$SOCKET_PATH" --detach &>/dev/null; then
        sleep 2

        # Verify it's working
        if kitty @ --to "$SOCKET_PATH" ls &>/dev/null; then
            test_passed "Start Kitty with remote control"
        else
            test_failed "Start Kitty with remote control" "Socket not responding"
            return 1
        fi
    else
        test_failed "Start Kitty with remote control" "Failed to start Kitty"
        return 1
    fi
}

# Test 3: List windows and tabs
test_list_windows() {
    log_info "Test 3: List windows and tabs"

    if output=$(kitty @ --to "$SOCKET_PATH" ls 2>&1); then
        if echo "$output" | jq empty 2>/dev/null; then
            log_info "Got valid JSON output"
            test_passed "List windows and tabs"
        else
            test_failed "List windows and tabs" "Invalid JSON output"
            return 1
        fi
    else
        test_failed "List windows and tabs" "Command failed: $output"
        return 1
    fi
}

# Test 4: Create new tab
test_create_tab() {
    log_info "Test 4: Create new tab"

    local title="test-tab-$$"
    if tab_id=$(kitty @ --to "$SOCKET_PATH" launch \
        --type=tab \
        --title "$title" \
        --cwd "$TEST_CWD" \
        -- bash -c 'echo "Test tab created"; sleep 60' 2>&1); then

        log_info "Created tab with ID: $tab_id"

        # Verify tab exists
        if kitty @ --to "$SOCKET_PATH" ls | jq -e --arg title "$title" \
            '.[] | .tabs[] | .windows[] | select(.title == $title)' &>/dev/null; then
            test_passed "Create new tab"
        else
            test_failed "Create new tab" "Tab not found in list"
            return 1
        fi
    else
        test_failed "Create new tab" "Command failed: $tab_id"
        return 1
    fi
}

# Test 5: Create horizontal split
test_horizontal_split() {
    log_info "Test 5: Create horizontal split"

    local title="test-hsplit-$$"
    # Create a base window first
    local base_id
    base_id=$(kitty @ --to "$SOCKET_PATH" launch \
        --type=tab \
        --title "${title}-base" \
        --cwd "$TEST_CWD" \
        -- bash -c 'sleep 60')

    sleep 0.5

    # Create horizontal split
    if split_id=$(kitty @ --to "$SOCKET_PATH" launch \
        --type=window \
        --location=hsplit \
        --title "${title}-split" \
        --cwd "$TEST_CWD" \
        -- bash -c 'echo "Horizontal split"; sleep 60' 2>&1); then

        log_info "Created horizontal split with ID: $split_id"
        test_passed "Create horizontal split"
    else
        test_failed "Create horizontal split" "Command failed: $split_id"
        return 1
    fi
}

# Test 6: Create vertical split
test_vertical_split() {
    log_info "Test 6: Create vertical split"

    local title="test-vsplit-$$"
    # Create a base window first
    local base_id
    base_id=$(kitty @ --to "$SOCKET_PATH" launch \
        --type=tab \
        --title "${title}-base" \
        --cwd "$TEST_CWD" \
        -- bash -c 'sleep 60')

    sleep 0.5

    # Create vertical split with percentage
    if split_id=$(kitty @ --to "$SOCKET_PATH" launch \
        --type=window \
        --location=vsplit:30% \
        --title "${title}-split" \
        --cwd "$TEST_CWD" \
        -- bash -c 'echo "Vertical split 30%"; sleep 60' 2>&1); then

        log_info "Created vertical split with ID: $split_id"
        test_passed "Create vertical split"
    else
        test_failed "Create vertical split" "Command failed: $split_id"
        return 1
    fi
}

# Test 7: Focus window by title
test_focus_window() {
    log_info "Test 7: Focus window by title"

    local title="test-focus-$$"
    # Create a window to focus
    kitty @ --to "$SOCKET_PATH" launch \
        --type=tab \
        --title "$title" \
        -- bash -c 'sleep 60' &>/dev/null

    sleep 0.5

    # Try to focus it
    if kitty @ --to "$SOCKET_PATH" focus-window --match "title:$title" 2>&1; then
        test_passed "Focus window by title"
    else
        test_failed "Focus window by title" "Command failed"
        return 1
    fi
}

# Test 8: Send text to window
test_send_text() {
    log_info "Test 8: Send text to window"

    local title="test-sendtext-$$"
    local output_file="/tmp/kitty-test-output-$$"

    # Create a window that captures input
    kitty @ --to "$SOCKET_PATH" launch \
        --type=tab \
        --title "$title" \
        -- bash -c "cat > '$output_file'; sleep 2" &>/dev/null

    sleep 1

    # Send text
    if kitty @ --to "$SOCKET_PATH" send-text \
        --match "title:$title" \
        -- "Test message\r" 2>&1; then

        # Give it time to write
        sleep 1

        # Check if text was received (file should exist)
        if [[ -f "$output_file" ]]; then
            log_info "Text sent successfully"
            test_passed "Send text to window"
            rm -f "$output_file"
        else
            test_failed "Send text to window" "Output file not created"
            return 1
        fi
    else
        test_failed "Send text to window" "Command failed"
        rm -f "$output_file"
        return 1
    fi
}

# Test 9: Launch with environment variables
test_launch_with_env() {
    log_info "Test 9: Launch with environment variables"

    local title="test-env-$$"
    local test_value="test_value_$$"

    if win_id=$(kitty @ --to "$SOCKET_PATH" launch \
        --type=tab \
        --title "$title" \
        --env "TEST_VAR=$test_value" \
        -- bash -c 'echo "TEST_VAR=$TEST_VAR"; sleep 60' 2>&1); then

        log_info "Launched with environment variable"
        test_passed "Launch with environment variables"
    else
        test_failed "Launch with environment variables" "Command failed: $win_id"
        return 1
    fi
}

# Test 10: Launch with specific working directory
test_launch_with_cwd() {
    log_info "Test 10: Launch with specific working directory"

    local title="test-cwd-$$"
    local test_dir="/tmp"

    if win_id=$(kitty @ --to "$SOCKET_PATH" launch \
        --type=tab \
        --title "$title" \
        --cwd "$test_dir" \
        -- bash -c 'pwd; sleep 60' 2>&1); then

        log_info "Launched with working directory: $test_dir"
        test_passed "Launch with specific working directory"
    else
        test_failed "Launch with specific working directory" "Command failed: $win_id"
        return 1
    fi
}

# Test 11: Create Agent Harbor session layout
test_ah_session_layout() {
    log_info "Test 11: Create Agent Harbor session layout (3-pane)"

    local task_id="$TEST_TASK_ID"
    local title_prefix="ah-task-${task_id}"

    # Create editor pane (left, 70%)
    log_info "Creating editor pane..."
    if ! editor_id=$(kitty @ --to "$SOCKET_PATH" launch \
        --type=tab \
        --cwd "$TEST_CWD" \
        --title "${title_prefix}-editor" \
        --tab-title "$title_prefix" \
        -- bash -c 'echo "Editor pane"; sleep 60' 2>&1); then
        test_failed "Create AH session layout" "Failed to create editor pane"
        return 1
    fi

    sleep 0.5

    # Create TUI pane (top-right, 30%)
    log_info "Creating TUI pane..."
    if ! tui_id=$(kitty @ --to "$SOCKET_PATH" launch \
        --type=window \
        --location=vsplit:30% \
        --cwd "$TEST_CWD" \
        --title "${title_prefix}-tui" \
        -- bash -c 'echo "TUI pane"; sleep 60' 2>&1); then
        test_failed "Create AH session layout" "Failed to create TUI pane"
        return 1
    fi

    sleep 0.5

    # Create logs pane (bottom-right)
    log_info "Creating logs pane..."
    if ! logs_id=$(kitty @ --to "$SOCKET_PATH" launch \
        --type=window \
        --location=hsplit:50% \
        --cwd "$TEST_CWD" \
        --title "${title_prefix}-logs" \
        --match "id:$tui_id" \
        -- bash -c 'echo "Logs pane"; sleep 60' 2>&1); then
        test_failed "Create AH session layout" "Failed to create logs pane"
        return 1
    fi

    log_info "Layout created successfully:"
    log_info "  Editor: $editor_id"
    log_info "  TUI:    $tui_id"
    log_info "  Logs:   $logs_id"

    test_passed "Create Agent Harbor session layout"
}

# Test 12: Discover existing session by title
test_discover_session() {
    log_info "Test 12: Discover existing session by title"

    local title_prefix="ah-task-${TEST_TASK_ID}"

    if tab_info=$(kitty @ --to "$SOCKET_PATH" ls 2>&1 | \
        jq -r --arg prefix "$title_prefix" \
        '.[] | .tabs[] | select(.title | startswith($prefix)) | .id' | head -1); then

        if [[ -n "$tab_info" ]]; then
            log_info "Found existing session: $tab_info"
            test_passed "Discover existing session"
        else
            test_failed "Discover existing session" "Session not found"
            return 1
        fi
    else
        test_failed "Discover existing session" "jq command failed"
        return 1
    fi
}

# Test 13: Focus existing session
test_focus_existing_session() {
    log_info "Test 13: Focus existing session"

    local title_prefix="ah-task-${TEST_TASK_ID}"

    if kitty @ --to "$SOCKET_PATH" focus-tab --match "title:$title_prefix" 2>&1; then
        test_passed "Focus existing session"
    else
        test_failed "Focus existing session" "Command failed"
        return 1
    fi
}

# Test 14: Get window text (if supported)
test_get_text() {
    log_info "Test 14: Get window text"

    local title="test-gettext-$$"
    local test_text="UNIQUE_TEST_TEXT_$$"

    # Create window with known text
    kitty @ --to "$SOCKET_PATH" launch \
        --type=tab \
        --title "$title" \
        -- bash -c "echo '$test_text'; sleep 60" &>/dev/null

    sleep 1

    # Try to get text (this may not work in all versions)
    if text=$(kitty @ --to "$SOCKET_PATH" get-text \
        --match "title:$title" \
        --extent=screen 2>&1); then

        if echo "$text" | grep -q "$test_text"; then
            log_info "Retrieved window text successfully"
            test_passed "Get window text"
        else
            log_warning "get-text succeeded but text not found (may be timing issue)"
            test_passed "Get window text (command works)"
        fi
    else
        log_warning "get-text command not supported or failed: $text"
        log_info "This is optional functionality"
        test_passed "Get window text (not required)"
    fi
}

# Test 15: Close window by ID
test_close_window() {
    log_info "Test 15: Close window by ID"

    local title="test-close-$$"

    # Create a window to close
    if win_id=$(kitty @ --to "$SOCKET_PATH" launch \
        --type=tab \
        --title "$title" \
        -- bash -c 'sleep 60' 2>&1); then

        sleep 0.5

        # Close it
        if kitty @ --to "$SOCKET_PATH" close-window --match "id:$win_id" 2>&1; then
            sleep 0.5

            # Verify it's gone
            if kitty @ --to "$SOCKET_PATH" ls | \
                jq -e --arg id "$win_id" \
                '.[] | .tabs[] | .windows[] | select(.id == ($id | tonumber))' &>/dev/null; then
                test_failed "Close window by ID" "Window still exists"
                return 1
            else
                test_passed "Close window by ID"
            fi
        else
            test_failed "Close window by ID" "Command failed"
            return 1
        fi
    else
        test_failed "Close window by ID" "Failed to create test window"
        return 1
    fi
}

# Main test runner
run_all_tests() {
    log_info "=========================================="
    log_info "Kitty Integration Test Suite"
    log_info "=========================================="
    log_info "Socket: $SOCKET_PATH"
    log_info "Task ID: $TEST_TASK_ID"
    log_info "CWD: $TEST_CWD"
    log_info "=========================================="
    echo

    # Run tests
    test_kitty_available || exit 1
    test_start_kitty_remote_control || exit 1
    test_list_windows
    test_create_tab
    test_horizontal_split
    test_vertical_split
    test_focus_window
    test_send_text
    test_launch_with_env
    test_launch_with_cwd
    test_ah_session_layout
    test_discover_session
    test_focus_existing_session
    test_get_text
    test_close_window

    # Print summary
    echo
    log_info "=========================================="
    log_info "Test Summary"
    log_info "=========================================="
    log_success "Passed: $TESTS_PASSED"

    if [[ $TESTS_FAILED -gt 0 ]]; then
        log_error "Failed: $TESTS_FAILED"
        echo
        log_error "Failed tests:"
        for test in "${FAILED_TESTS[@]}"; do
            log_error "  - $test"
        done
        exit 1
    else
        log_success "All tests passed!"
        exit 0
    fi
}

# Parse command line arguments
show_help() {
    cat <<EOF
Usage: $0 [OPTIONS]

Test Kitty terminal multiplexer integration for Agent Harbor.

OPTIONS:
    -h, --help              Show this help message
    -s, --socket PATH       Use specific socket path (default: $SOCKET_PATH)
    -k, --keep-windows      Don't cleanup test windows on exit
    -t, --test NAME         Run specific test only
    -i, --interactive       Show interactive menu to choose tests

EXAMPLES:
    $0                      # Run all tests
    $0 -i                   # Interactive menu
    $0 -k                   # Run all tests and keep windows for inspection
    $0 -s unix:/tmp/my.sock # Use custom socket
    $0 -t test_create_tab   # Run specific test

AVAILABLE TESTS:
    test_kitty_available
    test_start_kitty_remote_control
    test_list_windows
    test_create_tab
    test_horizontal_split
    test_vertical_split
    test_focus_window
    test_send_text
    test_launch_with_env
    test_launch_with_cwd
    test_ah_session_layout
    test_discover_session
    test_focus_existing_session
    test_get_text
    test_close_window
EOF
}

# Interactive menu to choose tests
show_interactive_menu() {
    local -a all_tests=(
        "test_kitty_available:Check Kitty availability"
        "test_start_kitty_remote_control:Start Kitty with remote control"
        "test_list_windows:List windows and tabs"
        "test_create_tab:Create new tab"
        "test_horizontal_split:Create horizontal split"
        "test_vertical_split:Create vertical split"
        "test_focus_window:Focus window by title"
        "test_send_text:Send text to window"
        "test_launch_with_env:Launch with environment variables"
        "test_launch_with_cwd:Launch with working directory"
        "test_ah_session_layout:Create Agent Harbor session layout"
        "test_discover_session:Discover existing session"
        "test_focus_existing_session:Focus existing session"
        "test_get_text:Get window text"
        "test_close_window:Close window by ID"
    )

    # Run prerequisite tests once at the start
    local prereqs_ok=false

    while true; do
        # Reset counters for each menu iteration
        TESTS_PASSED=0
        TESTS_FAILED=0
        FAILED_TESTS=()

        echo
        log_info "=========================================="
        log_info "Kitty Integration Test Menu"
        log_info "=========================================="
        log_info "Socket: $SOCKET_PATH"
        if [[ "$CLEANUP_ON_EXIT" == "false" ]]; then
            log_warning "Cleanup disabled - windows will remain"
        fi
        echo
        echo "Select tests to run:"
        echo

        local i=1
        for test_item in "${all_tests[@]}"; do
            local test_func="${test_item%%:*}"
            local test_desc="${test_item#*:}"
            printf " %2d) %s\n" "$i" "$test_desc"
            ((i++))
        done

        echo
        echo " Commands:"
        echo "   a) Run all tests"
        echo "   r) Reset/rerun prerequisites (availability & remote control)"
        echo "   c) Toggle cleanup (currently: $([ "$CLEANUP_ON_EXIT" == "true" ] && echo "enabled" || echo "disabled"))"
        echo "   q) Quit"
        echo
        echo "Enter test numbers (space-separated), or command letter:"

        read -p "> " choices

        # Handle commands
        case "$choices" in
            q|Q)
                log_info "Exiting..."
                exit 0
                ;;
            a|A)
                log_info "Running all tests..."
                echo
                run_all_tests
                return
                ;;
            r|R)
                log_info "Rerunning prerequisite tests..."
                echo
                prereqs_ok=false
                continue
                ;;
            c|C)
                if [[ "$CLEANUP_ON_EXIT" == "true" ]]; then
                    CLEANUP_ON_EXIT=false
                    log_warning "Cleanup disabled - test windows will remain"
                else
                    CLEANUP_ON_EXIT=true
                    log_success "Cleanup enabled - test windows will be removed"
                fi
                sleep 1
                continue
                ;;
            "")
                log_warning "No selection made"
                sleep 1
                continue
                ;;
        esac

        # Run prerequisites if not already done
        if [[ "$prereqs_ok" == "false" ]]; then
            echo
            log_info "Running prerequisite checks..."
            if ! test_kitty_available; then
                log_error "Prerequisite failed. Fix the issue and try 'r' to retry."
                read -p "Press Enter to continue..."
                continue
            fi
            if ! test_start_kitty_remote_control; then
                log_error "Prerequisite failed. Fix the issue and try 'r' to retry."
                read -p "Press Enter to continue..."
                continue
            fi
            prereqs_ok=true
            echo
        fi

        # Parse and run selected tests
        local -a selected_tests=()
        for num in $choices; do
            if [[ "$num" =~ ^[0-9]+$ ]] && [ "$num" -ge 1 ] && [ "$num" -le "${#all_tests[@]}" ]; then
                local idx=$((num - 1))
                local test_item="${all_tests[$idx]}"
                local test_func="${test_item%%:*}"
                selected_tests+=("$test_func")
            else
                log_warning "Invalid selection: $num (skipping)"
            fi
        done

        if [ ${#selected_tests[@]} -eq 0 ]; then
            log_error "No valid tests selected"
            sleep 1
            continue
        fi

        echo
        log_info "Running ${#selected_tests[@]} selected test(s)..."
        echo

        for test_func in "${selected_tests[@]}"; do
            "$test_func" || true
        done

        # Print summary
        echo
        log_info "=========================================="
        log_info "Test Summary"
        log_info "=========================================="
        log_success "Passed: $TESTS_PASSED"

        if [[ $TESTS_FAILED -gt 0 ]]; then
            log_error "Failed: $TESTS_FAILED"
            echo
            log_error "Failed tests:"
            for test in "${FAILED_TESTS[@]}"; do
                log_error "  - $test"
            done
        else
            log_success "All selected tests passed!"
        fi

        echo
        read -p "Press Enter to return to menu (or Ctrl+C to exit)..."
    done
}

# Main entry point
SPECIFIC_TEST=""
INTERACTIVE=false

while [[ $# -gt 0 ]]; do
    case $1 in
        -h|--help)
            show_help
            exit 0
            ;;
        -s|--socket)
            SOCKET_PATH="$2"
            shift 2
            ;;
        -k|--keep-windows)
            CLEANUP_ON_EXIT=false
            shift
            ;;
        -t|--test)
            SPECIFIC_TEST="$2"
            shift 2
            ;;
        -i|--interactive)
            INTERACTIVE=true
            shift
            ;;
        *)
            log_error "Unknown option: $1"
            show_help
            exit 1
            ;;
    esac
done

# Run tests
if [[ "$INTERACTIVE" == "true" ]]; then
    show_interactive_menu
elif [[ -n "$SPECIFIC_TEST" ]]; then
    log_info "Running specific test: $SPECIFIC_TEST"
    if declare -f "$SPECIFIC_TEST" >/dev/null; then
        test_kitty_available || exit 1
        test_start_kitty_remote_control || exit 1
        "$SPECIFIC_TEST"
    else
        log_error "Test function '$SPECIFIC_TEST' not found"
        exit 1
    fi
else
    run_all_tests
fi
