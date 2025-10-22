/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { Component, createSignal, onMount, onCleanup, For } from 'solid-js';

export type ToastType = 'success' | 'error' | 'info' | 'warning';

export interface ToastAction {
  label: string;
  onClick: () => void;
  variant?: 'primary' | 'secondary' | 'danger';
}

export interface Toast {
  id: string;
  type: ToastType;
  message: string;
  duration?: number;
  actions?: ToastAction[];
}

interface ToastItemProps {
  toast: Toast;
  onRemove: (id: string) => void;
}

const ToastItem: Component<ToastItemProps> = props => {
  const [isVisible, setIsVisible] = createSignal(true);

  onMount(() => {
    const duration = props.toast.duration ?? 5000;
    const timer = setTimeout(() => {
      setIsVisible(false);
      setTimeout(() => props.onRemove(props.toast.id), 300); // Allow animation to complete
    }, duration);

    onCleanup(() => clearTimeout(timer));
  });

  const getToastStyles = (type: ToastType) => {
    const baseStyles = 'flex items-center p-4 mb-4 text-sm rounded-lg transition-all duration-300';

    switch (type) {
      case 'success':
        return `${baseStyles} bg-green-100 text-green-800 border border-green-200`;
      case 'error':
        return `${baseStyles} bg-red-100 text-red-800 border border-red-200`;
      case 'warning':
        return `${baseStyles} bg-yellow-100 text-yellow-800 border border-yellow-200`;
      case 'info':
      default:
        return `${baseStyles} bg-blue-100 text-blue-800 border border-blue-200`;
    }
  };

  const getIcon = (type: ToastType) => {
    switch (type) {
      case 'success':
        return '✓';
      case 'error':
        return '✕';
      case 'warning':
        return '⚠';
      case 'info':
      default:
        return 'ℹ';
    }
  };

  return (
    <div
      data-toast
      class={`
        ${getToastStyles(props.toast.type)}
        ${
          isVisible()
            ? 'translate-x-0 opacity-100'
            : `
          translate-x-full opacity-0
        `
        }
      `}
      role="alert"
      aria-live="assertive"
    >
      <div class="mr-3 flex-shrink-0">
        <span class="text-lg" aria-hidden="true">
          {getIcon(props.toast.type)}
        </span>
      </div>
      <div class="flex-1 font-medium">{props.toast.message}</div>
      <div class="ml-3 flex items-center space-x-2">
        <For each={props.toast.actions}>
          {action => {
            const variant = action.variant ?? 'primary';
            const baseClasses =
              'rounded px-3 py-1 text-sm font-medium transition-colors focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:ring-offset-2';
            const variantClasses: Record<typeof variant, string> = {
              danger: 'bg-red-600 text-white hover:bg-red-700',
              secondary: 'bg-gray-600 text-white hover:bg-gray-700',
              primary: 'bg-blue-600 text-white hover:bg-blue-700',
            };

            return (
              <button
                onClick={() => {
                  action.onClick();
                  props.onRemove(props.toast.id);
                }}
                class={`
                  ${baseClasses}
                  ${variantClasses[variant]}
                `}
              >
                {action.label}
              </button>
            );
          }}
        </For>
        <button
          onClick={() => props.onRemove(props.toast.id)}
          class={`
            flex-shrink-0 rounded p-1 text-current transition-opacity
            hover:opacity-75
            focus-visible:ring-2 focus-visible:ring-blue-500
            focus-visible:ring-offset-2
          `}
          aria-label="Dismiss notification"
        >
          <span class="text-lg" aria-hidden="true">
            ×
          </span>
        </button>
      </div>
    </div>
  );
};

interface ToastContainerProps {
  toasts: Toast[];
  onRemove: (id: string) => void;
}

export const ToastContainer: Component<ToastContainerProps> = props => {
  return (
    <div class="fixed top-4 right-4 z-50 max-w-sm" role="region" aria-label="Notifications">
      <For each={props.toasts}>
        {toast => <ToastItem toast={toast} onRemove={props.onRemove} />}
      </For>
    </div>
  );
};
