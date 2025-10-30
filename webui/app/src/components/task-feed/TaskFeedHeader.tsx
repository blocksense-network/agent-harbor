/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { For } from 'solid-js';

type TaskFeedHeaderProps = {
  statusFilter: string;
  statusOptions: { value: string; label: string }[];
  onStatusChange: (value: string) => void;
};

export const TaskFeedHeader = (props: TaskFeedHeaderProps) => (
  <div class="border-b border-gray-200 bg-white px-6 py-4">
    <div
      class={`
        flex flex-col gap-3
        md:flex-row md:items-center md:justify-between
      `}
    >
      <h2 class="text-lg font-semibold text-gray-900">Task Feed</h2>

      <div class="flex items-center gap-3" role="group" aria-label="Task filters">
        <label for="status-filter" class="text-sm font-medium text-gray-700">
          Status
        </label>
        <select
          id="status-filter"
          data-testid="status-filter"
          class={`
            rounded-md border border-gray-300 bg-white px-3 py-1 text-sm
            text-gray-700 shadow-sm transition
            focus:border-blue-500 focus:ring-2 focus:ring-blue-500
            focus:outline-none
          `}
          aria-label="Filter sessions by status"
          value={props.statusFilter}
          onInput={event => {
            props.onStatusChange(event.currentTarget.value);
          }}
        >
          <For each={props.statusOptions}>
            {option => <option value={option.value}>{option.label}</option>}
          </For>
        </select>
      </div>
    </div>
  </div>
);
