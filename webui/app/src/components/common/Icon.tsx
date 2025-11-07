/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import type { JSX, ComponentProps } from 'solid-js';
import { splitProps, Switch, Match, mergeProps } from 'solid-js';
import { Dynamic } from 'solid-js/web';
import { A } from '@solidjs/router';

import { iconsSources, type IconVariant } from './iconsSources';

const wrapperSizes = {
  xs: 'w-3 h-3',
  sm: 'w-4 h-4',
  md: 'w-6 h-6',
  lg: 'w-8 h-8',
};
type WrapperSize = keyof typeof wrapperSizes;

const sizes = {
  xs: 'w-2 h-auto',
  sm: 'w-3 h-auto',
  md: 'w-4 h-auto',
  lg: 'w-6 h-auto',
};
type Size = keyof typeof sizes;

type IconBaseProps = {
  variant: IconVariant;
  class?: JSX.IntrinsicElements['svg']['class'];
  classList?: JSX.IntrinsicElements['svg']['classList'];
  size?: Size;
};

type IconSvgElementProps = JSX.IntrinsicElements['svg'];
type IconInnerSvgProps = Omit<IconSvgElementProps, 'class' | 'classList'>;

type IconStandaloneProps = IconBaseProps & {
  as?: undefined;
  wrapperSize?: never;
  wrapperClass?: never;
  wrapperClassList?: never;
  iconProps?: never;
} & IconSvgElementProps;

type IconWrapperBase<TTag extends keyof JSX.IntrinsicElements, TProps> = IconBaseProps & {
  as: TTag;
  wrapperSize?: WrapperSize;
  wrapperClass?: JSX.IntrinsicElements[TTag]['class'];
  wrapperClassList?: JSX.IntrinsicElements[TTag]['classList'];
  iconProps?: IconInnerSvgProps;
} & Omit<TProps, 'children' | 'class' | 'classList'>;

type IconDivWrapperProps = IconWrapperBase<'div', JSX.IntrinsicElements['div']>;
type IconButtonWrapperProps = IconWrapperBase<'button', JSX.IntrinsicElements['button']>;
type IconLinkWrapperProps = IconWrapperBase<'a', ComponentProps<typeof A>>;

type IconProps =
  | IconStandaloneProps
  | IconDivWrapperProps
  | IconButtonWrapperProps
  | IconLinkWrapperProps;

// SUGGESTION: Remove width/height and replace fill/stroke with currentColor in the SVG
export const Icon = (props: IconProps) => {
  const mergedProps = mergeProps({ class: '', wrapperClass: '' }, props);
  const [local, others] = splitProps(mergedProps, [
    'variant',
    'class',
    'classList',
    'size',
    'as',
    'wrapperSize',
    'wrapperClass',
    'wrapperClassList',
    'iconProps',
  ]);

  const iconComponent = () => iconsSources[local.variant];
  const iconClass = () => `${local.class} ${local.size ? sizes[local.size] : ''}`;
  const wrapperClass = () =>
    `flex shrink-0 items-center justify-center ${local.wrapperSize ? wrapperSizes[local.wrapperSize] : ''} ${local.wrapperClass}`;

  return (
    <Switch>
      <Match when={!local.as}>
        <Dynamic
          component={iconComponent()}
          class={iconClass()}
          classList={local.classList}
          {...(others as IconSvgElementProps)}
        />
      </Match>

      <Match when={local.as === 'button'}>
        <button
          class={wrapperClass()}
          classList={local.wrapperClassList}
          {...(others as IconButtonWrapperProps)}
        >
          <Dynamic
            component={iconComponent()}
            class={iconClass()}
            classList={local.classList}
            {...local.iconProps}
          />
        </button>
      </Match>

      <Match when={local.as === 'a'}>
        <A
          class={wrapperClass()}
          classList={local.wrapperClassList}
          {...(others as IconLinkWrapperProps)}
        >
          <Dynamic
            component={iconComponent()}
            class={iconClass()}
            classList={local.classList}
            {...local.iconProps}
          />
        </A>
      </Match>

      <Match when={local.as === 'div'}>
        <div
          class={wrapperClass()}
          classList={local.wrapperClassList}
          {...(others as IconDivWrapperProps)}
        >
          <Dynamic
            component={iconComponent()}
            class={iconClass()}
            classList={local.classList}
            {...local.iconProps}
          />
        </div>
      </Match>
    </Switch>
  );
};
