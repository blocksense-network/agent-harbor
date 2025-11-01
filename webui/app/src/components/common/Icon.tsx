/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import type { JSX } from 'solid-js';
import { mergeProps, splitProps } from 'solid-js';
import { Dynamic } from 'solid-js/web';

import { iconsSources, IconVariant } from './iconsSources';

const wrapperSizes = {
  xs: 'w-3 h-3',
  sm: 'w-4 h-4',
  md: 'w-6 h-6',
  lg: 'w-8 h-8',
};

type IconProps = {
  variant: IconVariant;
  wrapperSize?: keyof typeof wrapperSizes;
  class?: string;
  classList?: JSX.IntrinsicElements['div']['classList'];
  iconClass?: string;
  iconClassList?: JSX.IntrinsicElements['svg']['classList'];
} & (
  | ({ as?: 'div' } & Omit<JSX.IntrinsicElements['div'], 'children' | 'class' | 'classList'>)
  | ({ as: 'button' } & Omit<JSX.IntrinsicElements['button'], 'children' | 'class' | 'classList'>)
);

export const Icon = (props: IconProps) => {
  const merged = mergeProps({ as: 'div' as const, class: '', iconClass: '' }, props);
  const [local, others] = splitProps(merged, [
    'variant',
    'wrapperSize',
    'class',
    'classList',
    'iconClass',
    'iconClassList',
    'as',
  ]);

  const IconComponent = iconsSources[local.variant];
  if (!IconComponent) return null;

  return (
    <Dynamic
      component={local.as === 'button' ? 'button' : 'div'}
      class={`flex shrink-0 items-center justify-center ${
        local.wrapperSize ? wrapperSizes[local.wrapperSize] : ''
      } ${local.class || ''}`}
      classList={local.classList}
      {...others}
    >
      <IconComponent class={local.iconClass} classList={local.iconClassList} />
    </Dynamic>
  );
};
