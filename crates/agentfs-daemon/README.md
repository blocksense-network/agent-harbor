# AgentFS Daemon

The AgentFS Daemon is the core component that manages file system event watching and distribution for the AgentFS system.

## Architecture

The daemon is structured as a library with a thin executable wrapper:

- **Library (`src/lib.rs`)**: Contains the core `AgentFsDaemon`, `WatchService`, and `DaemonEventSink` implementations
- **Executable (`src/main.rs`)**: Simple Unix socket server that accepts watch registration requests

## Features

- **Watch Service**: Manages kqueue and FSEvents watch registrations per process
- **Event Fanout**: Routes FsCore events to registered watchers based on path matching
- **IPC Interface**: Handles watch registration/unregistration via SSZ-encoded messages
- **Path Derivation**: Uses F_GETPATH to derive file paths from file descriptors

## Usage

### As a Library

```rust
use agentfs_core::{FsConfig, config::BackstoreMode};
use agentfs_daemon::AgentFsDaemon;

// Create FsCore
let config = FsConfig {
    track_events: true,
    backstore: BackstoreMode::InMemory,
    ..Default::default()
};
let core = agentfs_core::FsCore::new(config)?;

// Create daemon
let daemon = AgentFsDaemon::new(core)?;
daemon.subscribe_events()?;

// Handle watch requests
let response = daemon.handle_watch_request(&request)?;
```

### As an Executable

```bash
# Start daemon with default socket
./target/debug/agentfs-daemon

# Start daemon with custom socket
AGENTFS_DAEMON_SOCKET=/tmp/my-socket.sock ./target/debug/agentfs-daemon
```

## Protocol

The daemon communicates via Unix domain sockets using SSZ-encoded messages defined in `agentfs-proto`. Supported operations:

- `WatchRegisterKqueue`: Register kqueue-based file watches
- `WatchRegisterFSEvents`: Register FSEvents-based directory watches
- `WatchUnregister`: Unregister watches
- `WatchDoorbell`: Set doorbell identifiers
- `FsEventBroadcast`: Inject synthetic events for testing

## Testing

Run the daemon tests:

```bash
cargo test -p agentfs-daemon
```

The integration tests verify end-to-end event delivery between external processes and the daemon.
