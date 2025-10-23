/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import js from '@eslint/js';
import tseslint from '@typescript-eslint/eslint-plugin';
import tsparser from '@typescript-eslint/parser';
import prettier from 'eslint-plugin-prettier';
import prettierConfig from 'eslint-config-prettier';
import betterTw from 'eslint-plugin-better-tailwindcss';
import solid from 'eslint-plugin-solid';
import a11y from 'eslint-plugin-jsx-a11y';

export default [
  js.configs.recommended,
  prettierConfig,
  {
    files: ['**/*.{ts,tsx,js,jsx}'],
    plugins: { 'better-tailwindcss': betterTw },
    settings: {
      // Tailwind v4: point to the CSS entry that has `@import "tailwindcss";`
      'better-tailwindcss': {
        entryPoint: 'src/app.css',
        tailwindConfig: 'tailwind.config.js',
        ignore: ['tom-select-input', 'model-multi-select']
      }
    },
    rules: {
      // full recommended bundle (stylistic + correctness)
      ...betterTw.configs['recommended'].rules,
      // Disable unregistered class checking for known third-party/custom classes
      'better-tailwindcss/no-unregistered-classes': ['error', {
        ignore: ['tom-select-input', 'model-multi-select', 'toast-item', 'prose', 'prose-sm']
      }]
    }
  },
  solid.configs['flat/recommended'],
  {
    files: ['**/*.{ts,tsx,jsx}'],
    plugins: { 'jsx-a11y': a11y },
    rules: {
      'jsx-a11y/alt-text': 'warn',
      'jsx-a11y/no-autofocus': 'warn',
      'jsx-a11y/anchor-has-content': 'warn'
    }
  },
  {
    files: ['**/*.ts', '**/*.tsx', '**/*.js', '**/*.jsx'],
    languageOptions: {
      parser: tsparser,
      parserOptions: {
        ecmaVersion: 2022,
        sourceType: 'module'
      },
      globals: {
        console: 'readonly',
        process: 'readonly',
        Buffer: 'readonly',
        __dirname: 'readonly',
        __filename: 'readonly',
        // Browser globals
        document: 'readonly',
        window: 'readonly',
        localStorage: 'readonly',
        Event: 'readonly',
        SubmitEvent: 'readonly',
        fetch: 'readonly',
        RequestInit: 'readonly',
        URLSearchParams: 'readonly',
        EventSource: 'readonly',
        MessageEvent: 'readonly',
        CustomEvent: 'readonly',
        Element: 'readonly',
        Node: 'readonly',
        KeyboardEvent: 'readonly',
        MouseEvent: 'readonly',
        HTMLSelectElement: 'readonly',
        HTMLButtonElement: 'readonly',
        HTMLInputElement: 'readonly',
        HTMLUListElement: 'readonly',
        HTMLTextAreaElement: 'readonly',
        setTimeout: 'readonly',
        clearTimeout: 'readonly',
        setInterval: 'readonly',
        clearInterval: 'readonly',
        navigator: 'readonly'
      }
    },
    plugins: {
      '@typescript-eslint': tseslint,
      prettier: prettier
    },
    rules: {
      ...tseslint.configs.recommended.rules,
      'prettier/prettier': 'error',
      '@typescript-eslint/no-unused-vars': ['error', { argsIgnorePattern: '^_' }],
      '@typescript-eslint/explicit-function-return-type': 'off',
      '@typescript-eslint/no-explicit-any': 'warn'
    }
  },
  {
    files: ['**/*.js', '**/*.mjs'],
    languageOptions: {
      ecmaVersion: 2022,
      sourceType: 'module'
    },
    rules: {
      'no-unused-vars': ['error', { argsIgnorePattern: '^_' }]
    }
  }
];