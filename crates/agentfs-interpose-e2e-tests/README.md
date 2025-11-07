# AgentFS Interpose E2E Tests

End-to-end integration tests for AgentFS interpose functionality. This crate provides comprehensive testing of the interpose layer that intercepts filesystem operations and forwards them to the AgentFS daemon.

## Architecture

The test suite includes both the interpose shim library and comprehensive test programs:

- **`lib.rs`**: Test utilities and daemon path resolution
- **`handshake.rs`**: Handshake protocol (removed - now in agentfs-daemon)
- **`src/bin/`**: Test programs
  - `test_helper.rs`: Direct libc call test program

## Daemon Integration

The test suite uses the production AgentFS daemon (`agentfs-daemon`) for all testing. The daemon must be built before running tests.

## Test Coverage

The test suite covers:

- **Basic Operations**: File open, read, write, close
- **Directory Operations**: Directory reading, creation, removal
- **Symlinks**: Symlink creation and reading
- **Metadata**: File attributes, permissions, timestamps
- **Interpose Mechanics**: Function hooking, request forwarding
- **Handshake Protocol**: Connection establishment and process registration
- **Error Handling**: Malformed requests, permission issues, file not found

## Running Tests

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_file_operations

# Run with verbose output
cargo test -- --nocapture
```

## Building Test Dependencies

The test suite requires building the test helper and daemon binaries:

```bash
# Build test helper
cargo build --bin test_helper

# Build the production daemon
cargo build -p agentfs-daemon --bin agentfs-daemon
```

## Integration with Production Daemon

The test suite exclusively uses the production `agentfs-daemon` binary, providing:

- **Real Filesystem Semantics**: Tests against actual AgentFS core implementation
- **Production Validation**: Ensures compatibility with shipping daemon
- **Early Issue Detection**: Catches integration issues before deployment

## Test Programs

### test_helper

A test program that makes direct libc calls to exercise interpose hooks:

```rust
// Makes direct syscall without going through std::fs
libc::open(c_path, flags, mode)
```

### Daemon

The test suite uses the production `agentfs-daemon` from the `agentfs-daemon` crate, which provides full AgentFS core functionality for testing.

## Dependencies

- `agentfs-daemon`: Production daemon library (for handshake types)
- `agentfs-core`: Core filesystem functionality
- `agentfs-proto`: Protocol definitions
- `tempfile`: Temporary file management
- `once_cell`: Lazy static initialization
- `libc`: Direct system call access

## Future Development

The test suite will expand to cover:

- Advanced interpose features (watcher translation, ACLs)
- Performance regression testing
- Multi-process scenarios
- Error injection and fault tolerance
- Integration with different filesystem backends
