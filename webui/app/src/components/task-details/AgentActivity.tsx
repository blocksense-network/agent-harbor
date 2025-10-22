/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { For } from 'solid-js';

import { mockAgentEvents } from './mock-data';

export const AgentActivity = () => (
  <div class="flex h-3/5 flex-col p-4">
    <h3 class="mb-3 text-sm font-semibold text-gray-900">Agent Activity</h3>
    <div class="flex-1 space-y-2 overflow-y-auto">
      <For each={mockAgentEvents}>
        {event => (
          <div class="flex space-x-3">
            <span class="w-16 flex-shrink-0 text-xs text-gray-500">{event.timestamp}</span>
            <div class="min-w-0 flex-1">
              {event.type === 'thinking' && (
                <div class="text-sm text-gray-700">
                  <span class="font-medium">💭 </span>
                  {event.content}
                </div>
              )}
              {event.type === 'tool' && (
                <div class="text-sm text-gray-700">
                  <span class="font-medium">🔧 </span>
                  {event.content}
                  {event.lastLine && (
                    <div class="mt-1 pl-4 font-mono text-xs text-gray-600">{event.lastLine}</div>
                  )}
                </div>
              )}
              {event.type === 'file_edit' && (
                <div class="text-sm text-gray-700">
                  <span class="font-medium">📝 </span>
                  {event.content}
                </div>
              )}
            </div>
          </div>
        )}
      </For>
    </div>
  </div>
);
