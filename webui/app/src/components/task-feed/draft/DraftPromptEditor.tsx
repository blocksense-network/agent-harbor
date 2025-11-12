/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { SaveStatus } from './SaveStatus';
import { useDraftCardState } from '../../../contexts/DraftContext';

export const DraftPromptEditor = () => {
  const state = useDraftCardState();

  return (
    <div class="relative mb-3">
      <textarea
        ref={state.registerPromptTextarea}
        data-testid="draft-task-textarea"
        value={state.prompt()}
        onInput={event => {
          state.schedulePromptSave(event.currentTarget.value);
        }}
        onKeyDown={state.onPromptKeyDown}
        onFocus={state.onPromptFocus}
        placeholder="Describe what you want the agent to do..."
        class={`
          w-full resize-none rounded-md border border-slate-200 p-3 pr-20
          text-sm
          focus:border-transparent focus:ring-2 focus:ring-blue-500
          focus:outline-none
        `}
        rows="2"
        aria-label="Task description"
      />

      <div class="absolute right-2 bottom-2">
        <SaveStatus status={state.saveStatus()} />
      </div>
    </div>
  );
};
