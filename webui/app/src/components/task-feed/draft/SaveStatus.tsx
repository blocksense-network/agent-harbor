/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { createEffect, createSignal } from 'solid-js';

export type SaveStatusType = 'unsaved' | 'saving' | 'saved' | 'error';

type SaveStatusProps = {
  status: SaveStatusType;
  class?: string;
};

export const SaveStatus = (props: SaveStatusProps) => {
  const [currentStatus, setCurrentStatus] = createSignal<SaveStatusType>('saved');

  createEffect(() => {
    setCurrentStatus(props.status);
  });

  const getStatusConfig = (status: SaveStatusType) => {
    switch (status) {
      case 'unsaved':
        return {
          text: 'Unsaved',
          icon: '○',
          color: 'text-gray-500',
          bgColor: 'bg-gray-50',
        };
      case 'saving':
        return {
          text: 'Saving...',
          icon: '⟳',
          color: 'text-orange-600',
          bgColor: 'bg-orange-50',
        };
      case 'saved':
        return {
          text: 'Saved',
          icon: '✓',
          color: 'text-green-600',
          bgColor: 'bg-green-50',
        };
      case 'error':
        return {
          text: 'Save failed',
          icon: '✗',
          color: 'text-red-600',
          bgColor: 'bg-red-50',
        };
    }
  };

  const config = () => getStatusConfig(currentStatus());

  return (
    <div
      class={`
        inline-flex items-center gap-1 rounded-md px-2 py-1 text-xs font-medium
        transition-colors
        ${config().color}
        ${config().bgColor}
        ${props.class || ''}
      `}
      role="status"
      aria-live="polite"
      aria-label={`Save status: ${config().text}`}
    >
      <span class="text-sm" aria-hidden="true">
        {config().icon}
      </span>
      <span>{config().text}</span>
    </div>
  );
};
