/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { For, Show } from 'solid-js';

import { DraftTaskCard } from './DraftTaskCard';
import { useDrafts } from '../../../contexts/DraftContext';

export const TaskFeedDraftsSection = () => {
  const { drafts } = useDrafts();

  return (
    <Show when={drafts().length > 0}>
      <div class="space-y-3">
        <For each={drafts()}>{draft => <DraftTaskCard draft={draft} />}</For>
      </div>
    </Show>
  );
};
