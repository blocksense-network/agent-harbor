# Kitty Integration Test Script

This directory contains a comprehensive test script for validating Kitty terminal multiplexer integration with Agent Harbor.

## Files

- **Kitty.md** - Complete integration documentation following the Multiplexer Description Template
- **test-kitty-integration.sh** - Comprehensive test script covering all scenarios

## Prerequisites

Before running the tests, ensure you have:

1. **Kitty installed** (version 0.26.0 or later recommended)

   ```bash
   # Check installation
   kitty --version
   ```

2. **Required tools**:
   - `jq` - JSON processor for parsing Kitty's output
   - `bash` - Shell for running the script

   Install on Ubuntu/Debian:

   ```bash
   sudo apt install jq
   ```

   Install on macOS:

   ```bash
   brew install jq
   ```

3. **Kitty with remote control** - Either:
   - Running inside Kitty (automatically has `$KITTY_LISTEN_ON` set)
   - Or start Kitty with a socket: `kitty --listen-on unix:/tmp/kitty-test.sock`

## Running the Tests

### Run All Tests

```bash
./test-kitty-integration.sh
```

This will:

1. Check Kitty availability and version
2. Ensure remote control is enabled
3. Test all core functionalities:
   - Tab/window creation
   - Horizontal and vertical splits
   - Focus management
   - Text injection (send-text)
   - Environment variable passing
   - Working directory control
   - Agent Harbor 3-pane layout creation
   - Session discovery and focus
4. Clean up test windows automatically

### Run with Custom Socket

```bash
./test-kitty-integration.sh --socket unix:/tmp/my-kitty.sock
```

### Keep Test Windows for Inspection

```bash
./test-kitty-integration.sh --keep-windows
```

This prevents automatic cleanup, allowing you to inspect the created layout.

### Run Specific Test

```bash
./test-kitty-integration.sh --test test_ah_session_layout
```

Available tests:

- `test_kitty_available` - Check if Kitty is installed and usable
- `test_start_kitty_remote_control` - Verify remote control functionality
- `test_list_windows` - Test window/tab listing
- `test_create_tab` - Create a new tab
- `test_horizontal_split` - Create horizontal split
- `test_vertical_split` - Create vertical split with percentage
- `test_focus_window` - Focus window by title
- `test_send_text` - Send text to a window
- `test_launch_with_env` - Launch with environment variables
- `test_launch_with_cwd` - Launch with specific working directory
- `test_ah_session_layout` - Create Agent Harbor 3-pane layout
- `test_discover_session` - Find existing session by title
- `test_focus_existing_session` - Focus existing session
- `test_get_text` - Extract text from window
- `test_close_window` - Close window by ID

### Help

```bash
./test-kitty-integration.sh --help
```

## Test Scenarios Covered

### 1. Basic Multiplexer Operations

- Creating tabs and windows
- Horizontal and vertical splits
- Focusing windows and tabs
- Closing windows

### 2. Command Execution

- Launching commands in new panes
- Working directory control
- Environment variable passing
- Command output verification

### 3. Interactive Features

- Sending text to windows (simulating user input)
- Handling interactive prompts
- Text injection with proper escaping

### 4. Agent Harbor Specific

- Creating 3-pane layout (editor/TUI/logs)
- Session discovery by title prefix
- Focusing existing sessions
- Task ID-based naming conventions

### 5. Programmatic Control

- JSON-based window listing
- ID-based and title-based targeting
- Match selector syntax validation

## Expected Output

### Success

```
[INFO] ==========================================
[INFO] Kitty Integration Test Suite
[INFO] ==========================================
[INFO] Socket: unix:/tmp/kitty-test-ah.sock
[INFO] Task ID: test-task-1234567890
[INFO] CWD: /home/user/project
[INFO] ==========================================

[INFO] Test 1: Check Kitty availability
[INFO] Found Kitty version: 0.35.2
[SUCCESS] ✓ Kitty availability
[INFO] Test 2: Start Kitty with remote control
[INFO] Kitty already running with remote control
[SUCCESS] ✓ Start Kitty with remote control (already running)
...
[SUCCESS] ✓ Create Agent Harbor session layout
...
[INFO] ==========================================
[INFO] Test Summary
[INFO] ==========================================
[SUCCESS] Passed: 15
[SUCCESS] All tests passed!
```

### Failure

```
[ERROR] ✗ Create new tab - Command failed: socket not responding
...
[INFO] ==========================================
[INFO] Test Summary
[INFO] ==========================================
[SUCCESS] Passed: 3
[ERROR] Failed: 1
[ERROR] Failed tests:
[ERROR]   - Create new tab: socket not responding
```

## Troubleshooting

### "kitty command not found"

Kitty is not installed or not in your PATH.

**Solution:**

```bash
# Install Kitty
# Linux:
sudo apt install kitty
# macOS:
brew install --cask kitty
# Or download from https://sw.kovidgoyal.net/kitty/
```

### "Socket not responding"

Remote control is not enabled or socket is not accessible.

**Solution:**

```bash
# Start Kitty with remote control
kitty --listen-on unix:/tmp/kitty-test.sock &

# Or set environment variable and restart
export KITTY_LISTEN_ON=unix:/tmp/kitty-test.sock
kitty &
```

### "jq: command not found"

The `jq` JSON processor is not installed.

**Solution:**

```bash
# Ubuntu/Debian:
sudo apt install jq
# macOS:
brew install jq
# Arch:
sudo pacman -S jq
```

### "Permission denied" on socket

Socket file has incorrect permissions.

**Solution:**

```bash
# Check socket permissions
ls -l /tmp/kitty-*.sock

# Ensure you own the socket or have access
# Try with a socket in your home directory
./test-kitty-integration.sh --socket "unix:$HOME/.kitty.sock"
```

### Tests timeout or hang

Windows are not closing properly or commands are blocking.

**Solution:**

```bash
# Clean up manually
kitty @ --to unix:/tmp/kitty-test.sock close-window --match "title:test-"

# Or restart Kitty
pkill kitty
kitty --listen-on unix:/tmp/kitty-test.sock &
```

### "get-text command not supported"

Older Kitty version that doesn't support text extraction.

**Solution:**

- This is optional functionality; the test will pass with a warning
- Upgrade Kitty to 0.19.0+ for full feature support

## Integration with Agent Harbor

The test script validates all operations required for Agent Harbor's Kitty integration:

1. **Session Creation** - Tests the 3-pane layout (editor/TUI/logs) that Agent Harbor uses
2. **Session Discovery** - Validates finding existing sessions by task ID
3. **Session Focus** - Ensures sessions can be brought to foreground
4. **Command Execution** - Verifies commands can be launched in specific panes

These tests correspond to the workflows described in `Kitty.md` and ensure the Rust `Multiplexer` trait implementation will work correctly.

## CI/CD Integration

To run these tests in CI:

```yaml
# Example GitHub Actions workflow
- name: Install Kitty
  run: |
    curl -L https://sw.kovidgoyal.net/kitty/installer.sh | sh /dev/stdin
    export PATH="$HOME/.local/kitty.app/bin:$PATH"

- name: Start Kitty with remote control
  run: |
    kitty --listen-on unix:/tmp/kitty-ci.sock --detach
    sleep 2

- name: Run Kitty integration tests
  run: |
    ./test-kitty-integration.sh --socket unix:/tmp/kitty-ci.sock
```

Note: GUI tests in CI may require Xvfb or similar virtual display:

```bash
# Start virtual display
Xvfb :99 -screen 0 1024x768x24 &
export DISPLAY=:99

# Then run tests
./test-kitty-integration.sh
```

## Contributing

When adding new Kitty features to Agent Harbor:

1. Document the feature in `Kitty.md`
2. Add a test case to `test-kitty-integration.sh`
3. Ensure the test validates both success and failure cases
4. Update this README if new prerequisites are needed

## References

- [Kitty Documentation](https://sw.kovidgoyal.net/kitty/)
- [Kitty Remote Control](https://sw.kovidgoyal.net/kitty/remote-control/)
- [Agent Harbor TUI PRD](../../TUI-PRD.md)
- [Multiplexer Template](./Multiplexer-Description-Template.md)
