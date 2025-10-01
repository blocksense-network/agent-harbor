import {
  Component,
  createResource,
  createSignal,
  createEffect,
  For,
  Show,
  onMount,
} from "solid-js";
import { useNavigate } from "@solidjs/router";
import { apiClient, type Session } from "../../lib/api.js";
import { SessionCard } from "./SessionCard.js";
import { DraftTaskCard } from "../tasks/DraftTaskCard.js";
import { useSession } from "../../contexts/SessionContext.js";
import { useDrafts } from "../../contexts/DraftContext.js";
import { useFocus } from "../../contexts/FocusContext.js";
import { useToast } from "../../contexts/ToastContext.js";

interface TaskFeedProps {
  draftTasks?: any[]; // Will be defined later
  onDraftTaskCreated?: (taskId: string) => void;
  onDraftTaskRemoved?: (draftId: string) => void;
  initialSessions?: {
    items: Session[];
    pagination: {
      page: number;
      perPage: number;
      total: number;
      totalPages: number;
    };
  };
  initialDrafts?: any[]; // Drafts fetched during SSR
}

export const TaskFeed: Component<TaskFeedProps> = (props) => {
  const navigate = useNavigate();
  const { selectedSessionId, setSelectedSessionId } = useSession();
  const draftOps = useDrafts(); // Get CRUD operations from context
  const { setSessionFocus, clearFocus } = useFocus();
  const { addToast, addToastWithActions } = useToast();
  const [keyboardSelectedIndex, setKeyboardSelectedIndex] =
    createSignal<number>(-1);
  const [refreshTrigger, setRefreshTrigger] = createSignal(0); // For auto-refresh every 30s
  const [cancelConfirmSessionId, setCancelConfirmSessionId] =
    createSignal<string | null>(null);

  // Progressive enhancement: Drafts rendered from props during SSR
  // Context provides CRUD operations, not the list itself
  // PRD REQUIREMENT: "An empty task card is always visible" - ensure at least one draft
  const [clientDrafts, setClientDrafts] = createSignal(
    props.initialDrafts || [],
  );
  const [draftsRefreshTrigger, setDraftsRefreshTrigger] = createSignal(0);

  // Live region for announcing dynamic updates to screen readers
  const [liveAnnouncements, setLiveAnnouncements] = createSignal<string[]>([]);

  // Add announcement to live region
  const announce = (message: string) => {
    setLiveAnnouncements((prev) => [...prev, message]);
    // Clear announcements after 5 seconds
    setTimeout(() => {
      setLiveAnnouncements((prev) => prev.filter((msg) => msg !== message));
    }, 5000);
  };

  // Refetch drafts from API (client-side only)
  const refetchDrafts = async () => {
    if (typeof window === "undefined") return;

    try {
      const data = await apiClient.listDrafts();
      setClientDrafts(data.items || []);
    } catch (error) {
      console.error("Failed to fetch drafts:", error);
      addToast("error", "Failed to load draft tasks. Please refresh the page.");
    }
  };

  // Watch for draft changes and refetch
  createEffect(() => {
    const _ = draftsRefreshTrigger(); // Track changes
    if (typeof window !== "undefined") {
      refetchDrafts();
    }
  });

  // Ensure there's always at least one draft (PRD requirement)
  const drafts = () => {
    const draftsList = clientDrafts();
    if (draftsList.length === 0) {
      // Return a default empty draft if none exist
      return [
        {
          id: "local-draft-new",
          prompt: "",
          repo: { mode: "git", url: "", branch: "main" },
          agents: [],
          runtime: { type: "devcontainer" },
          delivery: { mode: "pr" },
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
  const [clientSessions, setClientSessions] = createSignal(
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
        (a, b) =>
          new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime(),
      ),
    };
  };

  // Refetch function for client-side updates (refreshing)
  const refetch = async () => {
    if (typeof window === "undefined") return; // SSR guard

    try {
      const params: any = { perPage: 50 };
      const data = await apiClient.listSessions(params);
      setClientSessions(data);
    } catch (error) {
      console.error("Failed to refresh sessions:", error);
      addToast("error", "Failed to refresh session list. Please try again.");
    }
  };

  // Auto-refetch when refresh trigger fires (client-side only)
  createEffect(() => {
    // Watch refreshTrigger to trigger refetch
    const _ = refreshTrigger(); // Track refreshTrigger changes
    if (typeof window !== "undefined") {
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
    const sessions = sessionsData()?.items || [];
    const draftsList = drafts();
    const totalItems = draftsList.length + sessions.length;

    if (totalItems === 0) return;

    const currentIndex = keyboardSelectedIndex();

    switch (e.key) {
      case "ArrowDown": {
        e.preventDefault();
        const nextIndex = currentIndex < totalItems - 1 ? currentIndex + 1 : 0;
        setKeyboardSelectedIndex(nextIndex);

        // Update selected session ID and focus state
        // Drafts are first (0 to draftsList.length - 1), sessions follow
        if (nextIndex < draftsList.length) {
          setSelectedSessionId(""); // Clear selection when on draft
          clearFocus(); // Clear focus when moving to draft (draft will set its own focus)
        } else {
          const sessionIndex = nextIndex - draftsList.length;
          setSelectedSessionId(sessions[sessionIndex].id);
          setSessionFocus(sessions[sessionIndex].id);
        }

        // Scroll selected card into view
        scrollCardIntoView(nextIndex);
        break;
      }

      case "ArrowUp": {
        e.preventDefault();
        const prevIndex = currentIndex > 0 ? currentIndex - 1 : totalItems - 1;
        setKeyboardSelectedIndex(prevIndex);

        // Update selected session ID and focus state
        // Drafts are first (0 to draftsList.length - 1), sessions follow
        if (prevIndex < draftsList.length) {
          setSelectedSessionId(""); // Clear selection when on draft
          clearFocus(); // Clear focus when moving to draft (draft will set its own focus)
        } else {
          const sessionIndex = prevIndex - draftsList.length;
          setSelectedSessionId(sessions[sessionIndex].id);
          setSessionFocus(sessions[sessionIndex].id);
        }

        // Scroll selected card into view
        scrollCardIntoView(prevIndex);
        break;
      }

      case "Enter":
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
    if (typeof window === "undefined") return;

    const draftsList = drafts();
    let cardElement: Element | null = null;

    if (index < draftsList.length) {
      // Draft card (first in list)
      const draftCards = document.querySelectorAll(
        '[data-testid="draft-task-card"]',
      );
      cardElement = draftCards[index] || null;
    } else {
      // Session card (after drafts)
      const sessionIndex = index - draftsList.length;
      const sessionCards = document.querySelectorAll(
        '[data-testid="task-card"]',
      );
      cardElement = sessionCards[sessionIndex] || null;
    }

    if (cardElement) {
      cardElement.scrollIntoView({
        behavior: "smooth",
        block: "nearest",
        inline: "nearest",
      });
    }
  };

  // Auto-refresh sessions every 30 seconds and setup draft creation listeners
  onMount(() => {
    const interval = setInterval(() => {
      setRefreshTrigger((prev) => prev + 1);
    }, 30000);

    // Listen for draft creation events (client-side only)
    if (typeof window !== "undefined") {
      const handleDraftCreated = () => {
        console.log("[TaskFeed] Draft created, refetching...");
        setDraftsRefreshTrigger((prev) => prev + 1);
      };
      window.addEventListener("draft-created", handleDraftCreated);

      return () => {
        clearInterval(interval);
        window.removeEventListener("draft-created", handleDraftCreated);
      };
    } else {
      return () => clearInterval(interval);
    }
  });

  const handleStopSession = async (sessionId: string) => {
    try {
      await apiClient.stopSession(sessionId);
      setRefreshTrigger((prev) => prev + 1); // Refresh the list
    } catch (error) {
      console.error("Failed to stop session:", error);
      addToast("error", "Failed to stop session. Please try again.");
    }
  };

  const handleCancelSession = async (sessionId: string) => {
    // Show confirmation toast instead of blocking confirm dialog
    addToastWithActions("warning", "Cancel session?", [
      {
        label: "Cancel Session",
        onClick: async () => {
          try {
            await apiClient.cancelSession(sessionId);
            setRefreshTrigger((prev) => prev + 1); // Refresh the list
            addToast("success", "Session cancelled successfully");
          } catch (error) {
            console.error("Failed to cancel session:", error);
            addToast("error", "Failed to cancel session. Please try again.");
          }
        },
        variant: "danger",
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
      return `draft-task-${draft.id}`;
    } else {
      // Session card
      const sessions = sessionsData()?.items || [];
      const sessionIndex = currentIndex - draftsList.length;
      const session = sessions[sessionIndex];
      return session ? `task-${session.id}` : undefined;
    }
  };

  return (
    <section
      data-testid="task-feed"
      class="flex flex-col h-full"
      role="region"
      aria-label="Task feed"
    >
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
                        onUpdate={async (updates) => {
                          const success = await draftOps.updateDraft(
                            draft.id,
                            updates,
                          );
                          if (success) {
                            // Update local draft list optimistically
                            setClientDrafts((prev) =>
                              prev.map((d) =>
                                d.id === draft.id ? { ...d, ...updates } : d,
                              ),
                            );
                          }
                        }}
                        onRemove={async () => {
                          const success = await draftOps.removeDraft(draft.id);
                          if (success) {
                            // Update local draft list
                            setClientDrafts((prev) =>
                              prev.filter((d) => d.id !== draft.id),
                            );
                          }
                        }}
                        onTaskCreated={async (taskId) => {
                          // Refresh the session list after creating a task
                          refetch();
                          // Remove the draft after creating the task
                          const success = await draftOps.removeDraft(draft.id);
                          if (success) {
                            setClientDrafts((prev) =>
                              prev.filter((d) => d.id !== draft.id),
                            );
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
          <Show when={sessionsData()?.items.length > 0}>
            <div class={drafts().length > 0 ? "mt-6" : ""}>
              <ul role="listbox" class="space-y-3">
                <For each={sessionsData()?.items}>
                  {(session, index) => {
                    const sessions = sessionsData()?.items || [];
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
                          session={session}
                          isSelected={
                            selectedSessionId() === session.id ||
                            keyboardSelectedIndex() === globalIndex
                          }
                          onClick={() =>
                            handleSessionSelect(session.id, globalIndex)
                          }
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
                class="sr-only"
              >
                {(() => {
                  const idx = keyboardSelectedIndex();
                  const draftCount = drafts().length;
                  if (idx >= 0 && idx < draftCount) {
                    return `Selected draft: ${drafts()[idx]?.prompt || "New task"}`;
                  } else if (
                    idx >= draftCount &&
                    sessionsData()?.items[idx - draftCount]
                  ) {
                    return `Selected task: ${sessionsData()!.items[idx - draftCount].prompt}`;
                  }
                  return "";
                })()}
              </div>

              {/* ARIA live region for dynamic content updates */}
              <div
                role="status"
                aria-live="polite"
                aria-atomic="false"
                class="sr-only"
              >
                {liveAnnouncements().join(". ")}
              </div>

              <Show when={sessionsData()?.pagination.totalPages > 1}>
                <div
                  class="mt-4 text-center text-sm text-gray-500"
                  role="status"
                >
                  Showing {sessionsData()?.items.length} of{" "}
                  {sessionsData()?.pagination.total} sessions
                </div>
              </Show>
            </div>
          </Show>

          {/* Empty state when no sessions and no drafts */}
          <Show
            when={sessionsData()?.items.length === 0 && drafts().length === 0}
          >
            <div class="text-center py-8" role="status" aria-live="polite">
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
              <p class="mt-1 text-sm text-gray-500">
                Get started by creating a new task.
              </p>
            </div>
          </Show>
        </div>
      </section>
    </section>
  );
};
