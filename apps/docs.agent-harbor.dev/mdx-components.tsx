/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { useMDXComponents as getThemeComponents } from 'nextra-theme-docs'; // nextra-theme-blog or your custom theme
import { MDXComponents } from 'nextra/mdx-components';

// Get the default MDX components
const themeComponents = getThemeComponents();

// Merge components
export function useMDXComponents(components: MDXComponents) {
  return {
    ...themeComponents,
    ...components,
  };
}
