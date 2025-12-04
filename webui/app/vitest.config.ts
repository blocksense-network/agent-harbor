/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

// import { defineConfig } from 'vite';
import { defineConfig } from 'vitest/config';
import solid from 'vite-plugin-solid';
import devtools from 'solid-devtools/vite';
import solidSvg from 'vite-plugin-solid-svg';
import path, { resolve } from 'path';
import { fileURLToPath } from 'url';
import { storybookTest } from '@storybook/addon-vitest/vitest-plugin';
import { playwright } from '@vitest/browser-playwright';

const dirname =
  typeof __dirname !== 'undefined' ? __dirname : path.dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  plugins: [
    devtools({
      autoname: true,
    }) as any,
    solid() as any,
    solidSvg({ defaultAsComponent: false }) as any,
  ],
  define: {
    'process.env': {},
  },
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./src/test/setup.ts'],
    projects: [
      {
        extends: true,
        plugins: [
          // The plugin will run tests for the stories defined in your Storybook config
          // See options at: https://storybook.js.org/docs/next/writing-tests/integrations/vitest-addon#storybooktest
          storybookTest({
            configDir: path.join(dirname, '.storybook'),
          }),
        ],
        test: {
          name: 'storybook',
          browser: {
            enabled: true,
            headless: true,
            provider: playwright(),
            instances: [
              {
                browser: 'chromium',
              },
            ],
          },
        },
      },
    ],
  },
  resolve: {
    alias: {
      '~': resolve(__dirname, './src'),
    },
  },
});
