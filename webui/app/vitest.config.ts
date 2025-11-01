/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { defineConfig } from "vitest/config";
import solid from "vite-plugin-solid";
import solidSvg from 'vite-plugin-solid-svg'
import { resolve } from "path";

export default defineConfig({
  plugins: [solid() as any, solidSvg({ defaultAsComponent: false }) as any],
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test/setup.ts"],
  },
  resolve: {
    alias: {
      "~": resolve(__dirname, "./src"),
    },
  },
});
