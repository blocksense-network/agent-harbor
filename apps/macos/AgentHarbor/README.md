# AgentHarbor macOS Application

This directory contains the main macOS application for the Agents Workflow project, which serves as a host for system extensions including the AgentFSKitExtension filesystem extension.

## Project Structure

```
AgentHarbor/
├── AgentHarbor.xcodeproj/          # Xcode project file
├── AgentHarbor/                    # Host application source code
│   ├── main.swift                     # Application entry point
│   ├── AppDelegate.swift              # Application delegate
│   ├── MainViewController.swift       # Main UI controller
│   ├── Info.plist                     # Application metadata
│   └── AgentHarbor.entitlements    # Code signing entitlements
├── PlugIns/                           # Embedded system extensions
│   └── AgentFSKitExtension.appex/     # FSKit filesystem extension
│       ├── AgentFSKitExtension.swift  # Extension main class
│       ├── AgentFsUnary.swift         # FSUnaryFileSystem implementation
│       ├── AgentFsVolume.swift        # Volume operations
│       ├── AgentFsItem.swift          # File/directory items
│       ├── AgentFSBridge.c            # C bridge functions
│       ├── AgentFSKitFFI.h            # FFI header
│       ├── Constants.swift            # Extension constants
│       ├── Info.plist                 # Extension metadata
│       └── AgentFSKitExtension.entitlements # Extension entitlements
├── libs/                              # Universal Rust libraries (generated)
├── build-universal.sh                 # Build script for universal binaries
└── README.md                          # This file
```

## Building the Project

### Prerequisites

- macOS 15.4+ with Xcode 15+
- Rust toolchain with targets: `aarch64-apple-darwin` and `x86_64-apple-darwin`
- `cbindgen` for generating C headers (optional)

### Building Universal Binaries

The project supports universal binaries for both Intel and Apple Silicon Macs. Use the provided build script:

```bash
./build-universal.sh
```

This script will:

1. Build all required Rust crates for both architectures
2. Create universal binaries using `lipo`
3. Generate C headers for FFI
4. Build the Xcode project

### Manual Xcode Build

You can also build directly with Xcode:

```bash
xcodebuild -project AgentHarbor.xcodeproj -scheme AgentHarbor -configuration Release build
```

## Code Signing and Entitlements

### Host Application Entitlements

- App Sandbox enabled
- Application group for shared data
- System extension install permission
- User-selected file access

### Extension Entitlements

- FSKit filesystem module entitlement
- App Sandbox enabled

## System Extension Registration

The AgentHarbor application automatically registers embedded system extensions when launched. However, macOS requires explicit user approval for system extensions to be activated.

### Automatic Registration (macOS 13.0+)

On macOS 13.0 and later, the application will automatically request system extension approval when launched. The approval process involves:

1. Launch the AgentHarbor application
2. A system dialog will appear requesting permission to install the filesystem extension
3. Click "Allow" to approve the extension

### Diagnostic Mode

For CI/testing purposes, the application supports a diagnostic mode that performs extension validation without launching the GUI:

```bash
# Run diagnostic checks
./AgentHarbor.app/Contents/MacOS/AgentHarbor --diagnostic

# Or use the short form
./AgentHarbor.app/Contents/MacOS/AgentHarbor -d
```

The diagnostic mode performs comprehensive validation of the extension bundle:

**Bundle Structure Validation:**

- ✅ Extension bundle exists and loads correctly
- ✅ All required Info.plist keys are present (`CFBundleIdentifier`, `CFBundleName`, etc.)
- ✅ NSExtension configuration is properly structured
- ✅ Extension point identifier is correct (`com.apple.filesystems`)

**Executable Validation:**

- ✅ Extension executable exists and is readable
- ✅ Executable size is within expected range
- ✅ Bundle identifier matches expected values

**System Compatibility:**

- ✅ Extension conforms to macOS system extension requirements
- ✅ Bundle structure follows Apple's extension guidelines

**Exit codes:**

- `0`: All checks passed - extension properly embedded
- `1`: One or more checks failed - build or embedding issue detected

This is ideal for automated testing and CI pipelines to verify that builds include the required extensions.

### Manual Approval Process

If automatic registration fails or on older macOS versions, manually approve the extension:

1. Launch the AgentHarbor application
2. Open System Settings
3. Navigate to General > Login Items & Extensions
4. Select "File System Extensions" from the sidebar
5. Find "AgentFSKitExtension" in the list
6. Toggle the switch to enable the extension
7. If prompted, enter your administrator password

### Extension Status Monitoring

The application provides real-time status monitoring of the filesystem extension:

- **Active**: Extension is loaded and functioning
- **Pending Reboot**: Extension will activate after system restart
- **Failed**: Extension approval was denied or encountered an error
- **Not Found**: Extension bundle could not be located

### Troubleshooting Extension Issues

**Extension Not Appearing in System Settings:**

- Ensure the host application has been launched at least once
- Check that the application has the necessary entitlements
- Verify the extension bundle is properly embedded in the app

**Extension Approval Denied:**

- Check System Settings > Privacy & Security for blocked extensions
- Restart the system and try again
- Ensure you're running macOS 15.4+ (required for FSKit)

**Extension Loading Errors:**

- Check Console.app for detailed error messages
- Verify all Rust libraries are properly linked
- Ensure code signing is valid (required for system extensions)

## Testing

### Mount Testing

Once the extension is enabled, you can test mounting:

```bash
# Create a test device
mkfile -n 100m test.img
hdiutil attach -imagekey diskimage-class=CRawDiskImage -nomount test.img

# Mount using the extension
sudo mount -F -t AgentFS diskN /tmp/test-mount
```

### Development Mode

For development, you can temporarily disable code signing:

```bash
xcodebuild -project AgentHarbor.xcodeproj -scheme AgentHarbor build CODE_SIGNING_ALLOWED=NO
```

## Troubleshooting

### Extension Not Appearing

- Ensure the host app has been launched at least once
- Check System Settings > Privacy & Security for approval prompts
- Verify code signing and entitlements

### Build Failures

- Ensure all Rust targets are installed: `rustup target add aarch64-apple-darwin x86_64-apple-darwin`
- Check that universal libraries were built correctly
- Verify Xcode command line tools are installed

### Mount Failures

- Check Console.app for extension logs
- Ensure the extension is enabled in System Settings
- Verify the FSShortName matches the mount command

## Architecture Notes

- The host application uses standard macOS AppKit
- Extensions are embedded as PlugIns/ in the app bundle
- Rust libraries are linked as universal static libraries
- FFI bridging uses C headers generated by cbindgen
