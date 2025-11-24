# AgentFS Daemon

The AgentFS Daemon is a production-ready filesystem daemon that provides interpose services for AgentFS. It acts as a library that can be embedded in executables or used as a standalone daemon.

## Architecture

The daemon is structured as a Rust library with a thin executable wrapper:

- **`lib.rs`**: Library interface and public API
- **`daemon.rs`**: Main daemon implementation with interpose request handling
- **`handshake.rs`**: Handshake protocol for client connections
- **`watch_service.rs`**: Watch service functionality for file system events
- **`bin/agentfs-daemon.rs`**: Thin executable wrapper that packages the library

## Usage as a Library

```rust
use agentfs_daemon::AgentFsDaemon;

let daemon = AgentFsDaemon::new()?;

// Handle interpose requests
let fd = daemon.handle_fd_open("/path/to/file".to_string(), 0, 0644, 12345)?;
let target = daemon.handle_readlink("/path/to/symlink".to_string(), 12345)?;
```

## Usage as an Executable

The daemon can be run as a standalone executable:

```bash
cargo run --bin agentfs-daemon /path/to/socket
```

Or build and run:

```bash
cargo build --release --bin agentfs-daemon
./target/release/agentfs-daemon /tmp/agentfs.sock
```

## Configuration

The daemon supports overlay filesystem configuration and logging:

```bash
# Basic daemon
./agentfs-daemon /tmp/agentfs.sock

# With overlay filesystem
./agentfs-daemon /tmp/agentfs.sock --lower-dir /path/to/lower --upper-dir /path/to/upper
```

### Logging Options

The daemon supports configurable logging output:

```bash
# Log to console (default)
./agentfs-daemon /tmp/agentfs.sock

# Set log level
./agentfs-daemon /tmp/agentfs.sock --log-level debug

# Log to file
./agentfs-daemon /tmp/agentfs.sock --log-dir /var/log --log-file agentfs-daemon.log

# Combined options
./agentfs-daemon /tmp/agentfs.sock --log-level info --log-dir /tmp --log-file agentfs.log
```

#### Log Level Options

- `error`: Only error messages
- `warn`: Warnings and errors (default)
- `info`: Informational messages, warnings, and errors
- `debug`: Debug information, plus all above levels
- `trace`: Detailed tracing, plus all above levels

#### Log Output Options

- `--log-dir <path>`: Directory to write log files to
- `--log-file <filename>`: Log filename within the log directory

If `--log-file` contains directory components, they are appended to `--log-dir`. If `--log-file` is an absolute path, the directory from `--log-file` takes precedence over `--log-dir`.

## Protocol

The daemon communicates with clients using SSZ-encoded messages over Unix domain sockets. The protocol includes:

- **Handshake**: Initial connection establishment with process information
- **Interpose Requests**: File operations (open, read, write, stat, etc.)
- **Directory Operations**: Directory reading and manipulation
- **Metadata Operations**: Extended attributes, permissions, timestamps
- **Filesystem State**: Daemon introspection and statistics

## Testing Integration

The daemon integrates with the AgentFS interpose e2e test suite. The test suite automatically detects and uses the new daemon binary if available, falling back to the legacy mock daemon for backward compatibility.

## Dependencies

- `agentfs-core`: Core filesystem functionality
- `agentfs-proto`: Protocol message definitions
- `ssz`: Serialization (ethereum_ssz)
- `tokio`: Async runtime (for future extensions)

## Future Extensions

The daemon architecture supports future extensions such as:

- Watch services for file system event notification
- Advanced caching and performance optimizations
- Multi-process coordination
- Persistent state management
