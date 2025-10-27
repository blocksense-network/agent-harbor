/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import type { Accessor } from 'solid-js';
import { For, Show } from 'solid-js';

import type { DraftTask } from '../../../lib/api.js';
import { DraftTaskCard } from './DraftTaskCard';

export type TaskFeedDraftsSectionProps = {
  drafts: Accessor<DraftTask[]>;
  keyboardSelectedIndex: Accessor<number>;
  onUpdateDraft: (draft: DraftTask, updates: Partial<DraftTask>) => void | Promise<void>;
  onRemoveDraft: (draft: DraftTask) => void | Promise<void>;
  onTaskCreated: (draft: DraftTask, taskId: string) => void | Promise<void>;
};

export const TaskFeedDraftsSection = (props: TaskFeedDraftsSectionProps) => (
  <Show when={props.drafts().length > 0}>
    <div class="space-y-3">
      <For each={props.drafts()}>
        {(draft, draftIndex) => (
          <div id={`draft-task-${draft.id}`}>
            <DraftTaskCard
              draft={draft}
              isSelected={props.keyboardSelectedIndex() === draftIndex()}
              onUpdate={updates => props.onUpdateDraft(draft, updates)}
              onRemove={() => props.onRemoveDraft(draft)}
              onTaskCreated={taskId => props.onTaskCreated(draft, taskId)}
            />
          </div>
        )}
      </For>
    </div>
  </Show>
);
