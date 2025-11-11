/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import type { Session, SessionEvent } from '~/lib/api';

export type DraftSaveStatus = 'unsaved' | 'saving' | 'saved' | 'error';

export interface DraftSaveState {
  status: DraftSaveStatus;
  lastError?: string | null;
  // Internal tracking for request invalidation
  nextRequestId: number;
  inFlightRequestId?: number;
}

export interface FocusState {
  focusedElement: 'draft-textarea' | 'session-card' | 'none';
  focusedDraftId?: string;
  focusedSessionId?: string;
  focusedDraftAgentCount?: number;
}

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

export interface LiveActivityState {
  rows: ActivityRow[];
  currentTool: string | null;
}

export interface SessionRecord {
  session: Session;
  activity: LiveActivityState;
}

export const ACTIVE_SESSION_STATUSES: Session['status'][] = [
  'running',
  'queued',
  'provisioning',
  'paused',
  'resuming',
  'stopping',
];

export const isActiveStatus = (status: Session['status']) =>
  ACTIVE_SESSION_STATUSES.includes(status);

export const convertEventToActivityRow = (event: any): ActivityRow | null => {
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

export const formatActivityRow = (row: ActivityRow): string[] => {
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

export const initialActivityFromRecentEvents = (
  recent: Array<SessionEvent | Record<string, unknown>> | undefined,
): LiveActivityState => {
  const rows: ActivityRow[] = (recent || [])
    .map(convertEventToActivityRow)
    .filter((row): row is ActivityRow => row !== null)
    .slice(-3);
  return { rows, currentTool: null };
};
