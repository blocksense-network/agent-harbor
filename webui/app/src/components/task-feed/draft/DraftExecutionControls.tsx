/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { ModelMultiSelect } from './ModelMultiSelect';
import { TomSelectComponent } from '../../common/TomSelect';
import type { DraftRepository } from './draftCardUtils';
import { useDraftCardState } from '../../../contexts/DraftContext';

export const DEFAULT_DRAFT_REPOSITORIES: DraftRepository[] = [
  {
    id: '1',
    name: 'agent-harbor-webui',
    url: 'https://github.com/example/agent-harbor-webui.git',
    keywords: ['webui', 'ahwebui', 'awwebui', 'agentharborwebui'],
  },
  {
    id: '2',
    name: 'agent-harbor-core',
    url: 'https://github.com/example/agent-harbor-core.git',
    keywords: ['core', 'ahcore', 'awcore', 'agentharborcore'],
  },
  {
    id: '3',
    name: 'agent-harbor-cli',
    url: 'https://github.com/example/agent-harbor-cli.git',
    keywords: ['cli', 'ahcli', 'awcli', 'agentharborcli'],
  },
];

export const DEFAULT_BRANCH_OPTIONS = ['main', 'develop', 'feature/new-ui', 'hotfix/bug-fix'];
export const DEFAULT_MODEL_OPTIONS = [
  'Claude 3.5 Sonnet',
  'Claude 3 Haiku',
  'GPT-4',
  'GPT-3.5 Turbo',
];

export const DraftExecutionControls = () => {
  const state = useDraftCardState();

  return (
    <div class="flex flex-wrap items-center gap-3">
      <div class="flex flex-col">
        <label for="repo-select" class="sr-only">
          Repository
        </label>
        <TomSelectComponent<DraftRepository>
          id="repo-select"
          items={DEFAULT_DRAFT_REPOSITORIES}
          selectedItem={state.selectedRepository()}
          onSelect={state.changeRepository}
          getDisplayText={(repository: DraftRepository) => repository.name}
          getKey={(repository: DraftRepository) => repository.id}
          getSearchTokens={(repository: DraftRepository) => {
            const base = repository.name.replace(/[^a-z0-9]/gi, '');
            return [base, ...(repository.keywords ?? [])];
          }}
          placeholder="Repository"
          class="w-48"
          testId="repo-selector"
        />
      </div>

      <div class="flex flex-col">
        <label for="branch-select" class="sr-only">
          Branch
        </label>
        <TomSelectComponent
          id="branch-select"
          items={DEFAULT_BRANCH_OPTIONS}
          selectedItem={state.selectedBranch()}
          onSelect={state.changeBranch}
          getDisplayText={(branch: string) => branch}
          getKey={(branch: string) => branch}
          getSearchTokens={(branch: string) => [branch.replace(/[^a-z0-9]/gi, '')]}
          placeholder="Branch"
          class="w-32"
          testId="branch-selector"
        />
      </div>

      <div class="flex min-w-48 flex-col">
        <label for="model-select" class="sr-only">
          Models
        </label>
        <ModelMultiSelect
          availableModels={DEFAULT_MODEL_OPTIONS}
          selectedModels={state.modelSelections()}
          onSelectionChange={state.changeModelSelections}
          placeholder="Models"
          testId="model-selector"
          class="flex-1"
        />
      </div>

      <div class="flex items-center gap-2">
        <button
          onClick={() => {
            void state.submit();
          }}
          disabled={Boolean(!state.canSubmit() || state.isSubmitting())}
          class={`
            rounded-md px-5 py-1.5 text-sm font-medium whitespace-nowrap
            transition-colors
          `}
          classList={{
            'bg-blue-600 text-white hover:bg-blue-700 focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:ring-blue-500 cursor-pointer':
              Boolean(state.canSubmit() && !state.isSubmitting()),
            'bg-slate-300 text-slate-500 cursor-not-allowed': Boolean(
              !state.canSubmit() || state.isSubmitting(),
            ),
          }}
          aria-label="Create task"
        >
          {state.isSubmitting() ? '...' : 'Go'}
        </button>
      </div>
    </div>
  );
};
