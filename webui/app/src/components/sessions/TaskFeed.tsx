import { Component, createSignal, createEffect, createMemo, For, Show, onMount } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import { apiClient, type Session, type DraftTask } from '../../lib/api.js';
import { type SessionsResponse } from '../../lib/server-data.js';
import { SessionCard } from './SessionCard.js';
import { DraftTaskCard } from '../tasks/DraftTaskCard.js';
import { useSession } from '../../contexts/SessionContext.js';
import { useDrafts } from '../../contexts/DraftContext.js';
import { useFocus } from '../../contexts/FocusContext.js';
import { useToast } from '../../contexts/ToastContext.js';

interface TaskFeedProps {
  draftTasks?: DraftTask[]; // Will be defined later
  onDraftTaskCreated?: (taskId: string) => void;
  onDraftTaskRemoved?: (draftId: string) => void;
  initialSessions?: SessionsResponse;
  initialDrafts?: DraftTask[]; // Drafts fetched during SSR
}

type StatusFilter = Session['status'] | '';

export const TaskFeed: Component<TaskFeedProps> = props => {
  const navigate = useNavigate();
  const { selectedSessionId, setSelectedSessionId } = useSession();
  const draftOps = useDrafts(); // Get CRUD operations from context
  const { setSessionFocus, clearFocus } = useFocus();
  const { addToast, addToastWithActions } = useToast();
  const [keyboardSelectedIndex, setKeyboardSelectedIndex] = createSignal<number>(-1);

  // Debug keyboard selection changes
  createEffect(() => {
    const index = keyboardSelectedIndex();
    if (!import.meta.env.PROD) {
      console.debug('[TaskFeed] keyboardSelectedIndex changed to:', index);
    }
  });
  const [refreshTrigger, setRefreshTrigger] = createSignal(0); // For auto-refresh every 30s
  const [statusFilter, setStatusFilter] = createSignal<StatusFilter>('');
  const statusOptions: Array<{ value: StatusFilter; label: string }> = [
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

  // Progressive enhancement: Drafts rendered from props during SSR
  // Context provides CRUD operations, not the list itself
  // PRD REQUIREMENT: "An empty task card is always visible" - ensure at least one draft
  const [clientDrafts, setClientDrafts] = createSignal<DraftTask[]>(props.initialDrafts || []);
  const [draftsRefreshTrigger, setDraftsRefreshTrigger] = createSignal(0);

  // Live region for announcing dynamic updates to screen readers
  const [liveAnnouncements] = createSignal<string[]>([]);

  // Refetch drafts from API (client-side only)
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

  // Watch for draft changes and refetch
  createEffect(() => {
    draftsRefreshTrigger(); // Track changes
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

  // Ensure there's always at least one draft (PRD requirement)
  const drafts = (): DraftTask[] => {
    const draftsList = clientDrafts();
    if (draftsList.length === 0) {
      // Return a default empty draft if none exist
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

  // Progressive enhancement: Render directly from props during SSR
  // This ensures the page works without JavaScript - all content visible in initial HTML
  // createResource is used ONLY for client-side filtering/refreshing after hydration
  const [clientSessions, setClientSessions] = createSignal<SessionsResponse>(
    props.initialSessions || {
      items: [],
      pagination: { page: 1, perPage: 50, total: 0, totalPages: 0 },
    },
  );

  // Simple accessor that works synchronously during SSR
  // Sort sessions by newest first
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

  // Refetch function for client-side updates (refreshing)
  const refetch = async () => {
    if (typeof window === 'undefined') return; // SSR guard

    try {
      const params = { perPage: 50 };
      const data = await apiClient.listSessions(params);
      setClientSessions(data);
    } catch (error) {
      console.error('Failed to refresh sessions:', error);
      addToast('error', 'Failed to refresh session list. Please try again.');
    }
  };

  // Auto-refetch when refresh trigger fires (client-side only)
  createEffect(() => {
    // Watch refreshTrigger to trigger refetch
    refreshTrigger(); // Track refreshTrigger changes
    if (typeof window !== 'undefined') {
      refetch();
    }
  });

  // Handle session selection
  const handleSessionSelect = (sessionId: string, index?: number) => {
    setSelectedSessionId(sessionId);
    if (index !== undefined) {
      setKeyboardSelectedIndex(index);
    }
  };

  // Handle keyboard navigation (drafts first, then sessions)
  const handleKeyDown = (e: KeyboardEvent) => {
    if (!import.meta.env.PROD) {
      console.log('[TaskFeed] Keyboard event:', e.key);
    }
    const sessions = filteredSessions();
    const draftsList = drafts();
    const totalItems = draftsList.length + sessions.length;

    if (!import.meta.env.PROD) {
      console.log(
        '[TaskFeed] Total items:',
        totalItems,
        'drafts:',
        draftsList.length,
        'sessions:',
        sessions.length,
      );
    }

    if (totalItems === 0) return;

    const currentIndex = keyboardSelectedIndex();
    if (!import.meta.env.PROD) {
      console.log('[TaskFeed] Current index:', currentIndex);
    }

    switch (e.key) {
      case 'ArrowDown': {
        e.preventDefault();
        const nextIndex = currentIndex < totalItems - 1 ? currentIndex + 1 : 0;
        if (!import.meta.env.PROD) {
          console.log('[TaskFeed] Setting keyboardSelectedIndex to:', nextIndex);
        }
        setKeyboardSelectedIndex(nextIndex);

        // Add a debug attribute to the keyboard navigation element for testing
        const keyboardNavEl = document.querySelector('[aria-label="Task list navigation"]');
        if (keyboardNavEl) {
          keyboardNavEl.setAttribute('data-keyboard-index', nextIndex.toString());
        }

        // Update selected session ID and focus state
        // Drafts are first (0 to draftsList.length - 1), sessions follow
        if (nextIndex < draftsList.length) {
          setSelectedSessionId(''); // Clear selection when on draft
          clearFocus(); // Clear focus when moving to draft (draft will set its own focus)
        } else {
          const sessionIndex = nextIndex - draftsList.length;
          const session = sessions[sessionIndex];
          if (session) {
            setSelectedSessionId(session.id);
            setSessionFocus(session.id);
          }
        }

        // Scroll selected card into view
        scrollCardIntoView(nextIndex);
        break;
      }

      case 'ArrowUp': {
        e.preventDefault();
        const prevIndex = currentIndex > 0 ? currentIndex - 1 : totalItems - 1;
        if (!import.meta.env.PROD) {
          console.log('[TaskFeed] Setting keyboardSelectedIndex to:', prevIndex);
        }
        setKeyboardSelectedIndex(prevIndex);

        // Add a debug attribute to the keyboard navigation element for testing
        const keyboardNavEl = document.querySelector('[aria-label="Task list navigation"]');
        if (keyboardNavEl) {
          keyboardNavEl.setAttribute('data-keyboard-index', prevIndex.toString());
        }

        // Update selected session ID and focus state
        // Drafts are first (0 to draftsList.length - 1), sessions follow
        if (prevIndex < draftsList.length) {
          setSelectedSessionId(''); // Clear selection when on draft
          clearFocus(); // Clear focus when moving to draft (draft will set its own focus)
        } else {
          const sessionIndex = prevIndex - draftsList.length;
          const session = sessions[sessionIndex];
          if (session) {
            setSelectedSessionId(session.id);
            setSessionFocus(session.id);
          }
        }

        // Scroll selected card into view
        scrollCardIntoView(prevIndex);
        break;
      }

      case 'Enter':
        e.preventDefault();
        // Drafts are first, sessions follow
        if (currentIndex >= draftsList.length && currentIndex < totalItems) {
          const sessionIndex = currentIndex - draftsList.length;
          if (sessions[sessionIndex]) {
            // Navigate to task details page for sessions
            navigate(`/tasks/${sessions[sessionIndex].id}`);
          }
        }
        // For drafts, Enter is handled by the DraftTaskCard component
        break;
    }
  };

  // Scroll the selected card into view
  const scrollCardIntoView = (index: number) => {
    if (typeof window === 'undefined') return;

    const draftsList = drafts();
    let cardElement: Element | null = null;

    if (index < draftsList.length) {
      // Draft card (first in list)
      const draftCards = document.querySelectorAll('[data-testid="draft-task-card"]');
      cardElement = draftCards[index] || null;
    } else {
      // Session card (after drafts)
      const sessionIndex = index - draftsList.length;
      const sessionCards = document.querySelectorAll('[data-testid="task-card"]');
      cardElement = sessionCards[sessionIndex] || null;
    }

    if (cardElement) {
      cardElement.scrollIntoView({
        behavior: 'smooth',
        block: 'nearest',
        inline: 'nearest',
      });
    }
  };

  // Auto-refresh sessions every 30 seconds and setup draft creation listeners
  onMount(() => {
    const interval = setInterval(() => {
      setRefreshTrigger(prev => prev + 1);
    }, 30000);

    // Listen for draft creation events (client-side only)
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
      setRefreshTrigger(prev => prev + 1); // Refresh the list
    } catch (error) {
      console.error('Failed to stop session:', error);
      addToast('error', 'Failed to stop session. Please try again.');
    }
  };

  const handleCancelSession = async (sessionId: string) => {
    // Show confirmation toast instead of blocking confirm dialog
    addToastWithActions('warning', 'Cancel session?', [
      {
        label: 'Cancel Session',
        onClick: async () => {
          try {
            await apiClient.cancelSession(sessionId);
            setRefreshTrigger(prev => prev + 1); // Refresh the list
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

  // Generate unique ID for the currently selected item for aria-activedescendant
  const getActiveDescendantId = () => {
    const currentIndex = keyboardSelectedIndex();
    if (currentIndex < 0) return undefined;

    const draftsList = drafts();
    if (currentIndex < draftsList.length) {
      // Draft card
      const draft = draftsList[currentIndex];
      return draft ? `draft-task-${draft.id}` : undefined;
    } else {
      // Session card
      const sessions = filteredSessions();
      const sessionIndex = currentIndex - draftsList.length;
      const session = sessions[sessionIndex];
      return session ? `task-${session.id}` : undefined;
    }
  };

  return (
    <section
      data-testid="task-feed"
      class="flex h-full flex-col"
      role="region"
      aria-label="Task feed"
    >
      {/* Task Feed Heading */}
      <div class="border-b border-gray-200 bg-white px-6 py-4">
        <div
          class={`
            flex flex-col gap-3
            md:flex-row md:items-center md:justify-between
          `}
        >
          <h2 class="text-lg font-semibold text-gray-900">Task Feed</h2>

          <div class="flex items-center gap-3" role="group" aria-label="Task filters">
            <label for="status-filter" class="text-sm font-medium text-gray-700">
              Status
            </label>
            <select
              id="status-filter"
              data-testid="status-filter"
              class={`
                rounded-md border border-gray-300 bg-white px-3 py-1 text-sm
                text-gray-700 shadow-sm transition
                focus:border-blue-500 focus:ring-2 focus:ring-blue-500
                focus:outline-none
              `}
              aria-label="Filter sessions by status"
              value={statusFilter()}
              onInput={event => {
                setStatusFilter(event.currentTarget.value as StatusFilter);
              }}
            >
              <For each={statusOptions}>
                {option => <option value={option.value}>{option.label}</option>}
              </For>
            </select>
          </div>
        </div>
      </div>

      <section
        class="flex-1 overflow-y-auto"
        role="region"
        tabindex="0"
        aria-activedescendant={getActiveDescendantId()}
        aria-label="Task list navigation"
        onKeyDown={handleKeyDown}
      >
        <div class="p-4">
          {/* Draft tasks - render first at the top */}
          <Show when={drafts().length > 0}>
            <div class="space-y-3">
              <For each={drafts()}>
                {(draft, draftIndex) => {
                  const globalIndex = draftIndex();
                  return (
                    <div id={`draft-task-${draft.id}`}>
                      <DraftTaskCard
                        draft={draft}
                        isSelected={keyboardSelectedIndex() === globalIndex}
                        onDebug={() => {
                          if (!import.meta.env.PROD) {
                            console.log(
                              `[TaskFeed] Draft ${draft.id} isSelected:`,
                              keyboardSelectedIndex() === globalIndex,
                              'keyboardIndex:',
                              keyboardSelectedIndex(),
                              'globalIndex:',
                              globalIndex,
                            );
                          }
                        }}
                        onUpdate={async updates => {
                          const success = await draftOps.updateDraft(draft.id, updates);
                          if (success) {
                            // Update local draft list optimistically
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
                                              mode: (updates.repo.mode ||
                                                d.repo.mode) as DraftTask['repo']['mode'],
                                            }
                                          : d.repo,
                                      } as DraftTask)
                                    : d,
                                )
                                .map(d => d as DraftTask),
                            );
                          }
                        }}
                        onRemove={async () => {
                          const success = await draftOps.removeDraft(draft.id);
                          if (success) {
                            // Update local draft list
                            setClientDrafts(prev => prev.filter(d => d.id !== draft.id));
                          }
                        }}
                        onTaskCreated={async taskId => {
                          // Refresh the session list after creating a task
                          refetch();
                          // Remove the draft after creating the task
                          const success = await draftOps.removeDraft(draft.id);
                          if (success) {
                            setClientDrafts(prev => prev.filter(d => d.id !== draft.id));
                          }
                          props.onDraftTaskCreated?.(taskId);
                        }}
                      />
                    </div>
                  );
                }}
              </For>
            </div>
          </Show>

          {/* Session tasks - render below drafts, sorted newest first */}
          <Show when={filteredSessions().length > 0}>
            <div class={drafts().length > 0 ? 'mt-6' : ''}>
              <ul role="listbox" class="space-y-3">
                <For each={filteredSessions()}>
                  {(session, index) => {
                    const globalIndex = drafts().length + index();
                    return (
                      <li
                        id={`task-${session.id}`}
                        role="option"
                        aria-selected={
                          selectedSessionId() === session.id ||
                          keyboardSelectedIndex() === globalIndex
                        }
                      >
                        <SessionCard
                          session={session as Session}
                          isSelected={
                            selectedSessionId() === session.id ||
                            keyboardSelectedIndex() === globalIndex
                          }
                          onClick={() => handleSessionSelect(session.id, globalIndex)}
                          onStop={() => handleStopSession(session.id)}
                          onCancel={() => handleCancelSession(session.id)}
                        />
                      </li>
                    );
                  }}
                </For>
              </ul>

              {/* ARIA live region for keyboard navigation announcements */}
              <div
                role="status"
                aria-live="polite"
                aria-atomic="true"
                class={`
                sr-only
              `}
              >
                {(() => {
                  const idx = keyboardSelectedIndex();
                  const draftCount = drafts().length;
                  if (idx >= 0 && idx < draftCount) {
                    return `Selected draft: ${drafts()[idx]?.prompt || 'New task'}`;
                  } else if (idx >= draftCount) {
                    const session = filteredSessions()[idx - draftCount];
                    if (session) {
                      return `Selected task: ${session.prompt}`;
                    }
                  }
                  return '';
                })()}
              </div>

              {/* ARIA live region for dynamic content updates */}
              <div
                role="status"
                aria-live="polite"
                aria-atomic="false"
                class={`
                sr-only
              `}
              >
                {liveAnnouncements().join('. ')}
              </div>

              <Show when={sessionsData()?.pagination.totalPages > 1}>
                <div class="mt-4 text-center text-sm text-gray-500" role="status">
                  Showing {filteredSessions().length} of {sessionsData()?.pagination.total} sessions
                </div>
              </Show>
            </div>
          </Show>

          {/* Filtered empty state */}
          <Show when={sessionsData()?.items.length > 0 && filteredSessions().length === 0}>
            <div class="py-8 text-center text-sm text-gray-500" role="status" aria-live="polite">
              No sessions match the selected filter.
            </div>
          </Show>

          {/* Empty state when no sessions and no drafts */}
          <Show when={sessionsData()?.items.length === 0 && drafts().length === 0}>
            <div class="py-8 text-center" role="status" aria-live="polite">
              <svg
                class="mx-auto h-12 w-12 text-gray-400"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                aria-hidden="true"
              >
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  stroke-width="2"
                  d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
                />
              </svg>
              <h3 class="mt-2 text-sm font-medium text-gray-900">No tasks</h3>
              <p class="mt-1 text-sm text-gray-500">Get started by creating a new task.</p>
            </div>
          </Show>
        </div>
      </section>
    </section>
  );
};
