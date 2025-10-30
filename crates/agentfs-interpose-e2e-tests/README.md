# AgentFS Interposition E2E Tests

End-to-end tests for the AgentFS interposition system using the real `agentfs-daemon`.

## Overview

This crate contains comprehensive integration tests that verify AgentFS functionality by:

1. Starting the real `agentfs-daemon` binary
2. Connecting to it via Unix domain sockets
3. Performing filesystem operations through the interposed APIs
4. Verifying correct event delivery and state management

## Architecture

The tests use a thin wrapper around the `agentfs-daemon` library to provide testing-specific behaviors:

- **Test Helper Binary**: Interposes filesystem calls and communicates with the daemon
- **Daemon Integration**: Uses the production `agentfs-daemon` binary instead of mocks
- **State Verification**: Captures and verifies filesystem state changes

## Test Categories

### Filesystem Operations

- File creation, reading, writing, deletion
- Directory operations (mkdir, rmdir, readdir)
- Symlink creation and resolution
- Permission and attribute management

### Event Delivery

- kqueue watch registration and event delivery
- FSEvents stream setup and event fanout
- Path-based event filtering and matching

### Overlay Semantics

- Upper/lower directory interactions
- Copy-up operations
- Whiteout handling

## Running Tests

Build the daemon first:

```bash
cargo build --bin agentfs-daemon
```

Run the e2e tests:

```bash
cargo test -p agentfs-interpose-e2e-tests
```

## Test Infrastructure

### Daemon Management

- `find_daemon_path()`: Locates the built `agentfs-daemon` binary
- `start_daemon()`: Launches daemon with Unix socket communication
- `start_overlay_daemon()`: Launches daemon with overlay configuration (future)

### State Capture

- Filesystem state queries via daemon IPC
- Directory tree traversal and content capture
- Process and statistics monitoring

### Event Verification

- Synthetic event injection for testing
- Watch registration verification
- Event routing and delivery confirmation

## Dependencies

- `agentfs-daemon`: The production daemon binary
- `agentfs-proto`: Message definitions and serialization
- `agentfs-core`: Filesystem core functionality
- Various testing utilities (tempfile, etc.)

## Future Enhancements

- Overlay filesystem testing (once daemon supports it)
- Performance benchmarking
- Stress testing with multiple concurrent clients
