/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import {
  apiClient,
  type DraftTask,
  type DraftUpdate,
  type Session,
  type SessionEvent,
} from '~/lib/api';
import { subscribeToSession as subscribeToSessionSSE } from '~/lib/sse-manager';
import {
  type DraftSaveState,
  type FocusState,
  type LiveActivityState,
  type SessionRecord,
  formatActivityRow,
  initialActivityFromRecentEvents,
  isActiveStatus,
} from './types';

type TimeoutHandle = ReturnType<typeof setTimeout> | undefined;

export interface LiveStoreOptions {
  saveDebounceMs?: number; // default 500ms
  now?: () => number; // for deterministic tests
  setTimeoutFn?: (cb: () => void, ms: number) => any;
  clearTimeoutFn?: (h: any) => void;
  logger?: Pick<typeof console, 'log' | 'error' | 'warn'>;
}

export type FooterShortcuts = {
  left: string;
  right: string;
};

export class LiveStore {
  private readonly saveDebounceMs: number;
  private readonly nowFn: () => number;
  private readonly setTimeoutFn: (cb: () => void, ms: number) => any;
  private readonly clearTimeoutFn: (h: any) => void;
  private readonly logger: Pick<typeof console, 'log' | 'error' | 'warn'>;

  // Sessions state
  private sessionMap = new Map<string, SessionRecord>();
  private sseUnsubscribers = new Map<string, () => void>();

  // Drafts state
  private drafts = new Map<string, DraftTask>();
  private draftSaveState = new Map<string, DraftSaveState>();
  private draftDebounceTimer = new Map<string, TimeoutHandle>();

  // Focus/UI
  private focus: FocusState = { focusedElement: 'none' };

  constructor(opts?: LiveStoreOptions) {
    this.saveDebounceMs = opts?.saveDebounceMs ?? 500;
    this.nowFn = opts?.now ?? Date.now;
    this.setTimeoutFn = opts?.setTimeoutFn ?? ((cb, ms) => setTimeout(cb, ms));
    this.clearTimeoutFn = opts?.clearTimeoutFn ?? (h => clearTimeout(h));
    this.logger = opts?.logger ?? console;
  }

  // Initialization
  initialize(params: { sessions?: Session[]; drafts?: DraftTask[] }) {
    if (params.sessions) {
      for (const s of params.sessions) {
        const rec: SessionRecord = {
          session: s,
          activity: initialActivityFromRecentEvents(s.recent_events as any),
        };
        this.sessionMap.set(s.id, rec);
      }
    }
    if (params.drafts) {
      for (const d of params.drafts) {
        this.drafts.set(d.id, d);
        this.draftSaveState.set(d.id, { status: 'saved', lastError: null, nextRequestId: 1 });
      }
    }
  }

  // Sessions API
  listSessions(): Session[] {
    return Array.from(this.sessionMap.values())
      .map(r => r.session)
      .sort((a, b) => (a.createdAt < b.createdAt ? 1 : -1));
  }

  getSessionRecord(id: string): SessionRecord | undefined {
    return this.sessionMap.get(id);
  }

  getLiveActivityLines(id: string): string[] {
    const rec = this.sessionMap.get(id);
    if (!rec) return ['', '', ''];
    const lines = rec.activity.rows.flatMap(formatActivityRow).slice(-3);
    while (lines.length < 3) lines.unshift('');
    return lines;
  }

  canStop(id: string): boolean {
    const s = this.sessionMap.get(id)?.session;
    if (!s) return false;
    return ['running', 'queued', 'provisioning', 'paused'].includes(s.status);
  }

  canCancel(id: string): boolean {
    const s = this.sessionMap.get(id)?.session;
    if (!s) return false;
    return ['queued', 'provisioning', 'running', 'paused'].includes(s.status);
  }

  subscribeSessionLive(sessionId: string): () => void {
    const rec = this.sessionMap.get(sessionId);
    if (!rec) return () => {};
    if (!isActiveStatus(rec.session.status)) return () => {};

    if (!this.sseUnsubscribers.has(sessionId)) {
      const unsubscribe = subscribeToSessionSSE(sessionId, (event: SessionEvent) => {
        this.applySessionEvent(sessionId, event);
      });
      this.sseUnsubscribers.set(sessionId, unsubscribe);
    }
    return () => {
      const unsub = this.sseUnsubscribers.get(sessionId);
      if (unsub) {
        unsub();
        this.sseUnsubscribers.delete(sessionId);
      }
    };
  }

  private applySessionEvent(sessionId: string, event: SessionEvent) {
    const rec = this.sessionMap.get(sessionId);
    if (!rec) return;

    if (event.type === 'status') {
      rec.session = { ...rec.session, status: (event as any).status };
      return;
    }

    const current = rec.activity;
    const next: LiveActivityState = { ...current, rows: current.rows.slice() };

    if (event.type === 'thinking') {
      next.rows = [
        ...current.rows,
        { type: 'thinking' as const, text: (event as any).thought as string },
      ].slice(-3);
    } else if (
      event.type === 'tool_execution' &&
      !(event as any).tool_output &&
      !(event as any).last_line
    ) {
      next.rows = [
        ...current.rows,
        { type: 'tool' as const, name: (event as any).tool_name as string },
      ].slice(-3);
      next.currentTool = (event as any).tool_name;
    } else if (event.type === 'tool_execution') {
      const lastLine = (event as any).last_line as string | undefined;
      const toolOutput = (event as any).tool_output as string | undefined;
      const toolStatus = (event as any).tool_status as string | undefined;
      if (typeof lastLine === 'string') {
        next.rows = current.rows.map(row =>
          row.type === 'tool' && row.name === current.currentTool ? { ...row, lastLine } : row,
        );
      } else if (typeof toolOutput === 'string') {
        next.rows = current.rows.map(row =>
          row.type === 'tool' && row.name === current.currentTool
            ? {
                type: 'tool',
                name: row.name,
                output: toolOutput,
                ...(toolStatus ? { status: toolStatus } : {}),
              }
            : row,
        );
        next.currentTool = null;
      }
    } else if (event.type === 'file_edit') {
      next.rows = [
        ...current.rows,
        {
          type: 'file' as const,
          path: (event as any).file_path as string,
          linesAdded: ((event as any).lines_added as number) || 0,
          linesRemoved: ((event as any).lines_removed as number) || 0,
        },
      ].slice(-3);
    }

    rec.activity = next;
    this.sessionMap.set(sessionId, rec);
  }

  // Drafts API
  listDrafts(): DraftTask[] {
    return Array.from(this.drafts.values());
  }

  getDraft(id: string): DraftTask | undefined {
    return this.drafts.get(id);
  }

  getDraftSaveState(id: string): DraftSaveState | undefined {
    return this.draftSaveState.get(id);
  }

  createDraft(
    draft: Omit<DraftTask, 'id' | 'createdAt' | 'updatedAt'> & { id?: string },
  ): Promise<DraftTask> {
    return apiClient
      .createDraft({
        prompt: draft.prompt,
        repo: draft.repo,
        agents: draft.agents,
        runtime: draft.runtime,
        delivery: draft.delivery,
      } as any)
      .then(created => {
        this.drafts.set(created.id, created);
        this.draftSaveState.set(created.id, {
          status: 'saved',
          lastError: null,
          nextRequestId: 1,
        });
        return created;
      });
  }

  updateDraftLocal(id: string, updates: Partial<DraftUpdate>) {
    const d = this.drafts.get(id);
    if (!d) return;
    const updated: DraftTask = {
      ...d,
      ...(updates as any),
      updatedAt: new Date(this.nowFn()).toISOString(),
    };
    this.drafts.set(id, updated);
    const save = this.draftSaveState.get(id) ?? { status: 'unsaved', nextRequestId: 1 };
    save.status = 'unsaved';
    save.lastError = null;
    // Invalidate any in-flight request so its eventual response is ignored
    if (save.inFlightRequestId !== undefined) {
      delete save.inFlightRequestId;
    }
    this.draftSaveState.set(id, save);

    const existingTimer = this.draftDebounceTimer.get(id);
    if (existingTimer) this.clearTimeoutFn(existingTimer as any);
    const timer = this.setTimeoutFn(() => this.flushDraftSave(id), this.saveDebounceMs);
    this.draftDebounceTimer.set(id, timer);
  }

  private async flushDraftSave(id: string) {
    this.draftDebounceTimer.delete(id);
    const draft = this.drafts.get(id);
    if (!draft) return;

    const state = this.draftSaveState.get(id) ?? { status: 'unsaved', nextRequestId: 1 };
    const requestId = state.nextRequestId++;
    state.status = 'saving';
    state.inFlightRequestId = requestId;
    this.draftSaveState.set(id, state);

    try {
      const res = await apiClient.updateDraft(id, {
        prompt: draft.prompt,
        repo: draft.repo,
        agents: draft.agents,
        runtime: draft.runtime,
        delivery: draft.delivery,
      } as any);

      const current = this.draftSaveState.get(id);
      if (!current) return;

      // If a newer save was started, ignore this response
      if (current.inFlightRequestId !== requestId) {
        return;
      }

      // Merge server-updated fields
      this.drafts.set(id, res);

      // If no new local edits happened, mark as saved
      current.status = 'saved';
      delete current.inFlightRequestId;
      current.lastError = null;
      this.draftSaveState.set(id, current);
    } catch (err: any) {
      const current = this.draftSaveState.get(id) ?? { status: 'unsaved', nextRequestId: 1 };
      // Only mark error if this response pertains to the latest in-flight request
      if (current.inFlightRequestId === requestId) {
        current.status = 'error';
        current.lastError = err?.message || 'Failed to save draft';
        delete current.inFlightRequestId;
        this.draftSaveState.set(id, current);
      }
    }
  }

  // UI/Focus and footer shortcuts
  setDraftFocus(draftId: string, agentCount?: number) {
    this.focus = {
      focusedElement: 'draft-textarea',
      focusedDraftId: draftId,
      ...(agentCount !== undefined ? { focusedDraftAgentCount: agentCount } : {}),
    };
  }

  setSessionFocus(sessionId: string) {
    this.focus = { focusedElement: 'session-card', focusedSessionId: sessionId };
  }

  clearFocus() {
    this.focus = { focusedElement: 'none' };
  }

  getFooterShortcuts(): FooterShortcuts {
    if (this.focus.focusedElement === 'draft-textarea') {
      const count = this.focus.focusedDraftAgentCount ?? 1;
      const plural = count === 1 ? '' : '(s)';
      return {
        left: `Enter Launch Agent${plural} • Shift+Enter New Line • Tab Next Field`,
        right: 'New Task',
      };
    }
    if (this.focus.focusedElement === 'session-card') {
      return { left: '↑↓ Navigate • Enter Review Session Details', right: 'New Task' };
    }
    return { left: '↑↓ Navigate • Enter Select Task', right: 'New Task' };
  }
}

let defaultStore: LiveStore | undefined;
export function getLiveStore(options?: LiveStoreOptions): LiveStore {
  if (!defaultStore) {
    defaultStore = new LiveStore(options);
  }
  return defaultStore;
}
