/**
 * Vite configuration for Electron app
 * 
 * This configuration sets up the Electron plugin for hot-reload development
 * and builds both main and renderer processes.
 */

import { defineConfig } from 'vite';
import electron from 'vite-plugin-electron/simple';

export default defineConfig({
  plugins: [
    electron({
      main: {
        // Main process entry point
        entry: 'src/main/index.ts',
        vite: {
          build: {
            outDir: 'dist-electron',
            rollupOptions: {
              external: ['electron'],
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
  // Base configuration for renderer process (when we add a renderer UI)
  build: {
    outDir: 'dist-renderer',
  },
  server: {
    port: 5173,
  },
});
