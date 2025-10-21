# Agent Harbor GUI

Cross-platform desktop application for Agent Harbor, providing a graphical interface wrapper around the WebUI with native OS integrations.

## Architecture

- **Framework**: Electron + TypeScript
- **Browser Automation**: Playwright (using Electron's bundled Chromium)
- **Native Addons**: Rust via N-API
- **Build System**: Vite + electron-builder

## Project Structure

```
electron-app/
├── src/
│   ├── main/               # Electron main process (Node.js)
│   │   ├── index.ts        # Main entry point
│   │   └── browser-automation/  # Playwright integration
│   └── renderer/           # Renderer process
│       └── preload.ts      # Secure IPC preload script
├── crates/
│   └── ah-gui-core/        # Rust native addon (N-API)
├── assets/                 # Application icons and resources
├── resources/              # Bundled resources
│   ├── webui/              # WebUI server files
│   └── cli/                # Bundled CLI tools
└── dist-electron/          # Build output
```

## Development

### Prerequisites

- Node.js 18+ (managed via nix flake at repository root)
- Rust toolchain (managed via nix flake)
- yarn (PnP mode)

### Setup

```bash
# Install Node.js dependencies
yarn install

# Build Rust native addon (first time only)
cd crates/ah-gui-core && yarn build && cd ../..

# Start development server with hot-reload
yarn dev
```

### Development Mode

The `yarn dev` command starts Vite with the Electron plugin, which:

- Compiles TypeScript for main and renderer processes
- Launches Electron automatically
- Watches for file changes and hot-reloads
- Opens DevTools for debugging

### Building

```bash
# Build for development (no installer)
yarn build:dev

# Build production installer
yarn build
```

This creates installers in `release/` directory:

- macOS: .dmg and .pkg
- Windows: .exe (NSIS) and .msi
- Linux: .AppImage, .deb, .rpm

### Linting and Formatting

```bash
# Run ESLint
yarn lint

# Run Prettier
yarn format

# Type check
yarn type-check
```

## Testing

### Manual Testing

1. Start development server: `yarn dev`
2. Verify window opens with "Agent Harbor GUI" splash screen
3. Check DevTools console for errors
4. Test hot-reload by editing `src/main/index.ts`

### Native Addon Testing

To verify Rust native addon integration:

<!-- prettier-ignore -->
```typescript
// In main process
import addon from './path/to/ah-gui-core';
console.log(addon.helloFromRust()); // Should output: "Hello from Agent Harbor GUI Core (Rust)!"
console.log(addon.getPlatform());   // Should output: "darwin", "linux", or "win32"
```

## Milestone Status

**M0.2 Electron Project Scaffolding & Build Infrastructure**: ✅ In Progress

See [specs/Public/Agent-Harbor-GUI.status.md](../../specs/Public/Agent-Harbor-GUI.status.md) for detailed milestone tracking.

## Next Steps

1. **M0.2.5**: Evaluate WebUI embedding strategies (static build vs server process)
2. **M0.3**: Implement WebUI process management
3. **M1.1**: Main window and WebUI embedding
4. **M1.2**: System tray integration
5. **M1.3**: Native notifications

## Documentation

- [Architecture Overview](../../specs/Public/Agent-Harbor-GUI.status.md)
- [Repository Layout](../../specs/Public/Repository-Layout.md)
- [Browser Automation](../../specs/Public/Browser-Automation/)
- [Electron Packaging Research](../../specs/Research/Electron-Packaging/)

## License

MIT
