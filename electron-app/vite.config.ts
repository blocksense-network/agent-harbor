/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

/**
 * Vite configuration for Electron app
 *
 * This configuration sets up the Electron plugin for hot-reload development
 * and builds both main and renderer processes.
 */

import { defineConfig } from 'vite';
import electron from 'vite-plugin-electron/simple';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);

/**
 * Resolve Electron executable path using Node's module resolver (PnP-aware)
 * This is more robust than scanning .yarn/unplugged directories.
 */
function resolveElectronCli(): string | undefined {
  // Preferred: exact path to Electron's CLI entry
  try {
    return require.resolve('electron/cli.js');
  } catch {}

  // Fallback for Node >= 20: import.meta.resolve returns a file URL
  try {
    // @ts-ignore - Node 20+
    const url = import.meta.resolve('electron/cli.js');
    return fileURLToPath(url);
  } catch {}

  // Last resort: the 'electron' package exports the binary path as a string
  try {
    const binPath = require('electron') as unknown as string;
    return binPath;
  } catch {}

  return undefined;
}

const electronPath = resolveElectronCli();

export default defineConfig(({ command }) => ({
  plugins: [
    electron({
      // vite-plugin-electron will spawn this executable
      ...(electronPath ? { electronPath } : {}),
      main: {
        // Main process entry point
        entry: 'src/main/index.ts',
        vite: {
          build: {
            outDir: 'dist-electron',
            rollupOptions: {
              external: ['electron', '@agent-harbor/gui-core'],
            },
          },
        },
      },
      preload: {
        // Preload script entry point
        input: {
          preload: 'src/renderer/preload.ts',
        },
        vite: {
          build: {
            outDir: 'dist-electron/renderer',
            rollupOptions: {
              external: ['electron'],
            },
          },
        },
      },
      // No renderer build needed - WebUI is loaded from mock server / ah webui
      // renderer: {},
    }),
  ],
  // Only configure build for development/preview, disable for production build
  build:
    command === 'serve'
      ? {
          // Allow normal development builds
        }
      : undefined,
  server: {
    port: 5173,
  },
}));
