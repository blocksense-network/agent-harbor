/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import type { SessionEvent } from '~/lib/api';

export type ActivityRow =
  | { kind: 'thinking'; text: string }
  | { kind: 'tool'; toolName: string; text: string }
  | { kind: 'tool-output'; toolName: string; text: string }
  | { kind: 'last-line'; toolName: string; text: string }
  | { kind: 'file-edit'; filePath: string; added?: number; removed?: number };

/**
 * Reduce activity rows to the last three visible rows based on incoming SSE events.
 * Implements PRD rules for activity area behavior.
 */
export function reduceActivityRows(current: ActivityRow[], event: SessionEvent): ActivityRow[] {
  const next = current.slice(0, 3); // copy, cap handled at end
  switch (event.type) {
    case 'thinking': {
      // Append new thinking row (scroll up)
      const row: ActivityRow = { kind: 'thinking', text: event.thought };
      return cap3([...next, row]);
    }
    case 'tool_execution': {
      const toolName = event.tool_name ?? 'tool';
      if (event.last_line) {
        // Update the existing last tool row in place or create if absent, without scrolling
        const idx = findLastToolRowIndex(next, toolName);
        if (idx >= 0) {
          const updated = next.slice();
          updated[idx] = { kind: 'last-line', toolName, text: event.last_line };
          return updated;
        }
        // No existing tool row -> create a tool row without scrolling beyond 3
        if (next.length === 0) return [{ kind: 'last-line', toolName, text: event.last_line }];
        const updated = next.slice();
        updated[updated.length - 1] = { kind: 'last-line', toolName, text: event.last_line };
        return updated;
      }
      if (event.tool_output || event.tool_status) {
        // Replace any existing last-line row for this tool with tool-output row
        const idx = findLastToolRowIndex(next, toolName);
        const text = event.tool_output ?? '';
        if (idx >= 0) {
          const updated = next.slice();
          updated[idx] = { kind: 'tool-output', toolName, text };
          return updated;
        }
        // Otherwise treat as a new row
        return cap3([...next, { kind: 'tool-output', toolName, text }]);
      }
      // Tool usage start: append a new tool row
      return cap3([...next, { kind: 'tool', toolName, text: `Tool usage: ${toolName}` }]);
    }
    case 'file_edit': {
      return cap3([
        ...next,
        {
          kind: 'file-edit',
          filePath: event.file_path,
          ...(event.lines_added !== undefined ? { added: event.lines_added } : {}),
          ...(event.lines_removed !== undefined ? { removed: event.lines_removed } : {}),
        },
      ]);
    }
    case 'log': {
      // Logs are not in the 3-line activity by PRD; ignore for rows
      return next;
    }
    case 'progress': {
      // Progress not shown in the 3 rows by PRD; ignore
      return next;
    }
    case 'status': {
      // Status reflected in header, not rows
      return next;
    }
    default:
      return next;
  }
}

export function applyEventSequence(events: SessionEvent[]): ActivityRow[] {
  return events.reduce<ActivityRow[]>((rows, e) => reduceActivityRows(rows, e), []);
}

function cap3(rows: ActivityRow[]): ActivityRow[] {
  if (rows.length <= 3) return rows;
  return rows.slice(rows.length - 3);
}

function findLastToolRowIndex(rows: ActivityRow[], toolName: string): number {
  for (let i = rows.length - 1; i >= 0; i--) {
    const r = rows[i]!;
    if (r.kind === 'tool' || r.kind === 'last-line' || r.kind === 'tool-output') {
      if (r.toolName === toolName) return i;
    }
  }
  return -1;
}
