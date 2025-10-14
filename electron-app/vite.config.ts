/**
 * Vite configuration for Electron app
 *
 * This configuration sets up the Electron plugin for hot-reload development
 * and builds both main and renderer processes.
 */

import { defineConfig } from 'vite';
import electron from 'vite-plugin-electron/simple';

export default defineConfig(({ command }) => ({
  plugins: [
    electron({
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
      // Automatically restart Electron when main process files change
      renderer: {},
    }),
  ],
  // Only configure build for development/preview, disable for production build
  build: command === 'serve' ? {
    // Allow normal development builds
  } : undefined,
  server: {
    port: 5173,
  },
}));
