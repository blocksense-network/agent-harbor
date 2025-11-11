/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';

const hoisted = vi.hoisted(() => ({
  updateDraftMock: vi.fn(),
  createDraftMock: vi.fn(),
}));

vi.mock('~/lib/api', () => ({
  apiClient: {
    updateDraft: hoisted.updateDraftMock,
    createDraft: hoisted.createDraftMock,
  },
}));

import { LiveStore } from '../live-store';

describe('LiveStore - Draft autosave', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    hoisted.updateDraftMock.mockReset();
  });

  const draft = {
    id: 'd1',
    prompt: 'Hello',
    repo: { mode: 'git', url: 'x', branch: 'main' },
    agents: [{ type: 'claude-code', version: 'latest', instances: 1 }],
    runtime: { type: 'local' },
    delivery: { mode: 'pr' },
    createdAt: '2025-01-01T00:00:00Z',
    updatedAt: '2025-01-01T00:00:00Z',
  } as any;

  it('debounces saves and marks status transitions correctly', async () => {
    const store = new LiveStore({ now: () => 0 });
    store.initialize({ drafts: [draft] });

    // stage local edit
    store.updateDraftLocal('d1', { prompt: 'A' });
    expect(store.getDraftSaveState('d1')?.status).toBe('unsaved');

    // not yet flushed
    vi.advanceTimersByTime(400);
    expect(store.getDraftSaveState('d1')?.status).toBe('unsaved');

    // when debounce elapses, save triggers
    hoisted.updateDraftMock.mockResolvedValueOnce({
      ...draft,
      prompt: 'A',
      updatedAt: '2025-01-01T00:00:01Z',
    });
    vi.advanceTimersByTime(100);
    // microtasks
    await Promise.resolve();
    expect(store.getDraftSaveState('d1')?.status).toBe('saved');
  });

  it('invalidates in-flight save when new edits occur', async () => {
    const store = new LiveStore({ now: () => 0 });
    store.initialize({ drafts: [draft] });

    // First edit -> triggers save after 500ms
    store.updateDraftLocal('d1', { prompt: 'First' });
    hoisted.updateDraftMock.mockResolvedValueOnce({
      ...draft,
      prompt: 'First',
      updatedAt: '2025-01-01T00:00:01Z',
    });
    vi.advanceTimersByTime(500);

    // While that request is in-flight, make another local edit which should invalidate previous
    store.updateDraftLocal('d1', { prompt: 'Second' });
    expect(store.getDraftSaveState('d1')?.status).toBe('unsaved');

    // Resolve first request - should be ignored (state stays unsaved because of new edit)
    await Promise.resolve();
    expect(store.getDraftSaveState('d1')?.status).toBe('unsaved');

    // Now debounce and send second request
    hoisted.updateDraftMock.mockResolvedValueOnce({
      ...draft,
      prompt: 'Second',
      updatedAt: '2025-01-01T00:00:02Z',
    });
    vi.advanceTimersByTime(500);
    await Promise.resolve();
    expect(store.getDraftSaveState('d1')?.status).toBe('saved');
  });

  it('marks error on failed save for latest request only', async () => {
    const store = new LiveStore({ now: () => 0 });
    store.initialize({ drafts: [draft] });

    // First edit -> schedule save
    store.updateDraftLocal('d1', { prompt: 'Err' });
    hoisted.updateDraftMock.mockRejectedValueOnce(new Error('network'));
    vi.advanceTimersByTime(500);
    await Promise.resolve();
    expect(store.getDraftSaveState('d1')?.status).toBe('error');
    expect(store.getDraftSaveState('d1')?.lastError).toBe('network');
  });
});
