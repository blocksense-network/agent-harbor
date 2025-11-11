/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('~/lib/sse-manager', () => {
  let cb: ((e: any) => void) | null = null;
  return {
    subscribeToSession: (_id: string, onEvent: (e: any) => void) => {
      cb = onEvent;
      return () => {
        cb = null;
      };
    },
    __emit: (e: any) => cb && cb(e),
  };
});

import { LiveStore } from '../live-store';

describe('LiveStore - Sessions live activity', () => {
  const baseSession = {
    id: 's1',
    status: 'running',
    createdAt: '2025-01-01T00:00:00Z',
    prompt: 'do work',
    repo: { mode: 'git', url: 'x', branch: 'main' },
    runtime: { type: 'local' },
    agent: { type: 'claude-code', version: 'latest' },
    links: { self: '', events: '', logs: '' },
  } as any;

  beforeEach(() => {
    vi.resetModules();
  });

  it('initializes activity from recent_events and formats to 3 lines', async () => {
    const store = new LiveStore();
    store.initialize({
      sessions: [
        {
          ...baseSession,
          recent_events: [
            { thought: 'Analyzing code' },
            { tool_name: 'search_codebase' },
            { file_path: 'src/app.ts', lines_added: 2, lines_removed: 1 },
          ],
        } as any,
      ],
    });

    const lines = store.getLiveActivityLines('s1');
    expect(lines).toEqual([
      'Thoughts: Analyzing code',
      'Tool usage: search_codebase',
      'File edits: src/app.ts (+2 -1)',
    ]);
  });

  it('applies SSE events with last_line updates and tool_output replacement', async () => {
    const store = new LiveStore();
    store.initialize({ sessions: [{ ...baseSession, recent_events: [] } as any] });

    // Start SSE subscription
    const unsub = store.subscribeSessionLive('s1');
    expect(typeof unsub).toBe('function');

    const sse: any = await import('~/lib/sse-manager');

    // thinking event adds a line
    sse.__emit({ type: 'thinking', thought: 'Start' });
    expect(store.getLiveActivityLines('s1')).toEqual(['', '', 'Thoughts: Start']);

    // tool start adds a line and sets currentTool
    sse.__emit({ type: 'tool_execution', tool_name: 'grep' });
    expect(store.getLiveActivityLines('s1')).toEqual(['', 'Thoughts: Start', 'Tool usage: grep']);

    // last_line updates in place without adding rows
    sse.__emit({ type: 'tool_execution', tool_name: 'grep', last_line: 'Found 10' });
    expect(store.getLiveActivityLines('s1')).toEqual([
      'Thoughts: Start',
      'Tool usage: grep',
      '  Found 10',
    ]);

    // tool_output replaces the tool row and clears last_line
    sse.__emit({
      type: 'tool_execution',
      tool_name: 'grep',
      tool_output: 'Done',
      tool_status: 'success',
    });
    expect(store.getLiveActivityLines('s1')).toEqual([
      '',
      'Thoughts: Start',
      'Tool usage: grep: Done',
    ]);

    // file_edit scrolls while keeping only last 3 lines
    sse.__emit({ type: 'file_edit', file_path: 'src/x.ts', lines_added: 1, lines_removed: 0 });
    expect(store.getLiveActivityLines('s1')).toEqual([
      'Thoughts: Start',
      'Tool usage: grep: Done',
      'File edits: src/x.ts (+1 -0)',
    ]);
  });
});
