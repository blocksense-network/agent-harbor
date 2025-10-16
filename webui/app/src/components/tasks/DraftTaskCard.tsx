import { Component, createSignal, createEffect, onMount } from 'solid-js';
import {
  apiClient,
  type DraftTask,
  type DraftUpdate,
  type CreateTaskRequest,
} from '../../lib/api.js';
import { TomSelectComponent } from '../common/TomSelect.js';
import { ModelMultiSelect } from '../common/ModelMultiSelect.js';
import { SaveStatus, type SaveStatusType } from '../common/SaveStatus.js';
import { useFocus } from '../../contexts/FocusContext.js';

interface Repository {
  id: string;
  name: string;
  url?: string;
  branch?: string;
  keywords?: string[];
}

interface DraftTaskCardProps {
  draft: DraftTask;
  isSelected?: boolean; // Keyboard navigation selection
  onDebug?: () => void; // Debug callback
  onUpdate: (updates: Partial<DraftTask>) => void;
  onRemove: () => void;
  onTaskCreated?: (taskId: string) => void;
}

export const DraftTaskCard: Component<DraftTaskCardProps> = (props) => {
  // Debug effect for selection changes
  createEffect(() => {
    if (props.onDebug) {
      props.onDebug();
    }
  });

  const [isSubmitting, setIsSubmitting] = createSignal(false);
  const [modelSelections, setModelSelections] = createSignal<
    Array<{ model: string; instances: number }>
  >([]);
  const [autoSaveTimeoutId, setAutoSaveTimeoutId] = createSignal<ReturnType<typeof setTimeout>>();
  const [saveStatus, setSaveStatus] = createSignal<SaveStatusType>('saved');
  let textareaRef: HTMLTextAreaElement | undefined;
  const { setDraftFocus, updateDraftAgentCount } = useFocus();

  // Convert draft data to local signals for easier handling
  const [localPrompt, setLocalPrompt] = createSignal(props.draft.prompt || '');
  const [lastSavedPrompt, setLastSavedPrompt] = createSignal(props.draft.prompt || '');

  // Track save requests to prevent race conditions and text truncation
  const [currentSaveRequestId, setCurrentSaveRequestId] = createSignal<number | null>(null);

  let nextSaveRequestId = 1;

  // Non-reactive auto-save function with request tracking to avoid infinite loops
  const scheduleAutoSave = () => {
    const currentPrompt = localPrompt();

    // Don't schedule if no changes to save
    if (currentPrompt === lastSavedPrompt()) {
      return;
    }

    // Mark any existing save request as invalidated by assigning new request ID
    const requestId = nextSaveRequestId++;
    setCurrentSaveRequestId(requestId);

    setSaveStatus('unsaved');

    // Clear any existing timeout
    const id = autoSaveTimeoutId();
    if (id) clearTimeout(id);

    const timeoutId = setTimeout(async () => {
      // Check if this request is still valid (not invalidated by newer typing)
      if (currentSaveRequestId() !== requestId) {
        return; // Silently skip invalidated requests
      }

      setSaveStatus('saving');

      try {
        await props.onUpdate({ prompt: currentPrompt });

        // Check again if request is still valid after async operation
        if (currentSaveRequestId() !== requestId) {
          return; // Silently ignore invalidated requests
        }

        // Always update local state optimistically for better UX
        setLastSavedPrompt(currentPrompt);

        // Update status to saved only if this is still the current request
        if (currentSaveRequestId() === requestId) {
          setSaveStatus('saved');
        }
      } catch {
        // API failed but still update local state optimistically
        setLastSavedPrompt(currentPrompt);
        if (currentSaveRequestId() === requestId) {
          setSaveStatus('saved'); // Show as saved since local state is updated
        }
      }

      setAutoSaveTimeoutId(undefined); // Clear timeout after save completes
    }, 500);

    setAutoSaveTimeoutId(timeoutId);
  };

  // Focus management for keyboard navigation
  createEffect(() => {
    if (props.isSelected && textareaRef && typeof window !== 'undefined') {
      // Prevent interrupting user typing by checking current focus
      if (document.activeElement !== textareaRef) {
        textareaRef.focus();
        setDraftFocus(props.draft.id, computeAgentCount());
      }
    }
  });

  const handleTextareaFocus = () => {
    setDraftFocus(props.draft.id, computeAgentCount());
  };

  const handleTextareaBlur = () => {
    // Don't clear focus immediately - let keyboard navigation handle it
    // This prevents clearing focus when clicking within the same card
  };

  const prompt = () => localPrompt();

  const selectedRepo = (): Repository | null =>
    props.draft.repo
      ? {
          id: props.draft.repo.url || 'unknown',
          name: props.draft.repo.url
            ? props.draft.repo.url.split('/').pop()?.replace('.git', '') || 'unknown'
            : 'unknown',
          ...(props.draft.repo.url !== undefined && {
            url: props.draft.repo.url,
          }),
          ...(props.draft.repo.branch !== undefined && {
            branch: props.draft.repo.branch,
          }),
        }
      : null;

  const setSelectedRepo = (repo: Repository | null) => {
    if (repo) {
      const repoUpdate: DraftUpdate['repo'] = {
        mode: 'git' as const,
        ...(repo.url !== undefined && { url: repo.url }),
        ...(repo.branch !== undefined && { branch: repo.branch }),
      };
      props.onUpdate({ repo: repoUpdate });
    }
  };

  const selectedBranch = () => props.draft.repo?.branch || '';
  const setSelectedBranch = (branch: string | null) => {
    const repoUpdate: DraftUpdate['repo'] = {
      mode: props.draft.repo?.mode || 'git',
      branch: branch || '',
      ...(props.draft.repo?.url !== undefined && { url: props.draft.repo.url }),
    };
    props.onUpdate({ repo: repoUpdate });
  };

  // Mock data - in real app, this would come from API
  const [repositories] = createSignal<Repository[]>([
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
  ]);

  const [availableModels] = createSignal<string[]>([
    'Claude 3.5 Sonnet',
    'Claude 3 Haiku',
    'GPT-4',
    'GPT-3.5 Turbo',
  ]);

  const computeAgentCount = (selections?: Array<{ model: string; instances: number }>) => {
    const source = selections ?? modelSelections();
    if (source.length > 0) {
      return source.reduce((total, selection) => total + selection.instances, 0);
    }

    const draftAgents = props.draft.agents ?? [];
    if (draftAgents.length === 0) {
      return 0;
    }

    return draftAgents.reduce((total, agent) => {
      const withInstances = agent as { instances?: number };
      return total + (withInstances.instances ?? 1);
    }, 0);
  };

  const canSubmit = () => {
    return prompt().trim() && selectedRepo() && selectedBranch() && computeAgentCount() > 0;
  };

  const handleRemove = () => {
    props.onRemove();
  };

  const handleModelSelectionChange = (selections: Array<{ model: string; instances: number }>) => {
    setModelSelections(selections);
    updateDraftAgentCount(props.draft.id, computeAgentCount(selections));
    // Convert model selections to agent format
    const agents = selections.map((sel) => {
      const [type, ...versionParts] = sel.model.toLowerCase().split(' ');
      return {
        type: type || 'unknown',
        version: versionParts.join('-') || 'latest',
        instances: sel.instances,
      };
    });
    props.onUpdate({ agents });
  };

  const handleSubmit = async () => {
    if (!canSubmit() || isSubmitting()) return;

    setIsSubmitting(true);
    try {
      // Use the first selected agent for the task creation
      const primaryAgent = props.draft.agents?.[0];
      if (!primaryAgent) {
        throw new Error('No agent selected');
      }

      const selectedRepoData = selectedRepo();
      if (!selectedRepoData?.url) {
        throw new Error('No repository selected');
      }

      const taskData: CreateTaskRequest = {
        prompt: prompt(),
        repo: {
          mode: 'git' as const,
          url: selectedRepoData.url,
          branch: selectedBranch(),
        },
        runtime: {
          type: 'devcontainer' as const, // Default runtime
        },
        agent: {
          type: primaryAgent.type,
          version: primaryAgent.version,
        },
      };

      const response = await apiClient.createTask(taskData);
      props.onTaskCreated?.(response.id);

      // The draft will be removed by the parent component
    } catch (error) {
      console.error('Failed to create task:', error);
      // TODO: Show error message
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleKeyDown = (e: KeyboardEvent) => {
    // Enter key launches task (if valid)
    if (e.key === 'Enter' && !e.shiftKey) {
      if (canSubmit()) {
        e.preventDefault();
        handleSubmit();
      }
    }
    // Shift+Enter creates new line (default browser behavior, but we track it)
    // No need to prevent default - let the browser handle it
  };

  onMount(() => {
    // Initialize model selections from draft agents if available
    if (props.draft.agents && props.draft.agents.length > 0) {
      const initialSelections = props.draft.agents.map((agent) => ({
        model: `${
          agent.type.charAt(0).toUpperCase() + agent.type.slice(1)
        } ${agent.version.replace(/-/g, ' ')}`,
        instances: (agent as { instances?: number }).instances || 1,
      }));
      setModelSelections(initialSelections);
      updateDraftAgentCount(props.draft.id, computeAgentCount(initialSelections));
    }
  });

  createEffect(() => {
    const agents = props.draft.agents || [];
    if (agents.length === 0) {
      return;
    }

    const selections = agents.map((agent) => ({
      model: `${
        agent.type.charAt(0).toUpperCase() + agent.type.slice(1)
      } ${agent.version.replace(/-/g, ' ')}`,
      instances: (agent as { instances?: number }).instances || 1,
    }));

    const currentKey = JSON.stringify(modelSelections());
    const nextKey = JSON.stringify(selections);
    if (currentKey !== nextKey) {
      setModelSelections(selections);
    }
    updateDraftAgentCount(props.draft.id, computeAgentCount(selections));
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
      {/* Close button - upper right corner */}
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

      {/* Task description textarea - always visible */}
      <div class="relative mb-3">
        <textarea
          ref={textareaRef}
          data-testid="draft-task-textarea"
          value={prompt()}
          onInput={(e) => {
            setLocalPrompt(e.currentTarget.value);
            scheduleAutoSave();
          }}
          onKeyDown={handleKeyDown}
          onFocus={handleTextareaFocus}
          onBlur={handleTextareaBlur}
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

        {/* Save status indicator positioned for optimal visibility */}
        <div class="absolute right-2 bottom-2">
          <SaveStatus status={saveStatus()} />
        </div>
      </div>

      {/* Single row: Compact selectors on left, Go button on right */}
      <div class="flex items-center gap-3">
        {/* Left side: balanced selectors with proper widths */}
        <div class="flex flex-col">
          <label for="repo-select" class="sr-only">
            Repository
          </label>
          <TomSelectComponent<Repository>
            id="repo-select"
            items={repositories()}
            selectedItem={selectedRepo()}
            onSelect={setSelectedRepo}
            getDisplayText={(repo: Repository) => repo.name}
            getKey={(repo: Repository) => repo.id}
            getSearchTokens={(repo: Repository) => {
              const base = repo.name.replace(/[^a-z0-9]/gi, '');
              return [base, ...(repo.keywords ?? [])];
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
            items={['main', 'develop', 'feature/new-ui', 'hotfix/bug-fix']}
            selectedItem={selectedBranch()}
            onSelect={setSelectedBranch}
            getDisplayText={(branch) => branch}
            getKey={(branch) => branch}
            getSearchTokens={(branch) => [branch.replace(/[^a-z0-9]/gi, '')]}
            placeholder="Branch"
            class="w-32"
            testId="branch-selector"
          />
        </div>

        <div class="flex flex-col">
          <label for="model-select" class="sr-only">
            Models
          </label>
          <ModelMultiSelect
            availableModels={availableModels()}
            selectedModels={modelSelections()}
            onSelectionChange={handleModelSelectionChange}
            placeholder="Models"
            testId="model-selector"
            class="min-w-48 flex-1"
          />
        </div>

        {/* Right side: Go button */}
        <div class="flex items-center gap-2">
          <button
            onClick={handleSubmit}
            disabled={Boolean(!canSubmit() || isSubmitting())}
            class={`
              rounded-md px-5 py-1.5 text-sm font-medium whitespace-nowrap
              transition-colors
            `}
            classList={{
              'bg-blue-600 text-white hover:bg-blue-700 focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:ring-blue-500 cursor-pointer':
                Boolean(canSubmit() && !isSubmitting()),
              'bg-slate-300 text-slate-500 cursor-not-allowed': Boolean(
                !canSubmit() || isSubmitting()
              ),
            }}
            aria-label="Create task"
          >
            {isSubmitting() ? '...' : 'Go'}
          </button>
        </div>
      </div>
    </div>
  );
};
