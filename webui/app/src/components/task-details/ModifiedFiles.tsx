/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { For, Show } from 'solid-js';

import { getStatusBadge, ModifiedFile } from './mock-data';

type ModifiedFilesProps = {
  files: ModifiedFile[];
  searchQuery: string;
  onSearchChange: (value: string) => void;
  statusFilter: string;
  onStatusFilterChange: (value: string) => void;
  onFileSelect: (file: ModifiedFile) => void;
};

const filters = [
  { label: 'All', value: 'all' },
  { label: 'Modified', value: 'modified' },
  { label: 'Added', value: 'added' },
  { label: 'Deleted', value: 'deleted' },
];

export const ModifiedFiles = (props: ModifiedFilesProps) => (
  <div class="h-2/5 border-b border-gray-200 p-4">
    <h3 class="mb-3 text-sm font-semibold text-gray-900">Modified Files</h3>
    <div class="mb-3 space-y-2">
      <input
        type="text"
        placeholder="Search files..."
        class={`
          w-full rounded-md border border-gray-300 px-3 py-1 text-sm
          focus:border-transparent focus:ring-2 focus:ring-blue-500
          focus:outline-none
        `}
        value={props.searchQuery}
        onInput={event => props.onSearchChange(event.currentTarget.value)}
      />
      <div class="flex space-x-1">
        <For each={filters}>
          {filter => {
            const isActive = props.statusFilter === filter.value;
            const buttonClasses = isActive
              ? 'border border-blue-200 bg-blue-100 text-blue-800'
              : 'bg-gray-100 text-gray-600 hover:bg-gray-200';

            return (
              <button
                class={`
                  rounded-md px-2 py-1 text-xs transition-colors
                  ${buttonClasses}
                `}
                onClick={() => props.onStatusFilterChange(filter.value)}
              >
                {filter.label}
              </button>
            );
          }}
        </For>
      </div>
    </div>
    <div class="max-h-48 space-y-2 overflow-y-auto">
      <For each={props.files}>
        {file => {
          const badge = getStatusBadge(file.status);
          return (
            <div
              class={`
                flex cursor-pointer items-center justify-between rounded p-2
                hover:bg-gray-50
              `}
              onClick={() => props.onFileSelect(file)}
            >
              <div class="flex min-w-0 flex-1 items-center space-x-2">
                <span
                  class={`
                    inline-flex items-center rounded-full border px-1.5 py-1
                    text-xs font-medium
                  `}
                  classList={{
                    [badge.bg]: true,
                    [badge.text]: true,
                    [badge.border]: true,
                  }}
                >
                  {badge.icon}
                </span>
                <span class="truncate text-sm text-gray-900" title={file.path}>
                  {file.path}
                </span>
              </div>
              <div class="flex-shrink-0 text-xs text-gray-500">
                +{file.linesAdded} -{file.linesRemoved}
              </div>
            </div>
          );
        }}
      </For>
      <Show when={props.files.length === 0}>
        <div class="py-4 text-center text-sm text-gray-500">No files match the current filters</div>
      </Show>
    </div>
  </div>
);
