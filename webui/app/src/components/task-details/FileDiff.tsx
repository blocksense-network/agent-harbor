/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { getStatusBadge, ModifiedFile } from './mock-data';
import { DiffViewer } from './DiffViewer';

type FileDiffProps = {
  file: ModifiedFile;
  index: number;
  totalFiles: number;
  onNavigate: (direction: 'prev' | 'next', currentIndex: number) => void;
  onLoadFullFile?: (file: ModifiedFile) => void;
  diffContent: string;
};

export const FileDiff = (props: FileDiffProps) => {
  const file = () => props.file;
  const anchorId = () => {
    const currentFile = file();
    return currentFile.path.replace(/[^a-zA-Z0-9]/g, '-').toLowerCase();
  };
  const badge = () => getStatusBadge(file().status);
  const handleLoadFullFile = () => props.onLoadFullFile?.(file());

  return (
    <div
      id={anchorId()}
      class={`
        border-b border-gray-200
        last:border-b-0
      `}
    >
      <div class="sticky top-0 z-10 border-b border-gray-300 bg-white p-4">
        <div class="flex items-center justify-between">
          <div class="flex items-center space-x-3">
            <span
              class={`
                inline-flex items-center rounded-full border px-2 py-1 text-xs
                font-medium
              `}
              classList={{
                [badge().bg]: true,
                [badge().text]: true,
                [badge().border]: true,
              }}
            >
              {badge().icon} {badge().label}
            </span>
            <h3 class="font-mono text-lg font-semibold text-gray-900">{file().path}</h3>
            <span class="text-sm text-gray-600">
              +{file().linesAdded} -{file().linesRemoved} lines
            </span>
          </div>
          <div class="flex space-x-2">
            <button
              class={`
                rounded bg-gray-100 px-3 py-1 text-sm
                hover:bg-gray-200
              `}
              onClick={handleLoadFullFile}
            >
              Load Full File
            </button>
            <button
              class={`
                rounded bg-gray-100 px-3 py-1 text-sm
                hover:bg-gray-200
                disabled:cursor-not-allowed disabled:opacity-50
              `}
              disabled={props.index === 0}
              onClick={() => props.onNavigate('prev', props.index)}
            >
              Previous
            </button>
            <button
              class={`
                rounded bg-gray-100 px-3 py-1 text-sm
                hover:bg-gray-200
                disabled:cursor-not-allowed disabled:opacity-50
              `}
              disabled={props.index === props.totalFiles - 1}
              onClick={() => props.onNavigate('next', props.index)}
            >
              Next
            </button>
          </div>
        </div>
      </div>

      <div class="p-4">
        <div class="overflow-x-auto rounded-lg border border-gray-200 bg-white">
          <DiffViewer content={props.diffContent} />
        </div>
      </div>
    </div>
  );
};
