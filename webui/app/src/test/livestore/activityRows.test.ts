/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import type { SessionEvent } from '~/lib/api';
import {
  applyEventSequence,
  reduceActivityRows,
  type ActivityRow,
} from '~/livestore/logic/activityRows';

describe('Activity rows reducer (PRD rules)', () => {
  it('handles thought → tool start → last_line → tool_output → new thought with correct scrolling', () => {
    const now = () => new Date().toISOString();
    const events: SessionEvent[] = [
      { type: 'thinking', sessionId: 's1', thought: 'Analyzing the codebase structure', ts: now() },
      {
        type: 'tool_execution',
        sessionId: 's1',
        tool_name: 'search_codebase',
        tool_args: {},
        ts: now(),
      },
      {
        type: 'tool_execution',
        sessionId: 's1',
        tool_name: 'search_codebase',
        tool_args: {},
        last_line: 'Found 42 matches in 12 files',
        ts: now(),
      } as SessionEvent,
      {
        type: 'tool_execution',
        sessionId: 's1',
        tool_name: 'search_codebase',
        tool_args: {},
        tool_output: 'Found 3 matches',
        tool_status: 'success',
        ts: now(),
      } as SessionEvent,
      {
        type: 'thinking',
        sessionId: 's1',
        thought: 'Now examining the authentication flow',
        ts: now(),
      },
    ];

    const rows = applyEventSequence(events);
    expect(rows).toEqual<ActivityRow[]>([
      { kind: 'thinking', text: 'Analyzing the codebase structure' },
      { kind: 'tool-output', toolName: 'search_codebase', text: 'Found 3 matches' },
      { kind: 'thinking', text: 'Now examining the authentication flow' },
    ]);
  });

  it('keeps at most 3 rows and updates last_line in place without scrolling', () => {
    const seq: SessionEvent[] = [];
    const now = () => new Date().toISOString();
    seq.push({ type: 'thinking', sessionId: 's1', thought: 'A', ts: now() });
    seq.push({ type: 'thinking', sessionId: 's1', thought: 'B', ts: now() });
    seq.push({
      type: 'tool_execution',
      sessionId: 's1',
      tool_name: 'grep',
      tool_args: {},
      ts: now(),
    });
    seq.push({
      type: 'tool_execution',
      sessionId: 's1',
      tool_name: 'grep',
      tool_args: {},
      last_line: 'line 1',
      ts: now(),
    } as SessionEvent);
    let rows = applyEventSequence(seq);
    expect(rows.length).toBe(3);
    expect(rows[2]).toEqual({ kind: 'last-line', toolName: 'grep', text: 'line 1' });

    // Another last_line should update in place (no scroll)
    rows = reduceActivityRows(rows, {
      type: 'tool_execution',
      sessionId: 's1',
      tool_name: 'grep',
      tool_args: {},
      last_line: 'line 2',
      ts: now(),
    } as SessionEvent);
    expect(rows.length).toBe(3);
    expect(rows[2]).toEqual({ kind: 'last-line', toolName: 'grep', text: 'line 2' });

    // Completion replaces last_line
    rows = reduceActivityRows(rows, {
      type: 'tool_execution',
      sessionId: 's1',
      tool_name: 'grep',
      tool_args: {},
      tool_output: 'done',
      tool_status: 'success',
      ts: now(),
    } as SessionEvent);
    expect(rows[2]).toEqual({ kind: 'tool-output', toolName: 'grep', text: 'done' });
  });
});

describe('Activity rows scheduling with virtual timers', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('processes a stream of SSE events under virtual time', () => {
    let rows: ActivityRow[] = [];
    const enqueue = (delayMs: number, evt: SessionEvent) => {
      setTimeout(() => {
        rows = reduceActivityRows(rows, evt);
      }, delayMs);
    };

    const now = () => new Date().toISOString();
    enqueue(100, { type: 'thinking', sessionId: 's1', thought: 'First', ts: now() });
    enqueue(200, {
      type: 'tool_execution',
      sessionId: 's1',
      tool_name: 'build',
      tool_args: {},
      ts: now(),
    });
    enqueue(250, {
      type: 'tool_execution',
      sessionId: 's1',
      tool_name: 'build',
      tool_args: {},
      last_line: 'Compiling...',
      ts: now(),
    } as SessionEvent);
    enqueue(400, {
      type: 'tool_execution',
      sessionId: 's1',
      tool_name: 'build',
      tool_args: {},
      tool_output: 'OK',
      tool_status: 'success',
      ts: now(),
    } as SessionEvent);

    // Initially nothing processed
    expect(rows).toEqual([]);

    vi.advanceTimersByTime(100);
    expect(rows).toEqual([{ kind: 'thinking', text: 'First' }]);

    vi.advanceTimersByTime(100);
    expect(rows[rows.length - 1]).toEqual({
      kind: 'tool',
      toolName: 'build',
      text: 'Tool usage: build',
    });

    vi.advanceTimersByTime(50);
    expect(rows[rows.length - 1]).toEqual({
      kind: 'last-line',
      toolName: 'build',
      text: 'Compiling...',
    });

    vi.advanceTimersByTime(150);
    expect(rows[rows.length - 1]).toEqual({ kind: 'tool-output', toolName: 'build', text: 'OK' });
  });
});
