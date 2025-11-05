/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import {
  createContext,
  useContext,
  createSignal,
  Component,
  JSX,
  createEffect,
  createMemo,
  onCleanup,
  onMount,
} from 'solid-js';

import { useToast } from './ToastContext';
import { apiClient, type Session } from '../lib/api.js';
import { type SessionsResponse } from '../lib/server-data';

const STATUS_OPTIONS = [
  { value: '', label: 'All Sessions' },
  { value: 'running', label: 'Running' },
  { value: 'queued', label: 'Queued' },
  { value: 'provisioning', label: 'Provisioning' },
  { value: 'paused', label: 'Paused' },
  { value: 'pausing', label: 'Pausing' },
  { value: 'resuming', label: 'Resuming' },
  { value: 'stopping', label: 'Stopping' },
  { value: 'stopped', label: 'Stopped' },
  { value: 'completed', label: 'Completed' },
  { value: 'failed', label: 'Failed' },
  { value: 'cancelled', label: 'Cancelled' },
] as const;

interface SessionContextValue {
  selectedSessionId: () => string | undefined;
  setSelectedSessionId: (id: string | undefined) => void;
  sessionsData: () => SessionsResponse;
  filteredSessions: () => Session[];
  setInitialSessions: (data: SessionsResponse | undefined) => void;
  statusFilter: () => string;
  setStatusFilter: (value: string) => void;
  statusOptions: typeof STATUS_OPTIONS;
  requestSessionsRefresh: () => void;
  stopSession: (sessionId: string) => Promise<void>;
  cancelSession: (sessionId: string) => void;
}

const SessionContext = createContext<SessionContextValue>();

interface SessionProviderProps {
  children: JSX.Element;
}

export const SessionProvider: Component<SessionProviderProps> = props => {
  const { addToast, addToastWithActions } = useToast();
  const [selectedSessionId, setSelectedSessionId] = createSignal<string | undefined>();
  const [statusFilter, setStatusFilter] = createSignal('');
  const [clientSessions, setClientSessions] = createSignal<SessionsResponse>({
    items: [],
    pagination: { page: 1, perPage: 50, total: 0, totalPages: 0 },
  });
  const [refreshTrigger, setRefreshTrigger] = createSignal(0);

  const sessionsData = createMemo<SessionsResponse>(() => {
    const data = clientSessions();
    return {
      ...data,
      items: [...data.items].sort(
        (a, b) => new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime(),
      ),
    };
  });

  const filteredSessions = createMemo<Session[]>(() => {
    const filter = statusFilter();
    const items = sessionsData().items ?? [];
    if (!filter) {
      return items;
    }
    return items.filter(session => session.status === filter);
  });

  const setInitialSessions = (data: SessionsResponse | undefined) => {
    if (data) {
      setClientSessions(data);
    }
  };

  const requestSessionsRefresh = () => {
    setRefreshTrigger(prev => prev + 1);
  };

  const refetchSessions = async () => {
    if (typeof window === 'undefined') {
      return;
    }

    try {
      const data = await apiClient.listSessions({ perPage: 50 });
      setClientSessions(data);
    } catch (error) {
      console.error('Failed to refresh sessions:', error);
      addToast('error', 'Failed to refresh session list. Please try again.');
    }
  };

  createEffect(() => {
    refreshTrigger();
    if (typeof window !== 'undefined') {
      void refetchSessions();
    }
  });

  onMount(() => {
    const interval = setInterval(() => {
      requestSessionsRefresh();
    }, 30000);

    onCleanup(() => {
      clearInterval(interval);
    });
  });

  const stopSession = async (sessionId: string) => {
    try {
      await apiClient.stopSession(sessionId);
      requestSessionsRefresh();
    } catch (error) {
      console.error('Failed to stop session:', error);
      addToast('error', 'Failed to stop session. Please try again.');
    }
  };

  const cancelSession = (sessionId: string) => {
    addToastWithActions('warning', 'Cancel session?', [
      {
        label: 'Cancel Session',
        onClick: async () => {
          try {
            await apiClient.cancelSession(sessionId);
            requestSessionsRefresh();
            addToast('success', 'Session cancelled successfully');
          } catch (error) {
            console.error('Failed to cancel session:', error);
            addToast('error', 'Failed to cancel session. Please try again.');
          }
        },
        variant: 'danger',
      },
    ]);
  };

  const value: SessionContextValue = {
    selectedSessionId,
    setSelectedSessionId,
    sessionsData,
    filteredSessions,
    setInitialSessions,
    statusFilter,
    setStatusFilter,
    statusOptions: STATUS_OPTIONS,
    requestSessionsRefresh,
    stopSession,
    cancelSession,
  };

  return <SessionContext.Provider value={value}>{props.children}</SessionContext.Provider>;
};

export const useSession = () => {
  const context = useContext(SessionContext);
  if (!context) {
    throw new Error('useSession must be used within a SessionProvider');
  }
  return context;
};
