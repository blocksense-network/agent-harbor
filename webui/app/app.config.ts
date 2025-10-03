import { defineConfig } from "@solidjs/start/config";
import tailwindcss from "@tailwindcss/vite";
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

export default defineConfig({
  ssr: true, // Enable SSR (default, but explicit for clarity)
  server: {
    preset: "node", // Use Node.js adapter for custom server needs
    // Proxy-based architecture: SSR server acts as single entry point
    // All /api/v1/* requests are forwarded to the access point daemon
    // This enables SSR server to implement user access policies in the future
  },
  vite: {
    plugins: [
      tailwindcss() as any,
      checker({ typescript: true, eslint: { lintCommand: 'eslint src --ext .ts,.tsx' } }) as any
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
