/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { defineConfig } from "@solidjs/start/config";
import tailwindcss from "@tailwindcss/vite";
import solidSvg from 'vite-plugin-solid-svg';
import checker from 'vite-plugin-checker';
import * as fs from 'fs';

// Suppress specific SolidJS Start warnings in quiet mode
if (process.env['QUIET_MODE'] === 'true') {
  const originalWarn = console.warn;
  console.warn = function(...args: any[]) {
    // Suppress the "No route matched for preloading js assets" warning
    if (args.length === 1 && typeof args[0] === 'string' && args[0].includes('No route matched for preloading js assets')) {
      return; // Suppress this specific warning
    }
    originalWarn.apply(console, args);
  };
}

// API server configuration
// In production: access point daemon (ah agent access-point) runs as subprocess/sidecar
// In development: mock server simulates the API
const API_TARGET = process.env['API_SERVER_URL'] || 'http://localhost:3001';

// Build mode: 'server' (SSR) or 'client' (CSR static build for Electron)
const BUILD_MODE = process.env['WEBUI_BUILD_MODE'] || 'server';
const isStaticBuild = BUILD_MODE === 'client';

export default defineConfig({
  // ssr: !isStaticBuild, // SSR mode for server builds, CSR for static builds
  // ...(isStaticBuild ? {
  //   // For CSR builds, disable prerendering
  //   server: {
  //     preset: "static",
  //     prerender: {
  //       routes: [], // Don't prerender any routes for CSR
  //     },
  //   },
  // } : {
  //   // For SSR builds, use Node.js adapter
  //   server: {
  //     preset: "node",
  //     // Proxy-based architecture: SSR server acts as single entry point
  //     // All /api/v1/* requests are forwarded to the access point daemon
  //     // This enables SSR server to implement user access policies in the future
  //   },
  // }),
  server: {
    preset: "cloudflare-pages",

    rollupConfig: {
      external: ["node:async_hooks"]
    }
  },
  vite: {
    // Define environment variables available at build time
    define: {
      'import.meta.env.VITE_STATIC_BUILD': JSON.stringify(isStaticBuild ? 'true' : 'false'),
    },
    plugins: [
      tailwindcss(),
      solidSvg({ defaultAsComponent: false }),
      // Disable checker for CSR builds (faster builds, linting happens separately)
      ...(isStaticBuild ? [] : [
        checker({ typescript: true, eslint: { lintCommand: 'eslint src --ext .ts,.tsx' } }) as any
      ]),
    ],
    server: {
      proxy: {
        '/api/v1': {
          target: API_TARGET,
          changeOrigin: true,
          // Preserve the /api/v1 prefix when forwarding
          rewrite: (path: string) => path,
          // WebSocket support for SSE
          ws: true,
          configure: (proxy: any, _options: any) => {
            proxy.on('error', (err: any, _req: any, _res: any) => {
              console.error('[Proxy Error]', err);
            });
            proxy.on('proxyReq', (proxyReq: any, req: any, _res: any) => {
              const isQuietMode = process.env['QUIET_MODE'] === 'true' || process.env['NODE_ENV'] === 'test';
              if (!isQuietMode) {
                console.log(`[Proxy] ${req.method} ${req.url} → ${API_TARGET}${req.url}`);
              }
            });
          },
        },
      },
    },
  }
});
