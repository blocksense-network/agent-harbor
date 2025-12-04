// For more info, see https://github.com/storybookjs/eslint-plugin-storybook#configuration-flat-config-format
import storybook from 'eslint-plugin-storybook';

import { defineConfig, globalIgnores } from 'eslint/config';
import js from '@eslint/js';
import globals from 'globals';
import tseslint from 'typescript-eslint';
import solid from 'eslint-plugin-solid/configs/typescript';
import { createTypeScriptImportResolver } from 'eslint-import-resolver-typescript';
import { flatConfigs as importConfigs } from 'eslint-plugin-import-x';

export default defineConfig(
  globalIgnores([
    '**/out',
    '**/dist',
    '**/build',
    '**/*.md',
    '**/.vinxi',
    '**/.output',
    '**/.next',
    '**/next-env.d.ts',
    '.yarn/**',
    '.pnp.cjs',
    '.pnp.loader.mjs',
    '.prettierrc.cjs',
    'vendor',
    '.obsidian',
    'electron-app',
  ]),
  {
    files: ['**/*.{js,mjs,cjs,ts,mts,cts,jsx,tsx}'],
    languageOptions: {
      globals: { ...globals.browser, ...globals.node },
    },
  },
  js.configs.recommended,
  tseslint.configs.strict,
  importConfigs.recommended,
  importConfigs.typescript,
  {
    ...solid,
    settings: {
      'import-x/resolver-next': [
        createTypeScriptImportResolver({
          alwaysTryTypes: true,
          project: ['docs/*/tsconfig*.json', 'webui/**/tsconfig*.json'],
        }),
      ],
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
    },
  },
);
