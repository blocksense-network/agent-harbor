import { defineConfig } from 'eslint/config';
import js from '@eslint/js';
import tseslint from 'typescript-eslint';

import solid from 'eslint-plugin-solid/configs/typescript';

import * as tsResolver from 'eslint-import-resolver-typescript';
import { flatConfigs as importConfigs } from 'eslint-plugin-import-x';

export default defineConfig(
  {
    ignores: [
      '**/dist',
      '**/build',
      '**/*.md',
      '.yarn/**',
      '.pnp.cjs',
      '.pnp.loader.mjs',
      '.prettierrc.cjs',
      '**/.vinxi',
      '**/.output',
      '**/.next',
      'vendor',
      '.obsidian',
      'electron-app',
    ],
  },
  js.configs.recommended,
  tseslint.configs.strict,
  importConfigs.recommended,
  importConfigs.typescript,
  {
    ...solid,
    settings: {
      'import-x/resolver': {
        name: 'tsResolver',
        resolver: tsResolver,
        options: {
          alwaysTryTypes: true,
        },
      },
    },

    rules: {
      '@typescript-eslint/no-unused-vars': [
        'error',
        {
          argsIgnorePattern: '^_',
          varsIgnorePattern: '^_',
        },
      ],
      '@typescript-eslint/no-explicit-any': 'off',
      '@typescript-eslint/no-non-null-assertion': 'off',
      'import-x/no-named-as-default-member': 'off',
      'import-x/no-unresolved': 'off',
    },
  },
);
