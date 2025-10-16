// Conditional route entry point that selects CSR or SSR implementation
// This uses Vite's build-time environment variable replacement

// Import both versions
import * as csrRoute from './index.csr.js';
import * as ssrRoute from './index.ssr.js';

// @ts-expect-error - VITE_STATIC_BUILD is injected by Vite at build time
const IS_STATIC = import.meta.env.VITE_STATIC_BUILD === 'true';

// Re-export based on build mode
// Vite will eliminate the unused branch at build time via tree-shaking
export const default_export = IS_STATIC ? csrRoute.default : ssrRoute.default;
export const route = IS_STATIC ? csrRoute.route : ssrRoute.route;

// SolidStart expects default export
export default default_export;
