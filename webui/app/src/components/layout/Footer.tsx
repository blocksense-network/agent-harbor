/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */
import { Show, createMemo, createSignal, onCleanup, onMount } from 'solid-js';

import { useDrafts } from '../../contexts/DraftContext';
import { useFocus } from '../../contexts/FocusContext';

export const Footer = () => {
  const { createDraft } = useDrafts();
  const { focusState } = useFocus();
  const [isMac, setIsMac] = createSignal(false);

  onMount(() => {
    if (typeof window === 'undefined') {
      return;
    }

    setIsMac(navigator.platform.toUpperCase().indexOf('MAC') >= 0);

    const handleShortcut = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey)) {
        return;
      }

      if (event.key.toLowerCase() !== 'n') {
        return;
      }

      const target = event.target;
      if (target && target instanceof window.HTMLElement) {
        const tagName = target.tagName?.toLowerCase();
        const isEditable =
          tagName === 'input' || tagName === 'textarea' || target.isContentEditable;
        if (isEditable) {
          return;
        }
      }

      event.preventDefault();
      void handleNewDraft();
    };

    window.addEventListener('keydown', handleShortcut);
    onCleanup(() => window.removeEventListener('keydown', handleShortcut));
  });

  const handleNewDraft = async () => {
    if (!import.meta.env.PROD) {
      console.log('[Footer] New Task button clicked');
    }

    const created = await createDraft({
      prompt: '',
      repo: { mode: 'git', url: '', branch: 'main' },
      agents: [],
      runtime: { type: 'devcontainer' },
      delivery: { mode: 'pr' },
    });

    if (!import.meta.env.PROD) {
      console.log('[Footer] Draft creation result:', created);
    }
  };

  const resolvedAgentCount = createMemo(() => focusState().focusedDraftAgentCount);

  const footerContext = createMemo(() => {
    const state = focusState();

    switch (state.focusedElement) {
      case 'draft-textarea':
        return 'draft-task';
      case 'session-card':
        return 'task-feed';
      default:
        return 'default';
    }
  });

  const enterShortcutLabel = createMemo(() => {
    const context = footerContext();

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
  });

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
        <button
          onClick={() => void handleNewDraft()}
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
      </div>

      <div class="flex items-center gap-2" role="toolbar" aria-label="Keyboard shortcuts">
        <Show when={footerContext() === 'task-feed'}>
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
            aria-label={`Keyboard shortcut: Enter ${enterShortcutLabel()}`}
          >
            <kbd class="font-semibold">Enter</kbd>
            <span>{enterShortcutLabel()}</span>
          </div>
        </Show>

        <Show when={footerContext() === 'draft-task'}>
          <div
            class={`
              flex items-center gap-1 rounded border border-gray-200 bg-gray-100
              px-2 py-1 text-xs text-gray-700
            `}
            aria-label={`Keyboard shortcut: Enter ${enterShortcutLabel()}`}
          >
            <kbd class="font-semibold">Enter</kbd>
            <span>{enterShortcutLabel()}</span>
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

        <Show when={footerContext() === 'default'}>
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
            aria-label={`Keyboard shortcut: Enter ${enterShortcutLabel()}`}
          >
            <kbd class="font-semibold">Enter</kbd>
            <span>{enterShortcutLabel()}</span>
          </div>
        </Show>
      </div>
    </footer>
  );
};
