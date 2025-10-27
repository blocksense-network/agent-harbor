/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { createMemo, createSignal, onCleanup, onMount } from 'solid-js';

import type { Session, SessionEvent } from '../../../lib/api.js';
import { subscribeToSession } from '../../../lib/sse-manager.js';

export type ActivityRow =
  | { type: 'thinking'; text: string }
  | {
      type: 'tool';
      name: string;
      lastLine?: string;
      output?: string;
      status?: string;
    }
  | { type: 'file'; path: string; linesAdded: number; linesRemoved: number };

type LiveActivityState = {
  rows: ActivityRow[];
  currentTool: string | null;
};

const ACTIVE_SESSION_STATUSES: Session['status'][] = [
  'running',
  'queued',
  'provisioning',
  'paused',
  'resuming',
  'stopping',
];

const isActiveStatus = (status: Session['status']) => ACTIVE_SESSION_STATUSES.includes(status);

const convertEventToRow = (event: any): ActivityRow | null => {
  if (event.thought) {
    return { type: 'thinking', text: event.thought };
  }
  if (event.file_path) {
    return {
      type: 'file',
      path: event.file_path,
      linesAdded: event.lines_added || 0,
      linesRemoved: event.lines_removed || 0,
    };
  }
  if (event.tool_name) {
    if (event.tool_output) {
      return {
        type: 'tool',
        name: event.tool_name,
        output: event.tool_output,
        status: event.tool_status,
      };
    }
    return { type: 'tool', name: event.tool_name };
  }
  return null;
};

const formatActivityRow = (row: ActivityRow): string[] => {
  switch (row.type) {
    case 'thinking':
      return [`Thoughts: ${row.text}`];
    case 'tool':
      if (row.output) {
        return [`Tool usage: ${row.name}: ${row.output}`];
      }
      if (row.lastLine) {
        return [`Tool usage: ${row.name}`, `  ${row.lastLine}`];
      }
      return [`Tool usage: ${row.name}`];
    case 'file':
      return [`File edits: ${row.path} (+${row.linesAdded} -${row.linesRemoved})`];
  }
};

export const useSessionLiveActivity = (session: () => Session) => {
  const initialRows: ActivityRow[] = (session().recent_events || [])
    .map(convertEventToRow)
    .filter((row): row is ActivityRow => row !== null);

  const [liveActivityState, setLiveActivityState] = createSignal<LiveActivityState>({
    rows: initialRows,
    currentTool: null,
  });
  const [sessionStatus, setSessionStatus] = createSignal(session().status);

  const liveActivityLines = createMemo(() => {
    const activity = liveActivityState();
    const allLines = activity.rows.flatMap(formatActivityRow);
    const lastThree = allLines.slice(-3);

    while (lastThree.length < 3) {
      lastThree.unshift('');
    }

    return lastThree;
  });

  const canStop = createMemo(() =>
    ['running', 'queued', 'provisioning', 'paused'].includes(sessionStatus()),
  );
  const canCancel = createMemo(() =>
    ['queued', 'provisioning', 'running', 'paused'].includes(sessionStatus()),
  );

  onMount(() => {
    if (typeof window === 'undefined') return;

    if (!isActiveStatus(session().status)) {
      return;
    }

    if (!import.meta.env.PROD) {
      console.log(`[SessionCard] Subscribing to SSE for session ${session().id}`);
    }

    const unsubscribe = subscribeToSession(session().id, (event: SessionEvent) => {
      if (!import.meta.env.PROD) {
        console.log(`[SessionCard ${session().id}] SSE event received:`, event);
      }

      if (event.type === 'status') {
        if (!import.meta.env.PROD) {
          console.log(`[SessionCard ${session().id}] Updating status to:`, event.status);
        }
        setSessionStatus(event.status as Session['status']);
        return;
      }

      setLiveActivityState(current => {
        const nextState: LiveActivityState = { ...current };

        if (event.type === 'thinking') {
          const newRow: ActivityRow = { type: 'thinking', text: event.thought };
          nextState.rows = [...current.rows, newRow].slice(-3);
        } else if (event.type === 'tool_execution' && !event.tool_output && !event.last_line) {
          const newRow: ActivityRow = { type: 'tool', name: event.tool_name };
          nextState.rows = [...current.rows, newRow].slice(-3);
          nextState.currentTool = event.tool_name;
        } else if (event.type === 'tool_execution') {
          const lastLine = event.last_line;
          const toolOutput = event.tool_output;

          if (typeof lastLine === 'string') {
            nextState.rows = current.rows.map(row => {
              if (row.type === 'tool' && row.name === current.currentTool) {
                return { ...row, lastLine };
              }
              return row;
            });
          } else if (typeof toolOutput === 'string') {
            const toolStatus = event.tool_status;
            nextState.rows = current.rows.map(row => {
              if (row.type === 'tool' && row.name === current.currentTool) {
                return {
                  type: 'tool' as const,
                  name: row.name,
                  ...(typeof toolOutput === 'string' && { output: toolOutput }),
                  ...(typeof toolStatus === 'string' && { status: toolStatus }),
                };
              }
              return row;
            });
            nextState.currentTool = null;
          }
        } else if (event.type === 'file_edit') {
          const newRow: ActivityRow = {
            type: 'file',
            path: event.file_path,
            linesAdded: event.lines_added || 0,
            linesRemoved: event.lines_removed || 0,
          };
          nextState.rows = [...current.rows, newRow].slice(-3);
        }

        if (!import.meta.env.PROD) {
          console.log(`[SessionCard ${session().id}] Live activity rows:`, nextState.rows.length);
        }

        return nextState;
      });
    });

    onCleanup(() => {
      if (!import.meta.env.PROD) {
        console.log(`[SessionCard] Unsubscribing from SSE for session ${session().id}`);
      }
      unsubscribe();
    });
  });

  return {
    sessionStatus,
    liveActivityLines,
    canStop,
    canCancel,
  };
};
