/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { Component, JSX, createContext, createSignal, useContext } from 'solid-js';

import type { DraftTask, Session } from '../lib/api';

interface FocusState {
  focusedElement: 'draft-textarea' | 'session-card' | 'none';
  focusedDraftId?: string;
  focusedSessionId?: string;
  focusedDraftAgentCount?: number;
}

interface FocusContextValue {
  focusState: () => FocusState;
  setDraftFocus: (draftId: string, agentCount?: number) => void;
  setSessionFocus: (sessionId: string) => void;
  updateDraftAgentCount: (draftId: string, agentCount: number) => void;
  clearFocus: () => void;
  isDraftFocused: (draftId: string) => boolean;
  isSessionFocused: (sessionId: string) => boolean;
  keyboardSelectedIndex: () => number;
  setKeyboardSelectedIndex: (index: number) => void;
  handleKeyboardNavigation: (event: KeyboardEvent) => void;
  getActiveDescendantId: () => string | undefined;
  registerCollections: (collections: {
    drafts: () => DraftTask[];
    sessions: () => Session[];
    onDraftFocus?: (draftId: string) => void;
    onSessionFocus?: (sessionId?: string) => void;
  }) => void;
}

const FocusContext = createContext<FocusContextValue>();

export const FocusProvider: Component<{ children: JSX.Element }> = props => {
  const [focusState, setFocusState] = createSignal<FocusState>({ focusedElement: 'none' });
  const [keyboardSelectedIndex, setKeyboardSelectedIndex] = createSignal(-1);
  const [draftsAccessor, setDraftsAccessor] = createSignal<() => DraftTask[]>(() => []);
  const [sessionsAccessor, setSessionsAccessor] = createSignal<() => Session[]>(() => []);

  const noopDraftFocus = (_draftId: string) => {};
  const noopSessionFocus = (_sessionId?: string) => {};

  const [draftFocusCallback, setDraftFocusCallback] =
    createSignal<(draftId: string) => void>(noopDraftFocus);
  const [sessionFocusCallback, setSessionFocusCallback] =
    createSignal<(sessionId?: string) => void>(noopSessionFocus);

  const setDraftFocus = (draftId: string, agentCount?: number) => {
    setFocusState({
      focusedElement: 'draft-textarea',
      focusedDraftId: draftId,
      ...(agentCount !== undefined && { focusedDraftAgentCount: agentCount }),
    });
  };

  const setSessionFocus = (sessionId: string) => {
    setFocusState({ focusedElement: 'session-card', focusedSessionId: sessionId });
  };

  const clearFocus = () => {
    setFocusState({ focusedElement: 'none' });
  };

  const isDraftFocused = (draftId: string) => {
    const state = focusState();
    return state.focusedElement === 'draft-textarea' && state.focusedDraftId === draftId;
  };

  const isSessionFocused = (sessionId: string) => {
    const state = focusState();
    return state.focusedElement === 'session-card' && state.focusedSessionId === sessionId;
  };

  const updateDraftAgentCount = (draftId: string, agentCount: number) => {
    setFocusState(prev => {
      if (prev.focusedElement === 'draft-textarea' && prev.focusedDraftId === draftId) {
        return { ...prev, focusedDraftAgentCount: agentCount };
      }
      return prev;
    });
  };

  const registerCollections: FocusContextValue['registerCollections'] = ({
    drafts,
    sessions,
    onDraftFocus,
    onSessionFocus,
  }) => {
    setDraftsAccessor(() => drafts);
    setSessionsAccessor(() => sessions);
    setDraftFocusCallback(() => onDraftFocus ?? noopDraftFocus);
    setSessionFocusCallback(() => onSessionFocus ?? noopSessionFocus);
  };

  const contextValue: FocusContextValue = {
    focusState,
    setDraftFocus,
    setSessionFocus,
    updateDraftAgentCount,
    clearFocus,
    isDraftFocused,
    isSessionFocused,
    keyboardSelectedIndex,
    setKeyboardSelectedIndex,
    handleKeyboardNavigation: event => {
      const draftList = draftsAccessor()();
      const sessionList = sessionsAccessor()();
      const totalItems = draftList.length + sessionList.length;
      if (totalItems === 0) return;

      const currentIndex = keyboardSelectedIndex();

      const scrollCardIntoView = (index: number) => {
        if (typeof window === 'undefined') return;

        let cardElement: Element | null = null;
        if (index < draftList.length) {
          const draftCards = document.querySelectorAll('[data-testid="draft-task-card"]');
          cardElement = draftCards[index] || null;
        } else {
          const sessionIndex = index - draftList.length;
          const sessionCards = document.querySelectorAll('[data-testid="task-card"]');
          cardElement = sessionCards[sessionIndex] || null;
        }

        cardElement?.scrollIntoView({ behavior: 'smooth', block: 'nearest', inline: 'nearest' });
      };

      switch (event.key) {
        case 'ArrowDown': {
          event.preventDefault();
          const nextIndex = currentIndex < totalItems - 1 ? currentIndex + 1 : 0;
          setKeyboardSelectedIndex(nextIndex);

          const keyboardNavEl = document.querySelector('[aria-label="Task list navigation"]');
          keyboardNavEl?.setAttribute('data-keyboard-index', nextIndex.toString());

          if (nextIndex < draftList.length) {
            const nextDraft = draftList[nextIndex];
            if (nextDraft) {
              setDraftFocus(nextDraft.id);
              draftFocusCallback()?.(nextDraft.id);
            }
            sessionFocusCallback()?.(undefined);
          } else {
            const sessionIndex = nextIndex - draftList.length;
            const session = sessionList[sessionIndex];
            if (session) {
              setSessionFocus(session.id);
              sessionFocusCallback()?.(session.id);
            } else {
              sessionFocusCallback()?.(undefined);
            }
          }

          scrollCardIntoView(nextIndex);
          break;
        }

        case 'ArrowUp': {
          event.preventDefault();
          const prevIndex = currentIndex > 0 ? currentIndex - 1 : totalItems - 1;
          setKeyboardSelectedIndex(prevIndex);

          const keyboardNavEl = document.querySelector('[aria-label="Task list navigation"]');
          keyboardNavEl?.setAttribute('data-keyboard-index', prevIndex.toString());

          if (prevIndex < draftList.length) {
            const prevDraft = draftList[prevIndex];
            if (prevDraft) {
              setDraftFocus(prevDraft.id);
              draftFocusCallback()?.(prevDraft.id);
            }
            sessionFocusCallback()?.(undefined);
          } else {
            const sessionIndex = prevIndex - draftList.length;
            const session = sessionList[sessionIndex];
            if (session) {
              setSessionFocus(session.id);
              sessionFocusCallback()?.(session.id);
            } else {
              sessionFocusCallback()?.(undefined);
            }
          }

          scrollCardIntoView(prevIndex);
          break;
        }

        case 'Enter': {
          event.preventDefault();
          if (currentIndex >= draftList.length && currentIndex < totalItems) {
            const sessionIndex = currentIndex - draftList.length;
            const session = sessionList[sessionIndex];
            if (session) {
              setSessionFocus(session.id);
              sessionFocusCallback()?.(session.id);
            }
          }
          break;
        }
      }
    },
    getActiveDescendantId: () => {
      const currentIndex = keyboardSelectedIndex();
      if (currentIndex < 0) return undefined;

      const draftList = draftsAccessor()();
      if (currentIndex < draftList.length) {
        const draft = draftList[currentIndex];
        return draft ? `draft-task-${draft.id}` : undefined;
      }

      const sessionList = sessionsAccessor()();
      const sessionIndex = currentIndex - draftList.length;
      const session = sessionList[sessionIndex];
      return session ? `task-${session.id}` : undefined;
    },
    registerCollections,
  };

  return <FocusContext.Provider value={contextValue}>{props.children}</FocusContext.Provider>;
};

export const useFocus = (): FocusContextValue => {
  const context = useContext(FocusContext);
  if (!context) {
    throw new Error('useFocus must be used within a FocusProvider');
  }
  return context;
};
