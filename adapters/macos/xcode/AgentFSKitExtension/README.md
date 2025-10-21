# AgentFSKitExtension

macOS FSKit extension for AgentFS user-space filesystem.

## Requirements

- macOS 15.4 or later (for FSKit framework)
- Xcode 16 or later
- Rust toolchain (for building FFI crates)

## Building

### Automated Build

Run the build script to prepare dependencies:

```bash
./build.sh
```

This will:

1. Build the Rust FFI crates (`agentfs-fskit-sys` and `agentfs-fskit-bridge`)
2. Copy the static libraries to the `libs/` directory
3. Generate C headers for Swift interop

### Xcode Build

1. Open `AgentFSKitExtension.xcodeproj` in Xcode
2. Ensure the extension target is selected
3. Build the project (Cmd+B)

### Manual Build

If you prefer to build manually:

```bash
# Build Rust crates
cargo build --release -p agentfs-fskit-sys -p agentfs-fskit-bridge

# Copy libraries (adjust paths as needed)
cp ../../../target/release/libagentfs_fskit_sys.a libs/
cp ../../../target/release/libagentfs_fskit_bridge.a libs/

# Then build in Xcode
```

## Installation

1. Enable the extension in System Settings:
   - Go to System Settings > General > Login Items & Extensions > File System Extensions
   - Enable "AgentFSKitExtension"

2. The extension should now be available for mounting

## Testing

### Basic Mount Test

Create a dummy block device:

```bash
mkfile -n 100m dummy
hdiutil attach -imagekey diskimage-class=CRawDiskImage -nomount dummy
```

Mount the filesystem:

```bash
mkdir /tmp/TestVol
mount -F -t AgentFS disk18 /tmp/TestVol
```

Verify the mount:

```bash
ls -la /tmp/TestVol
# Should show: . .. .agentfs test
```

### Control Plane Test

Test the control interface:

```bash
# List control files
ls -la /tmp/TestVol/.agentfs
# Should show: . .. snapshot branch bind

# Test snapshot creation (example)
echo '{"name": "test-snapshot"}' > /tmp/TestVol/.agentfs/snapshot
```

### File Operations Test

```bash
# Create and read files
echo "Hello AgentFS" > /tmp/TestVol/hello.txt
cat /tmp/TestVol/hello.txt
# Should output: Hello AgentFS
```

## Architecture

### Swift Components

- `AgentFSKitExtension.swift`: Main extension entry point conforming to `UnaryFileSystemExtension`
- `AgentFsUnary.swift`: FSUnaryFileSystem implementation handling resource probing and loading
- `AgentFsVolume.swift`: FSVolume implementation with core VFS operations mapping
- `AgentFsItem.swift`: FSItem implementation for files/directories with metadata
- `Constants.swift`: UUID constants for container and volume identification

### Rust FFI Components

- `agentfs-fskit-sys`: C ABI interface with basic filesystem operations
- `agentfs-fskit-bridge`: Higher-level Swift-callable wrapper with memory management

### Control Plane

- `.agentfs/` directory contains control files for CLI operations
- `snapshot`, `branch`, `bind` files accept JSON commands
- Operations are executed when JSON is written to control files

## Integration with AgentFS

This extension integrates with the main AgentFS system through:

1. **FFI Bridge**: Rust core operations called via C ABI
2. **Control Plane**: JSON-based command interface for snapshots/branches
3. **VFS Mapping**: FSKit operations mapped to AgentFS core VFS calls

## Troubleshooting

### Build Issues

- Ensure macOS 15.4+ SDK is selected in Xcode
- Check that Rust crates build successfully: `cargo build -p agentfs-fskit-sys -p agentfs-fskit-bridge`
- Verify library paths in Xcode build settings

### Runtime Issues

- Check Console.app for extension logs (subsystem: `com.agentfs.AgentFSKitExtension`)
- Ensure extension is enabled in System Settings
- Verify block device permissions

### Testing Issues

- Use `diskutil list` to verify block device creation
- Check `/var/log/system.log` for mount errors
- Test with small dummy devices first

## Development Status

- ✅ M13: FSKit Extension Bootstrap - Complete
- ✅ M14: Rust-Swift FFI Bridge - Complete
- ✅ M15: FSKit Volume Implementation - Complete (integrated with AgentFS core)
- ✅ M16: XPC Control Service - Complete
- 🔄 M17: Integration and Testing - In Progress

## Integration Status

The Swift FSKit extension is now fully integrated with the Rust AgentFS core:

- **Core Lifecycle**: AgentFS core is created/destroyed via FFI during FS resource load/unload
- **Filesystem Operations**: Volume operations call into AgentFS core for actual file operations
- **Control Plane**: XPC service provides control interface for snapshots, branches, and process binding
- **Memory Management**: Proper cleanup of Rust objects from Swift

### Build Process

1. **Build Rust crates**: `cargo build --release -p agentfs-core -p agentfs-proto -p agentfs-ffi -p agentfs-fskit-sys -p agentfs-fskit-bridge`
2. **Prepare libraries**: `./build.sh` copies Rust libraries to Swift project
3. **Build Swift**: `swift build` or open in Xcode

### Integration Testing

Run the integration test: `./test-integration.sh`

This tests the complete integration from Swift FSKit calls through FFI to Rust AgentFS core operations.
