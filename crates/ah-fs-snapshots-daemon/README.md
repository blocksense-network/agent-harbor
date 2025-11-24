# AH Filesystem Snapshots Daemon

The AH Filesystem Snapshots Daemon is a privileged service that provides filesystem snapshot and mount operations for Agent Harbor. It eliminates the need for users to run `sudo` directly by handling privileged filesystem operations (ZFS snapshots, Btrfs subvolumes, and AgentFS mounts) on behalf of client applications.

The daemon implements the filesystem snapshot provider matrix described in [FS-Snapshots-Overview.md](../../specs/Public/FS-Snapshots/FS-Snapshots-Overview.md), providing unified abstractions for cross-platform filesystem isolation and time-travel capabilities.

## Architecture

The daemon provides a Unix socket interface for privileged filesystem operations that implement the `FsSnapshotProvider` trait from the FS-Snapshots specification:

### Filesystem Providers

- **ZFS**: Creates snapshots, clones, and manages ZFS datasets with automatic mountpoint detection
- **Btrfs**: Creates subvolume snapshots and manages Btrfs filesystem operations
- **AgentFS**: Manages FUSE mounts on Linux and interposition mounts on macOS

### Core Operations

- **Filesystem snapshots**: Create point-in-time snapshots for time-travel functionality
- **Workspace preparation**: Set up isolated working copies using CoW (copy-on-write) semantics
- **Mount management**: Handle FUSE and interposition mounts with proper lifecycle management
- **Privilege escalation**: Execute privileged commands (zfs, btrfs, mount) using sudo when not running as root

### Security Model

The daemon is designed to run with elevated privileges and provides controlled access to filesystem operations. In production deployments, socket access should be restricted to users belonging to the `agentfs` group, ensuring only authorized applications can create mounts and snapshots.

## Usage

### Starting the Daemon

```bash
# Basic startup
./ah-fs-snapshots-daemon

# With custom socket path
./ah-fs-snapshots-daemon --socket-path /tmp/custom.sock

# With debug logging
./ah-fs-snapshots-daemon --log-level debug --log-to-file
```

### Client Communication

Clients communicate with the daemon using SSZ-encoded messages over Unix domain sockets:

```rust
use ah_fs_snapshots_daemon::client::DaemonClient;

let client = DaemonClient::new()?;
let status = client.mount_agentfs_interpose(request).await?;
```

## Configuration

### Command Line Options

- `--socket-path <path>`: Unix socket path for client connections (default: `/tmp/agent-harbor/ah-fs-snapshots-daemon`)
- `--log-level <level>`: Logging verbosity (`error`, `warn`, `info`, `debug`, `trace`)
- `--log-to-file`: Write logs to file instead of console

### Logging

The daemon supports configurable logging that is inherited by spawned AgentFS daemons:

```bash
# Console logging (default)
./ah-fs-snapshots-daemon --log-level info

# File logging with debug level
./ah-fs-snapshots-daemon --log-level debug --log-to-file
```

When `--log-to-file` is enabled, logs are written to `~/Library/Logs/ah-fs-snapshots-daemon.log` on macOS or the platform-standard location on other systems.

## AgentFS Daemon Integration

When handling interpose mount requests, the daemon spawns `agentfs-daemon` processes with matching logging configuration:

```bash
# Daemon spawns with inherited logging:
agentfs-daemon /tmp/agentfs-interpose/agentfs.sock \
  --lower-dir /path/to/repo \
  --owner-uid 501 \
  --owner-gid 20 \
  --log-level debug \
  --log-dir /tmp/agentfs-interpose \
  --log-file agentfs-daemon.log
```

The AgentFS daemon logging options are:

- `--log-level`: Matches the main daemon's log level
- `--log-dir`: Set to the AgentFS runtime directory
- `--log-file`: Set to `agentfs-daemon.log`

## Protocol

The daemon uses SSZ-encoded messages for type-safe client communication over Unix domain sockets:

### ZFS Operations

- `ListZfsSnapshots`: List all snapshots for a ZFS dataset
- `SnapshotZfs`: Create a new ZFS snapshot from a dataset
- `CloneZfs`: Create a ZFS clone from an existing snapshot
- `DeleteZfs`: Delete a ZFS dataset (snapshot or clone)

### Btrfs Operations

- `SnapshotBtrfs`: Create a Btrfs subvolume snapshot (clone)
- `CloneBtrfs`: Create a Btrfs subvolume snapshot (alias for snapshot)
- `DeleteBtrfs`: Delete a Btrfs subvolume

### AgentFS Operations

- `MountAgentfsFuse`: Mount a FUSE filesystem on Linux
- `MountAgentfsInterpose`: Mount an interposition filesystem on macOS
- `MountAgentfsInterposeWithHints`: Mount with explicit socket/runtime paths
- `UnmountAgentfsFuse`: Unmount a FUSE filesystem
- `UnmountAgentfsInterpose`: Unmount an interposition filesystem
- `StatusAgentfsFuse`: Get FUSE mount status
- `StatusAgentfsInterpose`: Get interposition mount status

### Utility

- `Ping`: Health check request

### Responses

- `Success`: Operation completed successfully
- `SuccessWithMountpoint`: Success with ZFS mountpoint information
- `SuccessWithPath`: Success with filesystem path information
- `SuccessWithList`: Success with JSON-encoded list (e.g., snapshot names)
- `Error`: Operation failed with error message
- `AgentfsFuseStatus`: Detailed FUSE mount status
- `AgentfsInterposeStatus`: Detailed interposition mount status

## Privilege Escalation and Security

The daemon's primary purpose is to eliminate the need for users to run `sudo` directly when performing filesystem operations. When not running as root, it automatically prefixes privileged commands with `sudo -n` to execute them without interactive password prompts.

### Production Deployment

In production environments, the daemon should be configured with restricted socket access:

1. **Group-based access control**: Only users belonging to the `agentfs` group should be able to connect to the daemon socket
2. **Socket permissions**: The Unix socket should have appropriate permissions (e.g., `srwxrwx---` with group ownership set to `agentfs`)
3. **Process isolation**: The daemon should run as a dedicated system user with minimal privileges beyond filesystem operations

This model ensures that Agent Harbor applications can create isolated workspaces and snapshots without requiring end users to have sudo privileges.

## Error Handling

The daemon provides comprehensive error reporting:

- Filesystem operation failures include detailed error messages from underlying tools (zfs, btrfs)
- Mount failures include process spawn details and timeout information
- Socket connection issues are logged with retry information
- Validation errors for non-existent datasets/subvolumes are clearly reported

## Implementation of FS-Snapshots Specification

This daemon implements the filesystem snapshot provider matrix defined in [FS-Snapshots-Overview.md](../../specs/Public/FS-Snapshots/FS-Snapshots-Overview.md). It provides concrete implementations for:

- **ZFS Provider**: Native CoW snapshots with automatic mountpoint detection
- **Btrfs Provider**: Subvolume snapshots with ownership preservation
- **AgentFS Provider**: Cross-platform user-space filesystem with FUSE (Linux) and interposition (macOS) support

The daemon enables the `CowOverlay` working copy mode on Linux through privileged mount operations, while providing `Worktree` and `InPlace` modes as fallbacks.

## Dependencies

- `ah-logging`: Unified logging infrastructure with file/console output support
- `agentfs-core`: Core filesystem functionality for AgentFS operations
- `tokio`: Async runtime for concurrent client handling
- `ssz`: Ethereum SSZ serialization for type-safe network messages
- `libc`: Low-level system calls for privilege detection

## Development

### Building

```bash
cargo build --package ah-fs-snapshots-daemon --bins
```

### Testing

```bash
# Unit tests
cargo test -p ah-fs-snapshots-daemon

# Integration tests
cargo test -p ah-fs-snapshots
```

### Logging in Tests

For debugging test failures, enable verbose logging:

```bash
# Run daemon with debug logging during tests
RUST_LOG=debug cargo test -p ah-fs-snapshots --test agentfs_provider
```
