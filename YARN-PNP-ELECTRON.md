# Yarn PnP + Electron Integration

This document explains how we've configured Electron to work with Yarn Plug'n'Play (PnP).

## The Problems

### 1. Electron Binary Access

Electron requires access to its binary files on the filesystem, but Yarn PnP keeps packages in a virtual filesystem by default. This causes errors like:

```
Error: Qualified path resolution failed: we looked for the following paths, but none could be accessed.
Source path: .yarn/unplugged/electron-npm-*/node_modules/electron/main
```

### 2. Native Addons in Playwright Tests

When running Playwright tests, native Node addons (like `@agent-harbor/gui-core`) cannot be resolved through PnP because:

- Playwright strips `NODE_OPTIONS`, preventing PnP loader activation
- Passing `--import` for the PnP loader doesn't work with Playwright's Electron launcher
- Preloading PnP in the source doesn't work (ESM imports are resolved before top-level await)

## The Solutions

### 1. Force Electron to be Unplugged

In `electron-app/package.json`, we add:

```json
"dependenciesMeta": {
  "electron": {
    "unplugged": true
  }
}
```

This tells Yarn to extract Electron to `.yarn/unplugged/` where its binaries can be accessed normally.

### 2. Copy Native Addons to dist-electron

For Playwright testing, we copy native addons from the PnP unplugged directory to `dist-electron/node_modules/` so Node.js can find them using standard module resolution.

**Script**: `electron-app/scripts/copy-native-addon.sh`

```bash
#!/usr/bin/env bash
# Copy native addon to dist-electron so it can be found without PnP

ADDON_FILE=$(find "$REPO_ROOT/.yarn/unplugged" -name "ah-gui-core.*.node" -type f | head -1)
ADDON_DIR=$(dirname "$ADDON_FILE")

# Copy all required files
cp "$ADDON_FILE" "$DIST_DIR/node_modules/@agent-harbor/gui-core/"
cp "$ADDON_DIR/index.js" "$DIST_DIR/node_modules/@agent-harbor/gui-core/"
cp "$ADDON_DIR/package.json" "$DIST_DIR/node_modules/@agent-harbor/gui-core/"
cp "$ADDON_DIR/index.d.ts" "$DIST_DIR/node_modules/@agent-harbor/gui-core/"
```

**Build Integration** in `package.json`:

```json
"scripts": {
  "build:dev": "vite build && npm run copy:native-addon",
  "copy:native-addon": "bash scripts/copy-native-addon.sh",
  "pretest": "npm run build:dev"
}
```

The `pretest` hook ensures the native addon is copied before Playwright tests run.

## Verification

After making changes, always run:

```bash
yarn install
```

This regenerates the `.pnp.cjs` file and unpacks Electron.

You can verify Electron is correctly unplugged:

```bash
ls -la .yarn/unplugged/ | grep electron
# Should show: electron-npm-X.Y.Z-...

yarn node -e "import('electron').then(() => console.log('✅ Works'))"
# Should print: ✅ Works
```

## Troubleshooting

### Electron Path Errors

If you get Electron path errors:

1. **Clean reinstall:**

   ```bash
   rm -rf .yarn/cache .yarn/unplugged .pnp.cjs .pnp.loader.mjs
   yarn install
   ```

2. **Check unplugged directory:**
   ```bash
   ls -la .yarn/unplugged/electron-npm-*/node_modules/electron/
   # Should show: cli.js, dist/, package.json, etc.
   ```

### Native Addon Resolution Errors

If Playwright tests fail with "Cannot find package '@agent-harbor/gui-core'":

1. **Verify native addon is built:**

   ```bash
   ls -la crates/ah-gui-core/ah-gui-core.*.node
   # Should show the .node binary file
   ```

2. **Run the copy script manually:**

   ```bash
   cd electron-app
   bash scripts/copy-native-addon.sh
   ls -la dist-electron/node_modules/@agent-harbor/gui-core/
   # Should show: index.js, package.json, *.node, index.d.ts
   ```

3. **Rebuild Electron app:**
   ```bash
   yarn build:dev  # or just: yarn vite build && yarn copy:native-addon
   ```

## Why This Matters

- **Development mode**: `yarn dev` uses `vite-plugin-electron` which needs to launch the Electron binary
- **Build mode**: `electron-builder` also needs filesystem access to Electron binaries
- **Playwright tests**: Our E2E tests need to:
  1. Launch Electron programmatically
  2. Load native addons (without PnP loader, since Playwright strips NODE_OPTIONS)

## Key Insights

### Why PnP Loader Doesn't Work in Tests

We tried several approaches that **did not work**:

1. **NODE_OPTIONS environment variable**: Playwright strips this, so PnP loader never activates
2. **Passing --import flag**: Doesn't work with Playwright's Electron launcher protocol
3. **Preloading PnP in source**: ESM imports are resolved before top-level await executes
4. **Wrapper scripts**: Playwright requires direct Electron executable, not wrapper scripts

### Why Copying Works

By copying the native addon to `dist-electron/node_modules/`, we:

- Use standard Node.js module resolution (no PnP needed)
- Keep development workflow using PnP (DRY principle)
- Only copy for testing/production builds (development uses PnP normally)
- Ensure all required files are present (`index.js`, `package.json`, `.node`, `index.d.ts`)

## References

- [Yarn PnP unplugged packages](https://yarnpkg.com/features/pnp#packages-with-peer-dependencies)
- [vite-plugin-electron issues with PnP](https://github.com/electron-vite/vite-plugin-electron/issues/233)
- [Playwright Electron API](https://playwright.dev/docs/api/class-electron)
- [N-API native addons structure](https://nodejs.org/api/n-api.html)
