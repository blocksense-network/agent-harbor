/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */
import type { Meta, StoryObj } from 'storybook-solidjs-vite';
import { Icon } from './Icon';
import { icon_variants } from './iconManifest';

const meta = {
  title: 'Components/Icon',
  component: Icon,
  parameters: {
    layout: 'centered',
  },
  tags: ['autodocs'],
  argTypes: {
    variant: {
      control: 'select',
      options: icon_variants,
      description: 'The icon variant to display',
    },
    size: {
      control: 'select',
      options: ['xs', 'sm', 'md', 'lg'],
      description: 'The size of the icon',
    },
  },
} satisfies Meta<typeof Icon>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: {
    variant: 'plus',
    size: 'md',
  },
};

export const AllVariants: Story = {
  render: () => (
    <div class="flex flex-wrap gap-8 items-center">
      {icon_variants.map(variant => (
        <div class="flex flex-col items-center gap-2">
          <Icon variant={variant} size="md" />
          <span class="text-xs text-gray-600">{variant}</span>
        </div>
      ))}
    </div>
  ),
};

export const Sizes: Story = {
  render: () => (
    <div class="flex items-center gap-8">
      {(['xs', 'sm', 'md', 'lg'] as const).map(size => (
        <div class="flex flex-col items-center gap-2">
          <Icon variant="plus" size={size} />
          <span class="text-xs text-gray-600">{size}</span>
        </div>
      ))}
    </div>
  ),
};

export const AsButton: Story = {
  render: () => (
    <div class="flex gap-4">
      <Icon
        variant="plus"
        as="button"
        size="md"
        wrapperSize="md"
        onClick={() => console.log('Plus clicked')}
        aria-label="Add"
      />
      <Icon
        variant="close"
        as="button"
        size="md"
        wrapperSize="md"
        onClick={() => console.log('Close clicked')}
        aria-label="Close"
      />
      <Icon
        variant="minus"
        as="button"
        size="md"
        wrapperSize="md"
        onClick={() => console.log('Minus clicked')}
        aria-label="Remove"
      />
    </div>
  ),
};

export const WithCustomClass: Story = {
  render: () => (
    <div class="flex gap-4">
      <Icon variant="plus" size="md" class="text-blue-500" />
      <Icon variant="close" size="md" class="text-red-500" />
      <Icon variant="models" size="md" class="text-green-500" />
    </div>
  ),
};
