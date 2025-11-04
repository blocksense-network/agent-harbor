/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import type { Component, JSX } from 'solid-js';

import { icon_variants, type IconVariant } from './iconManifest';

export { icon_variants } from './iconManifest';
export type { IconVariant } from './iconManifest';

type ComponentType = Component<JSX.IntrinsicElements['svg']>;

const modules = import.meta.glob<{ default: ComponentType }>('../../assets/icons/*.svg', {
  eager: true,
  query: '?component-solid',
});

export const iconsSources = icon_variants.reduce(
  (acc, variant) => {
    const module = modules[`../../assets/icons/${variant}.svg`];

    if (!module) {
      console.warn(`Missing module for icon variant: ${variant}`);
      return acc;
    }

    acc[variant] = module.default;
    return acc;
  },
  {} as Partial<Record<IconVariant, ComponentType>>,
);
