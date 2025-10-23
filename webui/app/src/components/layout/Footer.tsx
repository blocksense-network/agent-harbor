/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */
import { Show, createMemo, createSignal, onMount } from 'solid-js';

type FooterProps = {
  onNewDraft?: () => void;
  agentCount?: number | undefined;
  focusState?: {
    focusedElement: 'draft-textarea' | 'session-card' | 'none';
    focusedDraftId?: string;
    focusedSessionId?: string;
    focusedDraftAgentCount?: number;
  };
};

type FooterContext = 'task-feed' | 'draft-task' | 'modal' | 'default';

export const Footer = (props: FooterProps) => {
  const [isMac, setIsMac] = createSignal(false);

  onMount(() => {
    if (typeof window !== 'undefined') {
      setIsMac(navigator.platform.toUpperCase().indexOf('MAC') >= 0);
    }
  });

  const resolvedAgentCount = createMemo(
    () => props.agentCount ?? props.focusState?.focusedDraftAgentCount,
  );

  const getContext = (): FooterContext => {
    if (!props.focusState) return 'default';

    switch (props.focusState.focusedElement) {
      case 'draft-textarea':
        return 'draft-task';
      case 'session-card':
        return 'task-feed';
      default:
        return 'default';
    }
  };

  const getEnterShortcut = () => {
    const context = getContext();

    switch (context) {
      case 'draft-task': {
        const agentCount = resolvedAgentCount() ?? 0;
        return agentCount > 1 ? 'Launch Agents' : 'Launch Agent';
      }
      case 'task-feed':
        return 'Review Session Details';
      default:
        return 'Go';
    }
  };

  const modKey = () => (isMac() ? 'Cmd' : 'Ctrl');

  return (
    <footer
      class={`
        flex items-center justify-between border-t border-gray-200 bg-white px-4
        py-2 text-sm
      `}
      role="contentinfo"
      aria-label="Keyboard shortcuts"
    >
      <div class="flex items-center" role="toolbar" aria-label="Actions">
        <Show when={props.onNewDraft}>
          <button
            onClick={() => props.onNewDraft?.()}
            class={`
              flex cursor-pointer items-center gap-1 rounded border
              border-blue-700 bg-blue-600 px-3 py-1 text-xs text-white
              transition-colors
              hover:bg-blue-700
            `}
            aria-label={`New draft task (${modKey()}+N)`}
          >
            <kbd class="font-semibold">{modKey()}+N</kbd>
            <span>New Draft Task</span>
          </button>
        </Show>
      </div>

      <div class="flex items-center gap-2" role="toolbar" aria-label="Keyboard shortcuts">
        <Show when={getContext() === 'task-feed'}>
          <div
            class={`
              flex items-center gap-1 rounded border border-gray-200 bg-gray-100
              px-2 py-1 text-xs text-gray-700
            `}
            aria-label="Keyboard shortcut: Navigate between tasks"
          >
            <kbd class="font-semibold">↑↓</kbd>
            <span>Navigate</span>
          </div>
          <div
            class={`
              flex items-center gap-1 rounded border border-gray-200 bg-gray-100
              px-2 py-1 text-xs text-gray-700
            `}
            aria-label={`Keyboard shortcut: Enter ${getEnterShortcut()}`}
          >
            <kbd class="font-semibold">Enter</kbd>
            <span>{getEnterShortcut()}</span>
          </div>
        </Show>

        <Show when={getContext() === 'draft-task'}>
          <div
            class={`
              flex items-center gap-1 rounded border border-gray-200 bg-gray-100
              px-2 py-1 text-xs text-gray-700
            `}
            aria-label={`Keyboard shortcut: Enter ${getEnterShortcut()}`}
          >
            <kbd class="font-semibold">Enter</kbd>
            <span>{getEnterShortcut()}</span>
          </div>
          <div
            class={`
              flex items-center gap-1 rounded border border-gray-200 bg-gray-100
              px-2 py-1 text-xs text-gray-700
            `}
            aria-label="Keyboard shortcut: Shift+Enter inserts a new line"
          >
            <kbd class="font-semibold">Shift+Enter</kbd>
            <span>New Line</span>
          </div>
          <div
            class={`
              flex items-center gap-1 rounded border border-gray-200 bg-gray-100
              px-2 py-1 text-xs text-gray-700
            `}
            aria-label="Keyboard shortcut: Tab moves to the next field"
          >
            <kbd class="font-semibold">Tab</kbd>
            <span>Next Field</span>
          </div>
        </Show>

        <Show when={getContext() === 'default'}>
          <div
            class={`
              flex items-center gap-1 rounded border border-gray-200 bg-gray-100
              px-2 py-1 text-xs text-gray-700
            `}
            aria-label="Keyboard shortcut: Navigate between tasks"
          >
            <kbd class="font-semibold">↑↓</kbd>
            <span>Navigate</span>
          </div>
          <div
            class={`
              flex items-center gap-1 rounded border border-gray-200 bg-gray-100
              px-2 py-1 text-xs text-gray-700
            `}
            aria-label={`Keyboard shortcut: Enter ${getEnterShortcut()}`}
          >
            <kbd class="font-semibold">Enter</kbd>
            <span>{getEnterShortcut()}</span>
          </div>
        </Show>
      </div>
    </footer>
  );
};
