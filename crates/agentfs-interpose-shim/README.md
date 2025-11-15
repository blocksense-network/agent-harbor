# agentfs-interpose-shim

Minimal DYLD interposer shim for macOS that bootstraps AgentFS interpose sessions.

## Overview

The AgentFS interpose shim is a dynamic library that intercepts filesystem operations in macOS processes. It is designed to be loaded via `DYLD_INSERT_LIBRARIES` and provides a mechanism for supervisor processes (like the AH CLI) to transparently overlay filesystem operations with AgentFS semantics.

### Intended Usage Pattern

1. **Supervisor Setup**: A supervisor process (e.g., AH CLI) sets up environment variables and spawns child processes with `DYLD_INSERT_LIBRARIES` pointing to this shim.

2. **Automatic Handshake**: When a child process loads the shim, it automatically connects to the AgentFS daemon socket and performs a handshake.

3. **Process Binding**: If `AGENTFS_BRANCH_ID` is set, the shim automatically binds the process to the specified AgentFS branch.

4. **Transparent Interception**: All filesystem operations in the child process are intercepted and routed through the AgentFS daemon, enabling features like copy-on-write overlays, snapshots, and branch isolation.

## Features

- Loads via `DYLD_INSERT_LIBRARIES` environment variable.
- Performs a guarded handshake with the AgentFS control socket.
- Optional allow-list to restrict which executables activate the shim (development safety).
- Automatic process-to-branch binding based on environment configuration.
- Graceful degradation when the daemon is unavailable.

## Environment Variables

The shim is configured entirely through environment variables, which must be set by the supervisor process before spawning child processes.

| Variable                      | Description                                                                                                    | Default   | Required |
| ----------------------------- | -------------------------------------------------------------------------------------------------------------- | --------- | -------- |
| `AGENTFS_INTERPOSE_ENABLED`   | Enable (`1`/`true`) or disable (`0`/`false`) the shim.                                                         | `true`    | No       |
| `AGENTFS_INTERPOSE_SOCKET`    | Path to the AgentFS control UNIX-domain socket. Required for handshake.                                        | _(unset)_ | **Yes**  |
| `AGENTFS_INTERPOSE_ALLOWLIST` | Comma-separated allow-list of executable basenames or path fragments. `*` allows all.                          | `*`       | No       |
| `AGENTFS_INTERPOSE_LOG`       | When set to `0`/`false`, suppress shim diagnostics. Any other value keeps logging enabled.                     | `true`    | No       |
| `AGENTFS_INTERPOSE_FAIL_FAST` | When set to `1`/`true`, terminate the process immediately on handshake failure. Otherwise, degrade gracefully. | `false`   | No       |
| `AGENTFS_BRANCH_ID`           | Branch ID string for automatic process-to-branch binding. If set, the shim binds the process to this branch.   | _(unset)_ | No       |

### Supervisor Process Setup Example

```bash
# Set up environment for child process
export DYLD_INSERT_LIBRARIES="/path/to/libagentfs_interpose_shim.dylib"
export AGENTFS_INTERPOSE_SOCKET="/tmp/agentfs-daemon.sock"
export AGENTFS_BRANCH_ID="branch-abc123"
export AGENTFS_INTERPOSE_ALLOWLIST="myapp,otherapp"

# Spawn child process with interception enabled
/path/to/child/process --args
```

## Handshake Protocol

On initialization, the shim:

1. **Checks Allow-list**: Verifies the current executable is allowed to load the shim.
2. **Connects to Daemon**: Establishes a connection to the AgentFS daemon via the UNIX socket specified in `AGENTFS_INTERPOSE_SOCKET`.
3. **Performs Handshake**: Sends a structured handshake message containing process metadata, shim version, and configuration details.
4. **Receives Acknowledgment**: Waits for daemon acknowledgment (simple "OK\n" response).
5. **Binds to Branch**: If `AGENTFS_BRANCH_ID` is set, automatically binds the current process to the specified branch.
6. **Starts Interception**: Begins intercepting filesystem operations and forwarding them to the daemon.

The connection is kept alive for the lifetime of the process to enable ongoing coordination. If handshake fails and `AGENTFS_INTERPOSE_FAIL_FAST` is set, the process terminates immediately. Otherwise, the shim degrades gracefully and allows the process to continue without interception.

## Error Handling

- **Socket Unavailable**: If the daemon socket doesn't exist or is unreachable, the shim either terminates (fail-fast mode) or continues without interception.
- **Handshake Failure**: Similar to socket issues, either terminates or degrades gracefully.
- **Branch Binding Failure**: Logged as a warning but doesn't prevent interception from working.
- **Allow-list Rejection**: Process continues normally without loading the shim.

## Development and Testing

```bash
cargo test -p agentfs-interpose-shim
```

The unit tests verify allow-list logic and basic functionality. Integration testing requires:

1. Building the shim as a dynamic library (`cargo build --release`)
2. Setting up a mock AgentFS daemon (see `tests/fixtures/mock_daemon.rs`)
3. Launching test processes with `DYLD_INSERT_LIBRARIES` pointing to the shim
4. Verifying that filesystem operations are properly intercepted and forwarded

### Test Environment Setup

```bash
# Build the shim
cargo build --release -p agentfs-interpose-shim

# Start mock daemon
cargo run --bin mock_daemon /tmp/test-agentfs.sock

# In another terminal, test with interception
DYLD_INSERT_LIBRARIES=./target/release/libagentfs_interpose_shim.dylib \
AGENTFS_INTERPOSE_SOCKET=/tmp/test-agentfs.sock \
./target/release/test_helper basic-open /some/test/file
```
