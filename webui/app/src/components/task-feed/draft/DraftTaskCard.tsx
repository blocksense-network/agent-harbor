/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { createEffect, createSignal, onMount } from 'solid-js';

import { useFocus } from '../../../contexts/FocusContext';
import {
  apiClient,
  type CreateTaskRequest,
  type DraftTask,
  type DraftUpdate,
} from '../../../lib/api.js';
import { DraftPromptEditor } from './DraftPromptEditor';
import {
  DraftExecutionControls,
  DEFAULT_BRANCH_OPTIONS,
  DEFAULT_DRAFT_REPOSITORIES,
  DEFAULT_MODEL_OPTIONS,
  type DraftRepository,
} from './DraftExecutionControls';
import { useDraftPromptAutoSave } from './useDraftPromptAutoSave';
import type { ModelSelection } from './ModelMultiSelect';

const agentToModelSelection = (agent: DraftTask['agents'][number]): ModelSelection => ({
  model: `${agent.type.charAt(0).toUpperCase() + agent.type.slice(1)} ${agent.version.replace(/-/g, ' ')}`,
  instances: (agent as { instances?: number }).instances || 1,
});

const selectionsToAgents = (selections: ModelSelection[]): DraftTask['agents'] =>
  selections.map(selection => {
    const [type, ...versionParts] = selection.model.toLowerCase().split(' ');
    return {
      type: type || 'unknown',
      version: versionParts.join('-') || 'latest',
      instances: selection.instances,
    };
  });

const toRepository = (draft: DraftTask): DraftRepository | null =>
  draft.repo
    ? {
        id: draft.repo.url || 'unknown',
        name:
          draft.repo.url?.split('/').pop()?.replace('.git', '') || draft.repo.branch || 'unknown',
        ...(draft.repo.url !== undefined && { url: draft.repo.url }),
        ...(draft.repo.branch !== undefined && { branch: draft.repo.branch }),
      }
    : null;

const buildRepositoryUpdate = (
  draft: DraftTask,
  repository: DraftRepository,
): NonNullable<DraftUpdate['repo']> => ({
  mode: (draft.repo?.mode ?? 'git') as DraftTask['repo']['mode'],
  ...(repository.url !== undefined && { url: repository.url }),
  ...(repository.branch !== undefined && { branch: repository.branch }),
});

const buildBranchUpdate = (
  draft: DraftTask,
  branch: string | null,
): NonNullable<DraftUpdate['repo']> => ({
  mode: (draft.repo?.mode ?? 'git') as DraftTask['repo']['mode'],
  branch: branch || '',
  ...(draft.repo?.url !== undefined && { url: draft.repo.url }),
});

const countAgents = (selections: ModelSelection[]): number =>
  selections.reduce((total, selection) => total + selection.instances, 0);

export type DraftTaskCardProps = {
  draft: DraftTask;
  isSelected?: boolean;
  onDebug?: () => void;
  onUpdate: (updates: Partial<DraftTask>) => Promise<void> | void;
  onRemove: () => void;
  onTaskCreated?: (taskId: string) => void;
};

export const DraftTaskCard = (props: DraftTaskCardProps) => {
  const { setDraftFocus, updateDraftAgentCount } = useFocus();
  const [isSubmitting, setIsSubmitting] = createSignal(false);
  const [modelSelections, setModelSelections] = createSignal<ModelSelection[]>([]);
  let textareaRef: HTMLTextAreaElement | undefined;

  const handleTextareaRef = (element: HTMLTextAreaElement) => {
    textareaRef = element;
  };

  const { prompt, saveStatus, scheduleAutoSave } = useDraftPromptAutoSave({
    initialPrompt: () => props.draft.prompt || '',
    onSave: async value => {
      await props.onUpdate({ prompt: value });
    },
  });

  createEffect(() => {
    if (props.onDebug) {
      props.onDebug();
    }
  });

  createEffect(() => {
    if (props.isSelected && textareaRef && typeof window !== 'undefined') {
      if (document.activeElement !== textareaRef) {
        textareaRef.focus();
        setDraftFocus(props.draft.id, countAgents(modelSelections()));
      }
    }
  });

  const handleTextareaFocus = () => {
    setDraftFocus(props.draft.id, countAgents(modelSelections()));
  };

  const handleModelSelectionChange = (selections: ModelSelection[]) => {
    setModelSelections(selections);
    updateDraftAgentCount(props.draft.id, countAgents(selections));
    void props.onUpdate({ agents: selectionsToAgents(selections) });
  };

  const handleRemove = () => {
    props.onRemove();
  };

  const selectedRepository = () => toRepository(props.draft);

  const handleRepositoryChange = (repository: DraftRepository | null) => {
    if (!repository) {
      return;
    }
    void props.onUpdate({ repo: buildRepositoryUpdate(props.draft, repository) });
  };

  const selectedBranch = () => props.draft.repo?.branch || '';

  const handleBranchChange = (branch: string | null) => {
    void props.onUpdate({ repo: buildBranchUpdate(props.draft, branch) });
  };

  const canSubmit = () =>
    Boolean(
      prompt().trim() &&
        selectedRepository() &&
        selectedBranch() &&
        countAgents(modelSelections()) > 0,
    );

  const handleSubmit = async () => {
    if (!canSubmit() || isSubmitting()) return;

    setIsSubmitting(true);
    try {
      const selections = modelSelections();
      const agents = selections.length > 0 ? selectionsToAgents(selections) : props.draft.agents;
      const primaryAgent = agents?.[0];
      if (!primaryAgent) {
        throw new Error('No agent selected');
      }

      const repository = selectedRepository();
      if (!repository?.url) {
        throw new Error('No repository selected');
      }

      const taskData: CreateTaskRequest = {
        prompt: prompt(),
        repo: {
          mode: 'git' as const,
          url: repository.url,
          branch: selectedBranch(),
        },
        runtime: {
          type: 'devcontainer' as const,
        },
        agent: {
          type: primaryAgent.type,
          version: primaryAgent.version,
        },
      };

      const response = await apiClient.createTask(taskData);
      response.session_ids.map(id => props.onTaskCreated?.(id));
    } catch (error) {
      console.error('Failed to create task:', error);
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleKeyDown = (event: KeyboardEvent) => {
    if (event.key === 'Enter' && !event.shiftKey) {
      if (canSubmit()) {
        event.preventDefault();
        void handleSubmit();
      }
    }
  };

  const syncModelSelectionsFromDraft = () => {
    const agents = props.draft.agents || [];
    if (agents.length === 0) {
      return;
    }

    const selections = agents.map(agentToModelSelection);
    const currentKey = JSON.stringify(modelSelections());
    const nextKey = JSON.stringify(selections);
    if (currentKey !== nextKey) {
      setModelSelections(selections);
    }
    updateDraftAgentCount(props.draft.id, countAgents(selections));
  };

  onMount(() => {
    syncModelSelectionsFromDraft();
  });

  createEffect(() => {
    syncModelSelectionsFromDraft();
  });

  return (
    <div
      data-testid="draft-task-card"
      class="relative rounded-lg p-4"
      classList={{
        'bg-blue-50 border-2 border-blue-500': props.isSelected,
        'bg-white border border-slate-200': !props.isSelected,
      }}
    >
      <button
        onClick={handleRemove}
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

      <DraftPromptEditor
        value={prompt()}
        onValueChange={scheduleAutoSave}
        onKeyDown={handleKeyDown}
        onFocus={handleTextareaFocus}
        textareaRef={handleTextareaRef}
        saveStatus={saveStatus()}
      />

      <DraftExecutionControls
        repositories={DEFAULT_DRAFT_REPOSITORIES}
        selectedRepository={selectedRepository()}
        onRepositoryChange={handleRepositoryChange}
        branches={DEFAULT_BRANCH_OPTIONS}
        selectedBranch={selectedBranch()}
        onBranchChange={handleBranchChange}
        availableModels={DEFAULT_MODEL_OPTIONS}
        selections={modelSelections()}
        onSelectionChange={handleModelSelectionChange}
        canSubmit={canSubmit()}
        isSubmitting={isSubmitting()}
        onSubmit={handleSubmit}
      />
    </div>
  );
};
