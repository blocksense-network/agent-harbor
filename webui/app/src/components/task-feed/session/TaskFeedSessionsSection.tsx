/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { For, Show } from 'solid-js';

import { SessionCard } from './SessionCard';
import { useDrafts } from '../../../contexts/DraftContext';
import { useFocus } from '../../../contexts/FocusContext';
import { useSession } from '../../../contexts/SessionContext';

export const TaskFeedSessionsSection = () => {
  const { drafts } = useDrafts();
  const { filteredSessions, sessionsData } = useSession();
  const { keyboardSelectedIndex } = useFocus();

  const draftsCount = () => drafts().length;
  const sessionsInfo = () => sessionsData();

  const announcementText = () => {
    const idx = keyboardSelectedIndex();
    const draftTotal = draftsCount();
    if (idx >= 0 && idx < draftTotal) {
      return `Selected draft: ${drafts()[idx]?.prompt || 'New task'}`;
    }
    if (idx >= draftTotal) {
      const session = filteredSessions()[idx - draftTotal];
      if (session) {
        return `Selected task: ${session.prompt}`;
      }
    }
    return '';
  };

  return (
    <>
      <Show when={filteredSessions().length > 0}>
        <div class={draftsCount() > 0 ? 'mt-6' : ''}>
          <ul role="listbox" class="space-y-3">
            <For each={filteredSessions()}>
              {session => (
                <li role="option">
                  <SessionCard session={session} />
                </li>
              )}
            </For>
          </ul>

          <div role="status" aria-live="polite" aria-atomic="true" classList={{ 'sr-only': true }}>
            {announcementText()}
          </div>

          <Show when={sessionsInfo().pagination.totalPages > 1}>
            <div class="mt-4 text-center text-sm text-gray-500" role="status">
              Showing {filteredSessions().length} of {sessionsInfo().pagination.total} sessions
            </div>
          </Show>
        </div>
      </Show>

      <Show when={sessionsInfo().items.length > 0 && filteredSessions().length === 0}>
        <div class="py-8 text-center text-sm text-gray-500" role="status" aria-live="polite">
          No sessions match the selected filter.
        </div>
      </Show>

      <Show when={sessionsInfo().items.length === 0 && draftsCount() === 0}>
        <div class="py-8 text-center" role="status" aria-live="polite">
          <svg
            class="mx-auto h-12 w-12 text-gray-400"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            aria-hidden="true"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
            />
          </svg>
          <h3 class="mt-2 text-sm font-medium text-gray-900">No tasks</h3>
          <p class="mt-1 text-sm text-gray-500">Get started by creating a new task.</p>
        </div>
      </Show>
    </>
  );
};
