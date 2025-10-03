# Subcrates Design Pattern

## Overview

The agent-harbor project uses a **subcrates pattern** to organize related functionality into hierarchical crate structures. This pattern balances modularity, compile times, and maintainability while avoiding excessive crate proliferation.

## When to Use This Pattern

This pattern is particularly effective when creating **abstract interfaces** that have **concrete implementations** for various **third-party software**:

- **Terminal multiplexers**: tmux, zellij, screen, wezterm, kitty, etc.
- **Version control systems**: Git, Mercurial, Bazaar, Fossil
- **Code editors/IDEs**: VS Code, Cursor, Vim, Emacs, etc.
- **AI agents**: Claude Code, Codex, OpenHands, etc.
- **Filesystems**: ZFS, Btrfs, Git-based snapshots
- **Sandboxing backends**: Linux namespaces, Docker, Podman, etc.

The pattern allows us to:
- **Abstract over differences** between third-party tools while maintaining a consistent interface
- **Isolate platform/tool-specific code** in dedicated subcrates
- **Support optional compilation** of only needed backends
- **Easily add new implementations** without modifying existing code

## Core Concepts

### Monolith + Facades Strategy

We adopt a **monolith + facades** approach:

- **Monolith crate**: Contains all implementations, traits, and shared logic
- **Facade subcrates**: Tiny optional crates that re-export specific functionality
- **Feature-gated modules**: Implementation details are gated behind Cargo features

## Usage Patterns

### Direct Monolith Usage

Users can depend directly on the monolith crate and enable only the backends they need:

```toml
[dependencies]
ah-fs-snapshots = { version = "0.1", features = ["zfs", "btrfs"] }
```

```rust
// All enabled backends are available
use ah_fs_snapshots::{provider_for, FsSnapshotProvider};
```

### Facade Crate Usage

For applications that need only a specific backend, use the tiny facade crates:

```toml
[dependencies]
ah-fs-snapshots-zfs = "0.1"
```

```rust
// Only ZFS functionality is available
use ah_fs_snapshots_zfs::ZfsProvider;
```

### Automatic Feature Resolution

When multiple facade crates are combined in an application, Cargo automatically enables the correct features on the monolith crate:

```toml
[dependencies]
ah-fs-snapshots-zfs = "0.1"      # Enables "zfs" feature on monolith
ah-fs-snapshots-btrfs = "0.1"   # Enables "btrfs" feature on monolith
```

Cargo resolves this to:
```toml
ah-fs-snapshots = { features = ["zfs", "btrfs"] }  # Automatically determined
```

This ensures optimal compilation - only the requested backends are compiled while shared code is compiled once.

## Benefits

### Compile Time Optimization

Users have more control over the compilation time trade offs:

- **Single compilation unit** for shared logic in the monolith
- **Feature-gated implementations** avoid compiling unused backends
- **Facade crates compile instantly** when only re-exporting

### Maintainability

- **Clear separation** of concerns between abstraction and implementation
- **Platform-specific code** isolated in dedicated subcrates
- **Shared logic** lives in one place, reducing duplication

### Flexibility

- **Optional dependencies**: Users can include only needed backends
- **Testing isolation**: Each subcrate can be tested independently
- **Publishing freedom**: Subcrates can be published separately if needed

## Migration Strategy

When converting from Ruby implementations:

1. **Identify shared interfaces** → Monolith crate traits
2. **Extract platform-specific logic** → Facade subcrates
3. **Preserve Ruby API compatibility** → Integration tests
4. **Gradual rollout** → Feature flags for incremental adoption

## Testing Strategy

- **Unit tests** in each subcrate for isolated functionality
- **Integration tests** in monolith crate for cross-cutting concerns
- **Feature-gated test suites** to avoid compiling unused backends in CI

## Gotchas and Best Practices

- **Visibility changes**: Code moved under monolith modules; adjust `pub(crate)` declarations
- **Feature unification**: Multiple facades depending on the same monolith features work correctly
- **Version alignment**: Keep monolith and facade versions synchronized
- **Documentation**: Document feature requirements clearly for users

This pattern enables the agent-harbor project to scale to multiple platforms and backends while maintaining fast compile times and clear code organization.
