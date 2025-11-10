/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';

const hoisted = vi.hoisted(() => ({
  subscribeSpy: vi.fn((_: string, __: (e: any) => void) => () => {}),
}));

vi.mock('~/lib/sse-manager', () => ({
  subscribeToSession: hoisted.subscribeSpy,
}));

import { LiveStore } from '../live-store';

describe('LiveStore - SSE connection deduplication', () => {
  beforeEach(() => {
    hoisted.subscribeSpy.mockClear();
  });

  it('opens only one SSE connection per session', () => {
    const store = new LiveStore();
    store.initialize({
      sessions: [
        {
          id: 's1',
          status: 'running',
          createdAt: '2025-01-01T00:00:00Z',
          prompt: 'p',
          repo: { mode: 'git', url: 'x', branch: 'main' },
          runtime: { type: 'local' },
          agent: { type: 'claude-code', version: 'latest' },
          links: { self: '', events: '', logs: '' },
          recent_events: [],
        } as any,
      ],
    });

    const u1 = store.subscribeSessionLive('s1');
    const u2 = store.subscribeSessionLive('s1');
    expect(typeof u1).toBe('function');
    expect(typeof u2).toBe('function');
    expect(hoisted.subscribeSpy).toHaveBeenCalledTimes(1);

    // Unsubscribe both -> next subscribe should open a new connection
    u1();
    u2();
    const u3 = store.subscribeSessionLive('s1');
    expect(hoisted.subscribeSpy).toHaveBeenCalledTimes(2);
    u3();
  });
});
