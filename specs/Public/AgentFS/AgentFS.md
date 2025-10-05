# AgentFS — Cross-Platform Filesystem Snapshots and Per-Process Mounting

## Purpose

AgentFS implements the necessary filesystem snapshot and per-process mounting capabilities on macOS and Windows platforms, where native operating system support for these features is limited or absent. Linux provides these capabilities natively through mount namespaces and filesystem-level snapshots (ZFS/Btrfs/OverlayFS), but macOS and Windows require user-space implementations.

## Key Capabilities

### Filesystem Snapshots

- **Copy-on-Write (CoW) Snapshots**: Create point-in-time snapshots of the entire filesystem state without duplicating data
- **Writable Branches**: Create independent, diverging versions (branches) from any snapshot
- **Memory-Efficient Storage**: Primarily in-memory with transparent disk spillover for large files
- **Cross-Platform Implementation**: Core Rust library with platform-specific glue layers

### Per-Process Mounting

- **Process-Scoped Views**: Each process can have its own isolated filesystem view (branch)
- **Isolation**: Changes in one branch are invisible to processes in other branches
- **Platform-Specific Integration**:
  - **macOS**: Uses chroot with overlay mounting
  - **Windows**: Implements drive letter or mount point isolation
  - **Linux**: Leverages native mount namespaces (complementary to existing capabilities)

## Implementation Architecture

- **Core Library**: Rust-based filesystem logic with comprehensive operations support and per-process branch binding
- **Platform Glue Layers**:
  - Linux: FUSE integration with ioctl-based control plane
  - Windows: WinFsp integration with DeviceIoControl control plane
  - macOS: FSKit Unary File System extension with XPC control service
- **Snapshot Model**: CoW mechanism ensuring efficient storage and fast snapshot creation
- **Branch Isolation**: Per-process branch binding allows different processes to see different filesystem branches concurrently

## Current Implementation Approach

Unlike traditional overlay filesystems or mount namespace simulation, AgentFS implements **per-process branch binding directly in the core filesystem logic**. This approach provides:

- **Native Branch Isolation**: Each process can be bound to a specific filesystem branch at runtime
- **Cross-Platform Consistency**: Same branch binding mechanism works across all supported platforms
- **Efficient Resource Usage**: No need for multiple filesystem mounts or overlay layers
- **Direct Control**: Branch binding is managed through platform-specific control interfaces (XPC/ioctl/DeviceIoControl)

## Use Cases

- **Isolated Agent Execution**: Each AI agent runs in its own filesystem branch
- **Multi-Version Testing**: Test applications against different filesystem states
- **Development Sandboxes**: Create isolated development environments
- **Cross-Platform Consistency**: Uniform filesystem behavior across all supported platforms

## Files in This Directory

- [AgentFS: Per-process FS mounts](AgentFS-Per-process-FS-mounts.md): Detailed specification for per-process mount namespace simulation
- [AgentFS: Snapshots and Branching](AgentFS-Snapshots-and-Branching.md): Comprehensive specification for snapshot and branching functionality

Implementation Status: See [AgentFS.status.md](AgentFS.status.md) for current milestones, tasks, and success criteria.
