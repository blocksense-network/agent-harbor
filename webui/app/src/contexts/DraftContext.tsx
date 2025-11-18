/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import {
  JSX,
  createContext,
  useContext,
  Component,
  createEffect,
  createMemo,
  createSignal,
  onCleanup,
  onMount,
} from 'solid-js';

import { apiClient, DraftTask, DraftCreate, DraftUpdate, CreateTaskRequest } from '../lib/api';
import { useFocus } from './FocusContext';
import { useDraftPromptAutoSave } from '../components/task-feed/draft/useDraftPromptAutoSave';
import type { AgentSelection } from '../components/task-feed/draft/ModelMultiSelect';
import {
  agentToAgentSelection,
  buildBranchUpdate,
  buildRepositoryUpdate,
  countAgents,
  selectionsToAgents,
  toRepository,
  type DraftRepository,
} from '../components/task-feed/draft/draftCardUtils';
import type { SaveStatusType } from '../components/task-feed/draft/SaveStatus';

const createFallbackDraft = (): DraftTask => ({
  id: 'local-draft-new',
  prompt: '',
  repo: { mode: 'git', url: '', branch: 'main' },
  agents: [],
  runtime: { type: 'devcontainer' },
  delivery: { mode: 'pr' },
  createdAt: new Date().toISOString(),
  updatedAt: new Date().toISOString(),
});

// DraftContext provides CRUD operations for drafts
// Draft data itself comes from route-level fetching via props (progressive enhancement)
interface DraftContextValue {
  error: () => string | null;
  drafts: () => DraftTask[];
  setInitialDrafts: (drafts: DraftTask[] | undefined) => void;
  requestDraftsRefresh: () => void;
  handleDraftTaskCreated: (draft: DraftTask, taskId: string) => Promise<void>;
  registerDraftCallbacks: (callbacks: {
    onTaskCreated?: (taskId: string) => void;
    onDraftRemoved?: (draftId: string) => void;
  }) => void;
  createDraft: (draft: DraftCreate) => Promise<DraftTask | null>;
  removeDraft: (id: string) => Promise<boolean>;
  updateDraft: (id: string, updates: Partial<DraftUpdate>) => Promise<boolean>;
  onDraftChanged?: () => void; // Callback for components to refetch after changes
}

const DraftContext = createContext<DraftContextValue>();

interface DraftProviderProps {
  children: JSX.Element;
  onDraftChanged?: () => void; // Optional callback when drafts change
}

export const DraftProvider: Component<DraftProviderProps> = props => {
  const [error, setError] = createSignal<string | null>(null);
  const [clientDrafts, setClientDrafts] = createSignal<DraftTask[]>([]);
  const [refreshTrigger, setRefreshTrigger] = createSignal(0);
  let draftCallbacks: {
    onTaskCreated?: (taskId: string) => void;
    onDraftRemoved?: (draftId: string) => void;
  } = {};
  const notifyDraftChanged = () => props.onDraftChanged?.();

  const drafts = createMemo<DraftTask[]>(() => {
    const items = clientDrafts();
    if (items.length === 0) {
      return [createFallbackDraft()];
    }
    return items;
  });

  const setInitialDrafts = (initialDrafts: DraftTask[] | undefined) => {
    setClientDrafts(initialDrafts ?? []);
  };

  const requestDraftsRefresh = () => {
    setRefreshTrigger(prev => prev + 1);
  };

  const refetchDrafts = async () => {
    if (typeof window === 'undefined') {
      return;
    }

    try {
      const data = await apiClient.listDrafts();
      setClientDrafts(data.items ?? []);
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to load draft tasks';
      setError(errorMessage);
      console.error('Failed to fetch drafts:', err);
    }
  };

  createEffect(() => {
    refreshTrigger();
    if (typeof window !== 'undefined') {
      void refetchDrafts();
    }
  });

  onMount(() => {
    if (typeof window === 'undefined') {
      return;
    }

    const handleDraftCreated = () => {
      requestDraftsRefresh();
    };

    window.addEventListener('draft-created', handleDraftCreated);

    onCleanup(() => {
      window.removeEventListener('draft-created', handleDraftCreated);
    });
  });

  const createDraft = async (draft: DraftCreate): Promise<DraftTask | null> => {
    try {
      setError(null);
      if (!import.meta.env.PROD) {
        console.log('[DraftContext] Creating draft...', draft);
      }
      const fullDraft = await apiClient.createDraft(draft);
      if (!import.meta.env.PROD) {
        console.log('[DraftContext] Draft created:', fullDraft);
      }

      setClientDrafts(prev => [fullDraft, ...prev]);

      notifyDraftChanged();

      // Dispatch custom event for components to listen to
      if (typeof window !== 'undefined') {
        if (!import.meta.env.PROD) {
          console.log('[DraftContext] Dispatching draft-created event');
        }
        window.dispatchEvent(new CustomEvent('draft-created', { detail: fullDraft }));
      }

      return fullDraft;
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to create draft';
      setError(errorMessage);
      console.error('Failed to create draft:', err);
      return null;
    }
  };

  const removeDraft = async (id: string): Promise<boolean> => {
    try {
      setError(null);
      await apiClient.deleteDraft(id);
      setClientDrafts(prev => prev.filter(draft => draft.id !== id));
      notifyDraftChanged();
      draftCallbacks.onDraftRemoved?.(id);
      return true;
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to delete draft';
      setError(errorMessage);
      console.error('Failed to delete draft:', err);
      return false;
    }
  };

  const updateDraft = async (id: string, updates: Partial<DraftUpdate>): Promise<boolean> => {
    try {
      setError(null);
      await apiClient.updateDraft(id, updates);
      setClientDrafts(prev =>
        prev.map(current => {
          if (current.id !== id) {
            return current;
          }

          const nextMode = updates.repo?.mode || current.repo?.mode || 'git';
          const nextRepo = updates.repo
            ? {
                ...current.repo,
                ...updates.repo,
                mode: nextMode as DraftTask['repo']['mode'],
              }
            : current.repo;

          return {
            ...current,
            ...updates,
            repo: nextRepo,
          } as DraftTask;
        }),
      );
      notifyDraftChanged();
      return true;
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to update draft';
      setError(errorMessage);
      console.error(`Failed to update draft:`, err);
      return false;
    }
  };

  const handleDraftTaskCreated = async (draft: DraftTask, taskId: string) => {
    const success = await removeDraft(draft.id);
    if (!success) {
      setClientDrafts(prev => prev.filter(item => item.id !== draft.id));
    }
    draftCallbacks.onTaskCreated?.(taskId);
  };

  const registerDraftCallbacks: DraftContextValue['registerDraftCallbacks'] = callbacks => {
    draftCallbacks = callbacks;
  };

  const value: DraftContextValue = {
    error,
    drafts,
    setInitialDrafts,
    requestDraftsRefresh,
    handleDraftTaskCreated,
    registerDraftCallbacks,
    createDraft,
    removeDraft,
    updateDraft,
  };

  return <DraftContext.Provider value={value}>{props.children}</DraftContext.Provider>;
};

export const useDrafts = () => {
  const context = useContext(DraftContext);
  if (!context) {
    throw new Error('useDrafts must be used within a DraftProvider');
  }
  return context;
};

export type DraftCardState = {
  draft: () => DraftTask;
  isSelected: () => boolean;
  prompt: () => string;
  saveStatus: () => SaveStatusType;
  registerPromptTextarea: (element: HTMLTextAreaElement) => void;
  schedulePromptSave: (value: string) => void;
  onPromptKeyDown: (event: KeyboardEvent) => void;
  onPromptFocus: () => void;
  selectedRepository: () => DraftRepository | null;
  changeRepository: (repository: DraftRepository | null) => void;
  selectedBranch: () => string;
  changeBranch: (branch: string | null) => void;
  AgentSelections: () => AgentSelection[];
  changeAgentSelections: (selections: AgentSelection[]) => void;
  canSubmit: () => boolean;
  isSubmitting: () => boolean;
  submit: () => Promise<void>;
  removeDraft: () => Promise<void>;
};

const DraftCardStateContext = createContext<DraftCardState>();

const createDraftCardState = (getDraft: () => DraftTask): DraftCardState => {
  const { setDraftFocus, updateDraftAgentCount, keyboardSelectedIndex } = useFocus();
  const { drafts, updateDraft, removeDraft, handleDraftTaskCreated, requestDraftsRefresh } =
    useDrafts();

  const [isSubmitting, setIsSubmitting] = createSignal(false);
  const [AgentSelections, setAgentSelections] = createSignal<AgentSelection[]>([]);
  let textareaRef: HTMLTextAreaElement | undefined;

  const draft = createMemo(() => getDraft());

  const draftIndex = createMemo(() => drafts().findIndex(item => item.id === draft().id));
  const isSelected = createMemo(() => keyboardSelectedIndex() === draftIndex());

  const registerPromptTextarea = (element: HTMLTextAreaElement) => {
    textareaRef = element;
  };

  const autoSave = useDraftPromptAutoSave({
    initialPrompt: () => draft().prompt || '',
    onSave: async value => {
      await updateDraft(draft().id, { prompt: value });
    },
  });

  createEffect(() => {
    if (isSelected() && textareaRef && typeof window !== 'undefined') {
      if (document.activeElement !== textareaRef) {
        textareaRef.focus();
        setDraftFocus(draft().id, countAgents(AgentSelections()));
      }
    }
  });

  const handlePromptFocus = () => {
    setDraftFocus(draft().id, countAgents(AgentSelections()));
  };

  const changeAgentSelections = (selections: AgentSelection[]) => {
    setAgentSelections(selections);
    updateDraftAgentCount(draft().id, countAgents(selections));
    void updateDraft(draft().id, { agents: selectionsToAgents(selections) });
  };

  const remove = async () => {
    const success = await removeDraft(draft().id);
    if (!success) {
      requestDraftsRefresh();
    }
  };

  const selectedRepository = createMemo(() => toRepository(draft()));

  const changeRepository = (repository: DraftRepository | null) => {
    if (!repository) {
      return;
    }
    void updateDraft(draft().id, {
      repo: buildRepositoryUpdate(draft(), repository),
    });
  };

  const selectedBranch = createMemo(() => draft().repo?.branch || '');

  const changeBranch = (branch: string | null) => {
    void updateDraft(draft().id, {
      repo: buildBranchUpdate(draft(), branch),
    });
  };

  const canSubmit = createMemo(() =>
    Boolean(
      autoSave.prompt().trim() &&
        selectedRepository() &&
        selectedBranch() &&
        countAgents(AgentSelections()) > 0,
    ),
  );

  const submit = async () => {
    if (!canSubmit() || isSubmitting()) return;

    setIsSubmitting(true);
    try {
      const selections = AgentSelections();
      const agents = selections.length > 0 ? selectionsToAgents(selections) : draft().agents;
      const primaryAgent = agents?.[0];
      if (!primaryAgent) {
        throw new Error('No agent selected');
      }

      const repository = selectedRepository();
      if (!repository?.url) {
        throw new Error('No repository selected');
      }

      const taskData: CreateTaskRequest = {
        prompt: autoSave.prompt(),
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
      await Promise.all(response.session_ids.map(id => handleDraftTaskCreated(draft(), id)));
    } catch (error) {
      console.error('Failed to create task:', error);
    } finally {
      setIsSubmitting(false);
    }
  };

  const handlePromptKeyDown = (event: KeyboardEvent) => {
    if (event.key === 'Enter' && !event.shiftKey) {
      if (canSubmit()) {
        event.preventDefault();
        void submit();
      }
    }
  };

  const syncAgentSelectionsFromDraft = () => {
    const agents = draft().agents || [];
    if (agents.length === 0) {
      return;
    }

    const selections = agents.map(agentToAgentSelection);
    const currentKey = JSON.stringify(AgentSelections());
    const nextKey = JSON.stringify(selections);
    if (currentKey !== nextKey) {
      setAgentSelections(selections);
    }
    updateDraftAgentCount(draft().id, countAgents(selections));
  };

  onMount(() => {
    syncAgentSelectionsFromDraft();
  });

  createEffect(() => {
    syncAgentSelectionsFromDraft();
  });

  return {
    draft,
    isSelected,
    prompt: autoSave.prompt,
    saveStatus: autoSave.saveStatus,
    registerPromptTextarea,
    schedulePromptSave: autoSave.scheduleAutoSave,
    onPromptKeyDown: handlePromptKeyDown,
    onPromptFocus: handlePromptFocus,
    selectedRepository,
    changeRepository,
    selectedBranch,
    changeBranch,
    AgentSelections,
    changeAgentSelections,
    canSubmit,
    isSubmitting,
    submit,
    removeDraft: remove,
  };
};

type DraftCardStateProviderProps = {
  draft: DraftTask;
  children: JSX.Element;
};

export const DraftCardStateProvider: Component<DraftCardStateProviderProps> = props => {
  const state = createDraftCardState(() => props.draft);
  return (
    <DraftCardStateContext.Provider value={state}>{props.children}</DraftCardStateContext.Provider>
  );
};

export const useDraftCardState = (): DraftCardState => {
  const context = useContext(DraftCardStateContext);
  if (!context) {
    throw new Error('useDraftCardState must be used within a DraftCardStateProvider');
  }
  return context;
};
