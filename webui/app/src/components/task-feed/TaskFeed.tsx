/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { createEffect, createMemo, createSignal, onMount } from 'solid-js';
import { useNavigate } from '@solidjs/router';

import { useFocus } from '../../contexts/FocusContext';
import { useDrafts } from '../../contexts/DraftContext';
import { useSession } from '../../contexts/SessionContext';
import { useToast } from '../../contexts/ToastContext';
import { apiClient, type DraftTask, type Session } from '../../lib/api.js';
import { type SessionsResponse } from '../../lib/server-data.js';
import { TaskFeedHeader } from './TaskFeedHeader';
import { TaskFeedDraftsSection } from './draft/TaskFeedDraftsSection';
import { TaskFeedSessionsSection } from './session/TaskFeedSessionsSection';

type TaskFeedProps = {
  draftTasks?: DraftTask[];
  onDraftTaskCreated?: (taskId: string) => void;
  onDraftTaskRemoved?: (draftId: string) => void;
  initialSessions?: SessionsResponse;
  initialDrafts?: DraftTask[];
};

export const TaskFeed = (props: TaskFeedProps) => {
  const navigate = useNavigate();
  const draftOps = useDrafts();
  const { clearFocus, setSessionFocus } = useFocus();
  const { setSelectedSessionId } = useSession();
  const { addToast, addToastWithActions } = useToast();

  const [refreshTrigger, setRefreshTrigger] = createSignal(0);
  const [statusFilter, setStatusFilter] = createSignal('');
  const statusOptions = [
    { value: '', label: 'All Sessions' },
    { value: 'running', label: 'Running' },
    { value: 'queued', label: 'Queued' },
    { value: 'provisioning', label: 'Provisioning' },
    { value: 'paused', label: 'Paused' },
    { value: 'pausing', label: 'Pausing' },
    { value: 'resuming', label: 'Resuming' },
    { value: 'stopping', label: 'Stopping' },
    { value: 'stopped', label: 'Stopped' },
    { value: 'completed', label: 'Completed' },
    { value: 'failed', label: 'Failed' },
    { value: 'cancelled', label: 'Cancelled' },
  ];

  const [clientDrafts, setClientDrafts] = createSignal<DraftTask[]>(props.initialDrafts || []);
  const [draftsRefreshTrigger, setDraftsRefreshTrigger] = createSignal(0);
  const [keyboardSelectedIndex, setKeyboardSelectedIndex] = createSignal<number>(-1);

  const refetchDrafts = async () => {
    if (typeof window === 'undefined') return;

    try {
      const data = await apiClient.listDrafts();
      setClientDrafts(data.items || []);
    } catch (error) {
      console.error('Failed to fetch drafts:', error);
      addToast('error', 'Failed to load draft tasks. Please refresh the page.');
    }
  };

  createEffect(() => {
    draftsRefreshTrigger();
    if (typeof window !== 'undefined') {
      refetchDrafts();
    }
  });

  createEffect(() => {
    statusFilter();
    setKeyboardSelectedIndex(-1);
    setSelectedSessionId(undefined);
    clearFocus();
  });

  const drafts = (): DraftTask[] => {
    const draftsList = clientDrafts();
    if (draftsList.length === 0) {
      return [
        {
          id: 'local-draft-new',
          prompt: '',
          repo: { mode: 'git', url: '', branch: 'main' },
          agents: [],
          runtime: { type: 'devcontainer' },
          delivery: { mode: 'pr' },
          createdAt: new Date().toISOString(),
          updatedAt: new Date().toISOString(),
        },
      ];
    }
    return draftsList;
  };

  const [clientSessions, setClientSessions] = createSignal<SessionsResponse>(
    props.initialSessions || {
      items: [],
      pagination: { page: 1, perPage: 50, total: 0, totalPages: 0 },
    },
  );

  const sessionsData = () => {
    const data = clientSessions();
    return {
      ...data,
      items: [...data.items].sort(
        (a, b) => new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime(),
      ),
    };
  };

  const filteredSessions = createMemo<Session[]>(() => {
    const filter = statusFilter();
    const data = sessionsData();
    const sessions = data?.items ?? [];
    if (!filter) {
      return sessions;
    }
    return sessions.filter(session => session.status === filter);
  });

  const refetch = async () => {
    if (typeof window === 'undefined') return;

    try {
      const params = { perPage: 50 };
      const data = await apiClient.listSessions(params);
      setClientSessions(data);
    } catch (error) {
      console.error('Failed to refresh sessions:', error);
      addToast('error', 'Failed to refresh session list. Please try again.');
    }
  };

  createEffect(() => {
    refreshTrigger();
    if (typeof window !== 'undefined') {
      refetch();
    }
  });

  onMount(() => {
    const interval = setInterval(() => {
      setRefreshTrigger(prev => prev + 1);
    }, 30000);

    if (typeof window !== 'undefined') {
      const handleDraftCreated = () => {
        if (!import.meta.env.PROD) {
          console.log('[TaskFeed] Draft created, refetching...');
        }
        setDraftsRefreshTrigger(prev => prev + 1);
      };
      window.addEventListener('draft-created', handleDraftCreated);

      return () => {
        clearInterval(interval);
        window.removeEventListener('draft-created', handleDraftCreated);
      };
    } else {
      return () => clearInterval(interval);
    }
  });

  const handleStopSession = async (sessionId: string) => {
    try {
      await apiClient.stopSession(sessionId);
      setRefreshTrigger(prev => prev + 1);
    } catch (error) {
      console.error('Failed to stop session:', error);
      addToast('error', 'Failed to stop session. Please try again.');
    }
  };

  const handleCancelSession = async (sessionId: string) => {
    addToastWithActions('warning', 'Cancel session?', [
      {
        label: 'Cancel Session',
        onClick: async () => {
          try {
            await apiClient.cancelSession(sessionId);
            setRefreshTrigger(prev => prev + 1);
            addToast('success', 'Session cancelled successfully');
          } catch (error) {
            console.error('Failed to cancel session:', error);
            addToast('error', 'Failed to cancel session. Please try again.');
          }
        },
        variant: 'danger',
      },
    ]);
  };

  const handleDraftUpdate = async (draft: DraftTask, updates: Partial<DraftTask>) => {
    const success = await draftOps.updateDraft(draft.id, updates);
    if (success) {
      setClientDrafts(prev =>
        prev
          .map(d =>
            d.id === draft.id
              ? ({
                  ...d,
                  ...updates,
                  repo: updates.repo
                    ? {
                        ...d.repo,
                        ...updates.repo,
                        mode: (updates.repo.mode || d.repo.mode) as DraftTask['repo']['mode'],
                      }
                    : d.repo,
                } as DraftTask)
              : d,
          )
          .map(d => d as DraftTask),
      );
    }
  };

  const handleDraftRemove = async (draft: DraftTask) => {
    const success = await draftOps.removeDraft(draft.id);
    if (success) {
      setClientDrafts(prev => prev.filter(d => d.id !== draft.id));
    }
  };

  const handleDraftTaskCreated = async (draft: DraftTask, taskId: string) => {
    void refetch();
    const success = await draftOps.removeDraft(draft.id);
    if (success) {
      setClientDrafts(prev => prev.filter(d => d.id !== draft.id));
    }
    props.onDraftTaskCreated?.(taskId);
  };

  const handleDraftFocus = () => {
    setSelectedSessionId('');
    clearFocus();
  };

  const handleSessionFocus = (sessionId: string) => {
    setSelectedSessionId(sessionId);
    setSessionFocus(sessionId);
  };

  const handleSessionSelect = (sessionId: string, globalIndex: number) => {
    setSelectedSessionId(sessionId);
    setKeyboardSelectedIndex(globalIndex);
    setSessionFocus(sessionId);
  };

  const handleNavigateToSession = (sessionId: string) => {
    navigate(`/tasks/${sessionId}`);
  };

  const getActiveDescendantId = () => {
    const currentIndex = keyboardSelectedIndex();
    if (currentIndex < 0) return undefined;

    const draftList = drafts();
    if (currentIndex < draftList.length) {
      const draft = draftList[currentIndex];
      return draft ? `draft-task-${draft.id}` : undefined;
    }

    const sessionList = filteredSessions();
    const sessionIndex = currentIndex - draftList.length;
    const session = sessionList[sessionIndex];
    return session ? `task-${session.id}` : undefined;
  };

  const scrollCardIntoView = (index: number) => {
    if (typeof window === 'undefined') return;

    const draftList = drafts();
    let cardElement: Element | null = null;

    if (index < draftList.length) {
      const draftCards = document.querySelectorAll('[data-testid="draft-task-card"]');
      cardElement = draftCards[index] || null;
    } else {
      const sessionIndex = index - draftList.length;
      const sessionCards = document.querySelectorAll('[data-testid="task-card"]');
      cardElement = sessionCards[sessionIndex] || null;
    }

    cardElement?.scrollIntoView({ behavior: 'smooth', block: 'nearest', inline: 'nearest' });
  };

  const handleKeyDown = (event: KeyboardEvent) => {
    const sessions = filteredSessions();
    const draftList = drafts();
    const totalItems = draftList.length + sessions.length;
    if (totalItems === 0) return;

    const currentIndex = keyboardSelectedIndex();

    switch (event.key) {
      case 'ArrowDown': {
        event.preventDefault();
        const nextIndex = currentIndex < totalItems - 1 ? currentIndex + 1 : 0;
        setKeyboardSelectedIndex(nextIndex);

        const keyboardNavEl = document.querySelector('[aria-label="Task list navigation"]');
        keyboardNavEl?.setAttribute('data-keyboard-index', nextIndex.toString());

        if (nextIndex < draftList.length) {
          handleDraftFocus();
        } else {
          const sessionIndex = nextIndex - draftList.length;
          const session = sessions[sessionIndex];
          if (session) {
            handleSessionFocus(session.id);
          }
        }

        scrollCardIntoView(nextIndex);
        break;
      }

      case 'ArrowUp': {
        event.preventDefault();
        const prevIndex = currentIndex > 0 ? currentIndex - 1 : totalItems - 1;
        setKeyboardSelectedIndex(prevIndex);

        const keyboardNavEl = document.querySelector('[aria-label="Task list navigation"]');
        keyboardNavEl?.setAttribute('data-keyboard-index', prevIndex.toString());

        if (prevIndex < draftList.length) {
          handleDraftFocus();
        } else {
          const sessionIndex = prevIndex - draftList.length;
          const session = sessions[sessionIndex];
          if (session) {
            handleSessionFocus(session.id);
          }
        }

        scrollCardIntoView(prevIndex);
        break;
      }

      case 'Enter':
        event.preventDefault();
        if (currentIndex >= draftList.length && currentIndex < totalItems) {
          const sessionIndex = currentIndex - draftList.length;
          const session = sessions[sessionIndex];
          if (session) {
            handleNavigateToSession(session.id);
          }
        }
        break;
    }
  };

  return (
    <section
      data-testid="task-feed"
      class="flex h-full flex-col"
      role="region"
      aria-label="Task feed"
    >
      <TaskFeedHeader
        statusFilter={statusFilter()}
        statusOptions={statusOptions}
        onStatusChange={value => {
          setStatusFilter(value);
        }}
      />

      <section
        class="flex-1 overflow-y-auto"
        role="region"
        tabindex="0"
        aria-activedescendant={getActiveDescendantId()}
        aria-label="Task list navigation"
        onKeyDown={handleKeyDown}
      >
        <div class="p-4">
          <TaskFeedDraftsSection
            drafts={drafts}
            keyboardSelectedIndex={keyboardSelectedIndex}
            onUpdateDraft={handleDraftUpdate}
            onRemoveDraft={handleDraftRemove}
            onTaskCreated={handleDraftTaskCreated}
          />

          <TaskFeedSessionsSection
            drafts={drafts}
            sessions={filteredSessions}
            keyboardSelectedIndex={keyboardSelectedIndex}
            onSelectSession={handleSessionSelect}
            onStopSession={handleStopSession}
            onCancelSession={handleCancelSession}
            sessionsData={sessionsData}
          />
        </div>
      </section>
    </section>
  );
};
