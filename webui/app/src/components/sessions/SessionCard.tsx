import { Component, Show, For, createSignal, createMemo, onMount, onCleanup } from 'solid-js';
import { A } from '@solidjs/router';
import { Session, SessionEvent } from '../../lib/api.js';
import { subscribeToSession } from '../../lib/sse-manager.js';

interface SessionCardProps {
  session: Session;
  isSelected?: boolean;
  onClick?: () => void;
  onStop?: () => void;
  onCancel?: () => void;
}

// Activity row types for the 3-row live activity display
type ActivityRow =
  | { type: 'thinking'; text: string }
  | {
      type: 'tool';
      name: string;
      lastLine?: string;
      output?: string;
      status?: string;
    }
  | { type: 'file'; path: string; linesAdded: number; linesRemoved: number };

interface LiveActivityState {
  rows: ActivityRow[]; // Maximum 3 rows, newest at end
  currentTool: string | null; // Track tool in progress for last_line updates
}

const getStatusIcon = (status: string) => {
  switch (status) {
    case 'running':
      return { icon: '●', color: 'text-green-600', bg: 'bg-green-50' };
    case 'queued':
      return { icon: '●', color: 'text-yellow-600', bg: 'bg-yellow-50' };
    case 'provisioning':
      return { icon: '●', color: 'text-blue-600', bg: 'bg-blue-50' };
    case 'pausing':
    case 'paused':
      return { icon: '⏸', color: 'text-orange-600', bg: 'bg-orange-50' };
    case 'resuming':
      return { icon: '●', color: 'text-blue-600', bg: 'bg-blue-50' };
    case 'stopping':
      return { icon: '⏹', color: 'text-red-600', bg: 'bg-red-50' };
    case 'stopped':
    case 'completed':
      return { icon: '✓', color: 'text-gray-600', bg: 'bg-gray-50' };
    case 'failed':
    case 'cancelled':
      return { icon: '✗', color: 'text-red-600', bg: 'bg-red-50' };
    default:
      return { icon: '?', color: 'text-gray-600', bg: 'bg-gray-50' };
  }
};

const formatDate = (dateString: string) => {
  try {
    // Use stable formatting to avoid SSR/client hydration mismatches
    // toLocaleString() can produce different results on server vs client
    const date = new Date(dateString);

    // Use a fixed format: MM/DD/YYYY HH:MM
    const month = String(date.getMonth() + 1).padStart(2, '0');
    const day = String(date.getDate()).padStart(2, '0');
    const year = date.getFullYear();
    const hours = String(date.getHours()).padStart(2, '0');
    const minutes = String(date.getMinutes()).padStart(2, '0');

    return `${month}/${day}/${year} ${hours}:${minutes}`;
  } catch {
    return dateString;
  }
};

const getRepoName = (url?: string) => {
  if (!url) return 'Unknown';
  try {
    const match = url.match(/\/([^/]+)\.git$/);
    return match ? match[1] : url.split('/').pop() || 'Unknown';
  } catch {
    return 'Unknown';
  }
};

export const SessionCard: Component<SessionCardProps> = (props) => {
  const session = () => props.session;

  // Convert recent_events from session to ActivityRow[] format
  const convertEventToRow = (event: any): ActivityRow | null => {
    // For recent_events that don't have type field, infer from properties
    if (event.thought) {
      return { type: 'thinking', text: event.thought };
    } else if (event.file_path) {
      return {
        type: 'file',
        path: event.file_path,
        linesAdded: event.lines_added || 0,
        linesRemoved: event.lines_removed || 0,
      };
    } else if (event.tool_name) {
      if (event.tool_output) {
        // Tool completed
        return {
          type: 'tool',
          name: event.tool_name,
          output: event.tool_output,
          status: event.tool_status,
        } as ActivityRow;
      } else {
        // Tool started (shouldn't happen in recent_events, but handle it)
        return { type: 'tool', name: event.tool_name };
      }
    }
    return null;
  };

  // Initialize rows from recent_events (SSR pre-population)
  const initialRows: ActivityRow[] = (session().recent_events || [])
    .map(convertEventToRow)
    .filter((row): row is ActivityRow => row !== null);

  // Track live activity from SSE events - maximum 3 rows, pre-populated from SSR
  const [liveActivity, setLiveActivity] = createSignal<LiveActivityState>({
    rows: initialRows,
    currentTool: null,
  });
  const [sessionStatus, setSessionStatus] = createSignal(session().status);

  const statusInfo = () => getStatusIcon(sessionStatus());

  const canStop = () => ['running', 'queued', 'provisioning', 'paused'].includes(sessionStatus());
  const canCancel = () => ['queued', 'provisioning', 'running', 'paused'].includes(sessionStatus());

  // Subscribe to SSE events for active sessions
  onMount(() => {
    // Only subscribe to SSE for active sessions (client-side only)
    if (typeof window === 'undefined') return;

    const isActive = [
      'running',
      'queued',
      'provisioning',
      'paused',
      'resuming',
      'stopping',
    ].includes(session().status);
    if (!isActive) return;

    if (!import.meta.env.PROD) {
      console.log(`[SessionCard] Subscribing to SSE for session ${session().id}`);
    }

    const unsubscribe = subscribeToSession(session().id, (event: SessionEvent) => {
      if (!import.meta.env.PROD) {
        console.log(`[SessionCard ${session().id}] SSE event received:`, event);
      }

      // Update status
      if (event.type === 'status') {
        if (!import.meta.env.PROD) {
          console.log(`[SessionCard ${session().id}] Updating status to:`, event.status);
        }
        setSessionStatus(event.status as Session['status']);
      }

      // Update live activity based on event type
      const current = liveActivity();

      // Thinking event - adds new row, scrolls up
      if (event.type === 'thinking') {
        const newRow: ActivityRow = { type: 'thinking', text: event.thought };
        const newRows = [...current.rows, newRow].slice(-3); // Keep last 3
        setLiveActivity({ ...current, rows: newRows });
        if (!import.meta.env.PROD) {
          console.log(
            `[SessionCard ${session().id}] Added thinking row, total rows: ${newRows.length}`
          );
        }
      }

      // Tool start - adds new row, scrolls up, tracks tool name
      else if (event.type === 'tool_execution' && !event.tool_output && !event.last_line) {
        const newRow: ActivityRow = { type: 'tool', name: event.tool_name };
        const newRows = [...current.rows, newRow].slice(-3); // Keep last 3
        setLiveActivity({ rows: newRows, currentTool: event.tool_name });
        if (!import.meta.env.PROD) {
          console.log(
            `[SessionCard ${session().id}] Added tool start row: ${event.tool_name}, total rows: ${newRows.length}`
          );
        }
      }

      // Tool last_line - updates current tool row IN PLACE, no scroll
      else if (event.type === 'tool_execution' && event.last_line) {
        const newRows = current.rows.map((row) => {
          if (row.type === 'tool' && row.name === current.currentTool) {
            return { ...row, lastLine: event.last_line };
          }
          return row;
        });
        setLiveActivity({ ...current, rows: newRows } as LiveActivityState);
        if (!import.meta.env.PROD) {
          console.log(
            `[SessionCard ${session().id}] Updated tool last_line IN PLACE for: ${current.currentTool}`
          );
        }
      }

      // Tool complete - replaces tool row with completion, clears currentTool
      else if (event.type === 'tool_execution' && event.tool_output) {
        const newRows = current.rows.map((row) => {
          if (row.type === 'tool' && row.name === current.currentTool) {
            return {
              type: 'tool' as const,
              name: row.name,
              output: event.tool_output,
              status: event.tool_status,
            };
          }
          return row;
        });
        setLiveActivity({
          rows: newRows,
          currentTool: null,
        } as LiveActivityState);
        if (!import.meta.env.PROD) {
          console.log(
            `[SessionCard ${session().id}] Tool completed: ${event.tool_name}, cleared currentTool`
          );
        }
      }

      // File edit event - adds new row, scrolls up
      else if (event.type === 'file_edit') {
        const newRow: ActivityRow = {
          type: 'file',
          path: event.file_path,
          linesAdded: event.lines_added || 0,
          linesRemoved: event.lines_removed || 0,
        };
        const newRows = [...current.rows, newRow].slice(-3); // Keep last 3
        setLiveActivity({ ...current, rows: newRows });
        if (!import.meta.env.PROD) {
          console.log(
            `[SessionCard ${session().id}] Added file edit row, total rows: ${newRows.length}`
          );
        }
      }

      if (!import.meta.env.PROD) {
        console.log(
          `[SessionCard ${session().id}] Live activity rows:`,
          liveActivity().rows.length
        );
      }
    });

    // Cleanup on unmount
    onCleanup(() => {
      if (!import.meta.env.PROD) {
        console.log(`[SessionCard] Unsubscribing from SSE for session ${session().id}`);
      }
      unsubscribe();
    });
  });

  // Format activity rows for display - MUST be reactive (createMemo)
  const formatActivityRow = (row: ActivityRow): string[] => {
    switch (row.type) {
      case 'thinking':
        return [`Thoughts: ${row.text}`];

      case 'tool':
        if (row.output) {
          // Tool completed - single line
          return [`Tool usage: ${row.name}: ${row.output}`];
        } else if (row.lastLine) {
          // Tool in progress with last line - two lines (main + indented last_line)
          return [`Tool usage: ${row.name}`, `  ${row.lastLine}`];
        } else {
          // Tool just started - single line
          return [`Tool usage: ${row.name}`];
        }

      case 'file':
        return [`File edits: ${row.path} (+${row.linesAdded} -${row.linesRemoved})`];
    }
  };

  // Get formatted activity lines for display - ALWAYS returns exactly 3 lines
  const getLiveActivity = createMemo(() => {
    const activity = liveActivity();

    // Flatten all rows into lines (some rows can produce 2 lines like tool with last_line)
    const allLines = activity.rows.flatMap(formatActivityRow);

    // Get the last 3 lines
    const last3 = allLines.slice(-3);

    // ALWAYS return exactly 3 lines - pad with empty strings if needed
    while (last3.length < 3) {
      last3.unshift(''); // Add empty lines at the beginning
    }

    return last3;
  });

  return (
    <article
      data-testid="task-card"
      id={`session-${session().id}`}
      data-task-id={session().id}
      aria-labelledby={`session-heading-${session().id}`}
      aria-selected={props.isSelected}
      class="rounded-lg border bg-white p-4 shadow-sm transition-all"
      classList={{
        'ring-2 ring-blue-500 border-blue-500 bg-blue-50 selected': props.isSelected,
        'border-gray-200': !props.isSelected,
      }}
      tabindex={props.isSelected ? '0' : '-1'}
    >
      {/* First line: Status icon, clickable title, and actions */}
      <div class="mb-2 flex items-center justify-between">
        <div class="flex min-w-0 flex-1 items-center space-x-2">
          <span
            class={`
              text-sm
              ${statusInfo().color}
            `}
            aria-label={`Status: ${sessionStatus()}`}
          >
            <span aria-hidden="true">{statusInfo().icon}</span>
          </span>
          <h3 id={`session-heading-${session().id}`} class="min-w-0 flex-1 text-sm font-semibold">
            <A
              href={`/tasks/${session().id}`}
              data-testid="task-title-link"
              class={`
                cursor-pointer truncate text-gray-900
                hover:text-blue-600 hover:underline
                focus-visible:ring-2 focus-visible:ring-blue-500
                focus-visible:ring-offset-2
              `}
              title={session().prompt}
              onClick={(e) => {
                // Stop propagation to prevent parent handlers
                e.stopPropagation();
              }}
            >
              {session().prompt.length > 60
                ? `${session().prompt.slice(0, 60)}...`
                : session().prompt}
            </A>
          </h3>
        </div>

        <div class="flex space-x-1">
          <Show when={canStop()}>
            <button
              onClick={(e) => {
                e.stopPropagation();
                props.onStop?.();
              }}
              class={`
                rounded p-1 text-xs text-gray-400
                hover:bg-red-50 hover:text-red-600
                focus-visible:ring-2 focus-visible:ring-blue-500
                focus-visible:ring-offset-2
              `}
              title="Stop"
              aria-label="Stop session"
            >
              ⏹
            </button>
          </Show>
          <Show when={canCancel()}>
            <button
              onClick={(e) => {
                e.stopPropagation();
                props.onCancel?.();
              }}
              class={`
                rounded p-1 text-xs text-gray-400
                hover:bg-red-50 hover:text-red-600
                focus-visible:ring-2 focus-visible:ring-blue-500
                focus-visible:ring-offset-2
              `}
              title="Cancel"
              aria-label="Cancel session"
            >
              ✕
            </button>
          </Show>
        </div>
      </div>

      {/* Second line: Compact metadata - Repository • Branch • Agent • Timestamp (all on one line) */}
      <div class="mb-2 flex items-center space-x-1.5 text-xs text-gray-600">
        <span aria-hidden="true">📁</span>
        <span class="max-w-[120px] truncate">{getRepoName(session().repo.url)}</span>
        <Show when={session().repo.branch}>
          <>
            <span class="text-gray-400">•</span>
            <span
              class={`
                max-w-[100px] truncate rounded bg-gray-100 px-1 py-0.5
                text-gray-700
              `}
            >
              {session().repo.branch}
            </span>
          </>
        </Show>
        <span class="text-gray-400">•</span>
        <span aria-hidden="true">🤖</span>
        <span class="truncate">
          {session().agent.type} v{session().agent.version}
        </span>
        <span class="text-gray-400">•</span>
        <span aria-hidden="true">🕒</span>
        <time datetime={session().createdAt} class="truncate">
          {formatDate(session().createdAt)}
        </time>
      </div>

      {/* Lines 3-5: ALWAYS exactly 3 fixed-height activity rows (ONLY for active sessions) */}
      <Show
        when={['running', 'queued', 'provisioning', 'paused', 'resuming', 'stopping'].includes(
          sessionStatus()
        )}
      >
        <div class="space-y-0.5">
          <For each={getLiveActivity()}>
            {(activity, index) => (
              <div
                class="h-4 truncate overflow-hidden text-xs"
                classList={{
                  'text-blue-600': Boolean(activity && index() === 2),
                  'text-gray-600': Boolean(activity && index() !== 2),
                  'text-transparent': Boolean(!activity),
                }}
                title={activity || ''}
              >
                {activity || '\u00A0'}
              </div>
            )}
          </For>
        </div>
      </Show>
    </article>
  );
};
