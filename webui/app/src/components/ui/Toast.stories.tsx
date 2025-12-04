/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */
import type { Meta, StoryObj } from 'storybook-solidjs-vite';
import { createSignal } from 'solid-js';
import { ToastContainer, type Toast } from './Toast';

const meta = {
  title: 'Components/Toast',
  component: ToastContainer,
  parameters: {
    layout: 'fullscreen',
  },
  tags: ['autodocs'],
} satisfies Meta<typeof ToastContainer>;

export default meta;
type Story = StoryObj<typeof meta>;

// Helper component to manage toast state
const ToastWrapper = (props: { toasts: Toast[] }) => {
  const [toasts, setToasts] = createSignal(props.toasts);

  const handleRemove = (id: string) => {
    setToasts(prev => prev.filter(t => t.id !== id));
  };

  return <ToastContainer toasts={toasts()} onRemove={handleRemove} />;
};

export const Success: Story = {
  render: () => (
    <ToastWrapper
      toasts={[
        {
          id: '1',
          type: 'success',
          message: 'Task created successfully!',
        },
      ]}
    />
  ),
};

export const Error: Story = {
  render: () => (
    <ToastWrapper
      toasts={[
        {
          id: '1',
          type: 'error',
          message: 'Failed to save draft. Please try again.',
        },
      ]}
    />
  ),
};

export const Warning: Story = {
  render: () => (
    <ToastWrapper
      toasts={[
        {
          id: '1',
          type: 'warning',
          message: 'Your session will expire in 5 minutes.',
        },
      ]}
    />
  ),
};

export const Info: Story = {
  render: () => (
    <ToastWrapper
      toasts={[
        {
          id: '1',
          type: 'info',
          message: 'New updates available. Refresh to see changes.',
        },
      ]}
    />
  ),
};

export const WithAction: Story = {
  render: () => (
    <ToastWrapper
      toasts={[
        {
          id: '1',
          type: 'success',
          message: 'Draft saved successfully!',
          actions: [
            {
              label: 'View',
              onClick: () => console.log('View clicked'),
              variant: 'primary',
            },
          ],
        },
      ]}
    />
  ),
};

export const MultipleToasts: Story = {
  render: () => (
    <ToastWrapper
      toasts={[
        {
          id: '1',
          type: 'success',
          message: 'Task created successfully!',
        },
        {
          id: '2',
          type: 'info',
          message: 'Processing your request...',
        },
        {
          id: '3',
          type: 'warning',
          message: 'Please review your changes.',
        },
      ]}
    />
  ),
};

export const WithMultipleActions: Story = {
  render: () => (
    <ToastWrapper
      toasts={[
        {
          id: '1',
          type: 'error',
          message: 'Failed to connect to server.',
          actions: [
            {
              label: 'Retry',
              onClick: () => console.log('Retry clicked'),
              variant: 'primary',
            },
            {
              label: 'Cancel',
              onClick: () => console.log('Cancel clicked'),
              variant: 'secondary',
            },
          ],
        },
      ]}
    />
  ),
};
