/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import type { Component, JSX } from 'solid-js';

const modules = import.meta.glob<{ default: Component<JSX.SvgSVGAttributes<SVGSVGElement>> }>(
  '../../assets/icons/*.svg',
  { eager: true, query: '?component-solid' },
);

export const iconsSources: Record<
  string,
  Component<JSX.SvgSVGAttributes<SVGSVGElement>>
> = Object.fromEntries(
  Object.entries(modules).map(([filepath, mod]) => {
    const filename = filepath.split(/[\\/]/).pop() || filepath;
    const name = filename.replace(/\.svg$/i, '');
    return [name, mod.default];
  }),
);

export type IconVariant = keyof typeof iconsSources;
