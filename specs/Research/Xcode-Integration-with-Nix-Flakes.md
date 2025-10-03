# Xcode Integration with Nix Flakes

## Problem Statement

When using Nix flakes for development environment management, integrating Xcode's command-line tools (`xcodebuild`) can be challenging due to environment isolation and conflicting toolchains.

## Root Cause

- Nix provides reproducible environments but isolates from system tools
- Xcode command-line tools may point to CLI-only installation instead of full Xcode
- Nix's `stdenv` includes wrapped compilers that conflict with Xcode's expectations

## Recommended Solution: composeXcodeWrapper (Option 2)

### Current Nix Flake Configuration

```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/979daf34c8cacebcd917d540070b52a3c2b9b16e";
    darwin.url = "github:LnL7/nix-darwin";
    darwin.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { self, nixpkgs, darwin }: let
    # Configuration for macOS development environment
    devShells = forAllSystems (system: let
      isDarwin = (import nixpkgs { inherit system; }).stdenv.isDarwin;
    in {
      default = if isDarwin then
        # macOS: Use regular mkShell with Apple SDK frameworks
        ((import nixpkgs { inherit system; }).mkShell {
          buildInputs = [
            # Apple SDK frameworks for macOS development
            (import nixpkgs { inherit system; }).darwin.apple_sdk.frameworks.CoreFoundation
            (import nixpkgs { inherit system; }).darwin.apple_sdk.frameworks.Security
            # TODO: Add xcodeenv wrapper when available in nixpkgs
          ] ++ [
            # Rust toolchain and other tools...
          ];

          shellHook = ''
            # Clean up environment variables that might point to wrong tools
            unset DEVELOPER_DIR
            unset SDKROOT
          '';
        })
      else
        # Linux: Use regular mkShell
        ((import nixpkgs { inherit system; }).mkShell {
          # Linux build inputs...
        });
    });
  };
}
```

**Note**: The `xcodeenv.composeXcodeWrapper` is not currently available in the nixpkgs version used by this project. The environment is configured to work with system-installed Xcode, and the Apple SDK frameworks are provided for compilation. Xcode command-line tools integration works through the system Xcode installation.

### System Requirements

1. **Install Full Xcode**:
   ```bash
   # Install from Mac App Store or developer portal
   sudo xcode-select -s /Applications/Xcode.app/Contents/Developer
   xcode-select -p  # Verify: should show /Applications/Xcode.app/Contents/Developer
   ```

2. **Enter Development Environment**:
   ```bash
   nix develop
   xcodebuild -version  # Should work now
   ```

### Key Components

- **`mkShellNoCC`**: Uses shell without Nix's compiler toolchain to avoid conflicts
- **`composeXcodeWrapper`**: Creates lightweight wrapper to system Xcode without copying
- **`apple_sdk.frameworks`**: Provides macOS/iOS SDK headers and libraries
- **`shellHook`**: Cleans up environment variables that might point to wrong tools

### Advantages

- ✅ Maintains Nix flake reproducibility
- ✅ Avoids copying large Xcode installation into Nix store
- ✅ Proper integration with system Xcode
- ✅ Works across different developer machines

### Troubleshooting

**Error: "tool 'xcodebuild' requires Xcode"**
- Solution: Ensure full Xcode is installed and selected with `xcode-select`

**Error: SDK not found**
- Solution: Add required frameworks to `nativeBuildInputs`
- Common frameworks: CoreFoundation, Security, AppKit, UIKit

**Build fails with missing headers**
- Solution: Check that `apple_sdk.frameworks` includes required SDK components

### Alternative Approaches

1. **Manual Path Configuration**: Set `DEVELOPER_DIR` and `SDKROOT` in shellHook
2. **Swift Package Manager**: Use `swift build` instead of `xcodebuild` for simpler projects
3. **nix-darwin Integration**: Use nix-darwin for system-wide Xcode configuration

### Best Practices

- Pin Xcode version in `composeXcodeWrapper` for team consistency
- Document Xcode installation requirements for new developers
- Use `allowHigherVersions = true` for flexibility during Xcode updates
- Test builds on both Intel and Apple Silicon Macs

## Integration with Build Scripts

Once Xcode is properly configured, build scripts can use `xcodebuild`:

```bash
#!/bin/bash
# Build Swift app with xcodebuild
xcodebuild build \
  -project AgentHarbor.xcodeproj \
  -scheme AgentHarbor \
  -configuration Release \
  -destination 'platform=macOS'
```

## References

- Nixpkgs Darwin documentation: <https://github.com/NixOS/nixpkgs/blob/master/doc/languages-frameworks/darwin.section.md>
- nix-darwin: <https://github.com/LnL7/nix-darwin>
- Xcode command-line tools: <https://developer.apple.com/support/xcode/>
