/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { type DraftTask } from '../../../lib/api.js';
import { DraftPromptEditor } from './DraftPromptEditor';
import { DraftExecutionControls } from './DraftExecutionControls';
import { DraftCardStateProvider, useDraftCardState } from '../../../contexts/DraftContext';

export type DraftTaskCardProps = {
  draft: DraftTask;
};

export const DraftTaskCard = (props: DraftTaskCardProps) => (
  <DraftCardStateProvider draft={props.draft}>
    <DraftTaskCardContent />
  </DraftCardStateProvider>
);

const DraftTaskCardContent = () => {
  const state = useDraftCardState();

  return (
    <article
      id={`draft-task-${state.draft().id}`}
      data-testid="draft-task-card"
      class="relative rounded-lg p-4"
      classList={{
        'bg-blue-50 border-2 border-blue-500': state.isSelected(),
        'bg-white border border-slate-200': !state.isSelected(),
      }}
    >
      <button
        onClick={() => {
          void state.removeDraft();
        }}
        class={`
          absolute top-2 right-2 flex h-6 w-6 cursor-pointer items-center
          justify-center rounded text-slate-400 transition-colors
          hover:bg-red-50 hover:text-red-600
          focus-visible:ring-2 focus-visible:ring-blue-500
          focus-visible:ring-offset-2
        `}
        aria-label="Remove draft"
        title="Remove draft task"
      >
        ✕
      </button>

      <DraftPromptEditor />

      <DraftExecutionControls />
    </article>
  );
};
