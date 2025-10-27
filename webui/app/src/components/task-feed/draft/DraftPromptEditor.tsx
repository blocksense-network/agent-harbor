/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { SaveStatus, type SaveStatusType } from './SaveStatus';

type DraftPromptEditorProps = {
  value: string;
  onValueChange: (value: string) => void;
  onKeyDown?: (event: KeyboardEvent) => void;
  onFocus?: () => void;
  onBlur?: () => void;
  textareaRef?: (element: HTMLTextAreaElement) => void;
  saveStatus: SaveStatusType;
};

export const DraftPromptEditor = (props: DraftPromptEditorProps) => (
  <div class="relative mb-3">
    <textarea
      ref={props.textareaRef}
      data-testid="draft-task-textarea"
      value={props.value}
      onInput={event => {
        props.onValueChange(event.currentTarget.value);
      }}
      onKeyDown={event => props.onKeyDown?.(event)}
      onFocus={() => props.onFocus?.()}
      onBlur={() => props.onBlur?.()}
      placeholder="Describe what you want the agent to do..."
      class={`
        w-full resize-none rounded-md border border-slate-200 p-3 pr-20 text-sm
        focus:border-transparent focus:ring-2 focus:ring-blue-500
        focus:outline-none
      `}
      rows="2"
      aria-label="Task description"
    />

    <div class="absolute right-2 bottom-2">
      <SaveStatus status={props.saveStatus} />
    </div>
  </div>
);
